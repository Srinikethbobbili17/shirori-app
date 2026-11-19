//! `/v1/admin/...`: editor and admin endpoints.

pub mod gate;
pub mod ingest;
pub mod service;

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
use uuid::Uuid;

use crate::{
    auth::Editor,
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use service::{CardIn, ConceptIn, RelationIn, VersionIn, WorkIn};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/works", get(list_works).post(create_work))
        .route("/concepts", get(list_concepts).post(create_concept))
        .route("/concept-relations", get(list_relations).post(create_relation))
        .route("/concept-relations/{id}/review", post(review_relation))
        .route("/cards", get(list_cards).post(create_card))
        .route("/cards/{id}", get(card_detail))
        .route("/cards/{id}/versions", post(add_version))
        .route("/cards/{id}/publish", post(publish))
        .route("/cards/{id}/withdraw", post(withdraw))
        .route("/inventory", get(inventory))
        .merge(ingest::router())
}

type Created = (StatusCode, Json<Value>);

// ───────────────────────── Works, concepts, relations ─────────────────────────

async fn create_work(State(s): State<AppState>, Editor(u): Editor, ApiJson(w): ApiJson<WorkIn>) -> AppResult<Created> {
    let mut tx = s.pool.begin().await?;
    let id = service::create_work(&mut tx, u.id, &w).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn list_works(State(s): State<AppState>, _: Editor) -> AppResult<Json<Value>> {
    let rows: Value = sqlx::query_scalar(
        "select coalesce(json_agg(w order by w.created_at desc), '[]') from
           (select id, work_type, title, author, publisher, canonical_url, doi_or_isbn, published_on,
                   licence, rights_status, is_religious_text, created_at from source_work) w",
    )
    .fetch_one(&s.pool)
    .await?;
    Ok(Json(rows))
}

async fn create_concept(
    State(s): State<AppState>,
    Editor(u): Editor,
    ApiJson(c): ApiJson<ConceptIn>,
) -> AppResult<Created> {
    let mut tx = s.pool.begin().await?;
    let id = service::create_concept(&mut tx, u.id, &c).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn list_concepts(State(s): State<AppState>, _: Editor) -> AppResult<Json<Value>> {
    let rows: Value = sqlx::query_scalar(
        "select coalesce(json_agg(c order by c.slug), '[]') from (select id, slug, name, description from concept) c",
    )
    .fetch_one(&s.pool)
    .await?;
    Ok(Json(rows))
}

async fn create_relation(
    State(s): State<AppState>,
    Editor(u): Editor,
    ApiJson(r): ApiJson<RelationIn>,
) -> AppResult<Created> {
    let mut tx = s.pool.begin().await?;
    let id = service::create_relation(&mut tx, u.id, &r).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "review_status": "proposed" })),
    ))
}

async fn list_relations(State(s): State<AppState>, _: Editor) -> AppResult<Json<Value>> {
    let rows: Value = sqlx::query_scalar(
        "select coalesce(json_agg(r order by r.created_at desc), '[]') from
           (select r.id, sc.slug as source, tc.slug as target, r.relation_type, r.rationale,
                   r.review_status, r.created_at
            from concept_relation r
            join concept sc on sc.id = r.source_concept_id
            join concept tc on tc.id = r.target_concept_id) r",
    )
    .fetch_one(&s.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct ReviewIn {
    decision: String,
}

async fn review_relation(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    ApiJson(r): ApiJson<ReviewIn>,
) -> AppResult<Json<Value>> {
    let approve = match r.decision.as_str() {
        "approve" => true,
        "reject" => false,
        _ => return Err(AppError::BadRequest("decision must be `approve` or `reject`".into())),
    };
    let mut tx = s.pool.begin().await?;
    service::review_relation(&mut tx, u.id, id, approve).await?;
    tx.commit().await?;
    Ok(Json(
        json!({ "id": id, "review_status": if approve { "approved" } else { "rejected" } }),
    ))
}

// ───────────────────────── Cards ─────────────────────────

async fn create_card(State(s): State<AppState>, Editor(u): Editor, ApiJson(c): ApiJson<CardIn>) -> AppResult<Created> {
    let mut tx = s.pool.begin().await?;
    let (card_id, version_id) = service::create_card(&mut tx, u.id, &c).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "card_id": card_id, "version_id": version_id, "lifecycle_state": "draft" })),
    ))
}

async fn add_version(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    ApiJson(v): ApiJson<VersionIn>,
) -> AppResult<Created> {
    let mut tx = s.pool.begin().await?;
    let version_id = service::add_version(&mut tx, u.id, id, &v).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "card_id": id, "version_id": version_id })),
    ))
}

#[derive(Deserialize)]
struct PublishIn {
    version_id: Option<Uuid>,
}

async fn publish(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> AppResult<Json<Value>> {
    // The body is optional: an empty body publishes the latest revision.
    let version = if body.iter().all(u8::is_ascii_whitespace) {
        None
    } else {
        serde_json::from_slice::<PublishIn>(&body)
            .map_err(|e| AppError::BadRequest(format!("invalid JSON body: {e}")))?
            .version_id
    };
    let mut tx = s.pool.begin().await?;
    let version_id = service::publish(&mut tx, u.id, id, version).await?;
    tx.commit().await?;
    Ok(Json(
        json!({ "card_id": id, "version_id": version_id, "lifecycle_state": "published" }),
    ))
}

#[derive(Deserialize)]
struct WithdrawIn {
    reason: String,
}

async fn withdraw(
    State(s): State<AppState>,
    Editor(u): Editor,
    Path(id): Path<Uuid>,
    ApiJson(w): ApiJson<WithdrawIn>,
) -> AppResult<Json<Value>> {
    let mut tx = s.pool.begin().await?;
    let withdrawn = service::withdraw(&mut tx, u.id, id, &w.reason).await?;
    tx.commit().await?;
    Ok(Json(json!({ "withdrawn_card_ids": withdrawn })))
}

#[derive(Deserialize)]
struct ListCardsQuery {
    state: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CardSummary {
    card_id: Uuid,
    category: String,
    field: String,
    lifecycle_state: String,
    importance: String,
    story_id: Option<String>,
    follows_up_card_id: Option<Uuid>,
    published_at: Option<DateTime<Utc>>,
    withdrawn_at: Option<DateTime<Utc>>,
    latest_version_id: Uuid,
    latest_revision: i32,
    headline: String,
    evidence_label: String,
    active_revision: Option<i32>,
}

async fn list_cards(
    State(s): State<AppState>,
    _: Editor,
    Query(q): Query<ListCardsQuery>,
) -> AppResult<Json<Vec<CardSummary>>> {
    let rows = sqlx::query_as(
        "select c.id as card_id, c.category, c.field, c.lifecycle_state, c.importance, c.story_id,
                c.follows_up_card_id, c.published_at, c.withdrawn_at,
                lv.id as latest_version_id, lv.revision as latest_revision, lv.headline, lv.evidence_label,
                av.revision as active_revision
         from card c
         join lateral (select * from card_version v where v.card_id = c.id order by v.revision desc limit 1) lv on true
         left join card_version av on av.id = c.active_version_id
         where ($1::text is null or c.lifecycle_state = $1)
         order by c.created_at desc",
    )
    .bind(&q.state)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct VersionRow {
    id: Uuid,
    revision: i32,
    headline: String,
    summary: String,
    deeper_explanation: Option<String>,
    claim_kind: String,
    evidence_label: String,
    limitations: Option<String>,
    source_date: NaiveDate,
    created_at: DateTime<Utc>,
}

async fn card_detail(State(s): State<AppState>, _: Editor, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let card: Value = sqlx::query_scalar(
        "select row_to_json(c) from (
            select c.id, c.category, c.field, c.primary_work_id, c.story_id, c.follows_up_card_id,
                   c.importance, c.lifecycle_state, c.active_version_id, c.published_at,
                   c.withdrawn_at, c.withdrawal_reason,
                   (select coalesce(json_agg(k.slug order by k.slug), '[]')
                    from card_concept cc join concept k on k.id = cc.concept_id where cc.card_id = c.id) as concepts,
                   (select row_to_json(x) from (select left_card_id, right_card_id, concept_relation_id, bridge_sentence
                                                from connection_card where card_id = c.id) x) as connection
            from card c where c.id = $1) c",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(|| AppError::not_found("card"))?;
    let versions: Vec<VersionRow> = sqlx::query_as(
        "select id, revision, headline, summary, deeper_explanation, claim_kind, evidence_label,
                limitations, source_date, created_at
         from card_version where card_id = $1 order by revision desc",
    )
    .bind(id)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!({ "card": card, "versions": versions })))
}

/// Unread-inventory health for editors (§7.3).
async fn inventory(State(s): State<AppState>, _: Editor) -> AppResult<Json<Value>> {
    let by_state: Value = sqlx::query_scalar(
        "select coalesce(json_object_agg(category, states), '{}') from (
            select category, json_object_agg(lifecycle_state, n) as states
            from (select category, lifecycle_state, count(*) as n from card group by 1, 2) x
            group by category) y",
    )
    .fetch_one(&s.pool)
    .await?;
    let usable_connections: i64 = sqlx::query_scalar(
        "select count(*) from connection_card cc
         join card c on c.id = cc.card_id and c.lifecycle_state = 'published'
         join card l on l.id = cc.left_card_id and l.lifecycle_state = 'published'
         join card r on r.id = cc.right_card_id and r.lifecycle_state = 'published'",
    )
    .fetch_one(&s.pool)
    .await?;
    Ok(Json(
        json!({ "cards_by_category_and_state": by_state, "usable_connection_cards": usable_connections }),
    ))
}
