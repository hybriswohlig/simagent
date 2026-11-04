//! The day's rhythm of a household in its house (ADR-0024 section 4, ADR-0026 sections 3 to 6):
//! a plan at 5:00 per member, school and work by age and by the work plan's draw, a wake 45
//! minutes before leaving that ends sleep, the departure that ends what one does or waits for
//! an action for a critical need, away with every need still, and coming home hungry with the
//! jumps of the data and a choice. The repository's day (`data/house/day.json`): school 7:40 to
//! 15:20, full time 8:40 to 17:20, part time 7:40 to 13:20, the wake 2 700 s before, jumps
//! hunger −350, energy −200, bladder −400, hygiene −100, fun −150. The values by hand; the
//! points are those without the mood (ADR-0026, addendum of 2026-11-04 on the mood), which in
//! these runs turns no choice but the one the far house's test names.

mod common;

use std::ops::Range;

use common::day::{self, MORNING};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::house::choice::{ENOUGH, mood};
use sim_core::house::data::Need;
use sim_core::house::day::{DayDuty, duty};
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Due, Duty, Place};
use sim_core::ids::{EventId, SimId};
use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json`.
const MOOD: u8 = 10;

const D: i64 = DAY.as_secs();
const Y: i64 = YEAR.as_secs();

/// The seconds of a day: 5:00, the wakes 6:55 and 7:55, the departures 7:40 and 8:40, the
/// returns 13:20, 15:20 and 17:20.
const FIVE: i64 = 18_000;
const WAKE_EARLY: i64 = 24_900;
const WAKE_LATE: i64 = 28_500;
const EARLY: i64 = 27_600;
const LATE: i64 = 31_200;
const PART_BACK: i64 = 48_000;
const SCHOOL_BACK: i64 = 55_200;
const FULL_BACK: i64 = 62_400;

/// The same seconds of day 1, after its first morning at 6:00.
const fn after_morning(second: i64) -> i64 {
    D + second - MORNING
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

/// The draw of a founder's work plan (ADR-0024 section 4, `WORK_PLAN`), only read: the founder
/// of `tag` at `index`, the world seed 7.
fn draw(tag: &str, index: u64) -> u64 {
    let key = LineageKey::founder(&DataId::new(tag).expect("id"), index);
    let address = DrawKey {
        tag: tags::WORK_PLAN,
        subject: SubjectKey::from(key),
        occasion: 0,
        detail: 0,
    };
    Draws::new(WorldSeed::new(7)).uniform_below(address, 100)
}

/// The first tag `test:rhythm_<n>` whose founders' draws fall into `plans`, by founder.
fn tag_with(plans: &[Range<u64>]) -> String {
    (0..1_000)
        .map(|n| format!("test:rhythm_{n}"))
        .find(|tag| {
            (0_u64..)
                .zip(plans)
                .all(|(index, plan)| plan.contains(&draw(tag, index)))
        })
        .expect("a tag")
}

const FULL_TIME: Range<u64> = 0..70;
const PART_TIME: Range<u64> = 70..90;
const AT_HOME: Range<u64> = 90..100;
const WORKS: Range<u64> = 0..90;
const ANY: Range<u64> = 0..100;

/// The rates of the test houses, the same in every age band: awake only `hunger` and energy
/// (−30 an hour) fall, asleep energy rises by `asleep_energy`.
fn rates(hunger: i32, energy: i32, asleep_energy: i32) -> String {
    let bands = |rate: i32| [rate; 4];
    serde_json::json!({
        "awake": {"hunger": bands(hunger), "energy": bands(energy), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(asleep_energy), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)}
    })
    .to_string()
}

/// A loo for three (bladder +12 000 an hour until full, at most 5 min), a pot for two adults
/// (hunger +100 an hour for 3 h) and a bed for three: a child's sleep with fun +3 000, an older
/// person's doze, both for the night.
const HOME_KINDS: &str = r#"
  {"id": "test:loo", "slots": 3, "offers": [
    {"id": "use", "rates": {"bladder": 12000}, "until_full": "bladder", "max_seconds": 300, "min_age": 0, "max_age": 130}]},
  {"id": "test:pot", "slots": 2, "offers": [
    {"id": "cook", "rates": {"hunger": 100}, "seconds": 10800, "min_age": 18, "max_age": 130}]},
  {"id": "test:bed", "slots": 3, "offers": [
    {"id": "sleep", "sleep": true, "rates": {"fun": 3000}, "min_age": 6, "max_age": 12},
    {"id": "doze", "sleep": true, "min_age": 13, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance) and bedroom 1, 3 s apart. Objects: loo 0 and pot 1 in the hall,
/// bed 2 in the bedroom, the five essential ones 3 to 7 in the hall.
const HOME: &str = r#"{
  "id": "test:home", "capacity": 3,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "bedroom"}],
  "child_day_room": "hall", "child_night_room": "bedroom", "dwell_room": "bedroom",
  "doors": [{"a": "hall", "b": "bedroom", "seconds": 3}],
  "objects": [{"kind": "test:loo", "room": "hall"}, {"kind": "test:pot", "room": "hall"},
    {"kind": "test:bed", "room": "bedroom"}, ESSENTIALS]
}"#;

/// Hunger and fun still awake, energy −30 an hour awake and +30 asleep.
fn home() -> Data {
    day::house_data(&rates(0, -30, 30), HOME_KINDS, HOME)
}

/// A feast for one (hunger +6 an hour for 10 h), a gruel for one (+180 for 20 min), a cot for
/// two and a small loo (bladder +720 for 5 min).
const FAR_KINDS: &str = r#"
  {"id": "test:feast", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 6}, "seconds": 36000, "min_age": 0, "max_age": 130}]},
  {"id": "test:gruel", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 180}, "seconds": 1200, "min_age": 0, "max_age": 130}]},
  {"id": "test:cot", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:loo", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 720}, "seconds": 300, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance), a 1, b 2, den 3, each 110 s from the next: the den is 330 s
/// from the hall. Objects: feast 0 and gruel 1 in the den, cot 2 in a, loo 3 in the hall, the
/// five essential ones 4 to 8 in the hall.
const FAR: &str = r#"{
  "id": "test:far", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "a"}, {"id": "b"}, {"id": "den"}],
  "child_day_room": "hall", "child_night_room": "a", "dwell_room": "den",
  "doors": [
    {"a": "hall", "b": "a", "seconds": 110},
    {"a": "a", "b": "b", "seconds": 110},
    {"a": "b", "b": "den", "seconds": 110}],
  "objects": [{"kind": "test:feast", "room": "den"}, {"kind": "test:gruel", "room": "den"},
    {"kind": "test:cot", "room": "a"}, {"kind": "test:loo", "room": "hall"}, ESSENTIALS]
}"#;

/// Hunger falls 173 an hour awake, everything else stands still awake.
fn far() -> Data {
    day::house_data(&rates(-173, 0, 60), FAR_KINDS, FAR)
}

struct Family {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Family {
    /// The founders of `tag` born at `born` (a woman, a man, then children), the first two a
    /// couple who take the children in; their household moves in at 23:59:59 of day 0.
    fn moved_in(data: Data, tag: &str, born: &[i64]) -> Family {
        Family::moved_in_on(data, tag, born, 0)
    }

    /// The same, moved in at 23:59:59 of day `day`.
    fn moved_in_on(data: Data, tag: &str, born: &[i64], day: i64) -> Family {
        let founders = (born.iter().enumerate())
            .map(|(index, &born)| Founder {
                born: t(born),
                sex: if index == 1 { Sex::Male } else { Sex::Female },
                alleles: common::dummy(0, 0),
            })
            .collect();
        let mut world = World::new(data);
        let start = Start {
            world_seed: 7,
            founder_tag: DataId::new(tag).expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders,
            couples: vec![Couple { woman: 0, man: 1 }],
            capacity: None,
            pool: None,
        };
        world.submit(Input::Start(start)).expect("accepted");
        world.advance_until(&mut YearOut::default(), t((day + 1) * D - 2), &mut |_| {
            false
        });
        let home = world.state().household_of(sim(1)).expect("her household");
        for index in 1..born.len() {
            let member = sim(u64::try_from(index).expect("small") + 1);
            assert_eq!(world.state().household_of(member), Some(home), "{index}");
        }
        world
            .submit(Input::MoveIn(MoveIn { household: home }))
            .expect("accepted");
        let log = world.take_log();
        Family {
            world,
            out: YearOut::default(),
            log,
        }
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, t(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    /// The entries of a person's day as (second after the first morning, payload).
    fn day(&self, id: u64) -> Vec<(i64, LogPayload)> {
        (self.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
            .filter(|entry| of_the_day(&entry.payload))
            .map(|entry| (entry.t.as_secs() - MORNING, entry.payload.clone()))
            .collect()
    }

    fn entry(&self, id: &EventId) -> &LogEntry {
        (self.log.iter())
            .find(|entry| day::number(&entry.id) == day::number(id))
            .expect("an entry")
    }

    /// The person's entries of one kind, with their seconds.
    fn of(&self, id: u64, kind: fn(&LogPayload) -> bool) -> Vec<&LogEntry> {
        (self.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(id))] && kind(&entry.payload))
            .collect()
    }
}

fn of_the_day(payload: &LogPayload) -> bool {
    matches!(
        payload,
        LogPayload::DayStarted { .. }
            | LogPayload::DayPlanned { .. }
            | LogPayload::ActionStarted { .. }
            | LogPayload::Reached { .. }
            | LogPayload::ActionEnded { .. }
            | LogPayload::Idle { .. }
            | LogPayload::NeedCritical { .. }
            | LogPayload::LeftHome { .. }
            | LogPayload::CameHome { .. }
            | LogPayload::Missed { .. }
    )
}

fn planned(payload: &LogPayload) -> bool {
    matches!(payload, LogPayload::DayPlanned { .. })
}

const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

fn started(
    house: &str,
    (object, offer, slot): (u16, u16, u8),
    room: u8,
    walk_secs: u32,
) -> impl Fn([u16; 5], Vec<u16>) -> LogPayload {
    let house = DataId::new(house).expect("id");
    move |needs, busy| LogPayload::ActionStarted {
        house: house.clone(),
        object,
        offer,
        slot,
        room,
        walk_secs,
        needs,
        busy,
        commanded: false,
        held: Vec::new(),
        tier: Vec::new(),
    }
}

fn ended(object: u16, reason: EndReason, needs: [u16; 5]) -> LogPayload {
    LogPayload::ActionEnded {
        object,
        offer: 0,
        reason,
        needs,
    }
}

fn idle(house: &str, room: u8, needs: [u16; 5], busy: Vec<u16>) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new(house).expect("id"),
        room,
        needs,
        busy,
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
    }
}

/// The plan of day 1 at `depart` and `back`.
fn plan(place: Duty, depart: i64, back: i64) -> LogPayload {
    LogPayload::DayPlanned {
        place,
        depart: t(D + depart),
        back: t(D + back),
    }
}

fn left(place: Duty, late_secs: u32) -> LogPayload {
    LogPayload::LeftHome { place, late_secs }
}

/// A woman of 30 working full time (sim 1), a man of 32 working part time (sim 2) and a child
/// of 10 (sim 3), whom the couple takes in, in the test home. Day 1 is a weekday (1).
fn school_and_work() -> Family {
    let tag = tag_with(&[FULL_TIME, PART_TIME, ANY]);
    Family::moved_in(home(), &tag, &[-30 * Y, -32 * Y, -10 * Y])
}

/// The first morning, at 6:00 of day 1, plans the day of each (only what comes after 6:00).
/// All three take the loo first (bladder 500 to full: 500 · 500 − 50 · 5 = 249 750; the pot
/// 400 · 300 = 120 000 less its walk; the bed is closed by day, ADR-0026, addendum of
/// 2026-11-04), there at 6:00:05 and full 150 s later (bladder +12 000 an hour), energy 899
/// (900 − 1.29 truncated). Then the adults cook (119 750), she on place 0, he on 1, from
/// 6:02:40; the child, with the pot taken and too young for it and the bed closed, idles in the
/// hall. Its wake at 6:55 finds it idle: it chooses and idles again (energy 900 − 27.5
/// truncated). His wake at 6:55 and hers at 7:55 find them cooking and do nothing. At 7:40 he
/// leaves the pot (hunger 600 + 100 · 5 840 / 3 600 = 762, energy 850) and the child the hall
/// (energy 850), each with `core:left_home` and no choice; she leaves at 8:40 (hunger 862,
/// energy 820). Back at 13:20, 15:20 and 17:20 each comes in at the entrance with the jumps and
/// chooses: he the pot (hunger 412: 588 · 300 = 176 400 beats the loo's 400 · 400 = 160 000),
/// the child and she the loo (her pot 488 · 300).
#[test]
fn a_day_of_school_and_work() {
    let mut family = school_and_work();
    family.advance(MORNING + 40_800);
    let home = "test:home";
    let loo = |slot: u8| started(home, (0, 0, slot), 0, 5);
    let pot = |slot: u8| started(home, (1, 0, slot), 0, 5);
    let after_loo = [600, 899, 1000, 700, 800];
    let common = |slot: u8| {
        vec![
            (
                0,
                LogPayload::DayStarted {
                    needs: MORNING_VALUES,
                },
            ),
            (0, loo(slot)(MORNING_VALUES, Vec::new())),
            (5, LogPayload::Reached { object: 0 }),
            (155, ended(0, EndReason::Full, after_loo)),
        ]
    };
    let mut hers = common(0);
    hers.insert(1, (0, plan(Duty::Work, LATE, FULL_BACK)));
    hers.extend([
        (155, pot(0)(after_loo, Vec::new())),
        (160, LogPayload::Reached { object: 1 }),
        (
            after_morning(LATE),
            ended(1, EndReason::Obligation, [862, 820, 1000, 700, 800]),
        ),
        (after_morning(LATE), left(Duty::Work, 0)),
        (
            after_morning(FULL_BACK),
            LogPayload::CameHome { place: Duty::Work },
        ),
        (
            after_morning(FULL_BACK),
            loo(0)([512, 620, 600, 600, 650], Vec::new()),
        ),
    ]);
    assert_eq!(family.day(1), hers);
    let mut his = common(1);
    his.insert(1, (0, plan(Duty::Work, EARLY, PART_BACK)));
    his.extend([
        (155, pot(1)(after_loo, Vec::new())),
        (160, LogPayload::Reached { object: 1 }),
        (
            after_morning(EARLY),
            ended(1, EndReason::Obligation, [762, 850, 1000, 700, 800]),
        ),
        (after_morning(EARLY), left(Duty::Work, 0)),
        (
            after_morning(PART_BACK),
            LogPayload::CameHome { place: Duty::Work },
        ),
        (
            after_morning(PART_BACK),
            pot(0)([412, 650, 600, 600, 650], Vec::new()),
        ),
    ]);
    assert_eq!(family.day(2)[..his.len()], his);
    let at_wake = [600, 873, 1000, 700, 800];
    let mut child = common(2);
    child.insert(1, (0, plan(Duty::School, EARLY, SCHOOL_BACK)));
    child.extend([
        (155, idle(home, 0, after_loo, vec![1])),
        (after_morning(WAKE_EARLY), idle(home, 0, at_wake, vec![1])),
        (after_morning(EARLY), left(Duty::School, 0)),
        (
            after_morning(SCHOOL_BACK),
            LogPayload::CameHome {
                place: Duty::School,
            },
        ),
        (
            after_morning(SCHOOL_BACK),
            loo(0)([250, 650, 600, 600, 650], Vec::new()),
        ),
    ]);
    assert_eq!(family.day(3)[..child.len()], child);
    // The causes: the plan by the morning's `core:day_started`, the choice at the wake by the
    // wake that plan made, the departure after an end by that end; the return by the plan.
    let causes = |entry: &LogEntry| entry.causes.as_slice().to_vec();
    let by = |entry: &LogEntry| match causes(entry).as_slice() {
        [Cause::Plan(plan)] => family.entry(&plan.by).payload.clone(),
        [Cause::Event(id)] => family.entry(id).payload.clone(),
        other => panic!("{other:?}"),
    };
    let first = |id: u64, kind: fn(&LogPayload) -> bool| family.of(id, kind)[0];
    assert!(matches!(
        by(first(3, planned)),
        LogPayload::DayStarted { .. }
    ));
    let idled = |p: &LogPayload| matches!(p, LogPayload::Idle { .. });
    let woke = family.of(3, idled)[1];
    assert_eq!(woke.t, t(D + WAKE_EARLY));
    assert!(matches!(by(woke), LogPayload::DayPlanned { .. }));
    let away = |p: &LogPayload| matches!(p, LogPayload::LeftHome { .. });
    assert!(matches!(by(first(1, away)), LogPayload::ActionEnded { .. }));
    assert!(matches!(by(first(3, away)), LogPayload::DayPlanned { .. }));
    let back = |p: &LogPayload| matches!(p, LogPayload::CameHome { .. });
    assert!(matches!(by(first(2, back)), LogPayload::DayPlanned { .. }));
}

/// Away every need stands still from the departure on (ADR-0024 section 4): at 12:00 she, he
/// and the child are away with the values of their departures at rate 0 and nothing to do.
/// On the way out (the hall to the door 5 s) the needs already stand.
#[test]
fn away_the_needs_stand_still() {
    let mut family = school_and_work();
    family.advance(D + EARLY + 3);
    let house = family.world.state().house();
    let still =
        |values: [u16; 5], at: i64| values.map(|value| NeedSample::new(value, t(D + at), 0));
    let child = &house.sims[&sim(3)];
    assert_eq!(
        child.place,
        Place::Walking {
            from: 0,
            to: 0,
            depart: t(D + EARLY),
            arrive: t(D + EARLY + 5),
        }
    );
    assert_eq!(child.needs, still([600, 850, 1000, 700, 800], EARLY));
    family.advance(D + 12 * 3_600);
    let house = family.world.state().house();
    for (id, values, at, decisions) in [
        (1, [862, 820, 1000, 700, 800], LATE, 2),
        (2, [762, 850, 1000, 700, 800], EARLY, 2),
        (3, [600, 850, 1000, 700, 800], EARLY, 3),
    ] {
        let person = &house.sims[&sim(id)];
        assert_eq!(person.place, Place::Away, "{id}");
        assert_eq!(person.needs, still(values, at), "{id}");
        assert_eq!((person.action, person.due), (None, None), "{id}");
        assert_eq!(person.decisions.count, decisions, "{id}");
    }
    assert_eq!(
        family.world.away(sim(3)),
        Some((Duty::School, t(D + SCHOOL_BACK)))
    );
    assert_eq!(
        family.world.away(sim(1)),
        Some((Duty::Work, t(D + FULL_BACK)))
    );
}

/// Days 5 and 6 are the weekend (weekdays 5 and 6): moved in on day 4, the couple's first
/// morning plans nothing, nor do the plans of days 5 (before the morning) and 6; on day 7
/// (weekday 0) the plan at 5:00, the series that came with their `core:moved_in` (ADR-0024,
/// addendum of 2026-11-04), plans her full and his part time.
#[test]
fn a_weekend_has_no_duties() {
    let tag = tag_with(&[FULL_TIME, PART_TIME]);
    let mut family = Family::moved_in_on(home(), &tag, &[-30 * Y, -32 * Y], 4);
    family.advance(8 * D);
    let seconds = |id: u64| -> Vec<i64> {
        (family.of(id, planned).iter())
            .map(|entry| entry.t.as_secs())
            .collect()
    };
    assert_eq!(seconds(1), [7 * D + FIVE]);
    assert_eq!(seconds(2), [7 * D + FIVE]);
    let hers = family.of(1, planned)[0];
    assert_eq!(
        hers.payload,
        LogPayload::DayPlanned {
            place: Duty::Work,
            depart: t(7 * D + LATE),
            back: t(7 * D + FULL_BACK),
        }
    );
    // Planned at moving in by its `core:moved_in`.
    let [Cause::Plan(series)] = hers.causes.as_slice() else {
        panic!("{:?}", hers.causes);
    };
    assert_eq!(series.planned_at, t(5 * D - 1));
    assert!(matches!(
        family.entry(&series.by).payload,
        LogPayload::MovedIn { .. }
    ));
    let his = family.of(2, |p| matches!(p, LogPayload::LeftHome { .. }));
    assert_eq!(his.len(), 1);
    assert_eq!(his[0].t, t(7 * D + EARLY));
}

/// The work plan by its draw (ADR-0024 section 4): below 70 full time, below 90 part time,
/// else at home, with the repository's day; the draw is asked only for the working age.
#[test]
fn the_work_plan_by_its_draw() {
    let day = common::repository_houses().day().clone();
    let at = |second: i64| t(D + second);
    let work = |wake: i64, depart: i64, back: i64| DayDuty {
        place: Duty::Work,
        wake: at(wake),
        depart: at(depart),
        back: at(back),
    };
    let adult = t(-30 * Y);
    assert_eq!(
        duty(&day, 1, adult, || 0),
        Some(work(WAKE_LATE, LATE, FULL_BACK))
    );
    assert_eq!(
        duty(&day, 1, adult, || 69),
        Some(work(WAKE_LATE, LATE, FULL_BACK))
    );
    assert_eq!(
        duty(&day, 1, adult, || 70),
        Some(work(WAKE_EARLY, EARLY, PART_BACK))
    );
    assert_eq!(
        duty(&day, 1, adult, || 89),
        Some(work(WAKE_EARLY, EARLY, PART_BACK))
    );
    assert_eq!(duty(&day, 1, adult, || 90), None);
    assert_eq!(duty(&day, 1, adult, || 99), None);
    // In a household: she at home plans nothing, he works full time.
    let tag = tag_with(&[AT_HOME, FULL_TIME]);
    let mut family = Family::moved_in(home(), &tag, &[-30 * Y, -32 * Y]);
    family.advance(2 * D);
    assert!(family.of(1, planned).is_empty());
    assert_eq!(
        (family.of(2, planned).iter())
            .map(|entry| entry.payload.clone())
            .collect::<Vec<_>>(),
        [plan(Duty::Work, LATE, FULL_BACK)]
    );
}

/// The ages count at 5:00 of the day (ADR-0024 section 4): school from 6 to the day before the
/// 18th birthday, work from 18 to the day before the 67th, nothing under 6 and on the weekend.
#[test]
fn the_duty_by_age() {
    let day = common::repository_houses().day().clone();
    let at = |day: i64, second: i64| t(day * D + second);
    let school = |day: i64| DayDuty {
        place: Duty::School,
        wake: at(day, WAKE_EARLY),
        depart: at(day, EARLY),
        back: at(day, SCHOOL_BACK),
    };
    let unasked = || -> u64 { panic!("drawn outside the working age") };
    // Turning 6, 18 and 67 a second after 5:00 of day 1: the old age on day 1, the new on 2.
    let turning = |years: i64| t(D + FIVE + 1 - years * Y);
    assert_eq!(duty(&day, 1, turning(6), unasked), None);
    assert_eq!(duty(&day, 2, turning(6), unasked), Some(school(2)));
    assert_eq!(duty(&day, 1, turning(18), unasked), Some(school(1)));
    let part = DayDuty {
        place: Duty::Work,
        wake: at(2, WAKE_EARLY),
        depart: at(2, EARLY),
        back: at(2, PART_BACK),
    };
    assert_eq!(duty(&day, 2, turning(18), || 75), Some(part));
    assert_eq!(
        duty(&day, 1, turning(67), || 75).map(|d| d.place),
        Some(Duty::Work)
    );
    assert_eq!(duty(&day, 2, turning(67), unasked), None);
    // Turning 6 at 5:00 itself counts that day; the weekend has nothing.
    assert_eq!(duty(&day, 1, t(D + FIVE - 6 * Y), unasked), Some(school(1)));
    assert_eq!(duty(&day, 5, t(-10 * Y), unasked), None);
    assert_eq!(duty(&day, 6, t(-30 * Y), unasked), None);
    assert_eq!(duty(&day, 7, t(-10 * Y), unasked), Some(school(7)));
}

/// Turning 6 (ADR-0024 section 2): a child of 5 moved in with the couple has its plan at 5:00
/// from its `core:moved_in` and nothing else; it is 6 from 12:00 of day 2, and the plan at 5:00
/// of day 3 gives it its needs at the morning values, its plan of the day (school, day 3 is a
/// weekday) and a choice. The series runs on: one plan on day 4, no second. A man of 66 who is
/// 67 from 3:00 of day 2 works on day 1 and no more on day 2.
#[test]
fn six_and_sixty_seven() {
    let six = 2 * D + 12 * 3_600 - 6 * Y;
    let mut family = Family::moved_in(home(), "test:house", &[-30 * Y, -32 * Y, six]);
    family.advance(2 * D + 13 * 3_600);
    let house = family.world.state().house();
    assert!(!house.sims.contains_key(&sim(3)));
    assert!(family.day(3).is_empty());
    family.advance(4 * D + 6 * 3_600);
    let day = family.day(3);
    assert_eq!(
        day[..2],
        [
            (
                3 * D + FIVE - MORNING,
                LogPayload::DayStarted {
                    needs: MORNING_VALUES
                }
            ),
            (
                3 * D + FIVE - MORNING,
                LogPayload::DayPlanned {
                    place: Duty::School,
                    depart: t(3 * D + EARLY),
                    back: t(3 * D + SCHOOL_BACK),
                }
            ),
        ]
    );
    assert!(matches!(day[2].1, LogPayload::ActionStarted { .. }));
    let started = family.of(3, |p| matches!(p, LogPayload::DayStarted { .. }))[0];
    let [Cause::Plan(series)] = started.causes.as_slice() else {
        panic!("{:?}", started.causes);
    };
    assert_eq!(series.planned_at, t(D - 1));
    assert!(matches!(
        family.entry(&series.by).payload,
        LogPayload::MovedIn { .. }
    ));
    let plans: Vec<i64> = (family.of(3, planned).iter())
        .map(|e| e.t.as_secs())
        .collect();
    assert_eq!(plans, [3 * D + FIVE, 4 * D + FIVE]);

    let tag = tag_with(&[ANY, WORKS]);
    let mut family = Family::moved_in(home(), &tag, &[-30 * Y, 2 * D + 3 * 3_600 - 67 * Y]);
    family.advance(3 * D);
    let his: Vec<i64> = (family.of(2, planned).iter())
        .map(|e| e.t.as_secs())
        .collect();
    assert_eq!(his, [D + 6 * 3_600]);
}

/// The moods (ADR-0026, addendum of 2026-11-04 on the mood) of founder `index` of `test:house`,
/// the world seed 7, for the feast and the gruel of the far house at `second`, only read.
fn far_moods(index: u64, second: i64) -> [i64; 2] {
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), index);
    [0, 1].map(|object| i64::from(mood(7, key, t(second), (object, 0), MOOD)))
}

/// Two who work full time in the far house (sims 1 and 2 of `common::day`, their draws 30
/// and 3). Hunger falls 173 an hour: at 6:00 nothing is enough (the loo 500 · 60 − 250, at
/// most 33 000 − 250 with the best mood), at 7:09:22 (400) and at their wakes at 7:55 (269, the
/// feast 731 · 60 − 50 · 330, at most 48 246 − 16 500) neither. Nor does anything before hunger
/// is critical, at any mood, so no choice is planned before (ADR-0026, addendum of 2026-11-04
/// after the playtest): feast and gruel fill 60, so 849 · 60 · 110 / 100 − 16 500 = 39 534. At
/// 8:36:05 hunger is 150, and each fills 850 · 60 = 51 000, scaled by the person's mood for it
/// there (the draw `mood`): her moods, −10 and +9, give the feast 10⁹ + 45 900 − 16 500 and the
/// gruel 10⁹ + 55 590 − 16 500, so she takes the gruel; he finds it taken and takes the feast,
/// both 330 s away. At 8:40 they walk to it with hunger 139, critical: the departure waits
/// (`due`). There at 8:41:35 with hunger 135. Her gruel ends at 9:01:35 (135 + 60): she leaves
/// at once, 1 295 s late, and walks out 335 s; at 17:20 she comes home with hunger 0 and bladder
/// 100 and takes the gruel (the deeper need: 151 · 10⁹). His feast lasts until 18:41:35: at
/// 17:20 he has missed work, and eats on (135 + 60).
#[test]
fn a_late_departure_and_a_missed_day() {
    assert_eq!((draw("test:house", 0), draw("test:house", 1)), (30, 3));
    let [feast, gruel] = far_moods(0, MORNING + 9_365);
    assert_eq!((feast, gruel), (-10, 9));
    assert!(51_000 * (100 + gruel) / 100 > 51_000 * (100 + feast) / 100);
    let most = |fills: i64, deficit: i64| fills * deficit * 110 / 100 - 16_500;
    assert!(most(60, 849) < ENOUGH);
    let (mut run, _) = day::Run::moved_in(far());
    run.advance(MORNING + 9_630);
    let due = Due {
        place: Duty::Work,
        depart: t(D + LATE),
        back: t(D + FULL_BACK),
    };
    for id in [1, 2] {
        assert_eq!(run.world.state().house().sims[&sim(id)].due, Some(due));
    }
    run.advance(MORNING + 46_000);
    let far = "test:far";
    let at_six = idle(far, 0, MORNING_VALUES, Vec::new());
    let early = [
        (
            0,
            LogPayload::DayStarted {
                needs: MORNING_VALUES,
            },
        ),
        (0, plan(Duty::Work, LATE, FULL_BACK)),
        (0, at_six),
        (4_162, idle(far, 0, [400, 900, 500, 700, 800], Vec::new())),
        (6_900, idle(far, 0, [269, 900, 500, 700, 800], Vec::new())),
        (
            9_365,
            LogPayload::NeedCritical {
                need: Need::Hunger,
                level: 150,
            },
        ),
    ];
    let critical = [150, 900, 500, 700, 800];
    let mut hers = early.to_vec();
    hers.extend([
        (9_365, started(far, (1, 0, 0), 0, 330)(critical, Vec::new())),
        (9_695, LogPayload::Reached { object: 1 }),
        (10_895, ended(1, EndReason::Done, [195, 900, 500, 700, 800])),
        (10_895, left(Duty::Work, 1_295)),
        (40_800, LogPayload::CameHome { place: Duty::Work }),
        (
            40_800,
            started(far, (1, 0, 0), 0, 330)([0, 700, 100, 600, 650], vec![0]),
        ),
    ]);
    let doings = |id: u64| {
        let mut day = Vec::new();
        for entry in &run.log {
            if entry.subjects == [Subject::Sim(sim(id))] && of_the_day(&entry.payload) {
                day.push((entry.t.as_secs() - MORNING, entry.payload.clone()));
            }
        }
        day
    };
    assert_eq!(doings(1)[..hers.len()], hers);
    let mut his = early.to_vec();
    his.extend([
        (9_365, started(far, (0, 0, 0), 0, 330)(critical, vec![1])),
        (9_695, LogPayload::Reached { object: 0 }),
        (40_800, LogPayload::Missed { place: Duty::Work }),
        (45_695, ended(0, EndReason::Done, [195, 900, 500, 700, 800])),
    ]);
    assert_eq!(doings(2)[..his.len()], his);
}

/// Saved in every new kind of second, the save loads, saves to the same bytes and runs on as
/// the uninterrupted world, with the same entries: the wake, the departures and the walks
/// out, away, the plan of day 2 at 5:00, the returns; in the far house the waiting
/// departures, the late one and its walk out, the missed day.
#[test]
fn the_rhythm_is_saved_and_loaded() {
    let home_seconds: &[i64] = &[
        WAKE_EARLY,
        EARLY,
        EARLY + 3,
        EARLY + 10,
        12 * 3_600,
        PART_BACK,
        PART_BACK + 1,
        SCHOOL_BACK,
        D + FIVE - 1,
        D + FIVE,
    ];
    let far_seconds: &[i64] = &[
        LATE,
        LATE + 30,
        LATE + 100,
        LATE + 1_295,
        LATE + 1_400,
        LATE + 1_700,
        FULL_BACK,
        FULL_BACK + 1,
    ];
    let until = D + D + 10 * 3_600;
    let check = |data: Data, saved_at: i64, make: &dyn Fn() -> (World, YearOut, Vec<LogEntry>)| {
        let (mut world, mut out, mut log) = make();
        world.advance_until(&mut out, t(D + saved_at), &mut |_| false);
        log.extend(world.take_log());
        let bytes = world.save(&out.year_hashes).expect("saves");
        let (mut loaded, _) =
            World::load(&bytes, data).unwrap_or_else(|e| panic!("{saved_at}: {e:?}"));
        assert_eq!(loaded.save(&out.year_hashes).expect("saves"), bytes);
        let mut other = YearOut::default();
        loaded.advance_until(&mut other, t(until), &mut |_| false);
        world.advance_until(&mut out, t(until), &mut |_| false);
        assert_eq!(loaded.state_hash(), world.state_hash(), "{saved_at}");
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        assert_eq!(
            entries(&loaded.take_log()),
            entries(&world.take_log()),
            "{saved_at}"
        );
    };
    for &second in home_seconds {
        check(home(), second, &|| {
            let family = school_and_work();
            (family.world, family.out, family.log)
        });
    }
    for &second in far_seconds {
        check(far(), second, &|| {
            let (run, _) = day::Run::moved_in(far());
            (run.world, run.out, run.log)
        });
    }
}

/// The new entries in JSON: the place as a code, the seconds as numbers.
#[test]
fn the_entries_in_json() {
    let json = |payload: &LogPayload| serde_json::to_value(payload).expect("json");
    assert_eq!(
        json(&plan(Duty::School, EARLY, SCHOOL_BACK)),
        serde_json::json!({"core:day_planned": {"place": "school", "depart": D + EARLY, "back": D + SCHOOL_BACK}})
    );
    assert_eq!(
        json(&left(Duty::Work, 1_295)),
        serde_json::json!({"core:left_home": {"place": "work", "late_secs": 1_295}})
    );
    assert_eq!(
        json(&LogPayload::CameHome {
            place: Duty::School
        }),
        serde_json::json!({"core:came_home": {"place": "school"}})
    );
    assert_eq!(
        json(&LogPayload::Missed { place: Duty::Work }),
        serde_json::json!({"core:missed": {"place": "work"}})
    );
    assert_eq!(
        json(&ended(0, EndReason::Obligation, MORNING_VALUES))["core:action_ended"]["reason"],
        serde_json::json!("obligation")
    );
}
