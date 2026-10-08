//! The trace per sim (ADR-0004 section 6 and acceptance test 3): tracing changes
//! nothing about a run, and what is traced is neither state nor saved.
//!
//! Scenario: three founders born 30 years (sim 1), 25 years and 100 days (sim 2) and
//! one second (sim 3) before the start. The dummy trace of step 2 records every planned
//! birthday: three at the start, then one per birthday.

mod common;

use std::collections::BTreeSet;

use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{Fertility, Founder, Input, Start};
use sim_core::log::log_hash;
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::trace::{TraceEntry, TraceFilter};
use sim_core::world::{World, YearOut};

/// A fertility under which nobody ever conceives.
fn barren() -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(0.0).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    }
}

const Y: i64 = 31_449_600;
const END: i64 = 10 * Y - 1;

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid sim id")
}

fn new_world(filter: TraceFilter) -> World {
    let mut world = World::new(common::data());
    world.set_trace(filter);
    world
        .submit(Input::Start(Start {
            world_seed: 1,
            founder_tag: DataId::new("test:trace").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            death_causes: Vec::new(),
            founders: [-943_488_000, -794_880_000, -1]
                .into_iter()
                .map(|secs| Founder {
                    born: SimTime::from_secs(secs),
                    sex: Sex::Female,
                    alleles: common::dummy(0, 1),
                })
                .collect(),
            couples: Vec::new(),
            fertility: barren(),
        }))
        .expect("accepted");
    world
}

struct Run {
    year_hashes: Vec<sim_core::canonical::YearHash>,
    end_hash: u64,
    log_hash: u64,
    log_entries: usize,
    save: Vec<u8>,
    trace: Vec<TraceEntry>,
}

fn run(filter: TraceFilter) -> Run {
    let mut world = new_world(filter);
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| false);
    let log = world.take_log();
    Run {
        end_hash: world.state_hash(),
        log_hash: log_hash(&log),
        log_entries: log.len(),
        save: world.save(&out.year_hashes).expect("saves"),
        trace: world.take_trace(),
        year_hashes: out.year_hashes,
    }
}

fn lines(trace: &[TraceEntry]) -> Vec<String> {
    trace
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("json"))
        .collect()
}

/// ADR-0004 test 3: the trace for all sims against no trace: the same hashes, and the
/// trace is not empty.
#[test]
fn trace_on_equals_trace_off() {
    let off = run(TraceFilter::Off);
    let on = run(TraceFilter::All);
    assert!(off.trace.is_empty());
    assert_eq!(
        on.trace.len(),
        3 + 30,
        "three first birthdays, thirty later ones"
    );
    assert_eq!(on.year_hashes, off.year_hashes);
    assert_eq!(on.end_hash, off.end_hash);
    assert_eq!(on.log_hash, off.log_hash);
    assert_eq!(on.log_entries, off.log_entries);
    assert!(on.save == off.save, "the trace is not saved");
    assert_eq!(on.year_hashes.len(), 10);
}

/// The entries of the first year, by hand: at the start the first birthdays are
/// planned (sim 1 at 0, sim 2 on day 264, sim 3 in the last second of year 0), each
/// birthday plans the next one a year later.
#[test]
fn the_trace_is_pinned() {
    let mut world = new_world(TraceFilter::All);
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(Y - 1), &mut |_| false);
    assert_eq!(
        lines(&world.take_trace()),
        [
            r#"{"t":0,"sim":1,"payload":{"core:birthday_planned":{"next":0}}}"#,
            r#"{"t":0,"sim":2,"payload":{"core:birthday_planned":{"next":22809600}}}"#,
            r#"{"t":0,"sim":3,"payload":{"core:birthday_planned":{"next":31449599}}}"#,
            r#"{"t":0,"sim":1,"payload":{"core:birthday_planned":{"next":31449600}}}"#,
            r#"{"t":22809600,"sim":2,"payload":{"core:birthday_planned":{"next":54259200}}}"#,
            r#"{"t":31449599,"sim":3,"payload":{"core:birthday_planned":{"next":62899199}}}"#,
        ]
    );
    assert!(
        world.take_trace().is_empty(),
        "taking hands the buffer over"
    );
}

#[test]
fn the_trace_follows_the_filter() {
    let only_two: BTreeSet<SimId> = [sim(2)].into_iter().collect();
    let traced = run(TraceFilter::Sims(only_two));
    assert_eq!(traced.trace.len(), 1 + 10);
    assert!(traced.trace.iter().all(|entry| entry.sim == sim(2)));
    // The filter can change between two calls; it is no input and no state.
    let mut world = new_world(TraceFilter::Off);
    let before = world.state_hash();
    world.set_trace(TraceFilter::All);
    assert_eq!(world.state_hash(), before);
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(0), &mut |_| false);
    assert_eq!(world.take_trace().len(), 1, "sim 1 at t = 0");
    world.set_trace(TraceFilter::Off);
    world.advance_until(&mut out, SimTime::from_secs(Y), &mut |_| false);
    assert!(world.take_trace().is_empty());
}

/// ADR-0004 section 6: the filter is not saved; a loaded world traces nothing.
#[test]
fn a_loaded_world_traces_nothing() {
    let mut world = new_world(TraceFilter::All);
    world.take_log();
    assert_eq!(world.take_trace().len(), 3);
    let bytes = world.save(&[]).expect("saves");
    let (mut loaded, _) = World::load(&bytes, common::data()).expect("loads");
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(Y), &mut |_| false);
    assert!(loaded.take_trace().is_empty());
    assert_eq!(loaded.take_log().len(), 4);
}
