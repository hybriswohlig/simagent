//! The input path of the world (ADR-0004 sections 1, 4, 6 and 7): `Start` is stamped,
//! recorded in the input log and applied; a rejected input changes nothing.
//!
//! Scenario: three founders born 30 years, 25 years and 100 days, and one second before
//! the start (YEAR = 31 449 600 s, DAY = 86 400 s). Expected values are derived by hand
//! from the ADRs. Logs are compared as JSON lines because `EventId` cannot be compared.

mod common;

use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::ids::SimId;
use sim_core::input::{ForkOverride, Founder, Input, InputRejected, RejectCode, Start};
use sim_core::log::LogEntry;
use sim_core::params::{NutritionYear, ParamChange, WorldParams};
use sim_core::time::SimTime;
use sim_core::world::World;

const SEED: u64 = 0x5eed_0000_0000_0001;
const BORN: [i64; 3] = [-943_488_000, -794_880_000, -1];

fn start_with(born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: SEED,
        founder_tag: DataId::new("test:world_start").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: born.iter().copied().map(founder).collect(),
        couples: Vec::new(),
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

const OFF: WorldParams = WorldParams {
    hereditary_diseases: false,
    nutrition: Vec::new(),
};
const ON: WorldParams = WorldParams {
    hereditary_diseases: true,
    nutrition: Vec::new(),
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
        11,
        "the series of the year end and the rounds, three first birthdays and two deaths each"
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
            nutrition: Vec::new(),
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
    // The series of the year end and of the market rounds.
    assert_eq!(world.pending_events(), 2);
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
        r#"[{"t":-1,"seq":1,"input":{"start":{"world_seed":"5eed000000000001","founder_tag":"test:world_start","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}],"couples":[]}}}]"#
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

/// Genetics specification 2.1: the alleles of a founder must fit the genome map. A locus
/// it does not have, a locus twice, a wrong number of copies for the chromosome type and
/// sex, or an allele other than 0 and 1 reject `Start` and change nothing. A male founder
/// has one copy of an X locus, a female none of a Y locus.
#[test]
fn founder_alleles_must_fit_the_genome_map() {
    let dummy = |copies: Vec<u8>| LocusAlleles {
        locus: DataId::new("core:disease_ar").expect("valid"),
        copies,
    };
    let with = |sex: Sex, alleles: Vec<LocusAlleles>| {
        let Input::Start(mut start) = start_with(&[-1]) else {
            panic!("a start");
        };
        start.founders[0].sex = sex;
        start.founders[0].alleles = alleles;
        Input::Start(start)
    };
    let unknown = LocusAlleles {
        locus: DataId::new("core:nothing").expect("valid"),
        copies: vec![0, 1],
    };
    for (input, code) in [
        (
            with(Sex::Female, vec![unknown]),
            RejectCode::FounderUnknownLocus,
        ),
        (
            with(Sex::Female, vec![dummy(vec![0, 1]), dummy(vec![1, 1])]),
            RejectCode::FounderLocusTwice,
        ),
        (
            with(Sex::Male, vec![dummy(vec![1])]),
            RejectCode::FounderCopies,
        ),
        (
            with(Sex::Male, vec![dummy(vec![1, 1, 1])]),
            RejectCode::FounderCopies,
        ),
        (
            with(Sex::Female, vec![dummy(vec![0, 2])]),
            RejectCode::FounderCopies,
        ),
    ] {
        let mut world = World::new(common::data());
        let before = fingerprint(&world);
        let rejected = world.submit(input).expect_err("rejected");
        assert_eq!(rejected.code, code);
        assert_eq!(fingerprint(&world), before);
        assert!(world.take_log().is_empty());
    }
    // Copies per chromosome type and sex, on a map with an X, a Y and an mt locus: a man
    // has one X copy, a woman two and no Y copy (also not an empty list); mt has one.
    let (mut calibration, _) = common::files();
    // The diseases of the repository name loci this map does not have.
    calibration["diseases"] = serde_json::json!([]);
    let loci = serde_json::json!([
        {"id": "test:x", "chromosome": "x", "segment": 0, "slot": 0, "action": "recessive",
         "mark": "large", "mutation": "both"},
        {"id": "test:y", "chromosome": "y", "segment": 0, "slot": 0, "action": "recessive",
         "mark": "large", "mutation": "both"},
        {"id": "test:mt", "chromosome": "mt", "segment": 0, "slot": 0, "action": "recessive",
         "mark": "large", "mutation": "both"},
    ]);
    let genome_map =
        serde_json::json!({"format_version": 2, "map_version": 1, "traits": [], "loci": loci});
    let data = sim_core::data::Data::new(
        common::definitions(calibration, genome_map).expect("definitions"),
    )
    .expect("checked");
    let on = |id: &str, copies: Vec<u8>| LocusAlleles {
        locus: DataId::new(id).expect("valid"),
        copies,
    };
    for (sex, alleles, fits) in [
        (Sex::Male, on("test:x", vec![1]), true),
        (Sex::Male, on("test:x", vec![1, 0]), false),
        (Sex::Female, on("test:x", vec![1, 0]), true),
        (Sex::Female, on("test:x", vec![1]), false),
        (Sex::Male, on("test:y", vec![1]), true),
        (Sex::Male, on("test:y", vec![]), false),
        (Sex::Female, on("test:y", vec![]), false),
        (Sex::Female, on("test:y", vec![1]), false),
        (Sex::Female, on("test:mt", vec![1]), true),
        (Sex::Male, on("test:mt", vec![1, 1]), false),
    ] {
        let mut world = World::new(data.clone());
        let result = world.submit(with(sex, vec![alleles.clone()]));
        match fits {
            true => assert!(result.is_ok(), "{sex:?} {alleles:?}"),
            false => assert_eq!(
                result.expect_err("rejected").code,
                RejectCode::FounderCopies,
                "{sex:?} {alleles:?}"
            ),
        }
    }
    // The same founders with fitting alleles start.
    for input in [
        with(Sex::Male, vec![dummy(vec![1, 0])]),
        with(Sex::Female, vec![]),
    ] {
        World::new(common::data()).submit(input).expect("accepted");
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

fn fed(year: i64, index: f64) -> NutritionYear {
    NutritionYear {
        year,
        index: FiniteF64::try_new(index).expect("finite"),
    }
}

/// `start_with(&BORN)` with the nutrition index of these years.
fn start_fed(years: &[(i64, f64)]) -> Input {
    let Input::Start(mut start) = start_with(&BORN) else {
        unreachable!("a start")
    };
    start.world_params.nutrition = years.iter().map(|&(y, n)| fed(y, n)).collect();
    Input::Start(start)
}

fn nutrition_block(years: &[(i64, f64)]) -> Input {
    Input::ForkOverride(ForkOverride {
        changes: years
            .iter()
            .map(|&(y, n)| ParamChange::NutritionYear(fed(y, n)))
            .collect(),
    })
}

/// ADR-0013 section 1: the years of the nutrition index rise strictly, `N` lies in
/// [0, 1] (both ends allowed, decided by Grok 4.7), and every year has a first and a
/// last second in the range of time (−1 000 000 to 999 999); years before the start
/// are allowed.
#[test]
fn the_nutrition_index_of_the_start_is_checked() {
    for (years, code) in [
        (&[(3, 0.7), (3, 0.5)][..], RejectCode::NutritionOrder),
        (&[(4, 0.7), (3, 0.7)][..], RejectCode::NutritionOrder),
        (&[(3, -0.1)][..], RejectCode::NutritionOutOfRange),
        (&[(3, 1.000_001)][..], RejectCode::NutritionOutOfRange),
        (&[(1_000_000, 0.7)][..], RejectCode::NutritionOutOfRange),
        (&[(-1_000_001, 0.7)][..], RejectCode::NutritionOutOfRange),
    ] {
        let mut world = World::new(common::data());
        let before = fingerprint(&world);
        let rejected = world.submit(start_fed(years)).expect_err("rejected");
        assert_eq!(rejected, InputRejected { code }, "{years:?}");
        assert_eq!(fingerprint(&world), before, "{years:?}");
    }
    let years = [(-1_000_000, 0.0), (-30, 0.5), (2, 1.0), (999_999, 0.7)];
    let mut world = World::new(common::data());
    world.submit(start_fed(&years)).expect("accepted");
    let params = world.state().world_params().expect("started");
    assert_eq!(params.nutrition, years.map(|(y, n)| fed(y, n)));
}

/// ADR-0013 section 2: an override sets the index of a year, inserted or replaced in the
/// order of the years, one `core:param_changed` per change.
#[test]
fn an_override_sets_the_nutrition_index_of_a_year() {
    let mut world = World::new(common::data());
    world.submit(start_fed(&[(5, 0.9)])).expect("accepted");
    world.take_log();
    let hash = world.state_hash();
    world
        .submit(nutrition_block(&[(3, 0.7), (5, 0.8)]))
        .expect("accepted");
    let params = world.state().world_params().expect("started");
    assert_eq!(params.nutrition, [fed(3, 0.7), fed(5, 0.8)]);
    assert!(!params.hereditary_diseases);
    assert_eq!(
        lines(&world.take_log()),
        [
            r#"{"id":5,"t":0,"subjects":[],"causes":[{"input":2}],"payload":{"core:param_changed":{"change":{"core:nutrition_year":{"year":3,"index":0.7}}}}}"#,
            r#"{"id":6,"t":0,"subjects":[],"causes":[{"input":2}],"payload":{"core:param_changed":{"change":{"core:nutrition_year":{"year":5,"index":0.8}}}}}"#
        ]
    );
    assert_ne!(world.state_hash(), hash, "the index is state");
    // The switch and a year in one block are two parameters.
    let mixed = Input::ForkOverride(ForkOverride {
        changes: vec![
            ParamChange::HereditaryDiseases(true),
            ParamChange::NutritionYear(fed(4, 0.0)),
        ],
    });
    world.submit(mixed).expect("accepted");
    let params = world.state().world_params().expect("started");
    assert!(params.hereditary_diseases);
    assert_eq!(params.nutrition, [fed(3, 0.7), fed(4, 0.0), fed(5, 0.8)]);
}

/// ADR-0013 section 2: the year is the parameter (twice in a block is rejected), the
/// range is that of the start, and an override sets no year before the year of the
/// second after its stamp.
#[test]
fn a_rejected_nutrition_override_changes_nothing() {
    let mut world = World::new(common::data());
    world.submit(start_fed(&[])).expect("accepted");
    let mut out = sim_core::world::YearOut::default();
    let last_of_year_1 = sim_core::time::year_start(2) - sim_core::time::SimDuration::from_secs(1);
    world.advance_until(&mut out, last_of_year_1, &mut |_| false);
    world.take_log();
    let before = fingerprint(&world);
    for (years, code) in [
        (&[(3, 0.7), (3, 0.7)][..], RejectCode::OverrideParamTwice),
        (
            &[(3, 0.7), (4, 0.5), (3, 0.9)][..],
            RejectCode::OverrideParamTwice,
        ),
        (&[(3, 1.5)][..], RejectCode::NutritionOutOfRange),
        (&[(1_000_000, 0.5)][..], RejectCode::NutritionOutOfRange),
        (&[(1, 0.7)][..], RejectCode::NutritionYearPast),
        (&[(3, 0.7), (1, 0.7)][..], RejectCode::NutritionYearPast),
    ] {
        let rejected = world.submit(nutrition_block(years)).expect_err("rejected");
        assert_eq!(rejected, InputRejected { code }, "{years:?}");
        assert_eq!(fingerprint(&world), before, "{years:?}");
        assert!(world.take_log().is_empty());
    }
    // The stamp is the last second of year 1; the override acts from year 2 on.
    world
        .submit(nutrition_block(&[(2, 0.7), (3, 0.7)]))
        .expect("accepted");
    let params = world.state().world_params().expect("started");
    assert_eq!(params.nutrition, [fed(2, 0.7), fed(3, 0.7)]);
}

/// ADR-0013 section 2: the fork year is the year of the second after the stamp. Stamped
/// two seconds before year 2 the block acts in year 1 and may set it; one second later
/// it acts in year 2, and year 1 is past.
#[test]
fn the_fork_year_is_the_year_of_the_next_second() {
    let second = sim_core::time::SimDuration::from_secs(1);
    let at = |stamp: SimTime| {
        let mut world = World::new(common::data());
        world.submit(start_fed(&[])).expect("accepted");
        let mut out = sim_core::world::YearOut::default();
        world.advance_until(&mut out, stamp, &mut |_| false);
        world.submit(nutrition_block(&[(1, 0.7)]))
    };
    let year_2 = sim_core::time::year_start(2);
    assert!(at(year_2 - second - second).is_ok());
    assert_eq!(
        at(year_2 - second).expect_err("past"),
        InputRejected {
            code: RejectCode::NutritionYearPast
        }
    );
}
