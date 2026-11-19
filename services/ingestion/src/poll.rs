//! Polling one source: rights check → robots.txt → conditional GET → parse → store.

use serde::Serialize;
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    config::IngestConfig,
    net::{FetchError, Fetched, Fetcher},
    parse::parse_feed,
    robots,
    store::{Counts, SourcePolicy, store_items},
};

#[derive(sqlx::FromRow)]
struct SourceRow {
    id: Uuid,
    feed_url: String,
    allowed_host: String,
    rights_status: String,
    enabled: bool,
    store_excerpts: bool,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PollReport {
    /// Disabled, or its rights no longer allow ingestion. Not an error.
    Skipped {
        reason: String,
    },
    NotModified,
    Fetched {
        items_in_feed: u32,
        counts: Counts,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PollError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("{0}")]
    Parse(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("source not found")]
    NotFound,
}

impl PollError {
    pub fn code(&self) -> String {
        match self {
            Self::Fetch(e) => e.code(),
            Self::Parse(_) => "parse_error".into(),
            Self::Database(_) => "database".into(),
            Self::NotFound => "source_not_found".into(),
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            Self::Fetch(e) => e.retryable(),
            Self::Parse(_) => true, // feeds are sometimes briefly broken
            Self::Database(_) => true,
            Self::NotFound => false,
        }
    }
}

pub async fn poll_source(
    pool: &PgPool,
    fetcher: &Fetcher,
    cfg: &IngestConfig,
    source_id: Uuid,
) -> Result<PollReport, PollError> {
    let source: SourceRow = sqlx::query_as(
        "select id, feed_url, allowed_host, rights_status, enabled, store_excerpts, etag, last_modified
         from source where id = $1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PollError::NotFound)?;

    if !source.enabled {
        return Ok(PollReport::Skipped {
            reason: "source is disabled".into(),
        });
    }
    if source.rights_status != "cleared" && source.rights_status != "link_only" {
        return Ok(PollReport::Skipped {
            reason: format!("rights status is `{}`", source.rights_status),
        });
    }

    sqlx::query("update source set last_attempt_at = now() where id = $1")
        .bind(source.id)
        .execute(pool)
        .await?;

    let result = fetch_and_store(pool, fetcher, cfg, &source).await;
    match &result {
        Ok(_) => {
            sqlx::query(
                "update source set last_success_at = now(), last_error = null, consecutive_failures = 0,
                                   updated_at = now()
                 where id = $1",
            )
            .bind(source.id)
            .execute(pool)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                "update source set last_error = $2, consecutive_failures = consecutive_failures + 1,
                                   updated_at = now()
                 where id = $1",
            )
            .bind(source.id)
            .bind(format!("{}: {e}", e.code()))
            .execute(pool)
            .await?;
        }
    }
    result
}

async fn fetch_and_store(
    pool: &PgPool,
    fetcher: &Fetcher,
    cfg: &IngestConfig,
    source: &SourceRow,
) -> Result<PollReport, PollError> {
    let url = Url::parse(&source.feed_url).map_err(|e| PollError::Parse(format!("bad feed URL: {e}")))?;

    // robots.txt: a 4xx means "no rules"; a 5xx or network error means "try later".
    let mut robots_url = url.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    match fetcher.fetch(&robots_url, &source.allowed_host, None, None).await {
        Ok(Fetched::Body { bytes, .. }) => {
            let path = match url.query() {
                Some(q) => format!("{}?{q}", url.path()),
                None => url.path().to_string(),
            };
            if !robots::is_allowed(&String::from_utf8_lossy(&bytes), &cfg.robots_token, &path) {
                return Err(FetchError::Robots.into());
            }
        }
        Ok(Fetched::NotModified) => {}
        Err(FetchError::Status(s)) if (400..500).contains(&s) => {}
        Err(e) => return Err(e.into()),
    }

    let fetched = fetcher
        .fetch(
            &url,
            &source.allowed_host,
            source.etag.as_deref(),
            source.last_modified.as_deref(),
        )
        .await?;
    let (bytes, etag, last_modified, final_url) = match fetched {
        Fetched::NotModified => return Ok(PollReport::NotModified),
        Fetched::Body {
            bytes,
            etag,
            last_modified,
            final_url,
        } => (bytes, etag, last_modified, final_url),
    };

    let items = parse_feed(&bytes, &final_url, cfg.max_items_per_poll).map_err(PollError::Parse)?;
    let policy = SourcePolicy {
        source_id: source.id,
        store_excerpts: source.store_excerpts,
        max_excerpt_chars: cfg.max_excerpt_chars,
    };
    let mut tx = pool.begin().await?;
    let counts = store_items(&mut tx, &policy, &items).await?;
    // Only remember validators once the items are safely stored.
    sqlx::query("update source set etag = $2, last_modified = $3 where id = $1")
        .bind(source.id)
        .bind(etag)
        .bind(last_modified)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(PollReport::Fetched {
        items_in_feed: items.len() as u32,
        counts,
    })
}
