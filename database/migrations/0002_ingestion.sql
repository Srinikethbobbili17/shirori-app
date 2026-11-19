-- Build step 3: rights-aware content foundation.
-- Source register, durable job queue, and the ingested-item review queue.

-- ───────────────────────── Source register ─────────────────────────

create table source (
    id                    uuid primary key default gen_random_uuid(),
    name                  text not null,
    kind                  text not null default 'feed' check (kind in ('feed')),  -- RSS / Atom / JSON Feed
    feed_url              text not null unique,
    homepage_url          text,
    allowed_host          text not null,          -- the only host the fetcher may contact (SSRF guard)
    publisher             text,
    default_work_type     text not null default 'article'
                          check (default_work_type in ('article', 'paper', 'book', 'reference')),
    -- Rights: set by a human after checking the publisher's terms (§2.2).
    licence               text not null,
    rights_status         text not null
                          check (rights_status in ('cleared', 'link_only', 'restricted', 'unknown')),
    store_excerpts        boolean not null default false,  -- may we keep the feed's summary text?
    allow_ai_processing   boolean not null default false,  -- may content be sent to an LLM (step 4)?
    poll_interval_minutes integer not null default 60 check (poll_interval_minutes >= 15),
    enabled               boolean not null default false,
    -- Fetch state
    etag                  text,
    last_modified         text,
    last_attempt_at       timestamptz,
    last_success_at       timestamptz,
    last_error            text,
    consecutive_failures  integer not null default 0,
    created_at            timestamptz not null default now(),
    updated_at            timestamptz not null default now(),
    -- "Do not ingest unknown rights": only cleared or link-only sources can be switched on.
    constraint source_enabled_requires_rights
        check (not enabled or rights_status in ('cleared', 'link_only'))
);

alter table source_work add column source_id uuid references source(id);

-- ───────────────────────── Durable job queue ─────────────────────────

create table job (
    id              uuid primary key default gen_random_uuid(),
    kind            text not null,
    payload         jsonb not null default '{}'::jsonb,
    payload_version integer not null default 1,
    idempotency_key text unique,
    status          text not null default 'queued'
                    check (status in ('queued', 'running', 'succeeded', 'dead')),
    attempt_count   integer not null default 0,
    max_attempts    integer not null default 5 check (max_attempts >= 1),
    run_after       timestamptz not null default now(),
    locked_until    timestamptz,
    locked_by       text,
    error_code      text,
    last_error      text,
    result          jsonb,
    created_at      timestamptz not null default now(),
    updated_at      timestamptz not null default now(),
    finished_at     timestamptz
);

create index job_claim_idx on job (run_after) where status in ('queued', 'running');
create index job_source_idx on job ((payload ->> 'source_id')) where kind = 'poll_source';

-- ───────────────────────── Ingested items (editor review queue) ─────────────────────────

create table ingested_item (
    id                      uuid primary key default gen_random_uuid(),
    source_id               uuid not null references source(id) on delete cascade,
    external_id             text not null,           -- feed GUID / Atom id
    canonical_url           text,
    doi                     text,
    title                   text not null,
    authors                 text,
    excerpt                 text,                    -- kept only when the source allows it
    published_at            timestamptz,
    content_hash            text not null,           -- detects updates to the item
    status                  text not null default 'new'
                            check (status in ('new', 'duplicate', 'promoted', 'rejected')),
    duplicate_of            uuid references ingested_item(id),
    existing_work_id        uuid references source_work(id),  -- already in the catalogue
    source_work_id          uuid references source_work(id),  -- set when an editor promotes it
    changed_since_promotion boolean not null default false,
    revision_count          integer not null default 1,
    rejection_reason        text,
    first_seen_at           timestamptz not null default now(),
    last_seen_at            timestamptz not null default now(),
    updated_at              timestamptz not null default now(),
    unique (source_id, external_id)
);

create index ingested_item_url_idx on ingested_item (canonical_url) where canonical_url is not null;
create index ingested_item_doi_idx on ingested_item (doi) where doi is not null;
create index ingested_item_review_idx on ingested_item (status, first_seen_at desc);
create index source_work_doi_idx on source_work (lower(doi_or_isbn)) where doi_or_isbn is not null;
create index source_work_url_idx on source_work (canonical_url) where canonical_url is not null;
