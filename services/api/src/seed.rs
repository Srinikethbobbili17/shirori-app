//! Development seed loader.
//!
//! Seed cards go through the same editorial service and publication gate as cards created in
//! the editor, so a seed that breaks a rule fails loudly.

use std::collections::HashMap;

use anyhow::{Context, anyhow};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    editorial::service::{self, CardIn, ConceptIn, ConnectionIn, RelationIn, VersionIn, WorkIn},
    error::AppError,
};

#[derive(Deserialize)]
struct SeedFile {
    concepts: Vec<ConceptIn>,
    relations: Vec<SeedRelation>,
    works: Vec<SeedWork>,
    cards: Vec<SeedCard>,
}

#[derive(Deserialize)]
struct SeedRelation {
    key: String,
    #[serde(flatten)]
    relation: RelationIn,
}

#[derive(Deserialize)]
struct SeedWork {
    key: String,
    #[serde(flatten)]
    work: WorkIn,
}

#[derive(Deserialize)]
struct SeedCard {
    key: String,
    category: String,
    field: String,
    work: Option<String>,
    story_id: Option<String>,
    follows_up: Option<String>,
    importance: Option<String>,
    #[serde(default)]
    concepts: Vec<String>,
    version: VersionIn,
    connection: Option<SeedConnection>,
}

#[derive(Deserialize)]
struct SeedConnection {
    left: String,
    right: String,
    relation: String,
    bridge_sentence: String,
}

pub async fn load_file(pool: &PgPool, path: &str) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let seed: SeedFile = serde_json::from_str(&text).with_context(|| format!("parsing {path}"))?;
    load(pool, seed).await
}

fn lookup(map: &HashMap<String, Uuid>, kind: &str, key: &str) -> anyhow::Result<Uuid> {
    map.get(key)
        .copied()
        .ok_or_else(|| anyhow!("unknown {kind} key `{key}`"))
}

fn explain(key: &str, e: AppError) -> anyhow::Error {
    match e {
        AppError::GateFailed(reasons) => anyhow!("card `{key}` failed the publication gate: {}", reasons.join("; ")),
        other => anyhow!("card `{key}`: {other}"),
    }
}

async fn load(pool: &PgPool, seed: SeedFile) -> anyhow::Result<String> {
    let mut tx = pool.begin().await?;
    let existing: i64 = sqlx::query_scalar("select count(*) from card")
        .fetch_one(&mut *tx)
        .await?;
    if existing > 0 {
        return Ok(format!("database already has {existing} cards; seed skipped"));
    }
    let editor: Uuid = sqlx::query_scalar(
        "insert into app_user (subject, role) values ('seed:editor', 'editor')
         on conflict (subject) do update set role = 'editor' returning id",
    )
    .fetch_one(&mut *tx)
    .await?;

    for concept in &seed.concepts {
        service::create_concept(&mut tx, editor, concept)
            .await
            .map_err(|e| anyhow!("concept `{}`: {e}", concept.slug))?;
    }

    let mut relations = HashMap::new();
    for r in &seed.relations {
        let id = service::create_relation(&mut tx, editor, &r.relation)
            .await
            .map_err(|e| anyhow!("relation `{}`: {e}", r.key))?;
        service::review_relation(&mut tx, editor, id, true)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        relations.insert(r.key.clone(), id);
    }

    let mut works = HashMap::new();
    for w in &seed.works {
        let id = service::create_work(&mut tx, editor, &w.work)
            .await
            .map_err(|e| anyhow!("work `{}`: {e}", w.key))?;
        works.insert(w.key.clone(), id);
    }

    let mut cards: HashMap<String, Uuid> = HashMap::new();
    let mut connections = 0;
    for c in seed.cards {
        let connection = match &c.connection {
            Some(link) => {
                connections += 1;
                Some(ConnectionIn {
                    left_card_id: lookup(&cards, "card", &link.left)?,
                    right_card_id: lookup(&cards, "card", &link.right)?,
                    concept_relation_id: Some(lookup(&relations, "relation", &link.relation)?),
                    bridge_sentence: link.bridge_sentence.clone(),
                })
            }
            None => None,
        };
        let input = CardIn {
            category: c.category,
            field: c.field,
            primary_work_id: c.work.as_deref().map(|k| lookup(&works, "work", k)).transpose()?,
            story_id: c.story_id,
            follows_up_card_id: c.follows_up.as_deref().map(|k| lookup(&cards, "card", k)).transpose()?,
            importance: c.importance,
            concepts: c.concepts,
            version: c.version,
            connection,
        };
        let (card_id, _) = service::create_card(&mut tx, editor, &input)
            .await
            .map_err(|e| explain(&c.key, e))?;
        service::publish(&mut tx, editor, card_id, None)
            .await
            .map_err(|e| explain(&c.key, e))?;
        cards.insert(c.key, card_id);
    }
    tx.commit().await?;
    Ok(format!(
        "seeded {} concepts, {} relations, {} works, {} cards ({} connection cards)",
        seed.concepts.len(),
        relations.len(),
        works.len(),
        cards.len(),
        connections
    ))
}
