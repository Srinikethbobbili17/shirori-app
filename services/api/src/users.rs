//! Reader profile, preferences, data export and account deletion (FR-15, QR-08).

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    auth::CurrentUser,
    error::{ApiJson, AppError, AppResult},
    state::{AppState, DateOverride},
};

pub async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(date): Query<DateOverride>,
) -> AppResult<Json<Value>> {
    let today = state.reader_today(&user, &date).await?;
    Ok(Json(json!({
        "user_id": user.id,
        "role": user.role.as_str(),
        "timezone": user.timezone,
        "personalisation_consent": user.personalisation_consent,
        "today": today,
    })))
}

#[derive(Debug, Deserialize)]
pub struct PreferencesIn {
    pub timezone: Option<String>,
    pub personalisation_consent: Option<bool>,
}

pub async fn put_preferences(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(prefs): ApiJson<PreferencesIn>,
) -> AppResult<Json<Value>> {
    if let Some(tz) = &prefs.timezone {
        let valid: bool = sqlx::query_scalar("select exists (select 1 from pg_timezone_names where name = $1)")
            .bind(tz)
            .fetch_one(&state.pool)
            .await?;
        if !valid {
            return Err(AppError::BadRequest(format!("unknown time zone `{tz}`")));
        }
    }
    let (timezone, consent): (String, bool) = sqlx::query_as(
        "update app_user set timezone = coalesce($2, timezone),
                             personalisation_consent = coalesce($3, personalisation_consent)
         where id = $1 returning timezone, personalisation_consent",
    )
    .bind(user.id)
    .bind(&prefs.timezone)
    .bind(prefs.personalisation_consent)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(
        json!({ "timezone": timezone, "personalisation_consent": consent }),
    ))
}

/// Everything Shirori stores about the reader, as JSON.
pub async fn export(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let data: Value = sqlx::query_scalar(
        "select json_build_object(
            'profile', (select row_to_json(u) from app_user u where u.id = $1),
            'saved_cards', coalesce((select json_agg(s order by s.saved_at, s.card_id)
                                     from saved_card s where s.user_id = $1), '[]'),
            'read_cards', coalesce((select json_agg(r order by r.first_read_at, r.card_id)
                                    from read_card r where r.user_id = $1), '[]'),
            'daily_doses', coalesce((select json_agg(d order by d.dose_date)
                                     from daily_dose d where d.user_id = $1), '[]'),
            'daily_dose_items', coalesce((select json_agg(i order by i.dose_id, i.position)
                                     from daily_dose_item i join daily_dose d on d.id = i.dose_id
                                     where d.user_id = $1), '[]'),
            'discover_impressions', coalesce((select json_agg(di order by di.dose_date, di.served_at, di.card_id)
                                     from discover_impression di where di.user_id = $1), '[]'),
            'events', coalesce((select json_agg(e order by e.occurred_at, e.id)
                                from user_event e where e.user_id = $1), '[]')
        )",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(data))
}

/// Delete the account and everything tied to it (cascades through all reader tables).
pub async fn delete_me(State(state): State<AppState>, user: CurrentUser) -> AppResult<StatusCode> {
    sqlx::query("delete from app_user where id = $1")
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
