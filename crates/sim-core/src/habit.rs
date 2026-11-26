//! Habits in the choice (ADR-0033 sections 7 and 10): per person and key (furniture kind, offer
//! index, quarter of the day of the choice) a strength from 0 to 1000, stored as a value and the
//! second it was written. Each end with `done` or `full` not chosen by a command adds 100; the
//! strength decays by 20 a day toward 0, analytically, without an event. A person holds at most 8;
//! a full list drops the weakest. By day a habit gives its offer `15 · strength` points.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::log::EndReason;
use crate::time::{DAY, HOUR, SimTime};

/// The most habits a person holds.
pub const MAX_HABITS: usize = 8;

/// The strength an end adds.
pub const STEP: u16 = 100;

/// The strongest a habit gets.
pub const MAX_STRENGTH: u16 = 1000;

/// The strength a habit loses per day.
const DECAY_PER_DAY: i64 = 20;

/// The points per unit of strength.
const POINTS_PER_STRENGTH: i64 = 15;

/// A habit's key: the furniture kind's data id, the offer index and the quarter of the day, in
/// this order (ADR-0033 section 10).
pub type Key<'a> = (&'a DataId, u16, u8);

/// One habit: its key, the value written down and the second it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    pub kind: DataId,
    pub offer: u16,
    pub quarter: u8,
    pub value: u16,
    pub since: SimTime,
}

impl Habit {
    pub fn key(&self) -> Key<'_> {
        (&self.kind, self.offer, self.quarter)
    }

    /// The strength at `t`: the value less `20 · (t − since) / 86400` (toward 0), never below 0.
    pub fn strength(&self, t: SimTime) -> u16 {
        let lost = DECAY_PER_DAY * (t.as_secs() - self.since.as_secs()) / DAY.as_secs();
        let left = (i64::from(self.value) - lost).max(0);
        u16::try_from(left).unwrap_or_else(|_| panic!("invariant habit.strength"))
    }
}

/// The quarter of the day of `at`: 0 to 3 from 0, 6, 12 and 18 o'clock.
pub fn quarter(at: SimTime) -> u8 {
    let quarter = at.as_secs().rem_euclid(DAY.as_secs()) / (6 * HOUR.as_secs());
    u8::try_from(quarter).unwrap_or_else(|_| panic!("invariant habit.quarter"))
}

/// The strength of the habit `key` among `habits` at `t`, 0 without one.
pub fn strength(habits: &[Habit], key: Key<'_>, t: SimTime) -> u16 {
    (habits.iter())
        .find(|habit| habit.key() == key)
        .map_or(0, |habit| habit.strength(t))
}

/// The points a habit of `strength` gives its offer.
pub fn points(strength: u16) -> i64 {
    POINTS_PER_STRENGTH * i64::from(strength)
}

/// Whether an action's end strengthens its habit: `done` or `full`, and not chosen by a command.
pub fn completes(reason: EndReason, commanded: bool) -> bool {
    matches!(reason, EndReason::Done | EndReason::Full) && !commanded
}

/// The habit `key` completed at `t` (ADR-0033 section 10): the habits at strength 0 go first;
/// with a full list and a new key the weakest falls (the smallest key on a tie); the key is
/// written down with its strength at `t` plus 100, at most 1000, and the second `t`. Every other
/// habit keeps its value and second. The list stays in the order of the keys.
pub fn complete(habits: &mut Vec<Habit>, key: Key<'_>, t: SimTime) {
    habits.retain(|habit| habit.strength(t) > 0);
    let value = strength(habits, key, t)
        .saturating_add(STEP)
        .min(MAX_STRENGTH);
    let at = habits.binary_search_by(|habit| habit.key().cmp(&key));
    if at.is_err() && habits.len() >= MAX_HABITS {
        let weakest = (0..habits.len())
            .min_by_key(|&n| (habits[n].strength(t), habits[n].key()))
            .unwrap_or_else(|| panic!("invariant habit.full"));
        habits.remove(weakest);
    }
    let habit = Habit {
        kind: key.0.clone(),
        offer: key.1,
        quarter: key.2,
        value,
        since: t,
    };
    match habits.binary_search_by(|habit| habit.key().cmp(&key)) {
        Ok(at) => habits[at] = habit,
        Err(at) => habits.insert(at, habit),
    }
}

/// Whether `habits` could be a person's at `now`: at most 8, in the strict order of their keys,
/// each of a quarter 0 to 3, a value from 100 to 1000 and written down at or before `now`.
pub fn consistent(habits: &[Habit], now: SimTime) -> bool {
    let ordered = habits.windows(2).all(|pair| pair[0].key() < pair[1].key());
    let each = habits.iter().all(|habit| {
        habit.quarter < 4 && (STEP..=MAX_STRENGTH).contains(&habit.value) && habit.since <= now
    });
    habits.len() <= MAX_HABITS && ordered && each
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    /// The kinds `test:k1` to `test:k9`, then `test:seven` and `test:z`.
    fn kinds() -> [DataId; 11] {
        [
            "test:k1",
            "test:k2",
            "test:k3",
            "test:k4",
            "test:k5",
            "test:k6",
            "test:k7",
            "test:k8",
            "test:k9",
            "test:seven",
            "test:z",
        ]
        .map(|text| DataId::new(text).unwrap_or_else(|_| panic!("valid")))
    }

    fn at(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    // Expected values: tools/development_reference.py, "Habits (7)".
    #[test]
    fn strength_decays_and_completes_as_the_reference() {
        let kind = kinds()[9].clone();
        let key = (&kind, 0, quarter(at(25_200)));
        let mut habits = Vec::new();
        assert_eq!(strength(&habits, key, at(25_200)), 0);
        complete(&mut habits, key, at(25_200));
        assert_eq!(strength(&habits, key, at(25_200)), 100);
        assert_eq!(strength(&habits, key, at(25_200 + 2 * 86_400)), 60);
        complete(&mut habits, key, at(25_200 + 2 * 86_400));
        assert_eq!(strength(&habits, key, at(25_200 + 2 * 86_400)), 160);
        assert_eq!(points(160), 2400);
        let at_zero = |value, since| Habit {
            kind: kind.clone(),
            offer: 0,
            quarter: 1,
            value,
            since: at(since),
        };
        assert_eq!(at_zero(100, 0).strength(at(4_319)), 100);
        assert_eq!(at_zero(100, 0).strength(at(4_320)), 99);
        assert_eq!(at_zero(100, 0).strength(at(5 * 86_400)), 0);
        assert_eq!(
            [0, 21_599, 21_600, 86_399].map(|s| quarter(at(s))),
            [0, 0, 1, 3]
        );
    }

    // Expected values: tools/development_reference.py, "At most 8".
    #[test]
    fn a_full_list_drops_the_weakest_then_the_smallest_key() {
        let kinds = kinds();
        let mut habits: Vec<Habit> = (kinds.iter().zip(1..=8))
            .map(|(kind, n)| Habit {
                kind: kind.clone(),
                offer: 0,
                quarter: 0,
                value: 100 * n,
                since: at(0),
            })
            .collect();
        assert!(consistent(&habits, at(0)));
        complete(&mut habits, (&kinds[8], 0, 0), at(0));
        assert_eq!(habits.len(), 8);
        assert_eq!(habits[0].kind, kinds[1]);
        assert_eq!((habits[7].value, habits[7].since), (100, at(0)));
        let tie = |offer, quarter| Habit {
            kind: kinds[2].clone(),
            offer,
            quarter,
            value: 300,
            since: at(0),
        };
        let mut tied = vec![tie(0, 1), tie(1, 0)];
        tied.extend((3..9).map(|n| Habit {
            kind: kinds[n].clone(),
            ..tie(0, 0)
        }));
        assert!(consistent(&tied, at(0)));
        complete(&mut tied, (&kinds[10], 0, 0), at(0));
        assert!(!tied.contains(&tie(0, 1)) && tied.contains(&tie(1, 0)));
        assert!(!consistent(&[tied[0].clone(), tied[0].clone()], at(0)));
    }
}
