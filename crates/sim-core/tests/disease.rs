//! The dummy disease (ADR-0001 "Durchstich", C5): a cause of death that acts only on
//! sims with the dummy allele twice, and only while the switch of the hereditary
//! diseases is on. Such a sim keeps its exposure; an override that flips the switch
//! plans its death of the disease from the remaining budget, without a new draw
//! (ADR-0001 section 6).
//!
//! The expected bytes and times come from `tools/state_reference.py`, which runs the
//! model of `tools/world_reference.py`, written from the ADRs and not from this code.
//!
//! Scenario "sick": seed 0xff, founder tag `test:sick`, `test:base` with 0.02 per year
//! for everyone and `test:dummy_disease` with 0.3 per year for the homozygous. A woman
//! (wild, dummy) born one second and a man (dummy twice) born two seconds before the
//! start. The disease needs about 3.21 years of exposure to kill him, `test:base` about
//! 19.6 years.

mod common;

use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::{Allele, Genotype, Sex};
use sim_core::input::{AppliesTo, DeathCause, Fertility, ForkOverride, Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::SimTime;
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

// tools/state_reference.py
/// How long each cause must act on him, in seconds.
const HIS_DISEASE: i64 = 101_053_347;
const HIS_BASE: i64 = 617_173_485;
/// Switched on in this second, the disease and `test:base` are due in the same second.
const TIE_SWITCH: i64 = 516_120_138;
const SICK_ON_SAVE_AFTER_START: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0a01030106fc87ff1d0004010200fe87ff1d0002010100fe87ff1d000100018088ff1dc6ceaf6000060202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010007020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c010101010001000000000000060300020001000300030002000401010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0a00";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afe9ffc77030103fea7fb9501000d010100fea7fb9501000100018088ff1d8ea5b5f604000302018bbec9d8c5bd99888b01000e010101abd4f19caceffaefab010000010000000000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c010101010001000000030bfe9ffc77010002000c01010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0a040089f583baa3cddefb5a02b18baaceeca59d9d45048dbfb9dbe78393cf0f06b4cabdcbf29addb01a";
const SICK_OFF_SAVE_AFTER_START: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010100028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0a01030105fc87ff1d0004010200fe87ff1d0002010100fe87ff1d000100018088ff1ddac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010006020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c010101010000000000000005030002000100030002000401010100ff0109746573743a7369636b000209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0a00";
const SWITCHED_ON_SAVE: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afe87ff1d030106fc8ffe3b0006010200fe8ffe3b0007010100fe8ffe3b000100018088ff1dc6d6ae7e00080202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010009020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c0101010100018088ff1d00000000000604fc87ff1d05fe87ff1d0100068088ff1d030002000702010100ff0109746573743a7369636b000209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0afe87ff1d02010100010100e89bd4f8b4bfe28540";
const DISEASE_FIRST_SAVE: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff01010102dd90d4c2cac3a7bbf401333333333333d33f018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0afe87ff1d030106fc8ffe3b0006010200fe8ffe3b0007010100fe8ffe3b000100018088ff1dc6d6ae7e00080202dd90d4c2cac3a7bbf40100dac7cacc04000902028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b01000a020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c0101010100018088ff1d00000000000604fc87ff1d05fe87ff1d0100068088ff1d068088ff1d02000702010100ff0109746573743a7369636b000212746573743a64756d6d795f64697365617365333333333333d33f0109746573743a626173657b14ae47e17a943f00020100000103010101000000000000000000122d0afe87ff1d02010100010100cd88edaadcb8e6e672";
const ON_OFF_ON_SAVE: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afe8ffe3b030106fc97fd590009010200fe97fd59000a010100fe97fd59000100018088ff1dc6d6ae7e000b0202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b01000c020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c010101018088ff1d018090fe3b00000000000607fc8ffe3b08fe8ffe3b0100098090fe3b030002000a03010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0afe87ff1d0201010000fe8ffe3b0301010001020089f583baa3cddefb5a02ed9cf7d6edf0d5b240";
const NO_DISEASE_HIS_DEATH: &str =
    "2bdac7cacc04010002020103000404fba99785c4c1bc5c8bbec9d8c5bd99888b01000409746573743a62617365";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e540108c2a9cec6d380a0832d0801ff010100028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afea7fb9501030105fcaffab301000f010200feaffab3010010010100feaffab301000100018088ff1ddac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010011020101abd4f19caceffaefab010000010000000203fba99785c4c1bc5c01010101c6ceaf60000000000000050dfca7fb95010efea7fb95010100030002000f02010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100000103010101000000000000000000122d0ac4ceaf600201010000050089f583baa3cddefb5a02b18baaceeca59d9d45048dbfb9dbe78393cf0f06f8cfc4cd9ffb9cb05508a5a0e1dadfa3b38fbc01";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn base() -> DeathCause {
    DeathCause {
        cause: Code::new("test:base").expect("valid"),
        per_year: FiniteF64::try_new(0.02).expect("finite"),
        applies_to: AppliesTo::Everyone,
    }
}

fn disease() -> DeathCause {
    DeathCause {
        cause: Code::new("test:dummy_disease").expect("valid"),
        per_year: FiniteF64::try_new(0.3).expect("finite"),
        applies_to: AppliesTo::DummyHomozygous,
    }
}

fn founder(born: i64, sex: Sex, maternal: Allele) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex,
        genotype: Genotype {
            maternal,
            paternal: Allele::Dummy,
        },
    }
}

/// "sick" with the switch `on` at the start and the causes in this order.
fn sick(on: bool, death_causes: Vec<DeathCause>) -> World {
    let mut world = World::new(common::data());
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:sick").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: on,
            },
            death_causes,
            founders: vec![
                founder(-1, Sex::Female, Allele::Wild),
                founder(-2, Sex::Male, Allele::Dummy),
            ],
            couples: Vec::new(),
            fertility: barren(),
        }))
        .expect("accepted");
    world
}

fn switch(on: bool) -> Input {
    Input::ForkOverride(ForkOverride {
        changes: vec![ParamChange::HereditaryDiseases(on)],
    })
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn new(world: World) -> Run {
        let mut run = Run {
            world,
            out: YearOut::default(),
            log: Vec::new(),
        };
        run.log = run.world.take_log();
        run
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    /// An override block that acts at `at`: stamped with the second before.
    fn switch(&mut self, at: i64, on: bool) {
        self.advance(at - 1);
        self.world.submit(switch(on)).expect("accepted");
        self.log.extend(self.world.take_log());
    }

    fn save(&self) -> Vec<u8> {
        self.world.save(&self.out.year_hashes).expect("saves")
    }

    /// (time, cause) of every death in the log.
    fn deaths(&self) -> Vec<(i64, String)> {
        self.log
            .iter()
            .filter_map(|entry| {
                let payload = serde_json::to_value(&entry.payload).expect("json");
                let cause = payload
                    .get("core:death")?
                    .get("cause")?
                    .as_str()?
                    .to_owned();
                Some((entry.t.as_secs(), cause))
            })
            .collect()
    }
}

/// "sick" with the switch `on` at the start, the causes in the usual order and an
/// override at each of `switches` (the second it acts, the new value).
fn drive(on: bool, switches: &[(i64, bool)], until: i64) -> Run {
    let mut run = Run::new(sick(on, vec![base(), disease()]));
    for &(at, value) in switches {
        run.switch(at, value);
    }
    run.advance(until);
    run
}

fn died(time: i64, code: &str) -> (i64, String) {
    (time, code.to_owned())
}

/// With the switch on he is exposed from the start and dies of the disease; she has one
/// dummy allele, no exposure and no death of the disease. With the switch off the
/// disease acts on nobody, and his exposure stays at nothing.
#[test]
fn the_disease_acts_only_on_the_homozygous_while_the_switch_is_on() {
    let on = Run::new(sick(true, vec![base(), disease()]));
    assert_eq!(on.save(), hex(SICK_ON_SAVE_AFTER_START));
    assert_eq!(
        on.world.pending_events(),
        6,
        "the year end series, two birthdays, her death of test:base and his two deaths"
    );
    let off = Run::new(sick(false, vec![base(), disease()]));
    assert_eq!(off.save(), hex(SICK_OFF_SAVE_AFTER_START));
    assert_eq!(off.world.pending_events(), 5);

    let on = drive(true, &[], 4 * Y - 1);
    assert_eq!(on.deaths(), [died(HIS_DISEASE, "test:dummy_disease")]);
    assert_eq!(on.save(), hex(SICK_ON_SAVE_AFTER_YEAR_3));
    let entry = on
        .log
        .iter()
        .find(|entry| entry.t.as_secs() == HIS_DISEASE)
        .expect("his death");
    assert_eq!(
        serde_json::to_string(entry).expect("json"),
        format!(
            r#"{{"id":10,"t":{HIS_DISEASE},"subjects":[{{"sim":2}}],"causes":[{{"plan":{{"by":3,"planned_at":0}}}},{{"random":{{"tag":"0000000000000004","subject":"00b8f20c40a5d4fb","occasion":"f4769e1ca855085d","detail":"0000000000000000"}}}}],"payload":{{"core:death":{{"cause":"test:dummy_disease"}}}}}}"#
        )
    );
    let off = drive(false, &[], 4 * Y - 1);
    assert_eq!(off.deaths(), []);
    assert_eq!(off.world.state().living(), 2);
}

/// Switched on at the start of year 1, the disease needs its whole budget from then on.
/// Its death is planned by the entry of the override; his death of `test:base`, earlier
/// in the list, keeps its plan and its insertion number.
#[test]
fn switching_on_plans_the_disease_from_the_budget() {
    let mut run = Run::new(sick(false, vec![base(), disease()]));
    run.switch(Y, true);
    assert_eq!(run.save(), hex(SWITCHED_ON_SAVE));
    run.advance(5 * Y - 1);
    assert_eq!(run.deaths(), [died(Y + HIS_DISEASE, "test:dummy_disease")]);
    let entry = run
        .log
        .iter()
        .rev()
        .find(|entry| entry.t.as_secs() == Y + HIS_DISEASE);
    let json = serde_json::to_string(entry.expect("his death")).expect("json");
    assert!(
        json.contains(r#"{"plan":{"by":6,"planned_at":31449600}}"#),
        "{json}"
    );
}

/// With the disease first in the list, his death of `test:base` comes after it: the
/// override plans it again, in the same second, so that the order of the causes holds
/// for his deaths.
#[test]
fn the_deaths_after_the_disease_are_planned_again() {
    let mut run = Run::new(sick(false, vec![disease(), base()]));
    run.switch(Y, true);
    assert_eq!(run.save(), hex(DISEASE_FIRST_SAVE));
    run.advance(5 * Y - 1);
    assert_eq!(run.deaths(), [died(Y + HIS_DISEASE, "test:dummy_disease")]);
}

/// The exposure adds up: on from the start, off at the start of year 1, on again at the
/// start of year 2. The year it was on counts, so he dies one year after the start of
/// year 1 plus his budget. No new draw: the time follows from the same budget. With two
/// closed periods (on in years 0 and 2) two years count.
#[test]
fn the_exposure_adds_up_over_the_periods_of_the_switch() {
    let mut run = Run::new(sick(true, vec![base(), disease()]));
    run.switch(Y, false);
    run.switch(2 * Y, true);
    assert_eq!(run.save(), hex(ON_OFF_ON_SAVE));
    run.advance(6 * Y - 1);
    assert_eq!(run.deaths(), [died(Y + HIS_DISEASE, "test:dummy_disease")]);

    let switches = [(Y, false), (2 * Y, true), (3 * Y, false), (4 * Y, true)];
    let two = drive(true, &switches, 7 * Y - 1);
    assert_eq!(
        two.deaths(),
        [died(2 * Y + HIS_DISEASE, "test:dummy_disease")]
    );
}

/// Without a hereditary cause the switch changes no rate: the override plans nothing
/// again, his death of `test:base` keeps the plan of his creation entry (3, at the
/// start).
#[test]
fn without_a_hereditary_cause_the_switch_plans_nothing() {
    let mut run = Run::new(sick(false, vec![base()]));
    run.switch(Y, true);
    run.advance(HIS_BASE);
    assert_eq!(run.deaths(), [died(HIS_BASE, "test:base")]);
    let entry = run.log.last().expect("his death");
    assert_eq!(
        postcard::to_allocvec(entry).expect("encodes"),
        hex(NO_DISEASE_HIS_DEATH)
    );
}

/// Switched off in the very second his death of the disease is due: the input acts
/// before the event, he lives with the budget used up. Switched on again, he dies in
/// that second.
#[test]
fn switched_off_in_the_second_of_death_the_budget_stays_used_up() {
    let mut run = drive(true, &[(HIS_DISEASE, false)], 5 * Y - 1);
    assert_eq!(run.deaths(), []);
    assert_eq!(run.world.state().living(), 2);
    assert_eq!(run.save(), hex(OFF_AT_DUE_SAVE));
    run.switch(5 * Y, true);
    run.advance(5 * Y);
    assert_eq!(run.deaths(), [died(5 * Y, "test:dummy_disease")]);
}

/// An override that keeps the switch as it is plans nothing: the projection is that of
/// the run without it, the log has its entry.
#[test]
fn an_override_that_keeps_the_switch_changes_nothing() {
    for on in [true, false] {
        let with = drive(on, &[(Y, on)], Y - 1);
        let without = drive(on, &[], Y - 1);
        assert_eq!(
            with.world.canonical_bytes(),
            without.world.canonical_bytes()
        );
        assert_eq!(with.log.len(), without.log.len() + 1);
    }
}

/// Switched on so that the disease and `test:base` are due in the second of his death
/// of `test:base`: the cause earlier in the list wins, whichever was planned first.
#[test]
fn of_the_disease_and_a_cause_for_everyone_due_together_the_first_in_the_list_wins() {
    for (causes, code) in [
        (vec![base(), disease()], "test:base"),
        (vec![disease(), base()], "test:dummy_disease"),
    ] {
        let mut run = Run::new(sick(false, causes));
        run.switch(TIE_SWITCH, true);
        run.advance(HIS_BASE);
        assert_eq!(run.deaths(), [died(HIS_BASE, code)], "{code}");
        assert_eq!(run.world.state().living(), 1);
    }
}

/// Loading and going on gives the uninterrupted run, with the switch on, off, on, off,
/// on (two closed periods), his death of the disease between, and off again after it:
/// the loaded world keeps the exposure and the plans, and the archive the exposure of
/// his death.
#[test]
fn save_load_continue_across_the_switch() {
    let switches = [
        (Y, false),
        (2 * Y, true),
        (3 * Y, false),
        (4 * Y, true),
        (6 * Y, false),
    ];
    let end = 7 * Y - 1;
    let whole = drive(true, &switches, end);
    assert_eq!(
        whole.deaths(),
        [died(2 * Y + HIS_DISEASE, "test:dummy_disease")]
    );
    for stop in [
        0,
        Y - 2,
        Y - 1,
        Y,
        2 * Y - 1,
        2 * Y,
        3 * Y,
        4 * Y - 1,
        4 * Y,
        2 * Y + HIS_DISEASE - 1,
        2 * Y + HIS_DISEASE,
        6 * Y - 1,
        6 * Y,
    ] {
        let before: Vec<(i64, bool)> = switches
            .iter()
            .copied()
            .filter(|s| s.0 - 1 <= stop)
            .collect();
        let after: Vec<(i64, bool)> = switches
            .iter()
            .copied()
            .filter(|s| s.0 - 1 > stop)
            .collect();
        let part = drive(true, &before, stop);
        let bytes = part.save();
        let (loaded, years) = World::load(&bytes, common::data()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut rest = Run {
            world: loaded,
            out: YearOut {
                census: part.out.census.clone(),
                year_hashes: years,
            },
            log: part.log,
        };
        for (at, value) in after {
            rest.switch(at, value);
        }
        rest.advance(end);
        assert_eq!(rest.out.year_hashes, whole.out.year_hashes, "stop {stop}");
        assert_eq!(rest.out.census, whole.out.census, "stop {stop}");
        assert_eq!(log_hash(&rest.log), log_hash(&whole.log), "stop {stop}");
        assert_eq!(
            rest.world.state_hash(),
            whole.world.state_hash(),
            "stop {stop}"
        );
    }
}
