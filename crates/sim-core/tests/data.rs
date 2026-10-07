//! ADR-0007: the definitions of the repository, their checks and their data hash.

mod common;

use serde_json::{Value, json};
use sim_core::data::{Data, DataError, Morgans, PerTransmission};

/// The data hash of the definitions of the repository, from `tools/data_reference.py`.
const DATA_HASH: u64 = 0x1bdf_3636_c22d_0643;

/// A change of the two files: calibration, genome map.
type Change = fn(&mut Value, &mut Value);

/// The check of definitions changed by `change`, as its error code or "ok".
fn checked(change: impl FnOnce(&mut Value, &mut Value)) -> String {
    let (mut calibration, mut genome_map) = common::files();
    change(&mut calibration, &mut genome_map);
    match common::definitions(calibration, genome_map) {
        Err(serde) => format!("serde: {serde}"),
        Ok(definitions) => match Data::new(definitions) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.to_string(),
        },
    }
}

fn locus(slot: u64) -> Value {
    json!({
        "id": format!("test:l{slot}"),
        "chromosome": "autosome",
        "segment": 1,
        "slot": slot,
        "action": "additive",
        "mark": "small",
        "mutation": "both"
    })
}

#[test]
fn data_hash_is_pinned() {
    assert_eq!(common::data().hash(), DATA_HASH);
}

#[test]
fn the_calibration_is_read() {
    let genetics = common::data().genetics();
    assert_eq!(genetics.map_length_autosome_female, Morgans(5.35));
    assert_eq!(genetics.map_length_autosome_male, Morgans(3.24));
    assert_eq!(genetics.map_length_x_female, Morgans(1.79));
    assert_eq!(genetics.mutation_rate, PerTransmission(5e-5));
    assert_eq!(genetics.mutation_rate_disease, PerTransmission(1e-6));
    assert_eq!(common::data().genome_map().loci.len(), 1);
}

#[test]
fn the_hash_follows_every_value() {
    let changed = |change: Change| {
        let (mut calibration, mut genome_map) = common::files();
        change(&mut calibration, &mut genome_map);
        let definitions = common::definitions(calibration, genome_map).expect("definitions");
        Data::new(definitions).expect("checked").hash()
    };
    let changes: [Change; 3] = [
        |c, _| c["values"]["core:map_length_x_female"]["value"] = json!(1.8),
        |_, g| g["map_version"] = json!(2),
        |_, g| g["loci"][0]["slot"] = json!(1),
    ];
    for change in changes {
        assert_ne!(changed(change), DATA_HASH);
    }
}

#[test]
fn genome_map_rejects() {
    let cases: [(Change, &str); 19] = [
        (|_, g| g["format_version"] = json!(2), "data.format_version"),
        (|_, g| g["map_version"] = json!(0), "data.map_version"),
        // Positions: slot and segment within the chromosome type.
        (
            |_, g| g["loci"][0]["slot"] = json!(24),
            "data.locus_position",
        ),
        (
            |_, g| g["loci"][0]["segment"] = json!(8),
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("x");
                g["loci"][0]["slot"] = json!(16);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("y");
                g["loci"][0]["slot"] = json!(8);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("mt");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        // Order: strictly increasing by chromosome type, segment and slot.
        (
            |_, g| g["loci"] = json!([locus(3), locus(2)]),
            "data.locus_order",
        ),
        (
            |_, g| g["loci"] = json!([locus(2), locus(2)]),
            "data.locus_order",
        ),
        (
            |_, g| {
                let mut other = locus(3);
                other["id"] = json!("test:l2");
                g["loci"] = json!([locus(2), other]);
            },
            "data.locus_id",
        ),
        // Marks (B9, specification 2.4): disease loci mutate only to the alternative
        // allele, mendelian loci are marked large or disease.
        (
            |_, g| g["loci"][0]["mutation"] = json!("both"),
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:r", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "recessive", "mark": "small", "mutation": "both"}])
            },
            "data.locus_mark",
        ),
        (|_, g| g["loci"][0]["chromosome"] = json!("w"), "serde"),
        // X and Y have one segment; the chromosome type comes first in the order.
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("x");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("y");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                let mut on_x = locus(0);
                on_x["chromosome"] = json!("x");
                on_x["segment"] = json!(0);
                g["loci"] = json!([on_x, locus(5)]);
            },
            "data.locus_order",
        ),
        (
            |_, g| g["loci"][0]["mutation"] = json!("to_ref"),
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:d", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "dominant", "mark": "small", "mutation": "both"}])
            },
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:d", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "dominant", "mark": "small", "mutation": "to_alt"}])
            },
            "data.locus_mark",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // The cases are no accident of the repository's map: valid variants pass.
    assert_eq!(
        checked(|_, g| g["loci"] = json!([locus(2), locus(3)])),
        "ok"
    );
    assert_eq!(
        checked(
            |_, g| g["loci"] = json!([{"id": "test:d", "chromosome": "autosome",
            "segment": 0, "slot": 0, "action": "dominant", "mark": "large", "mutation": "both"}])
        ),
        "ok"
    );
    assert_eq!(
        checked(|_, g| {
            let mut on_x = locus(0);
            on_x["chromosome"] = json!("x");
            on_x["segment"] = json!(0);
            g["loci"] = json!([locus(5), on_x]);
        }),
        "ok"
    );
    assert_eq!(
        checked(|_, g| {
            g["loci"][0]["chromosome"] = json!("x");
            g["loci"][0]["slot"] = json!(15);
        }),
        "ok"
    );
}

#[test]
fn calibration_rejects() {
    let cases: [(Change, &str); 10] = [
        (|c, _| c["format_version"] = json!(0), "data.format_version"),
        (
            |c, _| {
                c["values"]
                    .as_object_mut()
                    .expect("values")
                    .remove("core:mutation_rate");
            },
            "data.calibration_missing",
        ),
        (
            |c, _| c["values"]["core:extra"] = json!({"value": 1.0, "unit": "morgan"}),
            "data.calibration_unknown",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["unit"] = json!("morgan"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(30.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(-1e-5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(1.000_001),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(-1.0),
            "data.calibration_range",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // A rate may be 1 at most.
    assert_eq!(
        checked(|c, _| c["values"]["core:mutation_rate"]["value"] = json!(1.0)),
        "ok"
    );
    // The upper limit of a map length is the largest mean of `poisson`.
    assert_eq!(
        checked(|c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(30.0)),
        "ok"
    );
    assert_eq!(
        DataError::CalibrationRange.to_string(),
        "data.calibration_range"
    );
}
