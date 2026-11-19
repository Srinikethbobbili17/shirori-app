//! Storing parsed items: insert new ones, detect updates, and deduplicate across sources
//! and against works already in the catalogue.

use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{canonical::truncate, parse::ParsedItem};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub new: u32,
    pub duplicates: u32,
    pub updated: u32,
    pub unchanged: u32,
}

pub struct SourcePolicy {
    pub source_id: Uuid,
    pub store_excerpts: bool,
    pub max_excerpt_chars: usize,
}

pub async fn store_items(conn: &mut PgConnection, policy: &SourcePolicy, items: &[ParsedItem]) -> sqlx::Result<Counts> {
    let mut counts = Counts::default();
    for item in items {
        // Rights: keep the feed's summary text only when the source allows it.
        let excerpt = if policy.store_excerpts {
            item.summary.as_deref().map(|s| truncate(s, policy.max_excerpt_chars))
        } else {
            None
        };

        let existing: Option<(Uuid, String, String)> = sqlx::query_as(
            "select id, content_hash, status from ingested_item where source_id = $1 and external_id = $2",
        )
        .bind(policy.source_id)
        .bind(&item.external_id)
        .fetch_optional(&mut *conn)
        .await?;

        if let Some((id, hash, status)) = existing {
            if hash == item.content_hash {
                sqlx::query("update ingested_item set last_seen_at = now() where id = $1")
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                counts.unchanged += 1;
            } else {
                // The publisher changed the item (news lifecycle, FR-16). If an editor already
                // turned it into a source work, flag it for review.
                sqlx::query(
                    "update ingested_item
                     set title = $2, canonical_url = $3, doi = coalesce($4, doi), authors = $5, excerpt = $6,
                         published_at = coalesce($7, published_at), content_hash = $8,
                         revision_count = revision_count + 1, last_seen_at = now(), updated_at = now(),
                         changed_since_promotion = changed_since_promotion or status = 'promoted'
                     where id = $1",
                )
                .bind(id)
                .bind(&item.title)
                .bind(&item.canonical_url)
                .bind(&item.doi)
                .bind(&item.authors)
                .bind(&excerpt)
                .bind(item.published_at)
                .bind(&item.content_hash)
                .execute(&mut *conn)
                .await?;
                if status == "promoted" {
                    tracing::warn!(item = %id, "EDITORIAL ALERT: a promoted item changed at the source");
                }
                counts.updated += 1;
            }
            continue;
        }

        // Same article already ingested from any source?
        let duplicate_of: Option<Uuid> = sqlx::query_scalar(
            "select id from ingested_item
             where duplicate_of is null
               and (($1::text is not null and canonical_url = $1) or ($2::text is not null and doi = $2))
             order by first_seen_at limit 1",
        )
        .bind(&item.canonical_url)
        .bind(&item.doi)
        .fetch_optional(&mut *conn)
        .await?;
        // Already a work in the catalogue (e.g. added by hand)?
        let existing_work: Option<Uuid> = sqlx::query_scalar(
            "select id from source_work
             where ($1::text is not null and canonical_url = $1)
                or ($2::text is not null and lower(doi_or_isbn) = $2)
             order by created_at limit 1",
        )
        .bind(&item.canonical_url)
        .bind(&item.doi)
        .fetch_optional(&mut *conn)
        .await?;
        let is_duplicate = duplicate_of.is_some() || existing_work.is_some();

        sqlx::query(
            "insert into ingested_item (source_id, external_id, canonical_url, doi, title, authors, excerpt,
                                        published_at, content_hash, status, duplicate_of, existing_work_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(policy.source_id)
        .bind(&item.external_id)
        .bind(&item.canonical_url)
        .bind(&item.doi)
        .bind(&item.title)
        .bind(&item.authors)
        .bind(&excerpt)
        .bind(item.published_at)
        .bind(&item.content_hash)
        .bind(if is_duplicate { "duplicate" } else { "new" })
        .bind(duplicate_of)
        .bind(existing_work)
        .execute(&mut *conn)
        .await?;
        if is_duplicate {
            counts.duplicates += 1;
        } else {
            counts.new += 1;
        }
    }
    Ok(counts)
}
