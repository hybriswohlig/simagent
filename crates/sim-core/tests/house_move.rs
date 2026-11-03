//! ADR-0024 sections 1 and 2 with its addendum of 2026-11-02: a household moves into its
//! house and lives there from its first morning at 6:00, moves out again, and its members come
//! and go. The expected values are the ADRs', by hand.

mod common;

use std::collections::BTreeMap;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::house::data::HouseData;
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Action, Decisions, InHouse, MovedIn, Place};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{Couple, Founder, Input, InputRejected, MoveIn, MoveOut, RejectCode, Start};
use sim_core::log::{Cause, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{DAY, HOUR, SimTime, YEAR, day_index};
use sim_core::world::{World, YearOut};

/// 6:00 of a day (ADR-0024 section 1).
const SIX: i64 = 6 * HOUR.as_secs();

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

/// The test data with the repository's houses: no disease, almost no death.
fn data() -> Data {
    common::death_data(1e-9, None, serde_json::json!([])).with_houses(common::repository_houses())
}

/// A woman of 30 and a man of 32 as a couple, and a single woman of 40.
fn start(singles: usize) -> Input {
    let founder = |years: i64, sex: Sex| Founder {
        born: SimTime::from_secs(-years * YEAR.as_secs()),
        sex,
        alleles: common::dummy(0, 0),
    };
    let mut founders = vec![founder(30, Sex::Female), founder(32, Sex::Male)];
    founders.extend((0..singles).map(|_| founder(40, Sex::Female)));
    Input::Start(Start {
        world_seed: 7,
        founder_tag: DataId::new("test:house").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders,
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    })
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn new(data: Data, singles: usize) -> Run {
        let mut world = World::new(data);
        world.submit(start(singles)).expect("accepted");
        let log = world.take_log();
        Run {
            world,
            out: YearOut::default(),
            log,
        }
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    fn submit(&mut self, input: Input) -> u64 {
        let stamp = self.world.submit(input).expect("accepted");
        self.log.extend(self.world.take_log());
        stamp.seq.get()
    }

    /// What a rejected input leaves: its code, and nothing changed.
    fn rejected(&mut self, input: Input) -> Option<RejectCode> {
        let before = self.world.state_hash();
        let inputs = self.world.input_log().len();
        let code = self
            .world
            .submit(input)
            .err()
            .map(|InputRejected { code }| code);
        assert_eq!(self.world.state_hash(), before);
        assert_eq!(self.world.input_log().len(), inputs);
        assert!(self.world.take_log().is_empty());
        code
    }

    /// Each living sim's household, after the log's entries in order.
    fn homes(&self) -> BTreeMap<SimId, HouseholdId> {
        let mut homes = BTreeMap::new();
        for entry in &self.log {
            if let LogPayload::HouseholdFormed { household } | LogPayload::Moved { household } =
                entry.payload
            {
                for Subject::Sim(sim) in &entry.subjects {
                    homes.insert(*sim, household);
                }
            }
        }
        homes
    }

    fn entries(&self, kind: fn(&LogPayload) -> bool) -> Vec<&LogEntry> {
        self.log
            .iter()
            .filter(|entry| kind(&entry.payload))
            .collect()
    }
}

fn move_in(household: HouseholdId) -> Input {
    Input::MoveIn(MoveIn { household })
}

fn move_out(household: HouseholdId) -> Input {
    Input::MoveOut(MoveOut { household })
}

fn day_started(payload: &LogPayload) -> bool {
    matches!(payload, LogPayload::DayStarted { .. })
}

/// An entry's id as the log writes it.
fn number(id: &sim_core::ids::EventId) -> u64 {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_u64())
        .expect("a number")
}

fn subjects(entry: &LogEntry) -> Vec<u64> {
    entry
        .subjects
        .iter()
        .map(|Subject::Sim(sim)| sim.get())
        .collect()
}

#[test]
fn move_in_starts_at_the_next_morning() {
    let mut run = Run::new(data(), 1);
    let home = run.homes()[&sim(1)];
    assert_eq!(run.homes()[&sim(2)], home);
    // Acting at 23:59:59 of day 3: the morning comes 6 h 1 s later.
    let late = 4 * DAY.as_secs() - 1;
    run.advance(late - 1);
    let seq = run.submit(move_in(home));
    let moved = run.log.last().expect("an entry");
    let flat = DataId::new("core:flat").expect("id");
    assert_eq!(
        moved.payload,
        LogPayload::MovedIn {
            household: home,
            house: flat.clone()
        }
    );
    assert_eq!((moved.t.as_secs(), subjects(moved)), (late, vec![1, 2]));
    assert!(matches!(moved.causes.as_slice(), [Cause::Input(input)] if input.get() == seq));
    let moved_id = number(&moved.id);
    let in_house = run.world.state().house().households[&home].clone();
    assert_eq!(
        in_house,
        MovedIn {
            house: flat,
            morning: false
        }
    );
    run.advance(late + SIX);
    assert!(run.entries(day_started).is_empty());
    run.advance(late + SIX + 1);
    let mornings = run.entries(day_started);
    assert_eq!(mornings.len(), 2);
    for (entry, id) in mornings.iter().zip([1, 2]) {
        assert_eq!(
            (entry.t.as_secs(), subjects(entry)),
            (late + SIX + 1, vec![id])
        );
        assert!(
            matches!(entry.causes.as_slice(), [Cause::Plan(plan)] if number(&plan.by) == moved_id)
        );
    }

    // Acting at 6:00:00 the morning is in that very second.
    let mut run = Run::new(data(), 1);
    let six = 2 * DAY.as_secs() + SIX;
    run.advance(six - 1);
    run.submit(move_in(home));
    run.advance(six);
    let mornings = run.entries(day_started);
    assert_eq!(mornings.len(), 2);
    assert!(mornings.iter().all(|entry| entry.t.as_secs() == six));
}

#[test]
fn move_in_and_out_reject_with_their_codes() {
    let mut never = World::new(data());
    let code = never.submit(move_in(HouseholdId::try_from(1).expect("valid")));
    assert_eq!(code.err().map(|r| r.code), Some(RejectCode::NotStarted));
    let mut run = Run::new(data(), 1);
    let homes = run.homes();
    let (home, single) = (homes[&sim(1)], homes[&sim(3)]);
    let unknown = HouseholdId::try_from(99).expect("valid");
    assert_eq!(
        run.rejected(move_in(unknown)),
        Some(RejectCode::HouseholdUnknown)
    );
    assert_eq!(
        run.rejected(move_out(unknown)),
        Some(RejectCode::HouseholdUnknown)
    );
    assert_eq!(run.rejected(move_out(home)), Some(RejectCode::NotInHouse));
    run.submit(move_in(home));
    assert_eq!(
        run.rejected(move_in(home)),
        Some(RejectCode::AlreadyInHouse)
    );
    assert_eq!(run.rejected(move_out(single)), Some(RejectCode::NotInHouse));
    // Only a house for one: the couple fits none, the single woman the one.
    let mut run = Run::new(data_with_one_place(), 1);
    assert_eq!(run.rejected(move_in(home)), Some(RejectCode::NoHouse));
    run.submit(move_in(single));
    // At most 64 households in houses: 65 single women and the couple.
    let mut run = Run::new(data(), 65);
    let homes = run.homes();
    let mut households: Vec<HouseholdId> = (3..=67).map(|id| homes[&sim(id)]).collect();
    households.dedup();
    assert_eq!(households.len(), 65);
    for household in &households[..64] {
        run.submit(move_in(*household));
    }
    assert_eq!(
        run.rejected(move_in(households[64])),
        Some(RejectCode::HouseLimit)
    );
}

/// The repository's houses but only the flat, for one person.
fn data_with_one_place() -> Data {
    let houses = common::repository_houses();
    let mut flat = houses.defs()[0].clone();
    assert_eq!(flat.id.as_str(), "core:flat");
    flat.capacity = 1;
    let (rates, day, kinds) = (houses.rates(), houses.day(), houses.kinds());
    let one =
        HouseData::new(rates.clone(), day.clone(), kinds.to_vec(), vec![flat]).expect("a house");
    common::death_data(1e-9, None, serde_json::json!([])).with_houses(one)
}

#[test]
fn the_first_morning_gives_the_needs() {
    let mut run = Run::new(data(), 1);
    let home = run.homes()[&sim(1)];
    run.advance(DAY.as_secs() - 1);
    run.submit(move_in(home));
    let morning = DAY.as_secs() + SIX;
    run.advance(morning);
    // Morning values with the awake rates of an adult (18 to 66), in the entrance; each
    // chooses at once and walks from there (she to the toilet in the bath, 3 s, he to the
    // table in the kitchen, 4 s; tests/house_day.rs has the choices by hand).
    let values = [600, 900, 500, 700, 800];
    for entry in run.entries(day_started) {
        assert_eq!(entry.payload, LogPayload::DayStarted { needs: values });
    }
    let at = SimTime::from_secs(morning);
    let walking = |to: u8, secs: i64, object: u16| InHouse {
        needs: [(600, -100), (900, -40), (500, -120), (700, -30), (800, -60)]
            .map(|(value, rate)| NeedSample::new(value, at, rate)),
        place: Place::Walking {
            from: 0,
            to,
            depart: at,
            arrive: SimTime::from_secs(morning + secs),
        },
        action: Some(Action {
            object,
            offer: 0,
            slot: 0,
        }),
        due: None,
        decisions: Decisions {
            day: day_index(at),
            count: 1,
        },
    };
    let house = run.world.state().house();
    assert!(house.households[&home].morning);
    assert_eq!(
        house.sims.keys().copied().collect::<Vec<_>>(),
        [sim(1), sim(2)]
    );
    assert_eq!(house.sims[&sim(1)], walking(3, 3, 5));
    assert_eq!(house.sims[&sim(2)], walking(1, 4, 0));
}

#[test]
fn moving_out_ends_the_house() {
    // Before the morning: no morning comes.
    let mut run = Run::new(data(), 1);
    let home = run.homes()[&sim(1)];
    run.advance(DAY.as_secs() - 1);
    run.submit(move_in(home));
    let seq = run.submit(move_out(home));
    let out = run.log.last().expect("an entry");
    assert_eq!(out.payload, LogPayload::MovedOut { household: home });
    assert_eq!(subjects(out), [1, 2]);
    assert!(matches!(out.causes.as_slice(), [Cause::Input(input)] if input.get() == seq));
    run.advance(3 * DAY.as_secs());
    assert!(run.entries(day_started).is_empty());
    assert!(run.world.state().house().is_empty());
    // After the morning: the needs go with them.
    let mut run = Run::new(data(), 1);
    run.advance(DAY.as_secs() - 1);
    run.submit(move_in(home));
    run.advance(2 * DAY.as_secs());
    assert_eq!(run.world.state().house().sims.len(), 2);
    run.submit(move_out(home));
    assert!(run.world.state().house().is_empty());
}

/// A save before the morning and one after it load, and the loaded world runs on as the
/// uninterrupted one.
#[test]
fn a_house_is_saved_and_loaded() {
    let morning = DAY.as_secs() + SIX;
    // Before the morning, with its crossings planned, in the second hunger reaches 400 (its
    // crossing just come), and after the last crossing.
    let points = [
        DAY.as_secs() + 60,
        morning + 60,
        morning + 2 * HOUR.as_secs(),
        3 * DAY.as_secs(),
    ];
    for save_at in points {
        let mut run = Run::new(data(), 1);
        let home = run.homes()[&sim(1)];
        run.advance(DAY.as_secs() - 1);
        run.submit(move_in(home));
        run.advance(save_at);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let (mut loaded, _) = World::load(&bytes, data()).expect("loads");
        assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
        let until = SimTime::from_secs(10 * DAY.as_secs());
        let mut out = YearOut::default();
        loaded.advance_until(&mut out, until, &mut |_| false);
        run.advance(until.as_secs());
        assert_eq!(loaded.state_hash(), run.world.state_hash(), "{save_at}");
    }
}

/// The first morning plans each member's crossings (ADR-0026 section 1): an adult at the
/// morning values has 17 thresholds below its needs; each then chooses and walks to it (the
/// toilet, the table). It plans their days too (ADR-0024 section 4): the series of plans from
/// 5:00 of tomorrow, and both working full time (their draws of `WORK_PLAN` 30 and 3, day 1 a
/// weekday) the wake, the departure and the return of today. Moving out cancels all of it.
#[test]
fn the_morning_plans_the_crossings() {
    let morning = DAY.as_secs() + SIX;
    let moved_in = || {
        let mut run = Run::new(data(), 1);
        let home = run.homes()[&sim(1)];
        run.advance(DAY.as_secs() - 1);
        run.submit(move_in(home));
        run.advance(morning - 1);
        let before = run.world.pending_events();
        (run, home, before)
    };
    let (mut run, _, before) = moved_in();
    run.advance(morning);
    // The morning is gone; two adults' crossings, days and walks have come.
    assert_eq!(run.world.pending_events(), before - 1 + 2 * (17 + 4) + 2);

    let (mut run, home, before) = moved_in();
    run.advance(morning + 60);
    run.submit(move_out(home));
    assert_eq!(run.world.pending_events(), before - 1);
}

/// The dynasty does not see the house (ADR-0024 section 3): the same run with and without
/// moving in has the same entries of phase 1 in the same order.
#[test]
fn move_in_does_not_touch_the_dynasty() {
    let lives = |moving: bool| {
        let mut run = Run::new(living_data(), 2);
        let home = run.homes()[&sim(1)];
        run.advance(DAY.as_secs() - 1);
        if moving {
            run.submit(move_in(home));
        }
        run.advance(30 * YEAR.as_secs());
        (run.log.into_iter())
            .filter(|entry| !house_entry(&entry.payload))
            .map(|entry| (entry.t, entry.subjects, entry.payload))
            .collect::<Vec<_>>()
    };
    let without = lives(false);
    assert!(without.len() > 100);
    assert_eq!(lives(true), without);
}

fn house_entry(payload: &LogPayload) -> bool {
    matches!(
        payload,
        LogPayload::MovedIn { .. }
            | LogPayload::MovedOut { .. }
            | LogPayload::DayStarted { .. }
            | LogPayload::ActionStarted { .. }
            | LogPayload::Reached { .. }
            | LogPayload::ActionEnded { .. }
            | LogPayload::Idle { .. }
            | LogPayload::NeedCritical { .. }
            | LogPayload::DayPlanned { .. }
            | LogPayload::LeftHome { .. }
            | LogPayload::CameHome { .. }
            | LogPayload::Missed { .. }
    )
}

/// Data in which people die (accidents) and couples have children.
fn living_data() -> Data {
    common::death_data(0.003, None, serde_json::json!([])).with_houses(common::repository_houses())
}

/// The house follows the household (ADR-0024 section 2): a child born into it moves in, a
/// member who dies or leaves home moves out with the entry that made it go as the cause;
/// every year end the save loads.
#[test]
fn members_come_and_go() {
    let mut run = Run::new(living_data(), 0);
    let home = run.homes()[&sim(1)];
    run.advance(DAY.as_secs() - 1);
    run.submit(move_in(home));
    for year in 0..60 {
        run.advance((year + 1) * YEAR.as_secs() - 1);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        assert!(World::load(&bytes, living_data()).is_ok(), "year {year}");
    }
    let by_id: BTreeMap<_, _> = run
        .log
        .iter()
        .map(|entry| (number(&entry.id), entry))
        .collect();
    let cause = |entry: &LogEntry| match entry.causes.as_slice() {
        [Cause::Event(id), ..] => Some(&by_id[&number(id)].payload),
        _ => None,
    };
    let came: Vec<_> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::MovedIn { .. }))
        .skip(1)
        .collect();
    assert!(!came.is_empty(), "a child is born into the house");
    for entry in &came {
        assert_eq!(subjects(entry).len(), 1);
        assert!(
            matches!(cause(entry), Some(LogPayload::Moved { household }) if household == &home)
        );
    }
    let gone: Vec<_> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::MovedOut { .. }))
        .collect();
    let died = gone
        .iter()
        .filter(|e| matches!(cause(e), Some(LogPayload::Death { .. })));
    let left = gone
        .iter()
        .filter(|e| matches!(cause(e), Some(LogPayload::Moved { .. })));
    assert!(died.count() > 0, "a member dies");
    assert!(left.count() > 0, "a child leaves home");
    assert!(gone.iter().all(|entry| subjects(entry).len() == 1));
}
