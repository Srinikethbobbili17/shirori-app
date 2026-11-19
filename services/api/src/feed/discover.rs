//! Discover ranking (architecture §6.3, ADR-018): related-first, then general.
//!
//! Pure function: the caller passes only eligible cards (published, unread, not yet served
//! today, not Connection cards).

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::dose::{PoolCard, round_robin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Tier 1: corrects a card the reader has read (that card is now withdrawn).
    Correction,
    /// Tier 1: updates or extends a card in today's dose or one the reader has read.
    FollowUp,
    /// Tier 2: same evolving story as a card in today's dose.
    SameStory,
    /// Tier 3: shares, or is linked by a reviewed relation to, a concept from today's dose.
    Connected,
    /// Tier 4: general, diversity-aware Discover.
    General,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Correction => "correction",
            Self::FollowUp => "follow_up",
            Self::SameStory => "same_story",
            Self::Connected => "connected",
            Self::General => "general",
        }
    }

    fn tier(self) -> u8 {
        match self {
            Self::Correction | Self::FollowUp => 1,
            Self::SameStory => 2,
            Self::Connected => 3,
            Self::General => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: Uuid,
    pub category: String,
    pub important: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub story_id: Option<String>,
    pub follows_up_card_id: Option<Uuid>,
    pub concepts: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct DoseCard {
    pub id: Uuid,
    pub story_id: Option<String>,
    pub concepts: Vec<Uuid>,
}

#[derive(Debug, Default)]
pub struct Context {
    /// Non-Connection cards in today's dose.
    pub dose_cards: Vec<DoseCard>,
    /// Cards the reader has read, mapped to whether they are now withdrawn.
    pub read_cards: HashMap<Uuid, bool>,
    /// Approved concept relations (either direction).
    pub relations: Vec<(Uuid, Uuid)>,
    /// Concept weights from the reader's saves and likes (empty without consent).
    pub interests: HashMap<Uuid, u32>,
    /// Related cards (tiers 1–3) still allowed today.
    pub related_budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub card_id: Uuid,
    pub reason: Reason,
    pub related_to: Option<Uuid>,
}

pub fn rank(ctx: &Context, candidates: &[Candidate], limit: usize) -> Vec<Pick> {
    let dose_ids: HashSet<Uuid> = ctx.dose_cards.iter().map(|d| d.id).collect();

    // concept -> a dose card it relates to (directly, or via an approved relation)
    let mut concept_anchor: HashMap<Uuid, Uuid> = HashMap::new();
    for dose_card in &ctx.dose_cards {
        for concept in &dose_card.concepts {
            concept_anchor.entry(*concept).or_insert(dose_card.id);
        }
    }
    let direct: Vec<(Uuid, Uuid)> = concept_anchor.iter().map(|(k, v)| (*k, *v)).collect();
    for (a, b) in &ctx.relations {
        for (concept, anchor) in &direct {
            if a == concept {
                concept_anchor.entry(*b).or_insert(*anchor);
            } else if b == concept {
                concept_anchor.entry(*a).or_insert(*anchor);
            }
        }
    }

    let classify = |c: &Candidate| -> Option<(Reason, Uuid)> {
        if let Some(original) = c.follows_up_card_id {
            if let Some(&withdrawn) = ctx.read_cards.get(&original) {
                return Some((
                    if withdrawn {
                        Reason::Correction
                    } else {
                        Reason::FollowUp
                    },
                    original,
                ));
            }
            if dose_ids.contains(&original) {
                return Some((Reason::FollowUp, original));
            }
        }
        if let Some(story) = &c.story_id
            && let Some(d) = ctx.dose_cards.iter().find(|d| d.story_id.as_ref() == Some(story))
        {
            return Some((Reason::SameStory, d.id));
        }
        c.concepts
            .iter()
            .find_map(|k| concept_anchor.get(k))
            .map(|anchor| (Reason::Connected, *anchor))
    };

    let mut related: Vec<(&Candidate, Reason, Uuid)> = candidates
        .iter()
        .filter_map(|c| classify(c).map(|(r, to)| (c, r, to)))
        .collect();
    related.sort_by(|(a, ra, _), (b, rb, _)| {
        ra.tier()
            .cmp(&rb.tier())
            .then(b.important.cmp(&a.important))
            .then(b.published_at.cmp(&a.published_at))
            .then(a.id.cmp(&b.id))
    });

    let mut picks: Vec<Pick> = related
        .into_iter()
        .take(ctx.related_budget.min(limit))
        .map(|(c, reason, to)| Pick {
            card_id: c.id,
            reason,
            related_to: Some(to),
        })
        .collect();

    let remaining = limit - picks.len();
    if remaining > 0 {
        let taken: HashSet<Uuid> = picks.iter().map(|p| p.card_id).collect();
        // Interest score acts like "importance" inside each category; categories still rotate.
        let general: Vec<PoolCard> = candidates
            .iter()
            .filter(|c| !taken.contains(&c.id))
            .map(|c| PoolCard {
                id: c.id,
                category: c.category.clone(),
                important: c.important || interest(ctx, c) > 0,
                published_at: c.published_at,
            })
            .collect();
        let covered = HashSet::new();
        picks.extend(
            round_robin(general.iter(), &covered, remaining)
                .into_iter()
                .map(|id| Pick {
                    card_id: id,
                    reason: Reason::General,
                    related_to: None,
                }),
        );
    }
    picks
}

fn interest(ctx: &Context, c: &Candidate) -> u32 {
    c.concepts.iter().filter_map(|k| ctx.interests.get(k)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn cand(n: u128, category: &str) -> Candidate {
        Candidate {
            id: id(n),
            category: category.into(),
            important: false,
            published_at: None,
            story_id: None,
            follows_up_card_id: None,
            concepts: vec![],
        }
    }

    fn ctx() -> Context {
        Context {
            dose_cards: vec![
                DoseCard {
                    id: id(1),
                    story_id: Some("nobel".into()),
                    concepts: vec![id(500)],
                },
                DoseCard {
                    id: id(2),
                    story_id: None,
                    concepts: vec![id(501)],
                },
            ],
            read_cards: HashMap::from([(id(1), false), (id(2), false), (id(90), true)]),
            relations: vec![(id(501), id(502))],
            interests: HashMap::new(),
            related_budget: 10,
        }
    }

    #[test]
    fn related_tiers_come_before_general_in_order() {
        let general = cand(10, "news");
        let connected = Candidate {
            concepts: vec![id(502)],
            ..cand(11, "book")
        }; // via relation
        let story = Candidate {
            story_id: Some("nobel".into()),
            ..cand(12, "news")
        };
        let follow = Candidate {
            follows_up_card_id: Some(id(2)),
            ..cand(13, "research")
        };
        let correction = Candidate {
            follows_up_card_id: Some(id(90)),
            ..cand(14, "research")
        };
        let picks = rank(&ctx(), &[general, connected, story, follow, correction], 10);
        let order: Vec<_> = picks.iter().map(|p| (p.card_id, p.reason)).collect();
        assert_eq!(
            order,
            vec![
                (id(13), Reason::FollowUp),
                (id(14), Reason::Correction),
                (id(12), Reason::SameStory),
                (id(11), Reason::Connected),
                (id(10), Reason::General),
            ]
        );
        assert_eq!(picks[3].related_to, Some(id(2)));
    }

    #[test]
    fn related_budget_caps_tiers_one_to_three() {
        let mut c = ctx();
        c.related_budget = 1;
        let a = Candidate {
            concepts: vec![id(500)],
            ..cand(11, "book")
        };
        let b = Candidate {
            concepts: vec![id(501)],
            ..cand(12, "book")
        };
        let g = cand(13, "news");
        let picks = rank(&c, &[a, b, g], 3);
        assert_eq!(picks.iter().filter(|p| p.reason != Reason::General).count(), 1);
        assert_eq!(picks.len(), 3); // the capped related card falls through to general
    }

    #[test]
    fn general_tier_rotates_categories() {
        let mut c = ctx();
        c.dose_cards.clear();
        let cands = [
            cand(10, "news"),
            cand(11, "news"),
            cand(12, "book"),
            cand(13, "research"),
        ];
        let picks = rank(&c, &cands, 3);
        let cats: HashSet<_> = picks
            .iter()
            .map(|p| cands.iter().find(|x| x.id == p.card_id).unwrap().category.clone())
            .collect();
        assert_eq!(cats.len(), 3);
    }

    #[test]
    fn limit_is_respected() {
        let cands: Vec<_> = (10..30).map(|n| cand(n, "news")).collect();
        assert_eq!(rank(&ctx(), &cands, 5).len(), 5);
    }
}
