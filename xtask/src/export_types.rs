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

/// The shapes of the pedigree (`simagent export`, format 4) and the manifest (format 5).
pub fn descriptor() -> Vec<Named> {
    use Kind::{Bool, Integer, Number, Ref, Text};
    vec![
        Named {
            name: "Pedigree",
            doc: "`pedigree.json` of a run, as `simagent export` writes it.",
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
    ]
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
