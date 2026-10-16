//! The units of the hand pedigree against the reference: `tools/criteria_reference.py`
//! derived them by hand from the design note, section 4 (`tests/golden/criteria/hand_units.json`)
//! and asserted its own selection equal to them.

use super::*;
use crate::criteria::export::read;
use crate::criteria::stats;
use serde_json::Value;

const HAND: &str = include_str!("../../../tests/golden/criteria/hand.ndjson");
const HAND_UNITS: &str = include_str!("../../../tests/golden/criteria/hand_units.json");

fn want() -> Value {
    serde_json::from_str(HAND_UNITS).expect("the fixture is JSON")
}

fn key(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().expect("a key"), 16).expect("hex")
}

fn keys(value: &Value) -> Vec<u64> {
    value.as_array().expect("keys").iter().map(key).collect()
}

/// A list of key pairs, flattened as the anchors of a pair check are.
fn pairs(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .expect("pairs")
        .iter()
        .flat_map(|pair| [key(&pair[0]), key(&pair[1])])
        .collect()
}

fn count(value: &Value) -> u64 {
    value.as_u64().expect("a count")
}

#[test]
fn the_reference_pins_the_hand_units() {
    let export = read(HAND).expect("the hand export");
    let town = Town::new(&export).expect("a town");
    let want = want();
    let families: Vec<[u64; 3]> = town
        .families()
        .iter()
        .map(|family| [family.children[0], family.mother, family.father])
        .collect();
    let want_families: Vec<[u64; 3]> = want["families"]
        .as_array()
        .expect("families")
        .iter()
        .map(|f| [key(&f[0]), key(&f[1]), key(&f[2])])
        .collect();
    assert_eq!(families, want_families, "families");
    assert_eq!(
        town.p5_siblings().anchors,
        pairs(&want["p5_siblings"]),
        "P5 siblings"
    );
    assert_eq!(
        town.p5_strangers().anchors,
        pairs(&want["p5_strangers"]),
        "P5 strangers"
    );
    assert_eq!(
        town.p9_polygenic().anchors,
        pairs(&want["p9_polygenic"]),
        "P9 polygenic"
    );
    let mc1r = town.p9_mc1r();
    assert!(mc1r.in_band());
    let sample = want["p9_mc1r"]["sample"].as_array().expect("sample");
    assert_eq!(
        mc1r.units.anchors,
        sample.iter().map(|s| key(&s[0])).collect::<Vec<_>>()
    );
    let unaffected: Vec<bool> = sample.iter().map(|s| count(&s[1]) == 1).collect();
    let flags: Vec<bool> = mc1r.units.values.iter().map(|value| value.0).collect();
    assert_eq!(flags, unaffected, "y_f");
    let expected: Vec<u64> = sample
        .iter()
        .map(|s| stats::mc1r_expectation(u32::try_from(count(&s[2])).expect("small")).to_bits())
        .collect();
    let got: Vec<u64> = mc1r
        .units
        .values
        .iter()
        .map(|value| value.1.to_bits())
        .collect();
    assert_eq!(got, expected, "E_f from n_f");
    let (copies, parents) = (
        count(&want["p9_mc1r"]["copies"]),
        count(&want["p9_mc1r"]["parents"]),
    );
    assert_eq!((mc1r.copies, mc1r.parents), (copies, parents), "q");
    assert_eq!(
        mc1r.q.expect("family parents").to_bits(),
        (stats::real(copies) / stats::real(2 * parents)).to_bits(),
        "q"
    );
    for decade in 0..DECADES {
        let units = &want["p4_city"][decade.to_string()];
        let expected = if units.is_null() {
            Vec::new()
        } else {
            keys(units)
        };
        assert_eq!(town.p4_city(decade).anchors, expected, "P4 decade {decade}");
    }
    assert_eq!(
        town.couple_pairs(Trait::Growth).anchors,
        pairs(&want["f11_pairs"]),
        "F11 pairs"
    );
    for (female, name) in [
        (true, "f11_first_partnership_f"),
        (false, "f11_first_partnership_m"),
    ] {
        let rows = want[name].as_array().expect("rows");
        let partnerships = town.first_partnerships(female);
        assert_eq!(
            partnerships.anchors,
            rows.iter().map(|r| key(&r[0])).collect::<Vec<_>>(),
            "{name}"
        );
        let mut hist = [0; MONTHS];
        let mut never = 0;
        for row in rows {
            match row[1].as_u64() {
                Some(month) => hist[usize::try_from(month).expect("small")] += 1,
                None => never += 1,
            }
        }
        assert_eq!(
            (partnerships.hist, partnerships.never),
            (hist, never),
            "{name}"
        );
    }
    let births = town.births();
    let rows = want["f11_births"].as_array().expect("rows");
    assert_eq!(
        births.anchors,
        rows.iter().map(|r| key(&r[0])).collect::<Vec<_>>(),
        "births"
    );
    assert_eq!(
        births.values,
        rows.iter().map(|r| count(&r[1])).collect::<Vec<_>>(),
        "births"
    );
    let early = town.early_births();
    assert_eq!(
        early.anchors,
        keys(&want["f11_early_births"]["units"]),
        "early births"
    );
    let early_keys: Vec<u64> = early
        .anchors
        .iter()
        .zip(&early.values)
        .filter(|(_, early)| **early)
        .map(|(key, _)| *key)
        .collect();
    assert_eq!(
        early_keys,
        keys(&want["f11_early_births"]["early"]),
        "early births"
    );
    let gap = town.age_gap();
    assert_eq!(gap.anchors, pairs(&want["f11_gap"]), "age gap");
    let thirteenths: Vec<f64> = want["f11_gap_years_times_13"]
        .as_array()
        .expect("gaps")
        .iter()
        .map(|g| f64::from(i32::try_from(g.as_i64().expect("an integer")).expect("small")) / 13.0)
        .collect();
    assert_eq!(gap.values, thirteenths, "age gap in years");
    for (female, sex) in [(true, "f"), (false, "m")] {
        let table = town.life_table(female);
        let want = &want["f11_life_table"][sex];
        let mut exposure = [0; AGES];
        let mut deaths = [0; AGES];
        for (age, e) in want["E"].as_object().expect("ages") {
            exposure[age.parse::<usize>().expect("an age")] = count(e);
        }
        for (age, d) in want["D"].as_object().expect("ages") {
            deaths[age.parse::<usize>().expect("an age")] = count(d);
        }
        assert_eq!(
            (table.exposure, table.deaths),
            (exposure, deaths),
            "life table {sex}"
        );
        // Every row of the sex is a unit of its table (NOTES item 15).
        assert_eq!(
            u64::try_from(table.anchors.len()).expect("small"),
            count(&want["rows"]),
            "life-table rows {sex}"
        );
        let longevity = town.longevity(female);
        let want_h2 = &want_h2t(sex);
        assert_eq!(
            longevity.units.anchors,
            keys(&want_h2["units"]),
            "h2_T {sex}"
        );
        assert_eq!(
            (longevity.emigrated, longevity.living),
            (count(&want_h2["emigrated"]), count(&want_h2["living"])),
            "h2_T {sex}"
        );
    }
    let village = &want["village"];
    let block = town.village_block().expect("200 years");
    assert_eq!(block, village["g_star"].as_i64().expect("g*"));
    let blocks: Vec<(i64, u64, u64)> = village["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|b| (b[0].as_i64().expect("g"), count(&b[1]), count(&b[2])))
        .collect();
    assert_eq!(town.parent_blocks(block), blocks, "N_m, N_f");
    // The hand village has no unit in its block (the report's finding) and one living.
    assert!(keys(&village["units"]).is_empty());
    assert_eq!(
        town.village(),
        Some(Village {
            block,
            units: Units::default(),
            blocks,
            living: count(&village["living"]),
        })
    );
}

fn want_h2t(sex: &str) -> Value {
    want()["f11_h2t"][sex].clone()
}
