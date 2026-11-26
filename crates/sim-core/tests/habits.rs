//! Habits in the choice (ADR-0033 sections 7, 9 and 10): a habit per (furniture kind, offer,
//! quarter of the day of the choice), +100 at each end with `done` or `full` not chosen by a
//! command, decaying 20 a day; `15 · strength` points on the offer by day, below the share of a
//! critical need, and a `core:leaning` beside the choice. Expected values from
//! tools/development_reference.py (section 7: `habit_complete`, `habits_at`, `habit_completes`,
//! `leaning_rows`); the run is checked against a port of those functions here.

mod common;

use std::collections::BTreeMap;

use common::day::{Run, places_hold};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::habit::{self, Habit};
use sim_core::house::choice::{Bonus, Choice, ENOUGH, age, choose, crosses_departure, score};
use sim_core::house::data::{House, HouseData};
use sim_core::house::day::{departure, night, work_draw};
use sim_core::house::state::{Broken, HouseState, Place, consistent};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input};
use sim_core::log::{EndReason, Leaning, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, SimTime, YEAR};

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn id(text: &str) -> DataId {
    DataId::new(text).expect("valid")
}

fn at(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn life_data() -> Data {
    let (mut calibration, genome_map) = common::repository_files();
    let values = &mut calibration["values"];
    for sex in ["female", "male"] {
        values[format!("core:accident_{sex}")]["value"] = json!(1e-9);
        values[format!("core:senescence_a_{sex}")]["value"] = json!(1e-15);
        values[format!("core:senescence_b_{sex}")]["value"] = json!(0.001);
    }
    calibration["diseases"] = json!([]);
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(1.0);
    demography["values"]["core:twins_identical"]["value"] = json!(0.0);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social())
        .with_upbringing(common::repository_upbringing())
}

/// A woman of 30 and a man of 32 who take in a girl of 12 and a girl of 17.
const FAMILY: [i64; 4] = [
    -30 * YEAR.as_secs(),
    -32 * YEAR.as_secs(),
    -12 * YEAR.as_secs(),
    -17 * YEAR.as_secs(),
];

// ---------------------------------------------------------------------------------------------
// The habit arithmetic (tools/development_reference.py, "Habits (7)").

/// Quarter of the day: 05:59:59 -> 0, 06:00 -> 1, 12:00 -> 2, 18:00 -> 3, 23:59:59 -> 3; 07:00
/// of day 3 -> 1.
#[test]
fn the_quarter_of_the_day_is_that_of_the_reference() {
    let seconds = [
        0,
        21_599,
        21_600,
        43_200,
        64_800,
        86_399,
        3 * 86_400 + 25_200,
    ];
    let quarters: Vec<u8> = seconds.iter().map(|&s| habit::quarter(at(s))).collect();
    assert_eq!(quarters, [0, 0, 1, 2, 3, 3, 1]);
}

/// The acceptance test of ADR-0033: habit 0, then 100, after two days 60, after one more
/// completion 160; 15 points per unit, 2400 for 160 and 15 000 at most.
#[test]
fn habit_zero_hundred_sixty_and_one_sixty() {
    let key = (&id("test:seven"), 0, habit::quarter(at(25_200)));
    assert_eq!(key.2, 1);
    let (t0, t2) = (25_200, 25_200 + 2 * 86_400);
    let mut habits: Vec<Habit> = Vec::new();
    assert_eq!(habit::strength(&habits, key, at(t0)), 0);
    habit::complete(&mut habits, key, at(t0));
    assert_eq!(habit::strength(&habits, key, at(t0)), 100);
    assert_eq!(habit::strength(&habits, key, at(t2)), 60);
    habit::complete(&mut habits, key, at(t2));
    assert_eq!(habit::strength(&habits, key, at(t2)), 160);
    assert_eq!(habit::points(160), 2400);
    assert_eq!(habit::points(1000), 15_000);
    assert_eq!(habit::points(0), 0);
}

/// Integer decay (20 · 4319 / 86400 = 0, 20 · 4320 / 86400 = 1), the cap at 1000 and a habit
/// at 0 gone after five days.
#[test]
fn decay_is_whole_and_the_cap_holds() {
    let kind = id("test:seven");
    let one = |value, since| Habit {
        kind: kind.clone(),
        offer: 0,
        quarter: 1,
        value,
        since: at(since),
    };
    assert_eq!(one(100, 0).strength(at(4_319)), 100);
    assert_eq!(one(100, 0).strength(at(4_320)), 99);
    assert_eq!(one(100, 0).strength(at(5 * 86_400 - 1)), 1);
    assert_eq!(one(100, 0).strength(at(5 * 86_400)), 0);
    let mut capped = vec![one(950, 25_200)];
    habit::complete(&mut capped, (&kind, 0, 1), at(25_200));
    assert_eq!(capped, [one(1000, 25_200)]);
    // Only the completed habit is stored again: A (100 at 0) keeps its value and second, B
    // completed at 1.5 days is 100 − 30 + 100 = 170 at that second.
    let pair = |name: &str, value, since| Habit {
        kind: id(name),
        offer: 0,
        quarter: 0,
        value,
        since: at(since),
    };
    let mut both = vec![pair("test:a", 100, 0), pair("test:b", 100, 0)];
    habit::complete(&mut both, (&id("test:b"), 0, 0), at(129_600));
    assert_eq!(both, [pair("test:a", 100, 0), pair("test:b", 170, 129_600)]);
}

/// At most 8: eight habits of 100 to 800 for the keys (1..8, 0, 0); a ninth drops the weakest.
/// On a tie the smallest key in (kind, offer, quarter) order falls; strengthening a key that is
/// there drops nothing; a habit at 0 goes before the list is counted.
#[test]
fn a_full_list_drops_the_weakest() {
    let kind = |n: u16| id(&format!("test:k{n:02}"));
    let habit = |n: u16, offer: u16, quarter: u8, value: u16| Habit {
        kind: kind(n),
        offer,
        quarter,
        value,
        since: at(0),
    };
    let full: Vec<Habit> = (1..=8).map(|n| habit(n, 0, 0, 100 * n)).collect();
    let mut nine = full.clone();
    habit::complete(&mut nine, (&kind(9), 0, 0), at(0));
    let want: Vec<Habit> = (2..=9)
        .map(|n| habit(n, 0, 0, if n == 9 { 100 } else { 100 * n }))
        .collect();
    assert_eq!(nine, want);
    let mut again = full.clone();
    habit::complete(&mut again, (&kind(8), 0, 0), at(0));
    assert_eq!(again.len(), 8);
    assert_eq!(again[7], habit(8, 0, 0, 900));
    assert_eq!(again[0], habit(1, 0, 0, 100));
    let mut tie = vec![
        habit(3, 0, 1, 300),
        habit(3, 1, 0, 300),
        habit(4, 0, 0, 300),
        habit(5, 0, 2, 300),
        habit(6, 0, 0, 300),
        habit(7, 0, 0, 300),
        habit(8, 0, 0, 300),
        habit(9, 0, 0, 300),
    ];
    habit::complete(&mut tie, (&kind(10), 0, 0), at(0));
    assert_eq!(tie.len(), 8);
    assert!(!tie.contains(&habit(3, 0, 1, 300)));
    assert!(tie.contains(&habit(3, 1, 0, 300)));
    // Eight habits, the first gone to 0 after five days: the newcomer takes its place and no
    // other falls.
    let mut faded: Vec<Habit> = (1..=8).map(|n| habit(n, 0, 0, 100 * n)).collect();
    habit::complete(&mut faded, (&kind(9), 0, 0), at(5 * 86_400));
    assert_eq!(faded.len(), 8);
    assert_eq!(faded[0], habit(2, 0, 0, 200));
}

/// Which ends strengthen: `done` and `full` without a command.
#[test]
fn only_done_and_full_without_a_command_strengthen() {
    use EndReason::*;
    let reasons = [Done, Full, Left, Critical, Obligation, Command, Collapse];
    let got: Vec<bool> = reasons
        .iter()
        .map(|&r| habit::completes(r, false))
        .collect();
    assert_eq!(got, [true, true, false, false, false, false, false]);
    assert!(!habit::completes(Done, true) && !habit::completes(Full, true));
}

// ---------------------------------------------------------------------------------------------
// The habit in the points (house_choice's flat; tools/development_reference.py, "leaning").

fn flat(houses: &HouseData) -> &House {
    houses.house(&id("core:flat")).expect("the flat")
}

fn no_mood(_: u16, _: u16) -> i8 {
    0
}

/// The most a habit gives stays below a critical need: hunger at 150 makes the table
/// (1 000 329 800) beat the television with a full habit (54 850), and the choice takes it.
#[test]
fn a_critical_need_always_wins() {
    let houses = common::repository_houses();
    let values = [150, 900, 500, 700, 800];
    let full = habit::points(1000);
    let scored = score(
        flat(&houses),
        &houses,
        (30, values, 0),
        (None, None),
        no_mood,
        0,
        |object, _| Bonus::habit(if object == 3 { full } else { 0 }),
    );
    let of = |object: u16| {
        (scored.iter())
            .find(|s| s.object == object)
            .map(|s| s.points)
    };
    assert_eq!(of(0), Some(1_000_329_800));
    assert_eq!(of(3), Some(54_850));
    let chosen = choose((flat(&houses), &houses), (30, values), &scored, &[], None);
    assert!(
        matches!(chosen, Choice::Take(s) if s.object == 0),
        "{chosen:?}"
    );
}

/// A habit tips a close choice and is not scaled by the mood: at the morning values television
/// and bookshelf tie at 39 850, below `ENOUGH`; a habit of 100 on the bookshelf gives it 1500 more,
/// 41 350, with any mood of the offer the same 1500 over its points without.
#[test]
fn a_habit_tips_a_close_choice_outside_the_mood() {
    let houses = common::repository_houses();
    let morning = [600, 900, 500, 700, 800];
    let weigh = |moods: &dyn Fn(u16, u16) -> i8, habit_on: Option<u16>| {
        score(
            flat(&houses),
            &houses,
            (30, morning, 0),
            (None, None),
            moods,
            0,
            |object, offer| match (habit_on == Some(object), offer) {
                (true, 0) => Bonus::habit(habit::points(100)),
                _ => Bonus::NONE,
            },
        )
    };
    let of = |scored: &[sim_core::house::choice::Scored], object: u16| {
        (scored.iter())
            .find(|s| s.object == object)
            .map(|s| s.points)
    };
    let plain = weigh(&no_mood, None);
    let leaning = weigh(&no_mood, Some(4));
    assert_eq!((of(&plain, 3), of(&plain, 4)), (Some(39_850), Some(39_850)));
    assert_eq!(of(&leaning, 4), Some(41_350));
    assert!(of(&leaning, 4).expect("scored") >= ENOUGH);
    let low = [0, 1, 2, 5, 6];
    let chose = choose(
        (flat(&houses), &houses),
        (30, morning),
        &leaning,
        &low,
        None,
    );
    assert!(
        matches!(chose, Choice::Take(s) if s.object == 4),
        "{chose:?}"
    );
    let glum = |_: u16, _: u16| -10;
    let (sad, sad_leaning) = (weigh(&glum, None), weigh(&glum, Some(4)));
    assert_eq!(
        of(&sad_leaning, 4).zip(of(&sad, 4)).map(|(a, b)| a - b),
        Some(1500)
    );
}

// ---------------------------------------------------------------------------------------------
// The habits of a run (a port of the reference's `habit_complete`, `habits_at`,
// `habit_completes` and `leaning_rows`).

type Key = (DataId, u16, u8);

/// `habit_strength`: the stored value less 20 · (t − since) / 86400 toward 0, never below 0.
fn strength_of((value, since): (u16, i64), t: i64) -> i64 {
    (i64::from(value) - 20 * (t - since) / 86_400).max(0)
}

/// `habit_complete` of the reference.
fn complete(habits: &mut BTreeMap<Key, (u16, i64)>, key: Key, t: i64) {
    habits.retain(|_, stored| strength_of(*stored, t) > 0);
    let current = habits.get(&key).map_or(0, |stored| strength_of(*stored, t));
    let value = (current + 100).min(1000);
    if !habits.contains_key(&key) && habits.len() >= 8 {
        let drop = (habits.iter())
            .map(|(k, stored)| (strength_of(*stored, t), k.clone()))
            .min()
            .map(|(_, k)| k)
            .expect("full");
        habits.remove(&drop);
    }
    habits.insert(key, (u16::try_from(value).expect("fits"), t));
}

fn subject(entry: &LogEntry) -> Option<SimId> {
    match entry.subjects.as_slice() {
        [Subject::Sim(sim)] => Some(*sim),
        _ => None,
    }
}

/// What the habits of a run came to: completions, leaning entries and their rows.
#[derive(Debug, Default)]
struct Seen {
    completions: usize,
    leanings: usize,
    rows: usize,
    tipped: usize,
    /// Of `tipped`, the time-fillers.
    fillers: usize,
}

/// Runs the family for `days` twice: once for its log, once a second at a time, replaying the
/// log in order through the reference's rules. After each second with events each person's habits
/// equal the replay's, its running action keeps the quarter of the second it began, and the
/// state passes the loader's checks. Each `core:leaning` comes by day, its rows the offers whose
/// habit the replay holds above 0 at that second, in order, with `15 · strength` points and no
/// niche; each choice by day of an offer with a habit above 0 has one with a row for it, but a
/// time-filler at an offer today's departure closed, which the choice did not weigh.
fn replay(days: i64) -> Seen {
    let (mut first, home) = Run::family(life_data(), 7, "test:habits", &FAMILY, 0);
    first.advance(days * DAY.as_secs());
    let log = first.log.clone();
    let houses = first.world.data().houses().expect("houses");
    let house = &first.world.state().house().households[&home].house;
    let house_def = houses.house(house).expect("its house");
    let kinds: Vec<DataId> = (house_def.def().objects.iter())
        .map(|o| o.kind.clone())
        .collect();
    let (mut run, _) = Run::family(life_data(), 7, "test:habits", &FAMILY, 0);
    let mut next = log
        .iter()
        .position(|e| e.t.as_secs() >= DAY.as_secs() - 1)
        .expect("moved in");
    let mut model: BTreeMap<SimId, BTreeMap<Key, (u16, i64)>> = BTreeMap::new();
    let mut running: BTreeMap<SimId, (bool, u8)> = BTreeMap::new();
    let mut leaned: BTreeMap<SimId, (i64, Vec<Leaning>)> = BTreeMap::new();
    let mut seen = Seen::default();
    let day_rules = houses.day().clone();
    run.step(days * DAY.as_secs(), |world, second| {
        let state = world.state();
        while let Some(entry) = log.get(next).filter(|e| e.t.as_secs() <= second) {
            next += 1;
            let t = entry.t.as_secs();
            let Some(who) = subject(entry) else {
                continue;
            };
            let born = state.born(who).expect("born");
            let by_day = night(&day_rules, born, entry.t).is_none();
            let strength = |object: u16, offer: u16| {
                let key = (
                    kinds[usize::from(object)].clone(),
                    offer,
                    habit::quarter(entry.t),
                );
                (model.get(&who).and_then(|h| h.get(&key)))
                    .map_or(0, |stored| strength_of(*stored, t))
            };
            match &entry.payload {
                LogPayload::Leaning { rows } => {
                    assert!(by_day, "a leaning in the night at {t}");
                    assert!(!rows.is_empty());
                    let keys: Vec<(u16, u16)> = rows.iter().map(|r| (r.object, r.offer)).collect();
                    assert!(
                        keys.windows(2).all(|w| w[0] < w[1]),
                        "rows out of order at {t}"
                    );
                    for row in rows {
                        let s = strength(row.object, row.offer);
                        assert!(s > 0, "a row without a habit at {t}: {row:?}");
                        let want = Leaning {
                            object: row.object,
                            offer: row.offer,
                            habit: 15 * s,
                            niche: 0,
                            skill: None,
                            sibling: None,
                            lead_milli: None,
                        };
                        assert_eq!(*row, want, "at {t}");
                    }
                    seen.leanings += 1;
                    seen.rows += rows.len();
                    leaned.insert(who, (t, rows.clone()));
                }
                LogPayload::ActionStarted {
                    object,
                    offer,
                    commanded,
                    filler,
                    room,
                    needs,
                    ..
                } => {
                    running.insert(who, (*commanded, habit::quarter(entry.t)));
                    let rows = leaned.remove(&who).filter(|(when, _)| *when == t);
                    // A time-filler may take an offer today's departure closed, which the choice
                    // did not weigh; any other it chose was weighed.
                    let seed = state.world_seed().expect("started");
                    let key = state.lineage_key(who).expect("a person");
                    let leaves = departure(&day_rules, born, || work_draw(seed, key), entry.t);
                    let depart_in = leaves.map(|leaves| (leaves - entry.t).as_secs());
                    let kind = houses.kind(&kinds[usize::from(*object)]).expect("a kind");
                    let def = &kind.offers[usize::from(*offer)];
                    let years = age(born, entry.t).expect("an age");
                    let walk = house_def.walk_to(*room, *object);
                    let closed =
                        crosses_departure(def, houses.rates(), (years, *needs), walk, depart_in);
                    let weighed = !filler || !closed;
                    if !commanded && weighed && by_day && strength(*object, *offer) > 0 {
                        let rows = rows.expect("a leaning before the choice").1;
                        assert!(
                            rows.iter()
                                .any(|r| (r.object, r.offer) == (*object, *offer))
                        );
                        seen.tipped += 1;
                        seen.fillers += usize::from(*filler);
                    }
                }
                LogPayload::Idle { .. } => {
                    leaned.remove(&who);
                }
                LogPayload::ActionEnded {
                    object,
                    offer,
                    reason,
                    ..
                } => {
                    let (commanded, quarter) = running.remove(&who).expect("started");
                    let completes = matches!(reason, EndReason::Done | EndReason::Full);
                    if completes && !commanded {
                        let key = (kinds[usize::from(*object)].clone(), *offer, quarter);
                        complete(model.entry(who).or_default(), key, t);
                        seen.completions += 1;
                    }
                }
                _ => {}
            }
        }
        for (sim, person) in &state.house().sims {
            let got: BTreeMap<Key, (u16, i64)> = (person.habits.iter())
                .map(|h| {
                    (
                        (h.kind.clone(), h.offer, h.quarter),
                        (h.value, h.since.as_secs()),
                    )
                })
                .collect();
            let keys: Vec<Key> = person
                .habits
                .iter()
                .map(|h| (h.kind.clone(), h.offer, h.quarter))
                .collect();
            assert!(keys.windows(2).all(|w| w[0] < w[1]), "habits out of order");
            let want = model.get(sim).cloned().unwrap_or_default();
            assert_eq!(got, want, "sim {} after {second}", sim.get());
            if let Some(action) = person.action {
                assert_eq!(Some(action.quarter), running.get(sim).map(|r| r.1));
            }
        }
        places_hold(world, second);
    });
    assert_eq!(next, log.len(), "the second run wrote the first's log");
    seen
}

/// Eight days of the family: everyone with needs builds habits by its own choices, they decay
/// and come back as the reference says, and the choices they tip say so in `core:leaning`.
#[test]
fn a_run_keeps_its_habits_as_the_reference() {
    let seen = replay(8);
    assert!(seen.completions >= 40, "{seen:?}");
    assert!(
        seen.leanings >= 20 && seen.rows >= seen.leanings,
        "{seen:?}"
    );
    assert!(seen.tipped >= 10 && seen.fillers >= 10, "{seen:?}");
}

/// A reading begun by a command strengthens no habit at its end, though it ends `done`.
#[test]
fn a_commanded_action_strengthens_nothing() {
    let (mut run, home) = Run::family(life_data(), 7, "test:habits", &FAMILY, 0);
    let houses = run.world.data().houses().expect("houses");
    let house = &run.world.state().house().households[&home].house;
    let def = houses.house(house).expect("its house").def();
    let shelf = (def.objects.iter())
        .position(|object| object.kind.as_str() == "core:bookshelf")
        .map(|at| u16::try_from(at).expect("fits"))
        .expect("a bookshelf");
    let mut t = 2 * DAY.as_secs() + 17 * 3600;
    loop {
        run.advance(t);
        let told = Input::Command(Command {
            sim: sim(3),
            object: shelf,
            offer: 0,
        });
        if run.world.submit(told).is_ok() {
            run.log.extend(run.world.take_log());
            break;
        }
        t += 600;
        assert!(t < 3 * DAY.as_secs(), "never free to be told");
    }
    let habits = |run: &Run| run.world.state().house().sims[&sim(3)].habits.clone();
    let before = habits(&run);
    let started = run.log.len();
    let reason = loop {
        t += 60;
        run.advance(t);
        let ended = (run.log[started..].iter()).find_map(|e| match e.payload {
            LogPayload::ActionEnded { reason, .. } if e.subjects == [Subject::Sim(sim(3))] => {
                Some(reason)
            }
            _ => None,
        });
        if let Some(reason) = ended {
            break reason;
        }
        assert!(t < 4 * DAY.as_secs(), "it never ended");
    };
    assert_eq!(reason, EndReason::Done);
    let ended_at = (run.log[started..].iter())
        .filter(|e| e.subjects == [Subject::Sim(sim(3))])
        .find(|e| matches!(e.payload, LogPayload::ActionEnded { .. }))
        .map(|e| e.t)
        .expect("ended");
    // Nothing else of hers ended in between, and her habits are as they were.
    let ends = (run.log[started..].iter())
        .filter(|e| e.subjects == [Subject::Sim(sim(3))])
        .filter(|e| matches!(e.payload, LogPayload::ActionEnded { .. }) && e.t <= ended_at)
        .count();
    assert_eq!(ends, 1);
    let after_end: Vec<Habit> = habits(&run);
    let changed_later = (run.log[started..].iter()).any(|e| {
        e.subjects == [Subject::Sim(sim(3))]
            && e.t > ended_at
            && matches!(e.payload, LogPayload::ActionEnded { .. })
    });
    if !changed_later {
        assert_eq!(after_end, before);
    }
    assert!(
        !before
            .iter()
            .chain(&after_end)
            .any(|h| h.kind.as_str() == "core:bookshelf" && h.since == ended_at)
    );
}

/// The loader checks the habits and the quarter of a running action (ADR-0033 sections 7 and 9):
/// at most 8, in the order of their keys, of a kind and offer the house data hold, a quarter 0 to
/// 3, a value from 100 to 1000 written down at or before the clock; on the way an action's quarter
/// is that of its walk's start.
#[test]
fn the_loader_checks_the_habits_and_the_quarter() {
    let (mut run, _) = Run::family(life_data(), 7, "test:habits", &FAMILY, 0);
    let (mut walked, mut listed) = (0, false);
    run.step(3 * DAY.as_secs(), |world, second| {
        let state = world.state();
        let houses = world.data().houses();
        let sim = |id: u64| SimId::try_from(id).expect("an id");
        let allocated = state.sim_ids_allocated();
        let alive = |home| (1..=allocated).any(|id| state.household_of(sim(id)) == Some(home));
        let member = |sim: SimId| state.household_of(sim).zip(state.born(sim));
        let now = at(second + 1);
        let check = |change: &dyn Fn(&mut HouseState)| {
            let mut house = state.house().clone();
            change(&mut house);
            consistent(&house, houses, now, None, alive, member)
        };
        assert_eq!(check(&|_| {}), Ok(()));
        for (&who, person) in &state.house().sims {
            if let (Place::Walking { .. }, Some(action)) = (person.place, person.action) {
                walked += 1;
                let other = |house: &mut HouseState| {
                    let moved = house.sims.get_mut(&who).and_then(|p| p.action.as_mut());
                    let action = moved.expect("its action");
                    action.quarter = (action.quarter + 1) % 4;
                };
                assert_eq!(check(&other), Err(Broken::Habit));
                assert!(action.quarter < 4);
            }
        }
        if listed || walked < 3 {
            return;
        }
        listed = true;
        // One person's list, rewritten: keys from the house's kinds and quarters, by key.
        let who = *state.house().sims.keys().next().expect("a person");
        let kinds: Vec<DataId> = {
            let houses = houses.expect("houses");
            let mut kinds: Vec<DataId> = (houses.kinds().iter())
                .filter(|kind| !kind.offers.is_empty())
                .map(|kind| kind.id.clone())
                .collect();
            kinds.sort();
            kinds
        };
        let list = |n: usize| -> Vec<Habit> {
            let mut out: Vec<Habit> = (kinds.iter())
                .flat_map(|kind| (0..4).map(move |quarter| (kind.clone(), quarter)))
                .take(n)
                .map(|(kind, quarter)| Habit {
                    kind,
                    offer: 0,
                    quarter,
                    value: 100,
                    since: at(second),
                })
                .collect();
            out.sort_by(|a, b| (&a.kind, a.offer, a.quarter).cmp(&(&b.kind, b.offer, b.quarter)));
            out
        };
        let with = |habits: Vec<Habit>| {
            check(&move |house: &mut HouseState| {
                house.sims.get_mut(&who).expect("her").habits = habits.clone();
            })
        };
        let edit = |change: fn(&mut Habit)| {
            let mut habits = list(2);
            change(&mut habits[1]);
            with(habits)
        };
        assert_eq!(with(list(8)), Ok(()));
        assert_eq!(with(list(9)), Err(Broken::Habit));
        assert_eq!(
            with(list(2).into_iter().rev().collect()),
            Err(Broken::Habit)
        );
        assert_eq!(edit(|h| h.value = 1000), Ok(()));
        assert_eq!(edit(|h| h.value = 99), Err(Broken::Habit));
        assert_eq!(edit(|h| h.value = 1001), Err(Broken::Habit));
        assert_eq!(edit(|h| h.quarter = 4), Err(Broken::Habit));
        assert_eq!(edit(|h| h.offer = 99), Err(Broken::Habit));
        assert_eq!(
            edit(|h| h.kind = id("test:no_such_kind")),
            Err(Broken::Habit)
        );
        assert_eq!(
            edit(|h| h.since = at(h.since.as_secs() + 2)),
            Err(Broken::Habit)
        );
    });
    assert!(listed, "{walked} walks");
}

/// Without a move-in nobody has needs, and so no habit and no `core:leaning`.
#[test]
fn without_a_move_in_there_is_no_habit() {
    let mut founded = Run::founded(life_data(), 7, "test:habits", &FAMILY);
    founded.advance(4 * DAY.as_secs());
    assert!(founded.world.state().house().sims.is_empty());
    let leaning = (founded.log.iter()).any(|e| matches!(e.payload, LogPayload::Leaning { .. }));
    assert!(!leaning);
}
