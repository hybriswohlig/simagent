//! Beliefs and the deeds that are their truth (ADR-0031 sections 1 to 3). A person holds at
//! most `limits.beliefs` beliefs, sorted by the statement key, and at most `limits.deeds` of
//! their own deeds, in the order they came about. A deed outside the last
//! `limits.deed_window_days` house days is inert: not known and not material. Certainty does
//! not fade. This module is the pure part: heard certainty, confirmation, a seen deed replacing
//! a heard one, eviction, the bond steps of a heard deed, gossip material and who speaks. The
//! state side (`state::belief`) writes the lists and the log. A lie (`TALK_LIE`, section 4) and
//! a refutation (section 5) are decided here and written by the talk.

use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::social::bond::{GRUDGE, TRUST};
use crate::social::data::BeliefRules;
use crate::time::SimTime;

/// Deed numbers in the order of `BeliefRules::deeds`. The data lists `core:teases`,
/// `core:gossips`, `core:lies`, which the ADR names `neckt`, `lästert`, `lügt`.
pub const NECKT: u16 = 0;
/// `lästert`.
pub const LAESTERT: u16 = 1;
/// `lügt`.
pub const LUEGT: u16 = 2;

/// A statement (ADR-0031 section 1). `VaterVon` sorts before `Tat`. The key of `VaterVon` is
/// the child; `man` is the content, so two men share one key. The key of `Tat` is the doer,
/// the kind, the other person and the house day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Statement {
    VaterVon {
        child: SimId,
        man: SimId,
    },
    Tat {
        doer: SimId,
        kind: u16,
        other: SimId,
        day: i64,
    },
}

/// The key a list holds at most once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatementKey {
    VaterVon(SimId),
    Tat {
        doer: SimId,
        kind: u16,
        other: SimId,
        day: i64,
    },
}

impl Statement {
    /// The key. `VaterVon`'s man is not part of it.
    pub fn key(self) -> StatementKey {
        match self {
            Statement::VaterVon { child, .. } => StatementKey::VaterVon(child),
            Statement::Tat {
                doer,
                kind,
                other,
                day,
            } => StatementKey::Tat {
                doer,
                kind,
                other,
                day,
            },
        }
    }

    /// The doer of a `Tat`. A `VaterVon` has none.
    pub fn doer(self) -> Option<SimId> {
        match self {
            Statement::Tat { doer, .. } => Some(doer),
            Statement::VaterVon { .. } => None,
        }
    }
}

/// How a belief is held (ADR-0031 section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    Beteiligt,
    Gesehen,
    #[serde(rename = "gehört")]
    Gehoert,
    Geschlossen,
    Widerlegt,
}

/// One belief. `since` is the second it was written down. Beliefs do not fade in this phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Belief {
    pub statement: Statement,
    pub certainty: u16,
    pub since: SimTime,
    pub source: Option<SimId>,
    pub origin: Option<SimId>,
    pub basis: Basis,
    pub secret: bool,
}

impl Belief {
    pub fn key(&self) -> StatementKey {
        self.statement.key()
    }
}

/// The belief of `key` in a list sorted by key.
pub fn find(list: &[Belief], key: StatementKey) -> Option<&Belief> {
    let at = list.binary_search_by_key(&key, Belief::key).ok()?;
    Some(&list[at])
}

/// What hearing a sentence did (ADR-0031 section 2, "Gehört").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// `widerlegt`, or a confirmation that does not rise. Nothing is written.
    Unchanged,
    /// The same statement. Certainty rose, source and origin stay.
    ///
    /// READING: the reference sets `since` to now. The ADR says source and origin stay and
    /// is silent on `since`, so this follows the reference.
    Confirmed(Belief),
    /// A new key, or the same key with different content (`replaced`: another man, or heard
    /// in place of seen, or the reverse).
    Stored { belief: Belief, replaced: bool },
}

/// `min(heard_cap, floor(c · (heard_floor + trust) / heard_divisor))` in `i64`.
pub fn heard_certainty(speaker_certainty: u16, trust: u16, rules: &BeliefRules) -> u16 {
    let raw = i64::from(speaker_certainty) * (i64::from(rules.heard_floor) + i64::from(trust))
        / i64::from(rules.heard_divisor);
    let capped = raw.min(i64::from(rules.heard_cap)).max(0);
    u16::try_from(capped).unwrap_or_else(|_| panic!("invariant belief.certainty"))
}

/// The hearer takes `incoming` from `speaker` (ADR-0031 section 2). `speaker_belief` is what
/// the speaker holds, if anything: a lie has none, and then the origin is the speaker. A held
/// `widerlegt` never changes. The same statement (same key and the same man) is a confirmation
/// of +`confirm_step` up to `confirm_cap`, and a certainty above the cap is not lowered. Another
/// man under the same key is replaced.
#[allow(clippy::too_many_arguments)]
pub fn hear(
    held: Option<Belief>,
    incoming: Statement,
    speaker: SimId,
    speaker_belief: Option<Belief>,
    speaker_certainty: u16,
    trust: u16,
    now: SimTime,
    rules: &BeliefRules,
) -> Taken {
    if held.is_some_and(|belief| belief.basis == Basis::Widerlegt) {
        return Taken::Unchanged;
    }
    if let Some(belief) = held
        && belief.key() == incoming.key()
        && belief.statement.man() == incoming.man()
    {
        let sum = i64::from(belief.certainty) + i64::from(rules.confirm_step);
        let capped = sum.min(i64::from(rules.confirm_cap)).max(0);
        let capped = u16::try_from(capped).unwrap_or_else(|_| panic!("invariant belief.certainty"));
        let raised = belief.certainty.max(capped);
        if raised == belief.certainty {
            return Taken::Unchanged;
        }
        return Taken::Confirmed(Belief {
            certainty: raised,
            since: now,
            ..belief
        });
    }
    let origin = match speaker_belief {
        Some(belief) if belief.basis == Basis::Gehoert => belief.origin,
        _ => Some(speaker),
    };
    let belief = Belief {
        statement: incoming,
        certainty: heard_certainty(speaker_certainty, trust, rules),
        since: now,
        source: Some(speaker),
        origin,
        basis: Basis::Gehoert,
        secret: false,
    };
    Taken::Stored {
        belief,
        replaced: held.is_some(),
    }
}

/// A deed seen in a talk: certainty 1000, basis `gesehen`, source and origin empty. It replaces
/// a heard sentence of the same key and leaves every other basis alone (ADR-0031 section 2).
pub fn see(held: Option<Belief>, statement: Statement, now: SimTime) -> Taken {
    if held.is_some_and(|belief| belief.key() != statement.key()) {
        panic!("invariant belief.key");
    }
    let belief = Belief {
        statement,
        certainty: 1000,
        since: now,
        source: None,
        origin: None,
        basis: Basis::Gesehen,
        secret: false,
    };
    match held {
        None => Taken::Stored {
            belief,
            replaced: false,
        },
        Some(old) if old.basis == Basis::Gehoert => Taken::Stored {
            belief,
            replaced: true,
        },
        Some(_) => Taken::Unchanged,
    }
}

impl Statement {
    /// The named man of a `VaterVon`. A `Tat` has none, so two tats of one key match.
    fn man(self) -> Option<SimId> {
        match self {
            Statement::VaterVon { man, .. } => Some(man),
            Statement::Tat { .. } => None,
        }
    }
}

/// What taking a belief of a new key into a full list did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeliefInsertion {
    /// The list had room.
    Added,
    /// The belief was taken in and this one fell out.
    Displaced(Belief),
    /// The newcomer was the weakest, or every belief already held is protected.
    Refused,
}

/// The order in which a belief falls: smallest certainty, then the older `since`, then the
/// smaller key.
fn falls(belief: Belief) -> (u16, SimTime, StatementKey) {
    (belief.certainty, belief.since, belief.key())
}

fn kept_back(belief: Belief, own_children: &[SimId]) -> bool {
    if belief.secret {
        return true;
    }
    match belief.statement {
        Statement::VaterVon { child, .. } => own_children.contains(&child),
        Statement::Tat { .. } => false,
    }
}

/// Takes `new` into the sorted `list`. The key must be new. A full list drops the weakest of
/// the ones that may fall and of `new`, unless `new` is protected, in which case it never
/// falls (ADR-0031 section 2: never a `secret`, never a `VaterVon` of the holder's own child).
/// The newcomer counts, as in ADR-0030 section 1. One belief falls, even when the list is
/// already past `cap` (the reference builds that case).
///
/// # Panics
///
/// Invariant `belief.double_key` if the list holds the key of `new`.
#[track_caller]
pub fn insert(
    list: &mut Vec<Belief>,
    new: Belief,
    own_children: &[SimId],
    cap: usize,
) -> BeliefInsertion {
    if list.binary_search_by_key(&new.key(), Belief::key).is_ok() {
        panic!("invariant belief.double_key");
    }
    if list.len() < cap {
        let at = list
            .binary_search_by_key(&new.key(), Belief::key)
            .unwrap_or_else(|at| at);
        list.insert(at, new);
        return BeliefInsertion::Added;
    }
    let mut weakest: Option<(usize, (u16, SimTime, StatementKey))> = None;
    for (index, belief) in list.iter().enumerate() {
        if kept_back(*belief, own_children) {
            continue;
        }
        let rank = falls(*belief);
        if weakest.is_none_or(|(_, old)| rank < old) {
            weakest = Some((index, rank));
        }
    }
    let Some((index, rank)) = weakest else {
        return BeliefInsertion::Refused;
    };
    if !kept_back(new, own_children) && falls(new) <= rank {
        return BeliefInsertion::Refused;
    }
    let old = list.remove(index);
    let at = list
        .binary_search_by_key(&new.key(), Belief::key)
        .unwrap_or_else(|at| at);
    list.insert(at, new);
    BeliefInsertion::Displaced(old)
}

/// The steps on the hearer's edge to the doer of a heard deed (ADR-0031 section 3). A
/// `lästert` or `lügt` against the hearer is trust and grudge in one entry. Any other deed,
/// `neckt` included, is trust only. A `VaterVon` has none. A confirmation does not ask.
pub fn heard_bond_steps(
    hearer: SimId,
    statement: Statement,
    rules: &BeliefRules,
) -> Option<Vec<(usize, i16)>> {
    let Statement::Tat { kind, other, .. } = statement else {
        return None;
    };
    if other == hearer && (kind == LAESTERT || kind == LUEGT) {
        Some(vec![
            (TRUST, rules.heard_aimed_trust),
            (GRUDGE, rules.heard_aimed_grudge),
        ])
    } else {
        Some(vec![(TRUST, rules.heard_other_trust)])
    }
}

/// One own deed, in the order the deeds came about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deed {
    pub kind: u16,
    pub other: SimId,
    pub day: i64,
}

/// The own deeds of the last window of house days, at most the cap, oldest first. `dropped_day`
/// is the latest house day of a deed the cap dropped. That day and every earlier one can no
/// longer be judged, even when a later deed of the same day is still in `items`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deeds {
    pub items: Vec<Deed>,
    pub dropped_day: Option<i64>,
}

/// What the deed list can say about one deed on one house day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeedAnswer {
    /// The deed is in the list.
    Done,
    /// The day is in the window and the deed is not in the list.
    NotDone,
    /// The day is in the future, or before the window, or on a day the cap already closed.
    Unknown,
}

impl Deeds {
    /// Appends a deed. Past `cap`, the oldest falls and `dropped_day` becomes the later of the
    /// fallen days. The calendar window is not applied here.
    pub fn add(&mut self, kind: u16, other: SimId, day: i64, cap: usize) {
        self.items.push(Deed { kind, other, day });
        if self.items.len() > cap {
            let fallen = self.items.remove(0);
            self.dropped_day = Some(match self.dropped_day {
                Some(old) => old.max(fallen.day),
                None => fallen.day,
            });
        }
    }

    /// Drops deeds outside today and the `window_days - 1` days before it. This does not set
    /// `dropped_day`: only the cap closes a day that is still inside the calendar window.
    pub fn prune_calendar(&mut self, today: i64, window_days: u16) {
        let start = today - (i64::from(window_days) - 1);
        self.items
            .retain(|deed| deed.day >= start && deed.day <= today);
    }

    /// The first house day the list can still judge.
    pub fn window_start(&self, today: i64, window_days: u16) -> i64 {
        window_open(today, window_days, self.dropped_day)
    }

    /// Whether this deed was done on `day`, as of `today`. A stored deed outside the window
    /// is unknown: the list may still hold it, and it does not count.
    pub fn knows(
        &self,
        kind: u16,
        other: SimId,
        day: i64,
        today: i64,
        window_days: u16,
    ) -> DeedAnswer {
        if !day_in_window(day, today, window_days, self.dropped_day) {
            return DeedAnswer::Unknown;
        }
        let done = self
            .items
            .iter()
            .any(|deed| deed.kind == kind && deed.other == other && deed.day == day);
        if done {
            DeedAnswer::Done
        } else {
            DeedAnswer::NotDone
        }
    }
}

/// The first house day still inside the window. `dropped_day` shortens it: that day and every
/// earlier one are closed (ADR-0031 section 1).
fn window_open(today: i64, window_days: u16, dropped_day: Option<i64>) -> i64 {
    let mut start = today - (i64::from(window_days) - 1);
    if let Some(dropped) = dropped_day {
        start = start.max(dropped + 1);
    }
    start
}

/// Whether `day` is inside the window as of `today`. Outside, the deed is inert.
pub fn day_in_window(day: i64, today: i64, window_days: u16, dropped_day: Option<i64>) -> bool {
    day >= window_open(today, window_days, dropped_day) && day <= today
}

/// The giver's material for the hearer (ADR-0031 section 3): a `Tat` about a third party who
/// is neither of the two and is not seated, not secret, certainty above 0, that the hearer
/// does not hold as `widerlegt` and does not hold at least as surely. Sorted by key.
///
/// READING: "Subjekt" is the doer. "Mindestens so sicher" compares with the giver's certainty.
/// Seated is whoever sits at the furniture, of any age.
pub fn gossip_material(
    giver: SimId,
    giver_beliefs: &[Belief],
    hearer: SimId,
    hearer_beliefs: &[Belief],
    seated: &[SimId],
) -> Vec<Belief> {
    let mut out = Vec::new();
    for belief in giver_beliefs {
        let Statement::Tat { doer, .. } = belief.statement else {
            continue;
        };
        if belief.secret || belief.certainty == 0 {
            continue;
        }
        if doer == giver || doer == hearer || seated.contains(&doer) {
            continue;
        }
        if let Some(held) = find(hearer_beliefs, belief.key())
            && (held.basis == Basis::Widerlegt || held.certainty >= belief.certainty)
        {
            continue;
        }
        out.push(*belief);
    }
    out.sort_by_key(Belief::key);
    out
}

/// Material that is still inside the house-day window (ADR-0031 section 1, addendum
/// 2026-11-11). A `Tat` outside it is inert, so it is not material. `dropped_of` is that
/// doer's `dropped_day`; the cap shortens the window the same way as [`Deeds::knows`].
pub fn live_material(
    material: &[Belief],
    today: i64,
    window_days: u16,
    dropped_of: impl Fn(SimId) -> Option<i64>,
) -> Vec<Belief> {
    material
        .iter()
        .copied()
        .filter(|belief| match belief.statement {
            Statement::Tat { doer, day, .. } => {
                day_in_window(day, today, window_days, dropped_of(doer))
            }
            Statement::VaterVon { .. } => false,
        })
        .collect()
}

/// The W of a lie (ADR-0031 section 4): grudge from `from` on a household member who is not
/// the hearer, the smallest [`SimId`] when several qualify.
///
/// READING: the ADR names “not the hearer”. The reference also skips the speaker. A grudge on
/// oneself is not a household edge, so this skips both.
pub fn lie_reason(
    speaker: SimId,
    hearer: SimId,
    household: &[SimId],
    grudge_of: impl Fn(SimId) -> u16,
    from: u16,
) -> Option<SimId> {
    let mut members = household.to_vec();
    members.sort_unstable();
    members
        .into_iter()
        .find(|&w| w != speaker && w != hearer && grudge_of(w) >= from)
}

/// What a liar says: `Tat(W, lästert, hearer, today)` at `rules.lie_certainty` (900).
pub fn lie_statement(w: SimId, hearer: SimId, today: i64, rules: &BeliefRules) -> (Statement, u16) {
    (
        Statement::Tat {
            doer: w,
            kind: LAESTERT,
            other: hearer,
            day: today,
        },
        rules.lie_certainty,
    )
}

/// Whether `refuter` can refute `belief` (ADR-0031 section 5). Certainty above 0, not
/// `widerlegt` and not secret, and the statement known false now. A `Tat` is known false
/// only when `doer_deeds` say the alleged doer did not do it. A day outside his window, or
/// a day the cap closed, is not known false, so nobody can refute it. Only then the doer
/// himself, or anyone who already holds that key as `widerlegt`. A `VaterVon` is known
/// false when the named man is not the genealogical father; then anyone who holds the true
/// man at 1000 with no secret. A true statement is never refuted.
///
/// `doer_deeds` are the alleged doer's. A `VaterVon` does not read them.
pub fn can_refute(
    refuter: SimId,
    refuter_beliefs: &[Belief],
    doer_deeds: &Deeds,
    belief: &Belief,
    true_father: impl Fn(SimId) -> Option<SimId>,
    today: i64,
    window_days: u16,
) -> bool {
    if belief.certainty == 0 || belief.basis == Basis::Widerlegt || belief.secret {
        return false;
    }
    match belief.statement {
        Statement::VaterVon { child, man } => {
            let Some(truth) = true_father(child) else {
                return false;
            };
            if man == truth {
                return false;
            }
            let Some(mine) = find(refuter_beliefs, belief.key()) else {
                return false;
            };
            let Statement::VaterVon { man: held, .. } = mine.statement else {
                return false;
            };
            held == truth && mine.certainty == 1000 && !mine.secret
        }
        Statement::Tat {
            doer,
            kind,
            other,
            day,
        } => {
            if doer_deeds.knows(kind, other, day, today, window_days) != DeedAnswer::NotDone {
                return false;
            }
            refuter == doer
                || find(refuter_beliefs, belief.key())
                    .is_some_and(|mine| mine.basis == Basis::Widerlegt)
        }
    }
}

/// One refutation of a talk: who refutes, whose belief, and which key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefutationPick {
    pub refuter: SimId,
    pub hearer: SimId,
    pub key: StatementKey,
}

/// At most one refutation (ADR-0031 section 5): first `a` as the refuter, then `b`, and in
/// each case the smallest key. A `Tat` is judged on the alleged doer's deeds from
/// `deeds_of`. A missing person is an empty list: in the window that is "not done", and
/// outside it nothing is known. The caller rolls `TALK_REFUTE` once for this pick.
#[allow(clippy::too_many_arguments)]
pub fn pick_refutation(
    a: SimId,
    beliefs_a: &[Belief],
    b: SimId,
    beliefs_b: &[Belief],
    deeds_of: impl Fn(SimId) -> Deeds,
    true_father: impl Fn(SimId) -> Option<SimId>,
    today: i64,
    window_days: u16,
) -> Option<RefutationPick> {
    let sides = [(a, beliefs_a, b, beliefs_b), (b, beliefs_b, a, beliefs_a)];
    for (refuter, own, hearer, theirs) in sides {
        let mut keys: Vec<StatementKey> = theirs
            .iter()
            .filter(|belief| {
                let deeds = match belief.statement {
                    Statement::Tat { doer, .. } => deeds_of(doer),
                    Statement::VaterVon { .. } => Deeds::default(),
                };
                can_refute(
                    refuter,
                    own,
                    &deeds,
                    belief,
                    &true_father,
                    today,
                    window_days,
                )
            })
            .map(Belief::key)
            .collect();
        keys.sort_unstable();
        if let Some(key) = keys.first().copied() {
            return Some(RefutationPick {
                refuter,
                hearer,
                key,
            });
        }
    }
    None
}

/// The hearer's belief after a refutation, and the steps on the edge to the old source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefutationEffect {
    pub belief: Belief,
    /// The immediate source and the one bond step. None when the false belief had no source.
    pub toward_source: Option<(SimId, Vec<(usize, i16)>)>,
}

/// The effects (ADR-0031 section 5). A false `Tat` becomes certainty 0 and `widerlegt`. A
/// false `VaterVon` becomes the true man at 1000, `widerlegt`, not secret. `since` is now,
/// and source and origin are the refuter. Trust in the old source is `refute_trust`; grudge
/// `refute_grudge` as well when that source is the origin. No source, no step.
///
/// READING: the reference adds the grudge whenever `origin == source`, which on a `Tat` is
/// the origin of a lie and on a directly heard sentence is the same person. A `VaterVon`
/// with a source is not a lie; the reference body still adds the grudge if the two match.
/// This follows that body. The checked `VaterVon` has no source.
pub fn refute(
    belief: Belief,
    refuter: SimId,
    true_man: Option<SimId>,
    now: SimTime,
    rules: &BeliefRules,
) -> RefutationEffect {
    let statement = match belief.statement {
        Statement::VaterVon { child, .. } => Statement::VaterVon {
            child,
            man: true_man.unwrap_or_else(|| panic!("invariant belief.father")),
        },
        Statement::Tat { .. } => belief.statement,
    };
    let certainty = match belief.statement {
        Statement::VaterVon { .. } => 1000,
        Statement::Tat { .. } => 0,
    };
    let new = Belief {
        statement,
        certainty,
        since: now,
        source: Some(refuter),
        origin: Some(refuter),
        basis: Basis::Widerlegt,
        secret: false,
    };
    let Some(source) = belief.source else {
        return RefutationEffect {
            belief: new,
            toward_source: None,
        };
    };
    let mut steps = vec![(TRUST, rules.refute_trust)];
    if belief.origin == Some(source) {
        steps.push((GRUDGE, rules.refute_grudge));
    }
    RefutationEffect {
        belief: new,
        toward_source: Some((source, steps)),
    }
}

/// Who speaks (ADR-0031 section 3). Someone with material or a reason may. If both may, the
/// larger sum of the certainties of the material speaks, and a tie is the smaller [`SimId`].
///
/// READING: the reference returns its first argument on a tie. The ADR says the smaller id,
/// which is the same when the caller passes the smaller id first, and this does that either way.
pub fn choose_speaker(
    a: SimId,
    material_a: &[Belief],
    reason_a: Option<SimId>,
    b: SimId,
    material_b: &[Belief],
    reason_b: Option<SimId>,
) -> Option<SimId> {
    let a_ok = !material_a.is_empty() || reason_a.is_some();
    let b_ok = !material_b.is_empty() || reason_b.is_some();
    if a_ok && !b_ok {
        return Some(a);
    }
    if b_ok && !a_ok {
        return Some(b);
    }
    if !a_ok {
        return None;
    }
    let sum = |material: &[Belief]| {
        material
            .iter()
            .fold(0_i64, |total, belief| total + i64::from(belief.certainty))
    };
    let sa = sum(material_a);
    let sb = sum(material_b);
    if sb > sa {
        Some(b)
    } else if sa > sb {
        Some(a)
    } else {
        Some(a.min(b))
    }
}

/// The sentence said: the surest material, the smaller key on a tie.
pub fn said_belief(material: &[Belief]) -> Option<Belief> {
    let mut best: Option<Belief> = None;
    for belief in material {
        let rank = (core::cmp::Reverse(belief.certainty), belief.key());
        let take = best.is_none_or(|old| rank < (core::cmp::Reverse(old.certainty), old.key()));
        if take {
            best = Some(*belief);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::social::bond::Bond;
    use crate::social::data::SocialData;

    fn social() -> SocialData {
        let raw: SocialData =
            serde_json::from_str(include_str!("../../../../data/social/social.json"))
                .expect("social json");
        raw.checked().expect("social checks")
    }

    fn rules() -> BeliefRules {
        social().beliefs
    }

    fn id(n: u64) -> SimId {
        SimId::try_from(n).expect("sim")
    }

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    fn tat(doer: u64, kind: u16, other: u64, day: i64) -> Statement {
        Statement::Tat {
            doer: id(doer),
            kind,
            other: id(other),
            day,
        }
    }

    fn heard(statement: Statement, certainty: u16, since: i64, source: u64, origin: u64) -> Belief {
        Belief {
            statement,
            certainty,
            since: t(since),
            source: Some(id(source)),
            origin: Some(id(origin)),
            basis: Basis::Gehoert,
            secret: false,
        }
    }

    fn seen_belief(statement: Statement, since: i64) -> Belief {
        Belief {
            statement,
            certainty: 1000,
            since: t(since),
            source: None,
            origin: None,
            basis: Basis::Gesehen,
            secret: false,
        }
    }

    /// The reference's full list: 32 necken of person 50, certainty 500, then `over` replaces a
    /// key or adds one.
    fn full(over: &[Belief]) -> Vec<Belief> {
        let mut list = Vec::new();
        for day in 0..32 {
            list.push(heard(tat(50, NECKT, 51, day), 500, 100 + day, 2, 2));
        }
        for belief in over {
            if let Some(found) = list.iter_mut().find(|old| old.key() == belief.key()) {
                *found = *belief;
            } else {
                list.push(*belief);
            }
        }
        list.sort_by_key(Belief::key);
        list
    }

    #[test]
    fn the_deed_numbers_follow_the_data() {
        let beliefs = rules();
        let teases = DataId::new("core:teases").expect("id");
        let gossips = DataId::new("core:gossips").expect("id");
        let lies = DataId::new("core:lies").expect("id");
        assert_eq!(beliefs.deeds, vec![teases, gossips, lies]);
        assert_eq!((NECKT, LAESTERT, LUEGT), (0, 1, 2));
        assert_eq!(beliefs.heard_cap, 900);
        assert_eq!(beliefs.heard_floor, 500);
        assert_eq!(beliefs.heard_divisor, 1500);
        assert_eq!(beliefs.confirm_step, 50);
        assert_eq!(beliefs.confirm_cap, 900);
        assert_eq!(beliefs.heard_aimed_trust, -80);
        assert_eq!(beliefs.heard_aimed_grudge, 100);
        assert_eq!(beliefs.heard_other_trust, -20);
    }

    #[test]
    fn vater_von_sorts_before_tat() {
        let child = Statement::VaterVon {
            child: id(1),
            man: id(2),
        };
        let deed = tat(1, NECKT, 2, 0);
        assert!(child.key() < deed.key());
        assert_eq!(child.key(), StatementKey::VaterVon(id(1)));
    }

    /// The heard chain of the reference, including a lie's origin when the speaker holds nothing.
    #[test]
    fn heard_certainty_and_the_chain_match_the_reference() {
        let rules = rules();
        assert_eq!(heard_certainty(1000, 600, &rules), 733);
        assert_eq!(heard_certainty(733, 500, &rules), 488);
        assert_eq!(heard_certainty(488, 0, &rules), 162);
        assert_eq!(heard_certainty(1000, 1000, &rules), 900);
        assert_eq!(heard_certainty(900, 600, &rules), 660);
        let key = tat(3, LAESTERT, 1, 4);
        let speaker = seen_belief(key, 0);
        let Taken::Stored {
            belief: b1,
            replaced,
        } = hear(None, key, id(7), Some(speaker), 1000, 600, t(50), &rules)
        else {
            panic!("new");
        };
        assert!(!replaced);
        assert_eq!(
            (
                b1.certainty,
                b1.source,
                b1.origin,
                b1.basis,
                b1.since,
                b1.secret
            ),
            (733, Some(id(7)), Some(id(7)), Basis::Gehoert, t(50), false)
        );
        let Taken::Stored { belief: b2, .. } =
            hear(None, key, id(8), Some(b1), b1.certainty, 500, t(60), &rules)
        else {
            panic!("passed on");
        };
        assert_eq!(
            (b2.certainty, b2.source, b2.origin),
            (488, Some(id(8)), Some(id(7)))
        );
        let Taken::Stored { belief: b3, .. } =
            hear(None, key, id(9), Some(b2), b2.certainty, 0, t(70), &rules)
        else {
            panic!("third");
        };
        assert_eq!(
            (b3.certainty, b3.source, b3.origin),
            (162, Some(id(9)), Some(id(7)))
        );
        let Taken::Stored {
            belief: lie,
            replaced,
        } = hear(None, key, id(7), None, 900, 600, t(10), &rules)
        else {
            panic!("lie");
        };
        assert!(!replaced);
        assert_eq!(
            (lie.certainty, lie.origin, lie.source),
            (660, Some(id(7)), Some(id(7)))
        );
    }

    #[test]
    fn confirmation_rises_by_fifty_up_to_900_and_keeps_source() {
        let rules = rules();
        let key = tat(3, LAESTERT, 1, 4);
        let held = heard(key, 700, 5, 2, 2);
        let speaker = seen_belief(key, 0);
        let Taken::Confirmed(c) = hear(
            Some(held),
            key,
            id(9),
            Some(speaker),
            733,
            600,
            t(99),
            &rules,
        ) else {
            panic!("confirmed");
        };
        assert_eq!(
            (c.certainty, c.source, c.origin, c.since),
            (750, Some(id(2)), Some(id(2)), t(99))
        );
        let high = Belief {
            certainty: 880,
            ..held
        };
        let Taken::Confirmed(c) = hear(
            Some(high),
            key,
            id(9),
            Some(speaker),
            733,
            600,
            t(99),
            &rules,
        ) else {
            panic!("capped");
        };
        assert_eq!(c.certainty, 900);
        assert_eq!(
            hear(
                Some(Belief {
                    certainty: 900,
                    ..held
                }),
                key,
                id(9),
                Some(speaker),
                733,
                600,
                t(99),
                &rules
            ),
            Taken::Unchanged
        );
        assert_eq!(
            hear(
                Some(seen_belief(key, 5)),
                key,
                id(9),
                Some(speaker),
                733,
                600,
                t(99),
                &rules
            ),
            Taken::Unchanged
        );
        let refuted = Belief {
            certainty: 0,
            basis: Basis::Widerlegt,
            ..held
        };
        assert_eq!(
            hear(
                Some(refuted),
                key,
                id(9),
                Some(speaker),
                733,
                600,
                t(99),
                &rules
            ),
            Taken::Unchanged
        );
    }

    #[test]
    fn another_man_is_replaced_and_seen_replaces_only_heard() {
        let rules = rules();
        let held = Belief {
            statement: Statement::VaterVon {
                child: id(1),
                man: id(5),
            },
            certainty: 700,
            since: t(5),
            source: Some(id(2)),
            origin: Some(id(2)),
            basis: Basis::Gehoert,
            secret: false,
        };
        let incoming = Statement::VaterVon {
            child: id(1),
            man: id(6),
        };
        let Taken::Stored { belief, replaced } =
            hear(Some(held), incoming, id(9), None, 900, 600, t(99), &rules)
        else {
            panic!("replaced");
        };
        assert!(replaced);
        assert_eq!(
            (
                belief.statement,
                belief.certainty,
                belief.secret,
                belief.basis
            ),
            (incoming, 660, false, Basis::Gehoert)
        );
        let key = tat(3, LAESTERT, 1, 4);
        let Taken::Stored { belief, replaced } = see(None, key, t(12)) else {
            panic!("seen");
        };
        assert!(!replaced);
        assert_eq!(
            (belief.certainty, belief.basis, belief.source, belief.origin),
            (1000, Basis::Gesehen, None, None)
        );
        let heard_one = heard(key, 733, 50, 7, 7);
        let Taken::Stored { belief, replaced } = see(Some(heard_one), key, t(12)) else {
            panic!("replaces heard");
        };
        assert!(replaced);
        assert_eq!((belief.certainty, belief.basis), (1000, Basis::Gesehen));
        let refuted = Belief {
            certainty: 0,
            basis: Basis::Widerlegt,
            ..heard_one
        };
        assert_eq!(see(Some(refuted), key, t(12)), Taken::Unchanged);
        assert_eq!(see(Some(seen_belief(key, 1)), key, t(12)), Taken::Unchanged);
        let took_part = Belief {
            basis: Basis::Beteiligt,
            ..seen_belief(key, 1)
        };
        assert_eq!(see(Some(took_part), key, t(12)), Taken::Unchanged);
    }

    #[test]
    fn heard_bond_steps_match_the_reference_chain() {
        let rules = rules();
        assert_eq!(
            heard_bond_steps(id(1), tat(4, LUEGT, 1, 0), &rules),
            Some(vec![(TRUST, -80), (GRUDGE, 100)])
        );
        assert_eq!(
            heard_bond_steps(id(1), tat(4, NECKT, 1, 0), &rules),
            Some(vec![(TRUST, -20)])
        );
        assert_eq!(
            heard_bond_steps(id(1), tat(4, LAESTERT, 9, 0), &rules),
            Some(vec![(TRUST, -20)])
        );
        assert_eq!(
            heard_bond_steps(
                id(1),
                Statement::VaterVon {
                    child: id(5),
                    man: id(6)
                },
                &rules
            ),
            None
        );
        let mut bond = Bond {
            other: id(4),
            values: [500, 600, 400, 0, 300, 0],
            since: t(0),
        };
        for (day, want) in [520_u16, 440, 360].into_iter().enumerate() {
            let day = i64::try_from(day).expect("day");
            let steps = heard_bond_steps(id(1), tat(4, LAESTERT, 1, day), &rules).expect("steps");
            bond.apply(t(0), 0, 0, &steps);
            assert_eq!(bond.values[TRUST], want);
        }
        assert_eq!(bond.values[GRUDGE], 300);
        assert!(bond.values[TRUST] < 400);
    }

    #[test]
    fn eviction_drops_the_weakest_and_protects_a_secret_and_an_own_child() {
        let new = heard(tat(60, NECKT, 61, 0), 800, 1000, 2, 2);
        let mut list = full(&[heard(tat(50, NECKT, 51, 5), 100, 105, 2, 2)]);
        match insert(&mut list, new, &[], 32) {
            BeliefInsertion::Displaced(old) => assert_eq!(old.statement, tat(50, NECKT, 51, 5)),
            other => panic!("{other:?}"),
        }
        assert_eq!(list.len(), 32);
        let mut list = full(&[
            heard(tat(50, NECKT, 51, 5), 100, 105, 2, 2),
            heard(tat(50, NECKT, 51, 9), 100, 99, 2, 2),
        ]);
        match insert(&mut list, new, &[], 32) {
            BeliefInsertion::Displaced(old) => assert_eq!(old.statement, tat(50, NECKT, 51, 9)),
            other => panic!("{other:?}"),
        }
        let mut list = full(&[
            heard(tat(50, NECKT, 51, 5), 100, 105, 2, 2),
            heard(tat(50, NECKT, 51, 9), 100, 105, 2, 2),
        ]);
        match insert(&mut list, new, &[], 32) {
            BeliefInsertion::Displaced(old) => assert_eq!(old.statement, tat(50, NECKT, 51, 5)),
            other => panic!("{other:?}"),
        }
        let weak = heard(tat(60, NECKT, 61, 1), 50, 1000, 2, 2);
        let mut list = full(&[]);
        assert_eq!(insert(&mut list, weak, &[], 32), BeliefInsertion::Refused);
        assert!(find(&list, weak.key()).is_none());
        let mut secret = heard(tat(50, NECKT, 51, 5), 100, 105, 2, 2);
        secret.secret = true;
        let mut list = full(&[secret]);
        let weaker = Belief {
            certainty: 0,
            ..new
        };
        assert_eq!(insert(&mut list, weaker, &[], 32), BeliefInsertion::Refused);
        assert!(find(&list, secret.key()).is_some_and(|belief| belief.secret));
        let vater = Belief {
            statement: Statement::VaterVon {
                child: id(77),
                man: id(5),
            },
            certainty: 100,
            since: t(5),
            source: None,
            origin: None,
            basis: Basis::Beteiligt,
            secret: false,
        };
        let mut list = full(&[vater]);
        assert_eq!(list.len(), 33);
        match insert(&mut list, new, &[id(77)], 32) {
            BeliefInsertion::Displaced(old) => {
                assert_ne!(old.key(), vater.key());
                assert_eq!(old.statement, tat(50, NECKT, 51, 0));
            }
            other => panic!("{other:?}"),
        }
        assert!(find(&list, vater.key()).is_some());
        let mut list = full(&[vater]);
        match insert(&mut list, new, &[], 32) {
            BeliefInsertion::Displaced(old) => assert_eq!(old.key(), vater.key()),
            other => panic!("{other:?}"),
        }
        let weak_secret = Belief {
            statement: Statement::VaterVon {
                child: id(78),
                man: id(5),
            },
            certainty: 10,
            since: t(1000),
            source: None,
            origin: None,
            basis: Basis::Beteiligt,
            secret: true,
        };
        let mut list = full(&[]);
        match insert(&mut list, weak_secret, &[], 32) {
            BeliefInsertion::Displaced(_) => assert!(find(&list, weak_secret.key()).is_some()),
            other => panic!("{other:?}"),
        }
        let weak_own = Belief {
            statement: Statement::VaterVon {
                child: id(79),
                man: id(6),
            },
            certainty: 10,
            since: t(1000),
            source: None,
            origin: None,
            basis: Basis::Geschlossen,
            secret: false,
        };
        let mut list = full(&[]);
        match insert(&mut list, weak_own, &[id(79)], 32) {
            BeliefInsertion::Displaced(_) => assert!(find(&list, weak_own.key()).is_some()),
            other => panic!("{other:?}"),
        }
        let mut protected = full(&[]);
        for belief in &mut protected {
            belief.secret = true;
        }
        assert_eq!(
            insert(&mut protected, new, &[], 32),
            BeliefInsertion::Refused
        );
        assert_eq!(protected.len(), 32);
    }

    #[test]
    fn a_newcomer_of_equal_certainty_counts_by_since_and_key() {
        let mut older = full(&[]);
        let incoming = heard(tat(60, NECKT, 61, 0), 500, 99, 2, 2);
        assert_eq!(
            insert(&mut older, incoming, &[], 32),
            BeliefInsertion::Refused
        );
        let mut newer = full(&[]);
        let incoming = heard(tat(60, NECKT, 61, 0), 500, 200, 2, 2);
        match insert(&mut newer, incoming, &[], 32) {
            BeliefInsertion::Displaced(old) => assert_eq!(old.statement, tat(50, NECKT, 51, 0)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_deed_window_and_the_cap_match_the_reference() {
        let mut deeds = Deeds::default();
        deeds.add(NECKT, id(9), 3, 64);
        assert_eq!(deeds.knows(NECKT, id(9), 3, 10, 30), DeedAnswer::Done);
        assert_eq!(deeds.knows(LAESTERT, id(9), 3, 10, 30), DeedAnswer::NotDone);
        assert_eq!(deeds.knows(NECKT, id(9), 3, 32, 30), DeedAnswer::Done);
        assert_eq!(deeds.knows(NECKT, id(9), 3, 33, 30), DeedAnswer::Unknown);
        assert_eq!(deeds.items.len(), 1, "outside the window it is stored");
        assert_eq!(deeds.knows(NECKT, id(9), 11, 10, 30), DeedAnswer::Unknown);
        let mut deeds = Deeds::default();
        for i in 0..65 {
            deeds.add(NECKT, id(9), i / 5, 64);
        }
        assert_eq!((deeds.items.len(), deeds.dropped_day), (64, Some(0)));
        assert_eq!(deeds.knows(LAESTERT, id(9), 0, 13, 30), DeedAnswer::Unknown);
        assert_eq!(deeds.knows(LAESTERT, id(9), 1, 13, 30), DeedAnswer::NotDone);
        let before = deeds.dropped_day;
        deeds.prune_calendar(13, 30);
        assert_eq!(deeds.dropped_day, before);
        assert_eq!(deeds.items.len(), 64);
    }

    #[test]
    fn gossip_material_speaker_and_the_said_statement_match_the_reference() {
        let kx = tat(30, NECKT, 1, 2);
        let giver = [heard(kx, 733, 5, 7, 7)];
        let raw = gossip_material(id(1), &giver, id(2), &[], &[]);
        assert_eq!(raw.len(), 1);
        // Day 2 is inside on house day 31 (the day and the 29 before) and inert on day 32.
        assert_eq!(live_material(&raw, 31, 30, |_| None).len(), 1);
        assert!(live_material(&raw, 32, 30, |_| None).is_empty());
        // The cap closes day 2, so the window opens at 3.
        assert!(live_material(&raw, 10, 30, |_| Some(2)).is_empty());
        assert_eq!(live_material(&raw, 10, 30, |_| Some(1)).len(), 1);
        assert!(gossip_material(id(1), &giver, id(2), &[], &[id(30)]).is_empty());
        assert!(gossip_material(id(1), &giver, id(30), &[], &[]).is_empty());
        assert!(gossip_material(id(30), &giver, id(2), &[], &[]).is_empty());
        let mut secret = giver[0];
        secret.secret = true;
        assert!(gossip_material(id(1), &[secret], id(2), &[], &[]).is_empty());
        assert!(
            gossip_material(
                id(1),
                &[Belief {
                    certainty: 0,
                    ..giver[0]
                }],
                id(2),
                &[],
                &[]
            )
            .is_empty()
        );
        assert!(
            gossip_material(
                id(1),
                &giver,
                id(2),
                &[Belief {
                    certainty: 733,
                    ..giver[0]
                }],
                &[]
            )
            .is_empty()
        );
        assert_eq!(
            gossip_material(
                id(1),
                &giver,
                id(2),
                &[Belief {
                    certainty: 732,
                    ..giver[0]
                }],
                &[]
            )
            .len(),
            1
        );
        let refuted = Belief {
            certainty: 0,
            basis: Basis::Widerlegt,
            source: Some(id(5)),
            origin: Some(id(5)),
            ..giver[0]
        };
        assert!(gossip_material(id(1), &giver, id(2), &[refuted], &[]).is_empty());
        assert!(gossip_material(id(1), &[refuted], id(2), &[], &[]).is_empty());
        let vater = Belief {
            statement: Statement::VaterVon {
                child: id(77),
                man: id(5),
            },
            certainty: 1000,
            since: t(5),
            source: None,
            origin: None,
            basis: Basis::Beteiligt,
            secret: false,
        };
        assert!(gossip_material(id(1), &[vater], id(2), &[], &[]).is_empty());
        let m1 = heard(tat(30, NECKT, 1, 2), 900, 5, 7, 7);
        let m2 = heard(tat(30, NECKT, 1, 1), 900, 5, 7, 7);
        let m3 = heard(tat(30, NECKT, 1, 3), 500, 5, 7, 7);
        assert_eq!(said_belief(&[m1, m2, m3]).expect("said").key(), m2.key());
        assert_eq!(said_belief(&[]), None);
        assert_eq!(
            choose_speaker(id(1), &[m1], None, id(2), &[], None),
            Some(id(1))
        );
        assert_eq!(
            choose_speaker(id(1), &[], None, id(2), &[], Some(id(5))),
            Some(id(2))
        );
        assert_eq!(choose_speaker(id(1), &[], None, id(2), &[], None), None);
        assert_eq!(
            choose_speaker(id(1), &[m3], None, id(2), &[m1], None),
            Some(id(2))
        );
        assert_eq!(
            choose_speaker(id(1), &[m1], None, id(2), &[m2], None),
            Some(id(1))
        );
        assert_eq!(
            choose_speaker(id(2), &[m1], None, id(1), &[m2], None),
            Some(id(1))
        );
        assert_eq!(
            choose_speaker(id(1), &[], Some(id(40)), id(2), &[m3], None),
            Some(id(2))
        );
    }

    fn vater(child: u64, man: u64, certainty: u16, basis: Basis, secret: bool) -> Belief {
        Belief {
            statement: Statement::VaterVon {
                child: id(child),
                man: id(man),
            },
            certainty,
            since: t(1),
            source: None,
            origin: None,
            basis,
            secret,
        }
    }

    /// The reference's lie reason, the sentence at 900, who can refute, the pick and the effects.
    #[test]
    fn the_lie_and_the_refutation_match_the_reference() {
        let rules = rules();
        let window = social().limits.deed_window_days;
        assert_eq!(window, 30);
        let house = [id(1), id(2), id(3), id(4)];
        fn grudge<'a>(table: &'a [(u64, u16)]) -> impl Fn(SimId) -> u16 + 'a {
            move |w: SimId| {
                table
                    .iter()
                    .find(|(id_, _)| id(*id_) == w)
                    .map(|(_, value)| *value)
                    .unwrap_or(0)
            }
        }
        assert_eq!(
            lie_reason(id(1), id(2), &house, grudge(&[(3, 700)]), 700),
            Some(id(3))
        );
        assert_eq!(
            lie_reason(id(1), id(2), &house, grudge(&[(3, 699)]), 700),
            None
        );
        assert_eq!(
            lie_reason(id(1), id(2), &house, grudge(&[(4, 900), (3, 700)]), 700),
            Some(id(3))
        );
        assert_eq!(
            lie_reason(id(1), id(2), &house, grudge(&[(2, 900)]), 700),
            None
        );
        assert_eq!(
            lie_reason(id(1), id(2), &house, grudge(&[(1, 900)]), 700),
            None
        );
        let (said, certainty) = lie_statement(id(3), id(2), 6, &rules);
        assert_eq!((said, certainty), (tat(3, LAESTERT, 2, 6), 900));

        let truth = |child: SimId| (child == id(7)).then_some(id(20));
        let false_belief = heard(tat(3, LAESTERT, 1, 4), 660, 10, 5, 3);
        let empty = Deeds::default();
        assert!(can_refute(
            id(3),
            &[],
            &empty,
            &false_belief,
            truth,
            6,
            window
        ));
        assert!(!can_refute(
            id(9),
            &[],
            &empty,
            &false_belief,
            truth,
            6,
            window
        ));
        let mut done = Deeds::default();
        done.add(LAESTERT, id(1), 4, 64);
        assert!(!can_refute(
            id(3),
            &[],
            &done,
            &false_belief,
            truth,
            6,
            window
        ));
        assert!(!can_refute(
            id(3),
            &[],
            &empty,
            &false_belief,
            truth,
            40,
            window
        ));
        let held_off = Belief {
            certainty: 0,
            basis: Basis::Widerlegt,
            source: Some(id(2)),
            origin: Some(id(2)),
            since: t(1),
            ..false_belief
        };
        assert!(
            can_refute(id(9), &[held_off], &empty, &false_belief, truth, 6, window),
            "a holder refutes a false deed inside the window"
        );
        assert!(
            !can_refute(id(9), &[held_off], &done, &false_belief, truth, 6, window),
            "a holder cannot refute a deed the doer did"
        );
        assert!(
            !can_refute(id(9), &[held_off], &empty, &false_belief, truth, 40, window),
            "a holder cannot refute a day outside the doer's window"
        );
        let already = Belief {
            basis: Basis::Widerlegt,
            certainty: 0,
            ..false_belief
        };
        assert!(!can_refute(id(3), &[], &empty, &already, truth, 6, window));
        let zero = Belief {
            certainty: 0,
            ..false_belief
        };
        assert!(!can_refute(id(3), &[], &empty, &zero, truth, 6, window));
        let secret = Belief {
            secret: true,
            ..false_belief
        };
        assert!(!can_refute(id(3), &[], &empty, &secret, truth, 6, window));

        let wrong = vater(7, 21, 900, Basis::Geschlossen, false);
        let holds = vater(7, 20, 1000, Basis::Beteiligt, false);
        assert!(can_refute(
            id(40),
            &[holds],
            &empty,
            &wrong,
            truth,
            6,
            window
        ));
        let hidden = Belief {
            secret: true,
            ..holds
        };
        assert!(!can_refute(
            id(40),
            &[hidden],
            &empty,
            &wrong,
            truth,
            6,
            window
        ));
        let short = Belief {
            certainty: 900,
            ..holds
        };
        assert!(!can_refute(
            id(40),
            &[short],
            &empty,
            &wrong,
            truth,
            6,
            window
        ));
        let true_father = Belief {
            statement: Statement::VaterVon {
                child: id(7),
                man: id(20),
            },
            ..wrong
        };
        assert!(!can_refute(
            id(40),
            &[holds],
            &empty,
            &true_father,
            truth,
            6,
            window
        ));

        let ka1 = heard(tat(2, LAESTERT, 1, 1), 600, 1, 5, 2);
        let ka2 = heard(tat(2, LAESTERT, 1, 2), 600, 1, 5, 2);
        let deeds = |_: SimId| Deeds::default();
        let none = pick_refutation(id(1), &[], id(2), &[], deeds, truth, 5, window);
        assert_eq!(none, None);
        let second = pick_refutation(id(1), &[ka2, ka1], id(2), &[], deeds, truth, 5, window);
        assert_eq!(
            second,
            Some(RefutationPick {
                refuter: id(2),
                hearer: id(1),
                key: ka1.key(),
            })
        );
        let pa = heard(tat(2, LAESTERT, 1, 1), 600, 1, 5, 2);
        let pb = heard(tat(1, NECKT, 2, 1), 600, 1, 5, 1);
        let first = pick_refutation(id(1), &[pa], id(2), &[pb], deeds, truth, 5, window);
        assert_eq!(
            first,
            Some(RefutationPick {
                refuter: id(1),
                hearer: id(2),
                key: pb.key(),
            })
        );
        let only_b = pick_refutation(id(1), &[pa], id(2), &[], deeds, truth, 5, window);
        assert_eq!(
            only_b,
            Some(RefutationPick {
                refuter: id(2),
                hearer: id(1),
                key: pa.key(),
            })
        );

        let lie_held = heard(tat(3, LAESTERT, 1, 4), 660, 10, 3, 3);
        let origin = refute(lie_held, id(9), None, t(77), &rules);
        assert_eq!(origin.belief.certainty, 0);
        assert_eq!(origin.belief.basis, Basis::Widerlegt);
        assert_eq!(origin.belief.since, t(77));
        assert_eq!(origin.belief.source, Some(id(9)));
        assert_eq!(origin.belief.origin, Some(id(9)));
        assert!(!origin.belief.secret);
        assert_eq!(origin.belief.statement, lie_held.statement);
        assert_eq!(
            origin.toward_source,
            Some((id(3), vec![(TRUST, -120), (GRUDGE, 150)]))
        );
        let passed = heard(tat(3, LAESTERT, 1, 4), 488, 10, 5, 3);
        let second_hand = refute(passed, id(9), None, t(77), &rules);
        assert_eq!(
            second_hand.toward_source,
            Some((id(5), vec![(TRUST, -120)]))
        );
        let cleared = refute(wrong, id(40), Some(id(20)), t(77), &rules);
        assert_eq!(
            cleared.belief.statement,
            Statement::VaterVon {
                child: id(7),
                man: id(20),
            }
        );
        assert_eq!(cleared.belief.certainty, 1000);
        assert_eq!(cleared.belief.basis, Basis::Widerlegt);
        assert!(!cleared.belief.secret);
        assert_eq!(cleared.belief.since, t(77));
        assert_eq!(cleared.belief.source, Some(id(40)));
        assert_eq!(cleared.belief.origin, Some(id(40)));
        assert_eq!(cleared.toward_source, None);
    }
}
