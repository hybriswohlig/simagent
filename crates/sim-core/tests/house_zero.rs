//! A need at 0 (ADR-0026, addendum of 2026-11-04 on a need at 0): a fifth threshold, 0, and one
//! function that settles every need reaching it in a second, in the order of the needs, from
//! its crossing or from a jump on coming home. Energy at 0 lays the person on the floor of its
//! room, at the sleeping rates with energy rising 50 an hour, until energy is at 250 or the
//! night it fell in ends; the bladder at 0 is full at once and leaves hygiene at most 50, and
//! what does not raise hygiene ends; hygiene reaching 0 takes 150 from fun once; with fun at 0
//! a command for anything that does not raise fun is refused. The zero values are the
//! repository's `day.json`, with weekday mornings on the weekend (`common::weekday_mornings`);
//! everything else is a test house with rates chosen so that each value can be done by hand.
//!
//! The test house: rooms hall 0 (the entrance), living 1, bath 2, bedroom 3; the living room
//! 3 s, the bath 2 s and the bedroom 4 s from the hall, 5 s to an object in one's own room.
//! Objects: a bed for two 0 in the bedroom; a bookshelf for two 1 and a television for one 2
//! in the living room; the shower 3 and the toilet 4 in the bath; the dining table for two 5
//! and the fridge 6 in the hall; the sofa 7 (sleep with energy +50) in the living room. By
//! default every offer but the sleeping ones raises its need by 1 an hour for a minute, a gain
//! of 0; a test makes some of them real. The household: a woman of 30 (sim 1, the age bands of
//! 6 to 66) whose rates each test sets, and a man of 70 (sim 2, from 67) whose needs stand
//! still awake and whose energy rises 100 an hour asleep. Every choice that decides a test is
//! at least 10 percent away from the threshold of 40 000 points or carried by a critical need,
//! so the mood of a choice changes none of them.

mod common;

use std::ops::Range;

use common::day::{MORNING, Run, number, places_hold};
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::data::Need;
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Decisions, Due, Duty, InHouse, Place, stand_up};
use sim_core::ids::{EventId, SimId};
use sim_core::input::{Command, Input, InputRejected, RejectCode};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
const Y: i64 = YEAR.as_secs();

/// The repository's day: a need at 0 with energy +50 on the floor, up at 250, an accident's
/// hygiene at most 50, hygiene at 0 taking 150 from fun; the night of an adult from 22:00, of
/// one from 67 from 21:30, until 6:00; part time from 7:40 to 13:20, its wake at 6:55; the
/// jumps on coming home hunger −350, energy −200, bladder −400, hygiene −100, fun −150.
const REPOSITORY_DAY: &str = include_str!("../../../data/house/day.json");

/// The first morning of a household moved in at 23:59:59 of day 4: 6:00 of day 5, a Saturday,
/// without duties that weekend; Sunday 6:00 is a day later.
const SATURDAY: i64 = 5 * D + 6 * H;

/// Seconds of day 1 after its first morning (`MORNING`, Tuesday 6:00): the wake at 6:55, the
/// part-time departure at 7:40 and its return at 13:20.
const WAKE: i64 = 3_300;
const DEPART: i64 = 6_000;
const BACK: i64 = 26_400;

const BED: u16 = 0;
const BOOK: u16 = 1;
const TV: u16 = 2;
const SHOWER: u16 = 3;
const TOILET: u16 = 4;
const TABLE: u16 = 5;
const SOFA: u16 = 7;

const HALL: u8 = 0;
const LIVING: u8 = 1;
const BEDROOM: u8 = 3;

const START: [u16; 5] = [600, 900, 500, 700, 800];

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

/// Offer `id` raising `need` by `rate` an hour for `seconds`, at every age.
fn offer(id: &str, need: &str, rate: i32, seconds: u32) -> Value {
    json!({"id": id, "rates": {need: rate}, "seconds": seconds, "min_age": 0, "max_age": 130})
}

/// The kinds of the test house, each offer that does not sleep with a gain of 0.
fn kinds() -> Value {
    let sleep = json!({"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130});
    let sofa = json!({"id": "sleep", "rates": {"energy": 50}, "sleep": true, "min_age": 0,
        "max_age": 130});
    json!([
        {"id": "test:bed", "slots": 2, "offers": [sleep]},
        {"id": "test:book", "slots": 2, "offers": [offer("read", "fun", 1, 60)]},
        {"id": "test:tv", "slots": 1, "offers": [offer("watch", "fun", 1, 60)]},
        {"id": "core:shower", "slots": 1, "offers": [offer("shower", "hygiene", 1, 60)]},
        {"id": "core:toilet", "slots": 1, "offers": [offer("use", "bladder", 1, 60)]},
        {"id": "core:dining_table", "slots": 2, "offers": [offer("eat", "hunger", 1, 60)]},
        {"id": "core:fridge", "slots": 1, "offers": [offer("snack", "hunger", 1, 60)]},
        {"id": "core:sofa", "slots": 1, "offers": [sofa]}
    ])
}

/// The kinds with the only offer of kind `id` replaced by `offer`.
fn with(mut kinds: Value, id: &str, offer: Value) -> Value {
    for kind in kinds.as_array_mut().expect("the kinds") {
        if kind["id"] == id {
            kind["offers"] = json!([offer]);
        }
    }
    kinds
}

fn house() -> String {
    json!({
        "id": "test:zero", "capacity": 2,
        "rooms": [{"id": "hall", "entrance": true}, {"id": "living"}, {"id": "bath"},
            {"id": "bedroom"}],
        "child_day_room": "living", "child_night_room": "bedroom", "dwell_room": "living",
        "doors": [
            {"a": "hall", "b": "living", "seconds": 3},
            {"a": "hall", "b": "bath", "seconds": 2},
            {"a": "hall", "b": "bedroom", "seconds": 4}],
        "objects": [
            {"kind": "test:bed", "room": "bedroom"}, {"kind": "test:book", "room": "living"},
            {"kind": "test:tv", "room": "living"}, {"kind": "core:shower", "room": "bath"},
            {"kind": "core:toilet", "room": "bath"}, {"kind": "core:dining_table", "room": "hall"},
            {"kind": "core:fridge", "room": "hall"}, {"kind": "core:sofa", "room": "living"}]
    })
    .to_string()
}

/// The woman's rates awake and asleep (hunger, energy, bladder, hygiene, fun) in the bands of 6
/// to 66; the man's awake in the band from 67, asleep only his energy rising 100 an hour.
fn rates(awake: [i32; 5], asleep: [i32; 5], his: [i32; 5]) -> String {
    let names = ["hunger", "energy", "bladder", "hygiene", "fun"];
    let column = |hers: [i32; 5], his: [i32; 5]| -> Value {
        (names.iter().zip(hers.into_iter().zip(his)))
            .map(|(name, (her, him))| (name.to_string(), json!([her, her, her, him])))
            .collect::<serde_json::Map<String, Value>>()
            .into()
    };
    let comfort = [1000; 5];
    json!({"awake": column(awake, his), "asleep": column(asleep, [0, 100, 0, 0, 0]), "comfort": comfort})
        .to_string()
}

/// The repository's day with weekday mornings (`common::weekday_mornings`): the runs here begin
/// on a Saturday to have no duty, and their nights end at 6:00 as on a weekday.
fn day_file() -> String {
    common::weekday_mornings(REPOSITORY_DAY)
}

/// The data of the test house with `rates`, the day `day` and the kinds `kinds`.
fn data(rates: &str, day: &str, kinds: &Value) -> Data {
    let houses = common::houses_from(rates, day, &kinds.to_string(), &[&house()]);
    common::death_data(1e-9, None, json!([])).with_houses(houses)
}

/// The repository's day changed by `change`.
fn day_with(change: impl FnOnce(&mut Value)) -> String {
    let mut day: Value = serde_json::from_str(&day_file()).expect("the day");
    change(&mut day);
    day.to_string()
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

/// The first tag `test:zero_<n>` whose woman works part time (a draw from 70 to 89).
fn part_time() -> String {
    let plan: Range<u64> = 70..90;
    (0..1_000)
        .map(|n| format!("test:zero_{n}"))
        .find(|tag| plan.contains(&draw(tag, 0)))
        .expect("a tag")
}

/// What a test runs: the data, the household's tag, the day it moves in at 23:59:59, and
/// the inputs each given at a clock (so acting in the second after it).
#[derive(Clone)]
struct Script {
    data: Data,
    tag: String,
    day: i64,
    inputs: Vec<(i64, Input)>,
}

impl Script {
    fn on(data: Data, tag: &str, day: i64) -> Script {
        Script {
            data,
            tag: tag.to_owned(),
            day,
            inputs: Vec::new(),
        }
    }

    /// A Saturday on `data`.
    fn saturday(data: Data) -> Script {
        Script::on(data, "test:zero", 4)
    }

    fn input(mut self, clock: i64, input: Input) -> Script {
        self.inputs.push((clock, input));
        self
    }

    /// The household moved in, run to `until` one second with events at a time with the
    /// places checked after each, the inputs before `until` given at their clocks.
    fn run(&self, until: i64) -> Run {
        let born = [-30 * Y, -70 * Y];
        let (mut run, _) = Run::family(self.data.clone(), 7, &self.tag, &born, self.day);
        for (clock, input) in self.inputs.iter().filter(|(clock, _)| *clock < until) {
            run.step(*clock, places_hold);
            run.submit(input.clone());
        }
        run.step(until, places_hold);
        run
    }
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

/// What a person does from `origin` on, as (seconds after it, payload): its day's plan, its
/// choices, arrivals, ends, idling, critical and empty needs and their consequences, leaving and
/// coming home.
fn doings(run: &Run, id: u64, origin: i64) -> Vec<(i64, LogPayload)> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
        .filter(|entry| entry.t.as_secs() >= origin)
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::DayPlanned { .. }
                    | LogPayload::ActionStarted { .. }
                    | LogPayload::Reached { .. }
                    | LogPayload::ActionEnded { .. }
                    | LogPayload::Idle { .. }
                    | LogPayload::NeedCritical { .. }
                    | LogPayload::LeftHome { .. }
                    | LogPayload::CameHome { .. }
                    | LogPayload::Missed { .. }
                    | LogPayload::NeedEmpty { .. }
                    | LogPayload::Collapsed { .. }
                    | LogPayload::Accident { .. }
            )
        })
        .map(|entry| (entry.t.as_secs() - origin, entry.payload.clone()))
        .collect()
}

/// The entry of `sim` with this payload at `at`.
fn entry_of<'a>(run: &'a Run, id: u64, at: i64, payload: &LogPayload) -> &'a LogEntry {
    (run.log.iter())
        .find(|entry| {
            entry.t.as_secs() == at
                && entry.subjects == [Subject::Sim(sim(id))]
                && entry.payload == *payload
        })
        .expect("the entry")
}

/// Whether an entry was caused by the entry `id` alone.
fn caused_by(entry: &LogEntry, id: &EventId) -> bool {
    matches!(entry.causes.as_slice(), [Cause::Event(cause)] if number(cause) == number(id))
}

/// The entry that planned the event an entry was caused by.
fn planned_by(entry: &LogEntry) -> EventId {
    match entry.causes.as_slice() {
        [Cause::Plan(plan)] => plan.by,
        other => panic!("not planned: {other:?}"),
    }
}

fn started(
    (object, slot): (u16, u8),
    (room, walk): (u8, u32),
    needs: [u16; 5],
    busy: &[u16],
    commanded: bool,
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new("test:zero").expect("id"),
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

fn idle(room: u8, needs: [u16; 5], busy: &[u16], waits: Option<u16>) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new("test:zero").expect("id"),
        room,
        needs,
        busy: busy.to_vec(),
        waits,
        held: Vec::new(),
        tier: Vec::new(),
    }
}

fn reached(object: u16) -> LogPayload {
    LogPayload::Reached { object }
}

fn critical(need: Need, level: u16) -> LogPayload {
    LogPayload::NeedCritical { need, level }
}

fn empty(need: Need) -> LogPayload {
    LogPayload::NeedEmpty { need }
}

fn collapsed(room: u8) -> LogPayload {
    LogPayload::Collapsed { room }
}

fn accident(needs: [u16; 5]) -> LogPayload {
    LogPayload::Accident { needs }
}

/// Whether she lies on the floor, and her needs, at the clock.
fn lying(run: &Run) -> (bool, [u16; 5]) {
    let state = run.world.state();
    let her = &state.house().sims[&sim(1)];
    let now = run.world.closed_through();
    (her.collapsed, her.needs.map(|need| need.value_at(now)))
}

/// She tires: energy falls 900 an hour awake. Asleep and on the floor hunger falls 30, the
/// bladder 40 and fun 25 an hour; the rest stands.
fn tiring() -> Data {
    let asleep = [-30, 100, -40, 0, -25];
    data(
        &rates([0, -900, 0, 0, 0], asleep, [0; 5]),
        &day_file(),
        &kinds(),
    )
}

/// Saturday: nothing brings 40 000 (the book and the television 200 · 0, the shower 300 · 0),
/// so she idles in the hall. Energy from 900 at −900 an hour reaches 700, 400, 150, 50 and 0
/// after 800, 2 000, 3 000, 3 400 and 3 600 s; she chooses at each and idles, nothing by day
/// raising energy. At 0 she lies down in the hall: hunger −30, energy +50, bladder −40, fun −25
/// an hour. The bladder at 400 (after 9 000 s on the floor) and fun at 700 (14 400 s) do
/// nothing. Energy is at 250 after exactly 18 000 s, at 12:00 (249 the second before); she gets
/// up with hunger 450, bladder 300, fun 675 and idles, by the event the collapse planned. Then
/// 250 at −900: 150, 50 and 0 after 400, 800 and 1 000 s; down again at 12:16:40. On the floor
/// hunger at 400 (6 000 s) does nothing, the bladder at 150 (13 500 s) writes its entry and
/// nothing more. The man idles all along. Commands to her while she lies are refused
/// `collapsed`, one for a bed by day first `not_night`.
#[test]
fn energy_at_0_lays_her_on_the_floor_for_five_hours() {
    let script = Script::saturday(tiring());
    let mut run = script.run(SATURDAY + 3_600);
    assert_eq!(lying(&run), (true, [600, 0, 500, 700, 800]));
    let world = &mut run.world;
    assert_eq!(
        rejected(world, command(1, BOOK)),
        Some(RejectCode::Collapsed)
    );
    assert_eq!(rejected(world, command(1, BED)), Some(RejectCode::NotNight));
    // 17 999 s on the floor: hunger −149.99, bladder −199.99, fun −124.99, each cut toward 0.
    let mut run = script.run(SATURDAY + 21_599);
    assert_eq!(lying(&run), (true, [451, 249, 301, 700, 676]));
    run.step(SATURDAY + 40_000, places_hold);
    let first = [600, 900, 500, 700, 800];
    let energy = |value: u16| [600, value, 500, 700, 800];
    let up = |value: u16| [450, value, 300, 700, 675];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, first, &[], None)),
            (800, idle(HALL, energy(700), &[], None)),
            (2_000, idle(HALL, energy(400), &[], None)),
            (3_000, critical(Need::Energy, 150)),
            (3_000, idle(HALL, energy(150), &[], None)),
            (3_400, critical(Need::Energy, 50)),
            (3_400, idle(HALL, energy(50), &[], None)),
            (3_600, empty(Need::Energy)),
            (3_600, collapsed(HALL)),
            (21_600, idle(HALL, up(250), &[], None)),
            (22_000, critical(Need::Energy, 150)),
            (22_000, idle(HALL, up(150), &[], None)),
            (22_400, critical(Need::Energy, 50)),
            (22_400, idle(HALL, up(50), &[], None)),
            (22_600, empty(Need::Energy)),
            (22_600, collapsed(HALL)),
            (36_100, critical(Need::Bladder, 150)),
        ]
    );
    assert_eq!(
        doings(&run, 2, SATURDAY),
        [(0, idle(HALL, first, &[], None))]
    );
    // The causes: the empty need by the plan of its crossing, the collapse by it, getting up by
    // the plan the collapse made.
    let at = SATURDAY + 3_600;
    let empty_entry = entry_of(&run, 1, at, &empty(Need::Energy));
    let down = entry_of(&run, 1, at, &collapsed(HALL));
    assert!(caused_by(down, &empty_entry.id));
    assert!(matches!(empty_entry.causes.as_slice(), [Cause::Plan(_)]));
    let got_up = entry_of(&run, 1, SATURDAY + 21_600, &idle(HALL, up(250), &[], None));
    assert_eq!(number(&planned_by(got_up)), number(&down.id));
    let (lies, _) = lying(&run);
    assert!(lies);
    assert_eq!(
        run.world.state().house().sims[&sim(1)].place,
        Place::Room(HALL)
    );
}

/// A walk ends where it began: commanded to the bookshelf at 3 599 s (energy 1, idle, so
/// neither critical nor listless), she walks the 3 s from the hall; at 3 600 s energy is at 0,
/// the walk ends with reason `collapse` and she lies on the floor of the hall, the bookshelf's
/// places free.
#[test]
fn a_walk_ends_in_the_room_it_began_in() {
    let script = Script::saturday(tiring()).input(SATURDAY + 3_598, command(1, BOOK));
    let run = script.run(SATURDAY + 3_700);
    let at = 3_599;
    assert_eq!(
        doings(&run, 1, SATURDAY + at),
        [
            (
                0,
                started((BOOK, 0), (HALL, 3), [600, 1, 500, 700, 800], &[], true)
            ),
            (1, empty(Need::Energy)),
            (1, ended(BOOK, EndReason::Collapse, [600, 0, 500, 700, 800])),
            (1, collapsed(HALL)),
        ]
    );
    let state = run.world.state();
    let her = &state.house().sims[&sim(1)];
    assert_eq!(
        (her.place, her.action, her.collapsed),
        (Place::Room(HALL), None, true)
    );
    let end = entry_of(
        &run,
        1,
        SATURDAY + 3_600,
        &ended(BOOK, EndReason::Collapse, [600, 0, 500, 700, 800]),
    );
    let down = entry_of(&run, 1, SATURDAY + 3_600, &collapsed(HALL));
    assert!(caused_by(down, &end.id));
}

/// The night ends the floor sooner. Only the bed is for children (to 12), so an adult sleeps on
/// the sofa or nowhere. Her energy falls 45 an hour: 700 and 400 after 16 000 and 40 000 s. The
/// man (67 and over, his night from 21:30) takes the sofa at his bedtime, 15:30 after the
/// morning. At her bedtime, 22:00 (57 600 s), with energy 180, there is no bed for her and the
/// sofa is taken: she idles. 150 at 60 000 s (22:40) and 50 at 68 000 s: the same. At 72 000 s,
/// 2:00 on Sunday, she lies down in the hall, in the night that began at 22:00; 250 would come
/// at 7:00, the night ends at 6:00, so she gets up then with energy 200 and, by day, idles; the
/// sofa is still his in that second, his sleep ending after her (sim 2 after sim 1).
#[test]
fn the_night_ends_the_floor_sooner() {
    let kinds = with(
        kinds(),
        "test:bed",
        json!({"id": "sleep", "sleep": true, "min_age": 0, "max_age": 12}),
    );
    let data = data(
        &rates([0, -45, 0, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &kinds,
    );
    let run = Script::saturday(data).run(SATURDAY + D + 60);
    let energy = |value: u16| [600, value, 500, 700, 800];
    let sofa = [SOFA];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (16_000, idle(HALL, energy(700), &[], None)),
            (40_000, idle(HALL, energy(400), &[], None)),
            (57_600, idle(HALL, energy(180), &sofa, None)),
            (60_000, critical(Need::Energy, 150)),
            (60_000, idle(HALL, energy(150), &sofa, None)),
            (68_000, critical(Need::Energy, 50)),
            (68_000, idle(HALL, energy(50), &sofa, None)),
            (72_000, empty(Need::Energy)),
            (72_000, collapsed(HALL)),
            (D, idle(HALL, energy(200), &sofa, None)),
        ]
    );
    assert_eq!(
        doings(&run, 2, SATURDAY)[..3],
        [
            (0, idle(HALL, START, &[], None)),
            (55_800, started((SOFA, 0), (HALL, 3), START, &[], false)),
            (55_803, reached(SOFA)),
        ]
    );
}

/// Lying through the bedtime and up in the night, into bed. Her energy falls 60 an hour: 700,
/// 400, 150, 50 and 0 after 12 000, 30 000, 45 000, 51 000 and 54 000 s, 21:00, outside her
/// night. The man goes to bed at 21:30 (place 0). Her bedtime at 22:00 finds her on the floor and
/// does nothing; she gets up after five hours, at 2:00 (72 000 s), with energy 250, in her night,
/// so she takes the free place of the bed, 4 s away; asleep energy +100 an hour from 250 at
/// 72 004 s to 649 at 6:00, when she idles in the bedroom (the bed is no offer by day).
#[test]
fn lying_through_bedtime_and_up_into_bed() {
    let data = data(
        &rates([0, -60, 0, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &kinds(),
    );
    let run = Script::saturday(data).run(SATURDAY + D + 60);
    let energy = |value: u16| [600, value, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (12_000, idle(HALL, energy(700), &[], None)),
            (30_000, idle(HALL, energy(400), &[], None)),
            (45_000, critical(Need::Energy, 150)),
            (45_000, idle(HALL, energy(150), &[], None)),
            (51_000, critical(Need::Energy, 50)),
            (51_000, idle(HALL, energy(50), &[], None)),
            (54_000, empty(Need::Energy)),
            (54_000, collapsed(HALL)),
            (
                72_000,
                started((BED, 1), (HALL, 4), energy(250), &[], false)
            ),
            (72_004, reached(BED)),
            (D, ended(BED, EndReason::Done, energy(649))),
            (D, idle(BEDROOM, energy(649), &[], None)),
        ]
    );
    assert_eq!(
        doings(&run, 2, SATURDAY)[1],
        (55_800, started((BED, 0), (HALL, 4), START, &[], false))
    );
}

/// She tires fast: energy falls 1 000 an hour awake, on the floor only energy moves.
fn weary(day: &str) -> Data {
    data(
        &rates([0, -1_000, 0, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
        day,
        &kinds(),
    )
}

/// The departure waits for one lying on the floor. Tuesday; she works part time: the plan at the
/// morning, her wake at 6:55, the departure at 7:40, the return at 13:20. Energy at −1 000 an
/// hour reaches 700, 400, 150, 50 and 0 after 720, 1 800, 2 700, 3 060 and 3 240 s; she lies in
/// the hall from 6:54. The wake finds her there and does nothing; the departure waits, due at
/// 13:20. She gets up at 11:54 (21 240 s) and leaves at once, 15 240 s late. Away every need
/// stands; at 13:20 she comes home and the jumps take [600, 250, 500, 700, 800] to [250, 50,
/// 100, 600, 650]: the bladder at 100 is critical, so the toilet (critical, 2 s away).
#[test]
fn a_departure_waits_for_one_lying_and_leaves_late() {
    let tag = part_time();
    let script = Script::on(weary(&day_file()), &tag, 0);
    let run = script.run(MORNING + DEPART);
    let due = Due {
        place: Duty::Work,
        depart: t(D + 27_600),
        back: t(D + 48_000),
    };
    let her = &run.world.state().house().sims[&sim(1)];
    assert_eq!((her.collapsed, her.due), (true, Some(due)));
    let run = script.run(MORNING + BACK);
    let energy = |value: u16| [600, value, 500, 700, 800];
    let planned = LogPayload::DayPlanned {
        place: Duty::Work,
        depart: t(D + 27_600),
        back: t(D + 48_000),
    };
    assert_eq!(
        doings(&run, 1, MORNING),
        [
            (0, planned),
            (0, idle(HALL, START, &[], None)),
            (720, idle(HALL, energy(700), &[], None)),
            (1_800, idle(HALL, energy(400), &[], None)),
            (2_700, critical(Need::Energy, 150)),
            (2_700, idle(HALL, energy(150), &[], None)),
            (3_060, critical(Need::Energy, 50)),
            (3_060, idle(HALL, energy(50), &[], None)),
            (3_240, empty(Need::Energy)),
            (3_240, collapsed(HALL)),
            (
                21_240,
                LogPayload::LeftHome {
                    place: Duty::Work,
                    late_secs: 15_240
                }
            ),
            (BACK, LogPayload::CameHome { place: Duty::Work }),
            (
                BACK,
                started((TOILET, 0), (HALL, 2), [250, 50, 100, 600, 650], &[], false)
            ),
        ]
    );
    // She is in the hall when the wake comes, and it writes nothing.
    assert!(
        run.log
            .iter()
            .all(|entry| entry.t.as_secs() != MORNING + WAKE)
    );
}

/// A duty missed lying, and the next day's left late. With energy rising only 10 an hour on the
/// floor she lies 25 hours from 6:54 on Tuesday: the departure at 7:40 waits, its return at
/// 13:20 finds it still waiting, a missed duty; the bedtime, Wednesday's plan at 5:00 and its
/// wake at 6:55 change nothing; Wednesday's departure at 7:40 waits too, and she leaves when she
/// gets up at 7:54, 840 s late.
#[test]
fn a_duty_missed_lying_and_the_next_left_late() {
    let tag = part_time();
    let slow = day_with(|day| day["zero"]["collapse_energy_rate"] = json!(10));
    let run = Script::on(weary(&slow), &tag, 0).run(MORNING + D + 7_300);
    let lying_tail: Vec<(i64, LogPayload)> = doings(&run, 1, MORNING + 3_240);
    assert_eq!(
        lying_tail,
        [
            (0, empty(Need::Energy)),
            (0, collapsed(HALL)),
            (BACK - 3_240, LogPayload::Missed { place: Duty::Work }),
            (
                // Wednesday 5:00 is 82 800 s after Tuesday 6:00.
                82_800 - 3_240,
                LogPayload::DayPlanned {
                    place: Duty::Work,
                    depart: t(2 * D + 27_600),
                    back: t(2 * D + 48_000),
                }
            ),
            (
                90_000,
                LogPayload::LeftHome {
                    place: Duty::Work,
                    late_secs: 840
                }
            ),
        ]
    );
}

/// A need at 50 gets her up. The table is real (hunger +1 800 an hour for 20 min, 400 · 400 at
/// 6:00): she eats from 6:00:05, hunger full, energy 599 at 1 205 s (−900 an hour), then idles.
/// Energy at 0 after 3 600 s: down, and on the floor hunger falls 200 an hour from 1 000: at 700
/// and 400 (5 400 and 10 800 s later) nothing, at 150 (15 300 s) its entry alone, at 50 (17 100
/// s) its entry and she gets up with energy 237 (237.5), long before 250; hunger is critical, so
/// the table. The man eats beside her in the morning and idles.
#[test]
fn a_need_at_50_gets_her_up() {
    let kinds = with(
        kinds(),
        "core:dining_table",
        offer("eat", "hunger", 1_800, 1_200),
    );
    let data = data(
        &rates([0, -900, 0, 0, 0], [-200, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &kinds,
    );
    let run = Script::saturday(data).run(SATURDAY + 20_700);
    let fed = |value: u16| [1000, value, 500, 700, 800];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, started((TABLE, 0), (HALL, 5), START, &[], false)),
            (5, reached(TABLE)),
            (1_205, ended(TABLE, EndReason::Done, fed(599))),
            (1_205, idle(HALL, fed(599), &[], None)),
            (2_000, idle(HALL, fed(400), &[], None)),
            (3_000, critical(Need::Energy, 150)),
            (3_000, idle(HALL, fed(150), &[], None)),
            (3_400, critical(Need::Energy, 50)),
            (3_400, idle(HALL, fed(50), &[], None)),
            (3_600, empty(Need::Energy)),
            (3_600, collapsed(HALL)),
            (18_900, critical(Need::Hunger, 150)),
            (20_700, critical(Need::Hunger, 50)),
            (
                20_700,
                started((TABLE, 0), (HALL, 5), [50, 237, 500, 700, 800], &[], false)
            ),
        ]
    );
    let woke = entry_of(&run, 1, SATURDAY + 20_700, &critical(Need::Hunger, 50));
    let chose = entry_of(
        &run,
        1,
        SATURDAY + 20_700,
        &started((TABLE, 0), (HALL, 5), [50, 237, 500, 700, 800], &[], false),
    );
    assert!(caused_by(chose, &woke.id));
    assert!(!lying(&run).0);
}

/// The bladder at 0 in a book. The toilet is for 12 hours at +1 an hour, the bookshelf +150 an
/// hour for an hour, the shower +480 an hour for 15 min (gains 12, 150 and 120: below 40 000 at
/// 6:00). The man is sent to the toilet at 6:00:01. Her bladder falls 120 an hour: at 400 (3 000
/// s) she idles (600 · 12 for the taken toilet is not enough to wait), at 150 and 50 she waits
/// for it. Sent to the bookshelf with the bladder at 40 (13 800 s), she reads from 13 803 s; the
/// bladder is at 0 at 15 000 s, 20 minutes on: full at once, hygiene from 700 to 50, the book
/// ends with reason `critical` with fun at 849 (800 + 150 · 1 197 / 3 600) and the shower wins,
/// hygiene the critical need, 5 s from the living room.
#[test]
fn the_bladder_at_0_in_a_book() {
    let kinds = with(kinds(), "core:toilet", offer("use", "bladder", 1, 43_200));
    let kinds = with(kinds, "test:book", offer("read", "fun", 150, 3_600));
    let kinds = with(kinds, "core:shower", offer("shower", "hygiene", 480, 900));
    let data = data(
        &rates([0, 0, -120, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &kinds,
    );
    let script = Script::saturday(data)
        .input(SATURDAY, command(2, TOILET))
        .input(SATURDAY + 13_799, command(1, BOOK));
    let run = script.run(SATURDAY + 15_005);
    let bladder = |value: u16| [600, 900, value, 700, 800];
    let after = [600, 900, 1000, 50, 849];
    let toilet = [TOILET];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (3_000, idle(HALL, bladder(400), &toilet, None)),
            (10_500, critical(Need::Bladder, 150)),
            (10_500, idle(HALL, bladder(150), &toilet, Some(TOILET))),
            (13_500, critical(Need::Bladder, 50)),
            (13_500, idle(HALL, bladder(50), &toilet, Some(TOILET))),
            (
                13_800,
                started((BOOK, 0), (HALL, 3), bladder(40), &toilet, true)
            ),
            (13_803, reached(BOOK)),
            (15_000, empty(Need::Bladder)),
            (15_000, accident(after)),
            (15_000, ended(BOOK, EndReason::Critical, after)),
            (
                15_000,
                started((SHOWER, 0), (LIVING, 5), after, &toilet, false)
            ),
            (15_005, reached(SHOWER)),
        ]
    );
    let at = SATURDAY + 15_000;
    let wet = entry_of(&run, 1, at, &accident(after));
    let end = entry_of(&run, 1, at, &ended(BOOK, EndReason::Critical, after));
    assert!(caused_by(end, &wet.id));
    assert_eq!(
        doings(&run, 2, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (1, started((TOILET, 0), (HALL, 2), START, &[], true)),
            (3, reached(TOILET)),
        ]
    );
}

/// The kinds of the tests of fun: the television real (+240 an hour for 45 min, a gain of 180:
/// 200 · 180 is not enough at 6:00, 300 · 180 is enough to wait for it), the bookshelf for
/// children alone, so that for an adult nothing else raises fun.
fn fun_kinds() -> Value {
    let kinds = with(kinds(), "test:tv", offer("watch", "fun", 240, 2_700));
    let book = json!({"id": "read", "rates": {"fun": 1}, "seconds": 60, "min_age": 0,
        "max_age": 12});
    with(kinds, "test:book", book)
}

/// Her fun falls 1 200 an hour; the man is sent to the television at 6:00:01, there until 2 704
/// s. She waits for it from 700 (300 s) and keeps her place at 400 (1 200 s; ADR-0026, addendum
/// of 2026-11-04 on comfort and the time-filler); at 150, 50 and at 0 (2 400 s) each is a choice. With
/// fun at 0, a command for the shower is refused `listless`; one for the taken television, which
/// raises fun, is refused only `slot_busy`; one for the bed by day `not_night`. When he is done the
/// place is held for her: she takes it, there at 2 707 s; fun is 0 there until 2 722 s (1 after
/// ⌈3 600 / 240⌉ = 15 s), so a command for the shower is refused `listless` at 2 721 s and,
/// listless no more, `critical_need` at 2 722 s (the television raises her fun at 1).
#[test]
fn fun_at_0_refuses_what_is_no_fun() {
    let data = data(
        &rates([0, 0, 0, 0, -1_200], [0, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &fun_kinds(),
    );
    let script = Script::saturday(data).input(SATURDAY, command(2, TV));
    let mut run = script.run(SATURDAY + 2_400);
    let world = &mut run.world;
    assert_eq!(
        rejected(world, command(1, SHOWER)),
        Some(RejectCode::Listless)
    );
    assert_eq!(rejected(world, command(1, TV)), Some(RejectCode::SlotBusy));
    assert_eq!(rejected(world, command(1, BED)), Some(RejectCode::NotNight));
    let mut run = script.run(SATURDAY + 2_720);
    let world = &mut run.world;
    assert_eq!(
        rejected(world, command(1, SHOWER)),
        Some(RejectCode::Listless)
    );
    run.step(SATURDAY + 2_721, places_hold);
    let world = &mut run.world;
    assert_eq!(
        rejected(world, command(1, SHOWER)),
        Some(RejectCode::CriticalNeed)
    );
    let fun = |value: u16| [600, 900, 500, 700, value];
    let tv = [TV];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (300, idle(HALL, fun(700), &tv, Some(TV))),
            (1_950, critical(Need::Fun, 150)),
            (1_950, idle(HALL, fun(150), &tv, Some(TV))),
            (2_250, critical(Need::Fun, 50)),
            (2_250, idle(HALL, fun(50), &tv, Some(TV))),
            (2_400, empty(Need::Fun)),
            (2_400, idle(HALL, fun(0), &tv, Some(TV))),
            (2_704, started((TV, 0), (HALL, 3), fun(0), &[], false)),
            (2_707, reached(TV)),
        ]
    );
}

/// Hygiene at 0 takes fun once. The shower runs 12 hours (+2 800 an hour, 300 · 300 at 6:00):
/// she takes it at 6:00 and stays. The man's hygiene falls 700 an hour; the bookshelf gives +150
/// an hour for an hour. He waits for the shower at 6:00, keeps his place at 400 (1 543 s; ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler) and chooses again at 150 and 50 (2 829
/// and 3 343 s). At 0 (3 600 s) fun drops from 800 to 650, and the bookshelf now brings 350 · 150:
/// he reads from 3 603 s with hygiene at 0, the change of rates there takes nothing more, and at
/// 7 203 s fun is 800, then he waits for the shower again.
#[test]
fn hygiene_at_0_takes_fun_once() {
    let kinds = with(
        kinds(),
        "core:shower",
        offer("shower", "hygiene", 2_800, 43_200),
    );
    let kinds = with(kinds, "test:book", offer("read", "fun", 150, 3_600));
    let data = data(
        &rates([0; 5], [0, 100, 0, 0, 0], [0, 0, 0, -700, 0]),
        &day_file(),
        &kinds,
    );
    let run = Script::saturday(data).run(SATURDAY + 7_203);
    let hygiene = |value: u16| [600, 900, 500, value, 800];
    let shower = [SHOWER];
    assert_eq!(
        doings(&run, 2, SATURDAY),
        [
            (0, idle(HALL, START, &shower, Some(SHOWER))),
            (2_829, critical(Need::Hygiene, 150)),
            (2_829, idle(HALL, hygiene(150), &shower, Some(SHOWER))),
            (3_343, critical(Need::Hygiene, 50)),
            (3_343, idle(HALL, hygiene(50), &shower, Some(SHOWER))),
            (3_600, empty(Need::Hygiene)),
            (
                3_600,
                started(
                    (BOOK, 0),
                    (HALL, 3),
                    [600, 900, 500, 0, 650],
                    &shower,
                    false
                )
            ),
            (3_603, reached(BOOK)),
            (7_203, ended(BOOK, EndReason::Done, [600, 900, 500, 0, 800])),
            (
                7_203,
                idle(LIVING, [600, 900, 500, 0, 800], &shower, Some(SHOWER))
            ),
        ]
    );
    assert_eq!(
        doings(&run, 1, SATURDAY)[..2],
        [
            (0, started((SHOWER, 0), (HALL, 2), START, &[], false)),
            (2, reached(SHOWER)),
        ]
    );
}

/// Two needs at 0 in one second, by their crossings: the toilet runs 12 hours (+1 an hour) and
/// the man is sent to it at 6:00:01; the shower is +480 an hour for 15 min. Her energy falls 900
/// and her bladder 500 an hour, both at 0 after 3 600 s. Before, she idles (bladder 400 at 720
/// s, energy 700 and 400 at 800 and 2 000 s) and waits for the toilet once the bladder is
/// critical (2 520, 3 000, 3 240 and 3 400 s). At 3 600 s energy first: its entry, she lies down
/// in the hall and leaves the list; then the bladder: its entry, the accident with hygiene at
/// 50, no choice on the floor. Nothing is dropped: she gets up at 21 600 s and takes the shower.
#[test]
fn two_needs_at_0_in_one_second() {
    let kinds = with(kinds(), "core:toilet", offer("use", "bladder", 1, 43_200));
    let kinds = with(kinds, "core:shower", offer("shower", "hygiene", 480, 900));
    let data = data(
        &rates([0, -900, -500, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
        &day_file(),
        &kinds,
    );
    let script = Script::saturday(data).input(SATURDAY, command(2, TOILET));
    let run = script.run(SATURDAY + 21_600);
    let both = |energy: u16, bladder: u16| [600, energy, bladder, 700, 800];
    let toilet = [TOILET];
    let waits = Some(TOILET);
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (720, idle(HALL, both(720, 400), &toilet, None)),
            (800, idle(HALL, both(700, 389), &toilet, None)),
            (2_000, idle(HALL, both(400, 223), &toilet, None)),
            (2_520, critical(Need::Bladder, 150)),
            (2_520, idle(HALL, both(270, 150), &toilet, waits)),
            (3_000, critical(Need::Energy, 150)),
            (3_000, idle(HALL, both(150, 84), &toilet, waits)),
            (3_240, critical(Need::Bladder, 50)),
            (3_240, idle(HALL, both(90, 50), &toilet, waits)),
            (3_400, critical(Need::Energy, 50)),
            (3_400, idle(HALL, both(50, 28), &toilet, waits)),
            (3_600, empty(Need::Energy)),
            (3_600, collapsed(HALL)),
            (3_600, empty(Need::Bladder)),
            (3_600, accident([600, 0, 1000, 50, 800])),
            (
                21_600,
                started(
                    (SHOWER, 0),
                    (HALL, 2),
                    [600, 250, 1000, 50, 800],
                    &toilet,
                    false
                )
            ),
        ]
    );
}

/// Coming home with every need at 0: the jumps of this day take 1 000 from each, her needs stand
/// still. She works part time; at 13:20 the jumps leave all five at 0: hunger's entry, energy's
/// and she lies down in the hall, the bladder's and the accident (bladder full, hygiene still 0),
/// hygiene's (fun already 0, nothing to take), fun's. On the floor she does not choose.
#[test]
fn coming_home_with_every_need_at_0() {
    let tag = part_time();
    let jumps = day_with(|day| {
        day["return_jumps"] = json!({"hunger": -1000, "energy": -1000, "bladder": -1000, "hygiene": -1000, "fun": -1000});
    });
    let data = data(&rates([0; 5], [0, 100, 0, 0, 0], [0; 5]), &jumps, &kinds());
    let script = Script::on(data, &tag, 0);
    let run = script.run(MORNING + BACK + 60);
    assert_eq!(
        doings(&run, 1, MORNING + WAKE),
        [
            (0, idle(HALL, START, &[], None)),
            (
                DEPART - WAKE,
                LogPayload::LeftHome {
                    place: Duty::Work,
                    late_secs: 0
                }
            ),
            (BACK - WAKE, LogPayload::CameHome { place: Duty::Work }),
            (BACK - WAKE, empty(Need::Hunger)),
            (BACK - WAKE, empty(Need::Energy)),
            (BACK - WAKE, collapsed(HALL)),
            (BACK - WAKE, empty(Need::Bladder)),
            (BACK - WAKE, accident([0, 0, 1000, 0, 0])),
            (BACK - WAKE, empty(Need::Hygiene)),
            (BACK - WAKE, empty(Need::Fun)),
        ]
    );
    let home = entry_of(
        &run,
        1,
        MORNING + BACK,
        &LogPayload::CameHome { place: Duty::Work },
    );
    let first = entry_of(&run, 1, MORNING + BACK, &empty(Need::Hunger));
    assert!(caused_by(first, &home.id));
    assert_eq!(lying(&run), (true, [0, 0, 1000, 0, 0]));
    // Up after five hours.
    let run = script.run(MORNING + BACK + 18_000 - 1);
    assert!(lying(&run).0);
    let run = script.run(MORNING + BACK + 18_000);
    assert!(!lying(&run).0);
}

/// Coming home with hygiene at 0 and fun at 100 (jumps of −1 000 and −700, the rest 0): hygiene's
/// entry, fun from 100 to 0 by the change of −150 held at 0, so fun's entry too; then, at the
/// door and not lying, she chooses: both critical, the shower (2 s) before the bookshelf (3 s),
/// each a gain of 0.
#[test]
fn coming_home_dirty_takes_the_last_fun() {
    let tag = part_time();
    let jumps = day_with(|day| {
        day["return_jumps"] =
            json!({"hunger": 0, "energy": 0, "bladder": 0, "hygiene": -1000, "fun": -700});
    });
    let data = data(&rates([0; 5], [0, 100, 0, 0, 0], [0; 5]), &jumps, &kinds());
    let run = Script::on(data, &tag, 0).run(MORNING + BACK + 1);
    let at_door = [600, 900, 500, 0, 0];
    assert_eq!(
        doings(&run, 1, MORNING + BACK),
        [
            (0, LogPayload::CameHome { place: Duty::Work }),
            (0, empty(Need::Hygiene)),
            (0, empty(Need::Fun)),
            (0, started((SHOWER, 0), (HALL, 2), at_door, &[], false)),
        ]
    );
    let last = entry_of(&run, 1, MORNING + BACK, &empty(Need::Fun));
    let chose = entry_of(
        &run,
        1,
        MORNING + BACK,
        &started((SHOWER, 0), (HALL, 2), at_door, &[], false),
    );
    assert!(caused_by(chose, &last.id));
}

/// No crossing of the second is lost around a collapse. The toilet runs 12 hours (+1 an hour)
/// and the man is sent to it at 6:00:01; the shower is +480 an hour for 15 min. Her energy falls
/// 900 an hour, her bladder 450: energy at 700 and the bladder at 400 both after 800 s (each a
/// choice), energy 400 after 2 000 s, the bladder at 150 after 2 800 s, energy at 150 and 50
/// after 3 000 and 3 400 s; she waits for the toilet once the bladder is critical. At 3 600 s
/// energy is at 0 and the bladder at 50 in one second: energy first, she lies down in the hall;
/// the bladder's 50 still comes and writes its entry, but with energy still at 0 she cannot get
/// up (0 + 50 · 0 / 3 600). On the floor the bladder falls 40 an hour: 0 after 4 500 s, at 8 100
/// s, the accident (energy 62 by then, hygiene to 50). She gets up at 21 600 s with energy 250
/// and the bladder at 850 (1 000 − 40 · 13 500 / 3 600), and takes the shower.
#[test]
fn a_need_at_50_in_the_second_of_the_collapse_is_not_lost() {
    let kinds = with(kinds(), "core:toilet", offer("use", "bladder", 1, 43_200));
    let kinds = with(kinds, "core:shower", offer("shower", "hygiene", 480, 900));
    let rates = rates([0, -900, -450, 0, 0], [0, 100, -40, 0, 0], [0; 5]);
    let script =
        Script::saturday(data(&rates, &day_file(), &kinds)).input(SATURDAY, command(2, TOILET));
    let run = script.run(SATURDAY + 21_600);
    let both = |energy: u16, bladder: u16| [600, energy, bladder, 700, 800];
    let toilet = [TOILET];
    let waits = Some(TOILET);
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (800, idle(HALL, both(700, 400), &toilet, None)),
            (800, idle(HALL, both(700, 400), &toilet, None)),
            (2_000, idle(HALL, both(400, 250), &toilet, None)),
            (2_800, critical(Need::Bladder, 150)),
            (2_800, idle(HALL, both(200, 150), &toilet, waits)),
            (3_000, critical(Need::Energy, 150)),
            (3_000, idle(HALL, both(150, 125), &toilet, waits)),
            (3_400, critical(Need::Energy, 50)),
            (3_400, idle(HALL, both(50, 75), &toilet, waits)),
            (3_600, empty(Need::Energy)),
            (3_600, collapsed(HALL)),
            (3_600, critical(Need::Bladder, 50)),
            (8_100, empty(Need::Bladder)),
            (8_100, accident([600, 62, 1000, 50, 800])),
            (
                21_600,
                started(
                    (SHOWER, 0),
                    (HALL, 2),
                    [600, 250, 850, 50, 800],
                    &toilet,
                    false
                )
            ),
        ]
    );
}

/// No collapse is lost when another need of its second ends an action first. The bookshelf is
/// +150 an hour for an hour. Her hunger falls 450 an hour and her energy 900: energy at 700
/// after 800 s, hunger at 400 after 1 600 s, energy at 400, 150 and 50 after 2 000, 3 000 and 3
/// 400 s, each a choice and idle. Sent to the bookshelf at 3 401 s (hunger 175, energy 50), she
/// reads from 3 404 s. At 3 600 s hunger is at 150 and energy at 0: hunger first, the book ends
/// with reason `critical` (fun 808: 800 + 150 · 196 / 3 600) and she walks to the table for her
/// critical hunger (table and fridge tie at the bonus, the table is the smaller object, 3 s
/// from the living room); then energy's 0 still comes: the walk ends with reason `collapse` and
/// she lies down in the living room.
#[test]
fn an_action_ended_in_the_second_of_a_collapse_does_not_hide_it() {
    let kinds = with(kinds(), "test:book", offer("read", "fun", 150, 3_600));
    let rates = rates([-450, -900, 0, 0, 0], [0, 100, 0, 0, 0], [0; 5]);
    let script = Script::saturday(data(&rates, &day_file(), &kinds))
        .input(SATURDAY + 3_400, command(1, BOOK));
    let run = script.run(SATURDAY + 3_700);
    let at = |hunger: u16, energy: u16| [hunger, energy, 500, 700, 800];
    let after = [150, 0, 500, 700, 808];
    assert_eq!(
        doings(&run, 1, SATURDAY),
        [
            (0, idle(HALL, START, &[], None)),
            (800, idle(HALL, at(500, 700), &[], None)),
            (1_600, idle(HALL, at(400, 500), &[], None)),
            (2_000, idle(HALL, at(350, 400), &[], None)),
            (3_000, critical(Need::Energy, 150)),
            (3_000, idle(HALL, at(225, 150), &[], None)),
            (3_400, critical(Need::Energy, 50)),
            (3_400, idle(HALL, at(175, 50), &[], None)),
            (3_401, started((BOOK, 0), (HALL, 3), at(175, 50), &[], true)),
            (3_404, reached(BOOK)),
            (3_600, critical(Need::Hunger, 150)),
            (3_600, ended(BOOK, EndReason::Critical, after)),
            (3_600, started((TABLE, 0), (LIVING, 3), after, &[], false)),
            (3_600, empty(Need::Energy)),
            (3_600, ended(TABLE, EndReason::Collapse, after)),
            (3_600, collapsed(LIVING)),
        ]
    );
    assert!(lying(&run).0);
}

/// A departure in the second of a collapse waits. Tuesday, part time (7:40 to 13:20), the man
/// sent to the toilet of 12 hours at 6:00:01. Her energy falls 540 and her bladder 300 an hour:
/// both at 0 after 6 000 s, at 7:40, the second of her departure, which runs before the
/// crossings of that second. It settles them first: energy, she lies down in the hall; the
/// bladder, the accident on the floor (hygiene to 50); then the departure waits for her lying.
/// She gets up after five hours, at 12:40, and leaves 18 000 s late.
#[test]
fn a_departure_in_the_second_of_a_collapse_waits() {
    let tag = part_time();
    let kinds = with(kinds(), "core:toilet", offer("use", "bladder", 1, 43_200));
    let kinds = with(kinds, "core:shower", offer("shower", "hygiene", 480, 900));
    let rates = rates([0, -540, -300, 0, 0], [0, 100, 0, 0, 0], [0; 5]);
    let script =
        Script::on(data(&rates, &day_file(), &kinds), &tag, 0).input(MORNING, command(2, TOILET));
    let run = script.run(MORNING + DEPART);
    let her = &run.world.state().house().sims[&sim(1)];
    let due = Due {
        place: Duty::Work,
        depart: t(D + 27_600),
        back: t(D + 48_000),
    };
    assert_eq!(
        (her.collapsed, her.due, her.place),
        (true, Some(due), Place::Room(HALL))
    );
    let run = script.run(MORNING + DEPART + 18_000);
    assert_eq!(
        doings(&run, 1, MORNING + DEPART),
        [
            (0, empty(Need::Energy)),
            (0, collapsed(HALL)),
            (0, empty(Need::Bladder)),
            (0, accident([600, 0, 1000, 50, 800])),
            (
                18_000,
                LogPayload::LeftHome {
                    place: Duty::Work,
                    late_secs: 18_000
                }
            ),
        ]
    );
}

/// Getting up needs energy above 0 (0 + 50 an hour is 1 after 72 s): fallen in the night of an
/// adult (22:00 to 6:00) 72 s before its end, she gets up at 6:00 with energy 1; fallen 71 s
/// before it (energy 0.99 at 6:00, so 0), or 30 s, the night's end does not get her up, 250
/// does, 18 000 s after the fall.
#[test]
fn getting_up_needs_energy_above_0() {
    let rules: sim_core::house::data::DayRules =
        serde_json::from_str(&day_file()).expect("the day");
    let born = t(-30 * Y);
    let six = 2 * D + 6 * H;
    let lying = |fell: i64| InHouse {
        needs: [(600, 0), (0, 50), (500, 0), (700, 0), (800, 0)]
            .map(|(value, rate)| NeedSample::new(value, t(fell), rate)),
        place: Place::Room(HALL),
        action: None,
        due: None,
        decisions: Decisions { day: 1, count: 0 },
        collapsed: true,
    };
    assert_eq!(stand_up(&lying(six - 72), &rules, born), Some(t(six)));
    assert_eq!(
        stand_up(&lying(six - 71), &rules, born),
        Some(t(six - 71 + 18_000))
    );
    assert_eq!(
        stand_up(&lying(six - 30), &rules, born),
        Some(t(six - 30 + 18_000))
    );
    // In the middle of the night, 250 comes first, at 3:00 with a fall at 22:00.
    assert_eq!(
        stand_up(&lying(six - 8 * H), &rules, born),
        Some(t(six - 3 * H))
    );
}

/// Saved while she lies, in the seconds of each consequence and right after: each save loads,
/// saves to the same bytes and runs on as the uninterrupted world, with the same entries and
/// causes and the same state hash, inputs still to come given to both.
#[test]
fn saves_while_lying_and_after_each_consequence_run_on_the_same() {
    let tag = part_time();
    let accident_kinds = with(kinds(), "core:toilet", offer("use", "bladder", 1, 43_200));
    let accident_kinds = with(
        accident_kinds,
        "test:book",
        offer("read", "fun", 150, 3_600),
    );
    let accident_kinds = with(
        accident_kinds,
        "core:shower",
        offer("shower", "hygiene", 480, 900),
    );
    let sting_kinds = with(
        kinds(),
        "core:shower",
        offer("shower", "hygiene", 2_800, 43_200),
    );
    let sting_kinds = with(sting_kinds, "test:book", offer("read", "fun", 150, 3_600));
    let jumps = day_with(|day| {
        day["return_jumps"] = json!({"hunger": -1000, "energy": -1000, "bladder": -1000, "hygiene": -1000, "fun": -1000});
    });
    let cases: Vec<(Script, Vec<i64>, i64)> = vec![
        (
            Script::saturday(tiring()),
            vec![3_599, 3_600, 3_601, 12_600, 21_599, 21_600, 22_600],
            40_000,
        ),
        (
            Script::on(weary(&day_file()), &tag, 0),
            vec![3_240, WAKE, DEPART, 21_240, 21_245],
            BACK + 10,
        ),
        (
            Script::saturday(data(
                &rates([0, 0, -120, 0, 0], [0, 100, 0, 0, 0], [0; 5]),
                &day_file(),
                &accident_kinds,
            ))
            .input(SATURDAY, command(2, TOILET))
            .input(SATURDAY + 13_799, command(1, BOOK)),
            vec![13_500, 13_803, 15_000, 15_001],
            15_905,
        ),
        (
            Script::saturday(data(
                &rates([0, 0, 0, 0, -1_200], [0, 100, 0, 0, 0], [0; 5]),
                &day_file(),
                &fun_kinds(),
            ))
            .input(SATURDAY, command(2, TV)),
            vec![2_400, 2_704, 2_707],
            2_800,
        ),
        (
            Script::saturday(data(
                &rates([0; 5], [0, 100, 0, 0, 0], [0, 0, 0, -700, 0]),
                &day_file(),
                &sting_kinds,
            )),
            vec![3_600, 3_601],
            7_300,
        ),
        (
            Script::on(
                data(&rates([0; 5], [0, 100, 0, 0, 0], [0; 5]), &jumps, &kinds()),
                &tag,
                0,
            ),
            vec![BACK, BACK + 1],
            BACK + 18_100,
        ),
    ];
    for (case, (script, seconds, until)) in cases.into_iter().enumerate() {
        let origin = if script.day == 4 { SATURDAY } else { MORNING };
        let whole = script.run(origin + until);
        for after in seconds {
            let at = origin + after;
            let run = script.run(at);
            let bytes = run.world.save(&run.out.year_hashes).expect("saves");
            let (mut loaded, _) = World::load(&bytes, script.data.clone()).expect("loads");
            assert_eq!(
                loaded.save(&run.out.year_hashes).expect("saves"),
                bytes,
                "{case} {after}"
            );
            let mut out = YearOut::default();
            for (clock, input) in script.inputs.iter().filter(|(clock, _)| *clock >= at) {
                loaded.advance_until(&mut out, t(*clock), &mut |_| false);
                loaded.submit(input.clone()).expect("accepted");
            }
            loaded.advance_until(&mut out, t(origin + until), &mut |_| false);
            assert_eq!(
                loaded.state_hash(),
                whole.world.state_hash(),
                "{case} {after}"
            );
            let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
            assert_eq!(
                entries(&loaded.take_log()),
                entries(&whole.log[run.log.len()..]),
                "{case} {after}"
            );
        }
    }
}
