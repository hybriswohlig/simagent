//! The check of the body contract (ADR-0005 section 7): `contracts/body.json` is the
//! single source of the visible body values. This module checks its schema, the rules of
//! the units, the melanin table and the changelog. The comparison with the merge base
//! and the generated types follow with their slices.

use serde_json::{Map, Value};

/// The closed list of parameter groups.
pub const GROUPS: [&str; 11] = [
    "age", "skeleton", "build", "shape", "face", "pigment", "hair", "beard", "freckles", "ageing",
    "puberty",
];

const TOP_FIELDS: [&str; 5] = [
    "format_version",
    "contract_version",
    "retired_keys",
    "parameters",
    "melanin_to_srgb",
];
const PARAMETER_FIELDS: [&str; 7] = [
    "key",
    "group",
    "unit",
    "range",
    "default",
    "stability",
    "description",
];

/// `MAJOR.MINOR.PATCH`: three decimal numbers without sign, leading zero or suffix.
pub fn is_version(text: &str) -> bool {
    !text.is_empty()
}

/// An 8-bit sRGB channel in linear light (IEC 61966-2-1).
fn linear(level: u8) -> f64 {
    f64::from(level)
}

/// Relative luminance of a colour, on linear values.
fn luminance(rgb: [u8; 3]) -> f64 {
    linear(rgb[0])
}

/// Share of red in a colour, on linear values.
fn red_share(rgb: [u8; 3]) -> f64 {
    linear(rgb[0])
}

/// The colour tables (ADR-0005 section 5).
fn melanin_findings(tables: &Value, findings: &mut Vec<String>) {
    let _ = (tables, luminance([0; 3]), red_share([0; 3]));
    findings.clear();
}

/// ADR-0005 section 7, check 1: the findings of the schema of `contracts/body.json`.
pub fn schema_findings(body_json: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let empty = Map::new();
    let _ = (
        body_json,
        GROUPS,
        TOP_FIELDS,
        PARAMETER_FIELDS,
        is_version(""),
        &empty,
    );
    melanin_findings(&Value::Null, &mut findings);
    findings
}

/// ADR-0005 section 7, check 2 (the changelog): `contracts/body.changelog.md` has a
/// section `## <contract_version>` with the entries Neu, Geändert and Stillgelegt.
pub fn changelog_findings(body_json: &str, changelog: &str) -> Vec<String> {
    let _ = (body_json, changelog);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO_CONTRACT: &str = include_str!("../../contracts/body.json");
    const REPO_CHANGELOG: &str = include_str!("../../contracts/body.changelog.md");

    /// A small valid contract: one parameter per unit.
    fn fixture() -> Value {
        let grid = json!({
            "eumelanin": [0, 0.5, 1],
            "pheomelanin": [0, 1],
            "srgb": [["#f0e0d0", "#f0d8c0"], ["#a08070", "#a07860"], ["#402010", "#401c0a"]]
        });
        let parameter = |key: &str, group: &str, unit: &str, range: [f64; 2], default: f64| {
            json!({
                "key": key, "group": group, "unit": unit, "range": range, "default": default,
                "stability": "stable", "description": "A value."
            })
        };
        json!({
            "format_version": 1,
            "contract_version": "0.1.0",
            "retired_keys": ["old_axis"],
            "parameters": [
                parameter("body_form_age", "age", "years", [0.0, 130.0], 25.0),
                parameter("height_cm", "skeleton", "cm", [40.0, 230.0], 170.0),
                parameter("muscle_z", "build", "z", [-4.0, 4.0], 0.0),
                parameter("hair_curl", "hair", "fraction", [0.0, 1.0], 0.2),
                parameter("balding_stage", "hair", "stage", [0.0, 7.0], 0.0),
                parameter("iris_melanin", "pigment", "melanin", [0.0, 1.0], 0.5),
            ],
            "melanin_to_srgb": {
                "skin": grid.clone(),
                "hair": grid,
                "iris": {"melanin": [0, 0.5, 1], "srgb": ["#90b0d0", "#907040", "#301810"]}
            }
        })
    }

    /// The findings of the fixture after `change`.
    fn after(change: impl FnOnce(&mut Value)) -> Vec<String> {
        let mut contract = fixture();
        change(&mut contract);
        schema_findings(&contract.to_string())
    }

    fn one(finding: &str) -> Vec<String> {
        vec![finding.to_string()]
    }

    const NONE: Vec<String> = Vec::new();

    #[test]
    fn the_fixture_is_clean() {
        assert_eq!(after(|_| {}), NONE);
    }

    /// ADR-0005 acceptance test of step 4.
    #[test]
    fn contract_check_passes_on_repo() {
        assert_eq!(schema_findings(REPO_CONTRACT), NONE);
        assert_eq!(changelog_findings(REPO_CONTRACT, REPO_CHANGELOG), NONE);
    }

    /// ADR-0005 `melanin_table_monotone`: the tables of the repository keep the rule, at
    /// the support points 0, 0.25, 0.5, 0.75 and 1.
    #[test]
    fn melanin_table_monotone() {
        let body: Value = serde_json::from_str(REPO_CONTRACT).expect("json");
        let mut findings = Vec::new();
        melanin_findings(&body["melanin_to_srgb"], &mut findings);
        assert_eq!(findings, NONE);
        let points = json!([0, 0.25, 0.5, 0.75, 1]);
        for (table, axis) in [
            ("skin", "eumelanin"),
            ("skin", "pheomelanin"),
            ("hair", "eumelanin"),
            ("hair", "pheomelanin"),
            ("iris", "melanin"),
        ] {
            assert_eq!(
                body["melanin_to_srgb"][table][axis], points,
                "{table}.{axis}"
            );
        }
    }

    /// ADR-0005 `rejects_unknown_field`: unknown fields are a loading error at every level.
    #[test]
    fn rejects_unknown_field() {
        assert_eq!(
            after(|c| c["extra"] = json!(1)),
            one("body.json: unknown field extra")
        );
        assert_eq!(
            after(|c| c["parameters"][2]["note"] = json!("x")),
            one("parameter muscle_z: unknown field note")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["eyes"] = json!({})),
            one("melanin_to_srgb: unknown field eyes")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["gamma"] = json!(2.2)),
            one("melanin_to_srgb.skin: unknown field gamma")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["iris"]["pheomelanin"] = json!([0, 1])),
            one("melanin_to_srgb.iris: unknown field pheomelanin")
        );
    }

    #[test]
    fn rejects_missing_fields() {
        let without = |path: &[&str]| {
            after(|c| {
                let (last, parents) = path.split_last().expect("a path");
                let mut at = c;
                for step in parents {
                    at = match step.parse::<usize>() {
                        Ok(index) => &mut at[index],
                        Err(_) => &mut at[*step],
                    };
                }
                at.as_object_mut().expect("an object").remove(*last);
            })
        };
        for field in TOP_FIELDS {
            assert_eq!(
                without(&[field]),
                one(&format!("body.json: field {field} is missing"))
            );
        }
        for field in PARAMETER_FIELDS {
            let what = if field == "key" { "#2" } else { "muscle_z" };
            assert_eq!(
                without(&["parameters", "2", field]),
                one(&format!("parameter {what}: field {field} is missing")),
            );
        }
        assert_eq!(
            without(&["melanin_to_srgb", "hair"]),
            one("melanin_to_srgb: field hair is missing")
        );
        assert_eq!(
            without(&["melanin_to_srgb", "hair", "srgb"]),
            one("melanin_to_srgb.hair: field srgb is missing")
        );
        assert_eq!(
            without(&["melanin_to_srgb", "iris", "melanin"]),
            one("melanin_to_srgb.iris: field melanin is missing")
        );
        assert_eq!(schema_findings("[]"), one("body.json is not a JSON object"));
        assert_eq!(schema_findings("{"), one("body.json is not a JSON object"));
    }

    /// ADR-0005 `rejects_default_out_of_range`.
    #[test]
    fn rejects_default_out_of_range() {
        assert_eq!(
            after(|c| c["parameters"][1]["default"] = json!(39.5)),
            one("parameter height_cm: default 39.5 lies outside the range [40, 230]")
        );
        assert_eq!(
            after(|c| c["parameters"][1]["default"] = json!(231)),
            one("parameter height_cm: default 231 lies outside the range [40, 230]")
        );
        // The ends of the range are inside.
        assert_eq!(after(|c| c["parameters"][1]["default"] = json!(40)), NONE);
        assert_eq!(after(|c| c["parameters"][1]["default"] = json!(230)), NONE);
        assert_eq!(
            after(|c| c["parameters"][1]["range"] = json!([170, 170])),
            one("parameter height_cm: range needs min < max")
        );
        assert_eq!(
            after(|c| c["parameters"][1]["range"] = json!([230, 40])),
            one("parameter height_cm: range needs min < max")
        );
        for broken in [
            json!([40]),
            json!([40, 230, 300]),
            json!(["40", "230"]),
            json!(40),
        ] {
            assert_eq!(
                after(|c| c["parameters"][1]["range"] = broken.clone()),
                one("parameter height_cm: range must be [min, max] and default a number"),
                "{broken}"
            );
        }
        assert_eq!(
            after(|c| c["parameters"][1]["default"] = json!("170")),
            one("parameter height_cm: range must be [min, max] and default a number")
        );
    }

    /// ADR-0005 `rejects_z_range_not_pm4`, and the other units with a fixed range or
    /// default (section 3).
    #[test]
    fn rejects_z_range_not_pm4() {
        let unit = |index: usize, range: Value, default: Value| {
            after(|c| {
                c["parameters"][index]["range"] = range;
                c["parameters"][index]["default"] = default;
            })
        };
        for range in [json!([-3, 3]), json!([-4, 5]), json!([0, 4])] {
            assert_eq!(
                unit(2, range.clone(), json!(0)),
                one("parameter muscle_z: unit z needs the range [-4, 4]"),
                "{range}"
            );
        }
        assert_eq!(
            unit(2, json!([-4, 4]), json!(1)),
            one("parameter muscle_z: unit z needs the default 0")
        );
        assert_eq!(
            unit(0, json!([0, 120]), json!(25)),
            one("parameter body_form_age: unit years needs the range [0, 130]")
        );
        assert_eq!(
            unit(5, json!([0, 0.9]), json!(0.5)),
            one("parameter iris_melanin: unit melanin needs the range [0, 1]")
        );
        for range in [json!([0, 2]), json!([-1, 1])] {
            assert_eq!(
                unit(3, range.clone(), json!(0.2)),
                one("parameter hair_curl: unit fraction needs a range within [0, 1]"),
                "{range}"
            );
        }
        assert_eq!(unit(3, json!([0.1, 0.9]), json!(0.2)), NONE);
        for range in [json!([1, 7]), json!([0, 7.5])] {
            assert_eq!(
                unit(4, range.clone(), json!(1)),
                one("parameter balding_stage: unit stage needs whole bounds from 0"),
                "{range}"
            );
        }
        // An absolute measure has its own range.
        assert_eq!(unit(1, json!([10, 300]), json!(50)), NONE);
    }

    #[test]
    fn rejects_bad_names_and_lists() {
        for key in ["Muscle", "9lives", "muscle-z", "", "muskel_größe"] {
            assert_eq!(
                after(|c| c["parameters"][2]["key"] = json!(key)),
                one(&format!(
                    "parameter {key}: the key must match [a-z][a-z0-9_]*"
                )),
            );
        }
        assert_eq!(
            after(|c| c["parameters"][3]["key"] = json!("muscle_z")),
            one("parameter key muscle_z occurs twice")
        );
        assert_eq!(
            after(|c| c["retired_keys"] = json!(["old_axis", "old_axis"])),
            one("retired key old_axis is listed twice")
        );
        assert_eq!(
            after(|c| c["retired_keys"] = json!(["muscle_z"])),
            one("retired key muscle_z is an active parameter")
        );
        assert_eq!(
            after(|c| c["retired_keys"] = json!(["Old"])),
            one("retired key \"Old\" must match [a-z][a-z0-9_]*")
        );
        assert_eq!(
            after(|c| c["retired_keys"] = json!([7])),
            one("retired key 7 must match [a-z][a-z0-9_]*")
        );
        assert_eq!(
            after(|c| c["retired_keys"] = json!("old_axis")),
            one("body.json: retired_keys must be a list")
        );
        assert_eq!(after(|c| c["retired_keys"] = json!([])), NONE);
        assert_eq!(
            after(|c| c["parameters"] = json!([])),
            one("body.json: parameters must be a list with entries")
        );
        assert_eq!(
            after(|c| c["parameters"][2] = json!("muscle_z")),
            one("parameter #2 is not an object")
        );
        assert_eq!(
            after(|c| c["parameters"][2]["group"] = json!("eyes")),
            one("parameter muscle_z: unknown group \"eyes\"")
        );
        assert_eq!(
            after(|c| c["parameters"][2]["unit"] = json!("kg")),
            one("parameter muscle_z: unknown unit \"kg\"")
        );
        assert_eq!(
            after(|c| c["parameters"][2]["stability"] = json!("final")),
            one("parameter muscle_z: stability must be stable or provisional")
        );
        for description in ["", "two\nlines", "Größe", "tab\there"] {
            assert_eq!(
                after(|c| c["parameters"][2]["description"] = json!(description)),
                one("parameter muscle_z: description must be one line of ASCII text"),
                "{description:?}"
            );
        }
        assert_eq!(GROUPS.len(), 11);
        for group in GROUPS {
            assert_eq!(after(|c| c["parameters"][2]["group"] = json!(group)), NONE);
        }
    }

    #[test]
    fn rejects_bad_versions() {
        for version in [
            "0.1",
            "0.1.0-beta",
            "00.1.0",
            "0.01.0",
            "v0.1.0",
            "0.1.0.0",
            "0..1",
            "",
        ] {
            assert_eq!(
                after(|c| c["contract_version"] = json!(version)),
                one(&format!(
                    "body.json: contract_version \"{version}\" is not MAJOR.MINOR.PATCH"
                )),
            );
        }
        assert_eq!(
            after(|c| c["contract_version"] = json!(1)),
            one("body.json: contract_version 1 is not MAJOR.MINOR.PATCH")
        );
        for version in ["0.1.0", "10.0.3", "1.20.300"] {
            assert_eq!(after(|c| c["contract_version"] = json!(version)), NONE);
        }
        for format in [json!(2), json!(0), json!("1")] {
            assert_eq!(
                after(|c| c["format_version"] = format.clone()),
                one("body.json: format_version must be 1"),
                "{format}"
            );
        }
    }

    #[test]
    fn rejects_broken_colour_tables() {
        for axis in [
            json!([0, 0.5, 0.5]),
            json!([0.1, 0.5, 1]),
            json!([0, 0.5, 0.9]),
            json!([0, 1, 0.5]),
            json!(["0", "1"]),
        ] {
            let found = after(|c| c["melanin_to_srgb"]["skin"]["eumelanin"] = axis.clone());
            assert_eq!(
                found.first().map(String::as_str),
                Some("melanin_to_srgb.skin.eumelanin must rise strictly from 0 to 1"),
                "{axis}"
            );
        }
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["eumelanin"] = json!([0, 1])),
            one("melanin_to_srgb.skin.srgb must have 2 rows of 2 colours")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["pheomelanin"] = json!([0, 0.5, 1])),
            one("melanin_to_srgb.skin.srgb must have 3 rows of 3 colours")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["iris"]["srgb"] = json!(["#90b0d0", "#907040"])),
            one("melanin_to_srgb.iris.srgb must have 3 colours")
        );
        for colour in [
            "#F0E0D0",
            "#000000",
            "f0e0d0",
            "#fff",
            "#f0e0d0ff",
            "#g0e0d0",
        ] {
            assert_eq!(
                after(|c| c["melanin_to_srgb"]["hair"]["srgb"][2][1] = json!(colour)),
                one(&format!(
                    "melanin_to_srgb.hair.srgb[2][1]: \"{colour}\" is not a colour #rrggbb in lower case other than #000000"
                )),
            );
        }
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["iris"]["srgb"][0] = json!(255)),
            one(
                "melanin_to_srgb.iris.srgb[0]: 255 is not a colour #rrggbb in lower case other than #000000"
            )
        );
    }

    /// ADR-0005 `rejects_melanin_not_monotone`: on linear values, luminance does not rise
    /// with eumelanin and the red share does not fall with pheomelanin.
    #[test]
    fn rejects_melanin_not_monotone() {
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["srgb"][1][0] = json!("#ffffff")),
            one("melanin_to_srgb.skin: luminance rises with eumelanin at srgb[1][0]")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["hair"]["srgb"][2][1] = json!("#f04020")),
            one("melanin_to_srgb.hair: luminance rises with eumelanin at srgb[2][1]")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["srgb"][0][1] = json!("#d0e0f0")),
            one("melanin_to_srgb.skin: the red share falls with pheomelanin at srgb[0][1]")
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["iris"]["srgb"][2] = json!("#ffffff")),
            one("melanin_to_srgb.iris: luminance rises with melanin at srgb[2]")
        );
        // Equal neighbours keep the rule: it says "does not rise" and "does not fall".
        assert_eq!(
            after(|c| {
                c["melanin_to_srgb"]["skin"]["srgb"][1] = json!(["#f0e0d0", "#f0d8c0"]);
                c["melanin_to_srgb"]["skin"]["srgb"][0][1] = json!("#f0e0d0");
            }),
            NONE
        );
    }

    /// IEC 61966-2-1: the curve of the reader.
    #[test]
    fn linear_light_follows_the_srgb_curve() {
        assert_eq!(linear(0), 0.0);
        assert_eq!(linear(255), 1.0);
        // Below the threshold 0.04045 (level 10 is 0.0392) the curve is a line.
        assert_eq!(linear(10), 10.0 / 255.0 / 12.92);
        // The published value of the middle level.
        assert!((linear(128) - 0.215_860_500_113_899_2).abs() < 1e-15);
        for level in 1..=u8::MAX {
            assert!(linear(level) > linear(level - 1), "level {level}");
        }
        assert!((luminance([255, 255, 255]) - 1.0).abs() < 1e-15);
        assert!((red_share([10, 10, 10]) - 1.0 / 3.0).abs() < 1e-15);
        assert!(red_share([200, 100, 100]) > red_share([200, 150, 150]));
    }

    /// ADR-0005 `rejects_missing_changelog_entry`.
    #[test]
    fn rejects_missing_changelog_entry() {
        let body = fixture().to_string();
        let complete = "# Changelog\n\n## 0.1.0\n\n- Neu: alles.\n- Geändert: nichts.\n- Stillgelegt: nichts.\n";
        assert_eq!(changelog_findings(&body, complete), NONE);
        assert_eq!(
            changelog_findings(&body, "# Changelog\n\n## 0.2.0\n\n- Neu: x.\n"),
            one("body.changelog.md has no section ## 0.1.0")
        );
        assert_eq!(
            changelog_findings(&body, ""),
            one("body.changelog.md has no section ## 0.1.0")
        );
        // The entries of another version do not count.
        let other = "## 0.1.0\n\n- Neu: alles.\n\n## 0.0.9\n\n- Geändert: x.\n- Stillgelegt: y.\n";
        assert_eq!(
            changelog_findings(&body, other),
            vec![
                "body.changelog.md: section ## 0.1.0 has no entry \"- Geändert: ...\"".to_string(),
                "body.changelog.md: section ## 0.1.0 has no entry \"- Stillgelegt: ...\""
                    .to_string(),
            ]
        );
        assert_eq!(
            changelog_findings("{}", complete),
            one("body.json has no contract_version to look up in the changelog")
        );
    }
}
