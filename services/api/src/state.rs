use std::sync::Arc;

use chrono::NaiveDate;
use serde::Deserialize;
use sqlx::PgPool;

use crate::{auth::CurrentUser, config::Config, error::AppResult};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Self {
        Self {
            pool,
            config: Arc::new(config),
        }
    }

    /// The reader's local date, which decides which daily dose they get.
    ///
    /// With dev auth on, `?date=YYYY-MM-DD` overrides it so testers can simulate later days.
    pub async fn reader_today(&self, user: &CurrentUser, date: &DateOverride) -> AppResult<NaiveDate> {
        if self.config.dev_auth
            && let Some(d) = date.date
        {
            return Ok(d);
        }
        let today: NaiveDate = sqlx::query_scalar("select (now() at time zone $1)::date")
            .bind(&user.timezone)
            .fetch_one(&self.pool)
            .await?;
        Ok(today)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DateOverride {
    pub date: Option<NaiveDate>,
}
