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

/// Reports fields of `object` that are missing or unknown; `what` names the object.
fn field_findings(object: &Map<String, Value>, fields: &[&str], what: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for field in fields {
        if !object.contains_key(*field) {
            findings.push(format!("{what}: field {field} is missing"));
        }
    }
    for field in object.keys() {
        if !fields.contains(&field.as_str()) {
            findings.push(format!("{what}: unknown field {field}"));
        }
    }
    findings
}

/// `^[a-z][a-z0-9_]*$`
fn is_key(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `MAJOR.MINOR.PATCH`: three decimal numbers without sign, leading zero or suffix.
pub fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

/// The rule of a unit on range and default (ADR-0005 section 3), as a finding or none.
fn unit_finding(unit: &str, min: f64, max: f64, default: f64) -> Option<&'static str> {
    let whole = |x: f64| sim_math::to_i64_exact(x).is_some();
    match unit {
        "years" if (min, max) != (0.0, 130.0) => Some("unit years needs the range [0, 130]"),
        "z" if (min, max) != (-4.0, 4.0) => Some("unit z needs the range [-4, 4]"),
        "z" if default != 0.0 => Some("unit z needs the default 0"),
        "melanin" if (min, max) != (0.0, 1.0) => Some("unit melanin needs the range [0, 1]"),
        "fraction" if min < 0.0 || max > 1.0 => Some("unit fraction needs a range within [0, 1]"),
        "stage" if min != 0.0 || !whole(max) => Some("unit stage needs whole bounds from 0"),
        "years" | "cm" | "z" | "fraction" | "stage" | "melanin" => None,
        _ => Some("unknown unit"),
    }
}

fn parameter_findings(
    index: usize,
    parameter: &Value,
    findings: &mut Vec<String>,
) -> Option<String> {
    let Some(object) = parameter.as_object() else {
        findings.push(format!("parameter #{index} is not an object"));
        return None;
    };
    let key = object.get("key").and_then(Value::as_str);
    let what = match key {
        Some(key) => format!("parameter {key}"),
        None => format!("parameter #{index}"),
    };
    let before = findings.len();
    findings.extend(field_findings(object, &PARAMETER_FIELDS, &what));
    if findings.len() > before {
        return key.map(str::to_string);
    }
    let text = |field: &str| {
        object
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    if !key.is_some_and(is_key) {
        findings.push(format!("{what}: the key must match [a-z][a-z0-9_]*"));
    }
    if !GROUPS.contains(&text("group")) {
        findings.push(format!("{what}: unknown group {}", object["group"]));
    }
    if !["stable", "provisional"].contains(&text("stability")) {
        findings.push(format!("{what}: stability must be stable or provisional"));
    }
    let description = text("description");
    let one_line = !description.is_empty()
        && description
            .bytes()
            .all(|b| b == b' ' || b.is_ascii_graphic());
    if !one_line {
        findings.push(format!(
            "{what}: description must be one line of ASCII text"
        ));
    }
    let range: Vec<f64> = object["range"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let numbers = object["range"]
        .as_array()
        .is_some_and(|values| values.len() == 2);
    let (Some(default), [min, max], true) = (object["default"].as_f64(), &range[..], numbers)
    else {
        findings.push(format!(
            "{what}: range must be [min, max] and default a number"
        ));
        return key.map(str::to_string);
    };
    if min >= max {
        findings.push(format!("{what}: range needs min < max"));
    } else if default < *min || default > *max {
        findings.push(format!(
            "{what}: default {default} lies outside the range [{min}, {max}]"
        ));
    }
    if let Some(rule) = unit_finding(text("unit"), *min, *max, default) {
        if rule == "unknown unit" {
            findings.push(format!("{what}: unknown unit {}", object["unit"]));
        } else {
            findings.push(format!("{what}: {rule}"));
        }
    }
    key.map(str::to_string)
}

/// The channels of `#rrggbb` in lower case; `#000000` is no colour of the table.
fn colour(value: &Value) -> Option<[u8; 3]> {
    let digits = value.as_str()?.strip_prefix('#')?;
    let lower_hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    if digits.len() != 6 || !digits.bytes().all(lower_hex) || digits == "000000" {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// An 8-bit sRGB channel in linear light (IEC 61966-2-1).
fn linear(level: u8) -> f64 {
    let c = f64::from(level) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        sim_math::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// Relative luminance of a colour, on linear values.
fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// Share of red in a colour, on linear values. The sum is positive: no colour is black.
fn red_share(rgb: [u8; 3]) -> f64 {
    let [r, g, b] = rgb.map(linear);
    r / (r + g + b)
}

/// The support points of an axis: strictly rising from 0 to 1.
fn axis(table: &Map<String, Value>, name: &str, what: &str, findings: &mut Vec<String>) -> usize {
    let values: Option<Vec<f64>> = table[name]
        .as_array()
        .map(|values| values.iter().map(Value::as_f64).collect())
        .unwrap_or_default();
    let rises = values.as_ref().is_some_and(|values| {
        values.len() >= 2
            && values.first() == Some(&0.0)
            && values.last() == Some(&1.0)
            && values.windows(2).all(|pair| pair[0] < pair[1])
    });
    if !rises {
        findings.push(format!("{what}.{name} must rise strictly from 0 to 1"));
    }
    values.map_or(0, |values| values.len())
}

/// A grid over eumelanin and pheomelanin (skin, hair).
fn grid_findings(name: &str, table: &Value, findings: &mut Vec<String>) {
    let what = format!("melanin_to_srgb.{name}");
    let Some(table) = table.as_object() else {
        findings.push(format!("{what} is not an object"));
        return;
    };
    let before = findings.len();
    findings.extend(field_findings(
        table,
        &["eumelanin", "pheomelanin", "srgb"],
        &what,
    ));
    if findings.len() > before {
        return;
    }
    let rows = axis(table, "eumelanin", &what, findings);
    let columns = axis(table, "pheomelanin", &what, findings);
    // An entry that is no row stays in the count, as a row without colours.
    let srgb: Vec<&[Value]> = table["srgb"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row.as_array().map_or(&[][..], Vec::as_slice))
                .collect()
        })
        .unwrap_or_default();
    if srgb.len() != rows || srgb.iter().any(|row| row.len() != columns) {
        findings.push(format!(
            "{what}.srgb must have {rows} rows of {columns} colours"
        ));
        return;
    }
    let mut colours = Vec::new();
    for (i, row) in srgb.iter().enumerate() {
        let mut parsed = Vec::new();
        for (j, value) in row.iter().enumerate() {
            match colour(value) {
                Some(rgb) => parsed.push(rgb),
                None => findings.push(format!(
                    "{what}.srgb[{i}][{j}]: {value} is not a colour #rrggbb in lower case other than #000000"
                )),
            }
        }
        colours.push(parsed);
    }
    if findings.len() > before {
        return;
    }
    for i in 0..rows {
        for j in 0..columns {
            if i > 0 && luminance(colours[i][j]) > luminance(colours[i - 1][j]) {
                findings.push(format!(
                    "{what}: luminance rises with eumelanin at srgb[{i}][{j}]"
                ));
            }
            if j > 0 && red_share(colours[i][j]) < red_share(colours[i][j - 1]) {
                findings.push(format!(
                    "{what}: the red share falls with pheomelanin at srgb[{i}][{j}]"
                ));
            }
        }
    }
}

/// The ramp of the iris over one melanin axis.
fn ramp_findings(table: &Value, findings: &mut Vec<String>) {
    let what = "melanin_to_srgb.iris";
    let Some(table) = table.as_object() else {
        findings.push(format!("{what} is not an object"));
        return;
    };
    let before = findings.len();
    findings.extend(field_findings(table, &["melanin", "srgb"], what));
    if findings.len() > before {
        return;
    }
    let points = axis(table, "melanin", what, findings);
    let srgb = table["srgb"].as_array().map_or(&[][..], Vec::as_slice);
    if srgb.len() != points {
        findings.push(format!("{what}.srgb must have {points} colours"));
        return;
    }
    let mut colours = Vec::new();
    for (i, value) in srgb.iter().enumerate() {
        match colour(value) {
            Some(rgb) => colours.push(rgb),
            None => findings.push(format!(
                "{what}.srgb[{i}]: {value} is not a colour #rrggbb in lower case other than #000000"
            )),
        }
    }
    if findings.len() > before {
        return;
    }
    for i in 1..points {
        if luminance(colours[i]) > luminance(colours[i - 1]) {
            findings.push(format!("{what}: luminance rises with melanin at srgb[{i}]"));
        }
    }
}

/// The colour tables (ADR-0005 section 5): support points, sizes, colours and the rule on
/// linear values: luminance does not rise with eumelanin, the red share does not fall
/// with pheomelanin.
fn melanin_findings(tables: &Value, findings: &mut Vec<String>) {
    let Some(object) = tables.as_object() else {
        findings.push("melanin_to_srgb is not an object".to_string());
        return;
    };
    let before = findings.len();
    findings.extend(field_findings(
        object,
        &["skin", "hair", "iris"],
        "melanin_to_srgb",
    ));
    if findings.len() > before {
        return;
    }
    grid_findings("skin", &object["skin"], findings);
    grid_findings("hair", &object["hair"], findings);
    ramp_findings(&object["iris"], findings);
}

/// Keys that occur twice in one object of a JSON text. The parser keeps the last value
/// without a word, while a reader of the file may see the first.
fn duplicate_keys(json: &str) -> Vec<String> {
    let mut found = Vec::new();
    // The keys seen so far per open object; `None` stands for an open array.
    let mut open: Vec<Option<Vec<String>>> = Vec::new();
    let mut rest = json;
    while let Some(first) = rest.chars().next() {
        let mut length = first.len_utf8();
        match first {
            '{' => open.push(Some(Vec::new())),
            '[' => open.push(None),
            '}' | ']' => {
                open.pop();
            }
            '"' => {
                // The string ends at the first quote that no backslash escapes.
                let mut escaped = false;
                let end = rest[1..].char_indices().find(|(_, c)| {
                    let closes = *c == '"' && !escaped;
                    escaped = *c == '\\' && !escaped;
                    closes
                });
                length = end.map_or(rest.len(), |(index, _)| index + 2);
                let is_key = rest[length..].trim_start().starts_with(':');
                if is_key
                    && let Some(Some(keys)) = open.last_mut()
                    && let Ok(key) = serde_json::from_str::<String>(&rest[..length])
                {
                    if keys.contains(&key) {
                        found.push(key);
                    } else {
                        keys.push(key);
                    }
                }
            }
            _ => {}
        }
        rest = &rest[length..];
    }
    found
}

/// ADR-0005 section 7, check 1: the findings of the schema of `contracts/body.json`.
pub fn schema_findings(body_json: &str) -> Vec<String> {
    let Ok(Value::Object(body)) = serde_json::from_str::<Value>(body_json) else {
        return vec!["body.json is not a JSON object".to_string()];
    };
    let twice = duplicate_keys(body_json);
    if !twice.is_empty() {
        return twice
            .iter()
            .map(|key| format!("body.json: the key {key} occurs twice in one object"))
            .collect();
    }
    let mut findings = field_findings(&body, &TOP_FIELDS, "body.json");
    if !findings.is_empty() {
        return findings;
    }
    if body["format_version"].as_u64() != Some(1) {
        findings.push("body.json: format_version must be 1".to_string());
    }
    if !body["contract_version"].as_str().is_some_and(is_version) {
        findings.push(format!(
            "body.json: contract_version {} is not MAJOR.MINOR.PATCH",
            body["contract_version"]
        ));
    }
    let mut active: Vec<String> = Vec::new();
    match body["parameters"].as_array() {
        Some(parameters) if !parameters.is_empty() => {
            for (index, parameter) in parameters.iter().enumerate() {
                if let Some(key) = parameter_findings(index, parameter, &mut findings) {
                    if active.contains(&key) {
                        findings.push(format!("parameter key {key} occurs twice"));
                    }
                    active.push(key);
                }
            }
        }
        _ => findings.push("body.json: parameters must be a list with entries".to_string()),
    }
    match body["retired_keys"].as_array() {
        Some(retired) => {
            let mut seen: Vec<&str> = Vec::new();
            for value in retired {
                let Some(key) = value.as_str().filter(|key| is_key(key)) else {
                    findings.push(format!("retired key {value} must match [a-z][a-z0-9_]*"));
                    continue;
                };
                if seen.contains(&key) {
                    findings.push(format!("retired key {key} is listed twice"));
                }
                if active.iter().any(|other| other == key) {
                    findings.push(format!("retired key {key} is an active parameter"));
                }
                seen.push(key);
            }
        }
        None => findings.push("body.json: retired_keys must be a list".to_string()),
    }
    melanin_findings(&body["melanin_to_srgb"], &mut findings);
    findings
}

/// The `contract_version` of the contract, if it has one in the form of a version.
pub fn version(body_json: &str) -> Option<String> {
    serde_json::from_str::<Value>(body_json)
        .ok()
        .and_then(|body| body["contract_version"].as_str().map(str::to_string))
        .filter(|version| is_version(version))
}

/// ADR-0005 section 7, check 2 (the changelog): `contracts/body.changelog.md` has a
/// section `## <contract_version>` with the entries Neu, Geändert and Stillgelegt.
pub fn changelog_findings(body_json: &str, changelog: &str) -> Vec<String> {
    let Some(version) = version(body_json) else {
        return vec!["body.json has no contract_version to look up in the changelog".to_string()];
    };
    let heading = format!("## {version}");
    let section: Vec<&str> = changelog
        .lines()
        .skip_while(|line| *line != heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect();
    if !changelog.lines().any(|line| line == heading) {
        return vec![format!("body.changelog.md has no section {heading}")];
    }
    ["Neu", "Geändert", "Stillgelegt"]
        .iter()
        .filter(|entry| {
            !section
                .iter()
                .any(|line| line.starts_with(&format!("- {entry}: ")))
        })
        .map(|entry| {
            format!("body.changelog.md: section {heading} has no entry \"- {entry}: ...\"")
        })
        .collect()
}

/// `cargo xtask melanin-chart` (ADR-0005 section 5): the colours of the melanin tables as
/// an HTML page for a look by eye, at the support points (readers interpolate between
/// them). `Err` with the findings if the contract does not pass its check.
pub fn melanin_chart(body_json: &str) -> Result<String, Vec<String>> {
    let findings = schema_findings(body_json);
    if !findings.is_empty() {
        return Err(findings);
    }
    let body: Value = serde_json::from_str(body_json).map_err(|error| vec![error.to_string()])?;
    let tables = &body["melanin_to_srgb"];
    // The label is white on dark colours, so that it stays readable.
    let cell = |value: &Value, title: String| {
        let dark = colour(value).is_some_and(|rgb| luminance(rgb) < 0.18);
        let text = if dark { ";color:#fff" } else { "" };
        let name = value.as_str().unwrap_or_default();
        format!("<td style=\"background:{name}{text}\" title=\"{title}\">{name}</td>")
    };
    let list = |values: &Value| values.as_array().cloned().unwrap_or_default();
    let mut html = String::from(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Melanin to sRGB</title>\n<style>td { width: 6em; height: 3em; font: 12px monospace; text-align: center; }</style>\n",
    );
    html.push_str(&format!(
        "<h1>Melanin to sRGB, contract {}</h1>\n<p>Rows: eumelanin. Columns: pheomelanin. Support points only.</p>\n",
        body["contract_version"].as_str().unwrap_or_default()
    ));
    for name in ["skin", "hair"] {
        let table = &tables[name];
        html.push_str(&format!("<h2>{name}</h2>\n<table>\n"));
        for (eumelanin, row) in list(&table["eumelanin"]).iter().zip(list(&table["srgb"])) {
            html.push_str("<tr>");
            for (pheomelanin, colour) in list(&table["pheomelanin"]).iter().zip(list(&row)) {
                let title = format!("eumelanin {eumelanin}, pheomelanin {pheomelanin}");
                html.push_str(&cell(&colour, title));
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</table>\n");
    }
    let iris = &tables["iris"];
    html.push_str("<h2>iris</h2>\n<table>\n<tr>");
    for (melanin, colour) in list(&iris["melanin"]).iter().zip(list(&iris["srgb"])) {
        html.push_str(&cell(&colour, format!("melanin {melanin}")));
    }
    html.push_str("</tr>\n</table>\n");
    Ok(html)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    const REPO_CONTRACT: &str = include_str!("../../contracts/body.json");
    const REPO_CHANGELOG: &str = include_str!("../../contracts/body.changelog.md");

    /// A small valid contract: one parameter per unit.
    pub(crate) fn fixture() -> Value {
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

    /// A key twice in one object: the parser keeps the last value without a word.
    #[test]
    fn duplicate_keys_are_found() {
        let twice = |key: &str| vec![key.to_string()];
        for (text, found) in [
            (r#"{"a": 1, "a": 2}"#, twice("a")),
            (r#"{"a":1,"b":2,"a":3}"#, twice("a")),
            (r#"{"p": [{"key": "x", "key": "y"}]}"#, twice("key")),
            (r#"{"p": {"q": {"r": 1, "s": 2, "r": 3}}}"#, twice("r")),
            // The same key written with an escape.
            (r#"{"a": 1, "\u0061": 2}"#, twice("a")),
            (r#"{"a\"b": 1, "a\"b": 2}"#, twice("a\"b")),
            (
                r#"{"a": 1, "b": 2, "a": 3, "b": 4}"#,
                vec!["a".to_string(), "b".to_string()],
            ),
            // With space between the key and its colon.
            ("{\"a\" : 1, \"a\"\n\t: 2}", twice("a")),
            // After a value that ends in a backslash, and after an array.
            (r#"{"a": "\\", "a": 1}"#, twice("a")),
            (r#"{"a": [1, [2]], "a": 2}"#, twice("a")),
            // No duplicates: other objects, values, texts that look like keys.
            (r#"{"a": 1, "b": 2}"#, Vec::new()),
            (r#"{"a": {"a": 1}, "b": {"a": 2}}"#, Vec::new()),
            (r#"[{"a": 1}, {"a": 2}]"#, Vec::new()),
            (r#"{"a": "a", "b": ["a", "a"]}"#, Vec::new()),
            (r#"{"x": "a\": 1, \"x", "y": "\\", "z": 2}"#, Vec::new()),
            (r#"{"a": {"b": 1}, "b": 2}"#, Vec::new()),
            (r#"{"a": [{"b": 1}], "b": 2}"#, Vec::new()),
        ] {
            assert_eq!(duplicate_keys(text), found, "{text}");
        }
    }

    /// A contract with a field twice is refused, whatever the values are.
    #[test]
    fn rejects_duplicate_key() {
        let text = fixture().to_string();
        assert_eq!(schema_findings(&text), NONE);
        let range = r#""range":[40.0,230.0]"#;
        assert_eq!(text.matches(range).count(), 1);
        let twice = text.replace(range, &format!(r#""range":[0.0,300.0],{range}"#));
        assert_eq!(
            schema_findings(&twice),
            one("body.json: the key range occurs twice in one object")
        );
        let version = r#""contract_version":"0.1.0""#;
        assert_eq!(text.matches(version).count(), 1);
        let twice = text.replace(version, &format!("{version},{version}"));
        assert_eq!(
            schema_findings(&twice),
            one("body.json: the key contract_version occurs twice in one object")
        );
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
        for key in [
            "Muscle",
            "muscleZ",
            "9lives",
            "muscle-z",
            "",
            "muskel_größe",
        ] {
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
        // From 0 to 1 with the right number of points, but not rising strictly.
        for axis in [json!([0, 0, 1]), json!([0, 1, 1]), json!([0, 2, 1])] {
            assert_eq!(
                after(|c| c["melanin_to_srgb"]["skin"]["eumelanin"] = axis.clone()),
                one("melanin_to_srgb.skin.eumelanin must rise strictly from 0 to 1"),
                "{axis}"
            );
        }
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["iris"]["melanin"] = json!([0, 0.5, 0.5])),
            vec!["melanin_to_srgb.iris.melanin must rise strictly from 0 to 1".to_string()]
        );
        assert_eq!(
            after(|c| c["melanin_to_srgb"]["skin"]["eumelanin"] = json!([0, 1])),
            one("melanin_to_srgb.skin.srgb must have 2 rows of 2 colours")
        );
        // An entry that is no row counts: three rows remain, but the row of the second
        // support point is no longer the second entry.
        for stray in [json!("#a08070"), json!(7), json!(null), json!({})] {
            for table in ["skin", "hair"] {
                let found = after(|c| {
                    let rows = c["melanin_to_srgb"][table]["srgb"]
                        .as_array_mut()
                        .expect("rows");
                    rows.insert(1, stray.clone());
                });
                assert_eq!(
                    found,
                    one(&format!(
                        "melanin_to_srgb.{table}.srgb must have 3 rows of 2 colours"
                    )),
                    "{stray}"
                );
            }
        }
        assert_eq!(
            after(|c| {
                c["melanin_to_srgb"]["iris"]["srgb"] =
                    json!(["#90b0d0", "#907040", "#301810", "#201008"])
            }),
            one("melanin_to_srgb.iris.srgb must have 3 colours")
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

    #[test]
    fn the_melanin_chart_shows_every_colour_once() {
        let chart = melanin_chart(&fixture().to_string()).expect("a valid contract");
        assert!(chart.starts_with("<!doctype html>"), "{chart}");
        assert_eq!(chart.matches("<td ").count(), 6 + 6 + 3);
        // Skin and hair share their colours in the fixture: each grid colour twice.
        for colour in [
            "#f0e0d0", "#f0d8c0", "#a08070", "#a07860", "#402010", "#401c0a",
        ] {
            let cell = format!("<td style=\"background:{colour}");
            assert_eq!(chart.matches(&cell).count(), 2, "{colour}");
        }
        for colour in ["#90b0d0", "#907040", "#301810"] {
            let cell = format!("<td style=\"background:{colour}");
            assert_eq!(chart.matches(&cell).count(), 1, "{colour}");
        }
        // The label is dark on light colours and white on dark ones.
        assert!(
            chart.contains("<td style=\"background:#f0e0d0\" "),
            "{chart}"
        );
        assert!(
            chart.contains("<td style=\"background:#402010;color:#fff\" "),
            "{chart}"
        );
        // Rows are eumelanin, columns pheomelanin, in the order of the file.
        let first = chart.find("background:#f0e0d0").expect("the first cell");
        let across = chart
            .find("background:#f0d8c0")
            .expect("the next pheomelanin");
        let down = chart
            .find("background:#a08070")
            .expect("the next eumelanin");
        assert!(first < across && across < down);
        assert!(chart.contains("<h2>skin</h2>") && chart.contains("<h2>hair</h2>"));
        assert!(chart.contains("<h2>iris</h2>"));
        assert!(chart.contains("contract 0.1.0"));
        assert!(
            chart.contains("title=\"eumelanin 0.5, pheomelanin 1\""),
            "{chart}"
        );
        assert!(chart.contains("title=\"melanin 0.5\""), "{chart}");
        // A contract that fails its check has no chart.
        let mut broken = fixture();
        broken["melanin_to_srgb"]["skin"]["srgb"][1][0] = json!("#ffffff");
        assert_eq!(
            melanin_chart(&broken.to_string()),
            Err(one(
                "melanin_to_srgb.skin: luminance rises with eumelanin at srgb[1][0]"
            ))
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
        // The words in running text are no entries.
        let prose = "## 0.1.0\n\nNeu ist alles. Geändert und Stillgelegt wurde nichts.\n- Neu\n";
        assert_eq!(changelog_findings(&body, prose).len(), 3);
        assert_eq!(
            changelog_findings("{}", complete),
            one("body.json has no contract_version to look up in the changelog")
        );
    }
}
