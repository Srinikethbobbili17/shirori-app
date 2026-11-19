//! Reader-facing card read models and the public card/work endpoints.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

/// What a reader sees on a card. Always built from the card's *active* (approved) version.
#[derive(Debug, Clone, Serialize)]
pub struct CardView {
    pub card_id: Uuid,
    pub version_id: Uuid,
    pub revision: i32,
    pub category: String,
    pub field: String,
    pub headline: String,
    pub summary: String,
    pub deeper_explanation: Option<String>,
    pub claim_kind: String,
    pub evidence_label: String,
    /// "What this doesn't show".
    pub limitations: Option<String>,
    pub source_date: NaiveDate,
    pub importance: String,
    pub story_id: Option<String>,
    pub follows_up_card_id: Option<Uuid>,
    pub lifecycle_state: String,
    pub published_at: Option<DateTime<Utc>>,
    pub concepts: Vec<String>,
    pub source: Option<SourceView>,
    pub connection: Option<ConnectionView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceView {
    pub work_id: Uuid,
    pub work_type: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub url: Option<String>,
    pub doi_or_isbn: Option<String>,
    pub published_on: Option<NaiveDate>,
    pub licence: String,
    pub rights_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionView {
    pub left_card_id: Uuid,
    pub left_headline: String,
    pub left_field: String,
    pub right_card_id: Uuid,
    pub right_headline: String,
    pub right_field: String,
    pub bridge_sentence: String,
    pub relation_type: Option<String>,
    pub rationale: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CardRow {
    card_id: Uuid,
    version_id: Uuid,
    revision: i32,
    category: String,
    field: String,
    headline: String,
    summary: String,
    deeper_explanation: Option<String>,
    claim_kind: String,
    evidence_label: String,
    limitations: Option<String>,
    source_date: NaiveDate,
    importance: String,
    story_id: Option<String>,
    follows_up_card_id: Option<Uuid>,
    lifecycle_state: String,
    published_at: Option<DateTime<Utc>>,
    work_id: Option<Uuid>,
    work_type: Option<String>,
    title: Option<String>,
    author: Option<String>,
    publisher: Option<String>,
    canonical_url: Option<String>,
    doi_or_isbn: Option<String>,
    published_on: Option<NaiveDate>,
    licence: Option<String>,
    rights_status: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ConnectionRow {
    card_id: Uuid,
    left_card_id: Uuid,
    left_headline: String,
    left_field: String,
    right_card_id: Uuid,
    right_headline: String,
    right_field: String,
    bridge_sentence: String,
    relation_type: Option<String>,
    rationale: Option<String>,
}

/// Load reader views for the given cards (any lifecycle state that has an active version).
/// Callers decide whether withdrawn cards may be shown.
pub async fn load_card_views(pool: &PgPool, ids: &[Uuid]) -> AppResult<HashMap<Uuid, CardView>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<CardRow> = sqlx::query_as(
        "select c.id as card_id, v.id as version_id, v.revision, c.category, c.field,
                v.headline, v.summary, v.deeper_explanation, v.claim_kind, v.evidence_label,
                v.limitations, v.source_date, c.importance, c.story_id, c.follows_up_card_id,
                c.lifecycle_state, c.published_at,
                w.id as work_id, w.work_type, w.title, w.author, w.publisher, w.canonical_url,
                w.doi_or_isbn, w.published_on, w.licence, w.rights_status
         from card c
         join card_version v on v.id = c.active_version_id
         left join source_work w on w.id = c.primary_work_id
         where c.id = any($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    let mut concepts: HashMap<Uuid, Vec<String>> = HashMap::new();
    let concept_rows: Vec<(Uuid, String)> = sqlx::query_as(
        "select cc.card_id, k.slug from card_concept cc join concept k on k.id = cc.concept_id
         where cc.card_id = any($1) order by k.slug",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    for (card_id, slug) in concept_rows {
        concepts.entry(card_id).or_default().push(slug);
    }

    let connections: HashMap<Uuid, ConnectionView> = load_connection_rows(pool, ids)
        .await?
        .into_iter()
        .map(|r| {
            (
                r.card_id,
                ConnectionView {
                    left_card_id: r.left_card_id,
                    left_headline: r.left_headline,
                    left_field: r.left_field,
                    right_card_id: r.right_card_id,
                    right_headline: r.right_headline,
                    right_field: r.right_field,
                    bridge_sentence: r.bridge_sentence,
                    relation_type: r.relation_type,
                    rationale: r.rationale,
                },
            )
        })
        .collect();
    let mut connections = connections;

    Ok(rows
        .into_iter()
        .map(|r| {
            let source = r.work_id.map(|work_id| SourceView {
                work_id,
                work_type: r.work_type.unwrap_or_default(),
                title: r.title.unwrap_or_default(),
                author: r.author,
                publisher: r.publisher,
                url: r.canonical_url,
                doi_or_isbn: r.doi_or_isbn,
                published_on: r.published_on,
                licence: r.licence.unwrap_or_default(),
                rights_status: r.rights_status.unwrap_or_default(),
            });
            let view = CardView {
                card_id: r.card_id,
                version_id: r.version_id,
                revision: r.revision,
                category: r.category,
                field: r.field,
                headline: r.headline,
                summary: r.summary,
                deeper_explanation: r.deeper_explanation,
                claim_kind: r.claim_kind,
                evidence_label: r.evidence_label,
                limitations: r.limitations,
                source_date: r.source_date,
                importance: r.importance,
                story_id: r.story_id,
                follows_up_card_id: r.follows_up_card_id,
                lifecycle_state: r.lifecycle_state,
                published_at: r.published_at,
                concepts: concepts.remove(&r.card_id).unwrap_or_default(),
                source,
                connection: connections.remove(&r.card_id),
            };
            (view.card_id, view)
        })
        .collect())
}

async fn load_connection_rows(pool: &PgPool, ids: &[Uuid]) -> AppResult<Vec<ConnectionRow>> {
    Ok(sqlx::query_as(
        "select cc.card_id, cc.left_card_id, lv.headline as left_headline, lc.field as left_field,
                cc.right_card_id, rv.headline as right_headline, rc.field as right_field,
                cc.bridge_sentence, r.relation_type, r.rationale
         from connection_card cc
         join card lc on lc.id = cc.left_card_id
         join card_version lv on lv.id = lc.active_version_id
         join card rc on rc.id = cc.right_card_id
         join card_version rv on rv.id = rc.active_version_id
         left join concept_relation r on r.id = cc.concept_relation_id
         where cc.card_id = any($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?)
}

/// Load a single published card or explain why it can't be shown.
pub async fn published_card(pool: &PgPool, id: Uuid) -> AppResult<CardView> {
    let Some(view) = load_card_views(pool, &[id]).await?.remove(&id) else {
        return Err(AppError::not_found("card"));
    };
    match view.lifecycle_state.as_str() {
        "published" => Ok(view),
        "withdrawn" => Err(withdrawn_error(pool, id).await?),
        _ => Err(AppError::not_found("card")),
    }
}

/// Correction details for a withdrawn card: the reason and the newest published follow-up.
#[derive(Debug, Clone, Serialize)]
pub struct Withdrawal {
    pub reason: Option<String>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub follow_up_card_id: Option<Uuid>,
}

pub async fn withdrawal_info<'e>(db: impl PgExecutor<'e>, card_id: Uuid) -> AppResult<Withdrawal> {
    let (reason, withdrawn_at, follow_up_card_id): (Option<String>, Option<DateTime<Utc>>, Option<Uuid>) =
        sqlx::query_as(
            "select c.withdrawal_reason, c.withdrawn_at,
                    (select f.id from card f
                     where f.follows_up_card_id = c.id and f.lifecycle_state = 'published'
                     order by f.published_at desc limit 1)
             from card c where c.id = $1",
        )
        .bind(card_id)
        .fetch_one(db)
        .await?;
    Ok(Withdrawal {
        reason,
        withdrawn_at,
        follow_up_card_id,
    })
}

async fn withdrawn_error(pool: &PgPool, id: Uuid) -> AppResult<AppError> {
    let info = withdrawal_info(pool, id).await?;
    Ok(AppError::Gone {
        code: "card_withdrawn",
        message: "this card was withdrawn by the editors".into(),
        details: json!(info),
    })
}

// ───────────────────────── Handlers ─────────────────────────

pub async fn get_card(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<CardView>> {
    Ok(Json(published_card(&state.pool, id).await?))
}

#[derive(Serialize)]
pub struct CardSource {
    pub card_id: Uuid,
    #[serde(flatten)]
    pub source: SourceView,
}

/// Sources behind a card. For a Connection card, the sources of both bridged cards.
pub async fn get_sources(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<Vec<CardSource>>> {
    let card = published_card(&state.pool, id).await?;
    let ids = match &card.connection {
        Some(c) => vec![c.left_card_id, c.right_card_id],
        None => vec![card.card_id],
    };
    let mut views = load_card_views(&state.pool, &ids).await?;
    let sources = ids
        .iter()
        .filter_map(|id| views.remove(id))
        .filter_map(|v| {
            v.source.map(|source| CardSource {
                card_id: v.card_id,
                source,
            })
        })
        .collect();
    Ok(Json(sources))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RelatedConnection {
    pub connection_card_id: Uuid,
    pub bridge_sentence: String,
    pub other_card_id: Uuid,
    pub other_headline: String,
    pub other_field: String,
    pub relation_type: Option<String>,
    pub rationale: Option<String>,
}

/// Published Connection cards that bridge this card to another field.
pub async fn get_connections(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<RelatedConnection>>> {
    published_card(&state.pool, id).await?;
    let rows: Vec<RelatedConnection> = sqlx::query_as(
        "select cc.card_id as connection_card_id, cc.bridge_sentence,
                o.id as other_card_id, ov.headline as other_headline, o.field as other_field,
                r.relation_type, r.rationale
         from connection_card cc
         join card c on c.id = cc.card_id and c.lifecycle_state = 'published'
         join card o on o.id = case when cc.left_card_id = $1 then cc.right_card_id else cc.left_card_id end
         join card_version ov on ov.id = o.active_version_id
         left join concept_relation r on r.id = cc.concept_relation_id
         where (cc.left_card_id = $1 or cc.right_card_id = $1) and o.lifecycle_state = 'published'
         order by c.published_at desc",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkView {
    pub id: Uuid,
    pub work_type: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub canonical_url: Option<String>,
    pub doi_or_isbn: Option<String>,
    pub edition: Option<String>,
    pub published_on: Option<NaiveDate>,
    pub licence: String,
    pub rights_status: String,
    pub language: String,
}

/// Work metadata plus the published cards drawn from it (exploration paths).
pub async fn get_work(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<serde_json::Value>> {
    let work: WorkView = sqlx::query_as(
        "select id, work_type, title, author, publisher, canonical_url, doi_or_isbn, edition,
                published_on, licence, rights_status, language
         from source_work where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("work"))?;
    let cards: Vec<(Uuid, String)> = sqlx::query_as(
        "select c.id, v.headline from card c join card_version v on v.id = c.active_version_id
         where c.primary_work_id = $1 and c.lifecycle_state = 'published' order by c.published_at desc",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let cards: Vec<_> = cards
        .into_iter()
        .map(|(id, headline)| json!({ "card_id": id, "headline": headline }))
        .collect();
    Ok(Json(json!({ "work": work, "cards": cards })))
}
