//! The onset of a hereditary disease in the log (ADR-0017 section 3, addendum of
//! 2026-10-20): one entry `core:disease_onset` per person and disease, in the second the
//! person becomes affected in the run. A creation whose onset second
//! `B = born + ⌈onset · YEAR⌉` is reached writes the entry right after the person's creation
//! entry if the switch is on (path 1); a later `B` gets an event in `B`, planned whatever the
//! switch, that writes the entry if the switch is on then (path 2).
//!
//! The expected values by hand from the addendum, on test data with two visible diseases of
//! fixed onset and penetrance 1 (`test:onset_birth` at the recessive `core:disease_ar`, onset
//! 0; `test:onset_late` at the dominant `core:disease_ad_late`, onset 30) where nobody dies
//! unless a test says so. A year is 31 449 600 seconds and 30 years exactly 943 488 000, so a
//! founder born at `−40 · YEAR` reaches `test:onset_late` at `−10 · YEAR`, in year −10, and
//! one born at `−10 · YEAR` at `20 · YEAR`. The ids of the log count from 1. The properties
//! of a town run from the pool follow from the addendum alone; the card, tested against the
//! reference, gives the year and the age of the living.

mod common;

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::input::{Couple, ForkOverride, Founder, Input, PoolStart, Start};
use sim_core::log::LogEntry;
use sim_core::params::{ParamChange, WorldParams};
use sim_core::presentation::card::DiseaseStatus;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const BIRTH: &str = "test:onset_birth";
const LATE: &str = "test:onset_late";
const ONSET: &str = "core:disease_onset";

/// A visible disease at `locus` with penetrance 1 and the fixed onset `onset` in years.
fn visible(code: &str, locus: &str, onset: f64) -> Value {
    let number = |value: f64, unit: &str| json!({"value": value, "unit": unit});
    json!({
        "code": code,
        "carrier": {"locus": {
            "locus": locus,
            "penetrance_female": number(1.0, "fraction"),
            "penetrance_male": number(1.0, "fraction")
        }},
        "onset": {"fixed": {"age": number(onset, "year")}},
        "effect": "visible"
    })
}

/// The test data: the two visible diseases in this order, accident `accident` per year,
/// senescence practically off, the fertility of the old model at 0.25 per attempt; with
/// `twins` every conception gives twins (identical 0.3, fraternal 0.7).
fn data(accident: f64, twins: bool) -> Data {
    let diseases = json!([
        visible(BIRTH, "core:disease_ar", 0.0),
        visible(LATE, "core:disease_ad_late", 30.0),
    ]);
    let (mut calibration, genome_map) = common::death_files(accident, None, diseases);
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 0.25);
    if twins {
        let values = &mut demography["values"];
        values["core:twins_identical"]["value"] = json!(0.3);
        values["core:twins_fraternal"]["value"] = json!(0.7);
    }
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

/// A founder born at `born` with `core:disease_ar` twice (`birth`) and `core:disease_ad_late`
/// once (`late`), in the order of the map.
fn founder(born: i64, sex: Sex, birth: bool, late: bool) -> Founder {
    let locus = |id: &str, copies: Vec<u8>| LocusAlleles {
        locus: DataId::new(id).expect("id"),
        copies,
    };
    let mut alleles = Vec::new();
    if birth {
        alleles.push(locus("core:disease_ar", vec![1, 1]));
    }
    if late {
        alleles.push(locus("core:disease_ad_late", vec![0, 1]));
    }
    Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles,
    }
}

fn started(data: Data, on: bool, founders: Vec<Founder>, couples: Vec<Couple>) -> World {
    let mut world = World::new(data);
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:onset").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: on,
                nutrition: Vec::new(),
            },
            founders,
            couples,
            capacity: None,
            pool: None,
        }))
        .expect("accepted");
    world
}

/// A run and its whole log.
struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn new(mut world: World) -> Run {
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

    /// A fork that sets the switch, stamped with the last closed second.
    fn fork(&mut self, on: bool) {
        self.world
            .submit(Input::ForkOverride(ForkOverride {
                changes: vec![ParamChange::HereditaryDiseases(on)],
            }))
            .expect("accepted");
        self.log.extend(self.world.take_log());
    }

    fn entries(&self) -> Vec<Value> {
        (self.log.iter())
            .map(|entry| serde_json::to_value(entry).expect("json"))
            .collect()
    }
}

/// The code of an entry's payload.
fn code(entry: &Value) -> &str {
    let payload = entry["payload"].as_object().expect("a payload");
    assert_eq!(payload.len(), 1);
    payload.keys().next().expect("a code")
}

fn sim(entry: &Value) -> u64 {
    entry["subjects"][0]["sim"].as_u64().expect("a sim")
}

/// The entry of an onset, without its id.
fn onset(
    t: i64,
    sim: u64,
    disease: &str,
    (since_year, age_years): (i64, i64),
    cause: Value,
) -> Value {
    json!({
        "t": t,
        "subjects": [{"sim": sim}],
        "causes": [cause],
        "payload": {ONSET: {"disease": disease, "since_year": since_year, "age_years": age_years}},
    })
}

/// The onset entries of `entries`, without their ids.
fn onsets(entries: &[Value]) -> Vec<Value> {
    (entries.iter())
        .filter(|entry| code(entry) == ONSET)
        .map(|entry| {
            let mut entry = entry.clone();
            entry.as_object_mut().expect("an entry").remove("id");
            entry
        })
        .collect()
}

/// The id of the creation entry of founder `sim`.
fn added(entries: &[Value], sim: u64) -> u64 {
    (entries.iter())
        .find(|entry| code(entry) == "core:founder_added" && self::sim(entry) == sim)
        .and_then(|entry| entry["id"].as_u64())
        .expect("added")
}

/// Path 1: a founder of 40 with the onset 30 has reached it at the start. With the switch
/// on its entry comes right after its `core:founder_added`, caused by it, in the second of
/// the start (0): the year of `B = −10 · YEAR` is −10, the onset 30 whole years. Never again.
#[test]
fn a_founder_past_the_onset_gets_its_entry_right_after_being_added() {
    let mut run = Run::new(started(
        data(1e-9, false),
        true,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    let entries = run.entries();
    let at = (entries.iter())
        .position(|entry| code(entry) == "core:founder_added")
        .expect("added");
    let id = entries[at]["id"].as_u64().expect("an id");
    let mut expected = onset(0, 1, LATE, (-10, 30), json!({"event": id}));
    expected["id"] = json!(id + 1);
    assert_eq!(entries[at + 1], expected);
    run.advance(60 * Y);
    assert_eq!(onsets(&run.entries()).len(), 1, "never again");
}

/// A founder born at `−30 · YEAR` reaches the onset 30 in the second of the start, `B = E =
/// 0`: path 1, so nothing is planned for it (as many events as for a founder without the
/// disease), and with the switch on its entry comes at the start, in year 0 at 30 years. The
/// save right after the start loads, the switch on or off.
#[test]
fn an_onset_in_the_second_of_the_start_is_reached_at_the_creation() {
    for on in [true, false] {
        let of = |late| {
            started(
                data(1e-9, false),
                on,
                vec![founder(-30 * Y, Sex::Female, false, late)],
                Vec::new(),
            )
        };
        let (world, healthy) = (of(true), of(false));
        assert_eq!(world.pending_events(), healthy.pending_events(), "{on}");
        let run = Run::new(world);
        let bytes = run.world.save(&[]).expect("saves");
        let entries = run.entries();
        let expected = if on {
            vec![onset(
                0,
                1,
                LATE,
                (0, 30),
                json!({"event": added(&entries, 1)}),
            )]
        } else {
            Vec::new()
        };
        assert_eq!(onsets(&entries), expected, "{on}");
        assert!(World::load(&bytes, data(1e-9, false)).is_ok(), "{on}");
    }
}

/// With the switch off at the start the reached onset writes nothing and plans nothing: the
/// queue holds as many events as for a founder without the disease, and in 60 years no
/// entry comes (path 3 would need a fork).
#[test]
fn with_the_switch_off_a_reached_onset_is_neither_written_nor_planned() {
    let world = started(
        data(1e-9, false),
        false,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    );
    let healthy = started(
        data(1e-9, false),
        false,
        vec![founder(-40 * Y, Sex::Female, false, false)],
        Vec::new(),
    );
    assert_eq!(world.pending_events(), healthy.pending_events());
    let mut run = Run::new(world);
    run.advance(60 * Y);
    assert_eq!(onsets(&run.entries()), Vec::<Value>::new());
}

/// Path 2: a founder of 10 with the onset 30 reaches it in `20 · YEAR`. The start plans one
/// event there whatever the switch (one more than for a founder without the disease, until
/// it has run); with the switch on it writes the entry, caused by its plan (`by` the
/// founder's creation entry, planned in the second of the start), in year 20 at 30 years;
/// with the switch off it writes nothing.
#[test]
fn an_onset_after_the_start_is_planned_whatever_the_switch() {
    for on in [true, false] {
        let run_of = |late| {
            Run::new(started(
                data(1e-9, false),
                on,
                vec![founder(-10 * Y, Sex::Female, false, late)],
                Vec::new(),
            ))
        };
        let (mut run, mut healthy) = (run_of(true), run_of(false));
        let pending = |run: &Run| run.world.pending_events();
        assert_eq!(pending(&run), pending(&healthy) + 1, "{on}");
        run.advance(20 * Y - 1);
        healthy.advance(20 * Y - 1);
        assert_eq!(onsets(&run.entries()), Vec::<Value>::new(), "{on}");
        assert_eq!(pending(&run), pending(&healthy) + 1, "{on}");
        run.advance(20 * Y);
        healthy.advance(20 * Y);
        assert_eq!(pending(&run), pending(&healthy), "{on}");
        let by = added(&run.entries(), 1);
        let plan = json!({"plan": {"by": by, "planned_at": 0}});
        let expected = if on {
            vec![onset(20 * Y, 1, LATE, (20, 30), plan)]
        } else {
            Vec::new()
        };
        assert_eq!(onsets(&run.entries()), expected, "{on}");
    }
}

/// Two founders of 40 with both diseases: founder 1, its onsets in the order of the data,
/// then founder 2 and its onsets, each caused by its own creation entry. `test:onset_birth`
/// began at the birth, `−40 · YEAR`, in year −40 at 0 years.
#[test]
fn founders_and_their_diseases_come_in_creation_and_data_order() {
    let run = Run::new(started(
        data(1e-9, false),
        true,
        vec![
            founder(-40 * Y, Sex::Female, true, true),
            founder(-40 * Y, Sex::Female, true, true),
        ],
        Vec::new(),
    ));
    let entries = run.entries();
    let line = |entry: &Value| {
        let payload = &entry["payload"][code(entry)];
        match code(entry) {
            "core:started" => "started".to_owned(),
            "core:founder_added" => format!("{} added {}", entry["id"], sim(entry)),
            ONSET => format!(
                "{} onset {} {} {} {} after {}",
                entry["id"],
                sim(entry),
                payload["disease"].as_str().expect("a code"),
                payload["since_year"],
                payload["age_years"],
                entry["causes"][0]["event"],
            ),
            other => other.to_owned(),
        }
    };
    let head: Vec<String> = entries.iter().take(7).map(line).collect();
    assert_eq!(
        head,
        [
            "started".to_owned(),
            "2 added 1".to_owned(),
            format!("3 onset 1 {BIRTH} -40 0 after 2"),
            format!("4 onset 1 {LATE} -10 30 after 2"),
            "5 added 2".to_owned(),
            format!("6 onset 2 {BIRTH} -40 0 after 5"),
            format!("7 onset 2 {LATE} -10 30 after 5"),
        ]
    );
}

/// A fork stamped `B − 1` that turns the switch on takes effect in `B`: the event there finds
/// it on and writes the entry, caused by its plan. A fork that turns it off before `B` leaves
/// the event nothing to write.
#[test]
fn a_fork_before_the_onset_decides_what_its_event_finds() {
    let founders = || vec![founder(-10 * Y, Sex::Female, false, true)];
    let mut on = Run::new(started(data(1e-9, false), false, founders(), Vec::new()));
    on.advance(20 * Y - 1);
    on.fork(true);
    on.advance(25 * Y);
    let by = added(&on.entries(), 1);
    assert_eq!(
        onsets(&on.entries()),
        [onset(
            20 * Y,
            1,
            LATE,
            (20, 30),
            json!({"plan": {"by": by, "planned_at": 0}})
        )]
    );
    let mut off = Run::new(started(data(1e-9, false), true, founders(), Vec::new()));
    off.advance(5 * Y);
    off.fork(false);
    off.advance(25 * Y);
    assert_eq!(onsets(&off.entries()), Vec::<Value>::new());
}

/// A child of two parents with `core:disease_ar` twice has it twice too (the disease loci
/// mutate only towards the alternative allele): `test:onset_birth` begins at its birth. Every
/// birth entry is followed by the child's onset entry, caused by the birth, in its second
/// (the year of the birth, 0 years); twins come as birth A, onset A, birth B, onset B.
#[test]
fn a_child_with_an_onset_at_birth_gets_its_entry_right_after_its_birth_entry() {
    let mut run = Run::new(started(
        data(1e-9, true),
        true,
        vec![
            founder(-20 * Y, Sex::Female, true, false),
            founder(-22 * Y, Sex::Male, true, false),
        ],
        vec![Couple { woman: 0, man: 1 }],
    ));
    run.advance(10 * Y);
    let entries = run.entries();
    let births: Vec<usize> = (0..entries.len())
        .filter(|&at| code(&entries[at]) == "core:birth")
        .collect();
    assert!(!births.is_empty(), "children are born");
    for &at in &births {
        let birth = &entries[at];
        let t = birth["t"].as_i64().expect("a second");
        let id = birth["id"].as_u64().expect("an id");
        let mut expected = onset(
            t,
            sim(birth),
            BIRTH,
            (t.div_euclid(Y), 0),
            json!({"event": id}),
        );
        expected["id"] = json!(id + 1);
        assert_eq!(entries[at + 1], expected);
    }
    let twins = (births.windows(2))
        .find(|pair| entries[pair[0]]["t"] == entries[pair[1]]["t"])
        .expect("twins");
    let t = &entries[twins[0]]["t"];
    let (a, b) = (sim(&entries[twins[0]]), sim(&entries[twins[1]]));
    let second: Vec<(&str, u64)> = (entries.iter())
        .filter(|entry| &entry["t"] == t && matches!(code(entry), "core:birth" | ONSET))
        .map(|entry| (code(entry), sim(entry)))
        .collect();
    assert_eq!(
        second,
        [("core:birth", a), (ONSET, a), ("core:birth", b), (ONSET, b)]
    );
}

/// Death takes the planned onset with the person's other plans: of six founders of 10 with
/// the onset 30 (in `20 · YEAR`) and an accident of 0.1 per year, those who died before
/// `20 · YEAR` never get an entry, the others get theirs in `20 · YEAR`.
#[test]
fn nobody_gets_an_onset_after_leaving() {
    let founders = (0..6)
        .map(|_| founder(-10 * Y, Sex::Female, false, true))
        .collect();
    let mut run = Run::new(started(data(0.1, false), true, founders, Vec::new()));
    run.advance(40 * Y);
    let written: BTreeMap<u64, i64> = (onsets(&run.entries()).iter())
        .map(|entry| (sim(entry), entry["t"].as_i64().expect("a second")))
        .collect();
    let pedigree = run.world.pedigree();
    let mut early = 0;
    for person in &pedigree.persons {
        let left = person.died.or(person.emigrated).map(SimTime::as_secs);
        assert_ne!(left, Some(20 * Y), "no tie with the onset");
        let expected = match left {
            Some(left) if left < 20 * Y => {
                early += 1;
                None
            }
            _ => Some(20 * Y),
        };
        assert_eq!(written.get(&person.sim.get()).copied(), expected);
    }
    assert!(early > 0, "someone died before the onset");
}

/// A town from the pool of the repository's data (the start of `tests/scenarios/town.json`,
/// the switch on throughout, arrivals every year) over 30 years: at most one entry per
/// person and disease; one caused by an entry comes in the second of the person's creation
/// entry (founder, birth or arrival), after it with only the person's other onsets between;
/// one caused by a plan comes after the second of the plan, which the creation entry made;
/// none after the person died or left; and every living person has an entry for exactly the
/// diseases its card shows as affected, with the card's year and whole years.
#[test]
fn a_town_writes_each_onset_once_where_the_addendum_says() {
    let data = common::repository_data();
    let pool = PoolStart::drawn(&data, 1, 100, true).expect("a pool");
    let mut world = World::new(data);
    world
        .submit(Input::Start(Start {
            world_seed: 1,
            founder_tag: DataId::new("test:town").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: true,
                nutrition: Vec::new(),
            },
            founders: Vec::new(),
            couples: Vec::new(),
            capacity: None,
            pool: Some(pool),
        }))
        .expect("accepted");
    let mut run = Run::new(world);
    run.advance(943_487_999);
    let entries = run.entries();
    let by_id: BTreeMap<u64, usize> = (0..entries.len())
        .map(|at| (entries[at]["id"].as_u64().expect("an id"), at))
        .collect();
    let creation = |id: &Value, sim: u64| {
        let at = by_id[&id.as_u64().expect("an id")];
        let entry = &entries[at];
        let made = matches!(
            code(entry),
            "core:founder_added" | "core:birth" | "core:arrived"
        );
        assert!(made && self::sim(entry) == sim, "{entry}");
        at
    };
    let pedigree = run.world.pedigree();
    let left: BTreeMap<u64, i64> = (pedigree.persons.iter())
        .filter_map(|person| {
            let left = person.died.or(person.emigrated)?;
            Some((person.sim.get(), left.as_secs()))
        })
        .collect();
    let mut seen = BTreeMap::new();
    for (at, entry) in entries.iter().enumerate() {
        if code(entry) != ONSET {
            continue;
        }
        let (sim, t) = (sim(entry), entry["t"].as_i64().expect("a second"));
        let payload = &entry["payload"][ONSET];
        let disease = payload["disease"].as_str().expect("a code").to_owned();
        let once = (
            payload["since_year"].as_i64(),
            payload["age_years"].as_i64(),
        );
        assert!(seen.insert((sim, disease), once).is_none(), "{entry}");
        assert!(left.get(&sim).is_none_or(|left| t <= *left), "{entry}");
        let cause = &entry["causes"][0];
        if let Some(event) = cause.get("event") {
            let made = creation(event, sim);
            assert_eq!(entries[made]["t"], entry["t"]);
            assert!(
                (made + 1..at).all(|between| code(&entries[between]) == ONSET
                    && self::sim(&entries[between]) == sim)
            );
        } else {
            let plan = &cause["plan"];
            let made = creation(&plan["by"], sim);
            assert_eq!(entries[made]["t"], plan["planned_at"]);
            assert!(
                t > plan["planned_at"].as_i64().expect("a second"),
                "{entry}"
            );
        }
    }
    assert!(!seen.is_empty(), "the town has diseases");
    for person in pedigree
        .persons
        .iter()
        .filter(|p| !left.contains_key(&p.sim.get()))
    {
        let card = run.world.genetics_card(person.sim).expect("a card");
        for disease in &card.diseases {
            let key = (person.sim.get(), disease.disease.as_str().to_owned());
            let expected = match disease.status {
                DiseaseStatus::Affected { year, age_years } => Some((Some(year), Some(age_years))),
                _ => None,
            };
            assert_eq!(seen.get(&key).copied(), expected, "{key:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------
// Path 3: a fork that turns the switch on

/// The id of the first entry `core:param_changed` of `entries` in the second `t`.
fn changed(entries: &[Value], t: i64) -> usize {
    (entries.iter())
        .position(|entry| code(entry) == "core:param_changed" && entry["t"] == t)
        .expect("changed")
}

/// Path 3: a founder of 40 whose start left the switch off has reached the onset 30 at
/// `−10 · YEAR`. A fork turning it on, stamped `5 · YEAR − 1`, writes the entry in
/// `5 · YEAR` in its own context, right after its `core:param_changed` and caused by it
/// (input 2), with the year of `B` (−10) and 30 years. Never again: off and on again in year
/// 8, then 60 years.
#[test]
fn a_fork_turning_the_switch_on_writes_a_reached_onset() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    run.advance(5 * Y - 1);
    run.fork(true);
    let entries = run.entries();
    let at = changed(&entries, 5 * Y);
    let mut expected = onset(5 * Y, 1, LATE, (-10, 30), json!({"input": 2}));
    expected["id"] = json!(entries[at]["id"].as_u64().expect("an id") + 1);
    assert_eq!(entries.get(at + 1), Some(&expected));
    run.advance(8 * Y - 1);
    run.fork(false);
    run.fork(true);
    run.advance(60 * Y);
    assert_eq!(onsets(&run.entries()).len(), 1, "never again");
}

/// "On, off, on" with one stamp writes the onset only at the first "on": the second finds
/// an earlier input of this path that turned the switch on while the onset was reached.
#[test]
fn on_off_on_in_one_second_writes_the_onset_once() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    run.advance(5 * Y - 1);
    run.fork(true);
    run.fork(false);
    run.fork(true);
    let entries = run.entries();
    let at = changed(&entries, 5 * Y);
    assert_eq!(code(&entries[at + 1]), ONSET);
    assert_eq!(
        onsets(&entries),
        [onset(5 * Y, 1, LATE, (-10, 30), json!({"input": 2}))]
    );
}

/// "On, off" stamped `B − 1` while the onset's own event is still open counts for nothing:
/// the event in `B = 20 · YEAR` finds the switch off and writes nothing, and neither input
/// writes (its event was open). A later "on", stamped `25 · YEAR − 1` (input 4), writes it in
/// `25 · YEAR`, in year 20 at 30 years.
#[test]
fn on_and_off_before_an_open_event_leave_the_onset_to_a_later_on() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![founder(-10 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    run.advance(20 * Y - 1);
    run.fork(true);
    run.fork(false);
    run.advance(25 * Y - 1);
    assert_eq!(onsets(&run.entries()), Vec::<Value>::new());
    run.fork(true);
    assert_eq!(
        onsets(&run.entries()),
        [onset(25 * Y, 1, LATE, (20, 30), json!({"input": 4}))]
    );
}

/// A founder whose start left the switch off gets the onset at a fork with the start's
/// stamp (submitted right after `Start`, acting in second 0): in second 0, after the fork's
/// `core:param_changed`, caused by input 2.
#[test]
fn a_fork_with_the_stamp_of_the_start_writes_the_founders_onset() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    run.fork(true);
    let entries = run.entries();
    let at = changed(&entries, 0);
    let mut expected = onset(0, 1, LATE, (-10, 30), json!({"input": 2}));
    expected["id"] = json!(entries[at]["id"].as_u64().expect("an id") + 1);
    assert_eq!(entries.get(at + 1), Some(&expected));
}

/// The entries of one fork go by SimId, each person's in the order of the data: founder 1
/// with `test:onset_birth` (begun at her birth, year −40, 0 years) and `test:onset_late`
/// (year −10, 30 years), then founder 2 the same, all caused by the fork.
#[test]
fn a_fork_writes_by_sim_and_in_data_order() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![
            founder(-40 * Y, Sex::Female, true, true),
            founder(-40 * Y, Sex::Female, true, true),
        ],
        Vec::new(),
    ));
    run.advance(5 * Y - 1);
    run.fork(true);
    let entries = run.entries();
    let at = changed(&entries, 5 * Y);
    let fork = json!({"input": 2});
    let expected = [
        onset(5 * Y, 1, BIRTH, (-40, 0), fork.clone()),
        onset(5 * Y, 1, LATE, (-10, 30), fork.clone()),
        onset(5 * Y, 2, BIRTH, (-40, 0), fork.clone()),
        onset(5 * Y, 2, LATE, (-10, 30), fork),
    ];
    let after = entries.get(at + 1..at + 5).unwrap_or_default();
    assert_eq!(onsets(after), expected);
    assert_eq!(onsets(&entries).len(), 4);
}

/// A fork writes only what no earlier moment could report with the switch on: with the
/// switch on at the start the creation wrote it, and "on" again writes nothing; an event that
/// found the switch on wrote it, and off then on writes nothing; an event that found it off
/// leaves it to the next "on" (input 2 in `22 · YEAR`, year 20, 30 years).
#[test]
fn a_fork_writes_only_what_no_earlier_moment_reported() {
    let mut on_at_start = Run::new(started(
        data(1e-9, false),
        true,
        vec![founder(-40 * Y, Sex::Female, false, true)],
        Vec::new(),
    ));
    on_at_start.advance(5 * Y - 1);
    on_at_start.fork(true);
    let entries = on_at_start.entries();
    assert_eq!(onsets(&entries).len(), 1, "the creation's");
    assert!(
        entries[changed(&entries, 5 * Y)..]
            .iter()
            .all(|e| code(e) != ONSET)
    );

    let late = || vec![founder(-10 * Y, Sex::Female, false, true)];
    let mut found_on = Run::new(started(data(1e-9, false), true, late(), Vec::new()));
    found_on.advance(22 * Y - 1);
    found_on.fork(false);
    found_on.advance(24 * Y - 1);
    found_on.fork(true);
    let by = added(&found_on.entries(), 1);
    assert_eq!(
        onsets(&found_on.entries()),
        [onset(
            20 * Y,
            1,
            LATE,
            (20, 30),
            json!({"plan": {"by": by, "planned_at": 0}})
        )]
    );

    let mut found_off = Run::new(started(data(1e-9, false), false, late(), Vec::new()));
    found_off.advance(22 * Y - 1);
    found_off.fork(true);
    assert_eq!(
        onsets(&found_off.entries()),
        [onset(22 * Y, 1, LATE, (20, 30), json!({"input": 2}))]
    );
}

/// Path 3 sees only the active population: of eight founders of 40 with the onset reached
/// and an accident of 0.1 per year, a fork in `10 · YEAR` writes for exactly those who live
/// then (the input acts before the events of its second).
#[test]
fn a_fork_writes_nothing_for_who_has_left() {
    let founders = (0..8)
        .map(|_| founder(-40 * Y, Sex::Female, false, true))
        .collect();
    let mut run = Run::new(started(data(0.1, false), false, founders, Vec::new()));
    run.advance(10 * Y - 1);
    run.fork(true);
    let written: BTreeMap<u64, i64> = (onsets(&run.entries()).iter())
        .map(|entry| (sim(entry), entry["t"].as_i64().expect("a second")))
        .collect();
    let pedigree = run.world.pedigree();
    let (mut gone, mut living) = (0, 0);
    for person in &pedigree.persons {
        let left = person.died.or(person.emigrated).map(SimTime::as_secs);
        let expected = match left {
            Some(left) if left < 10 * Y => {
                gone += 1;
                None
            }
            _ => {
                living += 1;
                Some(10 * Y)
            }
        };
        assert_eq!(written.get(&person.sim.get()).copied(), expected);
    }
    assert!(gone > 0 && living > 0, "{gone} gone, {living} living");
}

/// Children of two parents with `core:disease_ar` twice have `test:onset_birth` from their
/// birth. Born while the switch is off, they get it at the fork that turns it on, with the
/// parents, caused by the fork, in the year of their birth at 0 years; at a later "on" only
/// who has none yet gets one.
#[test]
fn children_born_while_the_switch_is_off_get_their_onset_at_the_fork() {
    let mut run = Run::new(started(
        data(1e-9, false),
        false,
        vec![
            founder(-20 * Y, Sex::Female, true, false),
            founder(-22 * Y, Sex::Male, true, false),
        ],
        vec![Couple { woman: 0, man: 1 }],
    ));
    run.advance(6 * Y - 1);
    run.fork(true);
    let born: BTreeMap<u64, i64> = (run.entries().iter())
        .filter(|entry| code(entry) == "core:birth")
        .map(|entry| (sim(entry), entry["t"].as_i64().expect("a second")))
        .collect();
    assert!(!born.is_empty(), "children are born");
    let fork = |input: u64| json!({"input": input});
    let mut expected = vec![
        onset(6 * Y, 1, BIRTH, (-20, 0), fork(2)),
        onset(6 * Y, 2, BIRTH, (-22, 0), fork(2)),
    ];
    for (&child, &t) in &born {
        expected.push(onset(6 * Y, child, BIRTH, (t.div_euclid(Y), 0), fork(2)));
    }
    assert_eq!(onsets(&run.entries()), expected);
    run.advance(8 * Y - 1);
    run.fork(false);
    run.advance(10 * Y - 1);
    run.fork(true);
    let entries = run.entries();
    let before: Vec<u64> = (onsets(&entries).iter())
        .filter(|entry| entry["t"].as_i64() < Some(10 * Y))
        .map(sim)
        .collect();
    let now: Vec<u64> = (onsets(&entries).iter())
        .filter(|entry| entry["t"] == 10 * Y)
        .map(sim)
        .collect();
    let pedigree = run.world.pedigree();
    let unreported: Vec<u64> = (pedigree.persons.iter())
        .filter(|person| person.died.is_none() && person.emigrated.is_none())
        .map(|person| person.sim.get())
        .filter(|sim| !before.contains(sim))
        .collect();
    assert_eq!(now, unreported);
}
