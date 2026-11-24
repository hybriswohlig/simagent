//! The practice of skills in a house (ADR-0033 section 6): language from hearing two adults
//! talk, reading from the bookshelf and from school, play from the toy chest; the effective
//! seconds by window and diligence, and a `core:practiced` at each 10 effective hours. The
//! formulas are checked against tools/development_reference.py in the core's `skill` module;
//! the occasions follow tools/practice_reference.py.

mod common;

use std::collections::BTreeMap;

use common::day::Run;
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::development::{Axis, RAISED_UNTIL};
use sim_core::house::day::{YoungPlace, young_place};
use sim_core::house::state::{Duty, Place};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input, MoveOut};
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::skill::{MILESTONE_SECONDS, Skill, TALENT_AGE, diligence, effective, milestones};
use sim_core::time::{DAY, SimDuration, SimTime, YEAR};
use sim_core::world::World;

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn life_data(houses: Option<sim_core::house::data::HouseData>) -> Data {
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
        .with_houses(houses.unwrap_or_else(common::repository_houses))
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

/// One occasion of practice in the log of a run: its second, who, the skill, and its raw
/// seconds (`None` for a talk, whose hearers the state before it names).
#[derive(Clone, Copy, Debug)]
struct Occasion {
    t: i64,
    who: Who,
    skill: Skill,
    raw: u64,
}

#[derive(Clone, Copy, Debug)]
enum Who {
    One(SimId),
    /// A talk of the two at the object.
    Talk(SimId, SimId, u16),
}

fn subject(entry: &LogEntry) -> Option<SimId> {
    match entry.subjects.as_slice() {
        [Subject::Sim(sim)] => Some(*sim),
        _ => None,
    }
}

/// The occasions of practice in `log`, by the kinds of the furniture of the house of `world`.
fn occasions(world: &World, log: &[LogEntry]) -> Vec<Occasion> {
    let state = world.state();
    let houses = world.data().houses().expect("houses");
    let kind = |sim: SimId, object: u16| -> DataId {
        let home = state.household_of(sim).expect("living");
        let house = &state.house().households[&home].house;
        houses.house(house).expect("its house").def().objects[usize::from(object)]
            .kind
            .clone()
    };
    // Per person: whether a command chose its action and the second it reached the object.
    let mut running: BTreeMap<SimId, (bool, Option<i64>)> = BTreeMap::new();
    let mut out = Vec::new();
    for entry in log {
        let t = entry.t.as_secs();
        match entry.payload {
            LogPayload::ActionStarted { commanded, .. } => {
                running.insert(subject(entry).expect("one"), (commanded, None));
            }
            LogPayload::Reached { .. } => {
                let who = subject(entry).expect("one");
                running.get_mut(&who).expect("started").1 = Some(t);
            }
            LogPayload::ActionEnded { object, reason, .. } => {
                let who = subject(entry).expect("one");
                let (commanded, reached) = running.remove(&who).expect("started");
                let skill = match kind(who, object).as_str() {
                    "core:bookshelf" => Skill::Reading,
                    "core:toy_chest" => Skill::Play,
                    _ => continue,
                };
                if let (false, Some(reached)) = (commanded || reason == EndReason::Command, reached)
                {
                    let raw = u64::try_from(t - reached).expect("after");
                    out.push(Occasion {
                        t,
                        who: Who::One(who),
                        skill,
                        raw,
                    });
                }
            }
            LogPayload::CameHome {
                place: Duty::School,
            } => {
                let who = Who::One(subject(entry).expect("one"));
                out.push(Occasion {
                    t,
                    who,
                    skill: Skill::Reading,
                    raw: 3600,
                });
            }
            LogPayload::Talked { object, .. } => {
                let [Subject::Sim(a), Subject::Sim(b)] = entry.subjects[..] else {
                    panic!("two talk");
                };
                let who = Who::Talk(a, b, object);
                out.push(Occasion {
                    t,
                    who,
                    skill: Skill::Language,
                    raw: 600,
                });
            }
            _ => {}
        }
    }
    out
}

fn age(world: &World, sim: SimId, t: i64) -> SimDuration {
    let born = world.state().born(sim).expect("born");
    SimTime::from_secs(t)
        .checked_since(born)
        .expect("born before")
}

/// Those who hear a talk of `a` and `b` at `object` in the state of `world`, at `t`: the
/// persons under 18 of the household other than the two in the object's room (a small child by
/// its young place), if both talkers are 18 or older (tools/practice_reference.py `talk`).
fn hearers(world: &World, (a, b, object): (SimId, SimId, u16), t: i64) -> Vec<SimId> {
    let state = world.state();
    let adult = |sim| age(world, sim, t) >= RAISED_UNTIL;
    if !adult(a) || !adult(b) {
        return Vec::new();
    }
    let home = state.household_of(a).expect("living");
    let houses = world.data().houses().expect("houses");
    let def = houses
        .house(&state.house().households[&home].house)
        .expect("its house")
        .def();
    let room = def.objects[usize::from(object)].room;
    let young = match young_place(houses.day(), SimTime::from_secs(t)) {
        YoungPlace::Bed => Some(def.child_night_room),
        YoungPlace::DayRoom => Some(def.child_day_room),
        YoungPlace::Kita { .. } => None,
    };
    state
        .members(home)
        .filter(|sim| ![a, b].contains(sim) && age(world, *sim, t) < RAISED_UNTIL)
        .filter(|sim| match state.house().sims.get(sim) {
            Some(person) => person.place == Place::Room(room),
            None => young == Some(room),
        })
        .collect()
}

/// What a person practises at an occasion at `t`, read from the state before it: the effective
/// seconds and the milestones (hours, level in thousandths) with its talent.
fn expected(
    world: &World,
    sim: SimId,
    (skill, raw, t): (Skill, u64, i64),
) -> (u64, Vec<(u64, i64)>) {
    let state = world.state();
    let at = age(world, sim, t);
    let z_c = state
        .phenotype(sim, &Axis::Conscientiousness.trait_id(), at)
        .expect("a trait");
    let talent = state
        .phenotype(sim, &skill.talent(), TALENT_AGE)
        .expect("a trait");
    let added = effective(raw, skill.window(at), diligence(z_c));
    let before = state.house().raised[&sim].practice[skill.index()];
    let learning = &world.data().demography().learning;
    let crossed = milestones(talent, (before, before + added), learning);
    let crossed = crossed.iter().map(|m| (m.hours, m.level_milli)).collect();
    (added, crossed)
}

/// Runs `data` from the family `born` for `days` twice: once to read the occasions from its log,
/// once a second at a time, checking at each second with events that each raised person's
/// practice grew by what its occasions there give from the state before them, and that each
/// `core:practiced` is one of their milestones, by the occasion. Returns the number of occasions
/// per skill that practised and the number of milestones.
fn check(data: impl Fn() -> Data, born: &[i64], days: i64) -> ([usize; 3], usize) {
    let (mut first, _) = Run::family(data(), 7, "test:practice", born, 0);
    first.advance(days * DAY.as_secs());
    let all = occasions(&first.world, &first.log);
    let (mut run, _) = Run::family(data(), 7, "test:practice", born, 0);
    let start = run.log.len();
    let mut next = 0;
    let mut want_at = i64::MIN;
    let mut counts = [0; 3];
    let mut milestones_want: Vec<(i64, SimId, Skill, u64, i64)> = Vec::new();
    let practice = |world: &World| -> BTreeMap<SimId, [u64; 3]> {
        (world.state().house().raised.iter())
            .map(|(sim, raised)| (*sim, raised.practice))
            .collect()
    };
    type Prepared = BTreeMap<SimId, (Skill, (u64, Vec<(u64, i64)>))>;
    let prepare = |world: &World, next: usize, want: &mut Prepared, want_at: &mut i64| {
        want.clear();
        let Some(first) = all.get(next) else {
            *want_at = i64::MAX;
            return;
        };
        *want_at = first.t;
        let mut seen = BTreeMap::new();
        for occasion in all[next..].iter().take_while(|o| o.t == first.t) {
            let people = match occasion.who {
                Who::One(sim) => vec![sim],
                Who::Talk(a, b, object) => hearers(world, (a, b, object), occasion.t),
            };
            for sim in people {
                let raised = world.state().house().raised.contains_key(&sim);
                let housed = world.state().household_of(sim).is_some();
                if !raised || !housed {
                    continue;
                }
                assert!(
                    seen.insert(sim, ()).is_none(),
                    "two occasions of one person in a second"
                );
                let got = expected(world, sim, (occasion.skill, occasion.raw, occasion.t));
                want.insert(sim, (occasion.skill, got));
            }
        }
    };
    let mut prepared = Prepared::new();
    prepare(&run.world, next, &mut prepared, &mut want_at);
    let mut last = practice(&run.world);
    run.step(days * DAY.as_secs(), |world, second| {
        let now = practice(world);
        for (sim, after) in &now {
            let before = last.get(sim).copied().unwrap_or([0; 3]);
            let mut grew = [0; 3];
            for at in 0..3 {
                grew[at] = after[at] - before[at];
            }
            match (second == want_at).then(|| prepared.get(sim)).flatten() {
                Some((skill, (added, crossed))) => {
                    let mut expect = [0; 3];
                    expect[skill.index()] = *added;
                    assert_eq!(grew, expect, "sim {} at {second}", sim.get());
                    for (hours, milli) in crossed {
                        milestones_want.push((second, *sim, *skill, *hours, *milli));
                    }
                    if *added > 0 {
                        counts[skill.index()] += 1;
                    }
                }
                None => assert_eq!(grew, [0; 3], "sim {} at {second}", sim.get()),
            }
        }
        last = now;
        if second >= want_at {
            assert_eq!(second, want_at, "an occasion was skipped");
            next += all[next..].iter().take_while(|o| o.t == want_at).count();
        }
        prepare(world, next, &mut prepared, &mut want_at);
    });
    let got: Vec<_> = (run.log[start..].iter())
        .filter_map(|e| match e.payload {
            LogPayload::Practiced {
                skill,
                hours,
                level_milli,
            } => {
                assert!(matches!(e.causes.as_slice(), [Cause::Event(_)]));
                Some((
                    e.t.as_secs(),
                    subject(e).expect("one"),
                    skill,
                    hours,
                    level_milli,
                ))
            }
            _ => None,
        })
        .collect();
    assert_eq!(got, milestones_want);
    assert_eq!(MILESTONE_SECONDS, 36_000);
    (counts, got.len())
}

/// The girls of 12 and 17 read at the bookshelf, play at the toy chest and go to school; the
/// couple talk where they are. Each occasion adds its effective seconds to the girl and to no one
/// else: an hour per school day, the seconds at the object of each reading or play neither begun
/// nor ended by a command, 600 seconds of each talk of the couple in her room, by the window of
/// her age and her diligence.
#[test]
fn reading_play_school_and_talks_practise_as_the_reference() {
    let (counts, crossed) = check(|| life_data(None), &FAMILY, 15);
    assert!(counts.iter().all(|n| *n >= 2), "{counts:?}");
    assert!(crossed >= 2, "{crossed} milestones");
}

/// A baby whose day room is the living room hears the couple talk there in full: language under 6
/// counts each talk's 600 seconds by its diligence alone.
#[test]
fn a_baby_hears_its_parents_in_the_living_room() {
    let data = || {
        let houses = common::repository_houses_with_each(|house| {
            house["child_day_room"] = json!("living");
        });
        life_data(Some(houses))
    };
    let (counts, _) = check(data, &[FAMILY[0], FAMILY[1]], 4 * 28);
    assert!(counts[Skill::Language.index()] >= 20, "{counts:?}");
}

/// Without a move-in no one practises; the couple, never raised, practise nothing in the house.
#[test]
fn only_the_raised_in_a_house_practise() {
    let mut founded = Run::founded(life_data(None), 7, "test:practice", &FAMILY);
    founded.advance(15 * DAY.as_secs());
    assert!(founded.world.state().house().raised.is_empty());
    let (mut run, _) = Run::family(life_data(None), 7, "test:practice", &FAMILY, 0);
    run.advance(15 * DAY.as_secs());
    let raised = &run.world.state().house().raised;
    assert!(!raised.contains_key(&sim(1)) && !raised.contains_key(&sim(2)));
    assert!(raised[&sim(3)].practice.iter().all(|s| *s > 0));
}

/// A reading begun by a command practises nothing at its end.
#[test]
fn reading_on_command_practises_nothing() {
    let (mut run, home) = Run::family(life_data(None), 7, "test:practice", &FAMILY, 0);
    let houses = run.world.data().houses().expect("houses");
    let house = &run.world.state().house().households[&home].house;
    let def = houses.house(house).expect("its house").def();
    let shelf = (def.objects.iter())
        .position(|object| object.kind.as_str() == "core:bookshelf")
        .map(|at| u16::try_from(at).expect("fits"))
        .expect("a bookshelf");
    let mut at = 2 * DAY.as_secs() + 17 * 3600;
    loop {
        run.advance(at);
        let told = Input::Command(Command {
            sim: sim(3),
            object: shelf,
            offer: 0,
        });
        if run.world.submit(told).is_ok() {
            run.log.extend(run.world.take_log());
            break;
        }
        at += 600;
        assert!(at < 3 * DAY.as_secs(), "never free to be told");
    }
    let before = run.world.state().house().raised[&sim(3)].practice;
    let started = run.log.len();
    loop {
        at += 60;
        run.advance(at);
        let ended = (run.log[started..].iter()).any(|e| {
            e.subjects == [Subject::Sim(sim(3))]
                && matches!(e.payload, LogPayload::ActionEnded { .. })
        });
        if ended {
            break;
        }
        assert!(at < 4 * DAY.as_secs(), "it never ended");
    }
    let reached = (run.log[started..].iter()).any(|e| {
        e.subjects == [Subject::Sim(sim(3))] && matches!(e.payload, LogPayload::Reached { .. })
    });
    assert!(reached);
    let after = run.world.state().house().raised[&sim(3)].practice;
    assert_eq!(
        after[Skill::Reading.index()],
        before[Skill::Reading.index()]
    );
}

/// Leaving the house in the middle of reading or play counts the seconds it ran (ADR-0033
/// section 6): the girl of 12 at the bookshelf or the toy chest for some minutes when the
/// household moves out; her action ends with reason left and her practice grows by those seconds
/// at its end, by her window and diligence then.
#[test]
fn moving_out_in_the_middle_counts_the_seconds_so_far() {
    let (mut first, _) = Run::family(life_data(None), 7, "test:practice", &FAMILY, 0);
    first.advance(15 * DAY.as_secs());
    let girl = sim(3);
    // A long reading or play of hers to the end: its arrival and end.
    let mut reached = None;
    let mut long = None;
    for entry in &first.log {
        if subject(entry) != Some(girl) {
            continue;
        }
        match entry.payload {
            LogPayload::Reached { object } => reached = Some((entry.t.as_secs(), object)),
            LogPayload::ActionEnded {
                object,
                reason: EndReason::Done,
                ..
            } => {
                let kind = first.world.data().houses().map(|houses| {
                    let state = first.world.state();
                    let home = state.household_of(girl).expect("living");
                    let house = &state.house().households[&home].house;
                    houses.house(house).expect("its house").def().objects[usize::from(object)]
                        .kind
                        .clone()
                });
                let own = kind.as_ref().is_some_and(|kind| {
                    matches!(kind.as_str(), "core:bookshelf" | "core:toy_chest")
                });
                if let Some((at, _)) = reached.filter(|_| own)
                    && entry.t.as_secs() - at > 1_200
                {
                    long = Some((at, kind_skill(kind.expect("a kind").as_str())));
                    break;
                }
            }
            _ => {}
        }
    }
    let (at, skill) = long.expect("a long reading or play");
    let (mut run, home) = Run::family(life_data(None), 7, "test:practice", &FAMILY, 0);
    run.advance(at + 600);
    let before = run.world.state().house().raised[&girl].practice;
    let doing = run.world.state().house().sims[&girl].action.expect("at it");
    assert_eq!(doing.reached, Some(SimTime::from_secs(at)));
    let start = run.log.len();
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let ended = (run.log[start..].iter())
        .find(|e| subject(e) == Some(girl) && matches!(e.payload, LogPayload::ActionEnded { .. }))
        .expect("her action ends");
    assert!(matches!(
        ended.payload,
        LogPayload::ActionEnded {
            reason: EndReason::Left,
            ..
        }
    ));
    let t = ended.t.as_secs();
    let raw = u64::try_from(t - at).expect("after");
    assert!(raw >= 600);
    let age = SimTime::from_secs(t).checked_since(run.world.state().born(girl).expect("born"));
    let age = age.expect("born before");
    let z_c = (run.world.state())
        .phenotype(girl, &Axis::Conscientiousness.trait_id(), age)
        .expect("a trait");
    let added = effective(raw, skill.window(age), diligence(z_c));
    let after = run.world.state().house().raised[&girl].practice;
    let mut want = before;
    want[skill.index()] += added;
    assert_eq!(after, want);
}

fn kind_skill(kind: &str) -> Skill {
    match kind {
        "core:bookshelf" => Skill::Reading,
        _ => Skill::Play,
    }
}
