//! The input path of the world (ADR-0004 sections 1, 4, 6 and 7): `Start` is stamped,
//! recorded in the input log and applied; a rejected input changes nothing.
//!
//! Scenario: three founders born 30 years, 25 years and 100 days, and one second before
//! the start (YEAR = 31 449 600 s, DAY = 86 400 s). Expected values are derived by hand
//! from the ADRs. Logs are compared as JSON lines because `EventId` cannot be compared.

mod common;

use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::{Allele, Genotype, Sex};
use sim_core::ids::SimId;
use sim_core::input::{Fertility, ForkOverride, Founder, Input, InputRejected, RejectCode, Start};
use sim_core::log::LogEntry;
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::SimTime;
use sim_core::world::World;

/// A fertility under which nobody ever conceives.
fn barren() -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(0.0).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    }
}

const SEED: u64 = 0x5eed_0000_0000_0001;
const BORN: [i64; 3] = [-943_488_000, -794_880_000, -1];

fn start_with(born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: SEED,
        founder_tag: DataId::new("test:world_start").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: Vec::new(),
        founders: born.iter().copied().map(founder).collect(),
        couples: Vec::new(),
        fertility: barren(),
    })
}

/// A founder born at `secs`: female, with the alleles wild and dummy.
fn founder(secs: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(secs),
        sex: Sex::Female,
        genotype: Genotype {
            maternal: Allele::Wild,
            paternal: Allele::Dummy,
        },
    }
}

const OFF: WorldParams = WorldParams {
    hereditary_diseases: false,
};
const ON: WorldParams = WorldParams {
    hereditary_diseases: true,
};

/// An override block that sets the switch of the hereditary diseases, once per value.
fn override_block(values: &[bool]) -> Input {
    Input::ForkOverride(ForkOverride {
        changes: values
            .iter()
            .map(|&on| ParamChange::HereditaryDiseases(on))
            .collect(),
    })
}

fn lines(log: &[LogEntry]) -> Vec<String> {
    log.iter()
        .map(|entry| serde_json::to_string(entry).expect("json"))
        .collect()
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid sim id")
}

/// Everything a rejected input must leave unchanged, visible through the public API.
fn fingerprint(world: &World) -> String {
    format!(
        "closed {} inputs {} pending {} contexts {} sims {} living {} seed {:?} params {:?} hash {:016x}",
        world.closed_through().as_secs(),
        serde_json::to_string(world.input_log()).expect("json"),
        world.pending_events(),
        world.stats().contexts,
        world.state().sim_ids_allocated(),
        world.state().living(),
        world.state().world_seed(),
        world.state().world_params(),
        world.state_hash(),
    )
}

#[test]
fn a_new_world_has_closed_nothing() {
    let mut world = World::new(common::data());
    assert_eq!(world.closed_through(), SimTime::from_secs(-1));
    assert!(world.take_log().is_empty());
    assert!(world.input_log().is_empty());
    assert_eq!(world.pending_events(), 0);
    assert_eq!(world.state().world_seed(), None);
    assert_eq!(world.state().living(), 0);
    assert_eq!(world.state().sim_ids_allocated(), 0);
}

/// ADR-0004 section 4: the stamp is `closed_through` (-1 before the first advance) and
/// the position in the input log, from 1.
#[test]
fn start_is_stamped_minus_one_seq_one() {
    let mut world = World::new(common::data());
    let stamp = world.submit(start_with(&BORN)).expect("accepted");
    assert_eq!(
        serde_json::to_string(&stamp).expect("json"),
        r#"{"t":-1,"seq":1}"#
    );
}

/// ADR-0004 section 7: `core:started` comes first; every founder gets one creation
/// entry. An input's entries carry `now`, the second after its stamp (ADR-0002). The
/// lineage keys are those of the founders 0 to 2 of `test:world_start` (ADR-0001
/// section 4, values from tools/rng_reference.py).
#[test]
fn start_writes_started_then_one_founder_added_each() {
    let mut world = World::new(common::data());
    world.submit(start_with(&BORN)).expect("accepted");
    let expected = [
        r#"{"id":1,"t":0,"subjects":[],"causes":[{"input":1}],"payload":{"core:started":{"founders":3}}}"#,
        r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:founder_added":{"born":-943488000,"lineage_key":"84828ab838f0f36a"}}}"#,
        r#"{"id":3,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1}],"payload":{"core:founder_added":{"born":-794880000,"lineage_key":"c4e5b66af9b28b79"}}}"#,
        r#"{"id":4,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1}],"payload":{"core:founder_added":{"born":-1,"lineage_key":"f6b2e48506372978"}}}"#,
    ];
    assert_eq!(lines(&world.take_log()), expected);
}

#[test]
fn start_records_seed_and_founders() {
    let mut world = World::new(common::data());
    world.submit(start_with(&BORN)).expect("accepted");
    let state = world.state();
    assert_eq!(state.world_seed(), Some(SEED));
    assert_eq!(state.world_params(), Some(OFF));
    assert_eq!(state.sim_ids_allocated(), 3);
    assert_eq!(state.living(), 3);
    for (index, &born) in BORN.iter().enumerate() {
        let id = sim(u64::try_from(index).expect("small") + 1);
        assert_eq!(
            state.born(id),
            Some(SimTime::from_secs(born)),
            "sim {index}"
        );
    }
    assert_eq!(state.born(sim(4)), None);
    assert_eq!(
        world.pending_events(),
        4,
        "the year end series and three first birthdays"
    );
    assert_eq!(world.stats().contexts, 1, "one applied input");
    assert_eq!(world.closed_through(), SimTime::from_secs(-1));
}

/// Lever test: the number of founders in the input shows in the state.
#[test]
fn more_founders_give_more_sims() {
    let mut small = World::new(common::data());
    small.submit(start_with(&BORN)).expect("accepted");
    let mut large = World::new(common::data());
    large
        .submit(start_with(&[-5, -4, -3, -2, -1]))
        .expect("accepted");
    assert_eq!(small.state().living(), 3);
    assert_eq!(large.state().living(), 5);
    assert_eq!(large.take_log().len(), 6);
}

/// Lever test: the world parameters of the input are those of the world, and they are
/// state: the same start with the switch on has another hash.
#[test]
fn start_sets_the_world_parameters() {
    let with_switch = |on: bool| {
        let Input::Start(mut start) = start_with(&BORN) else {
            panic!("a start");
        };
        start.world_params = WorldParams {
            hereditary_diseases: on,
        };
        let mut world = World::new(common::data());
        world.submit(Input::Start(start)).expect("accepted");
        world
    };
    let (off, on) = (with_switch(false), with_switch(true));
    assert_eq!(off.state().world_params(), Some(OFF));
    assert_eq!(on.state().world_params(), Some(ON));
    assert_ne!(off.state_hash(), on.state_hash());
    // A start with the switch on and an override that switches it on reach the same
    // parameters; the projections differ by nothing else.
    let mut overridden = with_switch(false);
    overridden
        .submit(override_block(&[true]))
        .expect("accepted");
    assert_eq!(overridden.state_hash(), on.state_hash());
}

#[test]
fn a_start_without_founders_writes_only_started() {
    let mut world = World::new(common::data());
    world.submit(start_with(&[])).expect("accepted");
    assert_eq!(
        lines(&world.take_log()),
        [
            r#"{"id":1,"t":0,"subjects":[],"causes":[{"input":1}],"payload":{"core:started":{"founders":0}}}"#
        ]
    );
    assert_eq!(world.state().living(), 0);
    assert_eq!(world.state().world_seed(), Some(SEED));
    assert_eq!(world.pending_events(), 1);
}

/// ADR-0004 section 6: the log buffer is handed over, the core keeps nothing.
#[test]
fn take_log_hands_the_buffer_over() {
    let mut world = World::new(common::data());
    world.submit(start_with(&BORN)).expect("accepted");
    assert_eq!(world.take_log().len(), 4);
    assert!(world.take_log().is_empty());
}

/// ADR-0004 section 4: one line `{"t":…,"seq":…,"input":{…}}` per accepted input.
#[test]
fn the_input_log_keeps_the_stamped_input() {
    let mut world = World::new(common::data());
    world.submit(start_with(&[-1])).expect("accepted");
    assert_eq!(
        serde_json::to_string(world.input_log()).expect("json"),
        r#"[{"t":-1,"seq":1,"input":{"start":{"world_seed":"5eed000000000001","founder_tag":"test:world_start","world_params":{"hereditary_diseases":false},"death_causes":[],"founders":[{"born":-1,"sex":"female","genotype":{"maternal":"wild","paternal":"dummy"}}],"couples":[],"fertility":{"per_month":0.0,"from_years":18,"until_years":45,"pregnancy_months":10}}}}]"#
    );
}

/// ADR-0004 test 8 (step 2 part; the state hash joins in step 2e): a rejected input is
/// an `Err` with a reason code, no invariant break, and changes nothing.
#[test]
fn rejected_input_changes_nothing() {
    let mut world = World::new(common::data());
    world.submit(start_with(&BORN)).expect("accepted");
    world.take_log();
    let before = fingerprint(&world);
    let rejected = world.submit(start_with(&BORN)).expect_err("second start");
    assert_eq!(
        rejected,
        InputRejected {
            code: RejectCode::AlreadyStarted
        }
    );
    assert_eq!(fingerprint(&world), before);
    assert!(world.take_log().is_empty());

    // ADR-0002: founders have negative birth times.
    for born in [0, 1, 31_449_600] {
        let mut world = World::new(common::data());
        let before = fingerprint(&world);
        let rejected = world
            .submit(start_with(&[-1, born]))
            .expect_err("founder not born before the start");
        assert_eq!(
            rejected.code,
            RejectCode::FounderNotBornBeforeStart,
            "{born}"
        );
        assert_eq!(fingerprint(&world), before, "{born}");
        assert!(world.take_log().is_empty());
    }
}

/// ADR-0001 section 6 and ADR-0004 section 4: an override block changes the world
/// parameters in the order of its list and writes one entry per change, with the input
/// as the cause. It is stamped like every input and acts in the second after its stamp.
#[test]
fn an_override_block_changes_the_world_parameters() {
    let mut world = World::new(common::data());
    assert_eq!(world.state().world_params(), None);
    world.submit(start_with(&[-1])).expect("accepted");
    world.take_log();
    assert_eq!(world.state().world_params(), Some(OFF));
    let (pending, hash) = (world.pending_events(), world.state_hash());

    let stamp = world.submit(override_block(&[true])).expect("accepted");
    assert_eq!(
        serde_json::to_string(&stamp).expect("json"),
        r#"{"t":-1,"seq":2}"#
    );
    assert_eq!(world.state().world_params(), Some(ON));
    assert_eq!(
        lines(&world.take_log()),
        [
            r#"{"id":3,"t":0,"subjects":[],"causes":[{"input":2}],"payload":{"core:param_changed":{"change":{"core:hereditary_diseases":true}}}}"#
        ]
    );
    assert_ne!(world.state_hash(), hash, "the parameters are state");
    assert_eq!(world.pending_events(), pending, "nothing is planned anew");
    assert_eq!(world.input_log().len(), 2);
    assert_eq!(
        serde_json::to_string(&world.input_log()[1]).expect("json"),
        r#"{"t":-1,"seq":2,"input":{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}}"#
    );

    // Later, after the clock moved: stamped with the last closed second, acting in the
    // next one. A block that sets the value the switch has is a block like any other.
    let mut out = sim_core::world::YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(99), &mut |_| false);
    let on = world.state_hash();
    let stamp = world.submit(override_block(&[true])).expect("accepted");
    assert_eq!(
        serde_json::to_string(&stamp).expect("json"),
        r#"{"t":99,"seq":3}"#
    );
    assert_eq!(world.state_hash(), on, "the same value, the same state");
    assert_eq!(
        lines(&world.take_log()),
        [
            r#"{"id":4,"t":100,"subjects":[],"causes":[{"input":3}],"payload":{"core:param_changed":{"change":{"core:hereditary_diseases":true}}}}"#
        ]
    );
    world.submit(override_block(&[false])).expect("accepted");
    assert_eq!(world.state().world_params(), Some(OFF));
    assert_eq!(world.take_log().len(), 1);
}

/// ADR-0004 test 8 for the override block: before the start, without a change or with a
/// parameter twice it is rejected with its code and changes nothing.
#[test]
fn a_rejected_override_block_changes_nothing() {
    let mut world = World::new(common::data());
    let before = fingerprint(&world);
    let rejected = world
        .submit(override_block(&[true]))
        .expect_err("before the start");
    assert_eq!(rejected.code, RejectCode::NotStarted);
    // The world has not started: that comes before the look at the block.
    let rejected = world.submit(override_block(&[])).expect_err("not started");
    assert_eq!(rejected.code, RejectCode::NotStarted);
    assert_eq!(fingerprint(&world), before);

    world.submit(start_with(&BORN)).expect("accepted");
    world.take_log();
    let before = fingerprint(&world);
    for (values, code) in [
        (&[][..], RejectCode::EmptyOverride),
        (&[true, true][..], RejectCode::OverrideParamTwice),
        (&[true, false][..], RejectCode::OverrideParamTwice),
        (&[false, true, false][..], RejectCode::OverrideParamTwice),
    ] {
        let rejected = world.submit(override_block(values)).expect_err("rejected");
        assert_eq!(rejected, InputRejected { code }, "{values:?}");
        assert_eq!(fingerprint(&world), before, "{values:?}");
        assert!(world.take_log().is_empty());
    }
    // The next accepted input takes the next sequence number.
    let stamp = world.submit(override_block(&[true])).expect("accepted");
    assert_eq!(
        serde_json::to_string(&stamp).expect("json"),
        r#"{"t":-1,"seq":2}"#
    );
}

/// ADR-0004 section 4: a rejected input takes no sequence number.
#[test]
fn a_rejected_first_start_takes_no_sequence_number() {
    let mut world = World::new(common::data());
    world.submit(start_with(&[0])).expect_err("rejected");
    let stamp = world.submit(start_with(&BORN)).expect("accepted");
    assert_eq!(
        serde_json::to_string(&stamp).expect("json"),
        r#"{"t":-1,"seq":1}"#
    );
    assert_eq!(world.input_log().len(), 1);
}
