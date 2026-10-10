//! A hereditary disease in the world (ADR-0017 section 3, C5): the disease of the walking
//! skeleton as test data (`test:dummy_disease` on `core:disease_ar`, penetrance 1, from
//! birth, 0.3 per year), a cause of death that acts only on sims with the allele twice,
//! and only while the switch of the hereditary diseases is on. Such a sim keeps its
//! exposure; an override that flips the switch plans its death of the disease from the
//! remaining budget, without a new draw (ADR-0001 section 6). The causes of `Start` come
//! first in the list of causes, the diseases after them (ADR-0017 section 2).
//!
//! The expected bytes and times come from `tools/state_reference.py`, which runs the
//! model of `tools/world_reference.py`, written from the ADRs and not from this code.
//!
//! Scenario "sick": seed 0xff, founder tag `test:sick`, `test:base` with 0.02 per year
//! for everyone, the test data with the dummy disease. A woman
//! (wild, dummy) born one second and a man (dummy twice) born two seconds before the
//! start. The disease needs about 3.21 years of exposure to kill him, `test:base` about
//! 19.6 years.

mod common;

use sim_core::data::Data;
use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
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
const SICK_ON_SAVE_AFTER_START: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010100018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0a01030106fc87ff1d0004010200fe87ff1d0002010100fe87ff1d000100018088ff1dc6ceaf6000060202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010007020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000001000000000000060300020001000300030002000401010100ff0109746573743a7369636b01000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a00";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010100018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0afe9ffc77030103fea7fb9501000d010100fea7fb9501000100018088ff1d8ea5b5f604000302018bbec9d8c5bd99888b01000e010101abd4f19caceffaefab0100000000010000000000000000000000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000001000000030bfe9ffc77010002000c01010100ff0109746573743a7369636b01000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a0400babeeeea98979f820602d080bf8e83c7c6b59f0104b8b3bc90f6defae8bf010698cacbc3a9d1bceb3d";
const SICK_OFF_SAVE_AFTER_START: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0a01030105fc87ff1d0004010200fe87ff1d0002010100fe87ff1d000100018088ff1ddac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010006020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000000000000000005030002000100030002000401010100ff0109746573743a7369636b00000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a00";
const SWITCHED_ON_SAVE: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010100018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0afe87ff1d030106fc8ffe3b0006010200fe8ffe3b0007010100fe8ffe3b000100018088ff1dc6d6ae7e00080202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010009020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf4010000000000000000010000000000000000018088ff1d00000000000604fc87ff1d05fe87ff1d0100068088ff1d030002000702010100ff0109746573743a7369636b00000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0afe87ff1d020101000101009f8cc8defcf1d1bccd01";
const ON_OFF_ON_SAVE: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010100018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0afe8ffe3b030106fc97fd590009010200fe97fd59000a010100fe97fd59000100018088ff1dc6d6ae7e000b0202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b01000c020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001333333333333d33f018090fe3b00000000000607fc8ffe3b08fe8ffe3b0100098090fe3b030002000a03010100ff0109746573743a7369636b01000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0afe87ff1d0201010000fe8ffe3b03010100010200babeeeea98979f820602bede8186a78c8bfe7c";
const NO_DISEASE_HIS_DEATH: &str =
    "2bdac7cacc04010002020103000404fba99785c4c1bc5c8bbec9d8c5bd99888b01000409746573743a62617365";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e540117bb9dbe9d8e94da853c0b01ff01010000018bbec9d8c5bd99888b017b14ae47e17a943f00010000000000000000122d0afea7fb9501030105fcaffab301000f010200feaffab3010010010100feaffab301000100018088ff1ddac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010011020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001c3992b7cb8d8ee3f000000000000050dfca7fb95010efea7fb95010100030002000f02010100ff0109746573743a7369636b01000109746573743a626173657b14ae47e17a943f00020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0ac4ceaf6002010100000500babeeeea98979f820602d080bf8e83c7c6b59f0104b8b3bc90f6defae8bf01068db6b288ead7bd94bd0108f5d7ac978ba5dfebe601";

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

/// The test data with the disease of the walking skeleton from birth on.
fn sick_data() -> Data {
    common::data_with_diseases(common::dummy_disease(0.0))
}

fn founder(born: i64, sex: Sex, maternal: u8) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: common::dummy(maternal, 1),
    }
}

/// "sick" with the switch `on` at the start, on the test data with the dummy disease.
fn sick(on: bool) -> World {
    sick_on(sick_data(), on)
}

/// "sick" with the switch `on` at the start and the cause `test:base`, on `data`.
fn sick_on(data: Data, on: bool) -> World {
    let mut world = World::new(data);
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:sick").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: on,
                nutrition: Vec::new(),
            },
            death_causes: vec![base()],
            founders: vec![founder(-1, Sex::Female, 0), founder(-2, Sex::Male, 1)],
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

/// "sick" with the switch `on` at the start and an override at each of `switches` (the
/// second it acts, the new value).
fn drive(on: bool, switches: &[(i64, bool)], until: i64) -> Run {
    let mut run = Run::new(sick(on));
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
    let on = Run::new(sick(true));
    assert_eq!(on.save(), hex(SICK_ON_SAVE_AFTER_START));
    assert_eq!(
        on.world.pending_events(),
        6,
        "the year end series, two birthdays, her death of test:base and his two deaths"
    );
    let off = Run::new(sick(false));
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
    let mut run = Run::new(sick(false));
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

/// The exposure adds up: on from the start, off at the start of year 1, on again at the
/// start of year 2. The year it was on counts, so he dies one year after the start of
/// year 1 plus his budget. No new draw: the time follows from the same budget. With two
/// closed periods (on in years 0 and 2) two years count.
#[test]
fn the_exposure_adds_up_over_the_periods_of_the_switch() {
    let mut run = Run::new(sick(true));
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

/// The causes of `Start` keep their plans when the switch flips: on the repository's data
/// he has `core:disease_ar`, whose death the override plans (`tools/state_reference.py`:
/// 645 730 542, after his death of `test:base`); his death of `test:base` keeps the plan
/// of his creation entry (3, at the start).
#[test]
fn a_switch_keeps_the_plans_of_the_causes_of_start() {
    let mut run = Run::new(sick_on(common::data(), false));
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
/// of `test:base`: the cause of `Start` wins, it comes first in the list of causes, though
/// the disease was planned later (ADR-0017 section 2).
#[test]
fn of_the_disease_and_a_cause_of_start_due_together_the_cause_of_start_wins() {
    let mut run = Run::new(sick(false));
    run.switch(TIE_SWITCH, true);
    run.advance(HIS_BASE);
    assert_eq!(run.deaths(), [died(HIS_BASE, "test:base")]);
    assert_eq!(run.world.state().living(), 1);
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
        let (loaded, years) = World::load(&bytes, sick_data()).expect("loads");
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

/// A budget used up before the hazard ends still kills (spec 7.1, review): the dummy
/// disease with 0.3 per year until 4 and none after; switched off in the second of his
/// death (about 3.2 years), he lives with the budget spent; switched on again at the start
/// of year 5, after the hazard ended, he dies in that very second. A save between loads.
#[test]
fn a_used_budget_kills_when_the_switch_goes_on_even_after_the_hazard_ended() {
    let number = |value: f64, unit: &str| serde_json::json!({"value": value, "unit": unit});
    let mut diseases = common::dummy_disease(0.0);
    diseases[0]["effect"] = serde_json::json!({"death_in_two_steps": {
        "before": number(0.3, "per_year"),
        "change_age": number(4.0, "year"),
        "after": number(0.0, "per_year")
    }});
    let data = common::data_with_diseases(diseases);
    let mut run = Run::new(sick_on(data.clone(), true));
    run.switch(HIS_DISEASE, false);
    run.advance(5 * Y - 1);
    assert_eq!(run.deaths(), []);
    let bytes = run.save();
    let (loaded, years) = World::load(&bytes, data).expect("loads");
    assert_eq!(loaded.save(&years).expect("saves"), bytes);
    run.switch(5 * Y, true);
    run.advance(5 * Y);
    assert_eq!(run.deaths(), [died(5 * Y, "test:dummy_disease")]);
}
