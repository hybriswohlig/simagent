//! The test family's week (plan of phase 2, section 5): two adults of 30 to 45 and two children
//! of 6 to 15 in the repository's terraced house, seven days, 20 fixed seeds, on the
//! repository's data, measured from the public state after every second with events and from
//! the log. The thresholds are the plan's. Item 1, nobody starves next to food, with its
//! canary; item 3, duties on time with their reasons; item 4, places, after every second (the
//! crowd cases are in `house_crowd.rs`); item 5, the same week however it is run, saved and
//! loaded mid walk and mid sleep; item 2, sleep in range. Item 6, the budget, is `cargo xtask
//! house-week`.
//!
//! The family: four founders of `Start`, a woman and a man as its couple and two children, whom
//! the couple takes in (ADR-0018 section 4: with no relative of 18 or more, a child goes to the
//! first woman `FOSTER_RANK` ranks among those in an open partnership, here the only one).
//! Their ages are a rule of the family's number (`ages`), the seed is `SEED_WEEK` plus that
//! number; the household moves in at 23:59:59 of day 0, its first morning is 6:00 of day 1
//! (weekday 1). The week runs to noon of day 8.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::day::{MORNING, Run, number, places_hold};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::age;
use sim_core::house::data::Need;
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Action, Place};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::log::{Cause, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
/// The end of the week: noon of day 8.
const END: i64 = 8 * D + 12 * H;
/// The departures of days 1 to 7 count; day 8 begins here.
const DAY_EIGHT: i64 = 8 * D;
/// Hunger under this value is hunger next to food (item 1).
const HUNGRY: u16 = 150;
/// A stretch this long fails item 1: 60 minutes.
const STARVING: i64 = 3_600;
/// At least this share of the departures is on time, in percent (item 3).
const ON_TIME_PERCENT: usize = 95;
/// The sleep of a night in range (item 2): adults 6.5 to 9 hours, children 8.5 to 11.
const ADULT_SLEEP: (i64, i64) = (23_400, 32_400);
const CHILD_SLEEP: (i64, i64) = (30_600, 39_600);
/// Each person sleeps in range in at least this many of the seven nights.
const GOOD_NIGHTS: usize = 6;

/// The gate seeds of the families: `SEED_WEEK + i` for family `i` below `FAMILIES` (never
/// reseeded).
const SEED_WEEK: u64 = 0x5eed_0002_0005_0001;
const FAMILIES: u64 = 20;
const TAG: &str = "test:week";

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

/// The ages of family `i` in whole years through the week (each born that many years before
/// the first second, so no birthday falls in the week): the woman, the man and the two
/// children, in fixed steps through 30 to 45 and 6 to 15; the 20 families hold every age of
/// each range.
fn ages(i: u64) -> [i64; 4] {
    let i = i64::try_from(i).expect("small");
    [
        30 + i % 16,
        30 + (7 * i + 3) % 16,
        6 + i % 10,
        6 + (3 * i + 5) % 10,
    ]
}

/// Family `i` on `data`, moved in at 23:59:59 of day 0, into the terraced house (the
/// smallest that holds four, ADR-0025 section 3).
fn family(data: &Data, i: u64) -> Run {
    let born = ages(i).map(|years| -years * YEAR.as_secs());
    let (run, home) = Run::family(data.clone(), SEED_WEEK + i, TAG, &born, 0);
    let house = &run.world.state().house().households[&home].house;
    assert_eq!(
        house,
        &DataId::new("core:terraced").expect("id"),
        "family {i}"
    );
    run
}

/// What the state showed after a second with events, per person with needs: whether it is at
/// home and awake with an eating place free, and its hunger.
type Seen = BTreeMap<SimId, (bool, NeedSample)>;

/// A family's week: the world at its end, its log, and what the state showed.
struct Week {
    world: World,
    log: Vec<LogEntry>,
    /// After each second with events.
    seen: Vec<(i64, Seen)>,
    /// The action a departure waited for, by person and planned departure (ADR-0024 section 4).
    waited: BTreeMap<(SimId, i64), Action>,
}

/// The week of family `i`, one second with events at a time.
fn week(data: &Data, i: u64) -> Week {
    let mut run = family(data, i);
    let (mut seen, mut waited) = (Vec::new(), BTreeMap::new());
    run.step(END, |world, second| {
        seen.push((second, observe(world, second)));
        for (&sim, person) in &world.state().house().sims {
            if let (Some(due), Some(action)) = (person.due, person.action) {
                waited.entry((sim, due.depart.as_secs())).or_insert(action);
            }
        }
    });
    Week {
        world: run.world,
        log: run.log,
        seen,
        waited,
    }
}

/// The house part after a second with events, read from the public state: its places hold
/// (item 4, `places_hold`); then, per person with needs, whether it is at home and awake (not
/// away, not on its way out, not at the object of an offer of sleep) with an eating place free
/// (a place of an object with an offer that raises hunger at the person's age, taken by no
/// action), and its hunger.
fn observe(world: &World, second: i64) -> Seen {
    places_hold(world, second);
    let state = world.state();
    let house = state.house();
    let houses = world.data().houses().expect("houses");
    let household = |sim: SimId| state.household_of(sim).expect("living");
    let taken: BTreeSet<(HouseholdId, u16, u8)> = (house.sims.iter())
        .filter_map(|(&sim, person)| Some((household(sim), person.action?)))
        .map(|(home, action)| (home, action.object, action.slot))
        .collect();
    let mut watched = BTreeMap::new();
    for (&sim, person) in &house.sims {
        let home = household(sim);
        let def = houses
            .house(&house.households[&home].house)
            .expect("its house")
            .def();
        let years = age(state.born(sim).expect("born"), t(second)).expect("an age");
        let kind = |object: u16| {
            let id = &def.objects[usize::from(object)].kind;
            houses.kind(id).expect("a kind")
        };
        let awake = match (person.place, person.action) {
            (Place::Room(_), None) | (Place::Walking { .. }, Some(_)) => true,
            (Place::Room(_), Some(action)) => {
                !kind(action.object).offers[usize::from(action.offer)].sleep
            }
            _ => false,
        };
        let food = (0..).zip(&def.objects).any(|(object, _)| {
            let kind = kind(object);
            let feeds =
                (kind.offers.iter()).any(|offer| offer.raises(houses.rates(), Need::Hunger, years));
            feeds && (0..kind.slots).any(|slot| !taken.contains(&(home, object, slot)))
        });
        watched.insert(sim, (awake && food, person.needs[0]));
    }
    watched
}

/// The first second in `from..to` at which `holds`, which turns true once and stays; `to` if
/// it never does.
fn first(from: i64, to: i64, holds: impl Fn(i64) -> bool) -> i64 {
    let (mut low, mut high) = (from, to);
    while low < high {
        let middle = low + (high - low) / 2;
        if holds(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// Item 1 per person: the longest stretch of seconds in which it was at home and awake with
/// an eating place free and its hunger under 150. Between two seconds with events nothing
/// changes but the needs, each at its rate, so within such a span the seconds under 150 are one
/// run at its start (hunger rising) or at its end (falling); runs that touch join.
fn hungry(week: &Week) -> BTreeMap<SimId, i64> {
    let persons: BTreeSet<SimId> = (week.seen.iter())
        .flat_map(|(_, seen)| seen.keys().copied())
        .collect();
    let ends = (week.seen.iter().skip(1).map(|(second, _)| *second)).chain([END + 1]);
    let spans: Vec<_> = week.seen.iter().zip(ends).collect();
    let mut longest = BTreeMap::new();
    for sim in persons {
        let (mut most, mut open) = (0, None::<(i64, i64)>);
        for ((from, seen), to) in &spans {
            let under = |hunger: NeedSample, second: i64| hunger.value_at(t(second)) < HUNGRY;
            let run = match seen.get(&sim) {
                Some(&(true, hunger)) if hunger.rate() <= 0 => {
                    let begins = first(*from, *to, |second| under(hunger, second));
                    (begins < *to).then_some((begins, *to))
                }
                Some(&(true, hunger)) => {
                    let ends = first(*from, *to, |second| !under(hunger, second));
                    (ends > *from).then_some((*from, ends))
                }
                _ => None,
            };
            match (open, run) {
                (Some((begun, end)), Some((begins, ends))) if end == begins => {
                    open = Some((begun, ends));
                }
                _ => {
                    if let Some((begun, end)) = open {
                        most = most.max(end - begun);
                    }
                    open = run;
                }
            }
        }
        if let Some((begun, end)) = open {
            most = most.max(end - begun);
        }
        longest.insert(sim, most);
    }
    longest
}

/// Item 2 per person: the seconds it slept in each of the seven nights. Asleep is a person
/// from its `core:reached` of an action whose offer has `sleep` to that action's
/// `core:action_ended` (the walk to the bed is awake), or to the end of the week. Night `n`
/// (1 to 7) is the 24 hours from noon of day `n` to noon of day `n + 1`, so that every second
/// of sleep after the first noon belongs to exactly one night and a nap counts for the next.
fn slept(week: &Week) -> BTreeMap<SimId, [i64; 7]> {
    let houses = week.world.data().houses().expect("houses");
    let sleeps = |house: &DataId, object: u16, offer: u16| {
        let def = houses.house(house).expect("its house").def();
        let kind = houses
            .kind(&def.objects[usize::from(object)].kind)
            .expect("a kind");
        kind.offers[usize::from(offer)].sleep
    };
    let mut nights: BTreeMap<SimId, [i64; 7]> = BTreeMap::new();
    let mut add = |sim: SimId, from: i64, to: i64| {
        for (night, slept) in (1..).zip(nights.entry(sim).or_default().iter_mut()) {
            let (noon, next) = (night * D + 12 * H, (night + 1) * D + 12 * H);
            *slept += (to.min(next) - from.max(noon)).max(0);
        }
    };
    let (mut chosen, mut since) = (BTreeMap::new(), BTreeMap::new());
    for entry in &week.log {
        let &[Subject::Sim(sim)] = entry.subjects.as_slice() else {
            continue;
        };
        let at = entry.t.as_secs();
        match &entry.payload {
            LogPayload::DayStarted { .. } => add(sim, at, at),
            LogPayload::ActionStarted {
                house,
                object,
                offer,
                ..
            } => {
                chosen.insert(sim, sleeps(house, *object, *offer));
            }
            LogPayload::Reached { .. } if chosen.get(&sim) == Some(&true) => {
                since.insert(sim, at);
            }
            LogPayload::ActionEnded { .. } => {
                chosen.remove(&sim);
                if let Some(from) = since.remove(&sim) {
                    add(sim, from, at);
                }
            }
            _ => {}
        }
    }
    for (sim, from) in since {
        add(sim, from, END);
    }
    nights
}

/// Item 3 over the departures planned for days 1 to 7.
#[derive(Debug, Default)]
struct Duties {
    planned: usize,
    on_time: usize,
    late: usize,
    missed: usize,
    /// A late or missed departure whose reason the log does not show, or one without a single
    /// outcome.
    unexplained: Vec<String>,
}

/// Whether a family's week has duties and at least 95 percent of them on time (item 3).
fn punctual(duties: &Duties) -> bool {
    duties.planned > 0 && duties.on_time * 100 >= duties.planned * ON_TIME_PERCENT
}

/// Each `core:day_planned` of days 1 to 7 has one outcome: `core:left_home` with `late_secs`
/// 0 (on time) or more (late), or `core:missed` at its return (not on time). A late departure's
/// cause is the `core:action_ended` of its second that ended the action it waited for; a
/// missed one's cause is the plan of its duty, and its reason the action it waited for: begun
/// before the departure (its `core:action_started`), not ended before the return.
fn duties(week: &Week) -> Duties {
    let by_id: BTreeMap<u64, &LogEntry> = (week.log.iter())
        .map(|entry| (number(&entry.id), entry))
        .collect();
    let mut duties = Duties::default();
    for plan in &week.log {
        let LogPayload::DayPlanned { depart, back, .. } = plan.payload else {
            continue;
        };
        let (depart, back) = (depart.as_secs(), back.as_secs());
        let &[Subject::Sim(sim)] = plan.subjects.as_slice() else {
            panic!("a plan of one person");
        };
        if depart >= DAY_EIGHT {
            continue;
        }
        duties.planned += 1;
        let of = || (week.log.iter()).filter(move |entry| entry.subjects == plan.subjects);
        let left = of().find_map(|entry| match entry.payload {
            LogPayload::LeftHome { late_secs, .. }
                if entry.t.as_secs() - i64::from(late_secs) == depart =>
            {
                Some((entry, late_secs))
            }
            _ => None,
        });
        let missed = of().find(|entry| {
            entry.t.as_secs() == back && matches!(entry.payload, LogPayload::Missed { .. })
        });
        let waited = week.waited.get(&(sim, depart));
        let is_waited = |object: u16, offer: u16| {
            waited.is_some_and(|action| (action.object, action.offer) == (object, offer))
        };
        let explained = match (left, missed) {
            (Some((_, 0)), None) => {
                duties.on_time += 1;
                true
            }
            (Some((left, _)), None) => {
                duties.late += 1;
                let cause = match left.causes.as_slice() {
                    [Cause::Event(id)] => by_id.get(&number(id)),
                    _ => None,
                };
                cause.is_some_and(|ended| {
                    let same = ended.t == left.t && ended.subjects == left.subjects;
                    let ended = match ended.payload {
                        LogPayload::ActionEnded { object, offer, .. } => is_waited(object, offer),
                        _ => false,
                    };
                    same && ended
                })
            }
            (None, Some(missed)) => {
                duties.missed += 1;
                let by_plan = matches!(missed.causes.as_slice(),
                    [Cause::Plan(by)] if number(&by.by) == number(&plan.id));
                let begun = (of().filter(|entry| entry.t.as_secs() <= depart))
                    .filter_map(|entry| match entry.payload {
                        LogPayload::ActionStarted { object, offer, .. } => Some((object, offer)),
                        _ => None,
                    })
                    .next_back();
                let ran_on = !of().any(|entry| {
                    (depart..back).contains(&entry.t.as_secs())
                        && matches!(entry.payload, LogPayload::ActionEnded { .. })
                });
                by_plan && begun.is_some_and(|(object, offer)| is_waited(object, offer)) && ran_on
            }
            _ => false,
        };
        if !explained {
            duties
                .unexplained
                .push(format!("sim {} at {depart}", sim.get()));
        }
    }
    duties
}

/// Seconds as hours and minutes.
fn hm(secs: i64) -> String {
    format!("{}:{:02}", secs / H, secs % H / 60)
}

/// Items 1, 3 and 4 for the 20 families, each with its numbers (`--nocapture` shows them);
/// item 4 holds after every second with events (`observe`). The thresholds are the plan's, for
/// each family's week: no stretch of 60 minutes under hunger 150 next to a free eating place; at
/// least 95 percent of its departures on time, and the reason of each late or missed one in
/// the log.
#[test]
fn the_week_of_the_test_family() {
    let data = common::repository_data();
    let (mut failures, mut total, mut worst) = (Vec::new(), Duties::default(), 0);
    let mut least: Option<(u64, usize, usize)> = None;
    for i in 0..FAMILIES {
        let week = week(&data, i);
        let longest = hungry(&week);
        let duties = duties(&week);
        for (sim, &secs) in &longest {
            worst = worst.max(secs);
            if secs >= STARVING {
                failures.push(format!(
                    "family {i}, sim {}: hungry for {}",
                    sim.get(),
                    hm(secs)
                ));
            }
        }
        let hungry: Vec<String> = longest.values().map(|&secs| hm(secs)).collect();
        println!(
            "family {i:2}, ages {:?}: longest hungry {}; duties {}/{} on time, {} late, {} missed",
            ages(i),
            hungry.join(" "),
            duties.on_time,
            duties.planned,
            duties.late,
            duties.missed
        );
        if !punctual(&duties) {
            failures.push(format!(
                "family {i}: {} of {} on time",
                duties.on_time, duties.planned
            ));
        }
        // The family with the smallest share on time: a / b below c / d if a · d < c · b.
        let below = |(_, on_time, planned): (u64, usize, usize)| {
            duties.on_time * planned < on_time * duties.planned
        };
        if least.is_none_or(below) {
            least = Some((i, duties.on_time, duties.planned));
        }
        total.planned += duties.planned;
        total.on_time += duties.on_time;
        total.late += duties.late;
        total.missed += duties.missed;
        let unexplained = duties.unexplained.into_iter();
        total
            .unexplained
            .extend(unexplained.map(|what| format!("family {i}: {what}")));
    }
    println!(
        "all: worst hunger stretch {}; duties {}/{} on time, {} late, {} missed; the least punctual (family, on time, planned) {least:?}",
        hm(worst),
        total.on_time,
        total.planned,
        total.late,
        total.missed
    );
    failures.extend(total.unexplained);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Item 2 for the 20 families: adults sleep 6.5 to 9 hours and children 8.5 to 11 in at least
/// 6 of the 7 nights, each night as `slept` counts it.
#[test]
fn the_test_family_sleeps_in_range() {
    let data = common::repository_data();
    let (mut failures, mut adults, mut children) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..FAMILIES {
        let week = week(&data, i);
        let mut line = format!("family {i:2}, ages {:?}:", ages(i));
        for (sim, nights) in slept(&week) {
            let born = week.world.state().born(sim).expect("born");
            let adult = age(born, t(END)).expect("an age") >= 18;
            let (low, high) = if adult { ADULT_SLEEP } else { CHILD_SLEEP };
            let good = (nights.iter())
                .filter(|&&slept| (low..=high).contains(&slept))
                .count();
            (if adult { &mut adults } else { &mut children }).extend(nights);
            let hours: Vec<String> = nights.iter().map(|&slept| hm(slept)).collect();
            line += &format!(" {}: {} ({good} good);", sim.get(), hours.join(" "));
            if good < GOOD_NIGHTS {
                failures.push(format!("family {i}, sim {}: {good} good nights", sim.get()));
            }
        }
        println!("{line}");
    }
    let range = |nights: &[i64]| {
        let (low, high) = (nights.iter().min(), nights.iter().max());
        format!("{} to {}", hm(*low.unwrap_or(&0)), hm(*high.unwrap_or(&0)))
    };
    println!(
        "all: adults {}, children {}",
        range(&adults),
        range(&children)
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The canary of item 1: the same family in the terraced house whose fridge and dining table
/// give hunger +1 an hour, no food worth the name. A house without them, or in which nothing
/// raises hunger, does not load (`house.essentials`, `house.need_unserved`), and without an
/// eating place the measure would have nothing to count; so they stay, free and useless, and
/// the family starves next to them. Item 3 turns red with it: whoever is critically hungry at
/// a departure waits at the table. The reason of every late or missed departure is still in the
/// log, and without the state's record of what each waited for none would be.
#[test]
fn starving_next_to_useless_food_is_red() {
    let houses = common::repository_houses_with(|kinds| {
        for kind in kinds.as_array_mut().expect("the kinds") {
            if kind["id"] == "core:fridge" || kind["id"] == "core:dining_table" {
                for offer in kind["offers"].as_array_mut().expect("the offers") {
                    offer["rates"]["hunger"] = serde_json::json!(1);
                }
            }
        }
    });
    let data = common::repository_data().with_houses(houses);
    let week = week(&data, 0);
    let longest = hungry(&week);
    assert!(
        longest.values().any(|&secs| secs >= STARVING),
        "{longest:?}"
    );
    let measured = duties(&week);
    assert!(!punctual(&measured), "{measured:?}");
    assert!(measured.unexplained.is_empty(), "{measured:?}");
    let blind = Week {
        waited: BTreeMap::new(),
        ..week
    };
    let unexplained = measured.late + measured.missed;
    assert_eq!(
        duties(&blind).unexplained.len(),
        unexplained,
        "{measured:?}"
    );
}

/// Item 5: the same seed gives the same week, run in one call, in steps of one minute and
/// one second with events at a time (the measured week): the same state hash and the same log.
#[test]
fn the_week_is_the_same_however_it_is_run() {
    let data = common::repository_data();
    for i in [0, 1] {
        let stepped = week(&data, i);
        let mut once = family(&data, i);
        once.advance(END);
        let mut minutes = family(&data, i);
        for minute in (D..=END).step_by(60) {
            minutes.advance(minute);
        }
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        for (run, world, log) in [
            ("minutes", &minutes.world, &minutes.log),
            ("stepped", &stepped.world, &stepped.log),
        ] {
            assert_eq!(
                world.state_hash(),
                once.world.state_hash(),
                "family {i}, {run}"
            );
            assert_eq!(entries(log), entries(&once.log), "family {i}, {run}");
        }
    }
}

/// Item 5: saved in the middle of a walk and of a sleep, the save loads, saves to the same
/// bytes and runs on as the uninterrupted week, with the same state hash and the same entries.
/// The walk is the first of 10 s or more that begins after the first morning, saved at its
/// middle; the sleep the first of two hours or more, saved at its middle. The loaded week also
/// gives what the week in one call gave from that second on.
#[test]
fn a_save_mid_walk_and_mid_sleep_runs_on_the_same() {
    let data = common::repository_data();
    let mut whole = family(&data, 0);
    whole.advance(END);
    let houses = data.houses().expect("houses");
    let one = |entry: &LogEntry| match entry.subjects.as_slice() {
        &[Subject::Sim(sim)] => Some(sim),
        _ => None,
    };
    let walk = (whole.log.iter())
        .filter(|entry| entry.t.as_secs() > MORNING)
        .find_map(|entry| match entry.payload {
            LogPayload::ActionStarted { walk_secs, .. } if walk_secs >= 10 => {
                Some((one(entry)?, entry.t.as_secs() + i64::from(walk_secs / 2)))
            }
            _ => None,
        })
        .expect("a walk");
    let (mut sleeping, mut sleep) = (BTreeMap::new(), None);
    for entry in &whole.log {
        let Some(sim) = one(entry) else {
            continue;
        };
        match &entry.payload {
            LogPayload::ActionStarted {
                house,
                object,
                offer,
                ..
            } => {
                let def = houses.house(house).expect("its house").def();
                let kind = houses
                    .kind(&def.objects[usize::from(*object)].kind)
                    .expect("a kind");
                sleeping.insert(sim, (kind.offers[usize::from(*offer)].sleep, None));
            }
            LogPayload::Reached { .. } => {
                if let Some((true, since)) = sleeping.get_mut(&sim) {
                    *since = Some(entry.t.as_secs());
                }
            }
            LogPayload::ActionEnded { .. } => {
                if let Some((true, Some(since))) = sleeping.remove(&sim)
                    && entry.t.as_secs() - since >= 2 * H
                {
                    sleep = Some((sim, (since + entry.t.as_secs()) / 2));
                    break;
                }
            }
            _ => {}
        }
    }
    let sleep = sleep.expect("a sleep");
    for ((who, saved_at), walking) in [(walk, true), (sleep, false)] {
        let mut run = family(&data, 0);
        run.advance(saved_at);
        let person = &run.world.state().house().sims[&who];
        let inside = match person.place {
            Place::Walking { depart, arrive, .. } => {
                walking && depart.as_secs() < saved_at && saved_at < arrive.as_secs()
            }
            Place::Room(_) => !walking && person.action.is_some(),
            Place::Away => false,
        };
        assert!(inside, "{saved_at}: {:?}", person.place);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let (mut loaded, _) = World::load(&bytes, data.clone()).expect("loads");
        assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
        loaded.advance_until(&mut YearOut::default(), t(END), &mut |_| false);
        let saved = run.log.len();
        run.advance(END);
        assert_eq!(loaded.state_hash(), run.world.state_hash(), "{saved_at}");
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        let continued = entries(&loaded.take_log());
        assert_eq!(continued, entries(&run.log[saved..]), "{saved_at}");
        let after: Vec<LogEntry> = (whole.log.iter())
            .filter(|entry| entry.t.as_secs() > saved_at)
            .cloned()
            .collect();
        assert_eq!(continued, entries(&after), "{saved_at}");
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "{saved_at}");
    }
}
