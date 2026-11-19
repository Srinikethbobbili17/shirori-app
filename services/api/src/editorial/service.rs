//! Editorial operations, shared by the admin API and the seed loader.
//! Every function takes a connection so callers control the transaction.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::gate;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct WorkIn {
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
    #[serde(default)]
    pub is_religious_text: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConceptIn {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationIn {
    /// Concept slugs.
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionIn {
    pub headline: String,
    pub summary: String,
    pub deeper_explanation: Option<String>,
    pub claim_kind: String,
    pub evidence_label: String,
    pub limitations: Option<String>,
    pub source_date: NaiveDate,
    pub model_lineage: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionIn {
    pub left_card_id: Uuid,
    pub right_card_id: Uuid,
    pub concept_relation_id: Option<Uuid>,
    pub bridge_sentence: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CardIn {
    pub category: String,
    pub field: String,
    pub primary_work_id: Option<Uuid>,
    pub story_id: Option<String>,
    pub follows_up_card_id: Option<Uuid>,
    pub importance: Option<String>,
    /// Concept slugs.
    #[serde(default)]
    pub concepts: Vec<String>,
    pub version: VersionIn,
    pub connection: Option<ConnectionIn>,
}

pub async fn audit(
    conn: &mut PgConnection,
    actor: Uuid,
    action: &str,
    entity_type: &str,
    entity_id: Uuid,
    details: Value,
) -> AppResult<()> {
    sqlx::query(
        "insert into audit_event (actor_id, action, entity_type, entity_id, details) values ($1, $2, $3, $4, $5)",
    )
    .bind(actor)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(details)
    .execute(conn)
    .await?;
    Ok(())
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 60
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub async fn create_work(conn: &mut PgConnection, actor: Uuid, w: &WorkIn) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "insert into source_work (work_type, title, author, publisher, canonical_url, doi_or_isbn, edition,
                                  published_on, licence, rights_status, is_religious_text, language)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, coalesce($12, 'en')) returning id",
    )
    .bind(&w.work_type)
    .bind(&w.title)
    .bind(&w.author)
    .bind(&w.publisher)
    .bind(&w.canonical_url)
    .bind(&w.doi_or_isbn)
    .bind(&w.edition)
    .bind(w.published_on)
    .bind(&w.licence)
    .bind(&w.rights_status)
    .bind(w.is_religious_text)
    .bind(&w.language)
    .fetch_one(&mut *conn)
    .await?;
    audit(
        conn,
        actor,
        "work.created",
        "source_work",
        id,
        json!({ "title": w.title }),
    )
    .await?;
    Ok(id)
}

pub async fn create_concept(conn: &mut PgConnection, actor: Uuid, c: &ConceptIn) -> AppResult<Uuid> {
    if !valid_slug(&c.slug) {
        return Err(AppError::BadRequest("concept slug must match [a-z0-9-]{1,60}".into()));
    }
    let id: Uuid = sqlx::query_scalar("insert into concept (slug, name, description) values ($1, $2, $3) returning id")
        .bind(&c.slug)
        .bind(&c.name)
        .bind(&c.description)
        .fetch_one(&mut *conn)
        .await?;
    audit(conn, actor, "concept.created", "concept", id, json!({ "slug": c.slug })).await?;
    Ok(id)
}

async fn concept_id(conn: &mut PgConnection, slug: &str) -> AppResult<Uuid> {
    sqlx::query_scalar("select id from concept where slug = $1")
        .bind(slug)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("unknown concept `{slug}`")))
}

pub async fn create_relation(conn: &mut PgConnection, actor: Uuid, r: &RelationIn) -> AppResult<Uuid> {
    if r.rationale.trim().is_empty() {
        return Err(AppError::BadRequest("a concept relation needs a rationale".into()));
    }
    let source = concept_id(conn, &r.source).await?;
    let target = concept_id(conn, &r.target).await?;
    let id: Uuid = sqlx::query_scalar(
        "insert into concept_relation (source_concept_id, target_concept_id, relation_type, rationale)
         values ($1, $2, $3, $4) returning id",
    )
    .bind(source)
    .bind(target)
    .bind(&r.relation_type)
    .bind(&r.rationale)
    .fetch_one(&mut *conn)
    .await?;
    audit(
        conn,
        actor,
        "relation.proposed",
        "concept_relation",
        id,
        json!({ "source": r.source, "target": r.target }),
    )
    .await?;
    Ok(id)
}

pub async fn review_relation(conn: &mut PgConnection, actor: Uuid, id: Uuid, approve: bool) -> AppResult<()> {
    let status = if approve { "approved" } else { "rejected" };
    let updated = sqlx::query("update concept_relation set review_status = $2 where id = $1")
        .bind(id)
        .bind(status)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::not_found("concept relation"));
    }
    audit(
        conn,
        actor,
        &format!("relation.{status}"),
        "concept_relation",
        id,
        json!({}),
    )
    .await
}

async fn insert_version(
    conn: &mut PgConnection,
    actor: Uuid,
    card_id: Uuid,
    revision: i32,
    v: &VersionIn,
) -> AppResult<Uuid> {
    Ok(sqlx::query_scalar(
        "insert into card_version (card_id, revision, headline, summary, deeper_explanation, claim_kind,
                                   evidence_label, limitations, source_date, model_lineage, editor_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) returning id",
    )
    .bind(card_id)
    .bind(revision)
    .bind(v.headline.trim())
    .bind(v.summary.trim())
    .bind(&v.deeper_explanation)
    .bind(&v.claim_kind)
    .bind(&v.evidence_label)
    .bind(v.limitations.as_deref().map(str::trim).filter(|l| !l.is_empty()))
    .bind(v.source_date)
    .bind(&v.model_lineage)
    .bind(actor)
    .fetch_one(conn)
    .await?)
}

/// Create a draft card with its first version. Returns `(card_id, version_id)`.
pub async fn create_card(conn: &mut PgConnection, actor: Uuid, c: &CardIn) -> AppResult<(Uuid, Uuid)> {
    let is_connection = c.category == "connection";
    if is_connection != c.connection.is_some() {
        return Err(AppError::BadRequest(
            "connection details are required for, and only allowed on, `connection` cards".into(),
        ));
    }
    let field = c.field.trim().to_lowercase();
    if field.is_empty() {
        return Err(AppError::BadRequest("field is required".into()));
    }
    let card_id: Uuid = sqlx::query_scalar(
        "insert into card (category, field, primary_work_id, story_id, follows_up_card_id, importance)
         values ($1, $2, $3, $4, $5, coalesce($6, 'normal')) returning id",
    )
    .bind(&c.category)
    .bind(&field)
    .bind(c.primary_work_id)
    .bind(&c.story_id)
    .bind(c.follows_up_card_id)
    .bind(&c.importance)
    .fetch_one(&mut *conn)
    .await?;
    let version_id = insert_version(conn, actor, card_id, 1, &c.version).await?;

    for slug in &c.concepts {
        let concept = concept_id(conn, slug).await?;
        sqlx::query("insert into card_concept (card_id, concept_id) values ($1, $2) on conflict do nothing")
            .bind(card_id)
            .bind(concept)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(link) = &c.connection {
        sqlx::query(
            "insert into connection_card (card_id, left_card_id, right_card_id, concept_relation_id, bridge_sentence)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(card_id)
        .bind(link.left_card_id)
        .bind(link.right_card_id)
        .bind(link.concept_relation_id)
        .bind(link.bridge_sentence.trim())
        .execute(&mut *conn)
        .await?;
    }
    audit(
        conn,
        actor,
        "card.created",
        "card",
        card_id,
        json!({ "version_id": version_id, "category": c.category }),
    )
    .await?;
    Ok((card_id, version_id))
}

/// Add a new draft revision. The current public version stays live until this one is published.
pub async fn add_version(conn: &mut PgConnection, actor: Uuid, card_id: Uuid, v: &VersionIn) -> AppResult<Uuid> {
    let state: String = sqlx::query_scalar("select lifecycle_state from card where id = $1 for update")
        .bind(card_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("card"))?;
    if state == "withdrawn" {
        return Err(AppError::Conflict {
            code: "card_withdrawn",
            message: "withdrawn cards cannot be revised; create a follow-up card".into(),
        });
    }
    let next: i32 = sqlx::query_scalar("select coalesce(max(revision), 0) + 1 from card_version where card_id = $1")
        .bind(card_id)
        .fetch_one(&mut *conn)
        .await?;
    let version_id = insert_version(conn, actor, card_id, next, v).await?;
    audit(
        conn,
        actor,
        "card.revised",
        "card",
        card_id,
        json!({ "version_id": version_id, "revision": next }),
    )
    .await?;
    Ok(version_id)
}

/// Approve and publish a version (latest revision by default) after the publication gate.
pub async fn publish(conn: &mut PgConnection, actor: Uuid, card_id: Uuid, version_id: Option<Uuid>) -> AppResult<Uuid> {
    sqlx::query("select 1 from card where id = $1 for update")
        .bind(card_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("card"))?;
    let version_id = match version_id {
        Some(v) => v,
        None => {
            sqlx::query_scalar("select id from card_version where card_id = $1 order by revision desc limit 1")
                .bind(card_id)
                .fetch_one(&mut *conn)
                .await?
        }
    };
    let failures = gate::check(conn, card_id, version_id).await?;
    if !failures.is_empty() {
        return Err(AppError::GateFailed(failures));
    }
    sqlx::query(
        "update card set lifecycle_state = 'published', active_version_id = $2,
                         published_at = coalesce(published_at, now())
         where id = $1",
    )
    .bind(card_id)
    .bind(version_id)
    .execute(&mut *conn)
    .await?;
    audit(
        conn,
        actor,
        "card.published",
        "card",
        card_id,
        json!({ "version_id": version_id }),
    )
    .await?;
    Ok(version_id)
}

/// Withdraw a card. Connection cards that bridge it are withdrawn too (§6.5).
/// Returns every card id that was withdrawn.
pub async fn withdraw(conn: &mut PgConnection, actor: Uuid, card_id: Uuid, reason: &str) -> AppResult<Vec<Uuid>> {
    if reason.trim().is_empty() {
        return Err(AppError::BadRequest(
            "a withdrawal needs a reason readers can see".into(),
        ));
    }
    let state: String = sqlx::query_scalar("select lifecycle_state from card where id = $1 for update")
        .bind(card_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("card"))?;
    if state == "withdrawn" {
        return Ok(vec![]);
    }
    sqlx::query(
        "update card set lifecycle_state = 'withdrawn', withdrawn_at = now(), withdrawal_reason = $2 where id = $1",
    )
    .bind(card_id)
    .bind(reason.trim())
    .execute(&mut *conn)
    .await?;
    audit(
        conn,
        actor,
        "card.withdrawn",
        "card",
        card_id,
        json!({ "reason": reason }),
    )
    .await?;

    let dependants: Vec<Uuid> = sqlx::query_scalar(
        "update card set lifecycle_state = 'withdrawn', withdrawn_at = now(),
                         withdrawal_reason = 'A card this connection relies on was withdrawn: ' || $2
         where lifecycle_state <> 'withdrawn'
           and id in (select card_id from connection_card where left_card_id = $1 or right_card_id = $1)
         returning id",
    )
    .bind(card_id)
    .bind(reason.trim())
    .fetch_all(&mut *conn)
    .await?;
    for dep in &dependants {
        audit(
            conn,
            actor,
            "card.withdrawn",
            "card",
            *dep,
            json!({ "cascade_from": card_id }),
        )
        .await?;
    }
    let mut all = vec![card_id];
    all.extend(dependants);
    Ok(all)
}
