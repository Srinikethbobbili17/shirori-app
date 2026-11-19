//! The publication gate (architecture §8.2, ADR-007/008/015/016).
//!
//! A card version may go public only if every check here passes. Returns the list of
//! human-readable failures (empty = publishable).

use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
struct GateRow {
    category: String,
    lifecycle_state: String,
    follows_up_state: Option<String>,
    version_card_id: Uuid,
    revision: i32,
    latest_revision: i32,
    claim_kind: String,
    evidence_label: String,
    limitations: Option<String>,
    rights_status: Option<String>,
    is_religious_text: Option<bool>,
}

#[derive(sqlx::FromRow)]
struct ConnectionGateRow {
    left_field: String,
    left_state: String,
    right_field: String,
    right_state: String,
    bridge_sentence: String,
    relation_status: Option<String>,
    concepts_match: bool,
}

pub const MAX_BRIDGE_SENTENCE_CHARS: usize = 300;

pub async fn check(conn: &mut PgConnection, card_id: Uuid, version_id: Uuid) -> AppResult<Vec<String>> {
    let row: GateRow = sqlx::query_as(
        "select c.category, c.lifecycle_state,
                (select f.lifecycle_state from card f where f.id = c.follows_up_card_id) as follows_up_state,
                v.card_id as version_card_id, v.revision,
                (select max(revision) from card_version where card_id = c.id) as latest_revision,
                v.claim_kind, v.evidence_label, v.limitations,
                w.rights_status, w.is_religious_text
         from card c
         join card_version v on v.id = $2
         left join source_work w on w.id = c.primary_work_id
         where c.id = $1",
    )
    .bind(card_id)
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::not_found("card version"))?;

    let mut failures = Vec::new();

    if row.version_card_id != card_id {
        return Err(AppError::BadRequest("version does not belong to this card".into()));
    }
    if row.lifecycle_state == "withdrawn" {
        failures.push("withdrawn cards cannot be republished; publish a follow-up card instead".into());
    }
    if row.revision != row.latest_revision {
        failures.push(format!(
            "stale review: revision {} was superseded by revision {}; only the latest revision can be approved",
            row.revision, row.latest_revision
        ));
    }
    if matches!(row.evidence_label.as_str(), "single_study" | "contested")
        && row.limitations.as_deref().is_none_or(|l| l.trim().is_empty())
    {
        failures.push(format!(
            "evidence label `{}` requires a \"What this doesn't show\" (limitations) line",
            row.evidence_label
        ));
    }
    if let Some(state) = &row.follows_up_state
        && state != "published"
        && state != "withdrawn"
    {
        failures.push("a follow-up must point at a published or withdrawn card".into());
    }

    if row.category == "connection" {
        check_connection(conn, card_id, &mut failures).await?;
    } else {
        match row.rights_status.as_deref() {
            Some("cleared" | "link_only") => {}
            Some(other) => failures.push(format!(
                "rights status of the source work is `{other}`; only `cleared` or `link_only` works can be published"
            )),
            None => failures.push("card has no source work".into()),
        }
        // Religion rule (§2.2): a religious text may be used only as a historical/literary source.
        if row.is_religious_text == Some(true) && row.claim_kind != "historical" {
            failures.push(format!(
                "religion rule: a card whose source is a religious text must make a `historical` claim \
                 (a clearly labelled historical or literary use), not a `{}` claim",
                row.claim_kind
            ));
        }
    }
    Ok(failures)
}

async fn check_connection(conn: &mut PgConnection, card_id: Uuid, failures: &mut Vec<String>) -> AppResult<()> {
    let row: Option<ConnectionGateRow> = sqlx::query_as(
        "select l.field as left_field, l.lifecycle_state as left_state,
                r.field as right_field, r.lifecycle_state as right_state,
                cc.bridge_sentence, rel.review_status as relation_status,
                exists (
                    select 1 from card_concept lc join card_concept rc on rc.card_id = r.id
                    where lc.card_id = l.id and (
                        (lc.concept_id = rel.source_concept_id and rc.concept_id = rel.target_concept_id)
                        or (lc.concept_id = rel.target_concept_id and rc.concept_id = rel.source_concept_id)
                    )
                ) as concepts_match
         from connection_card cc
         join card l on l.id = cc.left_card_id
         join card r on r.id = cc.right_card_id
         left join concept_relation rel on rel.id = cc.concept_relation_id
         where cc.card_id = $1",
    )
    .bind(card_id)
    .fetch_optional(conn)
    .await?;
    let Some(row) = row else {
        failures.push("connection card has no bridged cards".into());
        return Ok(());
    };
    if row.left_state != "published" || row.right_state != "published" {
        failures.push("both bridged cards must be published first".into());
    }
    if row.left_field == row.right_field {
        failures.push(format!(
            "a Connection card must bridge two different fields (both are `{}`)",
            row.left_field
        ));
    }
    match row.relation_status.as_deref() {
        Some("approved") => {}
        Some(status) => failures.push(format!("the concept relation is `{status}`; it must be approved")),
        None => failures.push("a Connection card needs a reviewed concept relation".into()),
    }
    if !row.concepts_match {
        failures.push("the concept relation must match concepts on both bridged cards".into());
    }
    if row.bridge_sentence.chars().count() > MAX_BRIDGE_SENTENCE_CHARS {
        failures.push(format!(
            "the bridge sentence must be one plain sentence (max {MAX_BRIDGE_SENTENCE_CHARS} characters)"
        ));
    }
    Ok(())
}
