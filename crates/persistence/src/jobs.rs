//! Durable job queue in PostgreSQL (ADR-006, §7.3).
//!
//! - **Claiming** uses `FOR UPDATE SKIP LOCKED`, so many workers can poll the same table
//!   without taking the same job.
//! - **Leases:** a claimed job is locked until `locked_until`. If a worker dies, the lease
//!   expires and another worker picks the job up.
//! - **Retries** use exponential backoff. After `max_attempts` the job is `dead`
//!   (dead-lettered) and waits for a human to inspect and retry it.
//! - **Idempotency:** an optional unique `idempotency_key` makes enqueueing safe to repeat.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub payload_version: i32,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub claim_token: Uuid,
}

/// Enqueue a job. Returns `None` if a job with the same idempotency key already exists.
pub async fn enqueue<'e>(
    db: impl PgExecutor<'e>,
    kind: &str,
    payload: &Value,
    idempotency_key: Option<&str>,
    run_after: Option<DateTime<Utc>>,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "insert into job (kind, payload, idempotency_key, run_after)
         values ($1, $2, $3, coalesce($4, now()))
         on conflict (idempotency_key) do nothing
         returning id",
    )
    .bind(kind)
    .bind(payload)
    .bind(idempotency_key)
    .bind(run_after)
    .fetch_optional(db)
    .await
}

/// Claim the next runnable job of one of `kinds`, or `None` if there is nothing to do.
///
/// Runnable means queued and due, or running with an expired lease (its worker died).
pub async fn claim<'e>(
    db: impl PgExecutor<'e>,
    worker_id: &str,
    kinds: &[&str],
    lease_seconds: i64,
) -> sqlx::Result<Option<Job>> {
    let kinds: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
    sqlx::query_as(
        "update job
         set status = 'running', attempt_count = attempt_count + 1,
             locked_until = now() + make_interval(secs => $2), locked_by = $1, claim_token = gen_random_uuid(), updated_at = now()
         where id = (
             select id from job
             where kind = any($3)
               and ((status = 'queued' and run_after <= now())
                    or (status = 'running' and locked_until < now()))
             order by run_after
             limit 1
             for update skip locked)
         returning id, kind, payload, payload_version, attempt_count, max_attempts, claim_token",
    )
    .bind(worker_id)
    .bind(lease_seconds as f64)
    .bind(&kinds)
    .fetch_optional(db)
    .await
}

pub async fn succeed<'e>(db: impl PgExecutor<'e>, job: &Job, result: &Value) -> sqlx::Result<bool> {
    let updated = sqlx::query(
        "update job set status = 'succeeded', result = $2, locked_until = null, error_code = null,
                        finished_at = now(), updated_at = now()
         where id = $1 and status = 'running' and claim_token = $3 and locked_until > now()",
    )
    .bind(job.id)
    .bind(result)
    .bind(job.claim_token)
    .execute(db)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Delay before attempt `n + 1` after `n` failed attempts: 30 s, 1 min, 2 min, … capped at 1 h.
pub fn backoff_seconds(failed_attempts: i32) -> i64 {
    let exp = failed_attempts.clamp(1, 20) - 1;
    (30_i64 << exp).min(3600)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailOutcome {
    Retrying,
    Dead,
    LostLease,
}

/// Record a failure. Retryable failures are rescheduled with backoff until `max_attempts`;
/// permanent ones (e.g. a robots.txt block) go straight to `dead`.
pub async fn fail<'e>(
    db: impl PgExecutor<'e>,
    job: &Job,
    error_code: &str,
    message: &str,
    retryable: bool,
) -> sqlx::Result<FailOutcome> {
    let outcome = if retryable && job.attempt_count < job.max_attempts {
        FailOutcome::Retrying
    } else {
        FailOutcome::Dead
    };
    let message: String = message.chars().take(2000).collect();
    let updated = sqlx::query(
        "update job
         set status = $2, error_code = $3, last_error = $4, locked_until = null, updated_at = now(),
             run_after = case when $2 = 'queued' then now() + make_interval(secs => $5) else run_after end,
             finished_at = case when $2 = 'dead' then now() else null end
         where id = $1 and status = 'running' and claim_token = $6 and locked_until > now()",
    )
    .bind(job.id)
    .bind(match outcome {
        FailOutcome::Retrying => "queued",
        FailOutcome::Dead => "dead",
        FailOutcome::LostLease => unreachable!(),
    })
    .bind(error_code)
    .bind(message)
    .bind(backoff_seconds(job.attempt_count) as f64)
    .bind(job.claim_token)
    .execute(db)
    .await?;
    Ok(if updated.rows_affected() == 1 {
        outcome
    } else {
        FailOutcome::LostLease
    })
}

/// Put a dead job back in the queue with a fresh attempt budget (an editor's "retry").
pub async fn retry_dead<'e>(db: impl PgExecutor<'e>, id: Uuid) -> sqlx::Result<bool> {
    let n = sqlx::query(
        "update job set status = 'queued', attempt_count = 0, run_after = now(), finished_at = null,
                        updated_at = now()
         where id = $1 and status = 'dead'",
    )
    .bind(id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_seconds(1), 30);
        assert_eq!(backoff_seconds(2), 60);
        assert_eq!(backoff_seconds(3), 120);
        assert_eq!(backoff_seconds(8), 3600);
        assert_eq!(backoff_seconds(50), 3600);
    }
}
