//! The memory of a person (ADR-0030 section 1): at most `limits.memories` entries sorted by their
//! key, each with a strength that falls linearly per day from the second it was written down. The
//! strength lies as a value, a second and a rate, like a need's (ADR-0026), so nothing ticks: one
//! event per person, `Forget`, stands at the earliest second a strength is 0. Everything is an
//! integer. The state side (`state::memory`) writes entries and plans that event; this module is
//! the pure part: the strength at a second, the second of the zero, insertion with displacement
//! and the freeze on moving out.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::time::{DAY, SimDuration, SimTime};

/// The most a strength can be (ADR-0030 section 1).
pub const MOST: u16 = 1000;

/// The key of a memory: when it happened, its kind, the others involved (ADR-0030 section 1).
/// A list holds each key once, sorted by it; `None` sorts before any `Some`.
pub type MemoryKey = (SimTime, u16, [Option<SimId>; 2]);

/// A memory of a person: `kind` the number of its kind in the data, `with` the other persons
/// involved in ascending order (a single one first, then `None`), `at` when it happened,
/// `valence` from -1000 to 1000, `strength` from 0 to 1000 as it was at `since`, `fade` the
/// loss per day. It never points to the domain log (ADR-0004: the simulation does not read its
/// output).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub kind: u16,
    pub with: [Option<SimId>; 2],
    pub at: SimTime,
    pub valence: i16,
    pub strength: u16,
    pub since: SimTime,
    pub fade: u16,
}

impl Memory {
    /// The key `(at, kind, with)`.
    pub fn key(&self) -> MemoryKey {
        (self.at, self.kind, self.with)
    }

    /// The strength at `t`: `max(0, strength - fade · (t - since) / 86 400)` in `i64`, the
    /// division truncating toward 0. At or before `since` it is the strength as written.
    pub fn strength_at(&self, t: SimTime) -> u16 {
        let elapsed = (t.as_secs() - self.since.as_secs()).max(0);
        let lost = i64::from(self.fade) * elapsed / DAY.as_secs();
        let left = (i64::from(self.strength) - lost).max(0);
        u16::try_from(left).unwrap_or_else(|_| panic!("invariant memory.strength"))
    }

    /// The first second the strength is 0, `since + ceil(strength · 86 400 / fade)`; `None` for a
    /// memory that never fades and for one whose zero lies beyond the end of time.
    pub fn zero_second(&self) -> Option<SimTime> {
        if self.fade == 0 {
            return None;
        }
        let fade = i64::from(self.fade);
        let secs = (i64::from(self.strength) * DAY.as_secs() + fade - 1) / fade;
        self.since.checked_add(SimDuration::from_secs(secs))
    }

    /// The order in which memories fall when the list is full, the first falls first: the weaker
    /// at `now`, then the older `at`, then the smaller key.
    fn falls(&self, now: SimTime) -> (u16, SimTime, MemoryKey) {
        (self.strength_at(now), self.at, self.key())
    }
}

/// The second of the one open `Forget` of a list: the earliest zero of its memories; `None` if
/// none of them fades to 0 within time.
pub fn next_forget(list: &[Memory]) -> Option<SimTime> {
    list.iter().filter_map(Memory::zero_second).min()
}

/// What an insertion did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Insertion {
    /// The memory was taken in; the list had room.
    Added,
    /// The memory was taken in and this one fell out.
    Displaced(Memory),
    /// The list was full and the newcomer was the one to fall: nothing changed.
    Refused,
}

/// Takes `new` into the sorted `list` at the second `now` (`new.since == now`); with `cap` entries
/// in it, the weakest at `now` of the old ones and the new one falls (ADR-0030 section 1). The
/// newcomer counts: if it is the weakest, then the older `at`, then the smaller key, it is not
/// taken in.
///
/// # Panics
///
/// Invariant `memory.double_key` if the list holds the key of `new`.
#[track_caller]
pub fn insert(list: &mut Vec<Memory>, new: Memory, now: SimTime, cap: usize) -> Insertion {
    let at = match list.binary_search_by_key(&new.key(), Memory::key) {
        Ok(_) => panic!("invariant memory.double_key"),
        Err(at) => at,
    };
    if list.len() < cap {
        list.insert(at, new);
        return Insertion::Added;
    }
    let weakest = list.iter().enumerate().min_by_key(|(_, m)| m.falls(now));
    let Some((fell, old)) = weakest.filter(|(_, old)| old.falls(now) < new.falls(now)) else {
        return Insertion::Refused;
    };
    let old = *old;
    list.remove(fell);
    let at = list
        .binary_search_by_key(&new.key(), Memory::key)
        .unwrap_or_else(|at| at);
    list.insert(at, new);
    Insertion::Displaced(old)
}

/// What a person keeps on moving out (ADR-0030 section 1, "Auszug und Tod"): the `keep` strongest
/// in the order of falling (the strongest first), frozen with their strength at `now` and
/// `since = now`, sorted by key again.
pub fn keep_on_move_out(list: &[Memory], keep: usize, now: SimTime) -> Vec<Memory> {
    let mut ranked: Vec<&Memory> = list.iter().collect();
    ranked.sort_by_key(|m| core::cmp::Reverse(m.falls(now)));
    let mut kept: Vec<Memory> = (ranked.into_iter().take(keep))
        .map(|m| Memory {
            strength: m.strength_at(now),
            since: now,
            ..*m
        })
        .collect();
    kept.sort_by_key(Memory::key);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    /// A memory of kind `kind` at second `at`, written down at `since` with `strength` and the
    /// loss `fade` per day, as the reference builds one (`mem` of tools/social_reference.py).
    fn mem(kind: u16, at: i64, strength: u16, since: i64, fade: u16) -> Memory {
        Memory {
            kind,
            with: [None, None],
            at: t(at),
            valence: 0,
            strength,
            since: t(since),
            fade,
        }
    }

    /// ADR-0030 acceptance test 1, from `tools/social_reference.py --check`: 800 at 100 per day is
    /// 400 after 4 days and 0 after exactly 691 200 s, one second before it 1.
    #[test]
    fn a_strength_falls_to_zero_in_the_pinned_second() {
        let m = mem(0, 0, 800, 1000, 100);
        assert_eq!(m.strength_at(t(1000)), 800);
        assert_eq!(m.strength_at(t(1000 + 4 * 86_400)), 400);
        assert_eq!(m.strength_at(t(1000 + 691_199)), 1);
        assert_eq!(m.strength_at(t(1000 + 691_200)), 0);
        assert_eq!(m.zero_second(), Some(t(692_200)));
        assert_eq!(m.strength_at(t(1000 + 700_000)), 0);
    }

    /// The zero is the first second at 0 (ceil), for 1000 at 7 per day: 12 342 858 (reference).
    #[test]
    fn the_zero_second_is_the_first_second_at_zero() {
        let m = mem(0, 0, 1000, 0, 7);
        assert_eq!(m.zero_second(), Some(t(12_342_858)));
        assert_eq!(m.strength_at(t(12_342_857)), 1);
        assert_eq!(m.strength_at(t(12_342_858)), 0);
    }

    #[test]
    fn a_memory_without_fade_never_reaches_zero() {
        let m = mem(0, 0, 500, 0, 0);
        assert_eq!(m.zero_second(), None);
        assert_eq!(m.strength_at(t(1_000_000_000)), 500);
        assert_eq!(next_forget(&[m, mem(1, 0, 400, 0, 0)]), None);
    }

    /// A zero beyond the end of time never comes: no second to plan.
    #[test]
    fn a_zero_beyond_the_end_of_time_has_no_second() {
        let last = crate::time::MAX_ABS_SECS;
        assert_eq!(mem(0, 0, 1000, last - 5, 1).zero_second(), None);
        assert_eq!(
            mem(0, 0, 1, last - 100, 1000).zero_second(),
            Some(t(last - 13))
        );
    }

    /// The one `Forget` stands at the earliest zero of the list (reference: 86 400).
    #[test]
    fn the_forget_stands_at_the_earliest_zero() {
        let list = [mem(0, 0, 800, 1000, 100), mem(1, 0, 100, 0, 100)];
        assert_eq!(next_forget(&list), Some(t(86_400)));
        assert_eq!(next_forget(&[]), None);
    }

    fn full(special: &[(u16, u16, i64)]) -> Vec<Memory> {
        // 32 memories of strength 500, at `100 + k`, as `full` of the reference; `special` swaps
        // `(kind, strength, at)`.
        let mut list: Vec<Memory> = (0..32_u16)
            .map(|kind| {
                let (strength, at) = special
                    .iter()
                    .find(|(k, _, _)| *k == kind)
                    .map_or((500, 100 + i64::from(kind)), |&(_, s, a)| (s, a));
                mem(kind, at, strength, 0, 0)
            })
            .collect();
        list.sort_by_key(Memory::key);
        list
    }

    fn kinds(list: &[Memory]) -> Vec<u16> {
        list.iter().map(|m| m.kind).collect()
    }

    /// The reference's eviction checks (`check_memory`): the weakest falls, a tie goes to the older
    /// `at`, then to the smaller key; a weaker newcomer is not taken in.
    #[test]
    fn the_33rd_memory_displaces_the_weakest_by_the_rule() {
        let now = t(10_000);
        let new = |kind: u16, at: i64, strength: u16| mem(kind, at, strength, 10_000, 0);
        let mut list = full(&[(5, 100, 50)]);
        let outcome = insert(&mut list, new(40, 10_000, 600), now, 32);
        assert!(matches!(outcome, Insertion::Displaced(old) if old.kind == 5));
        assert_eq!(list.len(), 32);
        assert!(list.windows(2).all(|pair| pair[0].key() < pair[1].key()));
        assert!(list.iter().any(|m| m.kind == 40));

        let mut list = full(&[(5, 100, 60), (9, 100, 50)]);
        let outcome = insert(&mut list, new(40, 10_000, 600), now, 32);
        assert!(
            matches!(outcome, Insertion::Displaced(old) if old.kind == 9),
            "older at"
        );

        let mut list = full(&[(7, 100, 50), (3, 100, 50)]);
        let outcome = insert(&mut list, new(40, 10_000, 600), now, 32);
        assert!(
            matches!(outcome, Insertion::Displaced(old) if old.kind == 3),
            "smaller key"
        );

        let before = full(&[(5, 100, 50)]);
        let mut list = before.clone();
        assert_eq!(
            insert(&mut list, new(40, 10_000, 50), now, 32),
            Insertion::Refused
        );
        assert_eq!(list, before);
        // A tie with the weakest: the newcomer with the older `at` is not taken in, the one with
        // the younger `at` is.
        let mut list = before.clone();
        assert_eq!(
            insert(&mut list, new(40, 40, 100), now, 32),
            Insertion::Refused
        );
        let mut list = before.clone();
        let outcome = insert(&mut list, new(40, 70, 100), now, 32);
        assert!(matches!(outcome, Insertion::Displaced(old) if old.kind == 5));
    }

    /// The strength counts at `now`: 500 written at 0 with 100 per day is 100 after 4 days and
    /// falls before the 300s (reference "evict by faded strength at now").
    #[test]
    fn displacement_reads_the_strength_at_now() {
        let mut list: Vec<Memory> = core::iter::once(mem(0, 0, 500, 0, 100))
            .chain((1..32).map(|k| mem(k, i64::from(k), 300, 0, 0)))
            .collect();
        list.sort_by_key(Memory::key);
        let now = t(4 * 86_400);
        let outcome = insert(
            &mut list,
            mem(50, now.as_secs(), 700, now.as_secs(), 0),
            now,
            32,
        );
        assert!(matches!(outcome, Insertion::Displaced(old) if old.kind == 0));
    }

    /// With room, nothing falls.
    #[test]
    fn a_list_with_room_takes_everything_in() {
        let now = t(4 * 86_400);
        let mut list: Vec<Memory> = (1..32).map(|k| mem(k, i64::from(k), 300, 0, 0)).collect();
        let outcome = insert(&mut list, mem(50, 99, 1, now.as_secs(), 0), now, 32);
        assert_eq!(outcome, Insertion::Added);
        assert_eq!(list.len(), 32);
        assert_eq!(kinds(&list).last(), Some(&50));
    }

    #[test]
    #[should_panic(expected = "invariant memory.double_key")]
    fn a_key_twice_is_an_invariant_break() {
        let mut list = vec![mem(1, 5, 100, 0, 0)];
        insert(&mut list, mem(1, 5, 200, 0, 0), t(0), 32);
    }

    /// Reference `keep_on_move_out`: of twelve with strength 100 · (k % 10 + 1) at 10 per day, the
    /// eight strongest at one day stay, frozen at their strength then (kind 9: 1000 - 10 = 990).
    #[test]
    fn moving_out_keeps_the_eight_strongest_frozen() {
        let list: Vec<Memory> = (0..12_u16)
            .map(|k| mem(k, i64::from(k), 100 * (k % 10 + 1), 0, 10))
            .collect();
        let now = t(86_400);
        let kept = keep_on_move_out(&list, 8, now);
        assert_eq!(kinds(&kept), [2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(kept.iter().all(|m| m.since == now));
        assert_eq!(
            kept.iter().find(|m| m.kind == 9).map(|m| m.strength),
            Some(990)
        );
        assert_eq!(
            kept.iter().find(|m| m.kind == 2).map(|m| m.strength),
            Some(290)
        );
    }

    /// A shorter list is kept whole, still frozen at its strength then.
    #[test]
    fn a_short_list_is_frozen_whole() {
        let list = [mem(0, 0, 500, 0, 100), mem(1, 5, 300, 0, 0)];
        let kept = keep_on_move_out(&list, 8, t(2 * 86_400));
        assert_eq!(kept.len(), 2);
        assert_eq!((kept[0].strength, kept[0].since), (300, t(2 * 86_400)));
        assert_eq!((kept[1].strength, kept[1].since), (300, t(2 * 86_400)));
    }
}
