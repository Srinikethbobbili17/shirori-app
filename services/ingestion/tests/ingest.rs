//! End-to-end ingestion tests: a local HTTP server plays the publisher, and the worker polls it
//! into a fresh PostgreSQL database (`#[sqlx::test]`, needs `DATABASE_URL`).

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::Value;
use shirori_ingestion::{
    config::IngestConfig,
    net::{FetchError, Fetcher},
    poll::{PollError, PollReport, poll_source},
    worker::{self, JobOutcome},
};
use shirori_persistence::jobs::{self, FailOutcome};
use sqlx::PgPool;
use uuid::Uuid;

// ───────────────────────── Fake publisher ─────────────────────────

#[derive(Clone)]
struct Publisher {
    /// Feed body served at `/feed.xml` (and `/mirror.xml`). Its ETag changes with every edit.
    feed: String,
    robots: Option<String>,
    /// Status to return instead of the feed, e.g. 500.
    feed_status: Option<u16>,
    feed_requests: u32,
}

#[derive(Clone)]
struct Server {
    state: Arc<Mutex<Publisher>>,
    base: String,
}

impl Server {
    async fn start(feed: String) -> Self {
        let state = Arc::new(Mutex::new(Publisher {
            feed,
            robots: None,
            feed_status: None,
            feed_requests: 0,
        }));
        let app = Router::new()
            .route("/robots.txt", get(robots))
            .route("/feed.xml", get(serve_feed))
            .route("/mirror.xml", get(serve_feed))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { state, base }
    }

    fn set(&self, f: impl FnOnce(&mut Publisher)) {
        f(&mut self.state.lock().unwrap());
    }

    fn feed_requests(&self) -> u32 {
        self.state.lock().unwrap().feed_requests
    }
}

async fn robots(State(s): State<Arc<Mutex<Publisher>>>) -> Response {
    match s.lock().unwrap().robots.clone() {
        Some(body) => body.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn serve_feed(State(s): State<Arc<Mutex<Publisher>>>, headers: HeaderMap) -> Response {
    let mut p = s.lock().unwrap();
    p.feed_requests += 1;
    if let Some(status) = p.feed_status {
        return StatusCode::from_u16(status).unwrap().into_response();
    }
    let etag = format!(
        "\"{:x}\"",
        p.feed.len() * 31 + p.feed.bytes().map(usize::from).sum::<usize>()
    );
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    (
        [
            (header::CONTENT_TYPE, "application/rss+xml"),
            (header::ETAG, etag.as_str()),
        ],
        p.feed.clone(),
    )
        .into_response()
}

struct Item<'a> {
    guid: &'a str,
    title: &'a str,
    link: &'a str,
    description: &'a str,
}

fn rss(items: &[Item]) -> String {
    let items: String = items
        .iter()
        .map(|i| {
            format!(
                "<item><guid>{}</guid><title>{}</title><link>{}</link><description>{}</description>\
                 <pubDate>Mon, 09 Nov 2026 10:00:00 GMT</pubDate></item>",
                i.guid, i.title, i.link, i.description
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Test journal</title>\
         <link>https://journal.example/</link><description>t</description>{items}</channel></rss>"
    )
}

fn two_items() -> String {
    rss(&[
        Item {
            guid: "a-1",
            title: "Octopuses &lt;b&gt;dream&lt;/b&gt;",
            link: "https://journal.example/articles/octopus?utm_source=rss",
            description: "Sleep stages in cephalopods. doi:10.1234/OCTO.2026.1",
        },
        Item {
            guid: "a-2",
            title: "Glaciers and sea level",
            link: "https://journal.example/articles/glacier",
            description: "A summary that must not be kept for link-only sources.",
        },
    ])
}

// ───────────────────────── Helpers ─────────────────────────

fn cfg() -> IngestConfig {
    IngestConfig::for_tests()
}

async fn add_source(pool: &PgPool, server: &Server, path: &str, rights: &str, store_excerpts: bool) -> Uuid {
    sqlx::query_scalar(
        "insert into source (name, feed_url, allowed_host, licence, rights_status, store_excerpts, enabled)
         values ($1, $2, '127.0.0.1', 'test', $3, $4, $5) returning id",
    )
    .bind(format!("source {path}"))
    .bind(format!("{}{path}", server.base))
    .bind(rights)
    .bind(store_excerpts)
    .bind(rights == "cleared" || rights == "link_only")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn poll(pool: &PgPool, source: Uuid) -> Result<PollReport, PollError> {
    let cfg = cfg();
    poll_source(pool, &Fetcher::new(&cfg), &cfg, source).await
}

fn fetched_counts(report: &PollReport) -> (u32, u32, u32, u32) {
    match report {
        PollReport::Fetched { counts, .. } => (counts.new, counts.duplicates, counts.updated, counts.unchanged),
        other => panic!("expected a fetch, got {other:?}"),
    }
}

/// Queue a poll job for `source` and run the queue until it is empty right now.
async fn run_job(pool: &PgPool, source: Uuid) -> Uuid {
    let id = jobs::enqueue(
        pool,
        worker::POLL_SOURCE,
        &serde_json::json!({ "source_id": source }),
        None,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    let cfg = cfg();
    worker::run_one(pool, &Fetcher::new(&cfg), &cfg, "test").await.unwrap();
    id
}

async fn job_status(pool: &PgPool, id: Uuid) -> (String, i32, Option<String>) {
    sqlx::query_as("select status, attempt_count, error_code from job where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Make a backed-off job runnable now (instead of waiting for its backoff).
async fn make_due(pool: &PgPool, id: Uuid) {
    sqlx::query("update job set run_after = now() where id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

// ───────────────────────── Tests ─────────────────────────

#[sqlx::test(migrations = "../../database/migrations")]
async fn polls_a_feed_into_the_review_queue(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", true).await;

    let report = poll(&pool, source).await.unwrap();
    assert_eq!(fetched_counts(&report), (2, 0, 0, 0));

    type Row = (String, String, Option<String>, Option<String>, String);
    let rows: Vec<Row> =
        sqlx::query_as("select external_id, title, canonical_url, doi, status from ingested_item order by external_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    // HTML is stripped, tracking parameters dropped, the DOI found and lower-cased.
    assert_eq!(rows[0].1, "Octopuses dream");
    assert_eq!(rows[0].2.as_deref(), Some("https://journal.example/articles/octopus"));
    assert_eq!(rows[0].3.as_deref(), Some("10.1234/octo.2026.1"));
    assert!(rows.iter().all(|r| r.4 == "new"));

    let (failures, last_error): (i32, Option<String>) =
        sqlx::query_as("select consecutive_failures, last_error from source where id = $1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((failures, last_error), (0, None));
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn second_poll_uses_conditional_get(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    poll(&pool, source).await.unwrap();
    assert_eq!(poll(&pool, source).await.unwrap(), PollReport::NotModified);
    assert_eq!(server.feed_requests(), 2);
    let n: i64 = sqlx::query_scalar("select count(*) from ingested_item")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn excerpts_are_kept_only_when_the_source_allows_it(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let keeps = add_source(&pool, &server, "/feed.xml", "cleared", true).await;
    let link_only = add_source(&pool, &server, "/mirror.xml", "link_only", false).await;
    poll(&pool, keeps).await.unwrap();
    poll(&pool, link_only).await.unwrap();

    let excerpts: Vec<(Uuid, Option<String>)> = sqlx::query_as("select source_id, excerpt from ingested_item")
        .fetch_all(&pool)
        .await
        .unwrap();
    for (src, excerpt) in excerpts {
        if src == keeps {
            assert!(excerpt.is_some());
        } else {
            assert_eq!(excerpt, None, "link-only source must not store publisher text");
        }
    }
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn same_article_from_two_sources_is_a_duplicate(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let first = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    let second = add_source(&pool, &server, "/mirror.xml", "cleared", false).await;
    assert_eq!(fetched_counts(&poll(&pool, first).await.unwrap()), (2, 0, 0, 0));
    assert_eq!(fetched_counts(&poll(&pool, second).await.unwrap()), (0, 2, 0, 0));

    let dups: i64 = sqlx::query_scalar(
        "select count(*) from ingested_item d join ingested_item o on o.id = d.duplicate_of
         where d.source_id = $1 and o.source_id = $2 and d.status = 'duplicate'",
    )
    .bind(second)
    .bind(first)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dups, 2);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn article_already_in_the_catalogue_is_a_duplicate(pool: PgPool) {
    sqlx::query(
        "insert into source_work (work_type, title, canonical_url, licence, rights_status)
         values ('article', 'Glaciers', 'https://journal.example/articles/glacier', 'test', 'cleared')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let server = Server::start(two_items()).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    assert_eq!(fetched_counts(&poll(&pool, source).await.unwrap()), (1, 1, 0, 0));
    let linked: i64 = sqlx::query_scalar("select count(*) from ingested_item where existing_work_id is not null")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(linked, 1);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn change_after_promotion_is_flagged_for_editors(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    poll(&pool, source).await.unwrap();
    sqlx::query("update ingested_item set status = 'promoted' where external_id = 'a-2'")
        .execute(&pool)
        .await
        .unwrap();

    server.set(|p| {
        p.feed = rss(&[
            Item {
                guid: "a-1",
                title: "Octopuses &lt;b&gt;dream&lt;/b&gt;",
                link: "https://journal.example/articles/octopus?utm_source=rss",
                description: "Sleep stages in cephalopods. doi:10.1234/OCTO.2026.1",
            },
            Item {
                guid: "a-2",
                title: "Glaciers and sea level (corrected)",
                link: "https://journal.example/articles/glacier",
                description: "Correction: the rate was overstated.",
            },
        ])
    });
    assert_eq!(fetched_counts(&poll(&pool, source).await.unwrap()), (0, 0, 1, 1));

    let (title, flagged, revisions): (String, bool, i32) = sqlx::query_as(
        "select title, changed_since_promotion, revision_count from ingested_item where external_id = 'a-2'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (title.as_str(), flagged, revisions),
        ("Glaciers and sea level (corrected)", true, 2)
    );
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn sources_without_cleared_rights_are_never_fetched(pool: PgPool) {
    let server = Server::start(two_items()).await;
    // The database refuses to switch on a source with unknown rights.
    let refused = sqlx::query(
        "insert into source (name, feed_url, allowed_host, licence, rights_status, enabled)
         values ('x', 'http://127.0.0.1/x', '127.0.0.1', 'test', 'unknown', true)",
    )
    .execute(&pool)
    .await;
    assert!(refused.is_err());

    let restricted = add_source(&pool, &server, "/feed.xml", "restricted", false).await;
    assert!(matches!(
        poll(&pool, restricted).await.unwrap(),
        PollReport::Skipped { .. }
    ));
    assert_eq!(server.feed_requests(), 0);
    // …and the scheduler never queues it.
    assert_eq!(worker::enqueue_due_polls(&pool).await.unwrap(), 0);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn robots_disallow_is_respected_and_not_retried(pool: PgPool) {
    let server = Server::start(two_items()).await;
    server.set(|p| p.robots = Some("User-agent: *\nAllow: /\n\nUser-agent: ShiroriBot\nDisallow: /feed".into()));
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;

    let job = run_job(&pool, source).await;
    assert_eq!(
        job_status(&pool, job).await,
        ("dead".into(), 1, Some("robots_disallowed".into()))
    );
    assert_eq!(server.feed_requests(), 0, "the feed itself must not be fetched");
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn server_errors_retry_then_dead_letter_then_editor_retry(pool: PgPool) {
    let server = Server::start(two_items()).await;
    server.set(|p| p.feed_status = Some(503));
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    sqlx::query("alter table job alter column max_attempts set default 2")
        .execute(&pool)
        .await
        .unwrap();

    let job = run_job(&pool, source).await;
    assert_eq!(
        job_status(&pool, job).await,
        ("queued".into(), 1, Some("http_503".into()))
    );
    let backed_off: bool =
        sqlx::query_scalar("select run_after > now() + interval '20 seconds' from job where id = $1")
            .bind(job)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(backed_off, "a retry waits for its backoff");

    make_due(&pool, job).await;
    let cfg = cfg();
    let outcome = worker::run_one(&pool, &Fetcher::new(&cfg), &cfg, "test").await.unwrap();
    assert_eq!(outcome, Some(JobOutcome::Failed(FailOutcome::Dead)));
    assert_eq!(job_status(&pool, job).await.0, "dead");

    let failures: i32 = sqlx::query_scalar("select consecutive_failures from source where id = $1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(failures, 2);

    // The publisher recovers; an editor retries the dead job.
    server.set(|p| p.feed_status = None);
    assert!(jobs::retry_dead(&pool, job).await.unwrap());
    let outcome = worker::run_one(&pool, &Fetcher::new(&cfg), &cfg, "test").await.unwrap();
    assert_eq!(outcome, Some(JobOutcome::Succeeded));
    let result: Value = sqlx::query_scalar("select result from job where id = $1")
        .bind(job)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(result["outcome"], "fetched");
    assert_eq!(result["counts"]["new"], 2);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn expired_lease_lets_another_worker_take_the_job(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    let id = jobs::enqueue(
        &pool,
        worker::POLL_SOURCE,
        &serde_json::json!({ "source_id": source }),
        None,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    // Worker A claims it, then "crashes".
    let claimed = jobs::claim(&pool, "worker-a", &[worker::POLL_SOURCE], 60)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, id);
    assert!(
        jobs::claim(&pool, "worker-b", &[worker::POLL_SOURCE], 60)
            .await
            .unwrap()
            .is_none()
    );

    sqlx::query("update job set locked_until = now() - interval '1 second' where id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let cfg = cfg();
    let outcome = worker::run_one(&pool, &Fetcher::new(&cfg), &cfg, "worker-b")
        .await
        .unwrap();
    assert_eq!(outcome, Some(JobOutcome::Succeeded));
    let (status, attempts, _) = job_status(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("succeeded", 2));
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn scheduler_queues_each_due_source_once(pool: PgPool) {
    let server = Server::start(two_items()).await;
    add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    add_source(&pool, &server, "/mirror.xml", "link_only", false).await;
    assert_eq!(worker::enqueue_due_polls(&pool).await.unwrap(), 2);
    assert_eq!(worker::enqueue_due_polls(&pool).await.unwrap(), 0, "already queued");

    let cfg = cfg();
    assert_eq!(
        worker::drain(&pool, &Fetcher::new(&cfg), &cfg, "test").await.unwrap(),
        2
    );
    assert_eq!(worker::enqueue_due_polls(&pool).await.unwrap(), 0, "not due again yet");
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn fetcher_blocks_private_addresses_and_foreign_hosts(pool: PgPool) {
    let server = Server::start(two_items()).await;
    let url = url::Url::parse(&format!("{}/feed.xml", server.base)).unwrap();

    let production = IngestConfig {
        allow_private_networks: false,
        ..cfg()
    };
    let err = Fetcher::new(&production)
        .fetch(&url, "127.0.0.1", None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Blocked(_)), "{err:?}");

    let err = Fetcher::new(&cfg())
        .fetch(&url, "journal.example", None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Blocked(_)), "{err:?}");

    // Through the worker, a blocked fetch is a permanent failure.
    let source: Uuid = sqlx::query_scalar(
        "insert into source (name, feed_url, allowed_host, licence, rights_status, enabled)
         values ('wrong host', $1, 'journal.example', 'test', 'cleared', true) returning id",
    )
    .bind(url.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    let job = run_job(&pool, source).await;
    assert_eq!(job_status(&pool, job).await, ("dead".into(), 1, Some("blocked".into())));
    assert_eq!(server.feed_requests(), 0);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn oversized_feeds_are_refused(pool: PgPool) {
    let big = rss(&[Item {
        guid: "big",
        title: "Big",
        link: "https://journal.example/big",
        description: &"x".repeat(100 * 1024),
    }]);
    let server = Server::start(big).await;
    let source = add_source(&pool, &server, "/feed.xml", "cleared", false).await;
    let err = poll(&pool, source).await.unwrap_err();
    assert_eq!(err.code(), "too_large");
    let n: i64 = sqlx::query_scalar("select count(*) from ingested_item")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

// Expired claim owners must never modify a newer claim.
#[sqlx::test(migrations = "../../database/migrations")]
async fn stale_worker_cannot_finish_or_fail_reclaimed_job(pool: PgPool) {
    let id = jobs::enqueue(&pool, worker::POLL_SOURCE, &serde_json::json!({}), None, None)
        .await
        .unwrap()
        .unwrap();
    let stale = jobs::claim(&pool, "old-worker", &[worker::POLL_SOURCE], 60)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("update job set locked_until = now() - interval '1 second' where id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let current = jobs::claim(&pool, "new-worker", &[worker::POLL_SOURCE], 60)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.attempt_count, 2);
    assert!(
        !jobs::succeed(&pool, &stale, &serde_json::json!({"stale": true}))
            .await
            .unwrap()
    );
    assert_eq!(
        jobs::fail(&pool, &stale, "late", "late failure", true).await.unwrap(),
        FailOutcome::LostLease
    );
    assert_eq!(job_status(&pool, id).await.0, "running");
    assert_eq!(
        jobs::fail(&pool, &current, "blocked", "permanent failure", false)
            .await
            .unwrap(),
        FailOutcome::Dead
    );
    assert!(jobs::retry_dead(&pool, id).await.unwrap());
    // Attempt numbers reset after manual retry, but ownership tokens never repeat.
    let retried = jobs::claim(&pool, "old-worker", &[worker::POLL_SOURCE], 60)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.attempt_count, stale.attempt_count);
    assert_ne!(retried.claim_token, stale.claim_token);
    assert!(!jobs::succeed(&pool, &stale, &serde_json::json!({})).await.unwrap());
    assert!(
        jobs::succeed(&pool, &retried, &serde_json::json!({"fresh": true}))
            .await
            .unwrap()
    );
    assert_eq!(job_status(&pool, id).await.0, "succeeded");
    assert_eq!(
        jobs::fail(&pool, &retried, "late", "after completion", false)
            .await
            .unwrap(),
        FailOutcome::LostLease
    );
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn expired_claim_cannot_complete_before_reassignment(pool: PgPool) {
    jobs::enqueue(&pool, worker::POLL_SOURCE, &serde_json::json!({}), None, None)
        .await
        .unwrap();
    let job = jobs::claim(&pool, "worker", &[worker::POLL_SOURCE], 60)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("update job set locked_until = now() - interval '1 second' where id = $1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!jobs::succeed(&pool, &job, &serde_json::json!({})).await.unwrap());
    assert_eq!(
        jobs::fail(&pool, &job, "late", "expired", false).await.unwrap(),
        FailOutcome::LostLease
    );
}
