//! Authentication and roles.
//!
//! The architecture calls for OIDC (ADR-012, vendor undecided). Until that lands, the API
//! only accepts development tokens of the form `Bearer dev:<role>:<name>`, and only when
//! `SHIRORI_DEV_AUTH=true`. With dev auth off, every authenticated route returns 401.

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{error::AppError, state::AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Reader,
    Editor,
    Admin,
}

impl Role {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "reader" => Some(Self::Reader),
            "editor" => Some(Self::Editor),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::Editor => "editor",
            Self::Admin => "admin",
        }
    }

    pub fn can_edit(self) -> bool {
        matches!(self, Self::Editor | Self::Admin)
    }
}

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: Role,
    pub timezone: String,
    pub personalisation_consent: bool,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    role: String,
    timezone: String,
    personalisation_consent: bool,
}

/// Parse `dev:<role>:<name>`. Names are limited to `[a-z0-9_-]{1,40}`.
pub fn parse_dev_token(token: &str) -> Option<(Role, &str)> {
    let rest = token.strip_prefix("dev:")?;
    let (role, name) = rest.split_once(':')?;
    let valid_name = !name.is_empty()
        && name.len() <= 40
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    valid_name.then_some(())?;
    Some((Role::parse(role)?, name))
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<Option<CurrentUser>, AppError> {
    let Some(header) = parts.headers.get(axum::http::header::AUTHORIZATION) else {
        return Ok(None);
    };
    let token = header
        .to_str()
        .ok()
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    if !state.config.dev_auth {
        // OIDC verification is not implemented yet; refuse rather than guess.
        return Err(AppError::Unauthorized);
    }
    let (role, name) = parse_dev_token(token).ok_or(AppError::Unauthorized)?;
    let row: UserRow = sqlx::query_as(
        "insert into app_user (subject, role) values ($1, $2)
         on conflict (subject) do update set role = excluded.role
         returning id, role, timezone, personalisation_consent",
    )
    .bind(format!("dev:{name}"))
    .bind(role.as_str())
    .fetch_one(&state.pool)
    .await?;
    Ok(Some(CurrentUser {
        id: row.id,
        role: Role::parse(&row.role).unwrap_or(Role::Reader),
        timezone: row.timezone,
        personalisation_consent: row.personalisation_consent,
    }))
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        authenticate(parts, state).await?.ok_or(AppError::Unauthorized)
    }
}

/// The caller if a valid token was sent, otherwise `None` (for public endpoints).
pub struct MaybeUser(pub Option<CurrentUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        Ok(Self(authenticate(parts, state).await?))
    }
}

/// An authenticated user with the editor or admin role.
pub struct Editor(pub CurrentUser);

impl FromRequestParts<AppState> for Editor {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if !user.role.can_edit() {
            return Err(AppError::Forbidden("editor or admin role required".into()));
        }
        Ok(Self(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_dev_tokens() {
        assert_eq!(parse_dev_token("dev:reader:alice"), Some((Role::Reader, "alice")));
        assert_eq!(parse_dev_token("dev:editor:sam-1"), Some((Role::Editor, "sam-1")));
    }

    #[test]
    fn rejects_bad_dev_tokens() {
        for bad in [
            "alice",
            "dev:reader:",
            "dev:root:alice",
            "dev:reader:Alice",
            "dev:reader:a b",
            "x:reader:a",
        ] {
            assert_eq!(parse_dev_token(bad), None, "{bad}");
        }
    }
}
