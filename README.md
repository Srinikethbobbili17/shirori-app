# Shirori

A daily dose of knowledge instead of doomscrolling: 5–7 source-first cards that end with a
Connection card, then an optional swipe into Discover.

- Architecture: [`docs/architecture/shirori-architecture.md`](docs/architecture/shirori-architecture.md)
- Idea debate: [`IDEA_DEBATE.md`](IDEA_DEBATE.md)

## What's here (build steps 1 and 3)

| Path | What |
|---|---|
| `services/api` | Rust/Axum API (modular monolith) |
| `services/ingestion` | Rust/Tokio worker: polls rights-cleared feeds into the editors' review queue |
| `crates/persistence` | Shared migrations and the durable PostgreSQL job queue |
| `database/migrations` | PostgreSQL schema |
| `database/seed/dev-seed.json` | 25 knowledge cards and 5 Connection cards for testing |
| `infrastructure/docker/compose.yaml` | Local PostgreSQL (port 55432) |

## Run it locally

```bash
cp .env.example .env                                         # once
podman compose -f infrastructure/docker/compose.yaml up -d   # or: docker compose ...
cargo run -p shirori-api -- seed database/seed/dev-seed.json # once; runs migrations too
cargo run -p shirori-api                                     # http://127.0.0.1:8080
```

## Local web Test Lab

With the API running and `SHIRORI_DEV_AUTH=true`, open **http://127.0.0.1:8080/**.
The lightweight UI covers the daily dose, Discover, Saved, reader settings, editorial cards,
and ingestion queues. No frontend installation or build step is needed.
See [`apps/web/README.md`](apps/web/README.md) for the testing walkthrough.

## Try a reader's day

Auth is a placeholder until an OIDC provider is chosen. With `SHIRORI_DEV_AUTH=true`, send
`Authorization: Bearer dev:<reader|editor|admin>:<name>`. Users are created on first use.

```bash
T='Authorization: Bearer dev:reader:alice'
curl -s -H "$T" localhost:8080/v1/feed/daily                       # today's dose (ends with a Connection card)
curl -s -H "$T" localhost:8080/v1/feed/discover                    # 409 until the dose is completed
curl -s -X POST -H "$T" localhost:8080/v1/feed/daily/<dose_id>/complete
curl -s -H "$T" localhost:8080/v1/feed/discover                    # related-first Discover
curl -s -X PUT -H "$T" -H 'content-type: application/json' \
     -d '{"saved_from":"dose"}' localhost:8080/v1/me/saved/<card_id>
curl -s -H "$T" localhost:8080/v1/me/saved
curl -s -H "$T" 'localhost:8080/v1/feed/daily?date=2026-11-07'     # dev only: simulate tomorrow (no repeats)
```

| Endpoint | Purpose |
|---|---|
| `GET /v1/feed/daily` | Today's dose: stable for the day, never contains a read card |
| `POST /v1/feed/daily/{id}/complete` | Finish the dose; its cards become read; unlocks Discover |
| `GET /v1/feed/discover?limit=` | Next Discover page: updates/corrections → same story → connected ideas → general |
| `GET /v1/cards/{id}` · `/sources` · `/connections` | Card, its sources, cards it's connected to |
| `GET /v1/works/{id}` | Source work and the cards drawn from it |
| `POST /v1/events` | `card_viewed` marks a card read (always); analytics stored only with consent |
| `PUT/DELETE /v1/me/saved/{card_id}`, `GET /v1/me/saved` | Saved list |
| `GET /v1/me`, `PUT /v1/me/preferences`, `GET /v1/me/export`, `DELETE /v1/me` | Profile, time zone and consent, export, delete account |
| `/v1/admin/works`, `/concepts`, `/concept-relations`, `/cards`, `/cards/{id}/publish`, `/withdraw`, `/inventory` | Editorial (editor/admin role) |
| `/v1/admin/sources`, `/sources/{id}` (PATCH), `/sources/{id}/poll` | Source register: feeds, licences, rights, "poll now" |
| `/v1/admin/ingest/items?status=new\|duplicate\|promoted\|rejected\|changed\|all`, `/{id}/promote`, `/reject`, `/acknowledge-change` | Review queue for ingested items |
| `/v1/admin/jobs?status=dead\|queued\|running\|succeeded\|all`, `/jobs/{id}/retry` | Job queue and dead letters |

Errors always look like `{"error": {"code": "...", "message": "...", "details": ...}}`.

## Content pipeline (step 3)

Editors register feeds; the ingestion worker polls them into a review queue; an editor
promotes an item to a source work and writes a card about it, which still has to pass the
publication gate. **Nothing from a feed ever reaches readers on its own.**

```bash
E='Authorization: Bearer dev:editor:sam'
curl -s -X POST -H "$E" -H 'content-type: application/json' localhost:8080/v1/admin/sources -d '{
  "name": "PLOS ONE", "feed_url": "https://journals.plos.org/plosone/feed/atom",
  "publisher": "PLOS", "default_work_type": "paper", "licence": "cc-by-4.0",
  "rights_status": "cleared", "store_excerpts": true, "enabled": true }'
curl -s -X POST -H "$E" localhost:8080/v1/admin/sources/<source_id>/poll   # queue a poll now
cargo run -p shirori-ingestion -- once      # run due polls and exit (or no argument: run until Ctrl-C)
curl -s -H "$E" localhost:8080/v1/admin/ingest/items                      # review queue
curl -s -X POST -H "$E" localhost:8080/v1/admin/ingest/items/<item_id>/promote   # → source work
```

Rules the worker and API enforce:

- **Rights first.** A source can only be enabled with rights `cleared` or `link_only` (the
  database refuses otherwise). Downgrading a source's rights switches it off. Promotion
  re-checks rights. Feed summaries are kept only when `store_excerpts` is on, which needs
  `cleared`; link-only sources keep title, link, authors, date and DOI only.
- **Polite fetching.** robots.txt is honoured (token `shiroribot`); conditional GET (ETag /
  Last-Modified); size and time limits; failing sources back off (interval × 2^failures, max ×32).
- **SSRF protection.** The worker only contacts the source's `allowed_host` (or its subdomains),
  refuses private/loopback/link-local addresses, pins connections to the checked addresses and
  re-checks every redirect. `SHIRORI_INGEST_ALLOW_PRIVATE=true` lifts the address check for
  local testing only.
- **Dedupe.** Tracking parameters are stripped from URLs and DOIs are detected. An item matching
  an earlier item (any source) or an existing source work is marked `duplicate`; promoting it
  needs `"force": true`.
- **News lifecycle.** If a promoted item changes at the source, it is flagged
  (`?status=changed`) until an editor acknowledges it after correcting the affected cards.
- **Jobs.** PostgreSQL queue with `FOR UPDATE SKIP LOCKED`, leases (a crashed worker's job is
  picked up by another), exponential backoff, and a dead-letter state. Permanent failures
  (robots block, SSRF block) go straight to dead; editors retry with `/v1/admin/jobs/{id}/retry`.

Worker settings (all optional): `SHIRORI_INGEST_CONCURRENCY` (2), `SHIRORI_INGEST_SCHEDULER_SECS`
(30), `SHIRORI_INGEST_TIMEOUT_SECS` (20), `SHIRORI_INGEST_MAX_BYTES` (5 MiB),
`SHIRORI_INGEST_MAX_ITEMS` (200), `SHIRORI_INGEST_USER_AGENT`.

## Tests

```bash
export DATABASE_URL=postgres://shirori:shirori@127.0.0.1:55432/shirori
cargo test --workspace               # unit tests + integration tests (each gets a fresh database)
cargo clippy --workspace --all-targets -- -D warnings
```

The integration tests cover the product rules: the publication gate (rights, evidence labels,
religion rule, Connection-card rules, stale revisions), dose shape, no repeats across days and
Discover, the Discover lock, related-first ranking, Saved with corrections, withdrawal
cascades, and consent. The ingestion tests run the worker against a local fake publisher:
conditional GET, excerpt rights, dedupe across sources and against the catalogue, changes after
promotion, robots.txt, retries → dead letter → editor retry, lease expiry, the scheduler, SSRF
blocks and size limits.

## Seed content

The seed cards are original Shirori wording with real citations (DOIs, NobelPrize.org press
releases, public-domain books). They are for development. An editor should re-check every card
before real readers see it. To reset the database: `podman compose -f infrastructure/docker/compose.yaml down -v`.
