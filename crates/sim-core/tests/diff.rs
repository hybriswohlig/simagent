//! The fork comparison (ADR-0021 section 3 and its addendum of 2026-10-23): per lineage key
//! in both pedigrees the fields of a life where they differ, and the keys of only one.
//! The pedigrees are written by hand; every expected text is derived by hand from the
//! addendum's format, never from a run.

use sim_core::data_id::Code;
use sim_core::diff::diff;
use sim_core::genome::Sex;
use sim_core::ids::{HouseholdId, PartnershipId, SimId};
use sim_core::lineage::LineageKey;
use sim_core::pedigree::{Couple, End, FromPool, Pedigree, Person, Shown};
use sim_core::time::{MAX_ABS_SECS, SimTime};

fn key(n: u64) -> LineageKey {
    serde_json::from_value(serde_json::json!(format!("{n:016x}"))).expect("a key")
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("a sim")
}

fn at(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

/// A town person with the parents `parents` (sims), alive.
fn person(id: u64, lineage: u64, sex: Sex, born: i64, parents: Option<(u64, u64)>) -> Person {
    Person {
        sim: sim(id),
        lineage_key: key(lineage),
        sex,
        alleles: Vec::new(),
        born: at(born),
        died: None,
        cause_of_death: None,
        emigrated: None,
        mother: parents.map(|(mother, _)| sim(mother)),
        father: parents.map(|(_, father)| sim(father)),
        social_parents: Vec::new(),
        household: HouseholdId::try_from(1).expect("a household"),
        moved_out: false,
        carer: None,
        diseases: Vec::new(),
        pool: None,
    }
}

fn died(mut person: Person, secs: i64, cause: &str) -> Person {
    person.died = Some(at(secs));
    person.cause_of_death = Some(Code::new(cause).expect("a code"));
    person
}

fn couple(id: u64, woman: u64, man: u64, formed: i64, ended: Option<(i64, End)>) -> Couple {
    Couple {
        partnership: PartnershipId::try_from(id).expect("a partnership"),
        woman: sim(woman),
        man: sim(man),
        formed: at(formed),
        ended: ended.map(|(secs, _)| at(secs)),
        ended_by: ended.map(|(_, by)| by),
        kinship: 0.0,
    }
}

fn pedigree(persons: Vec<Person>, partnerships: Vec<Couple>) -> Pedigree {
    Pedigree {
        format_version: 4,
        persons,
        partnerships,
        households: Vec::new(),
    }
}

fn text(a: &Pedigree, b: &Pedigree) -> String {
    serde_json::to_string(&diff(a, b)).expect("json")
}

/// Run `a`: founders 1 (key 1, she dies of age at 50) and 2 (key 2), a pair from 0, their
/// children 3 (key 3) and 4 (key 4); founder 5 (key 6) alone. Run `b`: 1 dies of a
/// hereditary disease at 30, the pair ends then, 4 is never conceived; 2 pairs with 5 at
/// 40 and they have a child, sim 4 with key 5 (a sim of `a` with another key: the
/// comparison never goes by sim).
fn two_runs() -> (Pedigree, Pedigree) {
    let (her, him, she) = (
        person(1, 1, Sex::Female, -100, None),
        person(2, 2, Sex::Male, -100, None),
        person(5, 6, Sex::Female, -90, None),
    );
    let a = pedigree(
        vec![
            died(her.clone(), 50, "core:senescence"),
            him.clone(),
            person(3, 3, Sex::Male, 10, Some((1, 2))),
            person(4, 4, Sex::Female, 20, Some((1, 2))),
            she.clone(),
        ],
        vec![couple(1, 1, 2, 0, Some((50, End::Death)))],
    );
    let b = pedigree(
        vec![
            died(her, 30, "core:disease_ar"),
            him,
            person(3, 3, Sex::Male, 10, Some((1, 2))),
            person(4, 5, Sex::Male, 45, Some((5, 2))),
            she,
        ],
        vec![
            couple(1, 1, 2, 0, Some((30, End::Death))),
            couple(2, 5, 2, 40, None),
        ],
    );
    (a, b)
}

const BOND_A: &str = r#"{"partner":"0000000000000002","formed":0,"ended":50,"ended_by":"death"}"#;
const BOND_B: &str = r#"{"partner":"0000000000000002","formed":0,"ended":30,"ended_by":"death"}"#;

#[test]
fn a_life_differs_in_its_death_its_partnerships_and_its_children() {
    let (a, b) = two_runs();
    let expected = [
        r#"{"format_version":2,"a_only":["0000000000000004"],"b_only":["0000000000000005"],"changed":["#,
        // Key 1: she dies earlier of another cause, the pair ends then, one child fewer.
        r#"{"lineage_key":"0000000000000001","differs":["died","cause_of_death","partnerships","children"],"#,
        &format!(r#""a":{{"born":-100,"died":50,"cause_of_death":"core:senescence","emigrated":null,"partnerships":[{BOND_A}],"children":2,"diseases":[]}},"#),
        &format!(r#""b":{{"born":-100,"died":30,"cause_of_death":"core:disease_ar","emigrated":null,"partnerships":[{BOND_B}],"children":1,"diseases":[]}}}},"#),
        // Key 2: the first pair ends earlier and a second follows; still two children.
        r#"{"lineage_key":"0000000000000002","differs":["partnerships"],"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[{"partner":"0000000000000001","formed":0,"ended":50,"ended_by":"death"}],"children":2,"diseases":[]},"#,
        r#""b":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[{"partner":"0000000000000001","formed":0,"ended":30,"ended_by":"death"},{"partner":"0000000000000006","formed":40,"ended":null,"ended_by":null}],"children":2,"diseases":[]}},"#,
        // Key 6: a pair and a child in `b` only. Key 3 is the same in both.
        r#"{"lineage_key":"0000000000000006","differs":["partnerships","children"],"#,
        r#""a":{"born":-90,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[]},"#,
        r#""b":{"born":-90,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[{"partner":"0000000000000002","formed":40,"ended":null,"ended_by":null}],"children":1,"diseases":[]}}"#,
        "]}",
    ]
    .concat();
    assert_eq!(text(&a, &b), expected);
}

/// `a` and `b` are the runs in the order given: swapped, the two sides swap.
#[test]
fn the_order_of_the_runs_is_the_order_of_the_sides() {
    let (a, b) = two_runs();
    let forward: serde_json::Value = serde_json::from_str(&text(&a, &b)).expect("json");
    let back: serde_json::Value = serde_json::from_str(&text(&b, &a)).expect("json");
    assert_eq!(back["a_only"], forward["b_only"]);
    assert_eq!(back["b_only"], forward["a_only"]);
    let changed = |value: &serde_json::Value, side: &str| -> Vec<serde_json::Value> {
        (value["changed"].as_array().expect("a list").iter())
            .map(|entry| entry[side].clone())
            .collect()
    };
    assert_eq!(changed(&back, "a"), changed(&forward, "b"));
    assert_eq!(changed(&back, "b"), changed(&forward, "a"));
}

#[test]
fn equal_pedigrees_differ_in_nothing() {
    let (a, _) = two_runs();
    assert_eq!(
        text(&a, &a),
        r#"{"format_version":2,"a_only":[],"b_only":[],"changed":[]}"#
    );
}

/// A child of a pool pregnancy born in the town has no town parents; it counts for the
/// keys of its parents in the pool (`pool.mother_key`, `pool.father_key`).
#[test]
fn a_child_of_a_pool_pregnancy_counts_for_its_parents_keys() {
    let parents = || {
        vec![
            person(1, 1, Sex::Female, -100, None),
            person(2, 2, Sex::Male, -100, None),
        ]
    };
    let mut child = person(3, 9, Sex::Female, 5, None);
    child.pool = Some(FromPool {
        genome_key: serde_json::from_value(serde_json::json!("0000000000000009")).expect("a key"),
        born_pool: None,
        arrived: at(0),
        arrival_year: 0,
        mother_key: Some(key(1)),
        father_key: Some(key(2)),
        social_parent_keys: Vec::new(),
    });
    let a = pedigree([parents(), vec![child]].concat(), Vec::new());
    let b = pedigree(parents(), Vec::new());
    let life = |born: i64, children: u32| {
        format!(
            r#"{{"born":{born},"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":{children},"diseases":[]}}"#
        )
    };
    let entry = |n: u64| {
        format!(
            r#"{{"lineage_key":"{:016x}","differs":["children"],"a":{},"b":{}}}"#,
            n,
            life(-100, 1),
            life(-100, 0)
        )
    };
    assert_eq!(
        text(&a, &b),
        format!(
            r#"{{"format_version":2,"a_only":["0000000000000009"],"b_only":[],"changed":[{},{}]}}"#,
            entry(1),
            entry(2)
        )
    );
}

/// Astra's review of G1: a different birth and emigration; the partnerships of a person
/// come out by the second they formed, then by the partner's key, whatever their order in
/// the pedigree; a parent by sim goes before the key of a pool origin, which counts only
/// where the sim is missing.
#[test]
fn each_field_and_each_order_by_hand() {
    let mut child = person(5, 5, Sex::Female, 30, None);
    child.mother = Some(sim(1));
    child.pool = Some(FromPool {
        genome_key: serde_json::from_value(serde_json::json!("0000000000000005")).expect("a key"),
        born_pool: None,
        arrived: at(30),
        arrival_year: 0,
        mother_key: Some(key(9)),
        father_key: Some(key(4)),
        social_parent_keys: Vec::new(),
    });
    let mut emigrant = person(2, 2, Sex::Male, -100, None);
    emigrant.emigrated = Some(at(70));
    let others = || {
        vec![
            person(3, 3, Sex::Male, -90, None),
            person(4, 4, Sex::Male, -80, None),
            person(9, 9, Sex::Female, -95, None),
        ]
    };
    let a = pedigree(
        [
            vec![person(1, 1, Sex::Female, -100, None), emigrant],
            others(),
            vec![child],
        ]
        .concat(),
        vec![
            couple(1, 1, 4, 20, None),
            couple(2, 1, 3, 10, Some((15, End::Separation))),
            couple(3, 1, 2, 10, Some((12, End::Separation))),
        ],
    );
    let b = pedigree(
        [
            vec![
                person(1, 1, Sex::Female, -101, None),
                person(2, 2, Sex::Male, -100, None),
            ],
            others(),
        ]
        .concat(),
        vec![
            couple(1, 1, 2, 10, Some((12, End::Separation))),
            couple(2, 1, 4, 20, None),
            couple(3, 1, 3, 10, Some((15, End::Separation))),
        ],
    );
    let bond = |partner: u64, formed: i64, ended: &str| {
        format!(r#"{{"partner":"{partner:016x}","formed":{formed},{ended}}}"#)
    };
    let (open, at_12, at_15) = (
        r#""ended":null,"ended_by":null"#,
        r#""ended":12,"ended_by":"separation""#,
        r#""ended":15,"ended_by":"separation""#,
    );
    let hers = format!(
        "[{},{},{}]",
        bond(2, 10, at_12),
        bond(3, 10, at_15),
        bond(4, 20, open)
    );
    let life = |born: i64, emigrated: &str, partnerships: &str, children: u32| {
        format!(
            r#"{{"born":{born},"died":null,"cause_of_death":null,"emigrated":{emigrated},"partnerships":{partnerships},"children":{children},"diseases":[]}}"#
        )
    };
    let changed = |n: u64, differs: &str, a: String, b: String| {
        format!(r#"{{"lineage_key":"{n:016x}","differs":[{differs}],"a":{a},"b":{b}}}"#)
    };
    let his = format!("[{}]", bond(1, 10, at_12));
    let theirs = format!("[{}]", bond(1, 20, open));
    let expected = format!(
        r#"{{"format_version":2,"a_only":["0000000000000005"],"b_only":[],"changed":[{},{},{}]}}"#,
        changed(
            1,
            r#""born","children""#,
            life(-100, "null", &hers, 1),
            life(-101, "null", &hers, 0)
        ),
        changed(
            2,
            r#""emigrated""#,
            life(-100, "70", &his, 0),
            life(-100, "null", &his, 0)
        ),
        changed(
            4,
            r#""children""#,
            life(-80, "null", &theirs, 1),
            life(-80, "null", &theirs, 0)
        ),
    );
    assert_eq!(text(&a, &b), expected);
}

/// Grok's round 34, E1 (ADR-0021, addendum of 2026-10-24): a disease that shows
/// (`affected`) is part of a life with the calendar year and the whole years of its onset, as
/// the card reads them: `B = born + ⌈onset · YEAR⌉`, the year of `B`, `⌊onset⌋` (ADR-0017
/// section 3). Key 1, born at −100 s, shows core:disease_ar with onset 34.5 in `a` and rests
/// in `b`: 34.5 years are 1 085 011 200 s, so `B` = 1 085 011 100, in year 34 (which begins at
/// 34 · 31 449 600 = 1 069 286 400 s), and ⌊34.5⌋ = 34. Key 2 shows core:disease_ad_late with
/// onset 0 in both (year −1, the year of its birth at −100 s, and 0 years): no difference.
#[test]
fn a_disease_that_shows_on_one_side_only_differs() {
    let shown = |disease: &str, onset: f64, affected: bool| Shown {
        disease: Code::new(disease).expect("a code"),
        onset,
        affected,
    };
    let with = |mut person: Person, diseases: Vec<Shown>| {
        person.diseases = diseases;
        person
    };
    let late = || shown("core:disease_ad_late", 0.0, true);
    let a = pedigree(
        vec![
            with(
                person(1, 1, Sex::Female, -100, None),
                vec![shown("core:disease_ar", 34.5, true)],
            ),
            with(person(2, 2, Sex::Male, -100, None), vec![late()]),
        ],
        Vec::new(),
    );
    let b = pedigree(
        vec![
            with(
                person(1, 1, Sex::Female, -100, None),
                vec![shown("core:disease_ar", 34.5, false)],
            ),
            with(person(2, 2, Sex::Male, -100, None), vec![late()]),
        ],
        Vec::new(),
    );
    let expected = [
        r#"{"format_version":2,"a_only":[],"b_only":[],"changed":["#,
        r#"{"lineage_key":"0000000000000001","differs":["diseases"],"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[{"disease":"core:disease_ar","year":34,"age_years":34}]},"#,
        r#""b":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[]}}"#,
        "]}",
    ]
    .concat();
    assert_eq!(text(&a, &b), expected);
}

/// Astra's review of the diseases in the comparison: the diseases come in the order of the
/// pedigree (that of the data), not by code, and one that rests between them is left out.
/// Key 1, born at −100 s, shows core:disease_xr with onset 40 (`B` = −100 + 40 · 31 449 600 =
/// 1 257 983 900 s, in year 39, which ends at 1 257 983 999; 40 whole years), rests in
/// core:disease_mt and shows core:disease_ar with onset 0 (`B` = −100 s, year −1, 0 years);
/// in `b` all three rest.
#[test]
fn the_diseases_keep_the_order_of_the_data() {
    let shown = |disease: &str, onset: f64, affected: bool| Shown {
        disease: Code::new(disease).expect("a code"),
        onset,
        affected,
    };
    let three = |on: bool| {
        let mut person = person(1, 1, Sex::Female, -100, None);
        person.diseases = vec![
            shown("core:disease_xr", 40.0, on),
            shown("core:disease_mt", 12.0, false),
            shown("core:disease_ar", 0.0, on),
        ];
        pedigree(vec![person], Vec::new())
    };
    let expected = [
        r#"{"format_version":2,"a_only":[],"b_only":[],"changed":["#,
        r#"{"lineage_key":"0000000000000001","differs":["diseases"],"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[{"disease":"core:disease_xr","year":39,"age_years":40},{"disease":"core:disease_ar","year":-1,"age_years":0}]},"#,
        r#""b":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[]}}"#,
        "]}",
    ]
    .concat();
    assert_eq!(text(&three(true), &three(false)), expected);
}

/// Astra's review: at the last second of the clock a disease can count as reached by its
/// age in years while its onset second, rounded up, lies one second beyond the clock: 31 s
/// as years and back are 31.000000000000004 s, so `⌈onset · YEAR⌉` is 32. Such an onset never
/// comes (ADR-0002), and the comparison does not list it instead of failing.
#[test]
fn an_onset_beyond_the_clock_is_not_listed() {
    let mut late = person(1, 1, Sex::Female, MAX_ABS_SECS - 31, None);
    late.diseases = vec![Shown {
        disease: Code::new("core:disease_ar").expect("a code"),
        onset: 31.0 / 31_449_600.0,
        affected: true,
    }];
    let resting = {
        let mut person = late.clone();
        person.diseases[0].affected = false;
        person
    };
    let (a, b) = (
        pedigree(vec![late], Vec::new()),
        pedigree(vec![resting], Vec::new()),
    );
    assert_eq!(
        text(&a, &b),
        r#"{"format_version":2,"a_only":[],"b_only":[],"changed":[]}"#
    );
}
