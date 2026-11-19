//! Daily dose assembly (architecture §6.2, ADR-013/014/017).
//!
//! Pure function: the caller passes only *unread, published* cards; this module decides
//! which of them form today's dose and in what order.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PoolCard {
    pub id: Uuid,
    pub category: String,
    pub important: bool,
    pub published_at: Option<DateTime<Utc>>,
}

/// A published Connection card whose bridged cards are also published and unread.
#[derive(Debug, Clone)]
pub struct ConnectionCandidate {
    pub id: Uuid,
    pub left: Uuid,
    pub right: Uuid,
    pub important: bool,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Assembled {
    /// Card ids in display order. If `ends_with_connection`, the last one is the Connection card.
    pub card_ids: Vec<Uuid>,
    pub ends_with_connection: bool,
}

/// Build a dose of up to `size` cards.
///
/// 1. Connection-first: pick the best Connection card whose bridged cards are in the pool.
/// 2. Fill the remaining slots round-robin across categories, preferring categories the
///    bridged cards don't already cover (diversity floor).
/// 3. Order: filler, left, filler, right, remaining fillers, Connection card last.
///
/// If no Connection card is usable the dose has no Connection card rather than repeating a
/// read card (§6.2); the caller raises an editorial alert.
pub fn assemble(connections: &[ConnectionCandidate], pool: &[PoolCard], size: usize) -> Assembled {
    let in_pool: HashSet<Uuid> = pool.iter().map(|c| c.id).collect();
    let mut usable: Vec<&ConnectionCandidate> = connections
        .iter()
        .filter(|c| in_pool.contains(&c.left) && in_pool.contains(&c.right) && c.left != c.right)
        .collect();
    usable.sort_by(|a, b| {
        b.important
            .cmp(&a.important)
            .then(b.published_at.cmp(&a.published_at))
            .then(a.id.cmp(&b.id))
    });
    let connection = usable.first().copied();

    let bridged: Vec<Uuid> = connection.map(|c| vec![c.left, c.right]).unwrap_or_default();
    let filler_slots = match connection {
        Some(_) => size.saturating_sub(3),
        None => size,
    };
    let covered: HashSet<&str> = pool
        .iter()
        .filter(|c| bridged.contains(&c.id))
        .map(|c| c.category.as_str())
        .collect();
    let fillers = round_robin(pool.iter().filter(|c| !bridged.contains(&c.id)), &covered, filler_slots);

    let mut card_ids = Vec::with_capacity(size);
    let mut fillers = fillers.into_iter();
    match connection {
        Some(conn) => {
            card_ids.extend(fillers.next());
            card_ids.push(conn.left);
            card_ids.extend(fillers.next());
            card_ids.push(conn.right);
            card_ids.extend(fillers);
            card_ids.push(conn.id);
        }
        None => card_ids.extend(fillers),
    }
    Assembled {
        card_ids,
        ends_with_connection: connection.is_some(),
    }
}

/// Pick up to `n` cards, one category at a time. Categories not in `covered` go first; within
/// a category, important and newer cards go first.
pub fn round_robin<'a>(cards: impl Iterator<Item = &'a PoolCard>, covered: &HashSet<&str>, n: usize) -> Vec<Uuid> {
    let mut by_category: BTreeMap<&str, Vec<&PoolCard>> = BTreeMap::new();
    for card in cards {
        by_category.entry(card.category.as_str()).or_default().push(card);
    }
    for group in by_category.values_mut() {
        group.sort_by(|a, b| {
            b.important
                .cmp(&a.important)
                .then(b.published_at.cmp(&a.published_at))
                .then(a.id.cmp(&b.id))
        });
        group.reverse(); // so `pop()` yields the best card
    }
    let mut order: Vec<&str> = by_category.keys().copied().collect();
    order.sort_by_key(|cat| covered.contains(cat)); // uncovered categories first (stable)

    let mut picked = Vec::with_capacity(n);
    while picked.len() < n {
        let mut progressed = false;
        for cat in &order {
            if picked.len() == n {
                break;
            }
            if let Some(card) = by_category.get_mut(cat).and_then(Vec::pop) {
                picked.push(card.id);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn card(n: u128, category: &str, day: u32) -> PoolCard {
        PoolCard {
            id: Uuid::from_u128(n),
            category: category.into(),
            important: false,
            published_at: Some(Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()),
        }
    }

    fn conn(n: u128, left: u128, right: u128) -> ConnectionCandidate {
        ConnectionCandidate {
            id: Uuid::from_u128(n),
            left: Uuid::from_u128(left),
            right: Uuid::from_u128(right),
            important: false,
            published_at: None,
        }
    }

    fn pool() -> Vec<PoolCard> {
        vec![
            card(1, "research", 1),
            card(2, "research", 2),
            card(3, "book", 3),
            card(4, "news", 4),
            card(5, "evergreen", 5),
            card(6, "book", 6),
            card(7, "news", 7),
        ]
    }

    #[test]
    fn connection_card_is_last_and_bridged_cards_come_earlier() {
        let dose = assemble(&[conn(100, 1, 3)], &pool(), 6);
        assert!(dose.ends_with_connection);
        assert_eq!(dose.card_ids.len(), 6);
        assert_eq!(*dose.card_ids.last().unwrap(), Uuid::from_u128(100));
        let pos = |n| dose.card_ids.iter().position(|id| *id == Uuid::from_u128(n)).unwrap();
        assert!(pos(1) < 5 && pos(3) < 5);
        // filler, left, filler, right, ...
        assert_eq!(dose.card_ids[1], Uuid::from_u128(1));
        assert_eq!(dose.card_ids[3], Uuid::from_u128(3));
    }

    #[test]
    fn no_card_appears_twice() {
        let dose = assemble(&[conn(100, 1, 3)], &pool(), 7);
        let unique: HashSet<_> = dose.card_ids.iter().collect();
        assert_eq!(unique.len(), dose.card_ids.len());
    }

    #[test]
    fn fillers_prefer_categories_the_bridged_cards_do_not_cover() {
        // Bridged: research + book. Three fillers should be news, evergreen, then a repeat category.
        let dose = assemble(&[conn(100, 1, 3)], &pool(), 6);
        let fillers: Vec<_> = [0usize, 2, 4].iter().map(|i| dose.card_ids[*i]).collect();
        let cats: Vec<_> = fillers
            .iter()
            .map(|id| pool().into_iter().find(|c| c.id == *id).unwrap().category)
            .collect();
        assert!(cats.contains(&"news".to_string()));
        assert!(cats.contains(&"evergreen".to_string()));
    }

    #[test]
    fn connection_with_missing_bridged_card_is_skipped() {
        // Card 99 is not in the pool (already read), so connection 100 is unusable.
        let dose = assemble(&[conn(100, 1, 99), conn(101, 2, 4)], &pool(), 6);
        assert_eq!(*dose.card_ids.last().unwrap(), Uuid::from_u128(101));
    }

    #[test]
    fn without_a_usable_connection_the_dose_has_none() {
        let dose = assemble(&[], &pool(), 6);
        assert!(!dose.ends_with_connection);
        assert_eq!(dose.card_ids.len(), 6);
    }

    #[test]
    fn small_pool_gives_a_short_dose() {
        let dose = assemble(&[conn(100, 1, 3)], &pool()[..3], 6);
        assert_eq!(dose.card_ids.len(), 4); // filler 2, left 1, right 3, connection
        assert!(dose.ends_with_connection);
    }
}
