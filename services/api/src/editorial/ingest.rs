//! `/v1/admin/...` endpoints for the content pipeline (build step 3):
//! the source register, the ingested-item review queue, and the job dead-letter queue.
//!
//! Nothing here reaches readers. Promoting an item only creates a *source work*; an editor
//! still has to write a card about it, and that card still has to pass the publication gate.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shirori_persistence::jobs;
use uuid::Uuid;

use super::service::audit;
use crate::{
    auth::Editor,
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources).post(create_source))
        .route("/sources/{id}", get(get_source).patch(update_source))
        .route("/sources/{id}/poll", post(poll_now))
        .route("/ingest/items", get(list_items))
        .route("/ingest/items/{id}", get(get_item))
        .route("/ingest/items/{id}/promote", post(promote))
        .route("/ingest/items/{id}/reject", post(reject))
        .route("/ingest/items/{id}/acknowledge-change", post(acknowledge_change))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}/retry", post(retry_job))
}

const RIGHTS: [&str; 4] = ["cleared", "link_only", "restricted", "unknown"];
const WORK_TYPES: [&str; 4] = ["article", "paper", "book", "reference"];

// ───────────────────────── Sources ─────────────────────────

#[derive(Deserialize)]
struct SourceIn {
    name: String,
    feed_url: String,
    homepage_url: Option<String>,
    /// Defaults to the feed URL's host. The fetcher may only contact this host or its subdomains.
    allowed_host: Option<String>,
    publisher: Option<String>,
    default_work_type: Option<String>,
    licence: String,
    rights_status: String,
    #[serde(default)]
    store_excerpts: bool,
    #[serde(default)]
    allow_ai_processing: bool,
    poll_interval_minutes: Option<i32>,
    #[serde(default)]
    enabled: bool,
}

#[derive(Serialize, sqlx::FromRow)]
struct SourceOut {
    id: Uuid,
    name: String,
    feed_url: String,
    homepage_url: Option<String>,
    allowed_host: String,
    publisher: Option<String>,
    default_work_type: String,
    licence: String,
    rights_status: String,
    store_excerpts: bool,
    allow_ai_processing: bool,
    poll_interval_minutes: i32,
    enabled: bool,
    last_attempt_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    consecutive_failures: i32,
    created_at: DateTime<Utc>,
    /// Items waiting for an editor (status `new`) plus promoted items that changed at the source.
    pending_review: i64,
}

const SOURCE_COLUMNS: &str = "s.id, s.name, s.feed_url, s.homepage_url, s.allowed_host, s.publisher,
    s.default_work_type, s.licence, s.rights_status, s.store_excerpts, s.allow_ai_processing,
    s.poll_interval_minutes, s.enabled, s.last_attempt_at, s.last_success_at, s.last_error,
    s.consecutive_failures, s.created_at,
    (select count(*) from ingested_item i
     where i.source_id = s.id and (i.status = 'new' or i.changed_since_promotion)) as pending_review";

/// Check a feed URL and work out the host the fetcher will be limited to.
fn validate_feed_url(feed_url: &str, allowed_host: Option<&str>) -> AppResult<String> {
    let url = url::Url::parse(feed_url).map_err(|e| AppError::BadRequest(format!("feed_url: {e}")))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(AppError::BadRequest("feed_url must be http or https".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::BadRequest("feed_url must not contain credentials".into()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("feed_url has no host".into()))?
        .to_ascii_lowercase();
    let allowed = allowed_host.map(|h| h.trim().trim_end_matches('.').to_ascii_lowercase());
    match allowed {
        None => Ok(host),
        Some(a) if !a.is_empty() && (host == a || host.ends_with(&format!(".{a}"))) => Ok(a),
        Some(a) => Err(AppError::BadRequest(format!(
            "feed host `{host}` is not `{a}` or one of its subdomains"
        ))),
    }
}

fn check_choice(field: &str, value: &str, allowed: &[&str]) -> AppResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "{field} must be one of {}",
            allowed.join(", ")
        )))
    }
}

/// "Do not ingest unknown rights" (§2.2): only cleared or link-only sources may be switched on.
fn check_can_enable(enabled: bool, rights_status: &str) -> AppResult<()> {
    if enabled && rights_status != "cleared" && rights_status != "link_only" {
        return Err(AppError::Conflict {
            code: "rights_not_cleared",
            message: format!("a source with rights status `{rights_status}` cannot be enabled"),
        });
    }
    Ok(())
}

async fn create_source(
    State(s): State<AppState>,
    Editor(u): Editor,
    ApiJson(src): ApiJson<SourceIn>,
) -> AppResult<(StatusCode, Json<SourceOut>)> {
    if src.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let allowed_host = validate_feed_url(&src.feed_url, src.allowed_host.as_deref())?;
    check_choice("rights_status", &src.rights_status, &RIGHTS)?;
    let work_type = src.default_work_type.as_deref().unwrap_or("article");
    check_choice("default_work_type", work_type, &WORK_TYPES)?;
    check_can_enable(src.enabled, &src.rights_status)?;
    // Keeping a publisher's text needs a licence that allows it, not just permission to link.
    if src.store_excerpts && src.rights_status != "cleared" {
        return Err(AppError::BadRequest(
            "store_excerpts needs rights_status `cleared`".into(),
        ));
    }

    let mut tx = s.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "insert into source (name, feed_url, homepage_url, allowed_host, publisher, default_work_type, licence,
                             rights_status, store_excerpts, allow_ai_processing, poll_interval_minutes, enabled)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, coalesce($11, 60), $12)
         returning id",
    )
    .bind(src.name.trim())
    .bind(&src.feed_url)
    .bind(&src.homepage_url)
    .bind(&allowed_host)
    .bind(&src.publisher)
    .bind(work_type)
    .bind(&src.licence)
    .bind(&src.rights_status)
    .bind(src.store_excerpts)
    .bind(src.allow_ai_processing)
    .bind(src.poll_interval_minutes)
    .bind(src.enabled)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        u.id,
        "source.created",
        "source",
        id,
        json!({ "feed_url": src.feed_url, "rights_status": src.rights_status, "enabled": src.enabled }),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(fetch_source(&s, id).await?)))
}

async fn fetch_source(s: &AppState, id: Uuid) -> AppResult<SourceOut> {
    sqlx::query_as(&format!("select {SOURCE_COLUMNS} from source s where s.id = $1"))
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(|| AppError::not_found("source"))
}

async fn list_sources(State(s): State<AppState>, _: Editor) -> AppResult<Json<Vec<SourceOut>>> {
    let rows = sqlx::query_as(&format!("select {SOURCE_COLUMNS} from source s order by s.name"))
        .fetch_all(&s.pool)
        .await?;
    Ok(Json(rows))
}

async fn get_source(State(s): State<AppState>, _: Editor, Path(id): Path<Uuid>) -> AppResult<Json<SourceOut>> {
    Ok(Json(fetch_source(&s, id).await?))
}

/// Partial update. Changing rights is how an editor switches a source off after a licence change.
#[derive(Deserialize)]
struct SourcePatch {
    name: Option<String>,
    homepage_url: Option<String>,
    publisher: Option<String>,
    default_work_type: Option<String>,
    licence: Option<String>,
    rights_status: Option<String>,
    store_excerpts: Option<bool>,
    allow_ai_processing: Option<bool>,
    poll_interval_minutes: Option<i32>,
    enabled: Option<bool>,
}

async fn update_source(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    ApiJson(p): ApiJson<SourcePatch>,
) -> AppResult<Json<SourceOut>> {
    let mut tx = s.pool.begin().await?;
    let (rights, enabled, store_excerpts): (String, bool, bool) =
        sqlx::query_as("select rights_status, enabled, store_excerpts from source where id = $1 for update")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found("source"))?;

    if let Some(r) = &p.rights_status {
        check_choice("rights_status", r, &RIGHTS)?;
    }
    if let Some(w) = &p.default_work_type {
        check_choice("default_work_type", w, &WORK_TYPES)?;
    }
    let new_rights = p.rights_status.clone().unwrap_or(rights);
    // Downgrading rights switches the source off and stops keeping excerpts, unless the
    // editor explicitly asks for something the new rights still allow.
    let rights_ok = new_rights == "cleared" || new_rights == "link_only";
    let new_enabled = p.enabled.unwrap_or(enabled && rights_ok);
    check_can_enable(new_enabled, &new_rights)?;
    let new_store_excerpts = p.store_excerpts.unwrap_or(store_excerpts && new_rights == "cleared");
    if new_store_excerpts && new_rights != "cleared" {
        return Err(AppError::BadRequest(
            "store_excerpts needs rights_status `cleared`".into(),
        ));
    }

    sqlx::query(
        "update source set
            name = coalesce($2, name), homepage_url = coalesce($3, homepage_url),
            publisher = coalesce($4, publisher), default_work_type = coalesce($5, default_work_type),
            licence = coalesce($6, licence), rights_status = $7, store_excerpts = $8,
            allow_ai_processing = coalesce($9, allow_ai_processing),
            poll_interval_minutes = coalesce($10, poll_interval_minutes), enabled = $11,
            -- re-enabling gives a struggling source a fresh start
            consecutive_failures = case when $11 and not enabled then 0 else consecutive_failures end,
            updated_at = now()
         where id = $1",
    )
    .bind(id)
    .bind(&p.name)
    .bind(&p.homepage_url)
    .bind(&p.publisher)
    .bind(&p.default_work_type)
    .bind(&p.licence)
    .bind(&new_rights)
    .bind(new_store_excerpts)
    .bind(p.allow_ai_processing)
    .bind(p.poll_interval_minutes)
    .bind(new_enabled)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        u.id,
        "source.updated",
        "source",
        id,
        json!({ "rights_status": new_rights, "enabled": new_enabled, "store_excerpts": new_store_excerpts }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(fetch_source(&s, id).await?))
}

/// Queue a poll right now, ignoring the schedule. Safe to repeat: one queued poll per source.
async fn poll_now(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let mut tx = s.pool.begin().await?;
    let (rights, enabled): (String, bool) =
        sqlx::query_as("select rights_status, enabled from source where id = $1 for update")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found("source"))?;
    if !enabled {
        return Err(AppError::Conflict {
            code: "source_disabled",
            message: format!("enable the source first (rights status `{rights}`)"),
        });
    }
    let existing: Option<Uuid> = sqlx::query_scalar(
        "select id from job where kind = 'poll_source' and payload ->> 'source_id' = $1::text
           and status in ('queued', 'running') limit 1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(job_id) = existing {
        return Ok((
            StatusCode::OK,
            Json(json!({ "job_id": job_id, "already_queued": true })),
        ));
    }
    let job_id = jobs::enqueue(&mut *tx, "poll_source", &json!({ "source_id": id }), None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("job enqueue returned no id"))?;
    audit(
        &mut tx,
        u.id,
        "source.poll_requested",
        "source",
        id,
        json!({ "job_id": job_id }),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "job_id": job_id, "already_queued": false })),
    ))
}

// ───────────────────────── Review queue ─────────────────────────

#[derive(Deserialize)]
struct ItemsQuery {
    /// `new` (default), `duplicate`, `promoted`, `rejected`, `changed` (promoted items that
    /// changed at the source) or `all`.
    status: Option<String>,
    source_id: Option<Uuid>,
    limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemOut {
    id: Uuid,
    source_id: Uuid,
    source_name: String,
    rights_status: String,
    external_id: String,
    canonical_url: Option<String>,
    doi: Option<String>,
    title: String,
    authors: Option<String>,
    excerpt: Option<String>,
    published_at: Option<DateTime<Utc>>,
    status: String,
    duplicate_of: Option<Uuid>,
    existing_work_id: Option<Uuid>,
    source_work_id: Option<Uuid>,
    changed_since_promotion: bool,
    revision_count: i32,
    rejection_reason: Option<String>,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
}

const ITEM_COLUMNS: &str = "i.id, i.source_id, s.name as source_name, s.rights_status, i.external_id,
    i.canonical_url, i.doi, i.title, i.authors, i.excerpt, i.published_at, i.status, i.duplicate_of,
    i.existing_work_id, i.source_work_id, i.changed_since_promotion, i.revision_count,
    i.rejection_reason, i.first_seen_at, i.last_seen_at";

async fn list_items(
    State(s): State<AppState>,
    _: Editor,
    Query(q): Query<ItemsQuery>,
) -> AppResult<Json<Vec<ItemOut>>> {
    let status = q.status.as_deref().unwrap_or("new");
    check_choice(
        "status",
        status,
        &["new", "duplicate", "promoted", "rejected", "changed", "all"],
    )?;
    let rows = sqlx::query_as(&format!(
        "select {ITEM_COLUMNS}
         from ingested_item i join source s on s.id = i.source_id
         where ($1 = 'all' or ($1 = 'changed' and i.changed_since_promotion) or i.status = $1)
           and ($2::uuid is null or i.source_id = $2)
         order by coalesce(i.published_at, i.first_seen_at) desc, i.id
         limit $3"
    ))
    .bind(status)
    .bind(q.source_id)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(rows))
}

async fn fetch_item(s: &AppState, id: Uuid) -> AppResult<ItemOut> {
    sqlx::query_as(&format!(
        "select {ITEM_COLUMNS} from ingested_item i join source s on s.id = i.source_id where i.id = $1"
    ))
    .bind(id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(|| AppError::not_found("ingested item"))
}

async fn get_item(State(s): State<AppState>, _: Editor, Path(id): Path<Uuid>) -> AppResult<Json<ItemOut>> {
    Ok(Json(fetch_item(&s, id).await?))
}

/// Optional overrides for the source work created from an item. Feed metadata is often
/// messy, so the editor can correct it here.
#[derive(Deserialize, Default)]
struct PromoteIn {
    work_type: Option<String>,
    title: Option<String>,
    author: Option<String>,
    publisher: Option<String>,
    published_on: Option<NaiveDate>,
    #[serde(default)]
    is_religious_text: bool,
    language: Option<String>,
    /// Promote even though the item looks like a duplicate.
    #[serde(default)]
    force: bool,
}

#[derive(sqlx::FromRow)]
struct PromoteRow {
    status: String,
    title: String,
    authors: Option<String>,
    canonical_url: Option<String>,
    doi: Option<String>,
    published_at: Option<DateTime<Utc>>,
    duplicate_of: Option<Uuid>,
    existing_work_id: Option<Uuid>,
    source_id: Uuid,
    default_work_type: String,
    publisher: Option<String>,
    licence: String,
    rights_status: String,
}

async fn promote(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<Value>)> {
    // The body is optional: an empty body promotes the item with its feed metadata as is.
    let p: PromoteIn = if body.iter().all(u8::is_ascii_whitespace) {
        PromoteIn::default()
    } else {
        serde_json::from_slice(&body).map_err(|e| AppError::BadRequest(format!("invalid JSON body: {e}")))?
    };
    let mut tx = s.pool.begin().await?;
    let item: PromoteRow = sqlx::query_as(
        "select i.status, i.title, i.authors, i.canonical_url, i.doi, i.published_at, i.duplicate_of,
                i.existing_work_id, s.id as source_id, s.default_work_type, s.publisher, s.licence, s.rights_status
         from ingested_item i join source s on s.id = i.source_id
         where i.id = $1 for update of i",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("ingested item"))?;

    match item.status.as_str() {
        "promoted" | "rejected" => {
            return Err(AppError::Conflict {
                code: "already_reviewed",
                message: format!("item is already {}", item.status),
            });
        }
        "duplicate" if !p.force => {
            return Err(AppError::Conflict {
                code: "duplicate",
                message: match (item.existing_work_id, item.duplicate_of) {
                    (Some(w), _) => format!("already in the catalogue as work {w}; pass force=true to add it anyway"),
                    (_, Some(d)) => format!("duplicate of ingested item {d}; pass force=true to add it anyway"),
                    _ => "item looks like a duplicate; pass force=true to add it anyway".into(),
                },
            });
        }
        _ => {}
    }
    // Rights are checked again now: the source's licence may have changed since ingestion.
    if item.rights_status != "cleared" && item.rights_status != "link_only" {
        return Err(AppError::Conflict {
            code: "rights_not_cleared",
            message: format!("the source's rights status is now `{}`", item.rights_status),
        });
    }
    let work_type = p.work_type.as_deref().unwrap_or(&item.default_work_type);
    check_choice("work_type", work_type, &WORK_TYPES)?;
    let title = p.title.as_deref().unwrap_or(&item.title);
    if title.trim().is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }

    let work_id: Uuid = sqlx::query_scalar(
        "insert into source_work (work_type, title, author, publisher, canonical_url, doi_or_isbn, published_on,
                                  licence, rights_status, is_religious_text, language, source_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, coalesce($11, 'en'), $12)
         returning id",
    )
    .bind(work_type)
    .bind(title.trim())
    .bind(p.author.as_ref().or(item.authors.as_ref()))
    .bind(p.publisher.as_ref().or(item.publisher.as_ref()))
    .bind(&item.canonical_url)
    .bind(&item.doi)
    .bind(p.published_on.or(item.published_at.map(|t| t.date_naive())))
    .bind(&item.licence)
    .bind(&item.rights_status)
    .bind(p.is_religious_text)
    .bind(&p.language)
    .bind(item.source_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "update ingested_item set status = 'promoted', source_work_id = $2, changed_since_promotion = false,
                                  updated_at = now()
         where id = $1",
    )
    .bind(id)
    .bind(work_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        u.id,
        "ingest.promoted",
        "ingested_item",
        id,
        json!({ "source_work_id": work_id, "forced": p.force }),
    )
    .await?;
    audit(
        &mut tx,
        u.id,
        "work.created",
        "source_work",
        work_id,
        json!({ "title": title, "from_item": id }),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "item_id": id, "status": "promoted", "source_work_id": work_id })),
    ))
}

#[derive(Deserialize)]
struct RejectIn {
    reason: String,
}

async fn reject(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    ApiJson(r): ApiJson<RejectIn>,
) -> AppResult<Json<Value>> {
    if r.reason.trim().is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }
    let mut tx = s.pool.begin().await?;
    let status: String = sqlx::query_scalar("select status from ingested_item where id = $1 for update")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::not_found("ingested item"))?;
    if status == "promoted" || status == "rejected" {
        return Err(AppError::Conflict {
            code: "already_reviewed",
            message: format!("item is already {status}"),
        });
    }
    sqlx::query(
        "update ingested_item set status = 'rejected', rejection_reason = $2, updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(r.reason.trim())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        u.id,
        "ingest.rejected",
        "ingested_item",
        id,
        json!({ "reason": r.reason.trim() }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "item_id": id, "status": "rejected" })))
}

/// The editor has checked a promoted item that changed at the source (and corrected any
/// cards built on it with a new card version or a withdrawal).
async fn acknowledge_change(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let mut tx = s.pool.begin().await?;
    let changed: bool =
        sqlx::query_scalar("select changed_since_promotion from ingested_item where id = $1 for update")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found("ingested item"))?;
    if !changed {
        return Err(AppError::Conflict {
            code: "no_change",
            message: "item has no unacknowledged change".into(),
        });
    }
    sqlx::query("update ingested_item set changed_since_promotion = false, updated_at = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        u.id,
        "ingest.change_acknowledged",
        "ingested_item",
        id,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "item_id": id, "changed_since_promotion": false })))
}

// ───────────────────────── Jobs ─────────────────────────

#[derive(Deserialize)]
struct JobsQuery {
    /// `queued`, `running`, `succeeded`, `dead` (default) or `all`.
    status: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct JobOut {
    id: Uuid,
    kind: String,
    payload: Value,
    status: String,
    attempt_count: i32,
    max_attempts: i32,
    run_after: DateTime<Utc>,
    locked_by: Option<String>,
    error_code: Option<String>,
    last_error: Option<String>,
    result: Option<Value>,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

async fn list_jobs(State(s): State<AppState>, _: Editor, Query(q): Query<JobsQuery>) -> AppResult<Json<Vec<JobOut>>> {
    let status = q.status.as_deref().unwrap_or("dead");
    check_choice("status", status, &["queued", "running", "succeeded", "dead", "all"])?;
    let rows = sqlx::query_as(
        "select id, kind, payload, status, attempt_count, max_attempts, run_after, locked_by, error_code,
                last_error, result, created_at, finished_at
         from job where ($1 = 'all' or status = $1)
         order by updated_at desc limit $2",
    )
    .bind(status)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(rows))
}

async fn retry_job(State(s): State<AppState>, Editor(u): Editor, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let mut tx = s.pool.begin().await?;
    if !jobs::retry_dead(&mut *tx, id).await? {
        let exists: bool = sqlx::query_scalar("select exists(select 1 from job where id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        return Err(if exists {
            AppError::Conflict {
                code: "not_dead",
                message: "only dead (dead-lettered) jobs can be retried".into(),
            }
        } else {
            AppError::not_found("job")
        });
    }
    audit(&mut tx, u.id, "job.retried", "job", id, json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({ "job_id": id, "status": "queued" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_host_defaults_to_feed_host_and_accepts_parents() {
        assert_eq!(
            validate_feed_url("https://export.arxiv.org/rss/cs", None).unwrap(),
            "export.arxiv.org"
        );
        assert_eq!(
            validate_feed_url("https://export.arxiv.org/rss/cs", Some("arxiv.org")).unwrap(),
            "arxiv.org"
        );
    }

    #[test]
    fn rejects_bad_feed_urls() {
        assert!(validate_feed_url("ftp://example.org/feed", None).is_err());
        assert!(validate_feed_url("https://user:pw@example.org/feed", None).is_err());
        assert!(validate_feed_url("https://example.org/feed", Some("other.org")).is_err());
        assert!(validate_feed_url("https://evilexample.org/feed", Some("example.org")).is_err());
        assert!(validate_feed_url("not a url", None).is_err());
    }
}
