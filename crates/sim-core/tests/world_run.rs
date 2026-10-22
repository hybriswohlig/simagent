//! The event path of the world (ADR-0002, ADR-0004): `advance_until` runs the yearly
//! dummy birthdays and the year end observer.
//!
//! Scenario as in `world_start.rs`: three founders born 30 years (sim 1), 25 years and
//! 100 days (sim 2) and one second (sim 3) before the start. YEAR = 31 449 600 s.
//! By hand from the ADRs:
//! - `Start` writes entries 1 to 7 at t = 0: `core:started`, the three founders and their
//!   three households (ADR-0018 section 4; sim 3 leaves home only after the ten years).
//! - The first birthday is the first anniversary at or after t = 0: sim 1 at 0 (age 30),
//!   sim 2 at 22 809 600 (day 264, age 26), sim 3 at YEAR - 1 (age 1). Each birthday
//!   plans the next one a year later, with its own entry as `by`.
//! - In year k: sim 1 is entry 8 + 3k, sim 2 entry 9 + 3k, sim 3 entry 10 + 3k.
//! - The year end of year k runs at (k + 1) * YEAR - 1, after sim 3's birthday in the
//!   same second, and counts 3 living sims with 3 * YEAR lived seconds.

mod common;

use sim_core::body_contract::BodyCensus;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{Founder, Input, RejectCode, Start};
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{AdvanceOutcome, EventClass, SimDuration, SimTime, YEAR, year_start};
use sim_core::world::{
    CensusRow, EventPayload, HeightMean, InheritanceCensus, PopulationCensus, World, YearOut,
};

const Y: i64 = 31_449_600;
const _: () = assert!(Y == YEAR.as_secs());
const BORN: [i64; 3] = [-943_488_000, -794_880_000, -1];
/// The last second of year 9.
const END: i64 = 314_495_999;

fn start_with(born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: 1,
        founder_tag: DataId::new("test:world_run").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: born.iter().copied().map(founder).collect(),
        couples: Vec::new(),
        capacity: None,
        pool: None,
    })
}

/// A founder born at `secs`: female, with the alleles wild and dummy.
fn founder(secs: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(secs),
        sex: Sex::Female,
        alleles: common::dummy(0, 1),
    }
}

fn started(born: &[i64]) -> World {
    let mut world = World::new(common::data());
    world.submit(start_with(born)).expect("accepted");
    world
}

fn lines(log: &[LogEntry]) -> Vec<String> {
    log.iter()
        .map(|entry| serde_json::to_string(entry).expect("json"))
        .collect()
}

/// The birthday entry of `sim` (1 to 3) in year `k`, from the rules in the header.
fn birthday_line(k: i64, sim: i64) -> String {
    let id = 7 + 3 * k + sim;
    let (t, age) = match sim {
        1 => (k * Y, 30 + k),
        2 => (k * Y + 22_809_600, 26 + k),
        _ => ((k + 1) * Y - 1, 1 + k),
    };
    let (by, planned_at) = if k == 0 {
        (1 + sim, 0)
    } else {
        (id - 3, t - Y)
    };
    format!(
        r#"{{"id":{id},"t":{t},"subjects":[{{"sim":{sim}}}],"causes":[{{"plan":{{"by":{by},"planned_at":{planned_at}}}}}],"payload":{{"core:birthday":{{"age_years":{age}}}}}}}"#
    )
}

fn expected_birthdays() -> Vec<String> {
    (0..10)
        .flat_map(|k| (1..=3).map(move |sim| birthday_line(k, sim)))
        .collect()
}

/// Nobody born, arrived, gone or dead: founders of `Start` are no arrivals, and the
/// repository's data has seven causes of death (ADR-0021, addendum 2026-10-21).
fn unmoved() -> PopulationCensus {
    PopulationCensus {
        births: 0,
        arrivals: 0,
        emigrations: 0,
        deaths: vec![0; 7],
    }
}

/// The switch off, the six diseases of the repository's data and nobody affected, no body
/// measured (the test data has no body traits), nobody born (ADR-0021, addendum
/// 2026-10-21).
fn unaffected() -> InheritanceCensus {
    InheritanceCensus {
        diseases_on: false,
        affected: vec![0; 6],
        adults: [HeightMean::default(); 2],
        age25: [HeightMean::default(); 2],
        inbreeding_e4: 0,
    }
}

fn expected_census() -> Vec<CensusRow> {
    (0..10)
        .map(|year| CensusRow {
            year,
            living: 3,
            lived_secs: 94_348_800,
            body: BodyCensus::default(),
            population: unmoved(),
            inheritance: unaffected(),
        })
        .collect()
}

/// Runs to `END` in steps of `width` seconds and collects the log after every call.
fn run_in_steps(world: &mut World, width: i64) -> (Vec<String>, YearOut) {
    let mut out = YearOut::default();
    let mut log = lines(&world.take_log());
    while world.closed_through().as_secs() < END {
        let limit = (world.closed_through().as_secs() + width).min(END);
        let outcome = world.advance_until(&mut out, SimTime::from_secs(limit), &mut |_| false);
        assert_eq!(outcome, AdvanceOutcome::ReachedLimit);
        log.extend(lines(&world.take_log()));
    }
    (log, out)
}

#[test]
fn ten_years_of_birthdays_are_pinned() {
    let mut world = started(&BORN);
    let start_log = world.take_log();
    assert_eq!(start_log.len(), 7);
    let mut out = YearOut::default();
    let outcome = world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| false);
    assert_eq!(outcome, AdvanceOutcome::ReachedLimit);
    assert_eq!(world.closed_through(), SimTime::from_secs(END));
    let log = lines(&world.take_log());
    assert_eq!(log, expected_birthdays());
    // Spot lines written out, so the formula above is not the only witness.
    let spots = [
        (
            0,
            r#"{"id":8,"t":0,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":30}}}"#,
        ),
        (
            1,
            r#"{"id":9,"t":22809600,"subjects":[{"sim":2}],"causes":[{"plan":{"by":3,"planned_at":0}}],"payload":{"core:birthday":{"age_years":26}}}"#,
        ),
        (
            2,
            r#"{"id":10,"t":31449599,"subjects":[{"sim":3}],"causes":[{"plan":{"by":4,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
        ),
        (
            3,
            r#"{"id":11,"t":31449600,"subjects":[{"sim":1}],"causes":[{"plan":{"by":8,"planned_at":0}}],"payload":{"core:birthday":{"age_years":31}}}"#,
        ),
        (
            29,
            r#"{"id":37,"t":314495999,"subjects":[{"sim":3}],"causes":[{"plan":{"by":34,"planned_at":283046399}}],"payload":{"core:birthday":{"age_years":10}}}"#,
        ),
    ];
    for (index, line) in spots {
        assert_eq!(log[index], line, "entry {index}");
    }
    // Nobody dies in the ten years: each founder still has its two deaths ahead
    // (senescence and accident, ADR-0017 section 2).
    assert_eq!(
        world.pending_events(),
        12,
        "year end series, market rounds, 3 birthdays, 6 deaths and sim 3's leaving home"
    );
}

/// ADR-0002 (year end), ADR-0004 section 8: one row per year from the state.
#[test]
fn year_end_writes_the_census() {
    let mut world = started(&BORN);
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| false);
    assert_eq!(out.census, expected_census());
}

/// Precursor of ADR-0002 `state_hash_independent_of_step_width` (the hashes join in
/// step 2e): how a run is cut into calls changes neither the log nor the census.
#[test]
fn log_and_census_independent_of_step_width() {
    let mut expected_log = lines(&started(&BORN).take_log());
    expected_log.extend(expected_birthdays());
    for width in [END + 1, Y, 86_400, 7_919] {
        let mut world = started(&BORN);
        let (log, out) = run_in_steps(&mut world, width);
        assert_eq!(log, expected_log, "width {width}");
        assert_eq!(out.census, expected_census(), "width {width}");
    }
    // Halting after every second with events.
    let mut world = started(&BORN);
    let mut out = YearOut::default();
    let mut log = lines(&world.take_log());
    let mut stops = 0;
    loop {
        let outcome = world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| true);
        log.extend(lines(&world.take_log()));
        match outcome {
            AdvanceOutcome::Stopped { .. } => stops += 1,
            AdvanceOutcome::ReachedLimit => break,
        }
    }
    // 30 seconds with birthdays and year ends, 129 market rounds (one per month start
    // from month 1, ADR-0018 section 1), of which 9 share the second of sim 1's birthday.
    assert_eq!(stops, 150, "150 seconds with events in 10 years");
    assert_eq!(log, expected_log);
    assert_eq!(out.census, expected_census());
}

/// ADR-0002: `stop` is asked after the last event of a second, observers included.
#[test]
fn stop_ends_after_the_whole_second() {
    let mut world = started(&BORN);
    world.take_log();
    let mut out = YearOut::default();
    let limit = SimTime::from_secs(END);
    // Sim 1's birthday, the market rounds of months 1 to 9 (they write nothing without a
    // single man), sim 2's birthday, the rounds of months 10 to 12.
    let month = |k: i64| k * 2_419_200;
    let mut seconds = vec![(0, 1)];
    seconds.extend((1..=9).map(|k| (month(k), 0)));
    seconds.push((22_809_600, 1));
    seconds.extend((10..=12).map(|k| (month(k), 0)));
    for (second, entries) in seconds {
        let outcome = world.advance_until(&mut out, limit, &mut |_| true);
        let second = SimTime::from_secs(second);
        assert_eq!(outcome, AdvanceOutcome::Stopped { second });
        assert_eq!(world.closed_through(), second);
        assert!(out.census.is_empty());
        assert_eq!(world.take_log().len(), entries);
    }
    // Sim 3's birthday and the year end share the last second of year 0.
    let outcome = world.advance_until(&mut out, limit, &mut |_| true);
    let second = SimTime::from_secs(31_449_599);
    assert_eq!(outcome, AdvanceOutcome::Stopped { second });
    assert_eq!(out.census.len(), 1);
    assert_eq!(lines(&world.take_log()), [birthday_line(0, 3)]);
    // The stop closure sees the state after the second.
    let outcome = world.advance_until(&mut out, limit, &mut |state| state.living() == 3);
    assert_eq!(
        outcome,
        AdvanceOutcome::Stopped {
            second: SimTime::from_secs(Y)
        }
    );
}

/// ADR-0002 `no_second_time_loop` on the dummy world: per game year exactly the events
/// that follow from the plans (3 birthdays, 1 year end, the market rounds: 12 in year 0
/// from month 1, then 13 a year), one context per domain call plus one for the input,
/// and the same in every decomposition.
#[test]
fn no_second_time_loop_in_the_dummy_world() {
    for width in [Y, 86_400, 7_919] {
        let mut world = started(&BORN);
        let mut out = YearOut::default();
        for year in 0..10u64 {
            let year_end = year_start(i64::try_from(year).expect("small") + 1).as_secs() - 1;
            while world.closed_through().as_secs() < year_end {
                let limit = (world.closed_through().as_secs() + width).min(year_end);
                world.advance_until(&mut out, SimTime::from_secs(limit), &mut |_| false);
            }
            let done = year + 1;
            let stats = world.stats();
            let birthdays = stats.events_of(EventClass::GENERAL, EventPayload::BIRTHDAY);
            let year_ends = stats.events_of(EventClass::OBSERVER, EventPayload::YEAR_END);
            let rounds = stats.events_of(EventClass::GENERAL, EventPayload::MARKET_ROUND);
            assert_eq!(birthdays, 3 * done, "width {width}, year {year}");
            assert_eq!(year_ends, done, "width {width}, year {year}");
            assert_eq!(rounds, 13 * done - 1, "width {width}, year {year}");
            let domain = 3 * done + 13 * done - 1;
            assert_eq!(stats.domain_calls, domain, "width {width}, year {year}");
            assert_eq!(stats.observer_calls, done, "width {width}, year {year}");
            assert_eq!(stats.contexts, 1 + domain, "width {width}, year {year}");
        }
    }
}

/// Zero-effect twin: without founders only the year ends and the market rounds run, and
/// the rounds write nothing.
#[test]
fn an_empty_world_runs_only_year_ends() {
    let mut world = started(&[]);
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| false);
    let rows: Vec<CensusRow> = (0..10)
        .map(|year| CensusRow {
            year,
            living: 0,
            lived_secs: 0,
            body: BodyCensus::default(),
            population: unmoved(),
            inheritance: unaffected(),
        })
        .collect();
    assert_eq!(out.census, rows);
    assert_eq!(world.take_log().len(), 1, "only core:started");
    let stats = world.stats();
    // 129 rounds in ten years: months 1 to 129.
    assert_eq!(stats.domain_calls, 129);
    assert_eq!(stats.observer_calls, 10);
    assert_eq!(stats.contexts, 130);
    // The series of the year end and of the market rounds.
    assert_eq!(world.pending_events(), 2);
}

/// Lever test: the founders of the input show in the first census row.
#[test]
fn founders_show_in_the_first_census() {
    for born in [&BORN[..], &[-5, -4, -3, -2, -1][..]] {
        let mut world = started(born);
        let mut out = YearOut::default();
        world.advance_until(
            &mut out,
            year_start(1) - SimDuration::from_secs(1),
            &mut |_| false,
        );
        let count = u64::try_from(born.len()).expect("small");
        assert_eq!(
            out.census,
            [CensusRow {
                year: 0,
                living: count,
                lived_secs: count * 31_449_600,
                body: BodyCensus::default(),
                population: unmoved(),
                inheritance: unaffected(),
            }]
        );
    }
}

/// ADR-0004 test 5: year by year, creation entries so far equal the SimIds handed out,
/// departure entries equal the size of the archive, and every SimId in the log resolves.
#[test]
fn yearly_balance_log_matches_state() {
    let mut world = started(&BORN);
    let mut out = YearOut::default();
    let mut log = world.take_log();
    for year in 0..10 {
        let limit = year_start(year + 1) - SimDuration::from_secs(1);
        world.advance_until(&mut out, limit, &mut |_| false);
        log.extend(world.take_log());
        let created = log
            .iter()
            .filter(|entry| matches!(entry.payload, LogPayload::FounderAdded { .. }))
            .count();
        let state = world.state();
        assert_eq!(
            u64::try_from(created).expect("small"),
            state.sim_ids_allocated()
        );
        assert_eq!(state.archive_len(), 0, "no departure entry exists yet");
        for entry in &log {
            for Subject::Sim(id) in &entry.subjects {
                assert!(state.born(*id).is_some(), "sim {} in year {year}", id.get());
            }
        }
        assert_eq!(state.born(SimId::try_from(4).expect("valid")), None);
    }
    assert_eq!(log.len(), 37);
}

/// An accepted input never breaks an invariant later (ADR-0004 section 4): the oldest
/// founder the time range allows, born 1 000 000 years before the start, has its birthday
/// in the first second like everyone else; the span from birth to now exceeds the range of
/// a duration. In the same second it dies of senescence, after the birthday (planned
/// first): at that age `e^(−b · x0)` is 0, so the closed form leaves no time (genetics
/// specification 7.1).
#[test]
fn the_oldest_possible_founder_has_its_birthday_and_dies() {
    let born = -1_000_000 * Y;
    let mut world = started(&[born]);
    world.take_log();
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(2 * Y), &mut |_| false);
    let seen: Vec<(i64, String)> = world
        .take_log()
        .iter()
        .map(|entry| {
            let what = match &entry.payload {
                LogPayload::Birthday { age_years } => format!("birthday {age_years}"),
                LogPayload::Death { cause } => format!("death of {}", cause.as_str()),
                _ => panic!("only her birthday and her death expected"),
            };
            (entry.t.as_secs(), what)
        })
        .collect();
    assert_eq!(
        seen,
        [
            (0, "birthday 1000000".to_owned()),
            (0, "death of core:senescence".to_owned())
        ]
    );
    assert_eq!(out.census.len(), 2);
    assert_eq!(
        out.census[0].lived_secs, 0,
        "the second of death is not lived"
    );
    assert_eq!(world.state().archive_len(), 1);
}

/// A `Start` after the clock moved is rejected; it could not plan the first year end.
#[test]
fn start_after_advancing_is_rejected() {
    let mut world = World::new(common::data());
    let mut out = YearOut::default();
    let outcome = world.advance_until(&mut out, SimTime::START, &mut |_| false);
    assert_eq!(outcome, AdvanceOutcome::ReachedLimit);
    let rejected = world.submit(start_with(&BORN)).expect_err("clock moved");
    assert_eq!(rejected.code, RejectCode::ClockNotAtStart);
    assert_eq!(world.stats().contexts, 0);
    assert!(world.input_log().is_empty());
    assert!(world.take_log().is_empty());
    assert_eq!(world.pending_events(), 0);
}
