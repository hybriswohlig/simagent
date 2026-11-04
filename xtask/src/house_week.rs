//! `cargo xtask house-week`: the budget of a week in houses (plan of phase 2, sections 4.8 and
//! 5 item 6; ADR-0024 section 6). Two starts, each with 50 households that move in at 23:59:59
//! of day 0 and live the week from their first morning to 6:00 of day 8: a town founded from
//! the repository's pool, its first 50 households (by id) that a house holds; and 50 founder
//! families of 200 persons, a couple of 30 to 45 each and 100 children of 6 to 15 whom the
//! couples take in (ADR-0018 section 4). For each it prints the households and persons, the
//! events, entries and choices of the week, the most choices of one person in one day, the most
//! offers one choice weighs, the most entries of one person in one day, and the seconds the
//! week took. It is red when a counted limit breaks: 48 choices per person and day and 32
//! offers per choice (plan section 4.8), 150 entries per person and day (ADR-0024 section 6).
//! The time is reported against its target of 5 s, never a reason to be red. Outside the gate;
//! a debug build runs the release build of itself, so the time is the release time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::house::choice::{age, score};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{Couple, Founder, Input, MoveIn, PoolStart, Start};
use sim_core::log::{LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{DAY, HOUR, SimTime, YEAR, day_index};
use sim_core::world::{World, YearOut};

use crate::{Result, cargo_program};

/// The town's seed (never reseeded) and its persons at the start, enough for `HOUSEHOLDS`.
pub const SEED_TOWN: u64 = 0x5eed_0002_0006_0001;
const N_START: u64 = 250;
/// The founder families are those of the first seed from this one up (never reseeded) whose
/// households a house holds each, within `TRIES`: a couple may take in up to six children.
pub const SEED_FAMILIES: u64 = 0x5eed_0002_0006_0101;
const TRIES: u64 = 20;
/// The households that move in.
pub const HOUSEHOLDS: usize = 50;
/// The counted limits: choices per person and day, offers per choice (plan section 4.8),
/// entries per person and day (ADR-0024 section 6).
pub const CHOICES_PER_DAY: usize = 48;
pub const OFFERS_PER_CHOICE: usize = 32;
pub const ENTRIES_PER_DAY: usize = 150;
/// The time the week should take in release.
const TARGET_SECS: f64 = 5.0;

/// What a week in houses counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counted {
    pub households: usize,
    pub persons: usize,
    /// Persons who had needs in the week (6 or older).
    pub with_needs: usize,
    pub events: u64,
    pub entries: usize,
    /// `core:action_started` and `core:idle`: each choice writes one of them.
    pub choices: usize,
    pub most_choices: usize,
    pub most_offers: usize,
    pub most_entries: usize,
}

/// The counted limits `counted` breaks, each named with its value.
pub fn broken(counted: &Counted) -> Vec<String> {
    let mut broken = Vec::new();
    for (name, value, limit) in [
        (
            "choices of one person in one day",
            counted.most_choices,
            CHOICES_PER_DAY,
        ),
        (
            "offers of one choice",
            counted.most_offers,
            OFFERS_PER_CHOICE,
        ),
        (
            "entries of one person in one day",
            counted.most_entries,
            ENTRIES_PER_DAY,
        ),
    ] {
        if value > limit {
            broken.push(format!("{value} {name}, above {limit}"));
        }
    }
    broken
}

/// The town of `SEED_TOWN` from the pool of `data`, `N_START` persons.
pub fn town(data: &Data) -> Result<Start> {
    let pool = PoolStart::drawn(data, SEED_TOWN, N_START, false).ok_or("the data hold no pool")?;
    start(SEED_TOWN, Vec::new(), Vec::new(), Some(pool))
}

/// The founder families of `seed`: a couple of family `i` below `HOUSEHOLDS`, the woman of
/// `30 + i % 16` and the man of `30 + (7i + 3) % 16` years, and two children of `6 + i % 10`
/// and `6 + (3i + 5) % 10`, born that many years before the first second; the couples take
/// the children in as `FOSTER_RANK` ranks them.
pub fn families(seed: u64) -> Result<Start> {
    let founder = |years: i64, sex: Sex| Founder {
        born: SimTime::from_secs(-years * YEAR.as_secs()),
        sex,
        alleles: Vec::new(),
    };
    let (mut founders, mut couples, mut children) = (Vec::new(), Vec::new(), Vec::new());
    for (i, woman) in (0..).zip((0..).step_by(2)).take(HOUSEHOLDS) {
        couples.push(Couple {
            woman,
            man: woman + 1,
        });
        founders.push(founder(30 + i % 16, Sex::Female));
        founders.push(founder(30 + (7 * i + 3) % 16, Sex::Male));
        children.push(founder(6 + i % 10, Sex::Female));
        children.push(founder(6 + (3 * i + 5) % 10, Sex::Male));
    }
    founders.extend(children);
    start(seed, founders, couples, None)
}

/// The start of a week in houses: no hereditary diseases, no capacity.
fn start(
    seed: u64,
    founders: Vec<Founder>,
    couples: Vec<Couple>,
    pool: Option<PoolStart>,
) -> Result<Start> {
    Ok(Start {
        world_seed: seed,
        founder_tag: DataId::new("core:house_week").map_err(|e| format!("{e:?}"))?,
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders,
        couples,
        capacity: None,
        pool,
    })
}

/// The world of `start` on `data`: the first `households` of its households (by id) that a
/// house holds move in at 23:59:59 of day 0, then the week runs to 6:00 of day 8 in one call.
/// What it counted, and the seconds of that call; `None` if fewer households fit a house.
pub fn week(data: Data, start: Start, households: usize) -> Result<Option<(Counted, f64)>> {
    let mut world = World::new(data);
    world
        .submit(Input::Start(start))
        .map_err(|e| format!("start: {e:?}"))?;
    let mut out = YearOut::default();
    let day = DAY.as_secs();
    world.advance_until(&mut out, SimTime::from_secs(day - 2), &mut |_| false);
    let state = world.state();
    let mut members: BTreeMap<HouseholdId, Vec<SimId>> = BTreeMap::new();
    for id in 1..=state.sim_ids_allocated() {
        let sim = SimId::try_from(id).map_err(|e| format!("{e:?}"))?;
        if let Some(home) = state.household_of(sim) {
            members.entry(home).or_default().push(sim);
        }
    }
    let houses = world.data().houses().ok_or("the data hold no houses")?;
    let moving: Vec<HouseholdId> = (members.iter())
        .filter(|(_, members)| houses.house_for(members.len()).is_some())
        .map(|(home, _)| *home)
        .take(households)
        .collect();
    if moving.len() < households {
        return Ok(None);
    }
    let persons: BTreeSet<SimId> = moving
        .iter()
        .flat_map(|home| members[home].clone())
        .collect();
    for &household in &moving {
        world
            .submit(Input::MoveIn(MoveIn { household }))
            .map_err(|e| format!("move in {}: {e:?}", household.get()))?;
    }
    drop(world.take_log());
    let before = world.stats().domain_calls;
    let end = SimTime::from_secs(8 * day + 6 * HOUR.as_secs());
    let started = Instant::now();
    world.advance_until(&mut out, end, &mut |_| false);
    let secs = started.elapsed().as_secs_f64();
    let log = world.take_log();
    let state = world.state();
    let houses = world.data().houses().ok_or("the data hold no houses")?;
    let mut counted = Counted {
        households: moving.len(),
        persons: persons.len(),
        events: world.stats().domain_calls - before,
        entries: log.len(),
        ..Counted::default()
    };
    let (mut choices, mut entries) = (BTreeMap::new(), BTreeMap::new());
    let mut with_needs = BTreeSet::new();
    for entry in &log {
        let today = day_index(entry.t);
        for subject in &entry.subjects {
            if let Subject::Sim(sim) = subject
                && persons.contains(sim)
            {
                *entries.entry((*sim, today)).or_insert(0) += 1;
            }
        }
        let &[Subject::Sim(sim)] = entry.subjects.as_slice() else {
            continue;
        };
        let (house, room, needs) = match &entry.payload {
            LogPayload::DayStarted { .. } => {
                with_needs.insert(sim);
                continue;
            }
            LogPayload::ActionStarted {
                house, room, needs, ..
            }
            | LogPayload::Idle {
                house, room, needs, ..
            } => (house, *room, *needs),
            _ => continue,
        };
        counted.choices += 1;
        *choices.entry((sim, today)).or_insert(0) += 1;
        let born = state.born(sim).ok_or("a choice of nobody")?;
        let years = age(born, entry.t).ok_or("a choice before the birth")?;
        let house = houses.house(house).ok_or("a choice in no house")?;
        let weighed = score(house, houses, years, needs, room).len();
        counted.most_offers = counted.most_offers.max(weighed);
    }
    counted.with_needs = with_needs.len();
    counted.most_choices = choices.values().copied().max().unwrap_or(0);
    counted.most_entries = entries.values().copied().max().unwrap_or(0);
    Ok(Some((counted, secs)))
}

/// `cargo xtask house-week`.
pub fn command(root: &Path) -> Result {
    if cfg!(debug_assertions) {
        // Only the release time counts: the release build of this command measures it.
        let status = Command::new(cargo_program())
            .args([
                "run",
                "--release",
                "--locked",
                "--quiet",
                "--package",
                "xtask",
            ])
            .args(["--", "house-week"])
            .current_dir(root)
            .status()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err("the release build of house-week failed".to_string())
        };
    }
    let data = crate::criteria::village::repository_data(root)?;
    let town = week(data.clone(), town(&data)?, HOUSEHOLDS)?;
    let town = town.ok_or("the town has too few households a house holds")?;
    let mut found = None;
    for n in 0..TRIES {
        if let Some(week) = week(data.clone(), families(SEED_FAMILIES + n)?, HOUSEHOLDS)? {
            found = Some((SEED_FAMILIES + n, week));
            break;
        }
    }
    let (seed, families) = found.ok_or("no seed within the tries")?;
    let mut broken = Vec::new();
    for (name, (counted, secs)) in [
        (
            format!("a town of {N_START} from the pool, seed {SEED_TOWN:016x}"),
            town,
        ),
        (
            format!("{HOUSEHOLDS} founder families, seed {seed:016x}"),
            families,
        ),
    ] {
        print(&name, &counted, secs);
        broken.extend(
            self::broken(&counted)
                .into_iter()
                .map(|what| format!("{name}: {what}")),
        );
    }
    if broken.is_empty() {
        Ok(())
    } else {
        Err(broken.join("; "))
    }
}

fn print(name: &str, counted: &Counted, secs: f64) {
    println!(
        "house-week, {name}: {} households with {} persons ({} with needs) moved in",
        counted.households, counted.persons, counted.with_needs
    );
    println!(
        "  the week from the first morning: {} events, {} entries, {} choices",
        counted.events, counted.entries, counted.choices
    );
    println!(
        "  at most {} choices of one person in one day (limit {CHOICES_PER_DAY}), {} offers of one choice (limit {OFFERS_PER_CHOICE}), {} entries of one person in one day (limit {ENTRIES_PER_DAY})",
        counted.most_choices, counted.most_offers, counted.most_entries
    );
    let met = if secs < TARGET_SECS { "met" } else { "missed" };
    println!("  {secs:.3} s in release (target {TARGET_SECS} s, {met})");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A limit holds at its value and breaks one above it; each break is named.
    #[test]
    fn a_limit_breaks_above_its_value() {
        let at = Counted {
            most_choices: CHOICES_PER_DAY,
            most_offers: OFFERS_PER_CHOICE,
            most_entries: ENTRIES_PER_DAY,
            ..Counted::default()
        };
        assert!(broken(&at).is_empty());
        let above = Counted {
            most_choices: CHOICES_PER_DAY + 1,
            most_offers: OFFERS_PER_CHOICE + 1,
            most_entries: ENTRIES_PER_DAY + 1,
            ..at
        };
        assert_eq!(
            broken(&above),
            [
                "49 choices of one person in one day, above 48",
                "33 offers of one choice, above 32",
                "151 entries of one person in one day, above 150",
            ]
        );
    }
}
