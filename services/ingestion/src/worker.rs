//! Scheduler and job workers.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;
use shirori_persistence::jobs::{self, FailOutcome, Job};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{config::IngestConfig, net::Fetcher, poll::poll_source};

pub const POLL_SOURCE: &str = "poll_source";

/// Enqueue a poll for every enabled, rights-cleared source that is due and has no poll queued.
///
/// "Due" backs off with failures: interval × 2^min(failures, 5) (a simple circuit breaker).
/// A transaction-level advisory lock keeps two schedulers from enqueuing at the same time.
pub async fn enqueue_due_polls(pool: &PgPool) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;
    let got_lock: bool =
        sqlx::query_scalar("select pg_try_advisory_xact_lock(hashtextextended('shirori.scheduler', 0))")
            .fetch_one(&mut *tx)
            .await?;
    if !got_lock {
        return Ok(0);
    }
    let n = sqlx::query(
        "insert into job (kind, payload)
         select 'poll_source', jsonb_build_object('source_id', s.id)
         from source s
         where s.enabled and s.rights_status in ('cleared', 'link_only')
           and (s.last_attempt_at is null
                or s.last_attempt_at + make_interval(mins => s.poll_interval_minutes * (1 << least(s.consecutive_failures, 5)))
                   <= now())
           and not exists (select 1 from job j
                           where j.kind = 'poll_source' and j.payload ->> 'source_id' = s.id::text
                             and j.status in ('queued', 'running'))",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    if n > 0 {
        tracing::info!(jobs = n, "scheduled source polls");
    }
    Ok(n)
}

#[derive(Deserialize)]
struct PollPayload {
    source_id: Uuid,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JobOutcome {
    Succeeded,
    LostLease,
    Failed(FailOutcome),
}

/// Claim and run one job. Returns `None` when the queue has nothing runnable.
pub async fn run_one(
    pool: &PgPool,
    fetcher: &Fetcher,
    cfg: &IngestConfig,
    worker_id: &str,
) -> anyhow::Result<Option<JobOutcome>> {
    let Some(job) = jobs::claim(pool, worker_id, &[POLL_SOURCE], cfg.lease_seconds).await? else {
        return Ok(None);
    };
    Ok(Some(process(pool, fetcher, cfg, &job).await?))
}

async fn process(pool: &PgPool, fetcher: &Fetcher, cfg: &IngestConfig, job: &Job) -> anyhow::Result<JobOutcome> {
    if job.attempt_count > job.max_attempts {
        // The lease expired repeatedly (worker crashes); stop trying.
        let outcome = jobs::fail(pool, job, "lease_expired", "worker lease expired too many times", false).await?;
        return Ok(JobOutcome::Failed(outcome));
    }
    let payload: PollPayload = match serde_json::from_value(job.payload.clone()) {
        Ok(p) => p,
        Err(e) => {
            let outcome = jobs::fail(pool, job, "bad_payload", &e.to_string(), false).await?;
            return Ok(JobOutcome::Failed(outcome));
        }
    };
    match poll_source(pool, fetcher, cfg, payload.source_id).await {
        Ok(report) => {
            tracing::info!(source = %payload.source_id, ?report, "poll finished");
            if jobs::succeed(pool, job, &json!(report)).await? {
                Ok(JobOutcome::Succeeded)
            } else {
                Ok(JobOutcome::LostLease)
            }
        }
        Err(e) => {
            let outcome = jobs::fail(pool, job, &e.code(), &e.to_string(), e.retryable()).await?;
            tracing::warn!(source = %payload.source_id, error = %e, ?outcome, attempt = job.attempt_count, "poll failed");
            Ok(JobOutcome::Failed(outcome))
        }
    }
}

/// Run jobs until the queue has nothing runnable right now. Returns how many ran.
pub async fn drain(pool: &PgPool, fetcher: &Fetcher, cfg: &IngestConfig, worker_id: &str) -> anyhow::Result<usize> {
    let mut n = 0;
    while run_one(pool, fetcher, cfg, worker_id).await?.is_some() {
        n += 1;
    }
    Ok(n)
}

/// Long-running mode: one scheduler loop plus `cfg.concurrency` workers, until Ctrl-C.
pub async fn run_forever(pool: PgPool, cfg: IngestConfig) -> anyhow::Result<()> {
    let fetcher = Fetcher::new(&cfg);
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut tasks = tokio::task::JoinSet::new();

    {
        let pool = pool.clone();
        let mut stop = stop_rx.clone();
        let every = Duration::from_secs(cfg.scheduler_interval_secs);
        tasks.spawn(async move {
            loop {
                if let Err(e) = enqueue_due_polls(&pool).await {
                    tracing::error!(error = %e, "scheduler failed");
                }
                tokio::select! {
                    _ = tokio::time::sleep(every) => {}
                    _ = stop.changed() => break,
                }
            }
        });
    }

    for i in 0..cfg.concurrency {
        let (pool, fetcher, cfg, mut stop) = (pool.clone(), fetcher.clone(), cfg.clone(), stop_rx.clone());
        let worker_id = format!("{}-{}-{i}", hostname(), std::process::id());
        tasks.spawn(async move {
            loop {
                let idle = match run_one(&pool, &fetcher, &cfg, &worker_id).await {
                    Ok(Some(_)) => false,
                    Ok(None) => true,
                    Err(e) => {
                        tracing::error!(error = %e, "worker error");
                        true
                    }
                };
                if *stop.borrow() {
                    break;
                }
                if idle {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                        _ = stop.changed() => break,
                    }
                }
            }
        });
    }

    tracing::info!(workers = cfg.concurrency, "Shirori ingestion running; Ctrl-C to stop");
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("stopping: finishing current jobs");
    stop_tx.send(true).ok();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

fn hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".into())
}
