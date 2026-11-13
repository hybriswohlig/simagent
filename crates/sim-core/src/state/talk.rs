//! A talk of two people at one sociable furniture (ADR-0030 section 3). Planning follows an
//! arrival; the run is the event, or the same-second end that must happen first. Necken writes
//! the seen deed `neckt` for the target only. Klatsch passes one statement on, or a lie when
//! the speaker has a reason and the lie draw hits, and writes the seen deed `lästert` for a
//! bystander, not for the speaker and not for the hearer of that sentence (ADR-0031 sections
//! 2 to 4 and the addendum of 2026-11-12).
//! Every topic may then refute one false belief and both remember "Widerlegung" (section 5).

use alloc::vec;
use alloc::vec::Vec;

use super::action::person_mut;
use super::belief::{self, Heard, HeardStore};
use super::bond::{Formed, form_one, formation_refused};
use super::feeling::{self, Pending};
use super::memory::{WroteMemory, others, plan_forget, remember};
use super::{DomainState, Slot, whole_years};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::data_id::DataId;
use crate::house::data::Need;
use crate::house::state::Place;
use crate::ids::{EventId, HouseholdId, MAX_JS_SAFE, SimId};
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, WorldSeed};
use crate::social::belief::{
    Basis, Belief, Statement, aimed_doer, audience, choose_speaker, gossip_material,
    heard_bond_steps, lie_reason, lie_statement, live_material, pick_refutation, refute,
    said_belief,
};
use crate::social::bond::{GRUDGE, TRUST};
use crate::social::data::{Occasion, REFUTED, TALK};
use crate::social::memory::Memory;
use crate::social::talk::{
    GOSSIP, TEASE, TalkFacts, TalkPair, choose_topic, house_day, interests, lie_draw_key,
    recall_strength, refute_draw_key, talk_conditions, talk_ppm, topic_actor, topic_feelings,
    topic_outcome,
};
use crate::time::{SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The furniture the two share, and the household it stands in.
struct Seat {
    household: HouseholdId,
    object: u16,
    kind: DataId,
}

/// Where `sim` sits, if it is at an object in a moved-in household.
fn seat_of(domain: &DomainState, data: &Data, sim: SimId) -> Option<Seat> {
    let person = domain.house.sims.get(&sim)?;
    if !matches!(person.place, Place::Room(_)) {
        return None;
    }
    let action = person.action?;
    let household = domain.sims.get(&sim)?.household;
    let moved = domain.house.households.get(&household)?;
    let houses = data.houses()?;
    let house = houses.house(&moved.house)?;
    let kind = house
        .def()
        .objects
        .get(usize::from(action.object))?
        .kind
        .clone();
    Some(Seat {
        household,
        object: action.object,
        kind,
    })
}

/// Both at the same sociable object of one household: the object and its kind.
fn together(domain: &DomainState, data: &Data, a: SimId, b: SimId) -> Option<(u16, DataId)> {
    let left = seat_of(domain, data, a)?;
    let right = seat_of(domain, data, b)?;
    if left.household != right.household || left.object != right.object {
        return None;
    }
    let social = data.social()?;
    social
        .sociable(&left.kind)
        .then_some((left.object, left.kind))
}

fn ordered(a: SimId, b: SimId) -> (SimId, SimId) {
    if a < b { (a, b) } else { (b, a) }
}

fn same_home(domain: &DomainState, a: SimId, b: SimId) -> Option<HouseholdId> {
    let left = domain.sims.get(&a)?.household;
    let right = domain.sims.get(&b)?.household;
    (left == right && domain.house.households.contains_key(&left)).then_some(left)
}

fn years(domain: &DomainState, sim: SimId, at: SimTime) -> u32 {
    let born = (domain.sims.get(&sim))
        .map(|person| person.born)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
    whole_years(born, at)
}

/// Talks and social entries of `sim` on house day `day`. A stamp from another day reads as
/// zero: that is the reset at 06:00.
fn counts(domain: &DomainState, sim: SimId, day: i64) -> (u16, u16) {
    match domain.house.sims.get(&sim) {
        Some(person) if person.talk_day == day => (person.talks, person.entries),
        _ => (0, 0),
    }
}

/// Whether the pair may talk at `planned`. Missing household, or two households, is no.
/// A missing bond that cannot be formed is no as well.
fn may(
    domain: &mut DomainState,
    data: &Data,
    a: SimId,
    b: SimId,
    planned: SimTime,
    check_open: bool,
) -> bool {
    if a >= b {
        return false;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let allowed = {
        let Some(home) = same_home(domain, a, b) else {
            return false;
        };
        let moved = &domain.house.households[&home];
        let day = house_day(planned, moved.since);
        let (talks_a, entries_a) = counts(domain, a, day);
        let (talks_b, entries_b) = counts(domain, b, day);
        let facts = TalkFacts {
            open_talk: domain.pending.contains_key(&(a, Slot::Talk(b))),
            last_talk: moved.last_talk.get(&(a, b)).copied(),
            talks_a,
            talks_b,
            entries_a,
            entries_b,
            age_a: years(domain, a, planned),
            age_b: years(domain, b, planned),
        };
        talk_conditions(&social.limits, planned, &facts, check_open)
    };
    if !allowed {
        return false;
    }
    // Reading (ADR-0030 section 2 and section 3, decided 2026-11-10): a missing bond
    // that a full list of protected bonds cannot take is a legal refusal. The talk is
    // not planned and not run, and it writes nothing.
    !formation_refused(domain, data, a, b) && !formation_refused(domain, data, b, a)
}

fn clear(domain: &mut DomainState, a: SimId, b: SimId) {
    domain.pending.remove(&(a, Slot::Talk(b)));
    domain.pending.remove(&(b, Slot::Talk(a)));
}

fn talk_handles(domain: &DomainState, sim: SimId) -> Vec<(SimId, crate::time::EventHandle)> {
    let first = SimId::try_from(1).unwrap_or_else(|_| panic!("invariant talk.pair"));
    let last = SimId::try_from(MAX_JS_SAFE).unwrap_or_else(|_| panic!("invariant talk.pair"));
    (domain
        .pending
        .range((sim, Slot::Talk(first))..=(sim, Slot::Talk(last))))
    .filter_map(|((_, slot), handle)| match slot {
        Slot::Talk(other) => Some((*other, *handle)),
        _ => None,
    })
    .collect()
}

fn slot_due(
    domain: &DomainState,
    sim: SimId,
    slot: Slot,
    now: SimTime,
    ctx: &SimCtx<'_, '_>,
) -> bool {
    (domain.pending.get(&(sim, slot))).is_some_and(|handle| ctx.pending_time(*handle) == Some(now))
}

fn range_due(
    domain: &DomainState,
    sim: SimId,
    first: Slot,
    last: Slot,
    now: SimTime,
    ctx: &SimCtx<'_, '_>,
) -> bool {
    (domain.pending.range((sim, first)..=(sim, last)))
        .any(|(_, handle)| ctx.pending_time(*handle) == Some(now))
}

/// An end or an abort of `sim` is still pending in this second (ADR-0030 section 3, "Gleiche
/// Sekunde"). A crossing or a reserve that does not stand the person up is included: deferring
/// it one micro changes nothing, and the end that does stand them up then runs the talk.
fn abort_due(domain: &DomainState, sim: SimId, now: SimTime, ctx: &SimCtx<'_, '_>) -> bool {
    let one = [Slot::End, Slot::Wake, Slot::Depart, Slot::StandUp]
        .into_iter()
        .any(|slot| slot_due(domain, sim, slot, now, ctx));
    let crossing = range_due(
        domain,
        sim,
        Slot::Crossing(Need::Hunger, 0),
        Slot::Crossing(Need::Fun, u16::MAX),
        now,
        ctx,
    );
    let reserve = range_due(
        domain,
        sim,
        Slot::Reserve(0),
        Slot::Reserve(u16::MAX),
        now,
        ctx,
    );
    one || crossing || reserve
}

/// A death of `sim` is pending in this second. The slot is the signal: a death that was put
/// straight on the queue is not seen here, and a deferral must not pass it (class orders a
/// later micro after every death of the second).
fn death_due(domain: &DomainState, sim: SimId, now: SimTime, ctx: &SimCtx<'_, '_>) -> bool {
    range_due(domain, sim, Slot::Death(0), Slot::Death(u64::MAX), now, ctx)
}

fn defer(domain: &mut DomainState, a: SimId, b: SimId, ctx: &mut SimCtx<'_, '_>) {
    let by = ctx
        .planned_by()
        .unwrap_or_else(|| panic!("invariant talk.by"));
    let handle = ctx.schedule(ctx.now(), EventPayload::Talk { a, b }, by);
    domain.pending.insert((a, Slot::Talk(b)), handle);
    domain.pending.insert((b, Slot::Talk(a)), handle);
}

/// `sim` reached a sociable seat. For each household member already sitting there, in
/// ascending id, plan one Talk at `now` plus the delay when section 3 "Beginn" allows it.
pub(super) fn on_seated(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let Some(seat) = seat_of(domain, data, sim) else {
        return;
    };
    if !social.sociable(&seat.kind) {
        return;
    }
    let delay = i64::from(social.limits.talk_delay_seconds);
    let Some(at) = ctx.now().checked_add(SimDuration::from_secs(delay)) else {
        return;
    };
    let Some(members) = domain.members.get(&seat.household) else {
        return;
    };
    let others: Vec<SimId> = members
        .iter()
        .copied()
        .filter(|&other| other != sim)
        .filter(|&other| {
            seat_of(domain, data, other).is_some_and(|theirs| {
                theirs.household == seat.household && theirs.object == seat.object
            })
        })
        .collect();
    for other in others {
        let (a, b) = ordered(sim, other);
        if !may(domain, data, a, b, at, true) {
            continue;
        }
        let handle = ctx.schedule(at, EventPayload::Talk { a, b }, by);
        domain.pending.insert((a, Slot::Talk(b)), handle);
        domain.pending.insert((b, Slot::Talk(a)), handle);
    }
}

/// `sim` stops sitting now (ADR-0030 section 3, "Gleiche Sekunde" and "Abbruch"). A Talk due
/// this second runs first when the partner is still at the same seat and section 3 still
/// allows it, and plans no successor. A failed check writes nothing and plans nothing. The
/// event is cancelled either way. Every other open Talk of `sim` is cancelled. A partner
/// who is not there is not an orphan: the event simply does not run.
pub(super) fn on_stand_up(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let due: Vec<_> = talk_handles(domain, sim)
        .into_iter()
        .filter(|(_, handle)| ctx.pending_time(*handle) == Some(now))
        .collect();
    for (other, handle) in due {
        let (a, b) = ordered(sim, other);
        clear(domain, a, b);
        if let Some((object, kind)) = together(domain, data, a, b)
            && may(domain, data, a, b, now, false)
        {
            run(domain, data, (a, b), object, &kind, false, ctx);
        }
        ctx.cancel(handle);
    }
    for (other, handle) in talk_handles(domain, sim) {
        let (a, b) = ordered(sim, other);
        clear(domain, a, b);
        ctx.cancel(handle);
    }
}

/// The Talk event. An abort still pending in this second, and no death, defers it one micro
/// so the end runs it first. Otherwise the two must still sit together (`talk.orphan` if not).
/// Conditions that fail write nothing and plan nothing.
pub(crate) fn on_talk(
    domain: &mut DomainState,
    data: &Data,
    a: SimId,
    b: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if a >= b {
        panic!("invariant talk.pair");
    }
    let now = ctx.now();
    let abort = abort_due(domain, a, now, ctx) || abort_due(domain, b, now, ctx);
    let dying = death_due(domain, a, now, ctx) || death_due(domain, b, now, ctx);
    if abort && !dying {
        defer(domain, a, b, ctx);
        return;
    }
    clear(domain, a, b);
    let Some((object, kind)) = together(domain, data, a, b) else {
        panic!("invariant talk.orphan");
    };
    if !may(domain, data, a, b, now, false) {
        return;
    }
    run(domain, data, (a, b), object, &kind, true, ctx);
}

/// Drops every last-talk pair that names `sim`. The person may already be gone from the
/// sims (a death archives before the house forgets), so this does not look the household up.
pub(super) fn drop_last_talk(domain: &mut DomainState, sim: SimId) {
    for moved in domain.house.households.values_mut() {
        moved.last_talk.retain(|&(a, b), _| a != sim && b != sim);
    }
}

fn charge(total: &mut u16, n: u16) {
    *total = total
        .checked_add(n)
        .unwrap_or_else(|| panic!("invariant talk.entries"));
}

fn add_entries(domain: &mut DomainState, sim: SimId, day: i64, n: u16) {
    if n == 0 {
        return;
    }
    let person = person_mut(domain, sim);
    if person.talk_day != day {
        person.talk_day = day;
        person.talks = 0;
        person.entries = 0;
    }
    person.entries =
        (person.entries.checked_add(n)).unwrap_or_else(|| panic!("invariant talk.entries"));
}

fn add_talk(domain: &mut DomainState, sim: SimId, day: i64) {
    let person = person_mut(domain, sim);
    if person.talk_day != day {
        person.talk_day = day;
        person.talks = 0;
        person.entries = 0;
    }
    person.talks = person
        .talks
        .checked_add(1)
        .unwrap_or_else(|| panic!("invariant talk.talks"));
}

fn edge_at(
    domain: &DomainState,
    owner: SimId,
    other: SimId,
    now: SimTime,
    fade: (u16, u16),
) -> [u16; 6] {
    let (familiarity, grudge) = fade;
    (domain.house.social.bonds.get(&owner))
        .and_then(|list| list.iter().find(|bond| bond.other == other))
        .map(|bond| bond.values_at(now, familiarity, grudge))
        .unwrap_or([0; 6])
}

/// The strongest common memory: same `at` and kind, each list naming the other. The largest
/// sum of strengths, then the younger `at`, then the smaller kind.
fn shared_memory(
    domain: &DomainState,
    a: SimId,
    b: SimId,
    now: SimTime,
) -> Option<(Memory, Memory)> {
    let list_a = domain.house.social.memories.get(&a)?;
    let list_b = domain.house.social.memories.get(&b)?;
    let mut best: Option<(i32, i64, u16, Memory, Memory)> = None;
    for left in list_a {
        if !left.with.contains(&Some(b)) {
            continue;
        }
        for right in list_b {
            if right.at != left.at || right.kind != left.kind || !right.with.contains(&Some(a)) {
                continue;
            }
            let sum = i32::from(left.strength_at(now)) + i32::from(right.strength_at(now));
            let rank = (-sum, -left.at.as_secs(), left.kind);
            if best.as_ref().is_none_or(|old| rank < (old.0, old.1, old.2)) {
                best = Some((rank.0, rank.1, rank.2, *left, *right));
            }
        }
    }
    best.map(|(_, _, _, left, right)| (left, right))
}

fn change_bond(
    domain: &mut DomainState,
    owner: SimId,
    other: SimId,
    steps: &[(usize, i16)],
    now: SimTime,
    fade: (u16, u16),
) -> ([u16; 6], [u16; 6]) {
    let (familiarity, grudge) = fade;
    let list = (domain.house.social.bonds.get_mut(&owner))
        .unwrap_or_else(|| panic!("invariant talk.bond"));
    let bond = (list.iter_mut().find(|bond| bond.other == other))
        .unwrap_or_else(|| panic!("invariant talk.bond"));
    let before = bond.apply(now, familiarity, grudge, steps);
    (before, bond.values)
}

fn formed_edge(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
    cause: &mut Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<u16> {
    match form_one(domain, data, holder, other, cause.clone(), ctx) {
        Some(Formed::Wrote { last, entries }) => {
            *cause = Cause::Event(last);
            Some(entries)
        }
        // The same refusal `may` already applies. `form_one` wrote nothing.
        Some(Formed::Refused) => None,
        None => Some(0),
    }
}

fn earlier(cause: &Cause, ctx: &SimCtx<'_, '_>) -> EventId {
    match cause {
        Cause::Event(id) => *id,
        _ => ctx
            .planned_by()
            .unwrap_or_else(|| panic!("invariant talk.by")),
    }
}

fn recall(
    domain: &mut DomainState,
    owner: SimId,
    other: SimId,
    memory: Memory,
    step: u16,
    now: SimTime,
) {
    let strength = recall_strength(memory.strength_at(now), step);
    let Some(list) = domain.house.social.memories.get_mut(&owner) else {
        return;
    };
    let Some(found) = list.iter_mut().find(|item| {
        item.at == memory.at && item.kind == memory.kind && item.with.contains(&Some(other))
    }) else {
        return;
    };
    found.strength = strength;
    found.since = now;
}

/// Steps 0 to 8 of section 3 "Ablauf". `plan_next` is false when an end of this second runs
/// the talk: the two do not both stay, so no successor is planned.
pub(crate) fn run(
    domain: &mut DomainState,
    data: &Data,
    pair: (SimId, SimId),
    object: u16,
    kind: &DataId,
    plan_next: bool,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (a, b) = pair;
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant talk.social"));
    let home = same_home(domain, a, b).unwrap_or_else(|| panic!("invariant talk.house"));
    let now = ctx.now();
    let since = domain.house.households[&home].since;
    let day = house_day(now, since);
    // Both directions before either write, the same refusal `may` applies: one side
    // that cannot take the bond must not leave a bond on the other.
    if formation_refused(domain, data, a, b) || formation_refused(domain, data, b, a) {
        return;
    }
    let mut cause = ctx.trigger();
    let Some(mut entries_a) = formed_edge(domain, data, a, b, &mut cause, ctx) else {
        return;
    };
    let Some(mut entries_b) = formed_edge(domain, data, b, a, &mut cause, ctx) else {
        return;
    };
    let fade = (social.bonds.familiarity_fade, social.bonds.grudge_fade);
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let interest_a = interests(WorldSeed::new(seed), domain.key_of(a));
    let interest_b = interests(WorldSeed::new(seed), domain.key_of(b));
    let shared = shared_memory(domain, a, b, now);
    // Steps 0 and 2 do not change beliefs, so the material is the same before the topic.
    // A Tat outside the house-day window is inert, so it is not material (ADR-0031 section 1,
    // addendum 2026-11-11). The cap's closed day shortens that window.
    let seated = at_furniture(domain, home, object);
    let held_a = (domain.house.social.beliefs.get(&a))
        .cloned()
        .unwrap_or_default();
    let held_b = (domain.house.social.beliefs.get(&b))
        .cloned()
        .unwrap_or_default();
    let window = social.limits.deed_window_days;
    let dropped =
        |doer: SimId| (domain.house.social.deeds.get(&doer)).and_then(|deeds| deeds.dropped_day);
    let material_a = live_material(
        &gossip_material(a, &held_a, b, &held_b, &seated),
        day,
        window,
        dropped,
    );
    let material_b = live_material(
        &gossip_material(b, &held_b, a, &held_a, &seated),
        day,
        window,
        dropped,
    );
    // The reason is read before the topic's bond steps (an apology can change a grudge).
    // READING: the reference also skips the speaker. The ADR names only "not the hearer".
    let members: Vec<SimId> = (domain.members.get(&home))
        .map(|members| members.iter().copied().collect())
        .unwrap_or_default();
    let grudge_toward =
        |owner: SimId, other: SimId| edge_at(domain, owner, other, now, fade)[GRUDGE];
    let from = social.beliefs.lie_grudge;
    let reason_a = lie_reason(a, b, &members, |w| grudge_toward(a, w), from);
    let reason_b = lie_reason(b, a, &members, |w| grudge_toward(b, w), from);
    let gossip = !material_a.is_empty()
        || !material_b.is_empty()
        || reason_a.is_some()
        || reason_b.is_some();
    let view = TalkPair {
        a,
        b,
        age_a: years(domain, a, now),
        age_b: years(domain, b, now),
        edge_ab: edge_at(domain, a, b, now, fade),
        edge_ba: edge_at(domain, b, a, now, fade),
        interest_a: &interest_a,
        interest_b: &interest_b,
        common_memory: shared.is_some(),
        gossip,
    };
    let topic = choose_topic(
        social,
        &view,
        kind,
        WorldSeed::new(seed),
        domain.key_of(a),
        domain.key_of(b),
        now,
    );
    let actor = topic_actor(social, topic, &view);
    let outcomes = topic_outcome(social, topic, &view);
    for outcome in outcomes {
        let (before, after) = change_bond(
            domain,
            outcome.owner,
            outcome.other,
            &outcome.steps,
            now,
            fade,
        );
        if before == after {
            continue;
        }
        let id = ctx.emit(
            LogPayload::BondChanged {
                other: outcome.other,
                before,
                after,
            },
            vec![Subject::Sim(outcome.owner)],
            Causes::new(cause.clone()),
        );
        cause = Cause::Event(id);
        // Affection, familiarity, trust or a grudge moved. An idle choice weighs the first three.
        super::action::changed_for(domain, outcome.owner, id);
        if outcome.owner == a {
            charge(&mut entries_a, 1);
        } else {
            charge(&mut entries_b, 1);
        }
    }
    // What the talk makes the two feel, row by row; applied once each after `core:talked`.
    let rows = (topic_feelings(topic, &view, actor).into_iter())
        .map(|(who, occasion, band)| (who, Pending { occasion, band }))
        .collect();
    let mut charges = Charges {
        a,
        b,
        day,
        entries_a: &mut entries_a,
        entries_b: &mut entries_b,
        rows,
    };
    if topic == TEASE {
        let actor = actor.unwrap_or_else(|| panic!("invariant talk.actor"));
        let target = if actor == a { b } else { a };
        let teases = DataId::new("core:teases").unwrap_or_else(|_| panic!("invariant belief.deed"));
        let kind = deed_kind(data, &teases);
        belief::record_deed(domain, data, actor, kind, target, day, now);
        let statement = Statement::Tat {
            doer: actor,
            kind,
            other: target,
            day,
        };
        cause = offer_seen(
            domain,
            data,
            statement,
            (home, object),
            &mut charges,
            None,
            cause,
            ctx,
        );
    }
    if topic == GOSSIP
        && let Some(speaker) = choose_speaker(a, &material_a, reason_a, b, &material_b, reason_b)
    {
        let hearer = if speaker == a { b } else { a };
        let reason = if speaker == a { reason_a } else { reason_b };
        let material = if speaker == a {
            &material_a
        } else {
            &material_b
        };
        // No roll without a reason. A miss is not logged and is not the cause of a true telling.
        let lie = reason.and_then(|w| {
            let key = lie_draw_key(domain.key_of(speaker), now, domain.key_of(hearer));
            let u = talk_ppm(WorldSeed::new(seed), key);
            (u < u64::from(social.beliefs.lie_ppm)).then_some((w, key))
        });
        if let Some((w, key)) = lie {
            let (statement, certainty) = lie_statement(w, hearer, day, &social.beliefs);
            let said = Belief {
                statement,
                certainty,
                since: now,
                source: None,
                origin: None,
                basis: Basis::Gehoert,
                secret: false,
            };
            cause = pass_on(
                domain,
                data,
                &Pass {
                    speaker,
                    said,
                    place: (home, object),
                    day,
                    now,
                    fade,
                    lie: Some(key),
                },
                &mut charges,
                cause,
                ctx,
            );
        } else if let Some(said) = said_belief(
            material,
            WorldSeed::new(seed),
            domain.key_of(speaker),
            now,
            domain.key_of(hearer),
        ) {
            cause = pass_on(
                domain,
                data,
                &Pass {
                    speaker,
                    said,
                    place: (home, object),
                    day,
                    now,
                    fade,
                    lie: None,
                },
                &mut charges,
                cause,
                ctx,
            );
        }
    }
    let refuted = refute_one(
        domain,
        data,
        (a, b),
        day,
        now,
        fade,
        seed,
        &mut charges,
        &mut cause,
        ctx,
    );
    // Last use of the charges: the rows of the feelings leave them.
    let felt = core::mem::take(&mut charges.rows);
    let recall_topic =
        (social.topics.get(topic)).is_some_and(|row| row.id.as_str() == "core:recall");
    if recall_topic && let Some((left, right)) = shared {
        let step = social.memory.recall_step;
        recall(domain, a, b, left, step, now);
        recall(domain, b, a, right, step, now);
        let by = earlier(&cause, ctx);
        plan_forget(domain, a, by, ctx);
        plan_forget(domain, b, by, ctx);
    }
    let (cause, kept_a) = wrote_memory(domain, data, a, b, TALK, cause, ctx);
    charge(&mut entries_a, kept_a);
    let (mut cause, kept_b) = wrote_memory(domain, data, b, a, TALK, cause, ctx);
    charge(&mut entries_b, kept_b);
    if refuted {
        let (next, kept) = wrote_memory(domain, data, a, b, REFUTED, cause, ctx);
        charge(&mut entries_a, kept);
        cause = next;
        let (next, kept) = wrote_memory(domain, data, b, a, REFUTED, cause, ctx);
        charge(&mut entries_b, kept);
        cause = next;
    }
    let topic_u8 = u8::try_from(topic).unwrap_or_else(|_| panic!("invariant talk.topic"));
    let talked = ctx.emit(
        LogPayload::Talked {
            other: b,
            object,
            topic: topic_u8,
            actor,
        },
        vec![Subject::Sim(a), Subject::Sim(b)],
        Causes::new(cause),
    );
    charge(&mut entries_a, 1);
    charge(&mut entries_b, 1);
    // One `core:felt` per person after `core:talked`, from all its rows; it is not charged against
    // the talk's limits (ADR-0032 section 6).
    for sim in [a, b] {
        let own: Vec<Pending> = (felt.iter())
            .filter(|(who, _)| *who == sim)
            .map(|(_, row)| *row)
            .collect();
        if !own.is_empty() {
            let _ = feeling::feel(domain, data, sim, &own, Cause::Event(talked), ctx);
        }
    }
    add_entries(domain, a, day, entries_a);
    add_entries(domain, b, day, entries_b);
    add_talk(domain, a, day);
    add_talk(domain, b, day);
    domain
        .house
        .households
        .get_mut(&home)
        .unwrap_or_else(|| panic!("invariant talk.house"))
        .last_talk
        .insert((a, b), now);
    if !plan_next {
        return;
    }
    let gap = i64::from(social.limits.pair_gap_seconds);
    let Some(at) = now.checked_add(SimDuration::from_secs(gap)) else {
        return;
    };
    if together(domain, data, a, b).is_none() || !may(domain, data, a, b, at, false) {
        return;
    }
    let handle = ctx.schedule(at, EventPayload::Talk { a, b }, talked);
    domain.pending.insert((a, Slot::Talk(b)), handle);
    domain.pending.insert((b, Slot::Talk(a)), handle);
}

/// The two participants' running totals for this talk. A third person is charged through
/// [`add_entries`] at once, because their total is not written at the end of [`run`].
struct Charges<'a> {
    a: SimId,
    b: SimId,
    day: i64,
    entries_a: &'a mut u16,
    entries_b: &'a mut u16,
    /// The rows of the feelings the talk has brought so far, by the person who feels them
    /// (ADR-0032 section 3).
    rows: Vec<(SimId, Pending)>,
}

impl Charges<'_> {
    fn add(&mut self, who: SimId, n: u16) {
        if who == self.a {
            charge(self.entries_a, n);
        } else if who == self.b {
            charge(self.entries_b, n);
        }
    }
}

/// One said statement and where it is said. `lie` is the draw that made it a lie.
struct Pass {
    speaker: SimId,
    said: Belief,
    place: (HouseholdId, u16),
    day: i64,
    now: SimTime,
    fade: (u16, u16),
    lie: Option<DrawKey>,
}

/// The deed number of `id` in `beliefs.deeds`.
fn deed_kind(data: &Data, id: &DataId) -> u16 {
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let at = (social.beliefs.deeds.iter())
        .position(|kind| kind == id)
        .unwrap_or_else(|| panic!("invariant belief.deed"));
    u16::try_from(at).unwrap_or_else(|_| panic!("invariant belief.deed"))
}

/// Everyone at `object` in `household`, ascending [`SimId`], of any age.
fn at_furniture(domain: &DomainState, household: HouseholdId, object: u16) -> Vec<SimId> {
    let mut ids = Vec::new();
    for (id, person) in &domain.house.sims {
        let same = (domain.sims.get(id)).is_some_and(|sim| sim.household == household);
        let seated = matches!(person.place, Place::Room(_))
            && person.action.is_some_and(|action| action.object == object);
        if same && seated {
            ids.push(*id);
        }
    }
    ids
}

fn holds_bond(domain: &DomainState, owner: SimId, other: SimId) -> bool {
    (domain.house.social.bonds.get(&owner))
        .is_some_and(|list| list.iter().any(|bond| bond.other == other))
}

/// Trust of `holder` in `other` after the topic's bond steps. An existing edge is read at
/// `now`. A missing edge uses the role's start value and is not formed.
///
/// READING (ADR-0031 section 2): the start value is the fallback. `edge_at` would report 0
/// for a missing edge, which is not that start value.
fn trust_in(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
    now: SimTime,
    fade: (u16, u16),
) -> u16 {
    if holds_bond(domain, holder, other) {
        edge_at(domain, holder, other, now, fade)[TRUST]
    } else {
        super::bond::start_trust(domain, data, holder, other)
    }
}

/// Everyone of `min_age` or older whom [`audience`] names sees `statement`. A third person
/// needs two free places of the day's 34, even when the write is one entry (ADR-0030
/// section 5). The two participants are not third persons: their 16 free places were
/// required to plan the talk. Only entries actually written are charged. `sentence_hearer`
/// is the hearer of the sentence when `statement` is the seen `lästert` of that telling,
/// and `None` for a `neckt`.
#[allow(clippy::too_many_arguments)]
fn offer_seen(
    domain: &mut DomainState,
    data: &Data,
    statement: Statement,
    place: (HouseholdId, u16),
    charges: &mut Charges<'_>,
    sentence_hearer: Option<SimId>,
    mut cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Cause {
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let min_age = u32::from(social.limits.min_age);
    let now = ctx.now();
    let (household, object) = place;
    let Statement::Tat {
        doer, kind, other, ..
    } = statement
    else {
        panic!("invariant talk.seen");
    };
    let seated = at_furniture(domain, household, object);
    for holder in audience(kind, doer, other, &seated, sentence_hearer) {
        if years(domain, holder, now) < min_age {
            continue;
        }
        let third = holder != charges.a && holder != charges.b;
        if third {
            let (_, used) = counts(domain, holder, charges.day);
            let free = social.limits.entries_per_day.saturating_sub(used);
            if free < 2 {
                continue;
            }
        }
        match belief::store_seen(domain, data, holder, statement, cause.clone(), ctx) {
            belief::Store::Unchanged => {}
            belief::Store::Wrote { last, entries } => {
                cause = Cause::Event(last);
                if third {
                    add_entries(domain, holder, charges.day, entries);
                } else {
                    charges.add(holder, entries);
                }
            }
        }
    }
    cause
}

/// The biological father, living or archived. Founders have none (ADR-0031 section 5).
fn true_father(domain: &DomainState, child: SimId) -> Option<SimId> {
    let parents = (domain.sims.get(&child).and_then(|person| person.parents))
        .or_else(|| domain.archive.get(&child).and_then(|person| person.parents));
    parents.map(|parents| parents.father)
}

/// Step 5 (ADR-0031 section 5), after the topic. At most one refutation: first `a`, then
/// `b`, the smallest key, and one draw. A miss writes nothing. The belief is replaced in
/// place (`core:refuted`, not `core:believed`). Trust in the old source falls by the data
/// step; grudge rises as well when that source is the origin. No source, no step, and no
/// new edge. Returns whether it landed, so step 6 can write "Widerlegung".
///
/// READING: the reference adds the grudge whenever origin and source are the same person,
/// including a `VaterVon` that had a source. The checked `VaterVon` has none.
#[allow(clippy::too_many_arguments)]
fn refute_one(
    domain: &mut DomainState,
    data: &Data,
    pair: (SimId, SimId),
    day: i64,
    now: SimTime,
    fade: (u16, u16),
    seed: u64,
    charges: &mut Charges<'_>,
    cause: &mut Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> bool {
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let (a, b) = pair;
    let beliefs_a = (domain.house.social.beliefs.get(&a))
        .cloned()
        .unwrap_or_default();
    let beliefs_b = (domain.house.social.beliefs.get(&b))
        .cloned()
        .unwrap_or_default();
    // The alleged doer is often neither participant. A missing list is empty: inside the
    // window that person did not do it, and outside it the day is not known false.
    let deeds = domain.house.social.deeds.clone();
    let Some(pick) = pick_refutation(
        a,
        &beliefs_a,
        b,
        &beliefs_b,
        |sim| deeds.get(&sim).cloned().unwrap_or_default(),
        |child| true_father(domain, child),
        day,
        social.limits.deed_window_days,
    ) else {
        return false;
    };
    let key = refute_draw_key(domain.key_of(pick.refuter), now, domain.key_of(pick.hearer));
    let u = talk_ppm(WorldSeed::new(seed), key);
    if u >= u64::from(social.beliefs.refute_ppm) {
        return false;
    }
    let held = if pick.hearer == a {
        &beliefs_a
    } else {
        &beliefs_b
    };
    let old = (held.iter().copied())
        .find(|belief| belief.key() == pick.key)
        .unwrap_or_else(|| panic!("invariant belief.key"));
    let true_man = match old.statement {
        Statement::VaterVon { child, .. } => true_father(domain, child),
        Statement::Tat { .. } => None,
    };
    let effect = refute(old, pick.refuter, true_man, now, &social.beliefs);
    let statement = effect.belief.statement;
    belief::store_refuted(domain, pick.hearer, effect.belief);
    let id = ctx.emit(
        LogPayload::Refuted {
            by: pick.refuter,
            statement,
            source: old.source,
        },
        vec![Subject::Sim(pick.hearer)],
        Causes::new(Cause::Random(DrawRef::from(key))),
    );
    charges.add(pick.hearer, 1);
    *cause = Cause::Event(id);
    // The hearer's feeling: the band is read toward the source, none without one (ADR-0032
    // section 3).
    let row = Pending {
        occasion: Occasion::Refuted,
        band: old.source,
    };
    charges.rows.push((pick.hearer, row));
    if let Some((source, steps)) = effect.toward_source
        && holds_bond(domain, pick.hearer, source)
    {
        let (before, after) = change_bond(domain, pick.hearer, source, &steps, now, fade);
        if before != after {
            let id = ctx.emit(
                LogPayload::BondChanged {
                    other: source,
                    before,
                    after,
                },
                vec![Subject::Sim(pick.hearer)],
                Causes::new(cause.clone()),
            );
            *cause = Cause::Event(id);
            charges.add(pick.hearer, 1);
            super::action::changed_for(domain, pick.hearer, id);
        }
    }
    true
}

/// Step 4 (ADR-0030 section 3, ADR-0031 sections 3 and 4): `core:told`, the hearer's belief,
/// one bond step when the belief was stored and was not a confirmation, then the speaker's
/// deed. A lie records `lügt` toward the hearer (not seen) and says `Tat(W, lästert, hearer,
/// today)` without the speaker adopting it. Either way the seen deed is `lästert` of the
/// speaker about the doer of the said statement, for everyone old enough at the furniture
/// except the speaker and except the hearer of this sentence.
///
/// READING: the bond step is on the hearer's edge to the doer, not to the speaker. If that
/// edge does not exist, none is formed and no `core:bond_changed` is written. The first entry
/// after a lie draw names the draw. Later entries of the step name the previous entry, so the
/// hearer's `core:bond_changed` stays caused by `core:believed` (ADR-0031 section 5, Meiden).
fn pass_on(
    domain: &mut DomainState,
    data: &Data,
    pass: &Pass,
    charges: &mut Charges<'_>,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Cause {
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let hearer = if pass.speaker == charges.a {
        charges.b
    } else {
        charges.a
    };
    let told_cause = match pass.lie {
        Some(key) => Causes::new(Cause::Random(DrawRef::from(key))),
        None => Causes::new(cause),
    };
    let told = ctx.emit(
        LogPayload::Told {
            to: hearer,
            statement: pass.said.statement,
            certainty: pass.said.certainty,
            lie: pass.lie.is_some(),
        },
        vec![Subject::Sim(pass.speaker)],
        told_cause,
    );
    let mut cause = Cause::Event(told);
    charges.add(pass.speaker, 1);
    let trust = trust_in(domain, data, hearer, pass.speaker, pass.now, pass.fade);
    // A lie has no speaker belief, so the hearer's origin is the speaker.
    let speaker_belief = if pass.lie.is_some() {
        None
    } else {
        Some(pass.said)
    };
    match belief::store_heard(
        domain,
        data,
        hearer,
        Heard {
            incoming: pass.said.statement,
            speaker: pass.speaker,
            speaker_certainty: pass.said.certainty,
            speaker_belief,
            trust,
        },
        cause.clone(),
        ctx,
    ) {
        HeardStore::Unchanged => {}
        HeardStore::Confirmed { last, entries } => {
            cause = Cause::Event(last);
            charges.add(hearer, entries);
        }
        HeardStore::Stored { last, entries } => {
            cause = Cause::Event(last);
            charges.add(hearer, entries);
            // A deed of gossip or a lie against the hearer angers and saddens it, with the band
            // read toward the one it is about (ADR-0032 section 3).
            if let Some(doer) = aimed_doer(hearer, pass.said.statement) {
                let row = Pending {
                    occasion: Occasion::Heard,
                    band: Some(doer),
                };
                charges.rows.push((hearer, row));
            }
            if let Some(steps) = heard_bond_steps(hearer, pass.said.statement, &social.beliefs)
                && let Some(doer) = pass.said.statement.doer()
                && holds_bond(domain, hearer, doer)
            {
                let (before, after) =
                    change_bond(domain, hearer, doer, &steps, pass.now, pass.fade);
                if before != after {
                    let id = ctx.emit(
                        LogPayload::BondChanged {
                            other: doer,
                            before,
                            after,
                        },
                        vec![Subject::Sim(hearer)],
                        Causes::new(cause),
                    );
                    cause = Cause::Event(id);
                    charges.add(hearer, 1);
                    // A lie, a telling-on or an avoidance: trust and sometimes a grudge.
                    super::action::changed_for(domain, hearer, id);
                }
            }
        }
    }
    let Statement::Tat { doer, .. } = pass.said.statement else {
        panic!("invariant talk.gossip");
    };
    if pass.lie.is_some() {
        let lies = DataId::new("core:lies").unwrap_or_else(|_| panic!("invariant belief.deed"));
        let lying = deed_kind(data, &lies);
        belief::record_deed(
            domain,
            data,
            pass.speaker,
            lying,
            hearer,
            pass.day,
            pass.now,
        );
    }
    let gossips = DataId::new("core:gossips").unwrap_or_else(|_| panic!("invariant belief.deed"));
    let kind = deed_kind(data, &gossips);
    belief::record_deed(domain, data, pass.speaker, kind, doer, pass.day, pass.now);
    let statement = Statement::Tat {
        doer: pass.speaker,
        kind,
        other: doer,
        day: pass.day,
    };
    offer_seen(
        domain,
        data,
        statement,
        pass.place,
        charges,
        Some(hearer),
        cause,
        ctx,
    )
}

/// The memory `kind` for `owner`, about `other`. Returns the cause of the next entry and
/// how many entries named `owner`.
fn wrote_memory(
    domain: &mut DomainState,
    data: &Data,
    owner: SimId,
    other: SimId,
    kind: u16,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> (Cause, u16) {
    let with = others(Some(other), None);
    match remember(domain, data, owner, (kind, with), cause.clone(), ctx) {
        Some(WroteMemory::Kept(id)) => (Cause::Event(id), 1),
        Some(WroteMemory::Displaced(id)) => (Cause::Event(id), 2),
        None => (cause, 0),
    }
}

/// An open Talk of a loaded queue: `a` smaller, both seated at one sociable object of their
/// household. One event per pair is the caller's check.
pub(crate) fn open_talk_ok(loaded: &LoadedState, data: &Data, a: SimId, b: SimId) -> bool {
    if a >= b {
        return false;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let (Some(left), Some(right)) = (loaded.house.sims.get(&a), loaded.house.sims.get(&b)) else {
        return false;
    };
    let Some(home) = loaded.sims.get(&a).map(|person| person.household) else {
        return false;
    };
    let Some(other) = loaded.sims.get(&b).map(|person| person.household) else {
        return false;
    };
    if home != other || !loaded.house.households.contains_key(&home) {
        return false;
    }
    let (Place::Room(_), Some(seat_a)) = (left.place, left.action) else {
        return false;
    };
    let (Place::Room(_), Some(seat_b)) = (right.place, right.action) else {
        return false;
    };
    if seat_a.object != seat_b.object {
        return false;
    }
    let Some(houses) = data.houses() else {
        return false;
    };
    let Some(moved) = loaded.house.households.get(&home) else {
        return false;
    };
    let Some(house) = houses.house(&moved.house) else {
        return false;
    };
    let Some(object) = house.def().objects.get(usize::from(seat_a.object)) else {
        return false;
    };
    social.sociable(&object.kind)
}

/// Day counters at or under their caps, and last-talk pairs of the household at or before the
/// clock. A stamp from another day is kept: it reads as zero. Without social data every
/// counter is zero and no pair is stored.
///
/// READING: "unter ihren Grenzen" on a reachable state is at most the cap. A talk that used
/// the last place sits at the cap until the next house day.
pub(crate) fn counters_ok(loaded: &LoadedState, data: &Data) -> bool {
    let Some(social) = data.social() else {
        let clear =
            (loaded.house.sims.values()).all(|person| person.talks == 0 && person.entries == 0);
        let pairs = (loaded.house.households.values()).all(|moved| moved.last_talk.is_empty());
        return clear && pairs;
    };
    let over = |talks: u16, entries: u16| {
        talks > social.limits.talks_per_day || entries > social.limits.entries_per_day
    };
    if (loaded.house.sims.values()).any(|person| over(person.talks, person.entries)) {
        return false;
    }
    for (home, moved) in &loaded.house.households {
        for (&(a, b), &when) in &moved.last_talk {
            if a >= b || when > loaded.closed_through {
                return false;
            }
            let member =
                |id: SimId| (loaded.sims.get(&id)).is_some_and(|person| person.household == *home);
            if !member(a) || !member(b) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::bond::{Listed, listed, listed_larger_full};
    use super::{formation_refused, may};
    use crate::time::SimTime;

    /// The cap in ADR-0030 section 2. Both are 30, the home is one household, and nothing
    /// else in section 3 blocks a talk at second 0.
    fn at() -> SimTime {
        SimTime::from_secs(0)
    }

    fn refused(case: &mut Listed) -> bool {
        let (holder, newcomer) = (case.holder, case.newcomer);
        formation_refused(&mut case.domain, &case.data, holder, newcomer)
    }

    /// A missing bond a full protected list cannot take is not a talk, in either order
    /// (ADR-0030 section 2 and section 3).
    #[test]
    fn a_full_list_of_protected_bonds_may_not_talk_either_way() {
        let mut case = listed(32, false);
        assert!(refused(&mut case));
        let (holder, newcomer) = (case.holder, case.newcomer);
        assert!(!may(
            &mut case.domain,
            &case.data,
            holder,
            newcomer,
            at(),
            true
        ));
        assert!(!may(
            &mut case.domain,
            &case.data,
            newcomer,
            holder,
            at(),
            true
        ));
    }

    /// One droppable bond, or a list under the cap, is not a refusal. The smaller id first
    /// may talk. The larger id first is the pair order, which is never a talk.
    #[test]
    fn a_list_that_can_take_the_bond_may_talk() {
        for (bonds, drop_last) in [(32, true), (31, false)] {
            let mut case = listed(bonds, drop_last);
            assert!(!refused(&mut case), "{bonds} bonds, drop_last {drop_last}");
            let (holder, newcomer) = (case.holder, case.newcomer);
            assert!(
                may(&mut case.domain, &case.data, holder, newcomer, at(), true),
                "{bonds} bonds, drop_last {drop_last}"
            );
            assert!(!may(
                &mut case.domain,
                &case.data,
                newcomer,
                holder,
                at(),
                true
            ));
        }
    }

    /// The larger id's list is full and protected. The smaller id has room and no bond to
    /// the larger one. Either call order is refused (ADR-0030 sections 2 and 3).
    #[test]
    fn a_full_list_on_the_larger_id_may_not_talk() {
        let mut case = listed_larger_full();
        let (larger, smaller) = (case.holder, case.newcomer);
        assert!(smaller < larger);
        assert!(formation_refused(
            &mut case.domain,
            &case.data,
            larger,
            smaller
        ));
        assert!(!formation_refused(
            &mut case.domain,
            &case.data,
            smaller,
            larger
        ));
        assert!(!may(
            &mut case.domain,
            &case.data,
            smaller,
            larger,
            at(),
            true
        ));
        assert!(!may(
            &mut case.domain,
            &case.data,
            larger,
            smaller,
            at(),
            true
        ));
    }
}
