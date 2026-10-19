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
//! Scenario "sick": seed 0xff, founder tag `test:sick`, test data with accident 0.02 per
//! year, senescence practically off, and the dummy disease. A woman
//! (wild, dummy) born one second and a man (dummy twice) born two seconds before the
//! start. The disease needs about 3.21 years of exposure to kill him, accident about
//! 58.8 years.

mod common;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::input::{ForkOverride, Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;

// tools/state_reference.py
/// When he dies of the disease (switch on from the start) and of accident, in seconds.
const HIS_DISEASE: i64 = 101_053_347;
const HIS_ACCIDENT: i64 = 1_850_439_351;
/// Switched on in this second, the disease and accident are due in the same second.
const TIE_SWITCH: i64 = 1_749_386_004;
/// His death of `core:disease_ar` on the data without the dummy disease, switched on at
/// the start of year 1.
const HIS_AR: i64 = 645_730_542;
const SICK_ON_SAVE_AFTER_START: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010100010301030b80a8a7020002060180a8a702fc87ff1d0006010200fe87ff1d0003010100fe87ff1d000100018088ff1dc6ceaf6000090202dd90d4c2cac3a7bbf40100fc8fef9b04000b070200fe9fedd704000a070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c000c020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf4010000000000000000010000000000000000010000000002000000000000020100000200000000000b010003000200010003000300020003000200030002000601010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f617202010100000000";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010100fe9ffc770301030680a0fc770002060180a8a702fea7fb95010012010100fea7fb9501000100018088ff1dfe9fedd704000a070100a695b0d0280005020185d2bc82f8b2ead44d00dc84cbc8823300040201a0bb8ca1c58498a01c0013010101abd4f19caceffaefab010000000001000000000000000000000100000000000002010000020001c6ceaf6000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000001000000000200000000000601000dfe9ffc7701000200020002000e01010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010000000400fdacb382f5f9b0b16802e7859ec4ac85ebb29c0104c1ea83e4f6c7a5a792010698f9d4d3868dc3b501";
const SICK_OFF_SAVE_AFTER_START: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010000010301030a80a8a7020002060180a8a702fc87ff1d0006010200fe87ff1d0003010100fe87ff1d000100018088ff1dfc8fef9b04000a070200fe9fedd7040009070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c000b020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf40100000000000000000100000000000000000000000002000000000000020100000200000000000a01000300020001000300020003000200030002000601010100ff0109746573743a7369636b0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f617202010100000000";
const SWITCHED_ON_SAVE: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010100fe87ff1d0301030b8088ff1d0002060180a8a702fc8ffe3b000b010200fe8ffe3b000c010100fe8ffe3b000100018088ff1dc6d6ae7e000d0202dd90d4c2cac3a7bbf40100fc8fef9b04000a070200fe9fedd7040009070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c000e020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf4010000000000000000010000000000000000018088ff1d00000002000000000000020100000200000000000b010006fc87ff1d07fe87ff1d0100088088ff1d0300020003000200030002000902010100ff0109746573743a7369636b0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000fe87ff1d0201010001010088afc4a7f58dd4cd56";
const ON_OFF_ON_SAVE: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010100fe8ffe3b0301030b8090fe3b0002060180a8a702fc97fd59000e010200fe97fd59000f010100fe97fd59000100018088ff1dc6d6ae7e00100202dd90d4c2cac3a7bbf40100fc8fef9b04000b070200fe9fedd704000a070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c0011020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001333333333333d33f018090fe3b00000002000000000000020100000200000000000b010009fc8ffe3b0afe8ffe3b01000b8090fe3b0300020003000200030002000c03010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000fe87ff1d0201010000fe8ffe3b03010100010200fdacb382f5f9b0b16802ce9ff69accb0f4badc01";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e540229b5c9adf9d0a9a1ea8d011201ff01010000fea7fb95010301030a80a8fb95010002060180a8a702fcaffab3010014010200feaffab3010015010100feaffab301000100018088ff1dfc8fef9b04000b070200fe9fedd704000a070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c0016020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001c3992b7cb8d8ee3f0000000002000000000000020100000200000000000a01000ffca7fb950110fea7fb950101000300020003000200030002001102010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000c4ceaf6002010100000500fdacb382f5f9b0b16802e7859ec4ac85ebb29c0104c1ea83e4f6c7a5a7920106cba5c296f4849f847c088cb6f0c3a4b282d28101";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

/// The test data with the disease of the walking skeleton from birth on, accident 0.02.
fn sick_data() -> Data {
    common::death_data(0.02, None, common::dummy_disease(0.0))
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

/// "sick" with the switch `on` at the start, on `data`.
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
            founders: vec![founder(-1, Sex::Female, 0), founder(-2, Sex::Male, 1)],
            couples: Vec::new(),
            capacity: None,
            pool: None,
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
        11,
        "the series of the year end and the rounds, two birthdays, her two deaths (senescence and accident), his three and their two leavings of home"
    );
    let off = Run::new(sick(false));
    assert_eq!(off.save(), hex(SICK_OFF_SAVE_AFTER_START));
    assert_eq!(off.world.pending_events(), 10);

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
            r#"{{"id":12,"t":{HIS_DISEASE},"subjects":[{{"sim":2}}],"causes":[{{"plan":{{"by":3,"planned_at":0}}}},{{"random":{{"tag":"0000000000000004","subject":"00b8f20c40a5d4fb","occasion":"f4769e1ca855085d","detail":"0000000000000000"}}}}],"payload":{{"core:death":{{"cause":"test:dummy_disease"}}}}}}"#
        )
    );
    let off = drive(false, &[], 4 * Y - 1);
    assert_eq!(off.deaths(), []);
    assert_eq!(off.world.state().living(), 2);
}

/// Switched on at the start of year 1, the disease needs its whole budget from then on.
/// Its death is planned by the entry of the override; his deaths of senescence and accident, earlier
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
        json.contains(r#"{"plan":{"by":8,"planned_at":31449600}}"#),
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

/// Senescence and accident keep their plans when the switch flips: on the data with the
/// repository's diseases he has `core:disease_ar`, whose death the override (entry 6, at
/// the start of year 1) plans and which comes first (`tools/state_reference.py`).
#[test]
fn a_switch_plans_only_the_diseases_again() {
    let data = common::death_data(0.02, None, common::repository_diseases());
    let mut run = Run::new(sick_on(data, false));
    run.switch(Y, true);
    run.advance(HIS_AR);
    assert_eq!(run.deaths(), [died(HIS_AR, "core:disease_ar")]);
    let entry = run.log.last().expect("his death");
    let json = serde_json::to_string(entry).expect("json");
    assert!(
        json.contains(r#"{"plan":{"by":8,"planned_at":31449600}}"#),
        "{json}"
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

/// Switched on so that the disease and accident are due in the second of his death of
/// accident: accident wins, it comes first in the list of causes, though the disease was
/// planned later (ADR-0017 section 2). In these 58.8 years she turns 16 and the market
/// pairs them (month 297); two of their children die of accident before him
/// (`tools/state_reference.py`); his is the only death in that second.
#[test]
fn of_the_disease_and_accident_due_together_accident_wins() {
    let mut run = Run::new(sick(false));
    run.switch(TIE_SWITCH, true);
    run.advance(HIS_ACCIDENT);
    let in_that_second: Vec<(i64, String)> = run
        .deaths()
        .into_iter()
        .filter(|(t, _)| *t == HIS_ACCIDENT)
        .collect();
    assert_eq!(in_that_second, [died(HIS_ACCIDENT, "core:accident")]);
    let him = run
        .world
        .pedigree()
        .persons
        .into_iter()
        .find(|person| person.sim.get() == 2)
        .expect("known");
    assert_eq!(him.died.map(|t| t.as_secs()), Some(HIS_ACCIDENT));
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
                trace: Vec::new(),
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
