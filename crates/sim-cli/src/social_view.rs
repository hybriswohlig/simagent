//! The social part of the house answer and of "Warum?" (ADR-0030 sections 2 to 4, ADR-0031
//! sections 5 and 8). Bonds, memory strengths and the avoided person's last trust step are
//! settled here from the core's integers. The page only prints them. `VaterVon` is judged the
//! same way as a deed, so a belief of that shape is told once paternity exists.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim_core::ids::{EventId, SimId};
use sim_core::log::{Cause, Company, LogEntry, LogPayload, Subject};
use sim_core::social::belief::{Basis, DeedAnswer, Deeds, Statement};
use sim_core::social::bond::{Bond, TRUST};
use sim_core::social::data::FeelingRules;
use sim_core::social::feeling::{channel_at, temper_at};
use sim_core::social::memory::Memory;
use sim_core::social::talk::house_day;
use sim_core::time::SimTime;
use sim_core::world::{World, WorldState};

use crate::error::CliError;

const TOP_MEMORIES: usize = 5;
const EMPTY_ROWS: &[Company] = &[];
const EMPTY_HISTORY: &[LogEntry] = &[];

/// The `core:company` rows of one choice and the bond and belief lines at or before it.
/// An empty history falls back to a bond whose `since` is at or before the choice. A
/// non-empty history does not: a person with no bond line in it has no bond at the choice.
pub struct CompanyAt<'a> {
    pub rows: &'a [Company],
    pub history: &'a [LogEntry],
}

impl CompanyAt<'static> {
    pub const NONE: CompanyAt<'static> = CompanyAt {
        rows: EMPTY_ROWS,
        history: EMPTY_HISTORY,
    };
}

/// Bonds settled at `at`, the five strongest memories then, and every belief, for one person.
pub(crate) fn snapshot(state: &WorldState, sim: SimId, at: SimTime) -> (Value, Value, Value) {
    let (familiarity, grudge) = fade_rates(state);
    let bonds = match state.house().social.bonds.get(&sim) {
        None => json!([]),
        Some(list) => {
            let rows: Vec<Value> = list
                .iter()
                .map(|bond| bond_json(bond, at, familiarity, grudge))
                .collect();
            json!(rows)
        }
    };
    let memories = memories_json(state, sim, at);
    let beliefs = beliefs_json(state, sim);
    (bonds, memories, beliefs)
}

fn bond_json(bond: &Bond, at: SimTime, familiarity: u16, grudge: u16) -> Value {
    let values = bond.values_at(at, familiarity, grudge);
    json!({
        "other": bond.other.get(),
        "affection": values[0],
        "trust": values[TRUST],
        "respect": values[2],
        "attraction": values[3],
        "familiarity": values[4],
        "grudge": values[5],
    })
}

fn memories_json(state: &WorldState, sim: SimId, at: SimTime) -> Value {
    let Some(list) = state.house().social.memories.get(&sim) else {
        return json!([]);
    };
    let mut ranked: Vec<&Memory> = list.iter().collect();
    ranked.sort_by(|left, right| {
        right
            .strength_at(at)
            .cmp(&left.strength_at(at))
            .then_with(|| left.key().cmp(&right.key()))
    });
    ranked.truncate(TOP_MEMORIES);
    let rows: Vec<Value> = ranked
        .iter()
        .map(|memory| {
            let with: Vec<u64> = memory.with.into_iter().flatten().map(SimId::get).collect();
            json!({
                "kind": memory.kind,
                "with": with,
                "at": crate::dated(memory.at),
                "strength": memory.strength_at(at),
            })
        })
        .collect();
    json!(rows)
}

fn beliefs_json(state: &WorldState, sim: SimId) -> Value {
    let Some(list) = state.house().social.beliefs.get(&sim) else {
        return json!([]);
    };
    let rows: Vec<Value> = list
        .iter()
        .map(|belief| {
            json!({
                "statement": statement_json(belief.statement),
                "certainty": belief.certainty,
                "since": crate::dated(belief.since),
                "source": sim_number(belief.source),
                "origin": sim_number(belief.origin),
                "basis": basis_word(belief.basis),
                "secret": belief.secret,
            })
        })
        .collect();
    json!(rows)
}

/// One row per furniture the choice weighed, with the bond values at the choice and, for an
/// avoided person, the belief that last lowered trust (ADR-0031 section 5: the latest second,
/// then the smaller statement key).
pub(crate) fn company_rows(
    world: &World,
    chooser: SimId,
    at: SimTime,
    company: CompanyAt<'_>,
) -> Result<Vec<Value>, CliError> {
    if company.rows.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for row in company.rows {
        rows.push(one_row(world, chooser, at, row, company.history)?);
    }
    Ok(rows)
}

fn one_row(
    world: &World,
    chooser: SimId,
    at: SimTime,
    row: &Company,
    history: &[LogEntry],
) -> Result<Value, CliError> {
    let liked = components(world, chooser, row.liked, at, history);
    let avoided = components(world, chooser, row.avoided, at, history);
    let belief = match row.avoided {
        Some(other) => avoid_json(world, chooser, other, at, history)?,
        None => missing_belief(),
    };
    // `company` in the log is already scaled by the temper. `plain` is the data constant the
    // choice adds when someone is liked, so the page can say both without reversing the scale.
    let plain = match row.liked {
        Some(_) => world
            .state()
            .data()
            .social()
            .map(|social| i64::from(social.choice.company))
            .unwrap_or(0),
        None => 0,
    };
    Ok(json!({
        "object": row.object,
        "liked": sim_number(row.liked),
        "affection": liked[0],
        "familiarity": liked[4],
        "company": row.company,
        "plain": plain,
        "avoided": sim_number(row.avoided),
        "trust": avoided[TRUST],
        "avoid": row.avoid,
        "belief": belief,
    }))
}

fn components(
    world: &World,
    chooser: SimId,
    other: Option<SimId>,
    at: SimTime,
    history: &[LogEntry],
) -> [u16; 6] {
    let Some(other) = other else {
        return [0; 6];
    };
    replay_bond(world.state(), chooser, other, at, history).unwrap_or([0; 6])
}

/// The six values of `chooser`'s bond to `other` at `at`. Familiarity and grudge fall from the
/// last `core:bond_formed` or `core:bond_changed` at or before `at`. A later `core:bond_dropped`
/// with no new formation is no bond.
fn replay_bond(
    state: &WorldState,
    chooser: SimId,
    other: SimId,
    at: SimTime,
    history: &[LogEntry],
) -> Option<[u16; 6]> {
    let (familiarity, grudge) = fade_rates(state);
    if history.is_empty() {
        let bond = (state.house().social.bonds.get(&chooser))?
            .iter()
            .find(|bond| bond.other == other)?;
        if bond.since > at {
            return None;
        }
        return Some(bond.values_at(at, familiarity, grudge));
    }
    let mut live: Option<([u16; 6], SimTime)> = None;
    let mut seen = false;
    for entry in history {
        if entry.t > at {
            continue;
        }
        let [Subject::Sim(owner)] = entry.subjects.as_slice() else {
            continue;
        };
        if *owner != chooser {
            continue;
        }
        match &entry.payload {
            LogPayload::BondFormed {
                other: whom,
                values,
            } if *whom == other => {
                seen = true;
                live = Some((*values, entry.t));
            }
            LogPayload::BondChanged {
                other: whom, after, ..
            } if *whom == other => {
                seen = true;
                live = Some((*after, entry.t));
            }
            LogPayload::BondDropped { other: whom } if *whom == other => {
                seen = true;
                live = None;
            }
            _ => {}
        }
    }
    if !seen {
        return None;
    }
    let (values, since) = live?;
    Some(fade_from(values, since, at, familiarity, grudge))
}

fn fade_from(
    values: [u16; 6],
    since: SimTime,
    at: SimTime,
    familiarity: u16,
    grudge: u16,
) -> [u16; 6] {
    let other = SimId::try_from(1).expect("a sim id");
    Bond {
        other,
        values,
        since,
    }
    .values_at(at, familiarity, grudge)
}

struct Heard {
    index: usize,
    t: SimTime,
    statement: Statement,
    certainty: u16,
    basis: Basis,
    source: Option<SimId>,
    origin: Option<SimId>,
}

fn avoid_json(
    world: &World,
    chooser: SimId,
    avoided: SimId,
    at: SimTime,
    history: &[LogEntry],
) -> Result<Value, CliError> {
    let Some(heard) = last_lowering(chooser, avoided, at, history) else {
        return Ok(missing_belief());
    };
    let secret = (world.state().house().social.beliefs.get(&chooser))
        .and_then(|list| {
            list.iter()
                .find(|belief| belief.statement == heard.statement)
        })
        .is_some_and(|belief| belief.secret);
    Ok(json!({
        "found": true,
        "statement": statement_json(heard.statement),
        "certainty": heard.certainty,
        "basis": basis_word(heard.basis),
        "source": sim_number(heard.source),
        "origin": sim_number(heard.origin),
        "secret": secret,
        "truth": truth_word(world, heard.statement),
    }))
}

/// The `core:believed` whose `core:bond_changed` last lowered trust of `chooser` toward
/// `avoided` at or before `at`. The cause has to be that belief, already written. A later
/// second wins; the same second keeps the smaller statement key, and the same key the earlier
/// line.
fn last_lowering(
    chooser: SimId,
    avoided: SimId,
    at: SimTime,
    history: &[LogEntry],
) -> Option<Heard> {
    let mut believed: BTreeMap<u64, Heard> = BTreeMap::new();
    let mut best: Option<Heard> = None;
    for (index, entry) in history.iter().enumerate() {
        if entry.t > at {
            continue;
        }
        if let LogPayload::Believed {
            statement,
            certainty,
            basis,
            source,
            origin,
            ..
        } = &entry.payload
            && let Some(number) = event_number(&entry.id)
        {
            believed.insert(
                number,
                Heard {
                    index,
                    t: entry.t,
                    statement: *statement,
                    certainty: *certainty,
                    basis: *basis,
                    source: *source,
                    origin: *origin,
                },
            );
        }
        let LogPayload::BondChanged {
            other,
            before,
            after,
        } = &entry.payload
        else {
            continue;
        };
        let [Subject::Sim(owner)] = entry.subjects.as_slice() else {
            continue;
        };
        if *owner != chooser || *other != avoided || before[TRUST] <= after[TRUST] {
            continue;
        }
        for cause in entry.causes.as_slice() {
            let Cause::Event(id) = cause else {
                continue;
            };
            let Some(number) = event_number(id) else {
                continue;
            };
            let Some(heard) = believed.get(&number) else {
                continue;
            };
            let candidate = Heard {
                index: heard.index,
                t: entry.t,
                statement: heard.statement,
                certainty: heard.certainty,
                basis: heard.basis,
                source: heard.source,
                origin: heard.origin,
            };
            let replace = best.as_ref().is_none_or(|old| beats(&candidate, old));
            if replace {
                best = Some(candidate);
            }
        }
    }
    best
}

fn beats(candidate: &Heard, old: &Heard) -> bool {
    match candidate.t.cmp(&old.t) {
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Equal => match candidate.statement.key().cmp(&old.statement.key()) {
            core::cmp::Ordering::Less => true,
            core::cmp::Ordering::Greater => false,
            core::cmp::Ordering::Equal => candidate.index < old.index,
        },
    }
}

/// Truth at the clock, not at the choice. A deed the doer is not moved in for is unknown.
/// `VaterVon` is false for a founder and for any other man than the biological father.
fn truth_word(world: &World, statement: Statement) -> &'static str {
    match statement {
        Statement::VaterVon { child, man } => {
            if world.state().born(child).is_none() {
                return "unknown";
            }
            if world.state().biological_father(child) == Some(man) {
                "true"
            } else {
                "false"
            }
        }
        Statement::Tat {
            doer,
            kind,
            other,
            day,
        } => tat_truth(world, doer, kind, other, day),
    }
}

fn tat_truth(world: &World, doer: SimId, kind: u16, other: SimId, day: i64) -> &'static str {
    let state = world.state();
    let Some(social) = state.data().social() else {
        return "unknown";
    };
    let Some(home) = state.household_of(doer) else {
        return "unknown";
    };
    let Some(moved) = state.house().households.get(&home) else {
        return "unknown";
    };
    let today = house_day(world.closed_through(), moved.since);
    let answer = match state.house().social.deeds.get(&doer) {
        Some(deeds) => deeds.knows(kind, other, day, today, social.limits.deed_window_days),
        None => Deeds::default().knows(kind, other, day, today, social.limits.deed_window_days),
    };
    match answer {
        DeedAnswer::Done => "true",
        DeedAnswer::NotDone => "false",
        DeedAnswer::Unknown => "unknown",
    }
}

fn missing_belief() -> Value {
    json!({
        "found": false,
        "statement": {},
        "certainty": 0,
        "basis": "",
        "source": 0,
        "origin": 0,
        "secret": false,
        "truth": "none",
    })
}

fn statement_json(statement: Statement) -> Value {
    serde_json::to_value(statement).expect("a statement is json")
}

fn basis_word(basis: Basis) -> &'static str {
    match basis {
        Basis::Beteiligt => "beteiligt",
        Basis::Gesehen => "gesehen",
        Basis::Gehoert => "gehört",
        Basis::Geschlossen => "geschlossen",
        Basis::Widerlegt => "widerlegt",
    }
}

fn sim_number(id: Option<SimId>) -> u64 {
    id.map(SimId::get).unwrap_or(0)
}

fn event_number(id: &EventId) -> Option<u64> {
    serde_json::to_value(id).ok()?.as_u64()
}

fn fade_rates(state: &WorldState) -> (u16, u16) {
    match state.data().social() {
        Some(social) => (social.bonds.familiarity_fade, social.bonds.grudge_fade),
        None => (0, 0),
    }
}

/// Feelings replayed from the log. A move-out freezes them, so the days outside do not fade
/// (ADR-0032 section 7). A move-in keeps the frozen values and starts the fade again.
#[derive(Clone, Copy)]
struct Held {
    channels: [u16; 4],
    since: SimTime,
    temper: i16,
    temper_since: SimTime,
    frozen: bool,
}

fn channels_now(held: &Held, t: SimTime, rules: &FeelingRules) -> [u16; 4] {
    if held.frozen {
        return held.channels;
    }
    let rates = rules.rates.all();
    core::array::from_fn(|channel| {
        channel_at(held.channels[channel], rates[channel], held.since, t)
    })
}

fn temper_now(held: &Held, t: SimTime, rules: &FeelingRules) -> i16 {
    if held.frozen {
        held.temper
    } else {
        temper_at(held.temper, rules.mood_return, held.temper_since, t)
    }
}

fn sims_of(entry: &LogEntry) -> Vec<SimId> {
    entry
        .subjects
        .iter()
        .map(|Subject::Sim(sim)| *sim)
        .collect()
}

/// `history` in order. `until` drops lines after that second. The map is the state after the
/// last kept line; the second map is the channels just before each `core:felt`.
fn replay(
    rules: &FeelingRules,
    history: &[LogEntry],
    until: Option<SimTime>,
) -> (BTreeMap<SimId, Held>, BTreeMap<u64, [u16; 4]>) {
    let mut held: BTreeMap<SimId, Held> = BTreeMap::new();
    let mut before = BTreeMap::new();
    for entry in history {
        if until.is_some_and(|until| entry.t > until) {
            continue;
        }
        match &entry.payload {
            LogPayload::Felt {
                joy,
                anger,
                sadness,
                fear,
                temper,
            } => {
                let [Subject::Sim(sim)] = entry.subjects.as_slice() else {
                    continue;
                };
                let prior = match held.get(sim) {
                    Some(stored) => channels_now(stored, entry.t, rules),
                    None => [0; 4],
                };
                if let Some(id) = event_number(&entry.id) {
                    before.insert(id, prior);
                }
                held.insert(
                    *sim,
                    Held {
                        channels: [*joy, *anger, *sadness, *fear],
                        since: entry.t,
                        temper: *temper,
                        temper_since: entry.t,
                        frozen: false,
                    },
                );
            }
            LogPayload::MovedOut { .. } => {
                for sim in sims_of(entry) {
                    let Some(stored) = held.get(&sim).filter(|stored| !stored.frozen).copied()
                    else {
                        continue;
                    };
                    held.insert(
                        sim,
                        Held {
                            channels: channels_now(&stored, entry.t, rules),
                            since: entry.t,
                            temper: temper_now(&stored, entry.t, rules),
                            temper_since: entry.t,
                            frozen: true,
                        },
                    );
                }
            }
            LogPayload::MovedIn { .. } => {
                for sim in sims_of(entry) {
                    let Some(stored) = held.get(&sim).copied() else {
                        continue;
                    };
                    let (channels, temper) = if stored.frozen {
                        (stored.channels, stored.temper)
                    } else {
                        (
                            channels_now(&stored, entry.t, rules),
                            temper_now(&stored, entry.t, rules),
                        )
                    };
                    held.insert(
                        sim,
                        Held {
                            channels,
                            since: entry.t,
                            temper,
                            temper_since: entry.t,
                            frozen: false,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    (held, before)
}

/// The temper of `sim` at `at` (ADR-0032 section 7). Lines after `at` are ignored. No feeling
/// line means 0, and so does a world without feeling rules.
pub(crate) fn temper_of(
    rules: Option<&FeelingRules>,
    sim: SimId,
    at: SimTime,
    history: &[LogEntry],
) -> i16 {
    let Some(rules) = rules else {
        return 0;
    };
    match replay(rules, history, Some(at)).0.get(&sim) {
        Some(stored) => temper_now(stored, at, rules),
        None => 0,
    }
}

/// The four channels just before each `core:felt` in `history`, keyed by the entry's number.
pub(crate) fn channels_before(
    rules: &FeelingRules,
    history: &[LogEntry],
) -> BTreeMap<u64, [u16; 4]> {
    replay(rules, history, None).1
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::social::data::SocialData;

    fn rules() -> FeelingRules {
        let raw: SocialData =
            serde_json::from_str(include_str!("../../../data/social/social.json"))
                .expect("the file");
        raw.checked().expect("checked").feelings
    }

    fn line(id: u64, t: i64, subjects: &str, payload: &str) -> LogEntry {
        serde_json::from_str(&format!(
            r#"{{"id":{id},"t":{t},"subjects":[{subjects}],"causes":[{{"rule":"core:test"}}],"payload":{payload}}}"#
        ))
        .expect("a line")
    }

    fn who(id: u64) -> SimId {
        SimId::try_from(id).expect("a sim")
    }

    /// Temper −300 frozen across ten days outside, then one day after moving back in, is −200.
    /// Fading the whole eleven days would reach 0.
    #[test]
    fn a_move_out_freezes_the_temper_across_the_gap() {
        let rules = rules();
        let day = 86_400;
        let felt = r#"{"core:felt":{"joy":0,"anger":0,"sadness":0,"fear":0,"temper":-300}}"#;
        let out = r#"{"core:moved_out":{"household":1}}"#;
        let inn = r#"{"core:moved_in":{"household":1,"house":"core:flat"}}"#;
        let history = vec![
            line(1, 0, r#"{"sim":1}"#, felt),
            line(2, 0, r#"{"sim":1}"#, out),
            line(3, 10 * day, r#"{"sim":1}"#, inn),
        ];
        let at = SimTime::from_secs(11 * day);
        assert_eq!(temper_of(Some(&rules), who(1), at, &history), -200);
        assert_eq!(
            temper_of(Some(&rules), who(1), SimTime::from_secs(0), &history),
            -300
        );
        assert_eq!(temper_of(None, who(1), at, &history), 0);
    }

    /// Joy 800 at 300 a day is 500 a day later. A move-out in the same second keeps 800.
    #[test]
    fn channels_before_a_felt_fade_unless_the_person_is_outside() {
        let rules = rules();
        let day = 86_400;
        let joy = |value: u16| {
            format!(
                r#"{{"core:felt":{{"joy":{value},"anger":0,"sadness":0,"fear":0,"temper":0}}}}"#
            )
        };
        let open = vec![
            line(1, 0, r#"{"sim":1}"#, &joy(800)),
            line(2, day, r#"{"sim":1}"#, &joy(0)),
        ];
        let before = channels_before(&rules, &open);
        assert_eq!(before.get(&1), Some(&[0, 0, 0, 0]));
        assert_eq!(before.get(&2), Some(&[500, 0, 0, 0]));
        let frozen = vec![
            line(1, 0, r#"{"sim":1}"#, &joy(800)),
            line(
                2,
                0,
                r#"{"sim":1}"#,
                r#"{"core:moved_out":{"household":1}}"#,
            ),
            line(3, day, r#"{"sim":1}"#, &joy(0)),
        ];
        let before = channels_before(&rules, &frozen);
        assert_eq!(before.get(&3), Some(&[800, 0, 0, 0]));
    }
}
