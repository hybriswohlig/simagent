//! The upbringing table (ADR-0033 sections 4 and 5): the occasions that step a child's lived
//! experience `D`, each change a `core:developed`. Who is a parent and who an older sibling is
//! read from the state at the second of the occasion, never from the log.

use alloc::vec;
use alloc::vec::Vec;

use crate::data::Data;
use crate::data_id::DataId;
use crate::development::{RAISED_UNTIL, Upbringing};
use crate::house::data::Need;
use crate::house::state::{Duty, Place};
use crate::ids::{EventId, SimId};
use crate::log::EndReason;
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::time::{SimTime, YEAR, day_index};
use crate::world::SimCtx;

use super::DomainState;
use super::action::need_index;

/// A child takes part in the talk row from 6 (ADR-0033 section 5).
const TALK_FROM: i64 = 6 * YEAR.as_secs();

/// A talk of `a` and `b` on `topic`, `actor` the one who praises, teases or apologizes, logged
/// as `talked`: for each child of the two, in the order `a`, `b`, the rows of praise, tease or
/// apology if the other is the actor and its parent (for tease and apology also its older
/// sibling), and the talk row from 6.
pub(super) fn talked(
    domain: &mut DomainState,
    data: &Data,
    (a, b): (SimId, SimId),
    (topic, actor): (&DataId, Option<SimId>),
    talked: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let by_actor = match topic.as_str() {
        "core:praise" => Some(Upbringing::Praise),
        "core:tease" => Some(Upbringing::Tease),
        "core:apology" => Some(Upbringing::Apology),
        _ => None,
    };
    for (child, other) in [(a, b), (b, a)] {
        let mut reached = Vec::new();
        if let Some(occasion) = by_actor
            && actor == Some(other)
        {
            let parent = parent_of(domain, other, child);
            let elder =
                occasion != Upbringing::Praise && older_sibling_of(domain, other, child, now);
            if parent || elder {
                reached.push((occasion, other));
            }
        }
        if age(domain, child, now).is_some_and(|secs| secs >= TALK_FROM) {
            reached.push((Upbringing::Talk, child));
        }
        develop(domain, data, child, &reached, talked, ctx);
    }
}

/// `sim` came home from `place`, logged as `came`: the school row.
pub(super) fn came_home(
    domain: &mut DomainState,
    data: &Data,
    (sim, place): (SimId, Duty),
    came: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if place == Duty::School {
        develop(domain, data, sim, &[(Upbringing::School, sim)], came, ctx);
    }
}

/// `sim` ended its action at furniture of `kind` for `reason`, logged as `ended`; `commanded` if
/// a command chose it. Reading or playing to the end by its own choice: the done rows. The core
/// keeps no names of offers, so reading is an offer of the bookshelf and playing one of the toy
/// chest, the only offers of those kinds.
pub(super) fn finished(
    domain: &mut DomainState,
    data: &Data,
    (sim, kind, reason, commanded): (SimId, &DataId, EndReason, bool),
    ended: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let own = matches!(kind.as_str(), "core:bookshelf" | "core:toy_chest");
    if own && reason == EndReason::Done && !commanded {
        develop(domain, data, sim, &[(Upbringing::Done, sim)], ended, ctx);
    }
}

/// `need` of `sim` reached 0, logged as `empty`: the row of a need at 0 if a parent is at home
/// (in the house and not away), once per need and calendar day. The day is kept when the row
/// applies, whether or not `D` changed.
pub(super) fn need_empty(
    domain: &mut DomainState,
    data: &Data,
    (sim, need): (SimId, Need),
    empty: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let at_home = |p: &SimId| {
        (domain.house.sims.get(p)).is_some_and(|person| !matches!(person.place, Place::Away))
    };
    let home = (domain.sims.keys()).any(|p| parent_of(domain, *p, sim) && at_home(p));
    if data.upbringing().is_none() || !home || !develops(domain, sim, now) {
        return;
    }
    let today = day_index(now);
    let Some(raised) = domain.house.raised.get_mut(&sim) else {
        panic!("invariant upbringing.raised");
    };
    let counted = &mut raised.counted[need_index(need)];
    if *counted == Some(today) {
        return;
    }
    *counted = Some(today);
    develop(
        domain,
        data,
        sim,
        &[(Upbringing::NeedEmpty, sim)],
        empty,
        ctx,
    );
}

/// Whether `D` of `sim` can change at `now`: living, raised, under 18, its household in a house.
fn develops(domain: &DomainState, sim: SimId, now: SimTime) -> bool {
    let Some(person) = domain.sims.get(&sim) else {
        return false;
    };
    let young = now
        .checked_since(person.born)
        .is_some_and(|age| age < RAISED_UNTIL);
    let housed = domain.house.households.contains_key(&person.household);
    young && housed && domain.house.raised.contains_key(&sim)
}

/// The age of a living person at `now` in seconds.
fn age(domain: &DomainState, sim: SimId, now: SimTime) -> Option<i64> {
    let born = domain.sims.get(&sim)?.born;
    now.checked_since(born).map(|age| age.as_secs())
}

/// `p` is a parent of `k`: living, in the same household, and its mother or father from the
/// birth or one of its social parents.
fn parent_of(domain: &DomainState, p: SimId, k: SimId) -> bool {
    let (Some(parent), Some(child)) = (domain.sims.get(&p), domain.sims.get(&k)) else {
        return false;
    };
    let (mother, father) = domain.town_parents(k);
    p != k
        && parent.household == child.household
        && (mother == Some(p) || father == Some(p) || child.social_parents.contains(&p))
}

/// `s` is an older sibling of `k`: another living person under 18 in the same household, born
/// strictly earlier, who shares a parent with `k` in the sense of [`parent_of`].
fn older_sibling_of(domain: &DomainState, s: SimId, k: SimId, now: SimTime) -> bool {
    let (Some(sibling), Some(child)) = (domain.sims.get(&s), domain.sims.get(&k)) else {
        return false;
    };
    let young = age(domain, s, now).is_some_and(|secs| secs < RAISED_UNTIL.as_secs());
    let earlier = sibling.born < child.born;
    let same = sibling.household == child.household;
    let shared =
        || (domain.sims.keys()).any(|p| parent_of(domain, *p, s) && parent_of(domain, *p, k));
    s != k && young && earlier && same && shared()
}

/// The rows of the occasions `reached` (each with its source) for `child`: if it is raised,
/// under 18 and its household lives in a house, [`UpbringingTable::steps`]; each change writes
/// `core:developed` by `cause`.
fn develop(
    domain: &mut DomainState,
    data: &Data,
    child: SimId,
    reached: &[(Upbringing, SimId)],
    cause: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (Some(table), false) = (data.upbringing(), reached.is_empty()) else {
        return;
    };
    let now = ctx.now();
    if !develops(domain, child, now) {
        return;
    }
    let Some(age) = (domain.sims.get(&child)).and_then(|person| now.checked_since(person.born))
    else {
        panic!("invariant upbringing.age");
    };
    let Some(raised) = domain.house.raised.get_mut(&child) else {
        panic!("invariant upbringing.raised");
    };
    let occasions: Vec<Upbringing> = reached.iter().map(|(occasion, _)| *occasion).collect();
    for stepped in table.steps(&occasions, age, &mut raised.experience) {
        let (occasion, source) = reached[stepped.by];
        let subjects = if source == child {
            vec![Subject::Sim(child)]
        } else {
            vec![Subject::Sim(child), Subject::Sim(source)]
        };
        let developed = LogPayload::Developed {
            axis: stepped.axis,
            before: stepped.before,
            after: stepped.after,
            step: stepped.step,
            cause: occasion,
        };
        ctx.emit(developed, subjects, Causes::new(Cause::Event(cause)));
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{older_sibling_of, parent_of};
    use crate::data_id::DataId;
    use crate::genome::{Genome, Sex};
    use crate::ids::{HouseholdId, SimId};
    use crate::lineage::LineageKey;
    use crate::state::{DomainState, Parents, Sim};
    use crate::time::{SimTime, YEAR};

    fn sid(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    const NOW: i64 = 100 * YEAR.as_secs();

    /// The family of `tools/upbringing_reference.py` (`family`): mother 1 and father 2, daughter
    /// 3 (8), son 4 (14), twins 5 and 6 (10, the same second), stepdaughter 7 (9) of the father
    /// with the mother as social parent, a neighbour 8 in household 2, a girl 9 (16) of the
    /// mother and a father outside the town. The mothers and fathers not in the town are absent.
    fn family() -> DomainState {
        let mut domain = DomainState::new();
        let mut add =
            |n: u64, years: i64, home: u64, parents: Option<(u64, u64)>, social: &[u64]| {
                let id = sid(n);
                let tag = DataId::new("test:upbringing").expect("tag");
                let lineage_key = LineageKey::founder(&tag, n);
                domain.keys.insert(lineage_key, id);
                let sim = Sim {
                    born: SimTime::from_secs(NOW - years * YEAR.as_secs()),
                    lineage_key,
                    sex: Sex::Female,
                    genome: Genome::reference(Sex::Female),
                    afflictions: Vec::new(),
                    conceptions: 0,
                    births: 0,
                    partnerships: 0,
                    household: HouseholdId::try_from(home).expect("valid"),
                    moved_out: false,
                    carer: None,
                    parents: parents.map(|(mother, father)| Parents {
                        mother: sid(mother),
                        father: sid(father),
                        conception: 0,
                    }),
                    social_parents: social.iter().map(|n| sid(*n)).collect(),
                    pool: None,
                };
                domain.sims.insert(id, sim);
            };
        add(1, 38, 1, None, &[]);
        add(2, 40, 1, None, &[]);
        add(3, 8, 1, Some((1, 2)), &[]);
        add(4, 14, 1, Some((1, 2)), &[]);
        add(5, 10, 1, Some((1, 2)), &[]);
        add(6, 10, 1, Some((1, 2)), &[]);
        add(7, 9, 1, Some((20, 2)), &[1]);
        add(8, 30, 2, None, &[]);
        add(9, 16, 1, Some((1, 21)), &[]);
        domain
    }

    /// Who is a parent (ADR-0033 section 5): the mother and father from the birth and a social
    /// parent, in the same household; not a sibling, not a neighbour, not oneself.
    #[test]
    fn a_parent_is_from_the_birth_or_social_and_at_home() {
        let domain = family();
        let parent = |p, k| parent_of(&domain, sid(p), sid(k));
        assert!(parent(1, 3) && parent(2, 3) && parent(2, 7) && parent(1, 7) && parent(1, 9));
        assert!(!parent(2, 9) && !parent(4, 3) && !parent(8, 3) && !parent(3, 3));
        let mut moved = family();
        moved.sims.get_mut(&sid(1)).expect("mother").household =
            HouseholdId::try_from(2).expect("valid");
        assert!(!parent_of(&moved, sid(1), sid(3)));
    }

    /// Who is an older sibling: under 18, the same household, a shared parent, born strictly
    /// earlier. Twins of the same second are not older for each other; a stepsister through a
    /// social parent counts; the girl of 16 shares the mother with the son of 14.
    #[test]
    fn an_older_sibling_shares_a_parent_and_was_born_earlier() {
        let domain = family();
        let now = SimTime::from_secs(NOW);
        let older = |s, k| older_sibling_of(&domain, sid(s), sid(k), now);
        assert!(older(4, 3) && older(4, 7) && older(9, 4) && older(5, 3));
        assert!(!older(3, 4) && !older(5, 6) && !older(6, 5) && !older(4, 9) && !older(1, 3));
        // At 18 a sibling no longer counts.
        let later = SimTime::from_secs(NOW + 2 * YEAR.as_secs());
        assert!(!older_sibling_of(&domain, sid(9), sid(4), later));
        assert!(older_sibling_of(
            &domain,
            sid(9),
            sid(4),
            SimTime::from_secs(NOW + 2 * YEAR.as_secs() - 1)
        ));
    }
}
