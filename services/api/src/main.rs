use anyhow::Context;
use shirori_api::{AppState, MIGRATOR, config::Config, router, seed};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: shirori-api [serve | migrate | seed <file.json>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "shirori_api=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("could not connect to PostgreSQL (is the container running?)")?;
    MIGRATOR.run(&pool).await.context("running migrations")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["serve"] => serve(pool, config).await,
        ["migrate"] => {
            tracing::info!("migrations are up to date");
            Ok(())
        }
        ["seed", path] => {
            let report = seed::load_file(&pool, path).await?;
            println!("{report}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve(pool: sqlx::PgPool, config: Config) -> anyhow::Result<()> {
    if config.dev_auth {
        tracing::warn!("SHIRORI_DEV_AUTH is on: dev tokens are accepted. Never enable this in production.");
    }
    let bind = config.bind.clone();
    let app = router(AppState::new(pool, config));
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("Shirori API listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
