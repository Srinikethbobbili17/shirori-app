//! End-to-end API tests against a real PostgreSQL database.
//!
//! `#[sqlx::test]` creates a fresh database per test (needs `DATABASE_URL` pointing at a
//! server where the user may create databases) and applies the migrations.

use std::collections::HashSet;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use shirori_api::{AppState, config::Config, router};
use sqlx::PgPool;
use tower::ServiceExt;

const EDITOR: &str = "dev:editor:ed";
const READER: &str = "dev:reader:alice";

struct Api {
    app: Router,
}

impl Api {
    fn new(pool: PgPool) -> Self {
        Self {
            app: router(AppState::new(pool, Config::for_tests())),
        }
    }

    async fn call(&self, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn get(&self, uri: &str, token: &str) -> (StatusCode, Value) {
        self.call(Method::GET, uri, Some(token), None).await
    }

    async fn post(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    async fn put(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, uri, Some(token), Some(body)).await
    }

    async fn ok(&self, method: Method, uri: &str, token: &str, body: Option<Value>) -> Value {
        let (status, value) = self.call(method, uri, Some(token), body).await;
        assert!(status.is_success(), "{uri} -> {status}: {value}");
        value
    }

    // ── fixtures built through the editorial API ──

    async fn work(&self, title: &str, rights: &str, religious: bool) -> String {
        let v = self
            .ok(
                Method::POST,
                "/v1/admin/works",
                EDITOR,
                Some(
                    json!({ "work_type": "paper", "title": title, "licence": "test", "rights_status": rights,
                             "is_religious_text": religious }),
                ),
            )
            .await;
        v["id"].as_str().unwrap().to_string()
    }

    async fn concept(&self, slug: &str) {
        self.ok(
            Method::POST,
            "/v1/admin/concepts",
            EDITOR,
            Some(json!({ "slug": slug, "name": slug })),
        )
        .await;
    }

    async fn relation(&self, source: &str, target: &str, approve: bool) -> String {
        let v = self
            .ok(
                Method::POST,
                "/v1/admin/concept-relations",
                EDITOR,
                Some(json!({ "source": source, "target": target, "relation_type": "relates_to", "rationale": "test" })),
            )
            .await;
        let id = v["id"].as_str().unwrap().to_string();
        if approve {
            self.ok(
                Method::POST,
                &format!("/v1/admin/concept-relations/{id}/review"),
                EDITOR,
                Some(json!({ "decision": "approve" })),
            )
            .await;
        }
        id
    }

    async fn draft(&self, body: Value) -> String {
        let v = self.ok(Method::POST, "/v1/admin/cards", EDITOR, Some(body)).await;
        v["card_id"].as_str().unwrap().to_string()
    }

    async fn publish(&self, card: &str) -> (StatusCode, Value) {
        self.post(&format!("/v1/admin/cards/{card}/publish"), EDITOR, json!({}))
            .await
    }

    /// Create and publish a plain card.
    async fn card(&self, work: &str, category: &str, field: &str, extra: Value) -> String {
        let mut body = json!({
            "category": category, "field": field, "primary_work_id": work,
            "version": version("Headline", "empirical", "consensus"),
        });
        merge(&mut body, extra);
        let id = self.draft(body).await;
        let (status, v) = self.publish(&id).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        id
    }

    async fn connection(&self, left: &str, right: &str, relation: &str) -> String {
        let id = self
            .draft(json!({
                "category": "connection", "field": "cross-field",
                "version": version("Connection", "interpretive", "cited_argument"),
                "connection": { "left_card_id": left, "right_card_id": right,
                                "concept_relation_id": relation, "bridge_sentence": "One plain sentence." }
            }))
            .await;
        let (status, v) = self.publish(&id).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        id
    }
}

fn version(headline: &str, claim_kind: &str, label: &str) -> Value {
    json!({ "headline": headline, "summary": "Summary.", "claim_kind": claim_kind,
            "evidence_label": label, "source_date": "2024-01-01" })
}

fn merge(base: &mut Value, extra: Value) {
    if let (Some(b), Value::Object(e)) = (base.as_object_mut(), extra) {
        b.extend(e);
    }
}

fn ids(items: &Value, path: &str) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.pointer(path).and_then(Value::as_str).unwrap().to_string())
        .collect()
}

/// A small catalogue: 2 Connection cards over 4 bridged cards, plus fillers across categories.
struct Catalogue {
    research: Vec<String>,
    connections: Vec<String>,
    all_plain: Vec<String>,
}

async fn catalogue(api: &Api) -> Catalogue {
    let w = api.work("Work", "cleared", false).await;
    api.concept("a").await;
    api.concept("b").await;
    let rel = api.relation("a", "b", true).await;
    let mut all = Vec::new();
    let mut research = Vec::new();
    for (cat, field) in [
        ("research", "psychology"),
        ("research", "biology"),
        ("research", "economics"),
        ("research", "physics"),
        ("book", "philosophy"),
        ("news", "politics"),
        ("evergreen", "maths"),
        ("book", "history"),
        ("news", "science"),
        ("evergreen", "chemistry"),
    ] {
        let id = api
            .card(
                &w,
                cat,
                field,
                json!({"concepts": [if all.len() % 2 == 0 { "a" } else { "b" }]}),
            )
            .await;
        if cat == "research" {
            research.push(id.clone());
        }
        all.push(id);
    }
    let c1 = api.connection(&research[0], &research[1], &rel).await;
    let c2 = api.connection(&research[2], &research[3], &rel).await;
    Catalogue {
        research,
        connections: vec![c1, c2],
        all_plain: all,
    }
}

// ───────────────────────── Publication gate ─────────────────────────

#[sqlx::test(migrations = "../../database/migrations")]
async fn readers_cannot_use_admin_endpoints(pool: PgPool) {
    let api = Api::new(pool);
    let (status, body) = api.get("/v1/admin/cards", READER).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = api.call(Method::GET, "/v1/feed/daily", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn single_study_without_limitations_is_rejected(pool: PgPool) {
    let api = Api::new(pool);
    let w = api.work("W", "cleared", false).await;
    let (status, body) = api
        .post(
            "/v1/admin/cards",
            EDITOR,
            json!({ "category": "research", "field": "psychology", "primary_work_id": w,
                    "version": version("H", "empirical", "single_study") }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["error"]["details"]["constraint"],
        "card_version_limitations_required"
    );
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn gate_enforces_rights_and_religion_rule(pool: PgPool) {
    let api = Api::new(pool);
    let unknown = api.work("Unknown rights", "unknown", false).await;
    let card = api
        .draft(json!({ "category": "book", "field": "x", "primary_work_id": unknown,
                                 "version": version("H", "empirical", "consensus") }))
        .await;
    let (status, body) = api.publish(&card).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]["details"][0].as_str().unwrap().contains("rights status"));

    let scripture = api.work("Scripture", "cleared", true).await;
    let advice = api
        .draft(json!({ "category": "book", "field": "x", "primary_work_id": scripture,
                                   "version": version("H", "normative", "cited_argument") }))
        .await;
    let (status, body) = api.publish(&advice).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.to_string().contains("religion rule"), "{body}");

    let history = api
        .draft(
            json!({ "category": "book", "field": "linguistics", "primary_work_id": scripture,
                                    "version": version("H", "historical", "consensus") }),
        )
        .await;
    assert_eq!(api.publish(&history).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn gate_enforces_connection_rules(pool: PgPool) {
    let api = Api::new(pool);
    let w = api.work("W", "cleared", false).await;
    api.concept("a").await;
    api.concept("b").await;
    let approved = api.relation("a", "b", true).await;
    let proposed = api.relation("a", "b", false).await;
    let p1 = api.card(&w, "research", "psychology", json!({"concepts":["a"]})).await;
    let p2 = api.card(&w, "book", "psychology", json!({"concepts":["b"]})).await;
    let bio = api.card(&w, "research", "biology", json!({"concepts":["b"]})).await;

    let link = |l: &str, r: &str, rel: &str| {
        json!({ "category": "connection", "field": "cross-field",
                "version": version("C", "interpretive", "cited_argument"),
                "connection": { "left_card_id": l, "right_card_id": r, "concept_relation_id": rel,
                                "bridge_sentence": "One sentence." } })
    };
    let same_field = api.draft(link(&p1, &p2, &approved)).await;
    let (status, body) = api.publish(&same_field).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.to_string().contains("different fields"), "{body}");

    let unreviewed = api.draft(link(&p1, &bio, &proposed)).await;
    let (status, body) = api.publish(&unreviewed).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.to_string().contains("must be approved"), "{body}");

    let good = api.draft(link(&p1, &bio, &approved)).await;
    assert_eq!(api.publish(&good).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn stale_revision_cannot_be_approved(pool: PgPool) {
    let api = Api::new(pool);
    let w = api.work("W", "cleared", false).await;
    let card = api.card(&w, "research", "psychology", json!({})).await;
    let detail = api
        .ok(Method::GET, &format!("/v1/admin/cards/{card}"), EDITOR, None)
        .await;
    let v1 = detail["versions"][0]["id"].as_str().unwrap().to_string();
    api.ok(
        Method::POST,
        &format!("/v1/admin/cards/{card}/versions"),
        EDITOR,
        Some(version("Revised", "empirical", "consensus")),
    )
    .await;
    let (status, body) = api
        .post(
            &format!("/v1/admin/cards/{card}/publish"),
            EDITOR,
            json!({ "version_id": v1 }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.to_string().contains("stale review"), "{body}");
    // Readers keep seeing revision 1 until revision 2 is published.
    let (_, card_view) = api.get(&format!("/v1/cards/{card}"), READER).await;
    assert_eq!(card_view["revision"], 1);
    assert_eq!(api.publish(&card).await.0, StatusCode::OK);
    let (_, card_view) = api.get(&format!("/v1/cards/{card}"), READER).await;
    assert_eq!(card_view["headline"], "Revised");
}

// ───────────────────────── Daily dose ─────────────────────────

#[sqlx::test(migrations = "../../database/migrations")]
async fn dose_has_six_cards_and_ends_with_a_connection(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let items = &dose["items"];
    assert_eq!(items.as_array().unwrap().len(), 6);
    assert_eq!(dose["ends_with_connection"], true);
    assert_eq!(dose["discover_available"], false);

    let card_ids = ids(items, "/card_id");
    let last = items.as_array().unwrap().last().unwrap();
    assert!(cat.connections.contains(&last["card_id"].as_str().unwrap().to_string()));
    let bridge = &last["card"]["connection"];
    let left = bridge["left_card_id"].as_str().unwrap().to_string();
    let right = bridge["right_card_id"].as_str().unwrap().to_string();
    assert!(card_ids[..5].contains(&left) && card_ids[..5].contains(&right));
    assert_ne!(bridge["left_field"], bridge["right_field"]);
    assert_eq!(card_ids.iter().collect::<HashSet<_>>().len(), 6, "no duplicates");

    // Same dose all day.
    let again = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    assert_eq!(again["dose_id"], dose["dose_id"]);
    assert_eq!(ids(&again["items"], "/card_id"), card_ids);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn read_cards_never_repeat_in_later_doses(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    let day1 = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let dose_id = day1["dose_id"].as_str().unwrap();
    api.ok(
        Method::POST,
        &format!("/v1/feed/daily/{dose_id}/complete?date=2026-11-06"),
        READER,
        None,
    )
    .await;

    let day2 = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-07", READER, None)
        .await;
    let first: HashSet<_> = ids(&day1["items"], "/card_id").into_iter().collect();
    let second = ids(&day2["items"], "/card_id");
    assert!(!second.is_empty());
    assert!(
        second.iter().all(|id| !first.contains(id)),
        "day 2 repeated a read card"
    );
    // Day 2 uses the other Connection card.
    assert_eq!(day2["ends_with_connection"], true);
    assert_eq!(cat.connections.len(), 2);

    // A different reader still gets everything.
    let bob = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", "dev:reader:bob", None)
        .await;
    assert_eq!(bob["items"].as_array().unwrap().len(), 6);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn dose_ships_without_connection_rather_than_repeat(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    // Alice has already read both research cards that the first Connection card bridges,
    // and the second Connection card itself.
    for card in [&cat.research[0], &cat.connections[1]] {
        api.ok(
            Method::POST,
            "/v1/events",
            READER,
            Some(json!({
            "event_id": uuid(), "event_type": "card_viewed", "card_id": card, "feed_mode": "discover" })),
        )
        .await;
    }
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    assert_eq!(dose["ends_with_connection"], false);
    let card_ids = ids(&dose["items"], "/card_id");
    assert!(!card_ids.contains(&cat.research[0]));
    assert!(!card_ids.contains(&cat.connections[1]));
}

// ───────────────────────── Discover ─────────────────────────

#[sqlx::test(migrations = "../../database/migrations")]
async fn discover_is_locked_until_the_dose_is_completed(pool: PgPool) {
    let api = Api::new(pool);
    catalogue(&api).await;
    let (status, body) = api.get("/v1/feed/discover?date=2026-11-06", READER).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "dose_not_completed");

    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let (status, _) = api.get("/v1/feed/discover?date=2026-11-06", READER).await;
    assert_eq!(status, StatusCode::CONFLICT, "an unfinished dose still locks Discover");

    let id = dose["dose_id"].as_str().unwrap();
    let done = api
        .ok(
            Method::POST,
            &format!("/v1/feed/daily/{id}/complete?date=2026-11-06"),
            READER,
            None,
        )
        .await;
    assert_eq!(done["discover_available"], true);
    let (status, _) = api.get("/v1/feed/discover?date=2026-11-06", READER).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn discover_never_repeats_and_ends_when_everything_is_read(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let dose_cards: HashSet<_> = ids(&dose["items"], "/card_id").into_iter().collect();
    let id = dose["dose_id"].as_str().unwrap();
    api.ok(
        Method::POST,
        &format!("/v1/feed/daily/{id}/complete?date=2026-11-06"),
        READER,
        None,
    )
    .await;

    let mut seen = HashSet::new();
    loop {
        let page = api
            .ok(Method::GET, "/v1/feed/discover?date=2026-11-06&limit=3", READER, None)
            .await;
        let page_ids = ids(&page["items"], "/card/card_id");
        for card in &page_ids {
            assert!(!dose_cards.contains(card), "Discover repeated a dose card");
            assert!(seen.insert(card.clone()), "Discover served a card twice");
            assert!(!cat.connections.contains(card), "Connection cards stay in doses");
        }
        if page["exhausted"] == true {
            break;
        }
    }
    let plain_unread = cat.all_plain.iter().filter(|c| !dose_cards.contains(*c)).count();
    assert_eq!(seen.len(), plain_unread);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn discover_ranks_related_cards_first(pool: PgPool) {
    let api = Api::new(pool);
    let w = api.work("W", "cleared", false).await;
    for c in ["memory", "sleep", "unrelated"] {
        api.concept(c).await;
    }
    let rel = api.relation("memory", "sleep", true).await;
    // Dose: two bridged cards + connection + fillers. Make the dose deterministic with size 6.
    let left = api
        .card(
            &w,
            "research",
            "psychology",
            json!({ "concepts": ["memory"], "story_id": "story-1" }),
        )
        .await;
    let right = api
        .card(&w, "research", "neuroscience", json!({ "concepts": ["sleep"] }))
        .await;
    api.connection(&left, &right, &rel).await;
    for (cat, field) in [("book", "a"), ("news", "b"), ("evergreen", "c")] {
        api.card(&w, cat, field, json!({ "concepts": ["unrelated"] })).await;
    }
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let id = dose["dose_id"].as_str().unwrap();
    api.ok(
        Method::POST,
        &format!("/v1/feed/daily/{id}/complete?date=2026-11-06"),
        READER,
        None,
    )
    .await;

    // Published after the dose was built, so none of these were in it.
    let general = api.card(&w, "book", "z", json!({ "importance": "important" })).await;
    let connected = api.card(&w, "book", "y", json!({ "concepts": ["sleep"] })).await;
    let same_story = api.card(&w, "news", "x", json!({ "story_id": "story-1" })).await;
    let follow_up = api
        .card(&w, "research", "psychology", json!({ "follows_up_card_id": left }))
        .await;

    let page = api
        .ok(Method::GET, "/v1/feed/discover?date=2026-11-06&limit=4", READER, None)
        .await;
    let order = ids(&page["items"], "/card/card_id");
    let reasons = ids(&page["items"], "/reason");
    assert_eq!(order, vec![follow_up, same_story, connected.clone(), general]);
    assert_eq!(reasons, vec!["follow_up", "same_story", "connected", "general"]);
    assert_eq!(page["items"][2]["related_to_card_id"], json!(right));
}

// ───────────────────────── Saved, withdrawal, events ─────────────────────────

#[sqlx::test(migrations = "../../database/migrations")]
async fn saved_cards_survive_no_repeat_and_show_corrections(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    let card = &cat.all_plain[5];
    let (status, _) = api
        .put(&format!("/v1/me/saved/{card}"), READER, json!({ "saved_from": "dose" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    // Idempotent.
    api.put(&format!("/v1/me/saved/{card}"), READER, json!({ "saved_from": "dose" }))
        .await;
    let list = api.ok(Method::GET, "/v1/me/saved", READER, None).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["card"]["card_id"], json!(card));

    // Withdraw it and publish a correction as a follow-up.
    api.ok(
        Method::POST,
        &format!("/v1/admin/cards/{card}/withdraw"),
        EDITOR,
        Some(json!({ "reason": "The source was retracted." })),
    )
    .await;
    let w = api.work("Correction source", "cleared", false).await;
    let fix = api
        .card(&w, "news", "politics", json!({ "follows_up_card_id": card }))
        .await;

    let list = api.ok(Method::GET, "/v1/me/saved", READER, None).await;
    let item = &list["items"][0];
    assert_eq!(item["status"], "withdrawn");
    assert!(item["card"].is_null(), "old text must not be shown");
    assert_eq!(item["withdrawal"]["reason"], "The source was retracted.");
    assert_eq!(item["follow_up"]["card_id"], json!(fix));

    let (status, body) = api.get(&format!("/v1/cards/{card}"), READER).await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(body["error"]["details"]["follow_up_card_id"], json!(fix));

    api.ok(Method::DELETE, &format!("/v1/me/saved/{card}"), READER, None)
        .await;
    let list = api.ok(Method::GET, "/v1/me/saved", READER, None).await;
    assert!(list["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn withdrawing_a_bridged_card_withdraws_its_connection(pool: PgPool) {
    let api = Api::new(pool);
    let cat = catalogue(&api).await;
    let v = api
        .ok(
            Method::POST,
            &format!("/v1/admin/cards/{}/withdraw", cat.research[0]),
            EDITOR,
            Some(json!({ "reason": "Error found." })),
        )
        .await;
    let withdrawn = ids(&v["withdrawn_card_ids"], "");
    assert!(withdrawn.contains(&cat.connections[0]));
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    let card_ids = ids(&dose["items"], "/card_id");
    assert!(!card_ids.contains(&cat.connections[0]));
    assert!(card_ids.contains(&cat.connections[1]));
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn viewing_marks_read_but_analytics_needs_consent(pool: PgPool) {
    let api = Api::new(pool.clone());
    let cat = catalogue(&api).await;
    let card = &cat.all_plain[4];
    let event =
        |id: String| json!({ "event_id": id, "event_type": "card_viewed", "card_id": card, "feed_mode": "dose" });

    let v = api.ok(Method::POST, "/v1/events", READER, Some(event(uuid()))).await;
    assert_eq!(v["marked_read"], true);
    assert_eq!(v["stored_for_analytics"], false);

    api.ok(
        Method::PUT,
        "/v1/me/preferences",
        READER,
        Some(json!({ "personalisation_consent": true })),
    )
    .await;
    let v = api.ok(Method::POST, "/v1/events", READER, Some(event(uuid()))).await;
    assert_eq!(v["stored_for_analytics"], true);

    let stored: i64 = sqlx::query_scalar("select count(*) from user_event")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
    let dose = api
        .ok(Method::GET, "/v1/feed/daily?date=2026-11-06", READER, None)
        .await;
    assert!(
        !ids(&dose["items"], "/card_id").contains(card),
        "a viewed card must not come back"
    );
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn preferences_validate_time_zone_and_export_and_delete_work(pool: PgPool) {
    let api = Api::new(pool);
    let (status, _) = api
        .put("/v1/me/preferences", READER, json!({ "timezone": "Mars/Olympus" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let v = api
        .ok(
            Method::PUT,
            "/v1/me/preferences",
            READER,
            Some(json!({ "timezone": "Europe/London" })),
        )
        .await;
    assert_eq!(v["timezone"], "Europe/London");
    let export = api.ok(Method::GET, "/v1/me/export", READER, None).await;
    assert_eq!(export["profile"]["timezone"], "Europe/London");
    let (status, _) = api.call(Method::DELETE, "/v1/me", Some(READER), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ───────────────────────── Content pipeline (step 3) ─────────────────────────

const FEED: &str = "https://journal.example/feed.xml";

async fn register_source(api: &Api, feed_url: &str, rights: &str, enabled: bool) -> (StatusCode, Value) {
    api.post(
        "/v1/admin/sources",
        EDITOR,
        json!({ "name": "Test journal", "feed_url": feed_url, "publisher": "Test Press",
                "default_work_type": "paper", "licence": "cc-by-4.0", "rights_status": rights,
                "enabled": enabled }),
    )
    .await
}

/// What the ingestion worker would have written for one feed item.
async fn ingested(pool: &PgPool, source: &str, external_id: &str, url: &str, status: &str) -> String {
    let id: uuid::Uuid = sqlx::query_scalar(
        "insert into ingested_item (source_id, external_id, canonical_url, doi, title, authors, published_at,
                                    content_hash, status)
         values ($1::uuid, $2, $3, '10.1234/x.' || $2, 'Item ' || $2, 'A. Author', '2026-11-09T10:00:00Z', 'h', $4)
         returning id",
    )
    .bind(source)
    .bind(external_id)
    .bind(url)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap();
    id.to_string()
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn pipeline_endpoints_are_editor_only(pool: PgPool) {
    let api = Api::new(pool);
    for uri in ["/v1/admin/sources", "/v1/admin/ingest/items", "/v1/admin/jobs"] {
        let (status, _) = api.get(uri, READER).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn sources_need_cleared_rights_to_be_enabled(pool: PgPool) {
    let api = Api::new(pool);
    let (status, body) = register_source(&api, FEED, "unknown", true).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "rights_not_cleared");

    let (status, body) = register_source(&api, "ftp://journal.example/feed", "cleared", true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, src) = register_source(&api, FEED, "unknown", false).await;
    assert_eq!(status, StatusCode::CREATED, "{src}");
    assert_eq!(src["allowed_host"], "journal.example");
    let id = src["id"].as_str().unwrap();
    let uri = format!("/v1/admin/sources/{id}");

    let (status, _) = api
        .call(Method::PATCH, &uri, Some(EDITOR), Some(json!({ "enabled": true })))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = api.post(&format!("{uri}/poll"), EDITOR, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "a disabled source cannot be polled");

    // An editor checks the licence, clears it and switches it on.
    let v = api
        .ok(
            Method::PATCH,
            &uri,
            EDITOR,
            Some(json!({ "rights_status": "cleared", "enabled": true })),
        )
        .await;
    assert_eq!(
        (v["rights_status"].as_str(), v["enabled"].as_bool()),
        (Some("cleared"), Some(true))
    );

    // "Poll now" queues one job, and asking again doesn't queue a second.
    let (status, first) = api.post(&format!("{uri}/poll"), EDITOR, json!({})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, again) = api.post(&format!("{uri}/poll"), EDITOR, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["already_queued"], true);
    assert_eq!(first["job_id"], again["job_id"]);

    // If the publisher's terms change, downgrading rights switches the source off.
    let v = api
        .ok(
            Method::PATCH,
            &uri,
            EDITOR,
            Some(json!({ "rights_status": "restricted" })),
        )
        .await;
    assert_eq!(v["enabled"], false);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn promoted_item_becomes_a_work_that_a_card_can_cite(pool: PgPool) {
    let api = Api::new(pool.clone());
    let (_, src) = register_source(&api, FEED, "cleared", true).await;
    let source = src["id"].as_str().unwrap();
    let item = ingested(&pool, source, "1", "https://journal.example/a/1", "new").await;

    let queue = api.ok(Method::GET, "/v1/admin/ingest/items", EDITOR, None).await;
    assert_eq!(ids(&queue, "/id"), vec![item.clone()]);
    let listed = api
        .ok(Method::GET, &format!("/v1/admin/sources/{source}"), EDITOR, None)
        .await;
    assert_eq!(listed["pending_review"], 1);

    let (status, v) = api
        .call(
            Method::POST,
            &format!("/v1/admin/ingest/items/{item}/promote"),
            Some(EDITOR),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let work = v["source_work_id"].as_str().unwrap().to_string();

    let works = api.ok(Method::GET, "/v1/admin/works", EDITOR, None).await;
    let w = works
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == work.as_str())
        .unwrap();
    assert_eq!(w["title"], "Item 1");
    assert_eq!(w["work_type"], "paper");
    assert_eq!(w["doi_or_isbn"], "10.1234/x.1");
    assert_eq!(w["rights_status"], "cleared");
    assert_eq!(w["licence"], "cc-by-4.0");
    assert_eq!(w["published_on"], "2026-11-09");

    // Promoting is not publishing: an editor still writes the card, and it passes the gate.
    api.card(&work, "research", "biology", json!({})).await;

    let (status, body) = api
        .call(
            Method::POST,
            &format!("/v1/admin/ingest/items/{item}/promote"),
            Some(EDITOR),
            None,
        )
        .await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("already_reviewed"))
    );
    let queue = api.ok(Method::GET, "/v1/admin/ingest/items", EDITOR, None).await;
    assert!(queue.as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn duplicates_need_force_and_rejects_need_a_reason(pool: PgPool) {
    let api = Api::new(pool.clone());
    let (_, src) = register_source(&api, FEED, "cleared", true).await;
    let source = src["id"].as_str().unwrap();
    let dup = ingested(&pool, source, "d", "https://journal.example/a/d", "duplicate").await;
    let other = ingested(&pool, source, "o", "https://journal.example/a/o", "new").await;

    let promote = format!("/v1/admin/ingest/items/{dup}/promote");
    let (status, body) = api.post(&promote, EDITOR, json!({})).await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("duplicate"))
    );
    let (status, body) = api
        .post(&promote, EDITOR, json!({ "force": true, "title": "Corrected title" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let reject = format!("/v1/admin/ingest/items/{other}/reject");
    let (status, _) = api.post(&reject, EDITOR, json!({ "reason": "  " })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    api.ok(Method::POST, &reject, EDITOR, Some(json!({ "reason": "off-topic" })))
        .await;
    let rejected = api
        .ok(Method::GET, "/v1/admin/ingest/items?status=rejected", EDITOR, None)
        .await;
    assert_eq!(rejected[0]["rejection_reason"], "off-topic");
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn promotion_rechecks_rights_and_changes_must_be_acknowledged(pool: PgPool) {
    let api = Api::new(pool.clone());
    let (_, src) = register_source(&api, FEED, "cleared", true).await;
    let source = src["id"].as_str().unwrap();
    let a = ingested(&pool, source, "a", "https://journal.example/a/a", "new").await;
    let b = ingested(&pool, source, "b", "https://journal.example/a/b", "new").await;
    api.ok(
        Method::POST,
        &format!("/v1/admin/ingest/items/{a}/promote"),
        EDITOR,
        None,
    )
    .await;

    // The worker saw the promoted item change at the source.
    sqlx::query("update ingested_item set changed_since_promotion = true where id = $1::uuid")
        .bind(&a)
        .execute(&pool)
        .await
        .unwrap();
    let changed = api
        .ok(Method::GET, "/v1/admin/ingest/items?status=changed", EDITOR, None)
        .await;
    assert_eq!(ids(&changed, "/id"), vec![a.clone()]);
    let ack = format!("/v1/admin/ingest/items/{a}/acknowledge-change");
    api.ok(Method::POST, &ack, EDITOR, None).await;
    let (status, _) = api.call(Method::POST, &ack, Some(EDITOR), None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The licence changed before item b was reviewed.
    api.ok(
        Method::PATCH,
        &format!("/v1/admin/sources/{source}"),
        EDITOR,
        Some(json!({ "rights_status": "restricted" })),
    )
    .await;
    let (status, body) = api
        .call(
            Method::POST,
            &format!("/v1/admin/ingest/items/{b}/promote"),
            Some(EDITOR),
            None,
        )
        .await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("rights_not_cleared"))
    );
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn only_dead_jobs_can_be_retried(pool: PgPool) {
    let api = Api::new(pool.clone());
    let dead: uuid::Uuid = sqlx::query_scalar(
        "insert into job (kind, status, attempt_count, error_code, last_error, finished_at)
         values ('poll_source', 'dead', 5, 'http_503', 'HTTP status 503', now()) returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let queued: uuid::Uuid = sqlx::query_scalar("insert into job (kind) values ('poll_source') returning id")
        .fetch_one(&pool)
        .await
        .unwrap();

    let list = api.ok(Method::GET, "/v1/admin/jobs", EDITOR, None).await;
    assert_eq!(ids(&list, "/id"), vec![dead.to_string()]);
    assert_eq!(list[0]["error_code"], "http_503");

    let v = api
        .ok(Method::POST, &format!("/v1/admin/jobs/{dead}/retry"), EDITOR, None)
        .await;
    assert_eq!(v["status"], "queued");
    let (status, _) = api
        .call(
            Method::POST,
            &format!("/v1/admin/jobs/{queued}/retry"),
            Some(EDITOR),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = api
        .call(
            Method::POST,
            &format!("/v1/admin/jobs/{}/retry", uuid::Uuid::new_v4()),
            Some(EDITOR),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// Backend audit regression tests.
#[sqlx::test(migrations = "../../database/migrations")]
async fn audit_concurrent_daily_requests_return_one_dose(pool: PgPool) {
    let api = Api::new(pool.clone());
    catalogue(&api).await;
    let (a, b) = tokio::join!(api.get("/v1/feed/daily", READER), api.get("/v1/feed/daily", READER));
    assert_eq!(a.0, StatusCode::OK);
    assert_eq!(b.0, StatusCode::OK);
    assert_eq!(a.1["dose_id"], b.1["dose_id"]);
    let count: i64 = sqlx::query_scalar("select count(*) from daily_dose")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn audit_production_mode_refuses_dev_auth(pool: PgPool) {
    let mut config = Config::for_tests();
    config.dev_auth = false;
    let api = Api {
        app: router(AppState::new(pool, config)),
    };
    assert_eq!(api.get("/v1/me", READER).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(api.get("/v1/admin/inventory", EDITOR).await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn connection_rejects_unrelated_approved_relation(pool: PgPool) {
    let api = Api::new(pool);
    let work = api.work("Source", "cleared", false).await;
    api.concept("unrelated-a").await;
    api.concept("unrelated-b").await;
    let relation = api.relation("unrelated-a", "unrelated-b", true).await;
    // Neither card is tagged with either concept in this relationship.
    let left = api.card(&work, "research", "biology", json!({})).await;
    let right = api.card(&work, "book", "history", json!({})).await;
    let connection = api
        .draft(json!({
            "category": "connection", "field": "cross-field",
            "version": version("Connection", "interpretive", "cited_argument"),
            "connection": {"left_card_id": left, "right_card_id": right,
                "concept_relation_id": relation, "bridge_sentence": "A bridge."}
        }))
        .await;
    let (status, body) = api.publish(&connection).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.to_string().contains("match concepts on both"));
    // Reversed visual ordering is valid when each endpoint is represented.
    let left = api
        .card(&work, "research", "biology", json!({"concepts":["unrelated-b"]}))
        .await;
    let right = api
        .card(&work, "book", "history", json!({"concepts":["unrelated-a"]}))
        .await;
    api.connection(&left, &right, &relation).await;
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn export_includes_reader_history_without_other_readers(pool: PgPool) {
    let api = Api::new(pool.clone());
    catalogue(&api).await;
    let (_, dose) = api.get("/v1/feed/daily", READER).await;
    let id = dose["dose_id"].as_str().unwrap();
    assert_eq!(
        api.post(&format!("/v1/feed/daily/{id}/complete"), READER, json!({}))
            .await
            .0,
        StatusCode::OK
    );
    let (status, discover) = api.get("/v1/feed/discover", READER).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!discover["items"].as_array().unwrap().is_empty());
    let count: i64 = sqlx::query_scalar("select count(*) from discover_impression")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(count > 0);
    let (status, export) = api.get("/v1/me/export", READER).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["discover_impressions"].as_array().unwrap().len(), count as usize);
    assert_eq!(
        export["daily_dose_items"].as_array().unwrap().len(),
        dose["items"].as_array().unwrap().len()
    );
    assert!(export["daily_dose_items"][0]["card_version_id"].is_string());
    let (_, other) = api.get("/v1/feed/daily", "dev:reader:bob").await;
    let (_, isolated) = api.get("/v1/me/export", READER).await;
    assert_eq!(isolated, export);
    let (_, bob) = api.get("/v1/me/export", "dev:reader:bob").await;
    assert_eq!(bob["daily_doses"][0]["id"], other["dose_id"]);
    assert_eq!(bob["discover_impressions"], json!([]));
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn invalid_paths_queries_and_routes_have_json_errors(pool: PgPool) {
    let app = Api::new(pool).app;
    for (method, uri, status, code) in [
        (
            Method::GET,
            "/v1/cards/not-a-uuid",
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            Method::GET,
            "/v1/feed/daily?date=invalid",
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            Method::GET,
            "/v1/me/saved?limit=invalid",
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (Method::GET, "/missing", StatusCode::NOT_FOUND, "not_found"),
        (
            Method::POST,
            "/health",
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
        ),
        (Method::GET, "/v1/admin/missing", StatusCode::NOT_FOUND, "not_found"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {READER}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{uri}");
        if status == StatusCode::METHOD_NOT_ALLOWED {
            assert!(response.headers().contains_key("allow"));
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], code, "{uri}");
    }
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn development_seed_passes_stricter_publication_gate(pool: PgPool) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../database/seed/dev-seed.json");
    shirori_api::seed::load_file(&pool, path).await.unwrap();
    let counts: (i64, i64) = sqlx::query_as(
        "select count(*), count(*) filter (where category = 'connection') from card where lifecycle_state = 'published'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (30, 5));
}

#[sqlx::test(migrations = "../../database/migrations")]
async fn web_lab_assets_are_only_available_with_dev_auth(pool: PgPool) {
    for dev_auth in [true, false] {
        let mut config = Config::for_tests();
        config.dev_auth = dev_auth;
        let app = router(AppState::new(pool.clone(), config));
        for (path, mime, marker) in [
            ("/", "text/html", "SHIRORI / DEVELOPMENT PREVIEW"),
            ("/lab/app.js", "text/javascript", "async function daily"),
            ("/lab/style.css", "text/css", "prefers-reduced-motion"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if dev_auth {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_FOUND
                }
            );
            if dev_auth {
                assert!(response.headers()["content-type"].to_str().unwrap().starts_with(mime));
                assert_eq!(response.headers()["cache-control"], "no-store");
                let body = response.into_body().collect().await.unwrap().to_bytes();
                assert!(std::str::from_utf8(&body).unwrap().contains(marker));
            }
        }
    }
}
