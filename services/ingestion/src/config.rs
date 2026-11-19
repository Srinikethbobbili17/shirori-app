use anyhow::Context;

#[derive(Debug, Clone)]
pub struct IngestConfig {
    pub database_url: String,
    /// Allow fetching from private/loopback addresses. Local development and tests only.
    pub allow_private_networks: bool,
    /// Number of concurrent job workers.
    pub concurrency: usize,
    /// How often the scheduler looks for sources that are due.
    pub scheduler_interval_secs: u64,
    pub user_agent: String,
    /// Token matched against robots.txt `User-agent` lines.
    pub robots_token: String,
    pub request_timeout_secs: u64,
    pub max_feed_bytes: usize,
    pub max_items_per_poll: usize,
    /// Max characters of feed summary kept, for sources that allow excerpts.
    pub max_excerpt_chars: usize,
    pub lease_seconds: i64,
}

impl IngestConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let flag = |name: &str| matches!(std::env::var(name).as_deref(), Ok("1" | "true" | "yes"));
        let num = |name: &str, default: u64| -> anyhow::Result<u64> {
            std::env::var(name)
                .map(|v| v.parse::<u64>().with_context(|| format!("{name} must be a number")))
                .unwrap_or(Ok(default))
        };
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            allow_private_networks: flag("SHIRORI_INGEST_ALLOW_PRIVATE"),
            concurrency: num("SHIRORI_INGEST_CONCURRENCY", 2)?.clamp(1, 16) as usize,
            scheduler_interval_secs: num("SHIRORI_INGEST_SCHEDULER_SECS", 30)?.max(5),
            user_agent: std::env::var("SHIRORI_INGEST_USER_AGENT")
                .unwrap_or_else(|_| "ShiroriBot/0.1 (+https://shirori.app/bot)".into()),
            robots_token: "shiroribot".into(),
            request_timeout_secs: num("SHIRORI_INGEST_TIMEOUT_SECS", 20)?,
            max_feed_bytes: num("SHIRORI_INGEST_MAX_BYTES", 5 * 1024 * 1024)? as usize,
            max_items_per_poll: num("SHIRORI_INGEST_MAX_ITEMS", 200)? as usize,
            max_excerpt_chars: 600,
            lease_seconds: 120,
        })
    }

    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            allow_private_networks: true,
            concurrency: 1,
            scheduler_interval_secs: 5,
            user_agent: "ShiroriBot/0.1 (test)".into(),
            robots_token: "shiroribot".into(),
            request_timeout_secs: 5,
            max_feed_bytes: 64 * 1024,
            max_items_per_poll: 200,
            max_excerpt_chars: 600,
            lease_seconds: 60,
        }
    }
}
