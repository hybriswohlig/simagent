//! The onset of a hereditary disease as a log entry (ADR-0017 section 3, addendum of
//! 2026-10-20; ADR-0021 G4; Grok's P1 to P3 and Q1): exactly one `core:disease_onset` per
//! person and disease, in the second the person becomes affected in this run.
//!
//! `B` is the onset second `born + ⌈onset · YEAR⌉`, `E` the second of the person's creation
//! (the start for a founder, the arrival for a pool person, the birth otherwise): (1) a
//! creation with `B ≤ E` writes the entry right after the person's creation entry if the
//! switch is on; (2) a creation with `B > E` plans an event at `B` that writes it if the
//! switch is on then; (3) an input that sets the switch on writes it for a reached onset
//! with no event of its own still open, if none of its earlier moments had the switch on.
//! Nothing of this is state but the planned events; path 3 reads the input log.

use alloc::vec::Vec;

use super::{Affliction, DomainState, Slot, disease};
use crate::data::Data;
use crate::data_id::Code;
use crate::ids::{EventId, SimId};
use crate::input::{Input, InputRecord};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::params::ParamChange;
use crate::time::{SimDuration, SimTime, YEAR, calendar};
use crate::world::{EventPayload, SimCtx};

/// The onset second `born + ⌈onset · YEAR⌉` of an onset age in years; `None` beyond the
/// clock, where it never comes, as a death beyond it (ADR-0002).
pub(crate) fn onset_second(born: SimTime, onset: f64) -> Option<SimTime> {
    let year = sim_math::i64_to_f64_exact(YEAR.as_secs())
        .unwrap_or_else(|| panic!("invariant onset.year"));
    let secs = sim_math::to_i64_exact(sim_math::ceil(onset * year))
        .unwrap_or_else(|| panic!("invariant onset.second"));
    born.checked_add(SimDuration::from_secs(secs))
}

/// The calendar year of the onset second and the onset in whole years, of an onset that
/// has come.
pub(crate) fn since(born: SimTime, onset: f64) -> (i64, i64) {
    let whole = sim_math::to_i64_exact(-sim_math::ceil(-onset))
        .unwrap_or_else(|| panic!("invariant onset.age"));
    let at = onset_second(born, onset).unwrap_or_else(|| panic!("invariant onset.second"));
    (calendar(at).year, whole)
}

/// The second a person of the state was created: the start for a founder, the arrival for
/// a pool person (a pool child's is its birth), the birth for a town child.
pub(crate) fn creation_second(born: SimTime, founder: bool, arrived: Option<SimTime>) -> SimTime {
    match (founder, arrived) {
        (_, Some(arrived)) => arrived,
        (true, None) => SimTime::START,
        (false, None) => born,
    }
}

/// The entry of `sim`'s onset of `affliction`.
fn write(
    data: &Data,
    sim: SimId,
    born: SimTime,
    affliction: &Affliction,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> EventId {
    let disease = disease::of_key(data, affliction.disease)
        .map(|disease| Code::from(disease.code.clone()))
        .unwrap_or_else(|| panic!("invariant onset.disease"));
    let (since_year, age_years) = since(born, affliction.onset.get());
    ctx.emit(
        LogPayload::DiseaseOnset {
            disease,
            since_year,
            age_years,
        },
        Vec::from([Subject::Sim(sim)]),
        causes,
    )
}

/// Paths 1 and 2 at the creation of `sim` by the entry `creation` (`now` is `E`), with the
/// switch the creation sees: each disease in data order either written now (`B ≤ E`) or
/// planned at `B`, unless `B` lies beyond the clock.
pub(crate) fn on_creation(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    creation: EventId,
    switch_on: bool,
    ctx: &mut SimCtx<'_, '_>,
) {
    let person = domain
        .sims
        .get(&sim)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
    let (born, afflictions) = (person.born, person.afflictions.clone());
    let now = ctx.now();
    for affliction in &afflictions {
        let Some(at) = onset_second(born, affliction.onset.get()) else {
            continue;
        };
        if at <= now {
            if switch_on {
                write(
                    data,
                    sim,
                    born,
                    affliction,
                    Causes::new(Cause::Event(creation)),
                    ctx,
                );
            }
        } else {
            let payload = EventPayload::DiseaseOnset {
                sim,
                disease: affliction.disease,
            };
            let handle = ctx.schedule(at, payload, creation);
            domain
                .pending
                .insert((sim, Slot::Onset(affliction.disease)), handle);
        }
    }
}

/// Path 2: the planned onset of `disease` of `sim` comes due; written if the switch is on.
pub(crate) fn on_onset(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    disease: u64,
    ctx: &mut SimCtx<'_, '_>,
) {
    domain.pending.remove(&(sim, Slot::Onset(disease)));
    let on = (domain.world_params.as_ref()).is_some_and(|params| params.hereditary_diseases);
    let person = domain
        .sims
        .get(&sim)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
    let affliction = (person.afflictions.iter())
        .find(|affliction| affliction.disease == disease)
        .unwrap_or_else(|| panic!("invariant onset.affliction"));
    if on {
        write(
            data,
            sim,
            person.born,
            affliction,
            Causes::new(ctx.trigger()),
            ctx,
        );
    }
}

/// The switch of the hereditary diseases in `history`: `Start`'s value and each change of an
/// override or a world rule, each with the second it acts from (one after its stamp), in
/// the order of the input log.
fn switches(history: &[InputRecord]) -> Vec<(SimTime, bool)> {
    let acts = |t: SimTime| {
        t.checked_add(SimDuration::from_secs(1))
            .unwrap_or_else(|| panic!("invariant onset.input_time"))
    };
    (history.iter())
        .flat_map(|record| match &record.input {
            Input::Start(start) => {
                Vec::from([(acts(record.t), start.world_params.hereditary_diseases)])
            }
            Input::ForkOverride(block) => (block.changes.iter())
                .filter_map(|change| match change {
                    ParamChange::HereditaryDiseases(on) => Some((acts(record.t), *on)),
                    ParamChange::NutritionYear(_) => None,
                })
                .collect(),
            Input::SetWorldParam(rule) => match rule.change {
                ParamChange::HereditaryDiseases(on) => Vec::from([(acts(record.t), on)]),
                ParamChange::NutritionYear(_) => Vec::new(),
            },
            Input::ArrangePartnership(_) => Vec::new(),
        })
        .collect()
}

/// How many persons the `Start` of `history` created, its founders and the members of the
/// households (or a village's persons) it brought: the SimIds 1 to that number.
fn by_start(history: &[InputRecord]) -> u64 {
    let start = (history.iter()).find_map(|record| match &record.input {
        Input::Start(start) => Some(start),
        Input::ForkOverride(_) | Input::ArrangePartnership(_) | Input::SetWorldParam(_) => None,
    });
    super::start::persons(start.unwrap_or_else(|| panic!("invariant onset.no_start")))
}

/// Path 3: an input that sets the switch on, in its second `T` (`history` the inputs before
/// it). For each person of the active population by SimId and each of its diseases in data
/// order with `B ≤ T` and no event of its own still open (`B > E` and `B ≥ T`), the entry,
/// caused by the input, unless one of its earlier moments had the switch on: the creation
/// (`B ≤ E`) with the switch it saw, `Start`'s own for what `Start` made, else the one after
/// the inputs of `E`; its event (`B > E`) with the switch after the inputs of `B`; an earlier
/// input that set the switch on after that, inputs of one second in their order.
pub(crate) fn on_switch_on(
    domain: &DomainState,
    data: &Data,
    history: &[InputRecord],
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let switches = switches(history);
    let started = by_start(history);
    // The switch after every input that acts by the second `t`.
    let at = |t: SimTime| {
        (switches.iter())
            .rfind(|(from, _)| *from <= t)
            .is_some_and(|(_, on)| *on)
    };
    let start_value = switches.first().is_some_and(|(_, on)| *on);
    for (&sim, person) in &domain.sims {
        let by_start = sim.get() <= started;
        let arrived = person.pool.as_ref().map(|origin| origin.arrived);
        let founder = person.parents.is_none() && person.pool.is_none();
        let created = creation_second(person.born, founder, arrived);
        for affliction in &person.afflictions {
            let Some(onset) = onset_second(person.born, affliction.onset.get()) else {
                continue;
            };
            let own_open = onset > created && onset >= now;
            if onset > now || own_open {
                continue;
            }
            let first = match (onset <= created, by_start) {
                (true, true) => start_value,
                (true, false) => at(created),
                (false, _) => at(onset),
            };
            let earlier = (switches.iter().skip(1)).any(|(from, on)| {
                *on && if onset <= created {
                    *from > created || (*from == created && by_start)
                } else {
                    *from > onset
                }
            });
            if !first && !earlier {
                write(
                    data,
                    sim,
                    person.born,
                    affliction,
                    Causes::new(ctx.trigger()),
                    ctx,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The onset second is the birth plus the onset in years, rounded up to a second; its
    /// year and whole years by hand (a year is 31 449 600 seconds).
    #[test]
    fn the_onset_second_and_its_year() {
        let y = YEAR.as_secs();
        let born = SimTime::from_secs(-10 * y);
        assert_eq!(onset_second(born, 0.0), Some(born));
        assert_eq!(onset_second(born, 45.0), Some(SimTime::from_secs(35 * y)));
        assert_eq!(
            onset_second(born, 1.5),
            Some(SimTime::from_secs(-10 * y + 3 * y / 2))
        );
        // 1e-7 years is 3.14496 seconds: rounded up to 4.
        assert_eq!(
            onset_second(born, 1e-7),
            Some(SimTime::from_secs(-10 * y + 4))
        );
        assert_eq!(since(born, 45.9), (35, 45));
        assert_eq!(since(born, 0.0), (-10, 0));
        assert_eq!(since(born, 1.0), (-9, 1));
    }

    /// The clock ends at a million years (`MAX_ABS_SECS`): an onset after its last second
    /// never comes, one in it does.
    #[test]
    fn an_onset_beyond_the_clock_never_comes() {
        let last = crate::time::MAX_ABS_SECS;
        let born = SimTime::from_secs(last - 4);
        assert_eq!(onset_second(born, 1e-7), Some(SimTime::from_secs(last)));
        // 2e-7 years is 6.28992 seconds: rounded up to 7, three beyond the last second.
        assert_eq!(onset_second(born, 2e-7), None);
    }

    #[test]
    fn the_creation_second_of_each_kind() {
        let t = SimTime::from_secs;
        assert_eq!(creation_second(t(-100), true, None), SimTime::START);
        assert_eq!(creation_second(t(500), false, None), t(500));
        let year = t(YEAR.as_secs());
        assert_eq!(creation_second(t(-100), false, Some(year)), year);
    }
}
