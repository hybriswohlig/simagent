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

impl Class {
    const ALL: [Class; 3] = [Class::Correction, Class::Extension, Class::Breaking];

    fn name(self) -> &'static str {
        match self {
            Class::Correction => "a correction",
            Class::Extension => "an extending change",
            Class::Breaking => "a breaking change",
        }
    }
}

/// `value` with every number as f64: `25` and `25.0` are the same number, the way of
/// writing it is formatting.
fn numeric(value: &Value) -> Value {
    match value {
        Value::Number(number) => number.as_f64().map_or(Value::Null, Value::from),
        Value::Array(values) => values.iter().map(numeric).collect(),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), numeric(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The active parameters with their keys.
fn parameters(body: &Value) -> Vec<(&str, &Value)> {
    body["parameters"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|parameter| Some((parameter["key"].as_str()?, parameter)))
                .collect()
        })
        .unwrap_or_default()
}

fn retired(body: &Value) -> Vec<&str> {
    body["retired_keys"]
        .as_array()
        .map(|keys| keys.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The melanin tables without their colours: which tables there are and their support
/// points. The schema check ties the size of the colour lists to the support points.
fn table_layout(body: &Value) -> Value {
    let mut tables = numeric(&body["melanin_to_srgb"]);
    if let Some(tables) = tables.as_object_mut() {
        for table in tables.values_mut() {
            if let Some(table) = table.as_object_mut() {
                table.remove("srgb");
            }
        }
    }
    tables
}

/// The class of the change from `base` to `head`, from the comparison of the fields
/// (ADR-0005 section 6). A field that keeps its text and gets a new meaning, and a new
/// parameter whose default does not fit every age and sex, are breaking as well; only a
/// review sees that.
pub fn change_class(base: &Value, head: &Value) -> Class {
    let mut class = Class::Correction;
    let after = parameters(head);
    let before = parameters(base);
    for (key, old) in &before {
        let Some((_, new)) = after.iter().find(|(other, _)| other == key) else {
            // Retired or renamed.
            class = Class::Breaking;
            continue;
        };
        let differs = |field: &str| numeric(&old[field]) != numeric(&new[field]);
        if differs("unit")
            || differs("range")
            || (old["stability"] == "stable" && differs("stability"))
        {
            class = Class::Breaking;
        }
        if differs("group") || differs("default") || differs("stability") {
            class = class.max(Class::Extension);
        }
    }
    let new_key = |(key, _): &(&str, &Value)| !before.iter().any(|(other, _)| other == key);
    if after.iter().any(new_key) {
        class = class.max(Class::Extension);
    }
    if base["format_version"] != head["format_version"] || table_layout(base) != table_layout(head)
    {
        class = Class::Breaking;
    }
    if numeric(&base["melanin_to_srgb"]) != numeric(&head["melanin_to_srgb"]) {
        class = class.max(Class::Extension);
    }
    class
}

/// MAJOR, MINOR and PATCH of a version.
fn numbers(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
    let numbers = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(numbers)
}

/// The version one step of `class` after `base` (ADR-0005 section 6). Below 1.0.0 a
/// breaking change raises MINOR and every other change PATCH; from 1.0.0 on the classes
/// raise MAJOR, MINOR and PATCH. The places to the right of the raised one become 0.
fn step(base: [u64; 3], class: Class) -> Option<[u64; 3]> {
    let [major, minor, patch] = base;
    Some(match (major, class) {
        (0, Class::Breaking) => [0, minor.checked_add(1)?, 0],
        (0, _) => [0, minor, patch.checked_add(1)?],
        (_, Class::Breaking) => [major.checked_add(1)?, 0, 0],
        (_, Class::Extension) => [major, minor.checked_add(1)?, 0],
        (_, Class::Correction) => [major, minor, patch.checked_add(1)?],
    })
}

/// Every key of the base stays active or retired, `retired_keys` only grows, and no
/// retired key becomes a parameter again.
fn key_findings(base: &Value, head: &Value, findings: &mut Vec<String>) {
    let active: Vec<&str> = parameters(head).iter().map(|(key, _)| *key).collect();
    let retired_now = retired(head);
    for (key, _) in parameters(base) {
        if !active.contains(&key) && !retired_now.contains(&key) {
            findings.push(format!(
                "parameter {key} of the merge base is gone: a key that leaves the contract goes to retired_keys"
            ));
        }
    }
    for key in retired(base) {
        if active.contains(&key) {
            findings.push(format!(
                "retired key {key} is a parameter again: keys are never used again"
            ));
        } else if !retired_now.contains(&key) {
            findings.push(format!(
                "retired key {key} of the merge base is gone: retired_keys only grows"
            ));
        }
    }
}

/// A changed contract carries the version exactly one step after the base: the step of
/// the class of its change, or of a higher class (a new meaning is breaking, and only the
/// author and the review know).
fn version_findings(base: &Value, head: &Value, findings: &mut Vec<String>) {
    let before = base["contract_version"].as_str().unwrap_or_default();
    let now = head["contract_version"].as_str().unwrap_or_default();
    let (Some(from), Some(to)) = (numbers(before), numbers(now)) else {
        findings.push(format!(
            "cannot compare contract_version {now} with {before} of the merge base"
        ));
        return;
    };
    let class = change_class(base, head);
    let mut allowed: Vec<[u64; 3]> = Class::ALL
        .into_iter()
        .filter(|other| *other >= class)
        .filter_map(|other| step(from, other))
        .collect();
    allowed.dedup();
    if !allowed.contains(&to) {
        let names: Vec<String> = allowed
            .iter()
            .map(|[major, minor, patch]| format!("{major}.{minor}.{patch}"))
            .collect();
        findings.push(format!(
            "contract_version {now} after {before} of the merge base: {} needs {}",
            class.name(),
            names.join(" or ")
        ));
    }
}

/// The entries of the hash file of the merge base are still there and unchanged
/// (ADR-0005 section 7, checks 2 and 3).
pub fn hash_findings(base_hashes: Option<&str>, hashes_json: &str) -> Vec<String> {
    let entries = |text: &str| {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    };
    let now = entries(hashes_json);
    entries(base_hashes.unwrap_or("{}"))
        .iter()
        .filter(|(version, hash)| now.get(*version) != Some(*hash))
        .map(|(version, _)| {
            format!(
                "body.hashes.json: the entry {version} of the merge base was changed or removed: a changed contract needs a new version"
            )
        })
        .collect()
}

/// ADR-0005 section 7, check 3: the contract against the merge base. Expects a contract
/// that passes the schema check.
pub fn base_findings(base: &Base, body_json: &str, hashes_json: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let head: Value = serde_json::from_str(body_json).unwrap_or_default();
    match &base.body {
        None => {
            let version = head["contract_version"].as_str().unwrap_or_default();
            if version != "0.1.0" {
                findings.push(format!(
                    "the merge base has no contracts/body.json: the first contract_version is 0.1.0, not {version}"
                ));
            }
        }
        Some(text) if text == body_json => {}
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(old) => {
                key_findings(&old, &head, &mut findings);
                version_findings(&old, &head, &mut findings);
            }
            Err(_) => {
                findings.push("contracts/body.json of the merge base is not JSON".to_string());
            }
        },
    }
    findings.extend(hash_findings(base.hashes.as_deref(), hashes_json));
    findings
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
        // A version that does not fit into the numbers of the comparison, or that is not
        // three numbers.
        for version in ["0.1.99999999999999999999", "0.1.1.0", "0.1", "0.1.x", ""] {
            let odd = changed(|c| c["contract_version"] = json!(version));
            assert_eq!(
                base_findings(&base, &odd.to_string(), "{}"),
                vec![format!(
                    "cannot compare contract_version {version} with 0.1.0 of the merge base"
                )]
            );
        }
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
