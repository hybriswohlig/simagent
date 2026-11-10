//! A bond of a person (ADR-0030 section 2): at most `limits.bonds` directed bonds, sorted by
//! the other person. Familiarity and grudge fall per day from the second they were written
//! down, analytically, with no event. Affection, trust, respect and attraction stay until a
//! later slice changes them. Everything is an integer. The state side (`state::bond`) forms
//! bonds; this module is the pure part: the values at a second, settling them, a clamped
//! step, the first matching role, insertion with eviction, and the choice's company and avoid.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::social::data::BondStarts;
use crate::time::{DAY, SimTime};

/// The most a bond value can be (ADR-0030 section 2).
pub const MOST: u16 = 1000;

/// The index of affection in [`Bond::values`].
pub const AFFECTION: usize = 0;
/// The index of trust.
pub const TRUST: usize = 1;
/// The index of respect.
pub const RESPECT: usize = 2;
/// The index of attraction.
pub const ATTRACTION: usize = 3;
/// The index of familiarity, which falls per day.
pub const FAMILIARITY: usize = 4;
/// The index of grudge, which falls per day.
pub const GRUDGE: usize = 5;

/// The first matching role of the other person, in the order of the start table (ADR-0030
/// section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Partner,
    Parent,
    Child,
    Sibling,
    Kin,
    Housemate,
}

/// A bond to `other`: the six values as they were at `since`, each from 0 to 1000, in the
/// order affection, trust, respect, attraction, familiarity, grudge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bond {
    pub other: SimId,
    pub values: [u16; 6],
    pub since: SimTime,
}

impl Bond {
    /// The six values at `t`. Familiarity and grudge fall by `familiarity_fade` and
    /// `grudge_fade` per day: `max(0, value - fade · (t - since) / 86 400)` in `i64`, the
    /// division truncating toward 0. The other four stay. At or before `since` the values
    /// are the ones written down.
    pub fn values_at(&self, t: SimTime, familiarity_fade: u16, grudge_fade: u16) -> [u16; 6] {
        let elapsed = (t.as_secs() - self.since.as_secs()).max(0);
        let mut values = self.values;
        values[FAMILIARITY] = faded(values[FAMILIARITY], familiarity_fade, elapsed);
        values[GRUDGE] = faded(values[GRUDGE], grudge_fade, elapsed);
        values
    }

    /// Writes familiarity and grudge at `now` and sets `since` to `now`. A later slice calls
    /// this before it changes a bond; moving out calls it so the two do not fall while the
    /// person is outside.
    pub fn settle(&mut self, now: SimTime, familiarity_fade: u16, grudge_fade: u16) {
        self.values = self.values_at(now, familiarity_fade, grudge_fade);
        self.since = now;
    }

    /// Writes both fading values at `now`, then adds each step and clamps to 0..=1000.
    /// Returns the six values at `now` before the steps. One call is one `core:bond_changed`
    /// (before, after) when the talk writes the entry.
    pub fn apply(
        &mut self,
        now: SimTime,
        familiarity_fade: u16,
        grudge_fade: u16,
        steps: &[(usize, i16)],
    ) -> [u16; 6] {
        let before = self.values_at(now, familiarity_fade, grudge_fade);
        let mut values = before;
        for &(index, delta) in steps {
            let next = i32::from(values[index]) + i32::from(delta);
            let clamped = next.clamp(0, i32::from(MOST));
            values[index] =
                u16::try_from(clamped).unwrap_or_else(|_| panic!("invariant bond.value"));
        }
        self.values = values;
        self.since = now;
        before
    }
}

/// `max(0, value - fade · elapsed / 86 400)`.
fn faded(value: u16, fade: u16, elapsed: i64) -> u16 {
    let lost = i64::from(fade) * elapsed / DAY.as_secs();
    let left = (i64::from(value) - lost).max(0);
    u16::try_from(left).unwrap_or_else(|_| panic!("invariant bond.value"))
}

/// The first matching role (ADR-0030 section 2). `kin` is φ at least 1/8, which the caller
/// judges; [`kin_at_eighth`] is that comparison on a fraction.
pub fn start_role(
    partner: bool,
    other_is_my_parent: bool,
    other_is_my_child: bool,
    sibling: bool,
    kin: bool,
) -> Role {
    if partner {
        Role::Partner
    } else if other_is_my_parent {
        Role::Parent
    } else if other_is_my_child {
        Role::Child
    } else if sibling {
        Role::Sibling
    } else if kin {
        Role::Kin
    } else {
        Role::Housemate
    }
}

/// Whether the fraction `num`/`den` is at least 1/8. A zero denominator is not.
pub fn kin_at_eighth(num: u128, den: u128) -> bool {
    den != 0 && num.saturating_mul(8) >= den
}

/// The start values of `role` from the data table, in the bond's order.
pub fn values_for(starts: &BondStarts, role: Role) -> [u16; 6] {
    match role {
        Role::Partner => starts.partner,
        Role::Parent => starts.parent,
        Role::Child => starts.child,
        Role::Sibling => starts.sibling,
        Role::Kin => starts.kin,
        Role::Housemate => starts.housemate,
    }
}

/// What forming a bond did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formation {
    /// The bond was formed. `dropped` is the other person of the bond that fell out of a
    /// full list, if one did.
    Formed { dropped: Option<SimId> },
    /// The list was full and every bond in it is protected: the new bond was not formed.
    Refused,
}

/// Takes `new` into the sorted `list` (`new.since == now`). A full list drops the bond with
/// the smallest affection + familiarity at `now`, never one `protected` calls true for (a
/// housemate, or kin from φ = 1/8); a tie drops the larger [`SimId`]. The new bond does not
/// compete: only a bond already in the list can fall. If none can, the list is unchanged.
///
/// # Panics
///
/// Invariant `bond.double` if the list already holds `new.other`.
#[track_caller]
pub fn insert(
    list: &mut Vec<Bond>,
    new: Bond,
    now: SimTime,
    familiarity_fade: u16,
    grudge_fade: u16,
    cap: usize,
    protected: impl Fn(SimId) -> bool,
) -> Formation {
    if list.iter().any(|bond| bond.other == new.other) {
        panic!("invariant bond.double");
    }
    let mut dropped = None;
    if list.len() >= cap {
        let victim = (list.iter().enumerate())
            .filter(|(_, bond)| !protected(bond.other))
            .min_by_key(|(_, bond)| {
                let values = bond.values_at(now, familiarity_fade, grudge_fade);
                (
                    u32::from(values[AFFECTION]) + u32::from(values[FAMILIARITY]),
                    core::cmp::Reverse(bond.other),
                )
            });
        let Some((index, _)) = victim else {
            return Formation::Refused;
        };
        dropped = Some(list[index].other);
        list.remove(index);
    }
    let at = list
        .binary_search_by_key(&new.other, |bond| bond.other)
        .unwrap_or_else(|at| at);
    list.insert(at, new);
    Formation::Formed { dropped }
}

/// Company and avoid for one piece of furniture (ADR-0030 section 4). `avoid` is positive and
/// is subtracted. A row counts when `liked` or `avoided` is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Company {
    pub liked: Option<SimId>,
    pub company: i64,
    pub avoided: Option<SimId>,
    pub avoid: i64,
}

/// The two components for the people `present` at one sociable furniture. `edge` is the
/// chooser's bond to one of them at this second, the six values, or none. `liked` is the one
/// whose affection plus familiarity is at least `min_sum` and the largest such sum, then the
/// smaller id. `avoided` is the one whose trust is below `below` and the smallest such trust,
/// then the smaller id. Each counts at most once. A person with no bond is skipped. `company`
/// and `avoid` are the points from the data.
pub fn choice_bonus(
    present: &[SimId],
    edge: impl Fn(SimId) -> Option<[u16; 6]>,
    (company, avoid, min_sum, below): (i64, i64, u16, u16),
) -> Company {
    let mut there = present.to_vec();
    there.sort_unstable();
    there.dedup();
    let (mut liked, mut best_sum) = (None, 0_u32);
    let (mut avoided, mut worst_trust) = (None, 0_u16);
    for other in there {
        let Some(values) = edge(other) else {
            continue;
        };
        let sum = u32::from(values[AFFECTION]) + u32::from(values[FAMILIARITY]);
        if sum >= u32::from(min_sum) && liked.is_none_or(|_| sum > best_sum) {
            liked = Some(other);
            best_sum = sum;
        }
        let trust = values[TRUST];
        if trust < below && avoided.is_none_or(|_| trust < worst_trust) {
            avoided = Some(other);
            worst_trust = trust;
        }
    }
    Company {
        liked,
        company: if liked.is_some() { company } else { 0 },
        avoided,
        avoid: if avoided.is_some() { avoid } else { 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    fn bond(other: u64, values: [u16; 6], since: i64) -> Bond {
        Bond {
            other: id(other),
            values,
            since: t(since),
        }
    }

    /// The start table and the role order, from `tools/social_reference.py` `check_bonds`.
    #[test]
    fn the_first_matching_role_wins_and_kin_starts_at_one_eighth() {
        assert_eq!(
            start_role(true, false, false, false, true),
            Role::Partner,
            "partner beats kin"
        );
        assert_eq!(
            start_role(false, true, false, true, true),
            Role::Parent,
            "parent beats sibling"
        );
        assert_eq!(start_role(false, false, true, false, true), Role::Child);
        assert_eq!(start_role(false, false, false, true, true), Role::Sibling);
        assert_eq!(start_role(false, false, false, false, true), Role::Kin);
        assert_eq!(
            start_role(false, false, false, false, false),
            Role::Housemate
        );
        assert!(kin_at_eighth(1, 8));
        assert!(!kin_at_eighth(1, 16));
        assert!(!kin_at_eighth(1, 0));
    }

    /// Each role reads its own row of the table.
    #[test]
    fn each_role_reads_its_row() {
        let starts = BondStarts {
            partner: [1, 0, 0, 0, 0, 0],
            parent: [2, 0, 0, 0, 0, 0],
            child: [3, 0, 0, 0, 0, 0],
            sibling: [4, 0, 0, 0, 0, 0],
            kin: [5, 0, 0, 0, 0, 0],
            housemate: [6, 0, 0, 0, 0, 0],
        };
        let rows = [
            (Role::Partner, 1),
            (Role::Parent, 2),
            (Role::Child, 3),
            (Role::Sibling, 4),
            (Role::Kin, 5),
            (Role::Housemate, 6),
        ];
        for (role, first) in rows {
            assert_eq!(values_for(&starts, role)[AFFECTION], first);
        }
    }

    /// Familiarity falls by 5 a day and grudge by 10, truncating, and both stop at 0
    /// (`check_bonds`).
    #[test]
    fn familiarity_and_grudge_fall_analytically() {
        let sample = bond(7, [500, 500, 500, 0, 600, 300], 0);
        let day = DAY.as_secs();
        assert_eq!(
            sample.values_at(t(10 * day), 5, 10),
            [500, 500, 500, 0, 550, 200]
        );
        assert_eq!(sample.values_at(t(1), 5, 10), [500, 500, 500, 0, 600, 300]);
        assert_eq!(
            sample.values_at(t(17_280), 5, 10),
            [500, 500, 500, 0, 599, 298]
        );
        assert_eq!(
            sample.values_at(t(40 * day), 5, 10),
            [500, 500, 500, 0, 400, 0]
        );
        assert_eq!(
            bond(7, [0, 0, 0, 0, 100, 0], 0).values_at(t(30 * day), 5, 10)[FAMILIARITY],
            0
        );
    }

    /// Settling writes the faded values and moves `since`.
    #[test]
    fn settle_writes_the_faded_values() {
        let mut sample = bond(7, [500, 500, 500, 0, 600, 300], 0);
        let at = 10 * DAY.as_secs();
        sample.settle(t(at), 5, 10);
        assert_eq!(sample.values, [500, 500, 500, 0, 550, 200]);
        assert_eq!(sample.since, t(at));
        assert_eq!(sample.values_at(t(at), 5, 10), sample.values);
    }

    /// A step writes the fade first, then adds, and clamps to 0..=1000 (`check_bonds`).
    #[test]
    fn a_step_writes_the_fade_then_adds_and_clamps() {
        let mut sample = bond(7, [500, 500, 500, 0, 600, 300], 0);
        let at = 10 * DAY.as_secs();
        let before = sample.apply(t(at), 5, 10, &[(GRUDGE, 60)]);
        assert_eq!(before, [500, 500, 500, 0, 550, 200]);
        assert_eq!(sample.values, [500, 500, 500, 0, 550, 260]);
        assert_eq!(sample.since, t(at));

        let mut high = bond(7, [990, 20, 500, 0, 995, 995], 0);
        high.apply(
            t(0),
            5,
            10,
            &[
                (AFFECTION, 30),
                (TRUST, -80),
                (FAMILIARITY, 40),
                (GRUDGE, 100),
            ],
        );
        assert_eq!(high.values, [1000, 0, 500, 0, 1000, 1000]);
    }

    /// Eviction from `check_bonds`: 32 bonds to 100..=131. Others below 110 are protected.
    /// 115 and 122 sum to 200; 103 sums to 0 and is protected. The new bond does not compete.
    #[test]
    fn a_full_list_drops_the_smallest_sum_then_the_larger_id() {
        let mut bonds: Vec<Bond> = (100..132)
            .map(|other| bond(other, [300, 500, 400, 0, 200, 0], 0))
            .collect();
        bonds[15] = bond(115, [100, 500, 400, 0, 100, 0], 0);
        bonds[22] = bond(122, [100, 500, 400, 0, 100, 0], 0);
        bonds[3] = bond(103, [0, 500, 400, 0, 0, 0], 0);
        let protected = |other: SimId| other.get() < 110;
        let new = bond(200, [400, 450, 400, 0, 200, 0], 0);
        let outcome = insert(&mut bonds, new, t(0), 5, 10, 32, protected);
        assert_eq!(
            outcome,
            Formation::Formed {
                dropped: Some(id(122))
            }
        );
        assert_eq!(bonds.len(), 32);
        assert!(bonds.windows(2).all(|pair| pair[0].other < pair[1].other));
        assert_eq!(
            bonds
                .iter()
                .find(|bond| bond.other == id(200))
                .map(|bond| bond.values),
            Some([400, 450, 400, 0, 200, 0])
        );
        assert!(bonds.iter().all(|bond| bond.other != id(122)));

        // At day 10 every familiarity of 200 is 150 (sum 450); 120 stays at 200 (sum 500).
        // The tie on 450 drops 131, the larger id.
        let mut faded: Vec<Bond> = (100..132)
            .map(|other| bond(other, [300, 500, 400, 0, 200, 0], 0))
            .collect();
        faded[20] = bond(120, [300, 500, 400, 0, 250, 0], 0);
        let at = t(10 * DAY.as_secs());
        let outcome = insert(&mut faded, new, at, 5, 10, 32, protected);
        assert_eq!(
            outcome,
            Formation::Formed {
                dropped: Some(id(131))
            }
        );

        // Everyone protected: the list is unchanged and the new bond is not formed.
        let mut held: Vec<Bond> = (100..132)
            .map(|other| bond(other, [300, 500, 400, 0, 200, 0], 0))
            .collect();
        held[15] = bond(115, [100, 500, 400, 0, 100, 0], 0);
        held[22] = bond(122, [100, 500, 400, 0, 100, 0], 0);
        held[3] = bond(103, [0, 500, 400, 0, 0, 0], 0);
        let before = held.clone();
        let outcome = insert(&mut held, new, t(0), 5, 10, 32, |_| true);
        assert_eq!(outcome, Formation::Refused);
        assert_eq!(held, before);

        // Room for one more: nothing falls.
        let mut room: Vec<Bond> = (100..131)
            .map(|other| bond(other, [300, 500, 400, 0, 200, 0], 0))
            .collect();
        let partner = bond(200, [700, 700, 500, 500, 600, 0], 0);
        let outcome = insert(&mut room, partner, t(0), 5, 10, 32, protected);
        assert_eq!(outcome, Formation::Formed { dropped: None });
        assert_eq!(room.len(), 32);
        assert!(room.windows(2).all(|pair| pair[0].other < pair[1].other));
    }
}
