//! The types of the exports for `web/` (ADR-0021 section 4): a descriptor of each JSON shape
//! the command line writes, `web/src/generated/export.ts` generated from it, and a check of
//! real output against it: the same keys both ways and the same kind of every value,
//! recursively, with a record of what the output showed, so that a list of scenarios can
//! prove that every optional field, every nullable value, every listed text and every
//! variant appears at least once.

use std::collections::BTreeSet;
use std::fmt::Write;

use serde_json::Value;

/// Where the generated types live, relative to the repository root.
pub const GENERATED_EXPORT_TS: &str = "web/src/generated/export.ts";

/// The kind of a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Integer,
    Number,
    Bool,
    /// A text from a closed list.
    Literal(&'static [&'static str]),
    List(Box<Kind>),
    /// A named type of the descriptor.
    Ref(&'static str),
    Nullable(Box<Kind>),
    /// The body values of the contract, `BodyParams` of `web/src/generated/body.ts`: checked
    /// as the named type `BodyParams` that [`body_type`] makes of the contract's keys.
    Body,
    /// Any JSON object, whatever its keys: the cause and the payload of a log entry, whose
    /// keys the log codes define (`tests/golden/log_codes.txt`).
    Map,
}

/// A field of an object: its key, its kind, and whether it may be left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: &'static str,
    pub kind: Kind,
    pub optional: bool,
}

/// A named shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub name: &'static str,
    pub doc: &'static str,
    pub fields: Vec<Field>,
}

fn field(key: &'static str, kind: Kind) -> Field {
    Field {
        key,
        kind,
        optional: false,
    }
}

fn optional(key: &'static str, kind: Kind) -> Field {
    Field {
        key,
        kind,
        optional: true,
    }
}

fn list(kind: Kind) -> Kind {
    Kind::List(Box::new(kind))
}

fn nullable(kind: Kind) -> Kind {
    Kind::Nullable(Box::new(kind))
}

/// The shapes of the pedigree (`simagent export`, format 4), the manifest (format 5) and the
/// answers of the server.
pub fn descriptor() -> Vec<Named> {
    use Kind::{Bool, Integer, Number, Ref, Text};
    vec![
        Named {
            name: "Pedigree",
            doc: "`pedigree.json` of a run, as `simagent export` writes it (format 5, with names).",
            fields: vec![
                field("format_version", Integer),
                field("persons", list(Ref("Person"))),
                field("partnerships", list(Ref("Partnership"))),
                field("households", list(Ref("Household"))),
            ],
        },
        Named {
            name: "Person",
            doc: "A person of the pedigree; times in seconds of the simulation.",
            fields: vec![
                field("sim", Integer),
                field("lineage_key", Text),
                field("sex", Kind::Literal(&["female", "male"])),
                field("alleles", list(Ref("LocusAlleles"))),
                field("born", Integer),
                field("died", nullable(Integer)),
                field("cause_of_death", nullable(Text)),
                field("emigrated", nullable(Integer)),
                field("mother", nullable(Integer)),
                field("father", nullable(Integer)),
                field("social_parents", list(Integer)),
                field("household", Integer),
                field("moved_out", Bool),
                field("carer", nullable(Integer)),
                field("diseases", list(Ref("Disease"))),
                field("pool", nullable(Ref("FromPool"))),
                field("first_name", Text),
                field("surname", Text),
            ],
        },
        Named {
            name: "LocusAlleles",
            doc: "The allele of each copy at a locus with an alternative allele.",
            fields: vec![field("locus", Text), field("copies", list(Integer))],
        },
        Named {
            name: "Disease",
            doc: "A hereditary disease of a person.",
            fields: vec![
                field("disease", Text),
                field("onset", Number),
                field("affected", Bool),
            ],
        },
        Named {
            name: "FromPool",
            doc: "Where a pool person came from; its relations as lineage keys.",
            fields: vec![
                field("genome_key", Text),
                field("born_pool", nullable(Integer)),
                field("arrived", Integer),
                field("arrival_year", Integer),
                field("mother_key", nullable(Text)),
                field("father_key", nullable(Text)),
                field("social_parent_keys", list(Text)),
            ],
        },
        Named {
            name: "Partnership",
            doc: "A partnership of the pedigree, with the kinship of the two.",
            fields: vec![
                field("partnership", Integer),
                field("woman", Integer),
                field("man", Integer),
                field("formed", Integer),
                field("ended", nullable(Integer)),
                field(
                    "ended_by",
                    nullable(Kind::Literal(&["death", "separation", "emigration"])),
                ),
                field("kinship", Number),
            ],
        },
        Named {
            name: "Household",
            doc: "A household of the pedigree.",
            fields: vec![
                field("household", Integer),
                field("formed", Integer),
                field("ended", nullable(Integer)),
            ],
        },
        Named {
            name: "Manifest",
            doc: "`manifest.json` of a run; hashes as 16 hex digits.",
            fields: vec![
                field("format_version", Integer),
                field("core_version", Integer),
                field("data_hash", Text),
                field("build_commit", Text),
                field("world_seed", Text),
                field("closed_through", Integer),
                field("year_hashes", list(Ref("YearHash"))),
                field("log_hash", Text),
                optional("fork", Ref("ForkInfo")),
            ],
        },
        Named {
            name: "YearHash",
            doc: "The hash of the state at the end of a year.",
            fields: vec![field("year", Integer), field("hash", Text)],
        },
        Named {
            name: "ForkInfo",
            doc: "The fork block of a manifest: its parent by the hash of its log.",
            fields: vec![
                field("parent_log_hash", Text),
                field("from_year", Integer),
                field("dropped_inputs", Integer),
            ],
        },
        Named {
            name: "PersonAnswer",
            doc: "`GET /api/runs/{run}/persons/{sim}`: the core's presentation state of a person.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.person"])),
                field("version", Integer),
                field("contract_version", Text),
                field("sim", Integer),
                field("born", Ref("Moment")),
                field("ended", nullable(Kind::Literal(&["death", "emigration"]))),
                field("shown", Ref("Shown")),
                field("at_ages", list(Ref("Shown"))),
                field("partner_candidates", list(Integer)),
                field("card", Ref("GeneticsCard")),
                optional("kinship", Ref("Kinship")),
            ],
        },
        Named {
            name: "GeneticsCard",
            doc: "The core's genetics card of the person at its second shown: per trait the percentile and the parts, the height lines, per disease the copies and the status, the lines of skipped pigment copies. The variant parts (parts, copies, status, a skipped line's copy sources) are objects keyed by `kind`, a skipped line an object of `locus`, `mother_copy` and `father_copy`; the web checks their keys.",
            fields: vec![
                field("traits", list(Ref("TraitCard"))),
                field("height", nullable(Ref("HeightCard"))),
                field("diseases_off", Bool),
                field("diseases", list(Ref("DiseaseCard"))),
                field("skipped", list(Kind::Map)),
            ],
        },
        Named {
            name: "TraitCard",
            doc: "A trait of the card: `z`, its percentile, whether the person is an adult, the parts `z` adds up from (`kind` split or inherited) and the percentile of the parents' mean.",
            fields: vec![
                field("trait", Text),
                field("z", Number),
                field("percentile", Ref("Percentile")),
                field("adult", Bool),
                field("parts", Kind::Map),
                field("parents_mean", nullable(Ref("Percentile"))),
            ],
        },
        Named {
            name: "Percentile",
            doc: "\"größer als n von 100\" (`up`) or \"kleiner als n von 100\", `n` from 0 to 99.",
            fields: vec![field("up", Bool), field("n", Integer)],
        },
        Named {
            name: "HeightCard",
            doc: "The height lines in cm: the loss from hunger years and from age, the mean of both parents' target heights.",
            fields: vec![
                field("hunger_cm", Number),
                field("age_cm", Number),
                field("parents_target_cm", nullable(Number)),
            ],
        },
        Named {
            name: "DiseaseCard",
            doc: "A disease of the card: its code, the copies and the status (each an object keyed by `kind`).",
            fields: vec![
                field("disease", Text),
                field("copies", Kind::Map),
                field("status", Kind::Map),
            ],
        },
        Named {
            name: "Kinship",
            doc: "With `?to={other}`: what the other sim is to the person, by the core; `word` and `foster` are codes of `tests/golden/log_codes.txt`.",
            fields: vec![
                field("to", Integer),
                field("word", Text),
                field("foster", nullable(Text)),
                field("r_percent", Number),
            ],
        },
        Named {
            name: "RunsAnswer",
            doc: "`GET /api/runs`: the runs of the server's directory in name order.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.runs"])),
                field("version", Integer),
                field("runs", list(Ref("RunEntry"))),
            ],
        },
        Named {
            name: "RunEntry",
            doc: "A run of the list: its manifest and the date of its last closed second, or why not.",
            fields: vec![
                field("run", Text),
                optional("manifest", Ref("Manifest")),
                optional("date", Ref("CalendarDate")),
                optional("error", Kind::Literal(&["manifest_invalid", "run_failed"])),
            ],
        },
        Named {
            name: "CalendarDate",
            doc: "The core's calendar date of a second: every unit counted from 0, named by `web/`.",
            fields: vec![
                field("year", Integer),
                field("day_of_year", Integer),
                field("month", Integer),
                field("day_of_month", Integer),
                field("week", Integer),
                field("weekday", Integer),
                field("season", Integer),
                field("second_of_day", Integer),
            ],
        },
        Named {
            name: "AdvanceAnswer",
            doc: "`POST /api/runs/{run}/advance`: the last closed second after the step, and its date.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.advanced"])),
                field("version", Integer),
                field("closed_through", Integer),
                field("date", Ref("CalendarDate")),
            ],
        },
        Named {
            name: "CreatedAnswer",
            doc: "`POST /api/runs`: the name of the run made from a template.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.created"])),
                field("version", Integer),
                field("run", Text),
            ],
        },
        Named {
            name: "InputAnswer",
            doc: "`POST /api/runs/{run}/input`: the stamp of the accepted input.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.input"])),
                field("version", Integer),
                field("t", Integer),
                field("seq", Integer),
            ],
        },
        Named {
            name: "ErrorAnswer",
            doc: "A refusal of the server: its code, and the core's code of a rejected input.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.error"])),
                field("version", Integer),
                field("code", Text),
                optional("reason", Text),
            ],
        },
        Named {
            name: "EventsAnswer",
            doc: "`GET /api/runs/{run}/events`: a page of the log, each entry with its date.",
            fields: vec![
                field("format", Kind::Literal(&["simagent.view.events"])),
                field("version", Integer),
                field("events", list(Ref("EventItem"))),
                field("next", nullable(Integer)),
            ],
        },
        Named {
            name: "EventItem",
            doc: "An entry of `events.ndjson` and the core's calendar date of its second.",
            fields: vec![
                field("entry", Ref("LogEntry")),
                field("date", Ref("CalendarDate")),
            ],
        },
        Named {
            name: "LogEntry",
            doc: "An entry of the domain log; its causes and payload keyed by their codes.",
            fields: vec![
                field("id", Integer),
                field("t", Integer),
                field("subjects", list(Ref("Subject"))),
                field("causes", list(Kind::Map)),
                field("payload", Kind::Map),
            ],
        },
        Named {
            name: "Subject",
            doc: "What a log entry is about.",
            fields: vec![field("sim", Integer)],
        },
        Named {
            name: "Moment",
            doc: "A second of a life: the second, the age then in whole years, its calendar year.",
            fields: vec![
                field("t", Integer),
                field("age_years", Integer),
                field("calendar_year", Integer),
            ],
        },
        Named {
            name: "Shown",
            doc: "A body at a second of a life.",
            fields: vec![field("at", Ref("Moment")), field("body", Kind::Body)],
        },
    ]
}

/// The name of the body's type in the check.
const BODY: &str = "BodyParams";

/// The type of the body values for the check: one number per key of the contract, in the
/// contract's order (the keys of `contracts/body.json`; the TypeScript type is `body.ts`'s).
pub fn body_type(keys: &[String]) -> Named {
    let fields = keys
        .iter()
        .map(|key| field(Box::leak(key.clone().into_boxed_str()), Kind::Number))
        .collect();
    Named {
        name: BODY,
        doc: "The body values of the contract.",
        fields,
    }
}

/// A kind as TypeScript.
fn typescript(kind: &Kind) -> String {
    match kind {
        Kind::Text => "string".to_owned(),
        Kind::Integer | Kind::Number => "number".to_owned(),
        Kind::Bool => "boolean".to_owned(),
        Kind::Literal(values) => {
            let quoted: Vec<String> = values.iter().map(|v| Value::from(*v).to_string()).collect();
            quoted.join(" | ")
        }
        Kind::List(inner) => match **inner {
            Kind::Literal(_) | Kind::Nullable(_) => format!("({})[]", typescript(inner)),
            _ => format!("{}[]", typescript(inner)),
        },
        Kind::Ref(name) => (*name).to_owned(),
        Kind::Nullable(inner) => format!("{} | null", typescript(inner)),
        Kind::Body => "BodyParams".to_owned(),
        Kind::Map => "{ readonly [key: string]: unknown }".to_owned(),
    }
}

/// The TypeScript module of the descriptor.
pub fn generate(types: &[Named]) -> String {
    let mut out = String::new();
    // Writing into a String cannot fail.
    let mut line = |text: &str| {
        let _ = writeln!(out, "{text}");
    };
    line("// Generated by `cargo xtask codegen` from xtask/src/export_types.rs. Do not edit.");
    line("");
    line("import type { BodyParams } from \"./body.js\";");
    for named in types {
        line("");
        line(&format!("/** {} */", named.doc));
        line(&format!("export type {} = {{", named.name));
        for field in &named.fields {
            let mark = if field.optional { "?" } else { "" };
            line(&format!(
                "  readonly {}{mark}: {};",
                field.key,
                typescript(&field.kind)
            ));
        }
        line("};");
    }
    out
}

/// What the output showed: optional fields present, nullable values not null, lists not
/// empty, texts of a closed list, as `Type.key`, `Type.key[]` and `Type.key=text`.
pub type Seen = BTreeSet<String>;

/// The marks a complete coverage needs. A list of a named type must hold an element once,
/// or the fields of that type would never be checked; a list of plain values need not (its
/// values are checked whenever it holds one, and the social parents, for one, never occur
/// in the coverage scenario).
pub fn wanted(types: &[Named]) -> Seen {
    let mut marks = Seen::new();
    for named in types {
        for field in &named.fields {
            let mark = format!("{}.{}", named.name, field.key);
            if field.optional || matches!(field.kind, Kind::Nullable(_)) {
                marks.insert(mark.clone());
            }
            if let Kind::List(inner) = &field.kind
                && let Kind::Ref(_) = **inner
            {
                marks.insert(format!("{mark}[]"));
            }
            let literal = match &field.kind {
                Kind::Literal(values) => Some(values),
                Kind::Nullable(inner) | Kind::List(inner) => match &**inner {
                    Kind::Literal(values) => Some(values),
                    _ => None,
                },
                _ => None,
            };
            for value in literal.into_iter().flat_map(|values| values.iter()) {
                marks.insert(format!("{mark}={value}"));
            }
        }
    }
    marks
}

/// The differences of `value` from the named type `name`, each with its JSON path; what it
/// showed goes into `seen`.
pub fn conforms(types: &[Named], name: &str, value: &Value, seen: &mut Seen) -> Vec<String> {
    let mut found = Vec::new();
    check_named(types, name, value, "$", seen, &mut found);
    found
}

fn check_named(
    types: &[Named],
    name: &str,
    value: &Value,
    path: &str,
    seen: &mut Seen,
    found: &mut Vec<String>,
) {
    let Some(named) = types.iter().find(|n| n.name == name) else {
        found.push(format!("{path}: no type {name} in the descriptor"));
        return;
    };
    let Some(object) = value.as_object() else {
        found.push(format!("{path}: {name} is an object, not {value}"));
        return;
    };
    for key in object.keys() {
        if !named.fields.iter().any(|f| f.key == key) {
            found.push(format!("{path}.{key}: not a field of {name}"));
        }
    }
    for field in &named.fields {
        let mark = format!("{name}.{}", field.key);
        match object.get(field.key) {
            None if field.optional => {}
            None => found.push(format!("{path}.{}: missing from {name}", field.key)),
            Some(inner) => {
                if field.optional {
                    seen.insert(mark.clone());
                }
                let at = format!("{path}.{}", field.key);
                check_kind(types, &field.kind, inner, &at, &mark, seen, found);
            }
        }
    }
}

fn check_kind(
    types: &[Named],
    kind: &Kind,
    value: &Value,
    path: &str,
    mark: &str,
    seen: &mut Seen,
    found: &mut Vec<String>,
) {
    let fits = match kind {
        Kind::Text => value.is_string(),
        Kind::Integer => value.is_i64() || value.is_u64(),
        Kind::Number => value.is_number(),
        Kind::Bool => value.is_boolean(),
        Kind::Literal(values) => {
            let text = value.as_str();
            if let Some(text) = text.filter(|t| values.contains(t)) {
                seen.insert(format!("{mark}={text}"));
            }
            text.is_some_and(|t| values.contains(&t))
        }
        Kind::List(inner) => {
            if value.as_array().is_some_and(|items| !items.is_empty()) {
                seen.insert(format!("{mark}[]"));
            }
            for (i, item) in value.as_array().into_iter().flatten().enumerate() {
                check_kind(
                    types,
                    inner,
                    item,
                    &format!("{path}[{i}]"),
                    mark,
                    seen,
                    found,
                );
            }
            value.is_array()
        }
        Kind::Ref(name) => {
            check_named(types, name, value, path, seen, found);
            true
        }
        Kind::Body => {
            check_named(types, BODY, value, path, seen, found);
            true
        }
        Kind::Map => value.is_object(),
        Kind::Nullable(inner) => {
            if !value.is_null() {
                seen.insert(mark.to_owned());
                check_kind(types, inner, value, path, mark, seen, found);
            }
            true
        }
    };
    if !fits {
        found.push(format!("{path}: {value} is not {kind:?}"));
    }
}

/// The marks of `wanted` the output did not show.
pub fn missing(wanted: &Seen, seen: &Seen) -> Vec<String> {
    wanted.difference(seen).cloned().collect()
}

/// The checked-in file equals a fresh generation.
pub fn stale_finding(fresh: &str, checked_in: &str) -> Option<String> {
    (fresh != checked_in).then(|| {
        format!("{GENERATED_EXPORT_TS} is stale: run cargo xtask codegen and commit the result")
    })
}

#[cfg(test)]
#[path = "export_types_tests.rs"]
mod tests;
