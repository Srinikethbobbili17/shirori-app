# Backend review — 16 November 2026

## Repair update — 17 November 2026

The four confirmed defects have been repaired. The original review below is a historical baseline.

- Connection publication now checks that the approved relation matches concepts on both supporting cards, in either visual order. Existing published connections are not automatically withdrawn.
- Every job claim receives a fresh UUID token. Success and failure updates require that token, running status, and an unexpired lease. Protection survives manual retry resetting attempt counts. This protects queue status; it does not provide exactly-once ingestion side effects or lease renewal.
- Export includes full reader-table records, Discover impressions, and dose contents with version IDs. All queries remain scoped to the authenticated reader in one statement.
- Framework errors use the JSON error envelope while preserving status and headers such as Allow. Existing application errors pass through.

Validation: **79 tests passed** (37 unit, 42 integration), formatting passed, and Clippy passed with warnings denied. Regression coverage includes unrelated/reversed concept relationships, the seed catalogue, expired/reclaimed claims, stale failures, manual retries, export isolation, and malformed paths/queries/routes. Fixtures that previously used unrelated concepts were corrected.

Migration: `0003_job_claim_token.sql` adds the ownership token. Stop old ingestion workers before upgrading; old binaries do not enforce tokens. Updated executables apply migrations on startup. Resume only updated workers. Existing tokenless claims can be reclaimed after lease expiry. Migration was verified in isolated test databases; the application database was not explicitly migrated during this repair. PostgreSQL was started for testing and left running.

Production OIDC remains a separate planned task; no identity provider was selected or configured.

## Original result and scope

All 71 pre-existing tests passed. Six additional audit probes passed, for 77 tests total:

| Area | Unit tests | Integration tests |
| --- | ---: | ---: |
| API | 15 | 26 |
| Ingestion | 21 | 14 |
| Shared persistence | 1 | Tested through ingestion |

`cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.

Tests use real local PostgreSQL, a fresh database per integration test, the Axum router in process, and a local fake HTTP publisher for ingestion. The first attempts encountered sandbox networking restrictions and then a stopped database. After permission to run outside the sandbox and start the existing `shirori_postgres_1` container, the complete suite passed. The container was left running. No application behavior was changed; only tests and this report were added.

Four new tests are explicitly named `audit_gap_*`: they characterize confirmed defects/omissions, not desired behavior. Update their assertions when fixing those issues. The other two verify simultaneous daily requests and rejection of development tokens when development authentication is disabled.

This review is not exhaustive proof of correctness. Production deployment, live publisher behavior, load capacity, comprehensive race/security testing, backup restoration, and OIDC integration were not tested. API integration tests do not exercise an external reverse proxy or real mobile/web client.

## Structure

- `services/api`: Rust/Axum HTTP service. Handles readers, editorial actions, feed selection, account data, and saves.
- `services/ingestion`: separate Rust/Tokio process. Schedules and executes feed polling.
- `crates/persistence`: shared SQL migrations and durable job queue operations.
- `database/migrations`: two migrations define content, reader, editorial audit, source, ingestion, and job tables.
- PostgreSQL holds both application data and the job queue. There is no separate queue server in this implementation.
- Both executables load environment configuration, connect to PostgreSQL, and apply migrations on startup.

## Content journey, step by step

1. **Register a publisher feed.** An editor supplies its URL, allowed host, publication type, licence, rights status, and excerpt policy. Enabling requires `cleared` or `link_only`. These are editor-supplied decisions; the backend does not independently determine rights.
2. **Schedule a poll.** The worker checks due sources every 30 seconds by default. It puts a `poll_source` job into PostgreSQL, avoiding another queued/running poll for that source. Editors can also request polling manually. Failed sources receive increasing scheduling delays.
3. **Claim work.** A worker claims a job using PostgreSQL row locks and a lease. Defaults are two workers and a 120-second lease. Another worker can reclaim an expired lease. Temporary failures back off; permanent failures or exhausted retries mark jobs dead. Editors can retry dead jobs.
4. **Fetch safely.** The fetcher checks allowed hosts and resolved addresses, rejects private addresses in normal mode, validates redirects, checks robots.txt, and applies request/body limits. ETag and Last-Modified avoid downloading unchanged feeds. Tests cover representative network restrictions and a fake publisher; they are not a complete network-security audit.
5. **Parse and deduplicate.** RSS/Atom entries become normalized metadata. URLs lose tracking parameters; DOI information supports duplicate checks across feeds and existing works. Excerpts are stored only when permitted. Items enter a review queue as new or duplicate. Changes to previously promoted items are flagged.
6. **Promote a useful item.** An editor chooses an item and promotes it into `source_work`, the citation record for a paper, article, book, or reference. Rights are checked again. Promotion does not create a reader-facing card. Editors can also create source works manually.
7. **Write a card.** An editor supplies a headline, summary, optional explanation, claim type, evidence label, source date, limitations where required, category, field, and concepts. A card has a stable identity and separate version records. New drafts do not replace the currently published version.
8. **Create a connection.** An editor proposes and approves a relationship between concepts, links two cards from different fields, and writes the bridge sentence and Connection-card content. There is no implemented AI drafting or connection-suggestion worker.
9. **Publish.** Publication checks include allowed source rights, latest revision, limitations for single-study/contested labels, and restrictions on religious-source claims. Connection cards require published supporting cards, different fields, an approved relationship, and a bridge sentence no longer than 300 characters. These are structural checks: they do not establish that a claim is true, a label is justified, or a connection is intellectually useful. The same editor can write and publish; independent second-person review is not enforced.
10. **Maintain content.** Editors can publish new revisions. Withdrawal hides old content and also withdraws dependent Connection cards. Saved entries show withdrawal reasons and available follow-ups. A feed change creates a review flag, not an automatic correction. Feed polling does not detect every possible change to a full article or book.

## Reader journey, step by step

1. **Authenticate.** Currently only `Bearer dev:<role>:<name>` works, with development authentication explicitly enabled. First use creates the user. The role is supplied by the development token. With development auth disabled, authenticated endpoints return 401 because production OIDC is not implemented.
2. **Determine today.** The reader's time zone determines the date. A date override is available only in development mode.
3. **Find or assemble a dose.** If today's dose exists, its card selection is reused. Otherwise the API locks feed creation for this reader and selects published, unread cards. The concurrency probe verifies two simultaneous requests return the same dose.
4. **Choose the ending first.** It chooses an unread Connection card whose two supporting cards are also unread and published, then fills other slots across categories. Important/newer cards receive preference. Default dose size is six including the Connection card; configuration allows five to seven. This is rule-based selection, not an AI recommender.
5. **Handle scarcity.** If no eligible connection exists, the dose ships without one. If too few cards remain, it can be short or empty. Warnings go to logs. Today's selection stays fixed; publishing more cards later does not refill an already-created empty dose. Rendering uses current approved versions, so stable selection does not mean text can never be corrected.
6. **Record reading.** A `card_viewed` event records a card in the no-repeat ledger. Completing a dose marks every card in it read, even without separate view events. Analytics events are stored only with consent; the functional reading ledger still operates without consent.
7. **Unlock Discover.** Only completing today's dose unlocks it. Priority is follow-ups/corrections, the same story, connected concepts, then general category-diverse content. Consent permits saves/likes to influence general selection. The related-card budget caps priority placement: remaining related cards can still appear as general results.
8. **Prevent repetition.** Discover records cards handed out today to avoid repeats while paging. Long-term exclusion depends on read events; a served-but-unviewed Discover card may return another day. Fetching a page alone does not prove it was read.
9. **Save and manage account.** Saves preserve access separately from recommendations and surface revisions/withdrawals. Preferences, export, and deletion exist. Deletion cascades through reader tables; editorial audit records are retained with the actor reference cleared.

## Confirmed gaps

| Priority | Finding | Evidence and next direction |
| --- | --- | --- |
| Before production readers | Production authentication missing | `auth.rs`; disabled dev mode refuses authenticated requests. Choose and implement identity verification. |
| Before trusting connection automation | Connection gate accepts unrelated approved concepts | `audit_gap_connection_accepts_unrelated_approved_relation`. Gate checks approval status but not whether relation endpoints match concepts on the supporting cards. Add that validation. |
| Before increasing worker concurrency/long jobs | An expired worker can finish a reclaimed job | `audit_gap_stale_worker_can_finish_reclaimed_job`. Completion updates use only job ID. Use an ownership/attempt token in completion/failure updates; consider lease renewal and idempotent side effects. |
| Before claiming complete export | Reader export omits Discover impressions and dose contents | `audit_gap_export_omits_discover_history`, plus the export query in `users.rs`. Include the omitted reader data and relevant stored fields. |
| Before client integration relies on uniform errors | Invalid UUID paths return non-JSON errors | `audit_gap_invalid_path_is_not_json_error`. Custom JSON-body errors are wrapped, but standard path/query rejections and routing errors need consistent handling. |

## Editorial planning observations

The catalogue is shared. Editors do not need to write six new cards separately for every user. The same published card can serve many readers, provided each reader has not read it before.

The difficult constraint is reader-specific eligibility: a Connection card becomes unusable for a reader after either supporting card is read. Discover and daily-dose fillers can consume cards needed by a future connection. Thus a large global connection count does not guarantee tomorrow's connection for an active reader. The inventory endpoint currently reports global category/state counts and globally published connections, not remaining days per reader or cohort.

An illustrative budget at six cards per day is five ordinary cards plus one Connection card per active reader-day. Thirty days would consume 150 ordinary cards and 30 Connection cards for one continuously active reader, before extra Discover reading, assuming every day includes a connection. This is a content-consumption example, not a measured production requirement: readers share the catalogue and can start with a backlog.

Useful decisions for the owner to make:

- Which steps should remain human decisions: source selection, evidence judgement, wording, connection approval, final publication?
- Should future assistance draft cards and connections for review, or primarily filter and prioritize the source queue?
- Should connections be planned as small content bundles whose supporting cards are reserved from Discover?
- Is a shorter dose an acceptable fallback, and how important is a guaranteed daily connection?
- How much evergreen backlog and news should the catalogue contain?
- What should be measured first: time per accepted card, rejection rate, time per approved connection, correction rate, and remaining eligible inventory?

Possible improvements such as AI drafting, batch review, or per-reader inventory forecasts are proposals only. They were not implemented in this review. More automated ingestion alone may increase the review queue without reducing editorial effort.

## Reproduce

With local PostgreSQL available at the configured `DATABASE_URL`:

```bash
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Tests added in `services/api/tests/api.rs` and `services/ingestion/tests/ingest.rs`. No production fixes were applied so the owner can plan changes from the observed baseline.
