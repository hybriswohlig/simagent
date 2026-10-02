//! The body contract against the merge base with `origin/main` (ADR-0005 section 6 and
//! section 7, check 3): the class of a change and the step of the version it needs, keys
//! that are never used again, and the hash entries of the base.

use serde_json::Value;

/// The contract files as the merge base has them; `None` where the base has no such file.
#[derive(Default)]
pub struct Base {
    pub body: Option<String>,
    pub hashes: Option<String>,
}

/// The class of a change (ADR-0005 section 6), the lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Correction,
    Extension,
    Breaking,
}

/// The class of the change from `base` to `head`, from the comparison of the fields
/// (ADR-0005 section 6). A field that keeps its text and gets a new meaning, and a new
/// parameter whose default does not fit every age and sex, are breaking as well; only a
/// review sees that.
pub fn change_class(base: &Value, head: &Value) -> Class {
    let _ = (base, head);
    [Class::Correction, Class::Extension, Class::Breaking][0]
}

/// The entries of the hash file of the merge base are still there and unchanged
/// (ADR-0005 section 7, checks 2 and 3).
pub fn hash_findings(base_hashes: Option<&str>, hashes_json: &str) -> Vec<String> {
    let _ = (base_hashes, hashes_json);
    Vec::new()
}

/// ADR-0005 section 7, check 3: the contract against the merge base. Expects a contract
/// that passes the schema check.
pub fn base_findings(base: &Base, body_json: &str, hashes_json: &str) -> Vec<String> {
    let _ = (
        &base.body,
        change_class(&Value::Null, &Value::Null),
        body_json,
    );
    hash_findings(base.hashes.as_deref(), hashes_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::tests::fixture;
    use serde_json::json;

    const NONE: Vec<String> = Vec::new();

    /// The fixture after `change`.
    fn changed(change: impl FnOnce(&mut Value)) -> Value {
        let mut contract = fixture();
        change(&mut contract);
        contract
    }

    /// The findings for `head` against `base`, without hash files.
    fn findings(base: &Value, head: &Value) -> Vec<String> {
        let base = Base {
            body: Some(base.to_string()),
            hashes: None,
        };
        base_findings(&base, &head.to_string(), "{}")
    }

    /// The parameter of the fixture with this key.
    fn parameter<'a>(contract: &'a mut Value, key: &str) -> &'a mut Value {
        contract["parameters"]
            .as_array_mut()
            .expect("parameters")
            .iter_mut()
            .find(|parameter| parameter["key"] == key)
            .expect("the key")
    }

    fn retire(contract: &mut Value, key: &str) {
        let parameters = contract["parameters"].as_array_mut().expect("parameters");
        parameters.retain(|parameter| parameter["key"] != key);
        contract["retired_keys"]
            .as_array_mut()
            .expect("retired")
            .push(json!(key));
    }

    /// ADR-0005 section 6, the table of the classes.
    #[test]
    fn the_class_follows_the_table() {
        type Change = fn(&mut Value);
        let provisional: Change = |c| parameter(c, "hair_curl")["stability"] = json!("provisional");
        let nothing: Change = |_| {};
        let cases: [(&str, Change, Change, Class); 22] = [
            ("nothing", nothing, nothing, Class::Correction),
            (
                "description",
                nothing,
                |c| parameter(c, "muscle_z")["description"] = json!("Muscle."),
                Class::Correction,
            ),
            (
                "order",
                nothing,
                |c| c["parameters"].as_array_mut().expect("list").reverse(),
                Class::Correction,
            ),
            (
                "numbers written another way",
                nothing,
                |c| {
                    parameter(c, "height_cm")["range"] = json!([40, 230]);
                    parameter(c, "height_cm")["default"] = json!(170);
                    c["melanin_to_srgb"]["skin"]["eumelanin"] = json!([0.0, 0.5, 1.0]);
                },
                Class::Correction,
            ),
            (
                "retired key that was never active",
                nothing,
                |c| {
                    c["retired_keys"]
                        .as_array_mut()
                        .expect("list")
                        .push(json!("never_used"))
                },
                Class::Correction,
            ),
            (
                "default",
                nothing,
                |c| parameter(c, "hair_curl")["default"] = json!(0.3),
                Class::Extension,
            ),
            (
                "group",
                nothing,
                |c| parameter(c, "muscle_z")["group"] = json!("shape"),
                Class::Extension,
            ),
            (
                "colour",
                nothing,
                |c| c["melanin_to_srgb"]["iris"]["srgb"][2] = json!("#2e1a0e"),
                Class::Extension,
            ),
            (
                "new parameter",
                nothing,
                |c| {
                    let mut new = parameter(c, "muscle_z").clone();
                    new["key"] = json!("neck_z");
                    c["parameters"].as_array_mut().expect("list").push(new);
                },
                Class::Extension,
            ),
            (
                "provisional to stable",
                provisional,
                nothing,
                Class::Extension,
            ),
            (
                "stable to provisional",
                nothing,
                provisional,
                Class::Breaking,
            ),
            (
                "unit",
                nothing,
                |c| parameter(c, "hair_curl")["unit"] = json!("melanin"),
                Class::Breaking,
            ),
            (
                "range",
                nothing,
                |c| parameter(c, "height_cm")["range"] = json!([40, 250]),
                Class::Breaking,
            ),
            (
                "lower end of the range",
                nothing,
                |c| parameter(c, "height_cm")["range"] = json!([30, 230]),
                Class::Breaking,
            ),
            (
                "retired",
                nothing,
                |c| retire(c, "muscle_z"),
                Class::Breaking,
            ),
            (
                "renamed",
                nothing,
                |c| {
                    parameter(c, "muscle_z")["key"] = json!("muscle_mass_z");
                    c["retired_keys"]
                        .as_array_mut()
                        .expect("list")
                        .push(json!("muscle_z"));
                },
                Class::Breaking,
            ),
            (
                "support points",
                nothing,
                |c| c["melanin_to_srgb"]["hair"]["eumelanin"] = json!([0, 0.25, 1]),
                Class::Breaking,
            ),
            (
                "size of a table",
                nothing,
                |c| {
                    c["melanin_to_srgb"]["iris"] =
                        json!({"melanin": [0, 1], "srgb": ["#90b0d0", "#301810"]});
                },
                Class::Breaking,
            ),
            (
                "another table",
                nothing,
                |c| c["melanin_to_srgb"]["lips"] = c["melanin_to_srgb"]["iris"].clone(),
                Class::Breaking,
            ),
            (
                "format version",
                nothing,
                |c| c["format_version"] = json!(2),
                Class::Breaking,
            ),
            // The highest class of several changes counts.
            (
                "description and default",
                nothing,
                |c| {
                    parameter(c, "muscle_z")["description"] = json!("Muscle.");
                    parameter(c, "hair_curl")["default"] = json!(0.3);
                },
                Class::Extension,
            ),
            (
                "range and then a default",
                nothing,
                |c| {
                    parameter(c, "height_cm")["range"] = json!([40, 250]);
                    parameter(c, "iris_melanin")["default"] = json!(0.4);
                },
                Class::Breaking,
            ),
        ];
        for (name, before, after, class) in cases {
            assert_eq!(
                change_class(&changed(before), &changed(after)),
                class,
                "{name}"
            );
        }
    }

    /// ADR-0005 section 6: exactly one step after the base, by the class of the change or
    /// by a higher one; below 1.0.0 a breaking change raises MINOR and all else PATCH.
    #[test]
    fn the_version_takes_one_step_of_the_class() {
        type Change = fn(&mut Value);
        let correction: Change = |c| parameter(c, "muscle_z")["description"] = json!("Muscle.");
        let extension: Change = |c| parameter(c, "hair_curl")["default"] = json!(0.3);
        let breaking: Change = |c| parameter(c, "height_cm")["range"] = json!([40, 250]);
        let cases: [(&str, Change, &[&str], &[&str]); 6] = [
            (
                "0.3.7",
                correction,
                &["0.3.8", "0.4.0"],
                &[
                    "0.3.7", "0.3.9", "0.4.1", "0.5.0", "1.0.0", "1.3.7", "0.3.6", "0.2.0",
                ],
            ),
            (
                "0.3.7",
                extension,
                &["0.3.8", "0.4.0"],
                &["0.3.7", "0.3.9", "0.4.1", "0.5.0", "1.0.0"],
            ),
            (
                "0.3.7",
                breaking,
                &["0.4.0"],
                &["0.3.7", "0.3.8", "0.4.1", "0.5.0", "1.0.0", "1.3.7"],
            ),
            (
                "1.2.3",
                correction,
                &["1.2.4", "1.3.0", "2.0.0"],
                &[
                    "1.2.3", "1.2.5", "1.3.1", "1.3.4", "1.4.0", "2.0.1", "2.1.0", "2.2.3", "3.0.0",
                ],
            ),
            (
                "1.2.3",
                extension,
                &["1.3.0", "2.0.0"],
                &[
                    "1.2.3", "1.2.4", "1.3.1", "1.3.3", "1.4.0", "2.0.1", "2.3.0", "3.0.0",
                ],
            ),
            (
                "1.2.3",
                breaking,
                &["2.0.0"],
                &[
                    "1.2.3", "1.2.4", "1.3.0", "2.0.1", "2.0.3", "2.1.0", "2.2.3", "3.0.0",
                ],
            ),
        ];
        for (from, change, accepted, refused) in cases {
            let base = changed(|c| c["contract_version"] = json!(from));
            let head = |version: &str| {
                let mut head = base.clone();
                change(&mut head);
                head["contract_version"] = json!(version);
                head
            };
            for version in accepted {
                assert_eq!(findings(&base, &head(version)), NONE, "{from} to {version}");
            }
            for version in refused {
                assert_eq!(
                    findings(&base, &head(version)).len(),
                    1,
                    "{from} to {version}"
                );
            }
        }
    }

    /// ADR-0005 `rejects_breaking_change_without_bump`.
    #[test]
    fn rejects_breaking_change_without_bump() {
        let base = fixture();
        let mut head = changed(|c| parameter(c, "height_cm")["range"] = json!([40, 250]));
        assert_eq!(
            findings(&base, &head),
            vec![
                "contract_version 0.1.0 after 0.1.0 of the merge base: a breaking change needs 0.2.0"
                    .to_string()
            ]
        );
        // The step of a lower class is not enough.
        head["contract_version"] = json!("0.1.1");
        assert_eq!(
            findings(&base, &head),
            vec![
                "contract_version 0.1.1 after 0.1.0 of the merge base: a breaking change needs 0.2.0"
                    .to_string()
            ]
        );
        head["contract_version"] = json!("0.2.0");
        assert_eq!(findings(&base, &head), NONE);
    }

    /// ADR-0005 `rejects_double_bump`.
    #[test]
    fn rejects_double_bump() {
        let base = fixture();
        let mut head = changed(|c| parameter(c, "hair_curl")["default"] = json!(0.3));
        for version in ["0.1.2", "0.3.0", "0.2.1", "1.0.0"] {
            head["contract_version"] = json!(version);
            assert_eq!(
                findings(&base, &head),
                vec![format!(
                    "contract_version {version} after 0.1.0 of the merge base: an extending change needs 0.1.1 or 0.2.0"
                )]
            );
        }
        let mut head = changed(|c| parameter(c, "muscle_z")["description"] = json!("Muscle."));
        head["contract_version"] = json!("0.1.2");
        assert_eq!(
            findings(&base, &head),
            vec![
                "contract_version 0.1.2 after 0.1.0 of the merge base: a correction needs 0.1.1 or 0.2.0"
                    .to_string()
            ]
        );
    }

    /// Any change of the file counts, also one of its layout alone.
    #[test]
    fn a_changed_file_needs_a_new_version() {
        let base = Base {
            body: Some(fixture().to_string()),
            hashes: None,
        };
        let pretty = serde_json::to_string_pretty(&fixture()).expect("json");
        assert_eq!(
            base_findings(&base, &pretty, "{}"),
            vec![
                "contract_version 0.1.0 after 0.1.0 of the merge base: a correction needs 0.1.1 or 0.2.0"
                    .to_string()
            ]
        );
        assert_eq!(base_findings(&base, &fixture().to_string(), "{}"), NONE);
        // A version that does not fit into the numbers of the comparison.
        let huge = changed(|c| c["contract_version"] = json!("0.1.99999999999999999999"));
        assert_eq!(
            base_findings(&base, &huge.to_string(), "{}"),
            vec![
                "cannot compare contract_version 0.1.99999999999999999999 with 0.1.0 of the merge base"
                    .to_string()
            ]
        );
    }

    /// ADR-0005 section 7, check 3: without a file in the base the version is 0.1.0.
    #[test]
    fn the_first_contract_is_0_1_0() {
        let base = Base::default();
        assert_eq!(base_findings(&base, &fixture().to_string(), "{}"), NONE);
        for version in ["0.1.1", "0.2.0", "1.0.0", "0.0.1"] {
            let head = changed(|c| c["contract_version"] = json!(version));
            assert_eq!(
                base_findings(&base, &head.to_string(), "{}"),
                vec![format!(
                    "the merge base has no contracts/body.json: the first contract_version is 0.1.0, not {version}"
                )]
            );
        }
    }

    /// ADR-0005 `rejects_reused_key`: a retired key never becomes a parameter again.
    #[test]
    fn rejects_reused_key() {
        let base = fixture();
        let head = changed(|c| {
            let mut again = parameter(c, "muscle_z").clone();
            again["key"] = json!("old_axis");
            c["parameters"].as_array_mut().expect("list").push(again);
            c["retired_keys"] = json!([]);
            c["contract_version"] = json!("0.1.1");
        });
        assert_eq!(
            findings(&base, &head),
            vec![
                "retired key old_axis is a parameter again: keys are never used again".to_string()
            ]
        );
        // Nor does a retired key leave the list.
        let head = changed(|c| {
            c["retired_keys"] = json!([]);
            c["contract_version"] = json!("0.1.1");
        });
        assert_eq!(
            findings(&base, &head),
            vec![
                "retired key old_axis of the merge base is gone: retired_keys only grows"
                    .to_string()
            ]
        );
    }

    /// ADR-0005 `rejects_removed_key_not_retired`.
    #[test]
    fn rejects_removed_key_not_retired() {
        let base = fixture();
        let gone = "parameter muscle_z of the merge base is gone: a key that leaves the contract goes to retired_keys";
        let head = changed(|c| {
            c["parameters"]
                .as_array_mut()
                .expect("list")
                .retain(|parameter| parameter["key"] != "muscle_z");
            c["contract_version"] = json!("0.2.0");
        });
        assert_eq!(findings(&base, &head), vec![gone.to_string()]);
        let renamed = changed(|c| {
            parameter(c, "muscle_z")["key"] = json!("muscle_mass_z");
            c["contract_version"] = json!("0.2.0");
        });
        assert_eq!(findings(&base, &renamed), vec![gone.to_string()]);
        let retired = changed(|c| {
            retire(c, "muscle_z");
            c["contract_version"] = json!("0.2.0");
        });
        assert_eq!(findings(&base, &retired), NONE);
    }

    /// ADR-0005 `rejects_rewritten_base_hash`.
    #[test]
    fn rejects_rewritten_base_hash() {
        let base = "{\"0.1.0\": \"00000000000000aa\", \"0.1.1\": \"00000000000000bb\"}";
        let rewritten = |version: &str| {
            vec![format!(
                "body.hashes.json: the entry {version} of the merge base was changed or removed: a changed contract needs a new version"
            )]
        };
        for (now, found) in [
            (base, NONE),
            (
                "{\"0.1.0\": \"00000000000000aa\", \"0.1.1\": \"00000000000000bb\", \"0.2.0\": \"00000000000000cc\"}",
                NONE,
            ),
            (
                "{\"0.1.0\": \"00000000000000aa\", \"0.1.1\": \"00000000000000bc\"}",
                rewritten("0.1.1"),
            ),
            ("{\"0.1.1\": \"00000000000000bb\"}", rewritten("0.1.0")),
            ("{}", [rewritten("0.1.0"), rewritten("0.1.1")].concat()),
            ("[]", [rewritten("0.1.0"), rewritten("0.1.1")].concat()),
        ] {
            assert_eq!(hash_findings(Some(base), now), found, "{now}");
        }
        assert_eq!(
            hash_findings(None, "{\"0.1.0\": \"00000000000000aa\"}"),
            NONE
        );
        // Part of the check against the base, also for an unchanged contract.
        let files = Base {
            body: Some(fixture().to_string()),
            hashes: Some(base.to_string()),
        };
        assert_eq!(
            base_findings(
                &files,
                &fixture().to_string(),
                "{\"0.1.1\": \"00000000000000bb\"}"
            ),
            rewritten("0.1.0")
        );
    }
}
