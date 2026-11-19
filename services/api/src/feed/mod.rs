//! Daily dose and Discover endpoints.

pub mod discover;
pub mod dose;

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    catalogue::{CardView, Withdrawal, load_card_views, withdrawal_info},
    error::{AppError, AppResult},
    events::{AnalyticsEvent, record_analytics},
    state::{AppState, DateOverride},
};

// ───────────────────────── Daily dose ─────────────────────────

#[derive(Debug, Serialize)]
pub struct DoseResponse {
    pub dose_id: Uuid,
    pub dose_date: NaiveDate,
    pub status: String,
    pub ends_with_connection: bool,
    /// True once today's dose is completed. The app then allows the swipe into Discover.
    pub discover_available: bool,
    pub items: Vec<DoseItem>,
}

#[derive(Debug, Serialize)]
pub struct DoseItem {
    pub position: i32,
    pub card_id: Uuid,
    pub read: bool,
    /// `None` when the card has been withdrawn since the dose was assembled.
    pub card: Option<CardView>,
    pub withdrawn: Option<Withdrawal>,
}

pub async fn get_daily(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(date): Query<DateOverride>,
) -> AppResult<Json<DoseResponse>> {
    let today = state.reader_today(&user, &date).await?;
    let dose_id = match find_dose(&state.pool, user.id, today).await? {
        Some((id, _)) => id,
        None => create_dose(&state, &user, today).await?,
    };
    Ok(Json(render_dose(&state, &user, dose_id, today).await?))
}

async fn find_dose<'e>(db: impl PgExecutor<'e>, user_id: Uuid, date: NaiveDate) -> AppResult<Option<(Uuid, String)>> {
    Ok(
        sqlx::query_as("select id, status from daily_dose where user_id = $1 and dose_date = $2")
            .bind(user_id)
            .bind(date)
            .fetch_optional(db)
            .await?,
    )
}

/// Serialise feed writes per reader so concurrent requests can't build two doses or serve the
/// same Discover card twice.
async fn lock_reader(conn: &mut PgConnection, user_id: Uuid) -> AppResult<()> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(user_id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

async fn create_dose(state: &AppState, user: &CurrentUser, today: NaiveDate) -> AppResult<Uuid> {
    let mut tx = state.pool.begin().await?;
    lock_reader(&mut tx, user.id).await?;
    if let Some((id, _)) = find_dose(&mut *tx, user.id, today).await? {
        tx.commit().await?;
        return Ok(id);
    }

    type ConnectionRow = (Uuid, Uuid, Uuid, bool, Option<DateTime<Utc>>);
    let connections: Vec<ConnectionRow> = sqlx::query_as(
        "select c.id, cc.left_card_id, cc.right_card_id, c.importance = 'important', c.published_at
         from card c
         join connection_card cc on cc.card_id = c.id
         join card l on l.id = cc.left_card_id
         join card r on r.id = cc.right_card_id
         where c.lifecycle_state = 'published'
           and l.lifecycle_state = 'published'
           and r.lifecycle_state = 'published'
           and not exists (select 1 from read_card rc
                           where rc.user_id = $1 and rc.card_id in (c.id, l.id, r.id))",
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await?;

    let pool: Vec<(Uuid, String, bool, Option<DateTime<Utc>>)> = sqlx::query_as(
        "select c.id, c.category, c.importance = 'important', c.published_at
         from card c
         where c.lifecycle_state = 'published' and c.category <> 'connection'
           and not exists (select 1 from read_card rc where rc.user_id = $1 and rc.card_id = c.id)",
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await?;

    let connections: Vec<_> = connections
        .into_iter()
        .map(|(id, left, right, important, published_at)| dose::ConnectionCandidate {
            id,
            left,
            right,
            important,
            published_at,
        })
        .collect();
    let pool: Vec<_> = pool
        .into_iter()
        .map(|(id, category, important, published_at)| dose::PoolCard {
            id,
            category,
            important,
            published_at,
        })
        .collect();

    let assembled = dose::assemble(&connections, &pool, state.config.dose_size);
    if !assembled.ends_with_connection {
        tracing::warn!(user = %user.id, "EDITORIAL ALERT: no unread Connection card; dose ships without one");
    }
    if assembled.card_ids.len() < 5 {
        tracing::warn!(user = %user.id, cards = assembled.card_ids.len(), "EDITORIAL ALERT: low unread inventory");
    }

    let dose_id: Uuid = sqlx::query_scalar("insert into daily_dose (user_id, dose_date) values ($1, $2) returning id")
        .bind(user.id)
        .bind(today)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        "insert into daily_dose_item (dose_id, position, card_id, card_version_id)
         select $1, t.pos::int, c.id, c.active_version_id
         from unnest($2::uuid[]) with ordinality as t(card_id, pos)
         join card c on c.id = t.card_id",
    )
    .bind(dose_id)
    .bind(&assembled.card_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(dose_id)
}

async fn render_dose(state: &AppState, user: &CurrentUser, dose_id: Uuid, today: NaiveDate) -> AppResult<DoseResponse> {
    let (dose_date, status): (NaiveDate, String) =
        sqlx::query_as("select dose_date, status from daily_dose where id = $1")
            .bind(dose_id)
            .fetch_one(&state.pool)
            .await?;
    let rows: Vec<(i32, Uuid, String, bool)> = sqlx::query_as(
        "select i.position, i.card_id, c.lifecycle_state,
                exists (select 1 from read_card r where r.user_id = $2 and r.card_id = i.card_id)
         from daily_dose_item i join card c on c.id = i.card_id
         where i.dose_id = $1 order by i.position",
    )
    .bind(dose_id)
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.1).collect();
    let mut views = load_card_views(&state.pool, &ids).await?;
    let mut items = Vec::with_capacity(rows.len());
    for (position, card_id, lifecycle_state, read) in rows {
        let (card, withdrawn) = if lifecycle_state == "withdrawn" {
            (None, Some(withdrawal_info(&state.pool, card_id).await?))
        } else {
            (views.remove(&card_id), None)
        };
        items.push(DoseItem {
            position,
            card_id,
            read,
            card,
            withdrawn,
        });
    }
    let ends_with_connection = items
        .last()
        .and_then(|i| i.card.as_ref())
        .is_some_and(|c| c.category == "connection");
    let discover_available = status == "completed" && dose_date == today;
    Ok(DoseResponse {
        dose_id,
        dose_date,
        status,
        ends_with_connection,
        discover_available,
        items,
    })
}

/// Mark a dose as done. Every card in it counts as read from now on (no-repeat ledger).
pub async fn complete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(dose_id): Path<Uuid>,
    Query(date): Query<DateOverride>,
) -> AppResult<Json<serde_json::Value>> {
    let mut tx = state.pool.begin().await?;
    let dose_date: NaiveDate = sqlx::query_scalar(
        "update daily_dose set status = 'completed', completed_at = coalesce(completed_at, now())
         where id = $1 and user_id = $2 returning dose_date",
    )
    .bind(dose_id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("dose"))?;
    sqlx::query(
        "insert into read_card (user_id, card_id, feed_mode)
         select $1, card_id, 'dose' from daily_dose_item where dose_id = $2
         on conflict do nothing",
    )
    .bind(user.id)
    .bind(dose_id)
    .execute(&mut *tx)
    .await?;
    record_analytics(
        &mut tx,
        &user,
        AnalyticsEvent::server("dose_completed", None, Some("dose")),
    )
    .await?;
    tx.commit().await?;

    let today = state.reader_today(&user, &date).await?;
    Ok(Json(serde_json::json!({
        "dose_id": dose_id,
        "status": "completed",
        "discover_available": dose_date == today,
    })))
}

// ───────────────────────── Discover ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct DiscoverQuery {
    pub date: Option<NaiveDate>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub items: Vec<DiscoverItem>,
    /// More unread cards are available; call again for the next page.
    pub has_more: bool,
    /// The reader has read everything new for now. A natural finish, not an error.
    pub exhausted: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoverItem {
    pub reason: discover::Reason,
    pub related_to_card_id: Option<Uuid>,
    pub card: CardView,
}

/// The next page of Discover. Pagination is server-side: every card handed out today is
/// remembered, so calling again simply continues.
pub async fn discover(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DiscoverQuery>,
) -> AppResult<Json<DiscoverResponse>> {
    let today = state.reader_today(&user, &DateOverride { date: q.date }).await?;
    let limit = q.limit.unwrap_or(5).clamp(1, 20);

    let mut tx = state.pool.begin().await?;
    lock_reader(&mut tx, user.id).await?;
    let dose_id = match find_dose(&mut *tx, user.id, today).await? {
        Some((id, status)) if status == "completed" => id,
        _ => {
            return Err(AppError::Conflict {
                code: "dose_not_completed",
                message: "Discover opens after today's daily dose is finished".into(),
            });
        }
    };

    let dose_cards: Vec<(Uuid, Option<String>, Vec<Uuid>)> = sqlx::query_as(
        "select c.id, c.story_id,
                coalesce(array_agg(cc.concept_id) filter (where cc.concept_id is not null), '{}')
         from daily_dose_item i
         join card c on c.id = i.card_id
         left join card_concept cc on cc.card_id = c.id
         where i.dose_id = $1 and c.category <> 'connection'
         group by c.id, c.story_id",
    )
    .bind(dose_id)
    .fetch_all(&mut *tx)
    .await?;

    let read_cards: Vec<(Uuid, bool)> = sqlx::query_as(
        "select r.card_id, c.lifecycle_state = 'withdrawn'
         from read_card r join card c on c.id = r.card_id where r.user_id = $1",
    )
    .bind(user.id)
    .fetch_all(&mut *tx)
    .await?;

    let relations: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "select source_concept_id, target_concept_id from concept_relation where review_status = 'approved'",
    )
    .fetch_all(&mut *tx)
    .await?;

    let interests: Vec<(Uuid, i32)> = if user.personalisation_consent {
        sqlx::query_as(
            "select cc.concept_id, count(*)::int4
             from card_concept cc
             join (select card_id from saved_card where user_id = $1
                   union all
                   select card_id from user_event
                   where user_id = $1 and event_type = 'card_liked' and card_id is not null) s
               on s.card_id = cc.card_id
             group by cc.concept_id",
        )
        .bind(user.id)
        .fetch_all(&mut *tx)
        .await?
    } else {
        Vec::new()
    };

    let related_served: i64 = sqlx::query_scalar(
        "select count(*) from discover_impression
         where user_id = $1 and dose_date = $2 and reason <> 'general'",
    )
    .bind(user.id)
    .bind(today)
    .fetch_one(&mut *tx)
    .await?;

    #[allow(clippy::type_complexity)]
    let candidates: Vec<(
        Uuid,
        String,
        bool,
        Option<DateTime<Utc>>,
        Option<String>,
        Option<Uuid>,
        Vec<Uuid>,
    )> = sqlx::query_as(
        "select c.id, c.category, c.importance = 'important', c.published_at, c.story_id,
                    c.follows_up_card_id,
                    coalesce(array_agg(cc.concept_id) filter (where cc.concept_id is not null), '{}')
             from card c
             left join card_concept cc on cc.card_id = c.id
             where c.lifecycle_state = 'published' and c.category <> 'connection'
               and not exists (select 1 from read_card r where r.user_id = $1 and r.card_id = c.id)
               and not exists (select 1 from discover_impression di
                               where di.user_id = $1 and di.dose_date = $2 and di.card_id = c.id)
               and not exists (select 1 from daily_dose_item i where i.dose_id = $3 and i.card_id = c.id)
             group by c.id",
    )
    .bind(user.id)
    .bind(today)
    .bind(dose_id)
    .fetch_all(&mut *tx)
    .await?;

    let ctx = discover::Context {
        dose_cards: dose_cards
            .into_iter()
            .map(|(id, story_id, concepts)| discover::DoseCard { id, story_id, concepts })
            .collect(),
        read_cards: read_cards.into_iter().collect::<HashMap<_, _>>(),
        relations,
        interests: interests.into_iter().map(|(k, n)| (k, n.max(0) as u32)).collect(),
        related_budget: state
            .config
            .discover_related_cap
            .saturating_sub(related_served as usize),
    };
    let candidates: Vec<discover::Candidate> = candidates
        .into_iter()
        .map(
            |(id, category, important, published_at, story_id, follows_up_card_id, concepts)| discover::Candidate {
                id,
                category,
                important,
                published_at,
                story_id,
                follows_up_card_id,
                concepts,
            },
        )
        .collect();

    let picks = discover::rank(&ctx, &candidates, limit);
    for pick in &picks {
        sqlx::query(
            "insert into discover_impression (user_id, dose_date, card_id, reason, related_to_card_id)
             values ($1, $2, $3, $4, $5) on conflict do nothing",
        )
        .bind(user.id)
        .bind(today)
        .bind(pick.card_id)
        .bind(pick.reason.as_str())
        .bind(pick.related_to)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let ids: Vec<Uuid> = picks.iter().map(|p| p.card_id).collect();
    let mut views = load_card_views(&state.pool, &ids).await?;
    let items: Vec<DiscoverItem> = picks
        .into_iter()
        .filter_map(|p| {
            views.remove(&p.card_id).map(|card| DiscoverItem {
                reason: p.reason,
                related_to_card_id: p.related_to,
                card,
            })
        })
        .collect();
    let has_more = candidates.len() > items.len();
    let exhausted = items.is_empty();
    Ok(Json(DiscoverResponse {
        items,
        has_more,
        exhausted,
    }))
}
