//! The unscripted chain (ADR-0031 acceptance test 6, criterion 4).
//!
//! Reads an `events.ndjson` text and reports, per run, the full chains with their entry
//! ids, the number of two-hop passings, and, when no chain is complete, the missing links
//! of the furthest partial story. The definitions are those of `tools/story_reference.py`,
//! which stays the reference. The checker reads JSON values only, not the core's log types.
//!
//! The gate does not call this yet.

#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

const LINKS: [&str; 6] = [
    "told",
    "believed",
    "told_on",
    "avoided",
    "refuted",
    "trust_loss",
];
/// Trust is value index 1 of a bond (ADR-0030 section 2).
const TRUST: usize = 1;
/// Avoidance needs the bond to leave trust strictly under this (ADR-0030 section 4).
const AVOID_BELOW: i64 = 400;
const HEARD: &str = "gehört";
const TOLD: &str = "core:told";
const BELIEVED: &str = "core:believed";
const UNBELIEVED: &str = "core:unbelieved";
const BOND: &str = "core:bond_changed";
const COMPANY: &str = "core:company";
const REFUTED: &str = "core:refuted";

/// One story. Absent links are `None`. Entry ids are the log's `id` fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Story {
    pub told: Option<i64>,
    pub believed: Option<i64>,
    pub told_on: Option<i64>,
    pub lowered: Option<i64>,
    pub avoided: Option<i64>,
    pub refuted: Option<i64>,
    pub trust_loss: Option<i64>,
    pub lie: bool,
    pub speaker: Option<i64>,
    pub hearer: Option<i64>,
    pub third: Option<i64>,
    pub avoided_person: Option<i64>,
    pub source: Option<i64>,
    pub origin: Option<i64>,
    pub note: String,
}

impl Story {
    fn missing(&self) -> Vec<&'static str> {
        let present = [
            self.told,
            self.believed,
            self.told_on,
            self.avoided,
            self.refuted,
            self.trust_loss,
        ];
        LINKS
            .into_iter()
            .zip(present)
            .filter_map(|(name, value)| value.is_none().then_some(name))
            .collect()
    }

    fn complete(&self) -> bool {
        self.missing().is_empty()
    }

    fn hop(&self) -> bool {
        self.told.is_some() && self.believed.is_some() && self.told_on.is_some()
    }

    fn sort_key(&self) -> [i64; 7] {
        [
            self.told.unwrap_or(0),
            self.believed.unwrap_or(0),
            self.told_on.unwrap_or(0),
            self.lowered.unwrap_or(0),
            self.avoided.unwrap_or(0),
            self.refuted.unwrap_or(0),
            self.trust_loss.unwrap_or(0),
        ]
    }
}

/// Partial stories that share one missing-link pattern. Only the furthest patterns,
/// those with the fewest missing links, are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group {
    pub missing: Vec<&'static str>,
    pub count: usize,
    pub example: Story,
}

/// The chains and passings of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Distinct onward tellings: a heard sentence told on to a third person.
    pub hops: usize,
    pub chains: Vec<Story>,
    pub groups: Vec<Group>,
    pub warnings: Vec<String>,
}

/// The report for the text of an `events.ndjson`. A cut-off last line is a warning.
/// A broken earlier line, a line that is not a log entry, or a duplicate id is an error.
pub(crate) fn check(events: &str) -> Result<Report, String> {
    let (entries, warnings) = parse_events(events, "events.ndjson")?;
    Ok(analyze(&entries, warnings))
}

pub(crate) fn parse_events(text: &str, name: &str) -> Result<(Vec<Value>, Vec<String>), String> {
    let mut warnings = Vec::new();
    let mut entries = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (offset, line) in lines.iter().enumerate() {
        let number = offset + 1;
        if line.trim().is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) if number == lines.len() => {
                warnings.push(format!("{name}:{number}: cut-off last line ignored"));
                break;
            }
            Err(err) => return Err(format!("{name}:{number}: {err}")),
        };
        if entry_id(&value).is_none() || value.get("payload").is_none() {
            return Err(format!("{name}:{number}: not a log entry"));
        }
        entries.push(value);
    }
    let mut seen = BTreeSet::new();
    for entry in &entries {
        let id = entry_id(entry).expect("parsed entry id");
        if !seen.insert(id) {
            return Err(format!("{name}: duplicate entry id {id}"));
        }
    }
    Ok((entries, warnings))
}

fn analyze(entries: &[Value], warnings: Vec<String>) -> Report {
    let pos: BTreeMap<i64, usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry_id(entry).expect("parsed entry id"), index))
        .collect();
    let world = World::new(entries, &pos);
    let mut stories = Vec::new();
    if world.tolds.is_empty() {
        stories.push(Story::default());
    }
    for index in 0..world.tolds.len() {
        let told = world.tolds[index];
        if world.continues(told) {
            continue;
        }
        let heard = world.heard_beliefs(told);
        if heard.is_empty() {
            stories.push(world.origin_only(told));
            continue;
        }
        for believed in heard {
            world.follow(told, believed, &mut stories);
        }
    }
    let hops = stories
        .iter()
        .filter(|story| story.hop())
        .filter_map(|story| story.told_on)
        .collect::<BTreeSet<_>>()
        .len();
    let mut chains: Vec<Story> = stories
        .iter()
        .filter(|story| story.complete())
        .cloned()
        .collect();
    chains.sort_by_key(Story::sort_key);
    let groups = if chains.is_empty() {
        groups_of(&stories)
    } else {
        Vec::new()
    };
    Report {
        hops,
        chains,
        groups,
        warnings,
    }
}

fn groups_of(stories: &[Story]) -> Vec<Group> {
    let fallback = Story::default();
    let mut partials: Vec<&Story> = stories.iter().filter(|story| !story.complete()).collect();
    if partials.is_empty() {
        partials.push(&fallback);
    }
    let best = partials
        .iter()
        .map(|story| story.missing().len())
        .min()
        .expect("a partial");
    let mut buckets: BTreeMap<Vec<&'static str>, Vec<&Story>> = BTreeMap::new();
    for story in partials {
        let missing = story.missing();
        if missing.len() == best {
            buckets.entry(missing).or_default().push(story);
        }
    }
    buckets
        .into_iter()
        .map(|(missing, mut examples)| {
            examples.sort_by_key(|story| story.sort_key());
            Group {
                missing,
                count: examples.len(),
                example: examples[0].clone(),
            }
        })
        .collect()
}

struct World<'a> {
    entries: &'a [Value],
    pos: &'a BTreeMap<i64, usize>,
    tolds: Vec<usize>,
    beliefs: Vec<usize>,
    bonds: Vec<usize>,
    companies: Vec<usize>,
    refuteds: Vec<usize>,
}

impl<'a> World<'a> {
    fn new(entries: &'a [Value], pos: &'a BTreeMap<i64, usize>) -> World<'a> {
        let mut tolds = Vec::new();
        let mut beliefs = Vec::new();
        let mut bonds = Vec::new();
        let mut companies = Vec::new();
        let mut refuteds = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            match kind_of(entry) {
                Some(TOLD) => tolds.push(index),
                Some(BELIEVED) => beliefs.push(index),
                Some(BOND) => bonds.push(index),
                Some(COMPANY) => companies.push(index),
                Some(REFUTED) => refuteds.push(index),
                _ => {}
            }
        }
        World {
            entries,
            pos,
            tolds,
            beliefs,
            bonds,
            companies,
            refuteds,
        }
    }

    fn follow(&self, told: usize, believed: usize, stories: &mut Vec<Story>) {
        let onward = self.passings(told, believed);
        if onward.is_empty() {
            stories.push(self.story_from(told, Some(believed), None, None));
            return;
        }
        for told_on in onward {
            stories.push(self.story_from(told, Some(believed), Some(told_on), None));
            for later in self.heard_beliefs(told_on) {
                stories.push(self.story_from(told, Some(believed), Some(told_on), Some(later)));
                self.follow(told_on, later, stories);
            }
        }
    }

    fn origin_only(&self, told: usize) -> Story {
        let entry = &self.entries[told];
        Story {
            told: Some(id_of(entry)),
            speaker: subject_of(entry),
            lie: body_of(entry).is_some_and(|body| truthy(body.get("lie"))),
            ..Story::default()
        }
    }

    fn story_from(
        &self,
        told: usize,
        believed: Option<usize>,
        told_on: Option<usize>,
        chain: Option<usize>,
    ) -> Story {
        let entry = &self.entries[told];
        let mut story = Story {
            told: Some(id_of(entry)),
            speaker: subject_of(entry),
            lie: body_of(entry).is_some_and(|body| truthy(body.get("lie"))),
            ..Story::default()
        };
        let Some(believed) = believed else {
            return story;
        };
        let told_has_statement = body_of(&self.entries[told])
            .and_then(statement_of)
            .is_some();
        if body_of(&self.entries[believed]).is_none() || !told_has_statement {
            return story;
        }
        let person = chain.unwrap_or(believed);
        let Some(person_body) = body_of(&self.entries[person]) else {
            return story;
        };
        let source = map_int(person_body, "source");
        let origin = map_int(person_body, "origin");
        let hearer = subject_of(&self.entries[person]);
        story.believed = Some(id_of(&self.entries[believed]));
        story.hearer = hearer;
        story.source = source;
        story.origin = origin;
        if let Some(onward) = told_on {
            story.told_on = Some(id_of(&self.entries[onward]));
            story.third = body_of(&self.entries[onward]).and_then(|body| map_int(body, "to"));
        }
        self.finish(&mut story, person, told);
        story
    }

    /// Avoidance counts only after the onward telling, and a refutation only after that
    /// avoidance. A partial with no onward telling may still carry the later rows.
    fn finish(&self, story: &mut Story, person: usize, told: usize) {
        let doer = self.sentence_at(told).and_then(doer_of);
        let (lowered, note) = self.lowering(person, doer);
        story.note = note;
        if let Some(bond) = lowered {
            story.lowered = Some(id_of(&self.entries[bond]));
            story.avoided_person = bond_other(&self.entries[bond]);
            if let Some(company) = self.company_after(bond, story.told_on) {
                story.avoided = Some(id_of(&self.entries[company]));
            }
        }
        let previous = story.avoided.or(story.told_on);
        let candidates: Vec<usize> = self
            .refutations(person)
            .into_iter()
            .filter(|&refuted| {
                let id = id_of(&self.entries[refuted]);
                self.later_than(previous, id)
            })
            .collect();
        let mut chosen_refuted = None;
        let mut chosen_loss = None;
        for refuted in &candidates {
            if let Some(loss) = self.trust_loss(*refuted) {
                chosen_refuted = Some(*refuted);
                chosen_loss = Some(loss);
                break;
            }
        }
        if chosen_refuted.is_none() {
            chosen_refuted = candidates.first().copied();
        }
        story.refuted = chosen_refuted.map(|index| id_of(&self.entries[index]));
        story.trust_loss = chosen_loss.map(|index| id_of(&self.entries[index]));
    }

    fn heard_beliefs(&self, told: usize) -> Vec<usize> {
        let entry = &self.entries[told];
        let Some(speaker) = subject_of(entry) else {
            return Vec::new();
        };
        let Some(body) = body_of(entry) else {
            return Vec::new();
        };
        let Some(hearer) = map_int(body, "to") else {
            return Vec::new();
        };
        if hearer == speaker {
            return Vec::new();
        }
        let Some(wanted) = statement_of(body).and_then(sentence) else {
            return Vec::new();
        };
        let told_id = id_of(entry);
        let mut found = Vec::new();
        for &believed in &self.beliefs {
            let row = &self.entries[believed];
            if subject_of(row) != Some(hearer) || !earlier(self.pos, told_id, id_of(row)) {
                continue;
            }
            if !event_ids(row).contains(&told_id) {
                continue;
            }
            let Some(belief) = body_of(row) else {
                continue;
            };
            if belief.get("basis").and_then(Value::as_str) != Some(HEARD) {
                continue;
            }
            if map_int(belief, "source") != Some(speaker) {
                continue;
            }
            if statement_of(belief).and_then(sentence) != Some(wanted) {
                continue;
            }
            found.push(believed);
        }
        found
    }

    fn passings(&self, told: usize, believed: usize) -> Vec<usize> {
        let Some(speaker) = subject_of(&self.entries[told]) else {
            return Vec::new();
        };
        let Some(hearer) = subject_of(&self.entries[believed]) else {
            return Vec::new();
        };
        let Some(wanted) = self.sentence_at(told) else {
            return Vec::new();
        };
        let believed_id = id_of(&self.entries[believed]);
        let mut found = Vec::new();
        for &onward in &self.tolds {
            let row = &self.entries[onward];
            if !earlier(self.pos, believed_id, id_of(row)) || subject_of(row) != Some(hearer) {
                continue;
            }
            let Some(body) = body_of(row) else {
                continue;
            };
            let Some(third) = map_int(body, "to") else {
                continue;
            };
            if third == speaker || third == hearer {
                continue;
            }
            if statement_of(body).and_then(sentence) == Some(wanted) {
                found.push(onward);
            }
        }
        found
    }

    fn lowering(&self, believed: usize, doer: Option<i64>) -> (Option<usize>, String) {
        let Some(doer) = doer else {
            return (None, String::new());
        };
        let Some(hearer) = subject_of(&self.entries[believed]) else {
            return (None, String::new());
        };
        let believed_id = id_of(&self.entries[believed]);
        let mut short: Option<(usize, i64)> = None;
        for &bond in &self.bonds {
            let drops = {
                let row = &self.entries[bond];
                subject_of(row) == Some(hearer)
                    && bond_other(row) == Some(doer)
                    && earlier(self.pos, believed_id, id_of(row))
            };
            if !drops || !self.caused_by_believed(bond, believed) {
                continue;
            }
            let Some((before, after)) = trust_pair(&self.entries[bond]) else {
                continue;
            };
            if after >= before {
                continue;
            }
            if after < AVOID_BELOW {
                return (Some(bond), String::new());
            } else if short.is_none() {
                short = Some((bond, after));
            }
        }
        match short {
            Some((bond, after)) => (
                None,
                format!(
                    "bond {} leaves trust at {after}",
                    id_of(&self.entries[bond])
                ),
            ),
            None => (None, String::new()),
        }
    }

    /// The bond names the belief, or one `core:unbelieved` of the same hearer that names it.
    fn caused_by_believed(&self, bond: usize, believed: usize) -> bool {
        let bond_id = id_of(&self.entries[bond]);
        let believed_id = id_of(&self.entries[believed]);
        let hearer = subject_of(&self.entries[believed]);
        for cause_id in event_ids(&self.entries[bond]) {
            let Some(&mid_index) = self.pos.get(&cause_id) else {
                continue;
            };
            if !earlier(self.pos, cause_id, bond_id) {
                continue;
            }
            if cause_id == believed_id {
                return true;
            }
            let mid = &self.entries[mid_index];
            if kind_of(mid) != Some(UNBELIEVED) || subject_of(mid) != hearer {
                continue;
            }
            if event_ids(mid).contains(&believed_id) && earlier(self.pos, believed_id, cause_id) {
                return true;
            }
        }
        false
    }

    fn company_after(&self, bond: usize, after: Option<i64>) -> Option<usize> {
        let (person, who, bond_id) = {
            let row = &self.entries[bond];
            (bond_other(row)?, subject_of(row)?, id_of(row))
        };
        for &company in &self.companies {
            let entry = &self.entries[company];
            let company_id = id_of(entry);
            if !earlier(self.pos, bond_id, company_id) || !self.later_than(after, company_id) {
                continue;
            }
            if subject_of(entry) == Some(who) && avoids(entry, person) {
                return Some(company);
            }
        }
        None
    }

    fn refutations(&self, believed: usize) -> Vec<usize> {
        let entry = &self.entries[believed];
        let Some(hearer) = subject_of(entry) else {
            return Vec::new();
        };
        let Some(body) = body_of(entry) else {
            return Vec::new();
        };
        let Some(statement) = statement_of(body) else {
            return Vec::new();
        };
        let key = key_of(statement);
        let source = body.get("source").cloned();
        let believed_id = id_of(entry);
        let mut found = Vec::new();
        for &refuted in &self.refuteds {
            let row = &self.entries[refuted];
            if subject_of(row) != Some(hearer) || !earlier(self.pos, believed_id, id_of(row)) {
                continue;
            }
            let Some(body) = body_of(row) else {
                continue;
            };
            if body.get("source") != source.as_ref() {
                continue;
            }
            let Some(statement) = statement_of(body) else {
                continue;
            };
            if key_of(statement) == key {
                found.push(refuted);
            }
        }
        found
    }

    fn trust_loss(&self, refuted: usize) -> Option<usize> {
        let entry = &self.entries[refuted];
        let source = map_int(body_of(entry)?, "source")?;
        let who = subject_of(entry)?;
        let refuted_id = id_of(entry);
        for &bond in &self.bonds {
            let row = &self.entries[bond];
            if subject_of(row) != Some(who) || bond_other(row) != Some(source) {
                continue;
            }
            if !event_ids(row).contains(&refuted_id) || !earlier(self.pos, refuted_id, id_of(row)) {
                continue;
            }
            let Some((before, after)) = trust_pair(row) else {
                continue;
            };
            if after < before {
                return Some(bond);
            }
        }
        None
    }

    fn later_than(&self, bound: Option<i64>, id: i64) -> bool {
        bound.is_none_or(|bound| earlier(self.pos, bound, id))
    }

    fn continues(&self, told: usize) -> bool {
        let entry = &self.entries[told];
        let Some(speaker) = subject_of(entry) else {
            return false;
        };
        let Some(wanted) = self.sentence_at(told) else {
            return false;
        };
        let told_id = id_of(entry);
        for &believed in &self.beliefs {
            let row = &self.entries[believed];
            if subject_of(row) != Some(speaker) || !earlier(self.pos, id_of(row), told_id) {
                continue;
            }
            let Some(belief) = body_of(row) else {
                continue;
            };
            if belief.get("basis").and_then(Value::as_str) != Some(HEARD) {
                continue;
            }
            if statement_of(belief).and_then(sentence) != Some(wanted) {
                continue;
            }
            let believed_id = id_of(row);
            for cause_id in event_ids(row) {
                let Some(&cause_index) = self.pos.get(&cause_id) else {
                    continue;
                };
                if earlier(self.pos, cause_id, believed_id)
                    && kind_of(&self.entries[cause_index]) == Some(TOLD)
                {
                    return true;
                }
            }
        }
        false
    }

    fn sentence_at(&self, told: usize) -> Option<Sentence> {
        statement_of(body_of(&self.entries[told])?).and_then(sentence)
    }
}

fn earlier(pos: &BTreeMap<i64, usize>, left: i64, right: i64) -> bool {
    matches!((pos.get(&left), pos.get(&right)), (Some(left), Some(right)) if left < right)
}

fn kind_of(entry: &Value) -> Option<&str> {
    let payload = entry.get("payload")?.as_object()?;
    if payload.len() != 1 {
        return None;
    }
    payload.keys().next().map(String::as_str)
}

fn body_of(entry: &Value) -> Option<&Map<String, Value>> {
    let payload = entry.get("payload")?.as_object()?;
    if payload.len() != 1 {
        return None;
    }
    payload.values().next()?.as_object()
}

fn subject_of(entry: &Value) -> Option<i64> {
    map_int(
        entry.get("subjects")?.as_array()?.first()?.as_object()?,
        "sim",
    )
}

fn event_ids(entry: &Value) -> Vec<i64> {
    entry
        .get("causes")
        .and_then(Value::as_array)
        .map(|causes| {
            causes
                .iter()
                .filter_map(|cause| cause.as_object().and_then(|cause| map_int(cause, "event")))
                .collect()
        })
        .unwrap_or_default()
}

fn statement_of(body: &Map<String, Value>) -> Option<&Map<String, Value>> {
    body.get("statement")?.as_object()
}

fn map_int(map: &Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(Value::as_i64)
}

fn entry_id(entry: &Value) -> Option<i64> {
    entry.get("id").and_then(Value::as_i64)
}

fn id_of(entry: &Value) -> i64 {
    entry_id(entry).expect("parsed entry id")
}

fn bond_other(entry: &Value) -> Option<i64> {
    body_of(entry).and_then(|body| map_int(body, "other"))
}

fn trust_pair(entry: &Value) -> Option<(i64, i64)> {
    let body = body_of(entry)?;
    let at = |value: &Value| {
        value
            .as_array()
            .and_then(|row| row.get(TRUST))
            .and_then(Value::as_i64)
    };
    Some((at(body.get("before")?)?, at(body.get("after")?)?))
}

fn avoids(entry: &Value, person: i64) -> bool {
    let Some(objects) = body_of(entry)
        .and_then(|body| body.get("objects"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    objects.iter().any(|row| {
        let Some(row) = row.as_object() else {
            return false;
        };
        map_int(row, "avoided") == Some(person)
            && map_int(row, "avoid").is_some_and(|avoid| avoid > 0)
    })
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(bit)) => *bit,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        _ => false,
    }
}

/// The whole statement. A passing carries this. `VaterVon` keys only the child.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sentence {
    Tat(i64, i64, i64, i64),
    Father(i64, i64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Tat(i64, i64, i64, i64),
    Father(i64),
}

fn sentence(statement: &Map<String, Value>) -> Option<Sentence> {
    if let Some(tat) = statement.get("tat").and_then(Value::as_object) {
        return Some(Sentence::Tat(
            map_int(tat, "doer")?,
            map_int(tat, "kind")?,
            map_int(tat, "other")?,
            map_int(tat, "day")?,
        ));
    }
    let father = statement.get("vater_von")?.as_object()?;
    Some(Sentence::Father(
        map_int(father, "child")?,
        map_int(father, "man")?,
    ))
}

fn key_of(statement: &Map<String, Value>) -> Option<Key> {
    Some(match sentence(statement)? {
        Sentence::Tat(doer, kind, other, day) => Key::Tat(doer, kind, other, day),
        Sentence::Father(child, _) => Key::Father(child),
    })
}

fn doer_of(sentence: Sentence) -> Option<i64> {
    match sentence {
        Sentence::Tat(doer, _, _, _) => Some(doer),
        Sentence::Father(_, _) => None,
    }
}

#[cfg(test)]
#[path = "story_tests.rs"]
mod tests;
