-- Shirori initial schema (architecture v0.2, build step 1: backend thin slice).
-- Text columns with CHECK constraints are used instead of Postgres enums so that
-- adding a value later is a simple constraint change.

-- ───────────────────────── Users ─────────────────────────

create table app_user (
    id                      uuid primary key default gen_random_uuid(),
    subject                 text not null unique,          -- identity-provider subject (dev:<name> locally)
    role                    text not null default 'reader'
                            check (role in ('reader', 'editor', 'admin')),
    timezone                text not null default 'UTC',   -- IANA name; decides the reader's local dose date
    personalisation_consent boolean not null default false,
    created_at              timestamptz not null default now()
);

-- ───────────────────────── Sources ─────────────────────────

create table source_work (
    id                uuid primary key default gen_random_uuid(),
    work_type         text not null check (work_type in ('article', 'paper', 'book', 'reference')),
    title             text not null,
    author            text,
    publisher         text,
    canonical_url     text,
    doi_or_isbn       text,
    edition           text,
    published_on      date,
    licence           text not null,                       -- e.g. public-domain, cc-by-4.0, link-only
    rights_status     text not null
                      check (rights_status in ('cleared', 'link_only', 'restricted', 'unknown')),
    is_religious_text boolean not null default false,      -- drives the religion rule (§2.2)
    language          text not null default 'en',
    created_at        timestamptz not null default now(),
    updated_at        timestamptz not null default now()
);

-- ───────────────────────── Concepts ─────────────────────────

create table concept (
    id          uuid primary key default gen_random_uuid(),
    slug        text not null unique,
    name        text not null,
    description text
);

create table concept_relation (
    id                uuid primary key default gen_random_uuid(),
    source_concept_id uuid not null references concept(id),
    target_concept_id uuid not null references concept(id),
    relation_type     text not null,                       -- e.g. contrasts_with, explains, analogous_to
    rationale         text not null,
    review_status     text not null default 'proposed'
                      check (review_status in ('proposed', 'approved', 'rejected')),
    created_at        timestamptz not null default now()
);

-- ───────────────────────── Cards ─────────────────────────

create table card (
    id                 uuid primary key default gen_random_uuid(),
    category           text not null
                       check (category in ('book', 'research', 'news', 'evergreen', 'connection')),
    field              text not null,                      -- subject domain, e.g. psychology
    primary_work_id    uuid references source_work(id),
    story_id           text,                               -- groups news cards about one evolving story
    follows_up_card_id uuid references card(id),           -- this card updates/corrects/extends that one
    importance         text not null default 'normal' check (importance in ('normal', 'important')),
    lifecycle_state    text not null default 'draft'
                       check (lifecycle_state in ('draft', 'in_review', 'published', 'withdrawn')),
    active_version_id  uuid,                               -- FK added below
    published_at       timestamptz,
    withdrawn_at       timestamptz,
    withdrawal_reason  text,
    created_at         timestamptz not null default now(),
    -- Connection cards have no single primary work; every other card must have one.
    constraint card_primary_work_matches_category
        check ((category = 'connection') = (primary_work_id is null)),
    constraint card_not_follow_up_of_itself check (follows_up_card_id <> id)
);

create table card_version (
    id                 uuid primary key default gen_random_uuid(),
    card_id            uuid not null references card(id) on delete cascade,
    revision           integer not null check (revision >= 1),
    headline           text not null check (length(trim(headline)) > 0),
    summary            text not null check (length(trim(summary)) > 0),
    deeper_explanation text,
    claim_kind         text not null
                       check (claim_kind in ('empirical', 'historical', 'reported', 'interpretive', 'normative')),
    evidence_label     text not null
                       check (evidence_label in ('single_study', 'replicated', 'review', 'consensus',
                                                 'contested', 'cited_argument', 'reported')),
    limitations        text,                               -- "What this doesn't show"
    source_date        date not null,
    model_lineage      jsonb,
    editor_id          uuid references app_user(id) on delete set null,
    created_at         timestamptz not null default now(),
    unique (card_id, revision),
    constraint card_version_limitations_required
        check (evidence_label not in ('single_study', 'contested')
               or (limitations is not null and length(trim(limitations)) > 0))
);

alter table card
    add constraint card_active_version_fk foreign key (active_version_id) references card_version(id);

create table card_concept (
    card_id    uuid not null references card(id) on delete cascade,
    concept_id uuid not null references concept(id),
    primary key (card_id, concept_id)
);

create table connection_card (
    card_id             uuid primary key references card(id) on delete cascade,
    left_card_id        uuid not null references card(id),
    right_card_id       uuid not null references card(id),
    concept_relation_id uuid references concept_relation(id),
    bridge_sentence     text not null check (length(trim(bridge_sentence)) > 0),
    check (left_card_id <> right_card_id),
    check (card_id <> left_card_id and card_id <> right_card_id)
);

create index card_published_idx on card (lifecycle_state, category);
create index card_follows_up_idx on card (follows_up_card_id) where follows_up_card_id is not null;
create index card_story_idx on card (story_id) where story_id is not null;
create index connection_card_left_idx on connection_card (left_card_id);
create index connection_card_right_idx on connection_card (right_card_id);

-- ───────────────────────── Daily dose & Discover ─────────────────────────

create table daily_dose (
    id           uuid primary key default gen_random_uuid(),
    user_id      uuid not null references app_user(id) on delete cascade,
    dose_date    date not null,                            -- the reader's local date
    status       text not null default 'in_progress' check (status in ('in_progress', 'completed')),
    created_at   timestamptz not null default now(),
    completed_at timestamptz,
    unique (user_id, dose_date)
);

create table daily_dose_item (
    dose_id         uuid not null references daily_dose(id) on delete cascade,
    position        integer not null check (position >= 1),
    card_id         uuid not null references card(id),
    card_version_id uuid not null references card_version(id),  -- version served, for audit
    primary key (dose_id, position),
    unique (dose_id, card_id)
);

-- Cards handed out in Discover today, so paging never serves the same card twice in a day.
create table discover_impression (
    user_id            uuid not null references app_user(id) on delete cascade,
    dose_date          date not null,
    card_id            uuid not null references card(id),
    reason             text not null
                       check (reason in ('correction', 'follow_up', 'same_story', 'connected', 'general')),
    related_to_card_id uuid references card(id),
    served_at          timestamptz not null default now(),
    primary key (user_id, dose_date, card_id)
);

-- No-repeat ledger: a card in here is never served to this reader again (ADR-017).
create table read_card (
    user_id       uuid not null references app_user(id) on delete cascade,
    card_id       uuid not null references card(id),
    feed_mode     text not null check (feed_mode in ('dose', 'discover')),
    first_read_at timestamptz not null default now(),
    primary key (user_id, card_id)
);

create table saved_card (
    user_id          uuid not null references app_user(id) on delete cascade,
    card_id          uuid not null references card(id),
    saved_version_id uuid not null references card_version(id),
    saved_from       text not null check (saved_from in ('dose', 'discover', 'connection', 'search')),
    saved_at         timestamptz not null default now(),
    primary key (user_id, card_id)
);

create index saved_card_user_time_idx on saved_card (user_id, saved_at desc);

-- ───────────────────────── Events & audit ─────────────────────────

-- Analytics events. Stored only when the reader has given personalisation consent.
create table user_event (
    id              uuid primary key,                      -- client-generated event_id (idempotent)
    user_id         uuid not null references app_user(id) on delete cascade,
    event_type      text not null
                    check (event_type in ('card_viewed', 'card_liked', 'card_saved', 'card_shared',
                                          'source_opened', 'connection_followed', 'dose_completed',
                                          'discover_entered')),
    card_id         uuid references card(id),
    card_version_id uuid references card_version(id),
    feed_mode       text check (feed_mode in ('dose', 'discover')),
    session_id      text,
    occurred_at     timestamptz not null,
    received_at     timestamptz not null default now(),
    consent_scope   text not null default 'personalisation'
);

create index user_event_user_type_idx on user_event (user_id, event_type);

-- Editorial audit trail (kept under a separate retention policy).
create table audit_event (
    id          uuid primary key default gen_random_uuid(),
    actor_id    uuid references app_user(id) on delete set null,
    action      text not null,
    entity_type text not null,
    entity_id   uuid not null,
    details     jsonb not null default '{}'::jsonb,
    created_at  timestamptz not null default now()
);
