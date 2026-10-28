//! The fork comparison (ADR-0021 section 3 and its addenda of 2026-10-23, 2026-10-24 and
//! 2026-10-28, format 3): per lineage key in both pedigrees the fields of a life where they
//! differ, the keys of only one, and the hereditary diseases counted per run.
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

/// The six diseases of the repository's data, in their order.
const DISEASES: [&str; 6] = [
    "core:disease_ar",
    "core:disease_ad_late",
    "core:disease_ad_partial",
    "core:disease_xr",
    "core:disease_mt",
    "core:disease_liability",
];

/// The comparison with both clocks at `clocks` and the diseases `diseases` of the data.
fn text_at(a: &Pedigree, b: &Pedigree, clocks: (i64, i64), diseases: &[&str]) -> String {
    let codes: Vec<Code> = (diseases.iter())
        .map(|code| Code::new(code).expect("a code"))
        .collect();
    serde_json::to_string(&diff(a, b, (at(clocks.0), at(clocks.1)), &codes)).expect("json")
}

/// The comparison with both clocks at 100 s, after every second of these pedigrees, and the
/// diseases of the repository's data.
fn text(a: &Pedigree, b: &Pedigree) -> String {
    text_at(a, b, (100, 100), &DISEASES)
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
        // She dies of core:disease_ar in `b`: its row counts her there; nobody shows it.
        r#"{"format_version":3,"summary":[{"disease":"core:disease_ar","a":{"affected":0,"died":0},"b":{"affected":0,"died":1}}],"a_only":["0000000000000004"],"b_only":["0000000000000005"],"changed":["#,
        // Key 1: she dies earlier of another cause, the pair ends then, one child fewer.
        r#"{"lineage_key":"0000000000000001","differs":["died","cause_of_death","partnerships","children"],"hereditary":true,"#,
        &format!(r#""a":{{"born":-100,"died":50,"cause_of_death":"core:senescence","emigrated":null,"partnerships":[{BOND_A}],"children":2,"diseases":[]}},"#),
        &format!(r#""b":{{"born":-100,"died":30,"cause_of_death":"core:disease_ar","emigrated":null,"partnerships":[{BOND_B}],"children":1,"diseases":[]}}}},"#),
        // Key 2: the first pair ends earlier and a second follows; still two children.
        r#"{"lineage_key":"0000000000000002","differs":["partnerships"],"hereditary":false,"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[{"partner":"0000000000000001","formed":0,"ended":50,"ended_by":"death"}],"children":2,"diseases":[]},"#,
        r#""b":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[{"partner":"0000000000000001","formed":0,"ended":30,"ended_by":"death"},{"partner":"0000000000000006","formed":40,"ended":null,"ended_by":null}],"children":2,"diseases":[]}},"#,
        // Key 6: a pair and a child in `b` only. Key 3 is the same in both.
        r#"{"lineage_key":"0000000000000006","differs":["partnerships","children"],"hereditary":false,"#,
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
    let summary = |value: &serde_json::Value, side: &str| -> Vec<serde_json::Value> {
        (value["summary"].as_array().expect("a list").iter())
            .map(|row| row[side].clone())
            .collect()
    };
    assert_eq!(summary(&back, "a"), summary(&forward, "b"));
    assert_eq!(summary(&back, "b"), summary(&forward, "a"));
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
        r#"{"format_version":3,"summary":[],"a_only":[],"b_only":[],"changed":[]}"#
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
            r#"{{"lineage_key":"{:016x}","differs":["children"],"hereditary":false,"a":{},"b":{}}}"#,
            n,
            life(-100, 1),
            life(-100, 0)
        )
    };
    assert_eq!(
        text(&a, &b),
        format!(
            r#"{{"format_version":3,"summary":[],"a_only":["0000000000000009"],"b_only":[],"changed":[{},{}]}}"#,
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
        format!(
            r#"{{"lineage_key":"{n:016x}","differs":[{differs}],"hereditary":false,"a":{a},"b":{b}}}"#
        )
    };
    let his = format!("[{}]", bond(1, 10, at_12));
    let theirs = format!("[{}]", bond(1, 20, open));
    let expected = format!(
        r#"{{"format_version":3,"summary":[],"a_only":["0000000000000005"],"b_only":[],"changed":[{},{},{}]}}"#,
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
    // At the clocks of 100 s the onset of 34.5 years is not reached: no disease rests.
    let expected = [
        r#"{"format_version":3,"summary":[{"disease":"core:disease_ar","a":{"affected":1,"died":0},"b":{"affected":0,"died":0}},{"disease":"core:disease_ad_late","a":{"affected":1,"died":0},"b":{"affected":1,"died":0}}],"a_only":[],"b_only":[],"changed":["#,
        r#"{"lineage_key":"0000000000000001","differs":["diseases"],"hereditary":true,"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[{"disease":"core:disease_ar","year":34,"age_years":34,"rests":false}]},"#,
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
/// in `b` all three rest: core:disease_ar, reached at the clock of 100 s, as resting, the two
/// others not reached.
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
        r#"{"format_version":3,"summary":[{"disease":"core:disease_ar","a":{"affected":1,"died":0},"b":{"affected":0,"died":0}},{"disease":"core:disease_xr","a":{"affected":1,"died":0},"b":{"affected":0,"died":0}}],"a_only":[],"b_only":[],"changed":["#,
        r#"{"lineage_key":"0000000000000001","differs":["diseases"],"hereditary":true,"#,
        r#""a":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[{"disease":"core:disease_xr","year":39,"age_years":40,"rests":false},{"disease":"core:disease_ar","year":-1,"age_years":0,"rests":false}]},"#,
        r#""b":{"born":-100,"died":null,"cause_of_death":null,"emigrated":null,"partnerships":[],"children":0,"diseases":[{"disease":"core:disease_ar","year":-1,"age_years":0,"rests":true}]}}"#,
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
    // At the clock's last second: neither shown nor resting, and not counted.
    assert_eq!(
        text_at(&a, &b, (MAX_ABS_SECS, MAX_ABS_SECS), &DISEASES),
        r#"{"format_version":3,"summary":[],"a_only":[],"b_only":[],"changed":[]}"#
    );
}

fn shown(disease: &str, onset: f64, affected: bool) -> Shown {
    Shown {
        disease: Code::new(disease).expect("a code"),
        onset,
        affected,
    }
}

fn with(mut person: Person, diseases: Vec<Shown>) -> Person {
    person.diseases = diseases;
    person
}

/// Grok's round 41, V2 (ADR-0021, addendum of 2026-10-28): a disease whose onset is reached at
/// the second of the pedigree while the switch is off there (`affected` false) is named as
/// resting; the second is the clock for the living, the death or the emigration for the
/// others; an onset not reached stays out. Onset 34.5 years from −100 s: `B` = 1 085 011 100
/// (year 34, 34 years); both clocks at 2 000 000 000. Key 1 lives, shows in `a`, rests in `b`.
/// Key 2 lives with an onset of 70 years (`B` = 2 201 471 900, after the clocks): nowhere.
/// Key 3 dies of age at 1 000 000 000 in `a` (before `B`: nothing) and at 1 500 000 000 in `b`
/// (after `B`: resting); key 4 emigrates at those seconds.
#[test]
fn a_disease_at_rest_with_its_onset_reached_is_named() {
    let at_rest = || vec![shown("core:disease_ar", 34.5, false)];
    let living =
        |n: u64, diseases: Vec<Shown>| with(person(n, n, Sex::Female, -100, None), diseases);
    let gone = |n: u64, secs: i64| {
        let person = living(n, at_rest());
        if n == 3 {
            died(person, secs, "core:senescence")
        } else {
            let mut person = person;
            person.emigrated = Some(at(secs));
            person
        }
    };
    let late = || living(2, vec![shown("core:disease_ar", 70.0, false)]);
    let a = pedigree(
        vec![
            living(1, vec![shown("core:disease_ar", 34.5, true)]),
            late(),
            gone(3, 1_000_000_000),
            gone(4, 1_000_000_000),
        ],
        Vec::new(),
    );
    let b = pedigree(
        vec![
            living(1, at_rest()),
            late(),
            gone(3, 1_500_000_000),
            gone(4, 1_500_000_000),
        ],
        Vec::new(),
    );
    let shows = r#"{"disease":"core:disease_ar","year":34,"age_years":34,"rests":false}"#;
    let rests = r#"{"disease":"core:disease_ar","year":34,"age_years":34,"rests":true}"#;
    let life = |died: &str, cause: &str, emigrated: &str, diseases: &str| {
        format!(
            r#"{{"born":-100,"died":{died},"cause_of_death":{cause},"emigrated":{emigrated},"partnerships":[],"children":0,"diseases":[{diseases}]}}"#
        )
    };
    let entry = |n: u64, differs: &str, a: String, b: String| {
        format!(
            r#"{{"lineage_key":"{n:016x}","differs":[{differs}],"hereditary":true,"a":{a},"b":{b}}}"#
        )
    };
    let age = r#""core:senescence""#;
    let expected = format!(
        r#"{{"format_version":3,"summary":[{{"disease":"core:disease_ar","a":{{"affected":1,"died":0}},"b":{{"affected":0,"died":0}}}}],"a_only":[],"b_only":[],"changed":[{},{},{}]}}"#,
        entry(
            1,
            r#""diseases""#,
            life("null", "null", "null", shows),
            life("null", "null", "null", rests)
        ),
        entry(
            3,
            r#""died","diseases""#,
            life("1000000000", age, "null", ""),
            life("1500000000", age, "null", rests)
        ),
        entry(
            4,
            r#""emigrated","diseases""#,
            life("null", "null", "1000000000", ""),
            life("null", "null", "1500000000", rests)
        ),
    );
    assert_eq!(
        text_at(&a, &b, (2_000_000_000, 2_000_000_000), &DISEASES),
        expected
    );
}

/// Grok's round 41, V1: per disease of the data, in their order, and per run the living that
/// show it and everybody who died of it; a row once one of its four numbers is above 0; a
/// code the data name but no name of the web knows stays a code. Onsets 0, clocks at 100 s.
/// `a`: key 1 shows ar; key 2 died of ar (showing it, but dead); key 3 rests in xr; key 4
/// shows core:disease_new; key 6 emigrated showing ad_late; key 7 died of age. `b`: key 1
/// rests in ar; key 2 lives and shows ar; key 4 rests in core:disease_new; key 7 died in an
/// accident. Each changed life is hereditary but key 7's (age against accident).
#[test]
fn the_diseases_are_counted_per_run() {
    let p = |n: u64, diseases: Vec<Shown>| with(person(n, n, Sex::Male, -100, None), diseases);
    let mut emigrant = p(6, vec![shown("core:disease_ad_late", 0.0, true)]);
    emigrant.emigrated = Some(at(60));
    let xr = || p(3, vec![shown("core:disease_xr", 0.0, false)]);
    let a = pedigree(
        vec![
            p(1, vec![shown("core:disease_ar", 0.0, true)]),
            died(
                p(2, vec![shown("core:disease_ar", 0.0, true)]),
                50,
                "core:disease_ar",
            ),
            xr(),
            p(4, vec![shown("core:disease_new", 0.0, true)]),
            emigrant.clone(),
            died(p(7, Vec::new()), 40, "core:senescence"),
        ],
        Vec::new(),
    );
    let b = pedigree(
        vec![
            p(1, vec![shown("core:disease_ar", 0.0, false)]),
            p(2, vec![shown("core:disease_ar", 0.0, true)]),
            xr(),
            p(4, vec![shown("core:disease_new", 0.0, false)]),
            emigrant,
            died(p(7, Vec::new()), 40, "core:accident"),
        ],
        Vec::new(),
    );
    let diseases = [
        "core:disease_ar",
        "core:disease_ad_late",
        "core:disease_xr",
        "core:disease_new",
    ];
    let value: serde_json::Value =
        serde_json::from_str(&text_at(&a, &b, (100, 100), &diseases)).expect("json");
    assert_eq!(
        value["summary"],
        serde_json::json!([
            {"disease": "core:disease_ar", "a": {"affected": 1, "died": 1}, "b": {"affected": 1, "died": 0}},
            {"disease": "core:disease_new", "a": {"affected": 1, "died": 0}, "b": {"affected": 0, "died": 0}},
        ])
    );
    let flags: Vec<(String, bool)> = (value["changed"].as_array().expect("a list").iter())
        .map(|entry| {
            let key = entry["lineage_key"].as_str().expect("a key").to_owned();
            (key, entry["hereditary"].as_bool().expect("a flag"))
        })
        .collect();
    let key = |n: u64| format!("{n:016x}");
    assert_eq!(
        flags,
        vec![
            (key(1), true),
            (key(2), true),
            (key(4), true),
            (key(7), false)
        ]
    );
}
