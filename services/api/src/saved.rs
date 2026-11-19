//! The reader's Saved list (FR-11a, QR-02b).
//!
//! Saved cards are functional data: they work without analytics consent and are deleted with
//! the account. Saving does not remove a card from the no-repeat ledger; Saved is the one
//! deliberate way to reopen a read card.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    catalogue::{CardView, Withdrawal, load_card_views, withdrawal_info},
    error::{ApiJson, AppError, AppResult},
    events::{AnalyticsEvent, record_analytics},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SaveIn {
    pub saved_from: String,
}

pub async fn save(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
    ApiJson(body): ApiJson<SaveIn>,
) -> AppResult<Json<Value>> {
    if !["dose", "discover", "connection", "search"].contains(&body.saved_from.as_str()) {
        return Err(AppError::BadRequest(
            "saved_from must be one of: dose, discover, connection, search".into(),
        ));
    }
    let mut tx = state.pool.begin().await?;
    let version: Option<Uuid> =
        sqlx::query_scalar("select active_version_id from card where id = $1 and lifecycle_state = 'published'")
            .bind(card_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    let Some(version) = version else {
        return Err(AppError::not_found("published card"));
    };
    let inserted = sqlx::query(
        "insert into saved_card (user_id, card_id, saved_version_id, saved_from)
         values ($1, $2, $3, $4) on conflict (user_id, card_id) do nothing",
    )
    .bind(user.id)
    .bind(card_id)
    .bind(version)
    .bind(&body.saved_from)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted > 0 {
        let mode = match body.saved_from.as_str() {
            "dose" | "connection" => Some("dose"),
            "discover" => Some("discover"),
            _ => None,
        };
        record_analytics(
            &mut tx,
            &user,
            AnalyticsEvent::server("card_saved", Some(card_id), mode),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "card_id": card_id, "saved": true })))
}

pub async fn unsave(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    sqlx::query("delete from saved_card where user_id = $1 and card_id = $2")
        .bind(user.id)
        .bind(card_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "card_id": card_id, "saved": false })))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    /// Opaque cursor from the previous page's `next_cursor`.
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SavedItem {
    pub card_id: Uuid,
    pub saved_at: DateTime<Utc>,
    pub saved_from: String,
    pub status: String,
    /// The editors published a newer revision since the reader saved this card.
    pub changed_since_saved: bool,
    /// Current published version. `None` if the card was withdrawn: the old text is not shown.
    pub card: Option<CardView>,
    pub withdrawal: Option<Withdrawal>,
    /// For withdrawn cards: the published correction or follow-up, if any.
    pub follow_up: Option<CardView>,
}

#[derive(Debug, Serialize)]
pub struct SavedPage {
    pub items: Vec<SavedItem>,
    pub next_cursor: Option<String>,
}

fn encode_cursor(at: DateTime<Utc>, id: Uuid) -> String {
    format!("{}_{}", at.timestamp_micros(), id)
}

fn decode_cursor(cursor: &str) -> AppResult<(DateTime<Utc>, Uuid)> {
    let bad = || AppError::BadRequest("invalid cursor".into());
    let (micros, id) = cursor.split_once('_').ok_or_else(bad)?;
    let at = DateTime::from_timestamp_micros(micros.parse().map_err(|_| bad())?).ok_or_else(bad)?;
    Ok((at, id.parse().map_err(|_| bad())?))
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<SavedPage>> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let (after_at, after_id) = match q.cursor.as_deref() {
        Some(c) => {
            let (at, id) = decode_cursor(c)?;
            (Some(at), Some(id))
        }
        None => (None, None),
    };
    // card_id, saved_at, saved_from, saved_version_id, lifecycle_state, active_version_id
    type Row = (Uuid, DateTime<Utc>, String, Uuid, String, Option<Uuid>);
    let rows: Vec<Row> = sqlx::query_as(
        "select s.card_id, s.saved_at, s.saved_from, s.saved_version_id, c.lifecycle_state, c.active_version_id
         from saved_card s join card c on c.id = s.card_id
         where s.user_id = $1
           and ($2::timestamptz is null or (s.saved_at, s.card_id) < ($2, $3))
         order by s.saved_at desc, s.card_id desc
         limit $4",
    )
    .bind(user.id)
    .bind(after_at)
    .bind(after_id)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await?;

    let has_more = rows.len() as i64 > limit;
    let rows = &rows[..rows.len().min(limit as usize)];
    let ids: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let mut views = load_card_views(&state.pool, &ids).await?;

    let mut items = Vec::with_capacity(rows.len());
    for (card_id, saved_at, saved_from, saved_version, lifecycle_state, active_version) in rows {
        let changed_since_saved = active_version.is_some_and(|v| v != *saved_version);
        let (card, withdrawal, follow_up) = if lifecycle_state == "withdrawn" {
            let info = withdrawal_info(&state.pool, *card_id).await?;
            let follow_up = match info.follow_up_card_id {
                Some(id) => load_card_views(&state.pool, &[id]).await?.remove(&id),
                None => None,
            };
            (None, Some(info), follow_up)
        } else {
            (views.remove(card_id), None, None)
        };
        items.push(SavedItem {
            card_id: *card_id,
            saved_at: *saved_at,
            saved_from: saved_from.clone(),
            status: lifecycle_state.clone(),
            changed_since_saved,
            card,
            withdrawal,
            follow_up,
        });
    }
    let next_cursor = if has_more {
        rows.last().map(|r| encode_cursor(r.1, r.0))
    } else {
        None
    };
    Ok(Json(SavedPage { items, next_cursor }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips() {
        let at = DateTime::from_timestamp_micros(1_762_400_000_123_456).unwrap();
        let id = Uuid::from_u128(42);
        assert_eq!(decode_cursor(&encode_cursor(at, id)).unwrap(), (at, id));
        assert!(decode_cursor("nope").is_err());
    }
}
