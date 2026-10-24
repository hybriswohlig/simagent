//! The scenario of the sheet of portraits, `data/scenarios/bogen.json` (ADR-0021 G10 and its
//! addendum of 2026-10-24), against Grok's round 37 (B1 to B8) by hand: ten founders, each
//! naming every locus of the map in its order, five variants, each a woman, then a man, born
//! at −61 years; the switch off, no nutrition, no couples, no capacity, no pool, until 0.
//! The run takes it, and at the second 0 every founder shows the seven ages of the sheet.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sim_core::ids::SimId;
use sim_core::world::World;

const Y: i64 = 31_449_600;

fn read(path: &str) -> Value {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    serde_json::from_str(&std::fs::read_to_string(full).expect("the file")).expect("JSON")
}

/// The copies a founder names for `locus`.
fn named<'a>(founder: &'a Value, locus: &str) -> &'a Value {
    let alleles = founder["alleles"].as_array().expect("alleles");
    let found = alleles.iter().find(|allele| allele["locus"] == locus);
    &found.expect(locus)["copies"]
}

#[test]
fn the_scenario_follows_the_decisions_by_hand() {
    let scenario = read("data/scenarios/bogen.json");
    let map = read("data/genetics/genome_map.json");
    let start = &scenario["start"];
    assert_eq!(scenario["format_version"], 5);
    assert_eq!(scenario["until"], 0);
    assert_eq!(start["founder_tag"], "base:bogen");
    assert_eq!(start["world_params"]["hereditary_diseases"], false);
    assert_eq!(start["world_params"]["nutrition"], serde_json::json!([]));
    assert_eq!(start["couples"], serde_json::json!([]));
    assert!(start["capacity"].is_null() && start["pool"].is_null());
    let order: Vec<&str> = (map["loci"].as_array().expect("loci").iter())
        .map(|locus| locus["id"].as_str().expect("an id"))
        .collect();
    let founders = start["founders"].as_array().expect("founders");
    assert_eq!(founders.len(), 10);
    for (n, founder) in founders.iter().enumerate() {
        let sex = if n % 2 == 0 { "female" } else { "male" };
        assert_eq!(founder["sex"], sex, "founder {n}");
        assert_eq!(founder["born"], -61 * Y, "founder {n}");
        let loci: Vec<&str> = (founder["alleles"].as_array().expect("alleles").iter())
            .map(|allele| allele["locus"].as_str().expect("a locus"))
            .collect();
        assert_eq!(
            loci, order,
            "founder {n} names every locus in the map's order"
        );
        // B1, B5, B6: the base of every variant.
        assert_eq!(named(founder, "core:growth_01"), &serde_json::json!([0, 1]));
        assert_eq!(
            named(founder, "core:disease_liability_03"),
            &serde_json::json!([0, 0])
        );
        assert_eq!(
            named(founder, "core:disease_ar"),
            &serde_json::json!([0, 0])
        );
        assert_eq!(named(founder, "core:disease_mt"), &serde_json::json!([0]));
        let x = if sex == "female" {
            serde_json::json!([0, 0])
        } else {
            serde_json::json!([0])
        };
        assert_eq!(named(founder, "core:disease_xr"), &x);
    }
    // B2 to B4 and B8: early, base, late, red, light iris.
    let variant = |n: usize| &founders[n];
    for (n, puberty, mc1r, herc2) in [
        (0, [0, 0], [0, 0], [0, 0]),
        (2, [0, 1], [0, 0], [0, 0]),
        (4, [1, 1], [0, 0], [0, 0]),
        (6, [0, 1], [1, 1], [0, 0]),
        (8, [0, 1], [0, 0], [1, 1]),
    ] {
        for founder in [variant(n), variant(n + 1)] {
            for p in 1..=6 {
                let locus = format!("core:puberty_{p:02}");
                assert_eq!(
                    named(founder, &locus),
                    &serde_json::json!(puberty),
                    "{n} {locus}"
                );
            }
            assert_eq!(named(founder, "core:mc1r"), &serde_json::json!(mc1r), "{n}");
            assert_eq!(
                named(founder, "core:herc2"),
                &serde_json::json!(herc2),
                "{n}"
            );
        }
        // The same genes in both sexes, but the man's single X.
        let (woman, man) = (variant(n), variant(n + 1));
        let without_x = |founder: &Value| -> Vec<Value> {
            (founder["alleles"].as_array().expect("alleles").iter())
                .filter(|allele| allele["locus"] != "core:disease_xr")
                .cloned()
                .collect()
        };
        assert_eq!(without_x(woman), without_x(man), "{n}");
    }
}

#[test]
fn the_run_shows_every_founder_at_the_seven_ages() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sheet")
        .join("run");
    if out.exists() {
        std::fs::remove_dir_all(&out).expect("removes the old run");
    }
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/scenarios/bogen.json");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), out.as_os_str()])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let bytes = std::fs::read(out.join("state.save")).expect("the save");
    let world = World::load(&bytes, sim_cli::data::core_data().expect("data"))
        .expect("loads")
        .0;
    assert_eq!(world.closed_through().as_secs(), 0);
    for n in 1..=10 {
        let view = world
            .neutral_person_view(SimId::try_from(n).expect("an id"))
            .expect("a view");
        let ages: Vec<i64> = view
            .at_ages
            .iter()
            .map(|shown| shown.at.age_years)
            .collect();
        assert_eq!(ages, [2, 8, 12, 16, 25, 40, 60], "founder {n}");
        assert_eq!(view.shown.at.age_years, 61, "founder {n}");
    }
}
