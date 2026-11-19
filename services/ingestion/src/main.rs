use anyhow::Context;
use shirori_ingestion::{config::IngestConfig, net::Fetcher, poll::poll_source, worker};
use shirori_persistence::MIGRATOR;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: shirori-ingestion [run | once | poll <source-id>]
  run              scheduler + workers until Ctrl-C (default)
  once             schedule due sources, run all queued jobs, exit
  poll <source-id> poll one source right now (ignores its schedule)";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "shirori_ingestion=info".into()))
        .init();

    let cfg = IngestConfig::from_env()?;
    if cfg.allow_private_networks {
        tracing::warn!("SHIRORI_INGEST_ALLOW_PRIVATE is on: private addresses can be fetched. Never in production.");
    }
    let pool = PgPoolOptions::new()
        .max_connections(cfg.concurrency as u32 + 2)
        .connect(&cfg.database_url)
        .await
        .context("could not connect to PostgreSQL (is the container running?)")?;
    MIGRATOR.run(&pool).await.context("running migrations")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["run"] => worker::run_forever(pool, cfg).await,
        ["once"] => {
            let fetcher = Fetcher::new(&cfg);
            let scheduled = worker::enqueue_due_polls(&pool).await?;
            let ran = worker::drain(&pool, &fetcher, &cfg, "cli-once").await?;
            println!("scheduled {scheduled} polls, ran {ran} jobs");
            Ok(())
        }
        ["poll", id] => {
            let id = id.parse().context("source id must be a UUID")?;
            let report = poll_source(&pool, &Fetcher::new(&cfg), &cfg, id).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
