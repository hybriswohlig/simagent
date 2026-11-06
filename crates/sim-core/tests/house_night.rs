//! The night (ADR-0026, addendum of 2026-11-04): an offer of sleep is open only in the person's
//! night, from `night.from` of its day (by the age band at the day's plan at 5:00) to
//! `night.until` of the next, and a sleep lasts to the night's end. At its bedtime a person idle
//! in a room chooses; in the night a choice takes what raises a critical need first, then a
//! free bed, then a free place on the sofa, then waits for a bed. The night function at its
//! edges; then a couple (and once a child) in a test house on a weekend, the values by hand,
//! the night ending at 6:00 on the weekend too (`common::weekday_mornings`; the weekend's later
//! end is in `house_rules.rs`).
//!
//! The test house: rooms hall 0 (the entrance), living 1, bedroom 2, bath 3; the bedroom 4 s,
//! the living room 3 s and the bath 2 s from the hall (bath to bedroom 6 s, living room to
//! bedroom 7 s, 5 s to an object in one's own room). Objects: a bed for two 0 and a cot for one
//! from 0 to 12 years 1 in the bedroom, a television for two 2 (fun +180 an hour for an hour)
//! and the sofa 3 (sleep with energy +50) in the living room, the four other essential kinds 4
//! to 7 in the hall (each +1 an hour for a minute: a gain of 0), and in some data a loo for two
//! 8 in the bath (bladder +12 000 an hour until full, at most 5 min). Awake only energy falls,
//! asleep energy rises 100 an hour and the bladder falls as the data say; the rest stands. So
//! the television never brings enough by itself: fun stands at 800 unless one watches, and it
//! fills at most 200 · 180 = 36 000, at the best mood (ADR-0026, addendum of 2026-11-04 on the
//! mood) 39 600, less the walk. The other points here are without the mood; the moods of these
//! runs turn none of the choices, and where two offers would tie without them the test names
//! the moods that order them.

mod common;

use common::day::Run;
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::mood;
use sim_core::house::data::{DayRules, Need, OfferDef};
use sim_core::house::day::{Night, next_night, night, night_of};
use sim_core::house::need::NeedSample;
use sim_core::house::reserve::Reserve;
use sim_core::house::state::{Decisions, InHouse, Place, action_end};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input, InputRejected, RejectCode};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, MAX_ABS_SECS, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json`.
const MOOD: u8 = 10;

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
const Y: i64 = YEAR.as_secs();

/// The repository's day: the night from 20:00 for 6 to 17, from 22:00 for 18 to 66, from 21:30
/// from 67 on, each until 6:00 on a weekday.
const DAY_FILE: &str = include_str!("../../../data/house/day.json");

/// The repository's day with weekday mornings (`common::weekday_mornings`): the runs here begin
/// on a Saturday to have no duty, and their nights end at 6:00 as on a weekday.
fn day_file() -> String {
    common::weekday_mornings(DAY_FILE)
}

/// The first morning of a household moved in at 23:59:59 of day 4: 6:00 of day 5, a Saturday
/// (weekday 5), so no duty that weekend.
const SATURDAY: i64 = 5 * D + 6 * H;
/// Offsets from it: 20:00 and 22:00 of Saturday, 6:00 of Sunday.
const EIGHT_PM: i64 = 14 * H;
const TEN_PM: i64 = 16 * H;
const SUNDAY: i64 = D;

const BED: u16 = 0;
const COT: u16 = 1;
const TV: u16 = 2;
const SOFA: u16 = 3;
const LOO: u16 = 8;

const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn rules() -> DayRules {
    serde_json::from_str(DAY_FILE).expect("the day")
}

fn kinds(loo: bool) -> String {
    let mut kinds = json!([
        {"id": "test:bed", "slots": 2, "offers": [
            {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
        {"id": "test:cot", "slots": 1, "offers": [
            {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 12}]},
        {"id": "test:tv", "slots": 2, "offers": [
            {"id": "watch", "rates": {"fun": 180}, "seconds": 3600, "min_age": 0, "max_age": 130}]},
        {"id": "core:sofa", "slots": 3, "offers": [
            {"id": "sleep", "rates": {"energy": 50}, "sleep": true, "min_age": 0, "max_age": 130}]},
        {"id": "core:dining_table", "slots": 1, "offers": [
            {"id": "eat", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
        {"id": "core:fridge", "slots": 1, "offers": [
            {"id": "snack", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
        {"id": "core:toilet", "slots": 1, "offers": [
            {"id": "use", "rates": {"bladder": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
        {"id": "core:shower", "slots": 1, "offers": [
            {"id": "shower", "rates": {"hygiene": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]}
    ]);
    if loo {
        kinds.as_array_mut().expect("a list").push(json!(
            {"id": "test:loo", "slots": 2, "offers": [{"id": "use", "rates": {"bladder": 12000},
                "until_full": "bladder", "max_seconds": 300, "min_age": 0, "max_age": 130}]}
        ));
    }
    kinds.to_string()
}

fn house(loo: bool) -> String {
    let mut objects = json!([
        {"kind": "test:bed", "room": "bedroom"}, {"kind": "test:cot", "room": "bedroom"},
        {"kind": "test:tv", "room": "living"}, {"kind": "core:sofa", "room": "living"},
        {"kind": "core:dining_table", "room": "hall"}, {"kind": "core:fridge", "room": "hall"},
        {"kind": "core:toilet", "room": "hall"}, {"kind": "core:shower", "room": "hall"}
    ]);
    if loo {
        let list = objects.as_array_mut().expect("a list");
        list.push(json!({"kind": "test:loo", "room": "bath"}));
    }
    json!({
        "id": "test:night", "capacity": 3,
        "rooms": [{"id": "hall", "entrance": true}, {"id": "living"}, {"id": "bedroom"},
            {"id": "bath"}],
        "child_day_room": "living", "child_night_room": "bedroom", "dwell_room": "living",
        "doors": [
            {"a": "hall", "b": "living", "seconds": 3},
            {"a": "hall", "b": "bedroom", "seconds": 4},
            {"a": "hall", "b": "bath", "seconds": 2}],
        "objects": objects
    })
    .to_string()
}

/// Awake energy falls `energy` an hour, asleep it rises 100 and the bladder falls `bladder`; the
/// rest stands, in every age band.
fn rates(energy: i32, bladder: i32) -> String {
    let bands = |rate: i32| [rate; 4];
    json!({
        "awake": {"hunger": bands(0), "energy": bands(energy), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(bladder),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [1000, 1000, 1000, 1000, 1000]
    })
    .to_string()
}

/// The test house on `day` with the rates of `rates`, with or without the loo.
fn data_with(day: &str, (energy, bladder): (i32, i32), loo: bool) -> Data {
    let houses = common::houses_from(&rates(energy, bladder), day, &kinds(loo), &[&house(loo)]);
    common::death_data(1e-9, None, json!([])).with_houses(houses)
}

/// Energy falls 30 an hour awake; no loo, the bladder still asleep.
fn quiet() -> Data {
    data_with(&day_file(), (-30, 0), false)
}

/// A woman of 30 (sim 1) and a man of 32 (sim 2), and the children born `children` years ago
/// (sims 3 on), moved in at 23:59:59 of day `day`.
fn family(data: Data, children: &[i64], day: i64) -> Run {
    let mut born = vec![-30 * Y, -32 * Y];
    born.extend(children.iter().map(|years| -years * Y));
    Run::family(data, 7, "test:night", &born, day).0
}

/// What a person does from `origin` on, as (seconds after it, payload): its day's start and
/// plan, its choices, arrivals, ends, idling and critical needs.
fn doings(run: &Run, id: u64, origin: i64) -> Vec<(i64, LogPayload)> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
        .filter(|entry| entry.t.as_secs() >= origin)
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::DayStarted { .. }
                    | LogPayload::DayPlanned { .. }
                    | LogPayload::ActionStarted { .. }
                    | LogPayload::Reached { .. }
                    | LogPayload::ActionEnded { .. }
                    | LogPayload::Idle { .. }
                    | LogPayload::NeedCritical { .. }
            )
        })
        .map(|entry| (entry.t.as_secs() - origin, entry.payload.clone()))
        .collect()
}

/// A start of offer 0 of `object` on `slot`, walked from `room` in `walk` seconds.
fn started(
    (object, slot): (u16, u8),
    (room, walk): (u8, u32),
    needs: [u16; 5],
    busy: &[u16],
    commanded: bool,
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new("test:night").expect("id"),
        object,
        offer: 0,
        slot,
        room,
        walk_secs: walk,
        needs,
        busy: busy.to_vec(),
        commanded,
        held: Vec::new(),
        tier: Vec::new(),
        filler: false,
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

fn idle(room: u8, needs: [u16; 5], busy: &[u16]) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new("test:night").expect("id"),
        room,
        needs,
        busy: busy.to_vec(),
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
        too_short: None,
    }
}

fn reached(object: u16) -> LogPayload {
    LogPayload::Reached { object }
}

fn command(id: u64, object: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer: 0,
    })
}

/// A rejected input's code; nothing changed (ADR-0004 section 4).
fn rejected(world: &mut World, input: Input) -> Option<RejectCode> {
    let before = world.state_hash();
    let code = (world.submit(input).err()).map(|InputRejected { code }| code);
    assert_eq!(world.state_hash(), before);
    assert!(world.take_log().is_empty());
    code
}

/// His doings are hers in the same seconds, each start on the place after hers.
fn assert_mirrored(run: &Run, origin: i64) {
    let hers = doings(run, 1, origin);
    let mut his = doings(run, 2, origin);
    for (_, payload) in &mut his {
        if let LogPayload::ActionStarted { slot, .. } = payload {
            assert_eq!(*slot, 1);
            *slot = 0;
        }
    }
    assert_eq!(his, hers);
}

/// The night by the clock, at its edges (ADR-0026, addendum of 2026-11-04): for an adult of 30
/// from 22:00 to 6:00, for a child of 8 from 20:00; the band is the age's at 5:00 of the night's
/// day, so turning 18 at 4:00 makes that day's night an adult's and turning 18 at 6:00 does
/// not, and turning 67 at 5:00 itself makes it one of 67; under 6 at 5:00 there is no night.
/// Before the first second of time too.
#[test]
fn the_night_by_the_clock() {
    let rules = rules();
    let at = |day: i64, hours: i64, minutes: i64, seconds: i64| {
        t(day * D + hours * H + minutes * 60 + seconds)
    };
    let span = |from: SimTime, until: SimTime| {
        Some(Night {
            from,
            until: Some(until),
        })
    };
    let adult = t(-30 * Y);
    let child = t(-8 * Y);
    let adults_first = span(at(1, 22, 0, 0), at(2, 6, 0, 0));
    assert_eq!(night(&rules, adult, at(1, 21, 59, 59)), None);
    assert_eq!(night(&rules, adult, at(1, 22, 0, 0)), adults_first);
    assert_eq!(night(&rules, adult, at(2, 0, 0, 0)), adults_first);
    assert_eq!(night(&rules, adult, at(2, 5, 59, 59)), adults_first);
    assert_eq!(night(&rules, adult, at(2, 6, 0, 0)), None);
    let childs_first = span(at(1, 20, 0, 0), at(2, 6, 0, 0));
    assert_eq!(night(&rules, child, at(1, 19, 59, 59)), None);
    assert_eq!(night(&rules, child, at(1, 20, 0, 0)), childs_first);
    assert_eq!(night(&rules, child, at(1, 21, 59, 59)), childs_first);
    assert_eq!(night(&rules, child, at(2, 5, 59, 59)), childs_first);
    assert_eq!(night(&rules, child, at(2, 6, 0, 0)), None);
    // 18 at 4:00 of day 1: 17 at 5:00 of day 0, 18 at 5:00 of day 1.
    let early = t(D + 4 * H - 18 * Y);
    assert_eq!(
        night(&rules, early, at(1, 4, 0, 0)),
        span(at(0, 20, 0, 0), at(1, 6, 0, 0))
    );
    assert_eq!(night(&rules, early, at(1, 20, 0, 0)), None);
    assert_eq!(
        night(&rules, early, at(1, 22, 0, 0)),
        span(at(1, 22, 0, 0), at(2, 6, 0, 0))
    );
    // 18 at 6:00 of day 1: 17 at its 5:00, so its night still a child's; 18 on day 2.
    let late = t(D + 6 * H - 18 * Y);
    assert_eq!(
        night(&rules, late, at(1, 20, 0, 0)),
        span(at(1, 20, 0, 0), at(2, 6, 0, 0))
    );
    assert_eq!(night(&rules, late, at(2, 20, 0, 0)), None);
    assert_eq!(
        night_of(&rules, 2, late),
        span(at(2, 22, 0, 0), at(3, 6, 0, 0))
    );
    // 67 at 5:00 of day 1 itself: from 21:30.
    let old = t(D + 5 * H - 67 * Y);
    assert_eq!(
        night_of(&rules, 1, old),
        span(at(1, 21, 30, 0), at(2, 6, 0, 0))
    );
    assert_eq!(
        night_of(&rules, 0, old),
        span(at(0, 22, 0, 0), at(1, 6, 0, 0))
    );
    // 6 at noon of day 1: no night on day 1, one on day 2; none ever at 5.
    let six = t(D + 12 * H - 6 * Y);
    assert_eq!(night_of(&rules, 1, six), None);
    assert_eq!(night(&rules, six, at(1, 21, 0, 0)), None);
    assert_eq!(
        night_of(&rules, 2, six),
        span(at(2, 20, 0, 0), at(3, 6, 0, 0))
    );
    assert_eq!(night(&rules, t(-5 * Y), at(1, 23, 0, 0)), None);
    // Negative seconds: the night of day −1 reaches into day 0.
    let before = span(at(-1, 22, 0, 0), at(0, 6, 0, 0));
    assert_eq!(night(&rules, adult, t(-1)), before);
    assert_eq!(night(&rules, adult, at(-1, 21, 59, 59)), None);
    assert_eq!(night(&rules, adult, at(0, 5, 59, 59)), before);
    assert_eq!(night(&rules, adult, at(0, 6, 0, 0)), None);
    // The next night: tonight's until it begins, then tomorrow's.
    assert_eq!(next_night(&rules, adult, at(1, 6, 0, 0)), adults_first);
    assert_eq!(next_night(&rules, adult, at(1, 21, 59, 59)), adults_first);
    assert_eq!(
        next_night(&rules, adult, at(1, 22, 0, 0)),
        span(at(2, 22, 0, 0), at(3, 6, 0, 0))
    );
    assert_eq!(
        next_night(&rules, adult, at(2, 3, 0, 0)),
        span(at(2, 22, 0, 0), at(3, 6, 0, 0))
    );
}

/// The last night of time (the end of time `MAX_ABS_SECS` is a midnight, the first second of
/// day `K`): an adult of 29 has the night of day `K − 1` from 22:00 on, its end, 6:00 of day
/// `K + 1`, beyond time, so it lasts to the end, the last second included: `left` counts the
/// seconds to it, and a sleep begun in it has no end, as an action that would end beyond time
/// has none. Day `K` has no night (its plan at 5:00 lies beyond time). Time begins outside
/// every night: nobody is born before its first second, so nobody is 6 in its first years.
#[test]
fn the_last_night_of_time_lasts_to_its_end() {
    let rules = rules();
    let end = MAX_ABS_SECS;
    let last_day = end / D;
    assert_eq!(end % D, 0);
    let adult = t(end - 30 * Y);
    let whole = Night {
        from: t((last_day - 1) * D + 22 * H),
        until: None,
    };
    let last = Some(whole);
    assert_eq!(night_of(&rules, last_day - 1, adult), last);
    assert_eq!(night_of(&rules, last_day, adult), None);
    assert_eq!(night(&rules, adult, t(end - 2 * H - 1)), None);
    assert_eq!(night(&rules, adult, t(end - 2 * H)), last);
    assert_eq!(night(&rules, adult, t(end)), last);
    assert_eq!(next_night(&rules, adult, t(end - 3 * H)), last);
    assert_eq!(whole.left(t(end)), 1);
    assert_eq!(whole.left(t(end - 2 * H)), 2 * H + 1);
    assert!(whole.holds(t(end)) && !whole.holds(t(end - 2 * H - 1)));
    let asleep = InHouse {
        needs: [600, 420, 500, 700, 800].map(|value| NeedSample::new(value, t(end - H), 0)),
        place: Place::Room(2),
        action: None,
        due: None,
        decisions: Decisions {
            day: last_day - 1,
            count: 0,
        },
        collapsed: false,
        reserve: Reserve::full(1000, t(end - H)),
    };
    let sleep = OfferDef {
        rates: Default::default(),
        seconds: None,
        until_full: None,
        max_seconds: None,
        sleep: true,
        min_age: 0,
        max_age: 130,
        fallback: false,
    };
    assert_eq!(action_end(&asleep, &sleep, &rules, adult), None);
    // The first second of time: whoever lives then was born at it, under 6, without a night.
    let first = t(-end);
    assert_eq!(night(&rules, first, first), None);
    assert_eq!(night_of(&rules, -end / D, first), None);
}

/// A sleep ends with its night (`action_end`): begun at 22:00:04, or in the last second of the
/// night, at 6:00; begun at 6:00:03, after the night it was chosen in ended, at once. An offer
/// that does not sleep keeps its length.
#[test]
fn a_sleep_ends_with_its_night() {
    let rules = rules();
    let born = t(-30 * Y);
    let person = |at: i64| InHouse {
        needs: [600, 420, 500, 700, 800].map(|value| NeedSample::new(value, t(at), 0)),
        place: Place::Room(2),
        action: None,
        due: None,
        decisions: Decisions { day: 1, count: 0 },
        collapsed: false,
        reserve: Reserve::full(1000, t(at)),
    };
    let sleep = OfferDef {
        rates: Default::default(),
        seconds: None,
        until_full: None,
        max_seconds: None,
        sleep: true,
        min_age: 0,
        max_age: 130,
        fallback: false,
    };
    let morning = 2 * D + 6 * H;
    let end = |at: i64| action_end(&person(at), &sleep, &rules, born);
    assert_eq!(end(D + 22 * H + 4), Some(t(morning)));
    assert_eq!(end(morning - 1), Some(t(morning)));
    assert_eq!(end(morning + 3), Some(t(morning + 3)));
    let watch = OfferDef {
        rates: [(Need::Fun, 1200)].into_iter().collect(),
        seconds: Some(3_600),
        sleep: false,
        ..sleep
    };
    let at = D + 22 * H + 4;
    assert_eq!(
        action_end(&person(at), &watch, &rules, born),
        Some(t(at + 3_600))
    );
}

/// Bedtime (ADR-0026, addendum of 2026-11-04): on Saturday the couple idles all day, nothing
/// bringing 40 000 (the television at most 39 600 − 50 · 3 = 39 450, the rest −250) and the bed
/// closed; energy, 900 at 6:00 and −30 an hour, reaches 700 at 12:40 (another idle choice). At
/// their bedtime, 22:00, with energy 420 (900 − 480): the bed (energy +100 for the 8 h of the
/// night, 580 of 580: 580 · 580 − 50 · 4 = 336 200), she on place 0, he on 1; there at 22:00:04
/// with energy 420 (480.03 truncated). Energy is full at 3:48:04 (⌈3 600 · 580 / 100⌉ s) and
/// stays at 1 000; the sleep ends with the night at 6:00 (`done`) and, outside it, they idle in
/// the bedroom (the television at most 39 600 − 50 · 7). Her choice at 22:00 has its bedtime as its
/// cause, planned by her `core:day_started` of the first morning; Sunday's bedtime comes from
/// the plan at 5:00, by her `core:moved_in`. On Sunday energy, 1 000 at 6:00, is 700 at 16:00;
/// at 22:00, 1 000 − 480, the bed again, from the bedroom.
#[test]
fn bedtime_makes_an_idle_person_choose_the_bed() {
    let mut run = family(quiet(), &[], 4);
    run.advance(SATURDAY + SUNDAY + TEN_PM);
    let rested = [600, 1000, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY)[..9],
        [
            (
                0,
                LogPayload::DayStarted {
                    needs: MORNING_VALUES
                }
            ),
            (0, idle(0, MORNING_VALUES, &[])),
            (24_000, idle(0, [600, 700, 500, 700, 800], &[])),
            (
                TEN_PM,
                started((BED, 0), (0, 4), [600, 420, 500, 700, 800], &[], false)
            ),
            (TEN_PM + 4, reached(BED)),
            (SUNDAY, ended(BED, EndReason::Done, rested)),
            (SUNDAY, idle(2, rested, &[])),
            (SUNDAY + 36_000, idle(2, [600, 700, 500, 700, 800], &[])),
            (
                SUNDAY + TEN_PM,
                started((BED, 0), (2, 5), [600, 520, 500, 700, 800], &[], false)
            ),
        ]
    );
    assert_mirrored(&run, SATURDAY);
    let starts: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(1))])
        .filter(|entry| matches!(entry.payload, LogPayload::ActionStarted { .. }))
        .collect();
    let plan_of = |entry: &LogEntry| match entry.causes.as_slice() {
        [Cause::Plan(plan)] => (plan.planned_at, run.entry(&plan.by).payload.clone()),
        other => panic!("{other:?}"),
    };
    let (planned_at, by) = plan_of(starts[0]);
    assert_eq!(planned_at, t(SATURDAY));
    assert!(matches!(by, LogPayload::DayStarted { .. }), "{by:?}");
    let (planned_at, by) = plan_of(starts[1]);
    assert_eq!(planned_at, t(SATURDAY + SUNDAY - H));
    assert!(matches!(by, LogPayload::MovedIn { .. }), "{by:?}");
}

/// Sleep ends at 6:00 with energy at most 1 000: in the second after, both are awake in the
/// bedroom, energy written down at 1 000 at 6:00 with the awake rate, nothing pending at a
/// sleep.
#[test]
fn sleep_ends_at_six_with_energy_at_most_full() {
    let mut run = family(quiet(), &[], 4);
    run.advance(SATURDAY + SUNDAY);
    for id in [1, 2] {
        let person = &run.world.state().house().sims[&sim(id)];
        assert_eq!(
            (person.place, person.action),
            (Place::Room(2), None),
            "{id}"
        );
        assert_eq!(
            person.needs[1],
            NeedSample::new(1000, t(SATURDAY + SUNDAY), -30),
            "{id}"
        );
    }
}

/// A person watching television at bedtime finishes, then goes to bed: told at 21:30 to watch
/// (energy 435), she is there at 21:30:03. Her bedtime at 22:00 finds her at the television and
/// does nothing; he goes to bed on place 0. At 22:30:03 the television ends (energy 405: 495.03
/// truncated; fun 980, 800 + 180) and her choice, in the night, is the bed's free place 1, from
/// the living room, 7 s.
#[test]
fn a_person_watching_at_bedtime_finishes_then_goes_to_bed() {
    let mut run = family(quiet(), &[], 4);
    run.advance(SATURDAY + TEN_PM - 1_801);
    run.submit(command(1, TV));
    run.advance(SATURDAY + TEN_PM + 1_810);
    let watched = [600, 405, 500, 700, 980];
    let hers = doings(&run, 1, SATURDAY + TEN_PM - 1_800);
    assert_eq!(
        hers,
        [
            (
                0,
                started((TV, 0), (0, 3), [600, 435, 500, 700, 800], &[], true)
            ),
            (3, reached(TV)),
            (3_603, ended(TV, EndReason::Done, watched)),
            (3_603, started((BED, 1), (1, 7), watched, &[], false)),
            (3_610, reached(BED)),
        ]
    );
    let his = doings(&run, 2, SATURDAY + TEN_PM);
    assert_eq!(
        his[..2],
        [
            (
                0,
                started((BED, 0), (0, 4), [600, 420, 500, 700, 800], &[], false)
            ),
            (4, reached(BED)),
        ]
    );
}

/// The bladder at night (the loo, the bladder −200 an hour asleep): at 6:00 both take the loo
/// (500 to full: 500 · 500 − 50 · 2 = 249 900), full after 150 s, energy 899; then idle in the
/// bath. At 22:00 the bed from the bath (6 s), there at 22:00:06 with energy 420 and the
/// bladder 1 000, which reaches 150 at 2:15:06 (⌈3 600 · 850 / 200⌉ s): critical, the bed does
/// not raise it, the sleep ends (energy 420 + 425) and the loo comes first in the night (10⁹ +
/// 850 · 850 − 50 · 6 beats the toilet's 10⁹ − 50 · 4 and the bed). There at 2:15:12, the
/// bladder at 150 still (awake it stands), full 255 s later at 2:19:27 (energy 843: 2.18
/// truncated); then the bed again, there at 2:19:33 (energy 843), full of energy at 3:53:45,
/// the bladder at 266 when the night ends (1 000 − 734.8). He mirrors her on the second places.
#[test]
fn a_bladder_at_night_wakes_the_sleeper_who_goes_back_to_bed() {
    let data = data_with(&day_file(), (-30, -200), true);
    let mut run = family(data, &[], 4);
    run.advance(SATURDAY + SUNDAY);
    let after_loo = [600, 899, 1000, 700, 800];
    let woken = [600, 845, 150, 700, 800];
    let relieved = [600, 843, 1000, 700, 800];
    let critical = LogPayload::NeedCritical {
        need: Need::Bladder,
        level: 150,
    };
    let two_fifteen = TEN_PM + 4 * H + 906;
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (
                0,
                LogPayload::DayStarted {
                    needs: MORNING_VALUES
                }
            ),
            (0, started((LOO, 0), (0, 2), MORNING_VALUES, &[], false)),
            (2, reached(LOO)),
            (152, ended(LOO, EndReason::Full, after_loo)),
            (152, idle(3, after_loo, &[])),
            (24_000, idle(3, [600, 700, 1000, 700, 800], &[])),
            (
                TEN_PM,
                started((BED, 0), (3, 6), [600, 420, 1000, 700, 800], &[], false)
            ),
            (TEN_PM + 6, reached(BED)),
            (two_fifteen, critical.clone()),
            (two_fifteen, ended(BED, EndReason::Critical, woken)),
            (two_fifteen, started((LOO, 0), (2, 6), woken, &[], false)),
            (two_fifteen + 6, reached(LOO)),
            (two_fifteen + 261, ended(LOO, EndReason::Full, relieved)),
            (
                two_fifteen + 261,
                started((BED, 0), (3, 6), relieved, &[], false)
            ),
            (two_fifteen + 267, reached(BED)),
            (
                SUNDAY,
                ended(BED, EndReason::Done, [600, 1000, 266, 700, 800])
            ),
            (
                SUNDAY,
                started((LOO, 0), (2, 6), [600, 1000, 266, 700, 800], &[], false)
            ),
        ]
    );
    assert_mirrored(&run, SATURDAY);
}

/// No bed free means the sofa (ADR-0025): with a girl of 15 too old for the cot, she goes to
/// bed at her bedtime, 20:00, with energy 480 (the bed: energy +100 for 10 h, 520 · 520 − 50 ·
/// 4), on place 0. At 22:00 the woman takes place 1; for the man the bed is taken and the cot
/// not open at 32, so the sofa (580 · 400 − 50 · 3), from the hall. There at 22:00:03 with
/// energy 420 at +50: 819 at 6:00 (399.96 truncated), the girl and the woman full.
#[test]
fn no_free_bed_means_the_sofa() {
    let mut run = family(quiet(), &[15], 4);
    run.advance(SATURDAY + SUNDAY);
    assert_eq!(
        doings(&run, 3, SATURDAY + EIGHT_PM)[..2],
        [
            (
                0,
                started((BED, 0), (0, 4), [600, 480, 500, 700, 800], &[], false)
            ),
            (4, reached(BED)),
        ]
    );
    let late = [600, 420, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY + TEN_PM)[..2],
        [
            (0, started((BED, 1), (0, 4), late, &[], false)),
            (4, reached(BED)),
        ]
    );
    let rested = [600, 1000, 500, 700, 800];
    assert_eq!(
        doings(&run, 2, SATURDAY + TEN_PM),
        [
            (0, started((SOFA, 0), (0, 3), late, &[BED], false)),
            (3, reached(SOFA)),
            (
                SUNDAY - TEN_PM,
                ended(SOFA, EndReason::Done, [600, 819, 500, 700, 800])
            ),
            (SUNDAY - TEN_PM, idle(1, [600, 819, 500, 700, 800], &[])),
        ]
    );
    for id in [1, 3] {
        let ends: Vec<(i64, LogPayload)> = doings(&run, id, SATURDAY + SUNDAY)
            .into_iter()
            .take(1)
            .collect();
        assert_eq!(ends, [(0, ended(BED, EndReason::Done, rested))], "{id}");
    }
}

/// Nobody sleeps by day, even with low energy: falling 300 an hour from 900, it is 700 at
/// 6:40, 400 at 7:40, critical at 150 at 8:30 and at 50 at 8:50, each time a choice, and
/// nothing raises it while sofa and beds are closed; idle (the television at most 39 450). At 0,
/// at 9:00, they lie on the floor (ADR-0026, addendum of 2026-11-04 on a need at 0) until 14:00,
/// energy 250, by day again: idle (the television below 40 000 by its mood at each choice),
/// critical at 14:20 and 14:40, on the floor from 14:50 to 19:50, then 20:10 and 20:30, on the
/// floor again from 20:40, so their bedtime at 22:00 finds them lying and does nothing.
#[test]
fn nobody_sleeps_by_day() {
    let mut run = family(data_with(&day_file(), (-300, 0), false), &[], 4);
    run.advance(SATURDAY + TEN_PM);
    let critical = |level: u16| LogPayload::NeedCritical {
        need: Need::Energy,
        level,
    };
    let at = |energy: u16| [600, energy, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (
                0,
                LogPayload::DayStarted {
                    needs: MORNING_VALUES
                }
            ),
            (0, idle(0, MORNING_VALUES, &[])),
            (2_400, idle(0, at(700), &[])),
            (6_000, idle(0, at(400), &[])),
            (9_000, critical(150)),
            (9_000, idle(0, at(150), &[])),
            (10_200, critical(50)),
            (10_200, idle(0, at(50), &[])),
            (28_800, idle(0, at(250), &[])),
            (30_000, critical(150)),
            (30_000, idle(0, at(150), &[])),
            (31_200, critical(50)),
            (31_200, idle(0, at(50), &[])),
            (49_800, idle(0, at(250), &[])),
            (51_000, critical(150)),
            (51_000, idle(0, at(150), &[])),
            (52_200, critical(50)),
            (52_200, idle(0, at(50), &[])),
        ]
    );
    assert_mirrored(&run, SATURDAY);
}

/// A command of sleep outside the night is refused `not_night`, at 6:00 and in the last second
/// before 22:00, for the bed and the sofa; the cot is refused for her age first. One acting at
/// 22:00:00 is obeyed (energy 420): she sleeps on place 0, and her bedtime in that second finds
/// her walking. He goes to bed at his bedtime, on place 1; told at 23:00 to watch television he
/// wakes (energy 420 + 99.9 since 22:00:04), walks from the bedroom, 7 s, and watches until
/// 0:00:07 (energy 519 − 30.06, fun 980), then goes back to bed in the night. Her commanded
/// sleep ends with the night at 6:00; his too, energy full again (489 + 100 an hour from
/// 0:00:14).
#[test]
fn commands_of_sleep_and_in_the_night() {
    let mut run = family(quiet(), &[], 4);
    run.advance(SATURDAY);
    for object in [BED, SOFA] {
        assert_eq!(
            rejected(&mut run.world, command(1, object)),
            Some(RejectCode::NotNight)
        );
    }
    assert_eq!(
        rejected(&mut run.world, command(1, COT)),
        Some(RejectCode::WrongAge)
    );
    run.advance(SATURDAY + TEN_PM - 2);
    assert_eq!(
        rejected(&mut run.world, command(1, BED)),
        Some(RejectCode::NotNight)
    );
    run.advance(SATURDAY + TEN_PM - 1);
    run.submit(command(1, BED));
    run.advance(SATURDAY + TEN_PM + H - 1);
    run.submit(command(2, TV));
    run.advance(SATURDAY + SUNDAY);
    let late = [600, 420, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY + TEN_PM),
        [
            (0, started((BED, 0), (0, 4), late, &[], true)),
            (4, reached(BED)),
            (
                SUNDAY - TEN_PM,
                ended(BED, EndReason::Done, [600, 1000, 500, 700, 800])
            ),
            (SUNDAY - TEN_PM, idle(2, [600, 1000, 500, 700, 800], &[])),
        ]
    );
    let woken = [600, 519, 500, 700, 800];
    let watched = [600, 489, 500, 700, 980];
    assert_eq!(
        doings(&run, 2, SATURDAY + TEN_PM),
        [
            (0, started((BED, 1), (0, 4), late, &[], false)),
            (4, reached(BED)),
            (H, ended(BED, EndReason::Command, woken)),
            (H, started((TV, 0), (2, 7), woken, &[], true)),
            (H + 7, reached(TV)),
            (2 * H + 7, ended(TV, EndReason::Done, watched)),
            (2 * H + 7, started((BED, 1), (1, 7), watched, &[], false)),
            (2 * H + 14, reached(BED)),
            (
                SUNDAY - TEN_PM,
                ended(BED, EndReason::Done, [600, 1000, 500, 700, 980])
            ),
            (SUNDAY - TEN_PM, idle(2, [600, 1000, 500, 700, 980], &[])),
        ]
    );
}

/// A night that ends at a wake (here a child's, 6:55, for 6 to 12): the wake comes before the
/// sleep's end in that second (its kind first) and ends the sleep with reason `obligation`. The
/// family moves in on Tuesday, day 1, so its first morning, Wednesday 6:00, lies in the night
/// of the boy of 10 (sim 3, founder 2 of `test:night`): he goes to bed then. The bed and the cot
/// each fill 100 · 91 = 9 100, less 50 · 4; his moods there (the draw `mood`, ADR-0026, addendum
/// of 2026-11-04 on the mood), −4 for the bed and −7 for the cot, give the bed 8 736 − 200
/// against 8 463 − 200 (without the moods they tie and the bed is the smaller object). The
/// adults take the loo (both places); he is there at 6:00:04 with energy 900, at 6:55 energy 991
/// (900 + 91.6) when the wake ends his sleep, and outside the night he takes the loo (500 · 500
/// − 50 · 6), from the bedroom.
#[test]
fn a_wake_at_the_nights_end_ends_the_sleep() {
    let key = LineageKey::founder(&DataId::new("test:night").expect("id"), 2);
    let moods =
        [BED, COT].map(|object| i64::from(mood(7, key, t(2 * D + 6 * H), (object, 0), MOOD)));
    assert_eq!(moods, [-4, -7]);
    let [bed, cot] = moods.map(|mood| 9_100 * (100 + mood) / 100);
    assert_eq!((bed, cot), (8_736, 8_463));
    let mut day: serde_json::Value = serde_json::from_str(&day_file()).expect("the day");
    day["night"]["until"][0] = json!(24_900);
    let data = data_with(&day.to_string(), (-30, 0), true);
    let mut run = family(data, &[10], 1);
    let wednesday = 2 * D + 6 * H;
    run.advance(wednesday + 3_300);
    let woken = [600, 991, 500, 700, 800];
    assert_eq!(
        doings(&run, 3, wednesday)[2..],
        [
            (0, started((BED, 0), (0, 4), MORNING_VALUES, &[LOO], false)),
            (4, reached(BED)),
            (3_300, ended(BED, EndReason::Obligation, woken)),
            (3_300, started((LOO, 0), (2, 6), woken, &[], false)),
        ]
    );
}

/// The world `run` saved now: it loads and saves to the same bytes.
fn reloaded(run: &Run, data: &Data) -> World {
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (loaded, _) = World::load(&bytes, data.clone()).expect("loads");
    assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
    loaded
}

/// Saved in every new kind of second of the night, the save loads, saves to the same bytes and
/// runs on as the uninterrupted world to Monday 6:00, through Sunday's night, with the same
/// entries: with its bedtime pending (21:59:59), right after it and on the way to bed, asleep,
/// at the plan of 5:00 that plans Sunday's bedtime, in the last second of the night and right
/// after it; after a command of sleep acting at 22:00 (a walk begun in the next second); and in
/// the night of the bladder, woken, on the way to the loo and at it.
#[test]
fn a_save_in_the_night_runs_on_the_same() {
    let quiet_seconds = [
        TEN_PM - 1,
        TEN_PM,
        TEN_PM + 2,
        TEN_PM + 5 * H,
        SUNDAY - H - 1,
        SUNDAY - H,
        SUNDAY - 1,
        SUNDAY,
    ];
    let two_fifteen = TEN_PM + 4 * H + 906;
    let bladder_seconds = [two_fifteen, two_fifteen + 3, two_fifteen + 100];
    let until = SATURDAY + 2 * D;
    let check = |data: Data, saved_at: i64, then: Option<Input>| {
        let mut run = family(data.clone(), &[], 4);
        run.advance(SATURDAY + saved_at);
        if let Some(input) = then {
            run.submit(input);
        }
        let mut loaded = reloaded(&run, &data);
        let from = run.log.len();
        loaded.advance_until(&mut YearOut::default(), t(until), &mut |_| false);
        run.advance(until);
        assert_eq!(loaded.state_hash(), run.world.state_hash(), "{saved_at}");
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        assert_eq!(
            entries(&loaded.take_log()),
            entries(&run.log[from..]),
            "{saved_at}"
        );
    };
    for second in quiet_seconds {
        check(quiet(), second, None);
    }
    check(quiet(), TEN_PM - 1, Some(command(1, BED)));
    for second in bladder_seconds {
        check(data_with(&day_file(), (-30, -200), true), second, None);
    }
}
