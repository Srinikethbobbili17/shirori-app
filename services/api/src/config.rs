use anyhow::{Context, bail};

/// Runtime configuration, read from the environment (see `.env.example`).
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    /// Accept `Authorization: Bearer dev:<role>:<name>` tokens. Local development only.
    pub dev_auth: bool,
    /// Total cards in a daily dose, including the Connection card (5–7).
    pub dose_size: usize,
    /// Maximum related-first (tiers 1–3) Discover cards per reader per day.
    pub discover_related_cap: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let bind = std::env::var("SHIRORI_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
        let dev_auth = matches!(std::env::var("SHIRORI_DEV_AUTH").as_deref(), Ok("1" | "true" | "yes"));
        let dose_size = parse_env("SHIRORI_DOSE_SIZE", 6)?;
        if !(5..=7).contains(&dose_size) {
            bail!("SHIRORI_DOSE_SIZE must be between 5 and 7, got {dose_size}");
        }
        let discover_related_cap = parse_env("SHIRORI_DISCOVER_RELATED_CAP", 5)?;
        Ok(Self {
            database_url,
            bind,
            dev_auth,
            dose_size,
            discover_related_cap,
        })
    }

    /// Configuration used by integration tests.
    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            bind: "127.0.0.1:0".into(),
            dev_auth: true,
            dose_size: 6,
            discover_related_cap: 3,
        }
    }
}

fn parse_env(name: &str, default: usize) -> anyhow::Result<usize> {
    match std::env::var(name) {
        Ok(v) => v.parse().with_context(|| format!("{name} must be a number")),
        Err(_) => Ok(default),
    }
}
