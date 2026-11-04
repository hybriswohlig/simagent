//! Commands (ADR-0024 section 5, ADR-0026 section 4): `Input::Command` tells a person with
//! needs at home to take an offer of an object now. It is checked before it acts and rejected
//! with its code, changing nothing; otherwise what the person does ends with reason `command`,
//! the rates having worked until then, and the commanded walk begins in the same second with
//! `commanded: true`, both caused by the input; from there it goes on like any other. A place
//! held for the commanded person counts as free for it, one held for another as taken. The
//! couple of `common::day` in a test house; the values by hand.

mod common;

use common::day::{self, MORNING, Run};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::house::data::Need;
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Action, Place};
use sim_core::ids::SimId;
use sim_core::input::{Command, Couple, Founder, Input, InputRejected, MoveOut, RejectCode, Start};
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// A television (fun +1 200 an hour for 2 h) and a bed, two places each; a pantry of one
/// place that fills hunger at +6 000 an hour until full (at most 10 min); toys for one, only
/// from 6 to 12 (fun +600 an hour for 30 min).
const KINDS: &str = r#"
  {"id": "test:tv", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 1200}, "seconds": 7200, "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "until_full": "energy", "max_seconds": 43200, "min_age": 0, "max_age": 130}]},
  {"id": "test:pantry", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 6000}, "until_full": "hunger", "max_seconds": 600, "min_age": 0, "max_age": 130}]},
  {"id": "test:toys", "slots": 1, "offers": [
    {"id": "play", "rates": {"fun": 600}, "seconds": 1800, "min_age": 6, "max_age": 12}]}"#;

/// Rooms hall 0 (the entrance), kitchen 1, den 2, bedroom 3, each 3 s from the hall, so 6 s
/// from each other. Objects: the pantry 0 in the kitchen, the television 1 in the den, the bed
/// 2 in the bedroom, the toys 3 in the den, the five essential ones 4 to 8 in the hall.
const HOUSE: &str = r#"{
  "id": "test:command", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "kitchen"}, {"id": "den"}, {"id": "bedroom"}],
  "child_day_room": "den", "child_night_room": "bedroom",
  "doors": [
    {"a": "hall", "b": "kitchen", "seconds": 3},
    {"a": "hall", "b": "den", "seconds": 3},
    {"a": "hall", "b": "bedroom", "seconds": 3}],
  "objects": [{"kind": "test:pantry", "room": "kitchen"}, {"kind": "test:tv", "room": "den"},
    {"kind": "test:bed", "room": "bedroom"}, {"kind": "test:toys", "room": "den"}, ESSENTIALS]
}"#;

const PANTRY: u16 = 0;
const TV: u16 = 1;
const BED: u16 = 2;
const TOYS: u16 = 3;

/// Hunger falls 1 200 an hour awake and asleep, fun 600 awake; energy stands awake and rises
/// 1 200 an hour asleep.
fn data() -> Data {
    day::house_data(&day::rates(-1200, 0, -600), KINDS, HOUSE)
}

const MORNING_NEEDS: [u16; 5] = [600, 900, 500, 700, 800];

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn command(id: u64, object: u16, offer: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer,
    })
}

/// A start of offer 0 of `object` on `slot`, walked from `room` in `walk` seconds.
fn started(
    (object, slot): (u16, u8),
    room: u8,
    walk: u32,
    needs: [u16; 5],
    busy: &[u16],
    commanded: bool,
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new("test:command").expect("id"),
        object,
        offer: 0,
        slot,
        room,
        walk_secs: walk,
        needs,
        busy: busy.to_vec(),
        commanded,
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

/// He idles in the hall at 6:00, waiting for the pantry she walks to.
fn waits_for_the_pantry() -> LogPayload {
    LogPayload::Idle {
        house: DataId::new("test:command").expect("id"),
        room: 0,
        needs: MORNING_NEEDS,
        busy: vec![PANTRY],
        waits: Some(PANTRY),
    }
}

/// The couple of `common::day` started on `data`, its household never moved in.
fn started_world(data: Data) -> World {
    let founder = |years: i64, sex: Sex| Founder {
        born: t(-years * YEAR.as_secs()),
        sex,
        alleles: common::dummy(0, 0),
    };
    let mut world = World::new(data);
    let start = Start {
        world_seed: 7,
        founder_tag: DataId::new("test:house").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![founder(30, Sex::Female), founder(32, Sex::Male)],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    };
    world.submit(Input::Start(start)).expect("accepted");
    world.take_log();
    world
}

/// What a rejected input leaves: its code, and nothing changed (ADR-0004 section 4).
fn rejected(world: &mut World, input: Input) -> Option<RejectCode> {
    let before = world.state_hash();
    let inputs = world.input_log().len();
    let code = (world.submit(input).err()).map(|InputRejected { code }| code);
    assert_eq!(world.state_hash(), before);
    assert_eq!(world.input_log().len(), inputs);
    assert!(world.take_log().is_empty());
    code
}

/// An accepted command: its position in the input log and the entries it wrote.
fn obey(run: &mut Run, input: Input) -> (u64, Vec<LogEntry>) {
    let stamp = run.world.submit(input).expect("accepted");
    let made = run.world.take_log();
    run.log.extend(made.iter().cloned());
    (stamp.seq.get(), made)
}

/// An entry of the person `id` at `second` after the morning, caused by the input `seq`.
fn by_input(entry: &LogEntry, id: u64, second: i64, seq: u64) -> LogPayload {
    assert_eq!(entry.t, t(MORNING + second));
    assert_eq!(entry.subjects, [Subject::Sim(sim(id))]);
    assert!(
        matches!(entry.causes.as_slice(), [Cause::Input(input)] if input.get() == seq),
        "{:?}",
        entry.causes
    );
    entry.payload.clone()
}

/// Every code with a state that gives exactly it, and the effect. At 6:00 she walks to the
/// pantry (400 · 400 − 50 · 3 = 159 850), he finds it taken, nothing free brings enough (the
/// television 200 · 200 − 150 = 39 850) and he waits for it (`tests/house_waiting.rs`). Told to
/// watch television she stops on the way in the second the command acts, 6:00:01, with the
/// morning values (1 s at −1 200 an hour truncates to 0), and walks from the hall; the pantry,
/// freed, is held for him, so for her it is taken. Told to eat he takes the place held for him.
/// Both are there at 6:00:04: he with hunger 599 (600 − 1.3), full after 241 s (+401.7) at
/// 6:04:05 with fun 760 (800 − 40.8), when he takes the second place of the television from the
/// kitchen (240 · 240 − 50 · 6 = 57 300). Her hunger falls from 600 at 6:00 and reaches 150 at
/// 6:22:30 (3 600 · 450 / 1 200 s), which the television does not raise: it ends with fun
/// 1 000 (800 + 448.7 since 6:00:04, capped) and she walks from the den to the pantry for her
/// critical hunger (10⁹ + 850 · 850 − 50 · 6). A command then is refused, and so is any once
/// the household has moved out. Not here: `end_of_time`, for the second the command would act
/// in and for a walk whose arrival lies beyond the last second. No world reaches the last
/// second (a million years, ADR-0023 section 1), and no save there loads: the loader wants a
/// year hash for every year and a person with needs is at most 130.
#[test]
fn command_rejections_and_effect() {
    let mut never = World::new(data());
    assert_eq!(
        rejected(&mut never, command(1, TV, 0)),
        Some(RejectCode::NotStarted)
    );
    let mut outside = started_world(data());
    assert_eq!(
        rejected(&mut outside, command(1, TV, 0)),
        Some(RejectCode::NotInHouse)
    );
    assert_eq!(
        rejected(&mut outside, command(99, TV, 0)),
        Some(RejectCode::UnknownSim)
    );
    // In a world where people die (an accident rate of 0.5 a year), one who died.
    let mut deadly = started_world(common::death_data(0.5, None, json!([])));
    deadly.advance_until(&mut YearOut::default(), t(20 * YEAR.as_secs()), &mut |_| {
        false
    });
    let died = (deadly.take_log().into_iter())
        .find(|entry| matches!(entry.payload, LogPayload::Death { .. }))
        .and_then(|entry| entry.subjects.first().copied())
        .expect("a death");
    let Subject::Sim(died) = died;
    assert_eq!(
        rejected(&mut deadly, command(died.get(), TV, 0)),
        Some(RejectCode::SimNotAlive)
    );

    // Moved in at 23:59:59, but without needs before the first morning.
    let (mut run, home) = Run::moved_in(data());
    assert_eq!(
        rejected(&mut run.world, command(1, TV, 0)),
        Some(RejectCode::NotInHouse)
    );
    run.advance(MORNING);
    assert_eq!(
        run.doings(1),
        [(0, started((PANTRY, 0), 0, 3, MORNING_NEEDS, &[], false))]
    );
    assert_eq!(run.doings(2), [(0, waits_for_the_pantry())]);
    for (input, code) in [
        (command(1, 9, 0), RejectCode::ObjectUnknown),
        (command(1, PANTRY, 1), RejectCode::OfferUnknown),
        (command(1, TOYS, 0), RejectCode::WrongAge),
        // Her walk takes the pantry's only place.
        (command(2, PANTRY, 0), RejectCode::SlotBusy),
        // Her own place frees when her walk ends, and goes to him, who waits.
        (command(1, PANTRY, 0), RejectCode::SlotBusy),
    ] {
        assert_eq!(rejected(&mut run.world, input), Some(code));
    }

    let (hers, made) = obey(&mut run, command(1, TV, 0));
    assert_eq!(made.len(), 2);
    assert_eq!(
        by_input(&made[0], 1, 1, hers),
        ended(PANTRY, EndReason::Command, MORNING_NEEDS)
    );
    assert_eq!(
        by_input(&made[1], 1, 1, hers),
        started((TV, 0), 0, 3, MORNING_NEEDS, &[PANTRY], true)
    );
    let house = run.world.state().house();
    let her = &house.sims[&sim(1)];
    assert_eq!(
        (her.place, her.action),
        (
            Place::Walking {
                from: 0,
                to: 2,
                depart: t(MORNING + 1),
                arrive: t(MORNING + 4),
            },
            Some(Action {
                object: TV,
                offer: 0,
                slot: 0,
            })
        )
    );
    assert!(house.waits.is_empty());
    assert_eq!(
        house.holds.iter().collect::<Vec<_>>(),
        [(&(home, PANTRY, 0), &sim(2))]
    );
    // Held for him, the pantry is taken for her.
    assert_eq!(
        rejected(&mut run.world, command(1, PANTRY, 0)),
        Some(RejectCode::SlotBusy)
    );
    let (his, made) = obey(&mut run, command(2, PANTRY, 0));
    assert_eq!(made.len(), 1);
    assert_eq!(
        by_input(&made[0], 2, 1, his),
        started((PANTRY, 0), 0, 3, MORNING_NEEDS, &[], true)
    );
    let house = run.world.state().house();
    assert!(house.waits.is_empty() && house.holds.is_empty());

    run.advance(MORNING + 1_350);
    let critical = [150, 900, 500, 700, 1000];
    assert_eq!(
        run.doings(1),
        [
            (0, started((PANTRY, 0), 0, 3, MORNING_NEEDS, &[], false)),
            (1, ended(PANTRY, EndReason::Command, MORNING_NEEDS)),
            (1, started((TV, 0), 0, 3, MORNING_NEEDS, &[PANTRY], true)),
            (4, LogPayload::Reached { object: TV }),
            (
                1_350,
                LogPayload::NeedCritical {
                    need: Need::Hunger,
                    level: 150
                }
            ),
            (1_350, ended(TV, EndReason::Critical, critical)),
            (1_350, started((PANTRY, 0), 2, 6, critical, &[], false)),
        ]
    );
    let full = [1000, 900, 500, 700, 760];
    assert_eq!(
        run.doings(2),
        [
            (0, waits_for_the_pantry()),
            (1, started((PANTRY, 0), 0, 3, MORNING_NEEDS, &[], true)),
            (4, LogPayload::Reached { object: PANTRY }),
            (245, ended(PANTRY, EndReason::Full, full)),
            (245, started((TV, 1), 1, 6, full, &[], false)),
            (251, LogPayload::Reached { object: TV }),
        ]
    );
    // On her way to the pantry for her critical hunger (150 at 6:22:31 too).
    assert_eq!(
        rejected(&mut run.world, command(1, TV, 0)),
        Some(RejectCode::CriticalNeed)
    );
    run.submit(Input::MoveOut(MoveOut { household: home }));
    assert_eq!(
        rejected(&mut run.world, command(1, TV, 0)),
        Some(RejectCode::NotInHouse)
    );
}

/// A command while idle and while asleep. He waits for the pantry at 6:00 (as above); told to
/// sleep he leaves the waiting list and walks from the hall to the bed, there at 6:00:04 with
/// hunger 599 (600 − 1.3) and every need at the sleeping rates. Told at 6:01:40 to watch
/// television he wakes in the second it acts, 6:01:41: hunger 567 (599 − 32.3), energy 932
/// (900 + 32.3), the rest as in the morning, each written down then at its awake rate; he walks
/// from the bedroom to the den, 6 s, while she still eats at the pantry (hers until full at
/// 6:04:04).
#[test]
fn a_command_while_idle_and_while_asleep() {
    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING);
    let (to_bed, made) = obey(&mut run, command(2, BED, 0));
    assert_eq!(made.len(), 1);
    assert_eq!(
        by_input(&made[0], 2, 1, to_bed),
        started((BED, 0), 0, 3, MORNING_NEEDS, &[PANTRY], true)
    );
    let house = run.world.state().house();
    assert!(house.waits.is_empty() && house.holds.is_empty());
    run.advance(MORNING + 100);
    let (to_tv, made) = obey(&mut run, command(2, TV, 0));
    let woken = [567, 932, 500, 700, 800];
    assert_eq!(made.len(), 2);
    assert_eq!(
        by_input(&made[0], 2, 101, to_tv),
        ended(BED, EndReason::Command, woken)
    );
    assert_eq!(
        by_input(&made[1], 2, 101, to_tv),
        started((TV, 0), 3, 6, woken, &[PANTRY], true)
    );
    let him = &run.world.state().house().sims[&sim(2)];
    let at = t(MORNING + 101);
    let awake = [-1200, 0, 0, 0, -600];
    let needs: [NeedSample; 5] =
        core::array::from_fn(|need| NeedSample::new(woken[need], at, awake[need]));
    assert_eq!(him.needs, needs);
    assert_eq!(
        him.place,
        Place::Walking {
            from: 3,
            to: 2,
            depart: at,
            arrive: t(MORNING + 107),
        }
    );
    run.advance(MORNING + 107);
    assert_eq!(
        run.doings(2),
        [
            (0, waits_for_the_pantry()),
            (1, started((BED, 0), 0, 3, MORNING_NEEDS, &[PANTRY], true)),
            (4, LogPayload::Reached { object: BED }),
            (101, ended(BED, EndReason::Command, woken)),
            (101, started((TV, 0), 3, 6, woken, &[PANTRY], true)),
            (107, LogPayload::Reached { object: TV }),
        ]
    );
}

/// A feast for one (hunger +7 an hour for 10 h), a gruel for one (+180 for 20 min), a cot for
/// two and a small loo (bladder +720 for 5 min): `tests/house_rhythm.rs`.
const FAR_KINDS: &str = r#"
  {"id": "test:feast", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 7}, "seconds": 36000, "min_age": 0, "max_age": 130}]},
  {"id": "test:gruel", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 180}, "seconds": 1200, "min_age": 0, "max_age": 130}]},
  {"id": "test:cot", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "until_full": "energy", "max_seconds": 43200, "min_age": 0, "max_age": 130}]},
  {"id": "test:loo", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 720}, "seconds": 300, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance), a 1, b 2, den 3, each 110 s from the next. Objects: feast 0 and
/// gruel 1 in the den, cot 2 in a, loo 3 in the hall, the five essential ones 4 to 8 in the hall.
const FAR: &str = r#"{
  "id": "test:far", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "a"}, {"id": "b"}, {"id": "den"}],
  "child_day_room": "hall", "child_night_room": "a",
  "doors": [
    {"a": "hall", "b": "a", "seconds": 110},
    {"a": "a", "b": "b", "seconds": 110},
    {"a": "b", "b": "den", "seconds": 110}],
  "objects": [{"kind": "test:feast", "room": "den"}, {"kind": "test:gruel", "room": "den"},
    {"kind": "test:cot", "room": "a"}, {"kind": "test:loo", "room": "hall"}, ESSENTIALS]
}"#;

/// Hunger falls 173 an hour awake, everything else stands still awake; asleep energy rises 60.
fn far() -> Data {
    let bands = |rate: i32| [rate; 4];
    let rates = json!({
        "awake": {"hunger": bands(-173), "energy": bands(0), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(60), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)}
    });
    day::house_data(&rates.to_string(), FAR_KINDS, FAR)
}

/// The departure stays first (ADR-0024 section 5): in the far house both work full time and at
/// 8:40 walk to the feast and the gruel for their critical hunger, so their departures wait
/// (`tests/house_rhythm.rs`); told to use the loo they are refused as away. He leaves at
/// 9:01:35 and walks out for 335 s: on the way and once away he is refused the same, and so is
/// she, still waiting at the feast.
#[test]
fn a_command_for_one_away_or_due() {
    const LOO: u16 = 3;
    let (mut run, _) = Run::moved_in(far());
    run.advance(MORNING + 9_630);
    for id in [1, 2] {
        assert!(
            run.world.state().house().sims[&sim(id)].due.is_some(),
            "{id}"
        );
        assert_eq!(
            rejected(&mut run.world, command(id, LOO, 0)),
            Some(RejectCode::Away)
        );
    }
    run.advance(MORNING + 11_000);
    let him = &run.world.state().house().sims[&sim(2)];
    assert!(matches!(him.place, Place::Walking { .. }) && him.action.is_none());
    assert_eq!(
        rejected(&mut run.world, command(2, LOO, 0)),
        Some(RejectCode::Away)
    );
    run.advance(MORNING + 12_000);
    assert_eq!(run.world.state().house().sims[&sim(2)].place, Place::Away);
    for id in [1, 2] {
        assert_eq!(
            rejected(&mut run.world, command(id, LOO, 0)),
            Some(RejectCode::Away)
        );
    }
}

/// The world `run` saved now: it loads and saves to the same bytes.
fn reloaded(run: &Run) -> World {
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (loaded, _) = World::load(&bytes, data()).expect("loads");
    assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
    loaded
}

/// A world replayed from the input log of `run` to its clock (ADR-0004 section 4): the same
/// stamps, the same entries from the start and the same state hash.
fn replays(run: &Run) {
    let mut replayed = World::new(data());
    let mut out = YearOut::default();
    let mut log = Vec::new();
    for record in run.world.input_log().to_vec() {
        replayed.advance_until(&mut out, record.t, &mut |_| false);
        let stamp = replayed.submit(record.input).expect("accepted");
        assert_eq!((stamp.t, stamp.seq), (record.t, record.seq));
        log.extend(replayed.take_log());
    }
    replayed.advance_until(&mut out, run.world.closed_through(), &mut |_| false);
    log.extend(replayed.take_log());
    assert_eq!(replayed.state_hash(), run.world.state_hash());
    let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
    assert_eq!(entries(&log), entries(&run.log));
}

/// `loaded` and `run`, given `then` both, run on until `until` after the morning as one: the
/// same state hash and the same entries.
fn runs_on(mut loaded: World, run: &mut Run, then: Option<Input>, until: i64) {
    let from = run.log.len();
    if let Some(input) = then {
        loaded.submit(input.clone()).expect("accepted");
        run.submit(input);
    }
    loaded.advance_until(&mut YearOut::default(), t(MORNING + until), &mut |_| false);
    run.advance(MORNING + until);
    assert_eq!(loaded.state_hash(), run.world.state_hash());
    let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
    assert_eq!(entries(&loaded.take_log()), entries(&run.log[from..]));
}

/// A command is an input like the others: saved right after it the world loads and runs on as
/// the uninterrupted one, an hour after the morning. After her command of the first test his
/// choice is due in the next second for the place held for him: on load that choice is pending
/// again, and his own command cancels it as before. After both commands, and after waking him
/// to watch television. A run replayed from its input log is the same, entry for entry.
#[test]
fn a_command_is_saved_loaded_and_replayed() {
    let until = HOUR.as_secs();
    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING);
    obey(&mut run, command(1, TV, 0));
    assert_eq!(run.world.state().house().holds.len(), 1);
    runs_on(reloaded(&run), &mut run, None, until);

    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING);
    obey(&mut run, command(1, TV, 0));
    runs_on(reloaded(&run), &mut run, Some(command(2, PANTRY, 0)), until);
    assert_eq!(run.world.input_log().len(), 4);
    replays(&run);

    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING);
    obey(&mut run, command(2, BED, 0));
    run.advance(MORNING + 100);
    obey(&mut run, command(2, TV, 0));
    runs_on(reloaded(&run), &mut run, None, until);
}

/// The couple after both commands of the first test: she watches television from 6:00:04 on
/// its first place, her hunger falling from 600 at 6:00 at −1 200 an hour (400 at 6:10:00, 150
/// at 6:22:30), fun rising from 800 at +1 200; he eats at the pantry, then from 6:04:11 watches
/// on the second place.
fn both_commanded() -> Run {
    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING);
    obey(&mut run, command(1, TV, 0));
    obey(&mut run, command(2, PANTRY, 0));
    run
}

/// A command acts before the events of its second (ADR-0004 section 4), so a crossing in that
/// second still comes after it: told at 6:09:59 to sleep, she stops watching at 6:10:00 with
/// hunger 400 (600 − 200), fun 998 (800 + 198.7) and walks from the den to the bed, 6 s; her
/// hunger, which watching never rewrote, crosses 400 in that second, after the command, and
/// does nothing on the way. The crossing stays planned: a save right after the command loads,
/// saves to the same bytes and runs on as the uninterrupted world, and a replay is the same.
#[test]
fn a_crossing_in_the_second_of_a_command_stays_planned() {
    let mut run = both_commanded();
    run.advance(MORNING + 599);
    let (seq, made) = obey(&mut run, command(1, BED, 0));
    let watched = [400, 900, 500, 700, 998];
    assert_eq!(made.len(), 2);
    assert_eq!(
        by_input(&made[0], 1, 600, seq),
        ended(TV, EndReason::Command, watched)
    );
    assert_eq!(
        by_input(&made[1], 1, 600, seq),
        started((BED, 0), 2, 6, watched, &[], true)
    );
    runs_on(reloaded(&run), &mut run, None, HOUR.as_secs());
    assert_eq!(
        run.doings(1)[4..7],
        [
            (600, ended(TV, EndReason::Command, watched)),
            (600, started((BED, 0), 2, 6, watched, &[], true)),
            (606, LogPayload::Reached { object: BED }),
        ]
    );
    replays(&run);
}

/// The same for a critical need: told at 6:22:29 to sleep, she stops watching at 6:22:30 with
/// hunger 150 (600 − 450) and fun 1 000 (800 + 448.7, capped), and walks from the den to the
/// bed; in that second, after the command, her hunger crosses 150, which sleep does not raise
/// (−1 200 an hour asleep): the walk ends on the spot and she walks to the pantry for her
/// critical hunger (10⁹ + 850 · 850 − 50 · 6), there at 6:22:36. The critical entry is planned
/// by the end the command wrote. A save right after the command loads and runs on alike, and a
/// replay is the same.
#[test]
fn a_critical_crossing_in_the_second_of_a_command_cuts_its_walk_short() {
    let mut run = both_commanded();
    run.advance(MORNING + 1_349);
    let (seq, made) = obey(&mut run, command(1, BED, 0));
    let critical = [150, 900, 500, 700, 1000];
    assert_eq!(made.len(), 2);
    assert_eq!(
        by_input(&made[0], 1, 1_350, seq),
        ended(TV, EndReason::Command, critical)
    );
    let commanded_end = day::number(&made[0].id);
    runs_on(reloaded(&run), &mut run, None, HOUR.as_secs());
    assert_eq!(
        run.doings(1)[4..10],
        [
            (1_350, ended(TV, EndReason::Command, critical)),
            (1_350, started((BED, 0), 2, 6, critical, &[], true)),
            (
                1_350,
                LogPayload::NeedCritical {
                    need: Need::Hunger,
                    level: 150
                }
            ),
            (1_350, ended(BED, EndReason::Critical, critical)),
            (1_350, started((PANTRY, 0), 2, 6, critical, &[], false)),
            (1_356, LogPayload::Reached { object: PANTRY }),
        ]
    );
    let entry = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::NeedCritical { .. }))
        .expect("the critical entry");
    assert!(
        matches!(entry.causes.as_slice(), [Cause::Plan(plan)] if day::number(&plan.by) == commanded_end),
        "{:?}",
        entry.causes
    );
    replays(&run);
}
