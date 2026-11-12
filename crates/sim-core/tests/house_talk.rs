//! A talk at sociable furniture (ADR-0030 section 3). The expected seconds, topics and bond
//! steps are the reference's and the data's: a walk inside one room is 5 s (ADR-0025), the
//! talk is planned 600 s after the later arrival, and the next one 3600 s after a talk that
//! took place. Recall of Erinnern is the unit test `a_recall_adds_300_and_stops_at_1000`:
//! no interest pin of `test:social` selects that topic, and a bond cannot be planted here.

mod common;

use common::day::{self, MORNING, Run};
use serde_json::json;
use sim_core::canonical::PROJECTION_VERSION;
use sim_core::data_id::{Code, DataId};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{Command, Input, MoveIn, MoveOut};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, DrawRef, LogEntry, LogPayload, Subject};
use sim_core::mortality::accident_age;
use sim_core::rng::{DrawKey, Draws, WorldSeed, tags};
use sim_core::social::belief::{
    Basis, Belief, DeedAnswer, Deeds, LAESTERT, LUEGT, NECKT, Statement, gossip_material,
    live_material,
};
use sim_core::social::bond::{GRUDGE, TRUST};
use sim_core::social::data::{REFUTED, TALK};
use sim_core::social::talk::{house_day, lie_draw_key, refute_draw_key, talk_ppm};
use sim_core::time::{SimDuration, SimTime, YEAR};
use sim_core::version::CORE_VERSION;
use sim_core::world::{EventPayload, FORMAT_VERSION, LoadError, World, YearOut};

/// Inside one room (ADR-0025). Both people are put back in the hall, where every object is.
const WITHIN_ROOM: i64 = 5;
/// `limits.talk_delay_seconds`.
const TALK_DELAY: i64 = 600;
/// `limits.pair_gap_seconds`. A gap of exactly this passes; one second less does not.
const PAIR_GAP: i64 = 3600;

const DINING: u16 = 0;
const FRIDGE: u16 = 1;
const TOILET: u16 = 2;

/// A meal of `meal` seconds at a two-place dining table, the other essential kinds, and a bed
/// with one place per person of the house. Everything stands in the hall, so a command from
/// the entrance walks `WITHIN_ROOM` seconds. The fridge, toilet and shower have four places,
/// so a command is not refused because a morning walk still holds the one place.
fn data(meal: u32, people: u8) -> sim_core::data::Data {
    scene(
        meal,
        people,
        2,
        60,
        include_str!("../../../data/house/day.json"),
        1e-9,
        60,
    )
}

/// As `data`, with `dining_slots` at the table, a fridge offer of `fridge_secs`, and the day
/// file `day`. `accident_male` is his accident per year; hers stays at 1e-9.
fn scene(
    meal: u32,
    people: u8,
    dining_slots: u8,
    fridge_secs: u32,
    day: &str,
    accident_male: f64,
    dwell: u32,
) -> sim_core::data::Data {
    let kinds = format!(
        r#"[
      {{"id": "core:dining_table", "slots": {dining_slots}, "offers": [
        {{"id": "eat", "rates": {{"hunger": 1}}, "seconds": {meal}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:fridge", "slots": 4, "offers": [
        {{"id": "snack", "rates": {{"hunger": 1}}, "seconds": {fridge_secs}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:toilet", "slots": 4, "offers": [
        {{"id": "use", "rates": {{"bladder": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:shower", "slots": 4, "offers": [
        {{"id": "shower", "rates": {{"hygiene": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:sofa", "slots": 1, "offers": [
        {{"id": "sit", "rates": {{"fun": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:bed", "slots": {people}, "offers": [
        {{"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}}]}}
    ]"#
    );
    let house = format!(
        r#"{{
      "id": "test:talk", "capacity": {people},
      "rooms": [{{"id": "hall", "entrance": true}}, {{"id": "den"}}],
      "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
      "doors": [{{"a": "hall", "b": "den", "seconds": 1}}],
      "objects": [
        {{"kind": "core:dining_table", "room": "hall"}},
        {{"kind": "core:fridge", "room": "hall"}},
        {{"kind": "core:toilet", "room": "hall"}},
        {{"kind": "core:shower", "room": "hall"}},
        {{"kind": "core:sofa", "room": "hall"}},
        {{"kind": "core:bed", "room": "hall"}}]
    }}"#
    );
    let houses = common::houses_from(&day::rates(1, 0, 0), day, &kinds, &[&house]);
    let (mut calibration, genome_map) = common::death_files(1e-9, None, serde_json::json!([]));
    calibration["values"]["core:accident_male"]["value"] = serde_json::json!(accident_male);
    let definitions = common::definitions_with(common::demography(), calibration, genome_map)
        .expect("definitions");
    sim_core::data::Data::new(definitions)
        .expect("checked")
        .with_houses(houses)
        .with_social(common::repository_social())
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("id")
}

fn command(id: u64, object: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer: 0,
    })
}

fn year() -> i64 {
    YEAR.as_secs()
}

/// The couple, founders 0 and 1 of `test:social`, moved in at 23:59:59 of day 0.
fn couple(meal: u32) -> (Run, HouseholdId) {
    let born = [-30 * year(), -32 * year()];
    day::Run::family(data(meal, 2), 0x5EED, "test:social", &born, 0)
}

/// Founders 0 and 1, and two children (founders 2 and 3) taken into their household.
/// An adult outside the couple would found a household of their own, and the Alltag pin is
/// the pair of founders 1 and 3.
fn four() -> (Run, HouseholdId) {
    let born = [-30 * year(), -32 * year(), -10 * year(), -12 * year()];
    day::Run::family(data(7_200, 4), 0x5EED, "test:social", &born, 0)
}

/// Three founders and meals of `meal` seconds at a three-place table. School is only the age
/// of 6 and work only the age of 7, so the child of 10 and the adults have no departure and
/// stay through the afternoon. The repository day would send the child out at 07:40.
fn without_departure(meal: u32) -> sim_core::data::Data {
    let repository = include_str!("../../../data/house/day.json");
    let day = repository
        .replace(
            r#""school": {"from_age": 6, "until_age": 18"#,
            r#""school": {"from_age": 6, "until_age": 7"#,
        )
        .replace(
            "\"from_age\": 18,\n    \"until_age\": 67,",
            "\"from_age\": 7,\n    \"until_age\": 8,",
        );
    assert_ne!(day, repository, "the ages in the day file still bind");
    scene(meal, 3, 3, meal, &day, 1e-9, 60)
}

/// Commands `seats` at 06:00:01, after the morning's own walks have begun, and runs to the
/// arrival. Returns that second. The walk is the 5 s inside the hall.
fn seat(run: &mut Run, seats: &[(u64, u16)]) -> i64 {
    arrive(run, MORNING, seats)
}

/// As `seat`, measured from `morning` instead of the first morning.
fn arrive(run: &mut Run, morning: i64, seats: &[(u64, u16)]) -> i64 {
    run.advance(morning);
    let now = morning + 1;
    for &(id, object) in seats {
        run.submit(command(id, object));
    }
    let arrival = now + WITHIN_ROOM;
    run.advance(arrival);
    for &(id, object) in seats {
        let started = (run.log.iter().rev())
            .find_map(|entry| match entry.payload {
                LogPayload::ActionStarted {
                    object: got,
                    walk_secs,
                    commanded: true,
                    ..
                } if entry.subjects == vec![Subject::Sim(sim(id))] => {
                    Some((entry.t.as_secs(), walk_secs, got))
                }
                _ => None,
            })
            .expect("the commanded walk");
        let walk = u32::try_from(WITHIN_ROOM).expect("fits");
        assert_eq!(started, (now, walk, object));
        let reached = (run.log.iter()).any(|entry| {
            entry.t == t(arrival)
                && entry.subjects == vec![Subject::Sim(sim(id))]
                && matches!(entry.payload, LogPayload::Reached { object: got } if got == object)
        });
        assert!(reached, "sim {id} reaches {object} at {arrival}");
    }
    arrival
}

fn talked(run: &Run) -> Vec<&LogEntry> {
    (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Talked { .. }))
        .collect()
}

fn bond(run: &Run, owner: u64, other: u64) -> [u16; 6] {
    (run.world.state().house().social.bonds[&sim(owner)])
        .iter()
        .find(|bond| bond.other == sim(other))
        .expect("the bond")
        .values
}

fn memory_about(run: &Run, owner: u64, other: u64, at: i64) -> bool {
    (run.world.state().house().social.memories.get(&sim(owner))).is_some_and(|list| {
        list.iter().any(|memory| {
            memory.kind == TALK
                && memory.at == t(at)
                && memory.since == t(at)
                && memory.strength == 600
                && memory.fade == 60
                && memory.valence == 200
                && memory.with == [Some(sim(other)), None]
        })
    })
}

fn counts(run: &Run, id: u64) -> (u16, u16) {
    let person = &run.world.state().house().sims[&sim(id)];
    (person.talks, person.entries)
}

fn belief(run: &Run, owner: u64, statement: Statement) -> Belief {
    let list = &run.world.state().house().social.beliefs[&sim(owner)];
    *list
        .iter()
        .find(|belief| belief.statement == statement)
        .expect("belief")
}

fn neckt(doer: u64, other: u64, day: i64) -> Statement {
    Statement::Tat {
        doer: sim(doer),
        kind: NECKT,
        other: sim(other),
        day,
    }
}

fn laestert(doer: u64, other: u64, day: i64) -> Statement {
    Statement::Tat {
        doer: sim(doer),
        kind: LAESTERT,
        other: sim(other),
        day,
    }
}

/// Move-in at 20:00 is house day 0 through 05:59 the next calendar day, and day 1 at 06:00.
#[test]
fn a_move_in_at_20_00_starts_house_day_0() {
    let mut run = Run::founded(
        data(7_200, 2),
        0x5EED,
        "test:social",
        &[-30 * year(), -32 * year()],
    );
    let home = run.together(2).expect("their household");
    run.advance(20 * 3600 - 1);
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(20 * 3600));
    let day = |at: i64| sim_core::social::talk::house_day(t(at), since);
    assert_eq!(day(20 * 3600), 0);
    assert_eq!(day(6 * 3600 + 86_400 - 1), 0);
    assert_eq!(day(6 * 3600 + 86_400), 1);
    assert_eq!(day(6 * 3600 + 2 * 86_400), 2);
}

/// Founders 0 and 1 of `test:social` tease: the actor is founder 0, and the other's grudge
/// toward them rises by 60. Both stay, so the next talk is one hour later and adds another 60.
#[test]
fn partners_tease_and_talk_again_an_hour_later() {
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(first)));
    run.advance(first);
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 1);
    assert_eq!(spoken[0].t, t(first));
    assert_eq!(
        spoken[0].subjects,
        vec![Subject::Sim(sim(1)), Subject::Sim(sim(2))]
    );
    assert_eq!(
        spoken[0].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 2,
            actor: Some(sim(1)),
        }
    );
    assert!(memory_about(&run, 1, 2, first));
    assert!(memory_about(&run, 2, 1, first));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 60);
    assert_eq!(bond(&run, 1, 2)[GRUDGE], 0);
    // The first Necken of the day also writes one `core:believed` for each of the two.
    assert_eq!(counts(&run, 1), (1, 3));
    assert_eq!(counts(&run, 2), (1, 4));
    let since = run.world.state().house().households[&home].since;
    let day = sim_core::social::talk::house_day(t(first), since);
    assert_eq!(day, 1);
    let seen = neckt(1, 2, day);
    for owner in [1, 2] {
        let held = belief(&run, owner, seen);
        assert_eq!(held.certainty, 1000);
        assert_eq!(held.basis, Basis::Gesehen);
        assert_eq!(held.source, None);
        assert_eq!(held.origin, None);
        assert!(!held.secret);
        assert_eq!(held.since, t(first));
    }
    let deeds = &run.world.state().house().social.deeds[&sim(1)];
    assert_eq!(deeds.items.len(), 1);
    assert_eq!(deeds.items[0].kind, NECKT);
    assert_eq!(deeds.items[0].other, sim(2));
    assert_eq!(deeds.items[0].day, day);
    assert!(!run.world.state().house().social.deeds.contains_key(&sim(2)));
    assert_eq!(run.world.state().house().sims[&sim(1)].talk_day, day);
    assert_eq!(
        run.world.state().house().households[&home].last_talk[&(sim(1), sim(2))],
        t(first)
    );
    let second = first + PAIR_GAP;
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(second)));
    run.advance(second);
    assert_eq!(talked(&run).len(), 2);
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 120);
    // The same statement is already seen, so the second Necken adds no belief. The deed is
    // still appended.
    assert_eq!(counts(&run, 1), (2, 5));
    assert_eq!(counts(&run, 2), (2, 7));
    assert_eq!(
        run.world.state().house().social.deeds[&sim(1)].items.len(),
        2
    );
    assert_eq!(run.world.state().house().social.beliefs[&sim(1)].len(), 1);
    assert_eq!(
        run.world.talk_at(sim(1), sim(2)),
        Some(t(second + PAIR_GAP))
    );
}

/// Founders 1 and 3, housemates, take Alltag. Familiarity starts at 200. From the move-in at
/// 23:59:59 (86 399) to the talk at 06:10:06 (108 606) is 22 207 s, so it falls by
/// `5 * 22 207 / 86 400` = 1, then the topic adds 40.
#[test]
fn housemates_take_alltag_from_the_interest_pins() {
    let (mut run, home) = four();
    let arrival = seat(
        &mut run,
        &[(1, FRIDGE), (3, TOILET), (2, DINING), (4, DINING)],
    );
    let at = arrival + TALK_DELAY;
    assert_eq!(run.world.talk_at(sim(2), sim(4)), Some(t(at)));
    run.advance(at);
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 1);
    assert_eq!(
        spoken[0].payload,
        LogPayload::Talked {
            other: sim(4),
            object: DINING,
            topic: 0,
            actor: None,
        }
    );
    assert_eq!(bond(&run, 2, 4)[4], 239);
    assert_eq!(bond(&run, 4, 2)[4], 239);
    assert_eq!(counts(&run, 2), (1, 3));
    assert_eq!(counts(&run, 4), (1, 3));
    assert!(memory_about(&run, 2, 4, at));
    assert!(memory_about(&run, 4, 2, at));
    assert_eq!(
        run.world.state().house().households[&home].last_talk[&(sim(2), sim(4))],
        t(at)
    );
    assert_eq!(run.world.talk_at(sim(2), sim(4)), Some(t(at + PAIR_GAP)));
}

/// The smaller person's action ends in the talk's second, so that end runs the talk and no
/// successor is planned. The talk's entries are written before `core:action_ended`.
#[test]
fn an_action_end_in_the_same_second_runs_the_talk() {
    let (mut run, _) = couple(600);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let at = arrival + TALK_DELAY;
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(at)));
    run.advance(at);
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 1);
    assert_eq!(spoken[0].t, t(at));
    assert_eq!(counts(&run, 1), (1, 3));
    assert_eq!(counts(&run, 2), (1, 4));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 60);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    let order: Vec<_> = (run.log.iter())
        .filter(|entry| entry.t == t(at))
        .map(|entry| matches!(entry.payload, LogPayload::Talked { .. }))
        .collect();
    let talked_at = order.iter().position(|is_talk| *is_talk).expect("talked");
    let ended_at = (run.log.iter())
        .filter(|entry| entry.t == t(at))
        .position(|entry| matches!(entry.payload, LogPayload::ActionEnded { .. }))
        .expect("the action ended");
    assert!(talked_at < ended_at);
}

/// Five Necken leave him 16 entries, so 18 of the day's 34 are free and the next talk is
/// planned (ADR-0030 sections 3 and 5). It is due in the second the meals end. The child
/// sits down first. She teases the child, and he, at the same table with two free places,
/// sees that deed. The child then teases him. He has 13 free, below the 16 a talk needs, so
/// that end does not run the couple's talk: the counts, the grudge and the last talk stay,
/// and nothing is planned. The end still cancels the event.
///
/// Day 0 move-in is 86 399 and the first morning is 108 000. The walk starts at 108 001
/// and reaches the table at 108 006. Talks fall at 108 606 and every 3 600 s: the fifth at
/// 123 006, the sixth at 126 606. The meal is 18 600 s, and 108 006 + 18 600 = 126 606.
/// The child is sent at 123 007, arrives at 123 012 and talks at 123 612; those successors
/// are at 127 212, after the meal. The first Necken of a statement writes one `core:believed`
/// besides the memory and the talk, and the target's bond step. A repeat does not. She acts
/// in the five couple talks (11) and teases the child (14), then sees the child's tease (15).
/// He is the target of the five (16), sees her tease (17) and is the child's target (21).
/// The child writes 4, then 3. His grudge toward her is 5 * 60 = 300; one hour fades none
/// (`10 * 3 600 / 86 400` = 0). The house day at 126 606 is 1, so the counters do not reset.
#[test]
fn an_action_end_in_the_same_second_skips_a_talk_without_sixteen_free_entries() {
    const MEAL: u32 = 18_600;
    let born = [-30 * year(), -32 * year(), -10 * year()];
    let (mut run, home) =
        day::Run::family(without_departure(MEAL), 0x5EED, "test:social", &born, 0);
    let arrival = arrive(&mut run, MORNING, &[(1, DINING), (2, DINING), (3, FRIDGE)]);
    let first = arrival + TALK_DELAY;
    let fifth = first + 4 * PAIR_GAP;
    let sixth = fifth + PAIR_GAP;
    assert_eq!(arrival, 108_006);
    assert_eq!(fifth, 123_006);
    assert_eq!(sixth, 126_606);
    assert_eq!(sixth, arrival + i64::from(MEAL));
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(86_399));
    assert_eq!(sim_core::social::talk::house_day(t(sixth), since), 1);

    run.advance(fifth);
    assert_eq!(talked(&run).len(), 5);
    assert_eq!(counts(&run, 1), (5, 11));
    assert_eq!(counts(&run, 2), (5, 16));
    assert_eq!(counts(&run, 3), (0, 0));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 300);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(sixth)));

    run.submit(command(3, DINING));
    let child_arrival = fifth + 1 + WITHIN_ROOM;
    let child_talk = child_arrival + TALK_DELAY;
    assert_eq!(child_arrival, 123_012);
    assert_eq!(child_talk, 123_612);
    assert!(child_talk + PAIR_GAP > sixth);
    run.advance(child_talk);
    let joined = (run.log.iter().rev())
        .find_map(|entry| match entry.payload {
            LogPayload::ActionStarted {
                object: got,
                walk_secs,
                commanded: true,
                ..
            } if entry.subjects == vec![Subject::Sim(sim(3))] => {
                Some((entry.t.as_secs(), walk_secs, got))
            }
            _ => None,
        })
        .expect("the child walks to the table");
    let walk = u32::try_from(WITHIN_ROOM).expect("fits");
    assert_eq!(joined, (fifth + 1, walk, DINING));
    assert!(
        talked(&run)
            .iter()
            .all(|entry| { matches!(entry.payload, LogPayload::Talked { topic: 2, .. }) })
    );
    assert_eq!(talked(&run).len(), 7);
    assert_eq!(counts(&run, 1), (6, 15));
    assert_eq!(counts(&run, 2), (6, 21));
    assert_eq!(counts(&run, 3), (2, 7));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 300);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(sixth)));
    let watched = neckt(1, 3, 1);
    let held = belief(&run, 2, watched);
    assert_eq!(held.certainty, 1000);
    assert_eq!(held.basis, Basis::Gesehen);
    assert_eq!(held.source, None);
    assert_eq!(held.origin, None);
    assert!(!held.secret);

    run.advance(sixth);
    assert_eq!(counts(&run, 1), (6, 15));
    assert_eq!(counts(&run, 2), (6, 21));
    assert_eq!(counts(&run, 3), (2, 7));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 300);
    assert_eq!(talked(&run).len(), 7);
    assert!(talked(&run).iter().all(|entry| entry.t != t(sixth)));
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    assert_eq!(
        run.world.state().house().households[&home].last_talk[&(sim(1), sim(2))],
        t(fifth)
    );
    assert!((run.log.iter()).any(|entry| {
        entry.t == t(sixth) && matches!(entry.payload, LogPayload::ActionEnded { .. })
    }));
}

/// The accident budget of founder 1 of `test:social` at seed `0x5EED`, the same draw `Start`
/// uses for `core:accident`.
fn accident_budget() -> f64 {
    let tag = DataId::new("test:social").expect("tag");
    let lineage = LineageKey::founder(&tag, 1);
    Draws::new(WorldSeed::new(0x5EED)).exponential(
        DrawKey {
            tag: tags::DEATH_BUDGET,
            subject: lineage.into(),
            occasion: Code::new("core:accident").expect("code").key(),
            detail: 0,
        },
        1.0,
    )
}

/// Seconds from the start until an accident at age 32 with this budget and rate. The 32 years
/// he already has at second 0 cancel (`mortality::accident_age`).
fn accident_due(budget: f64, rate: f64) -> i64 {
    let years = accident_age(32.0, budget, rate) - 32.0;
    SimDuration::from_years_ceil(years).expect("due").as_secs()
}

/// His accident per year so the death is due at `at`. `at - 0.5` is inside the second `ceil`
/// lands on. The rate stays inside the accident range, and a JSON number does not move it.
fn male_accident_at(at: i64) -> f64 {
    let budget = accident_budget();
    let half_second = SimDuration::from_secs(1).as_years() / 2.0;
    let span = SimDuration::from_secs(at).as_years() - half_second;
    let rate = budget / span;
    assert!(
        (1e-9..=1000.0).contains(&rate),
        "the rate {rate} is outside the accident range"
    );
    let stored = serde_json::json!(rate);
    let loaded = stored.as_f64().expect("a finite rate");
    assert_eq!(accident_due(budget, loaded), at, "the stored rate");
    rate
}

/// A death in the talk's second is later than the talk. The talk counts; its successor does not
/// survive the death. His accident is due in that second (`Slot::Death`); class runs the talk
/// first (ADR-0002). The meal is still open.
#[test]
fn a_death_in_the_same_second_comes_after_the_talk() {
    let at = MORNING + 1 + WITHIN_ROOM + TALK_DELAY;
    let rate = male_accident_at(at);
    let born = [-30 * year(), -32 * year()];
    let (mut run, _) = day::Run::family(
        scene(
            7_200,
            2,
            2,
            60,
            include_str!("../../../data/house/day.json"),
            rate,
            60,
        ),
        0x5EED,
        "test:social",
        &born,
        0,
    );
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    assert_eq!(arrival + TALK_DELAY, at);
    let loaded = run.world.data().genetics().accident_male;
    assert_eq!(
        accident_due(accident_budget(), loaded),
        at,
        "the loaded rate"
    );
    run.advance(at);
    assert_eq!(talked(&run).len(), 1);
    assert_eq!(counts(&run, 1), (1, 3));
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    assert!(!run.world.state().house().sims.contains_key(&sim(2)));
    let social = &run.world.state().house().social;
    assert!(social.beliefs.contains_key(&sim(1)));
    assert!(social.deeds.contains_key(&sim(1)));
    assert!(!social.beliefs.contains_key(&sim(2)));
    assert!(!social.deeds.contains_key(&sim(2)));
    assert_eq!(run.world.state().living(), 1);
}

/// Moving out in the talk's second cancels it before it runs. It writes nothing.
#[test]
fn moving_out_at_the_talk_cancels_it() {
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let at = arrival + TALK_DELAY;
    run.advance(at - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    run.advance(at);
    assert!(talked(&run).is_empty());
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
}

/// Moving out after a Necken keeps the beliefs and the deed. The two stay alive and leave
/// the house part (ADR-0031 section 1).
#[test]
fn moving_out_freezes_beliefs_and_deeds() {
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    let since = run.world.state().house().households[&home].since;
    let day = sim_core::social::talk::house_day(t(first), since);
    let seen = neckt(1, 2, day);
    assert_eq!(belief(&run, 1, seen).certainty, 1000);
    assert_eq!(
        run.world.state().house().social.deeds[&sim(1)].items.len(),
        1
    );
    run.submit(Input::MoveOut(MoveOut { household: home }));
    run.advance(first + 1);
    assert_eq!(run.world.state().living(), 2);
    assert!(!run.world.state().house().sims.contains_key(&sim(1)));
    assert!(!run.world.state().house().sims.contains_key(&sim(2)));
    let held = belief(&run, 1, seen);
    assert_eq!(held.certainty, 1000);
    assert_eq!(held.basis, Basis::Gesehen);
    assert_eq!(belief(&run, 2, seen).basis, Basis::Gesehen);
    let deeds = &run.world.state().house().social.deeds[&sim(1)];
    assert_eq!(deeds.items.len(), 1);
    assert_eq!(deeds.items[0].kind, NECKT);
    assert_eq!(deeds.items[0].other, sim(2));
    assert_eq!(deeds.items[0].day, day);
}

/// A seen deed passes on as Klatsch once the teaser has left the table. Founder 0 is 60, so
/// Necken is closed and Klatsch wins. The hearer's trust in the speaker is the partner start
/// 700, so a seen 1000 is heard at `floor(1000 * (500 + 700) / 1500)` = 800. The bond step
/// is −20 trust on the hearer's edge to the doer. That edge is a housemate's (founders have
/// no parents), start trust 450, so it is 430. `core:told` carries the speaker's certainty.
#[test]
fn a_seen_deed_is_passed_on_with_the_heard_certainty_and_the_bond_step() {
    let born = [-60 * year(), -32 * year(), -10 * year()];
    let (mut run, home) =
        day::Run::family(without_departure(7_200), 0x5EED, "test:social", &born, 0);
    let arrival = arrive(&mut run, MORNING, &[(1, FRIDGE), (2, DINING), (3, DINING)]);
    let first = arrival + TALK_DELAY;
    assert_eq!(first, 108_606);
    run.advance(first);
    let since = run.world.state().house().households[&home].since;
    let day = sim_core::social::talk::house_day(t(first), since);
    assert_eq!(day, 1);
    let teased = neckt(3, 2, day);
    assert_eq!(belief(&run, 2, teased).certainty, 1000);
    assert_eq!(belief(&run, 3, teased).basis, Basis::Gesehen);
    assert!(
        !run.world
            .state()
            .house()
            .social
            .beliefs
            .contains_key(&sim(1))
    );
    assert_eq!(counts(&run, 3), (1, 3));
    assert_eq!(counts(&run, 2), (1, 4));
    assert_eq!(counts(&run, 1), (0, 0));

    run.submit(command(3, FRIDGE));
    run.submit(command(1, DINING));
    let back = first + 1 + WITHIN_ROOM;
    let gossip_at = back + TALK_DELAY;
    assert_eq!(back, 108_612);
    assert_eq!(gossip_at, 109_212);
    run.advance(gossip_at);
    assert_eq!(run.world.talk_at(sim(2), sim(3)), None);
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 2);
    assert_eq!(
        spoken[1].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    assert_eq!(spoken[1].t, t(gossip_at));
    assert!(run.log.iter().any(|entry| {
        entry.t == t(gossip_at)
            && entry.subjects == vec![Subject::Sim(sim(2))]
            && matches!(
                entry.payload,
                LogPayload::Told {
                    to,
                    statement,
                    certainty: 1000,
                    lie: false,
                } if to == sim(1) && statement == teased
            )
    }));
    let heard = belief(&run, 1, teased);
    assert_eq!(heard.certainty, 800);
    assert_eq!(heard.basis, Basis::Gehoert);
    assert_eq!(heard.source, Some(sim(2)));
    assert_eq!(heard.origin, Some(sim(2)));
    assert_eq!(heard.since, t(gossip_at));
    assert!(!heard.secret);
    assert_eq!(bond(&run, 1, 3)[TRUST], 430);
    let passed = laestert(2, 3, day);
    for owner in [1_u64, 2] {
        let held = belief(&run, owner, passed);
        assert_eq!(held.certainty, 1000);
        assert_eq!(held.basis, Basis::Gesehen);
        assert_eq!(held.source, None);
        assert_eq!(held.origin, None);
        assert!(!held.secret);
    }
    assert!(
        run.world.state().house().social.beliefs[&sim(3)]
            .iter()
            .all(|held| held.statement != passed)
    );
    let deeds = &run.world.state().house().social.deeds[&sim(2)];
    assert_eq!(deeds.items.len(), 1);
    assert_eq!(deeds.items[0].kind, LAESTERT);
    assert_eq!(deeds.items[0].other, sim(3));
    assert_eq!(deeds.items[0].day, day);
    assert_eq!(
        run.world.state().house().social.deeds[&sim(3)].items[0].kind,
        NECKT
    );
    assert!(!run.world.state().house().social.deeds.contains_key(&sim(1)));
    assert_eq!(counts(&run, 3), (1, 3));
    assert_eq!(counts(&run, 2), (2, 8));
    assert_eq!(counts(&run, 1), (1, 5));
    assert_eq!(
        run.world.talk_at(sim(1), sim(2)),
        Some(t(gossip_at + PAIR_GAP))
    );
}

/// Standing up the second before the talk cancels it. A cancelled talk does not count.
#[test]
fn standing_up_before_the_talk_cancels_it() {
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let at = arrival + TALK_DELAY;
    run.advance(at - 2);
    run.submit(command(1, FRIDGE));
    run.advance(at);
    assert!(talked(&run).is_empty());
    assert_eq!(counts(&run, 1), (0, 0));
    assert_eq!(counts(&run, 2), (0, 0));
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    assert!(
        run.world.state().house().households[&home]
            .last_talk
            .is_empty()
    );
}

fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("bytes")
}

/// Replaces the one occurrence of `from` with `to`. Both encodings have the same width.
fn spliced(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len(), "the forgery keeps the width");
    let hits = bytes
        .windows(from.len())
        .filter(|window| *window == from)
        .count();
    assert_eq!(hits, 1, "one occurrence");
    let at = bytes
        .windows(from.len())
        .position(|window| window == from)
        .expect("found");
    let mut forged = bytes.to_vec();
    forged[at..at + to.len()].copy_from_slice(to);
    forged
}

/// A same-width second after `since`: the largest time whose postcard width matches it.
fn later_same_width(since: i64) -> i64 {
    let width = encode(&t(since)).len();
    let max_zig = (1u128 << (7 * width)) - 1;
    let later = i64::try_from(max_zig / 2).expect("fits");
    assert!(later > since);
    assert_eq!(encode(&t(later)).len(), width);
    later
}

/// The core and projection version bytes. A postcard varint below 128 is the number.
fn version_at(bytes: &[u8]) -> (usize, usize) {
    let rest = bytes.strip_prefix(b"SIMAGENT").expect("magic");
    let (format, rest) = postcard::take_from_bytes::<u32>(rest).expect("format");
    assert_eq!(format, FORMAT_VERSION);
    assert_eq!(
        FORMAT_VERSION, 2,
        "the talk state is fields the format already holds"
    );
    let core_at = bytes.len() - rest.len();
    let (core, rest) = postcard::take_from_bytes::<u32>(rest).expect("core");
    assert_eq!(core, CORE_VERSION);
    const {
        assert!(CORE_VERSION < 128, "the version byte is the number");
    }
    assert_eq!(
        bytes[core_at],
        u8::try_from(CORE_VERSION).expect("one byte")
    );
    let (_, rest) = postcard::take_from_bytes::<u64>(rest).expect("hash");
    let proj_at = bytes.len() - rest.len();
    let (projection, _) = postcard::take_from_bytes::<u32>(rest).expect("projection");
    assert_eq!(projection, PROJECTION_VERSION);
    const {
        assert!(PROJECTION_VERSION < 128, "the version byte is the number");
    }
    assert_eq!(
        bytes[proj_at],
        u8::try_from(PROJECTION_VERSION).expect("one byte")
    );
    (core_at, proj_at)
}

/// A save between two talks loads and runs on to the same log. The loader refuses day
/// counters over their caps (the cap itself is a reachable state), a last-talk pair out of
/// order, after the clock or of someone outside the household, a Talk whose smaller id is
/// not first, and a Talk of someone who is not there. A save of core 66 or projection 22
/// is refused. The format stays 2.
#[test]
fn a_save_between_two_talks_loads_and_an_old_one_does_not() {
    let data = data(7_200, 2);
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    let between = first + 1;
    let second = first + PAIR_GAP;
    run.advance(between);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(second)));
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let person = run.world.state().house().sims[&sim(1)].clone();
    let moved = run.world.state().house().households[&home].clone();
    let (mut loaded, hashes) = World::load(&bytes, data.clone()).expect("loaded");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    assert_eq!(loaded.talk_at(sim(1), sim(2)), Some(t(second)));
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, t(second), &mut |_| false);
    run.advance(second);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    let tail: Vec<&LogEntry> = run
        .log
        .iter()
        .filter(|entry| entry.t > t(between))
        .collect();
    assert_eq!(
        serde_json::to_string(&loaded.take_log()).expect("json"),
        serde_json::to_string(&tail).expect("json")
    );

    let load = |bytes: &[u8]| World::load(bytes, data.clone()).err();
    let limits = data.social().expect("social").limits.clone();
    assert_eq!(person.talks, 1);
    assert!(person.entries < limits.entries_per_day);

    let mut over_talks = person.clone();
    over_talks.talks = limits.talks_per_day + 1;
    assert_eq!(
        load(&spliced(&bytes, &encode(&person), &encode(&over_talks))),
        Some(LoadError::Inconsistent)
    );
    let mut at_talks = person.clone();
    at_talks.talks = limits.talks_per_day;
    assert!(load(&spliced(&bytes, &encode(&person), &encode(&at_talks))).is_none());

    let mut over_entries = person.clone();
    over_entries.entries = limits.entries_per_day + 1;
    assert_eq!(
        load(&spliced(&bytes, &encode(&person), &encode(&over_entries))),
        Some(LoadError::Inconsistent)
    );
    let mut at_entries = person.clone();
    at_entries.entries = limits.entries_per_day;
    assert!(load(&spliced(&bytes, &encode(&person), &encode(&at_entries))).is_none());

    let when = moved.last_talk[&(sim(1), sim(2))];
    let mut reversed = moved.clone();
    reversed.last_talk.clear();
    reversed.last_talk.insert((sim(2), sim(1)), when);
    assert_eq!(
        load(&spliced(&bytes, &encode(&moved), &encode(&reversed))),
        Some(LoadError::Inconsistent)
    );
    let mut late = moved.clone();
    late.last_talk
        .insert((sim(1), sim(2)), t(later_same_width(when.as_secs())));
    assert_eq!(
        load(&spliced(&bytes, &encode(&moved), &encode(&late))),
        Some(LoadError::Inconsistent)
    );
    let mut outsider = moved.clone();
    outsider.last_talk.clear();
    outsider.last_talk.insert((sim(1), sim(50)), when);
    assert_eq!(
        load(&spliced(&bytes, &encode(&moved), &encode(&outsider))),
        Some(LoadError::Inconsistent)
    );

    let spoken = EventPayload::Talk {
        a: sim(1),
        b: sim(2),
    };
    let swapped = EventPayload::Talk {
        a: sim(2),
        b: sim(1),
    };
    assert_eq!(
        load(&spliced(&bytes, &encode(&spoken), &encode(&swapped))),
        Some(LoadError::Inconsistent)
    );
    let absent = EventPayload::Talk {
        a: sim(1),
        b: sim(3),
    };
    assert_eq!(
        load(&spliced(&bytes, &encode(&spoken), &encode(&absent))),
        Some(LoadError::Inconsistent)
    );

    let (core_at, proj_at) = version_at(&bytes);
    let mut old_core = bytes.clone();
    old_core[core_at] = 0x42;
    assert_eq!(load(&old_core), Some(LoadError::ForeignCoreVersion));
    let mut old_projection = bytes.clone();
    old_projection[proj_at] = 0x16;
    assert_eq!(
        load(&old_projection),
        Some(LoadError::ForeignProjectionVersion)
    );
    assert!(load(&bytes).is_none());
}

fn refuse(data: &sim_core::data::Data, bytes: &[u8], from: &[u8], to: &[u8]) {
    assert_eq!(
        World::load(&spliced(bytes, from, to), data.clone()).err(),
        Some(LoadError::Inconsistent)
    );
}

/// The two deeds of the second Necken stay in the save and run on. A day that goes
/// backwards, a negative day, a kind the data does not list and an unknown other are
/// refused (ADR-0031 section 7). Certainty 900 still loads. Certainty 1001, `widerlegt`
/// at 500 and `secret` on a `Tat` do not.
#[test]
fn loaded_deeds_and_beliefs_keep_the_section_7_checks() {
    let data = data(7_200, 2);
    let (mut run, _) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let second = arrival + TALK_DELAY + PAIR_GAP;
    run.advance(second);
    let deeds = run.world.state().house().social.deeds[&sim(1)].clone();
    assert_eq!(deeds.items.len(), 2);
    assert_eq!(deeds.items[0].day, 1);
    assert_eq!(deeds.items[1].day, 1);
    let beliefs = run.world.state().house().social.beliefs[&sim(1)].clone();
    assert_eq!(beliefs.len(), 1);
    assert_eq!(beliefs[0].certainty, 1000);
    assert_eq!(beliefs[0].basis, Basis::Gesehen);
    assert!(!beliefs[0].secret);
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, _) = World::load(&bytes, data.clone()).expect("loaded");
    assert_eq!(loaded.state().house().social.deeds[&sim(1)].items.len(), 2);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());

    let one = encode(&deeds.items[0]);
    let pair = [one.clone(), one.clone()].concat();
    let mut earlier = deeds.items[1];
    earlier.day = 0;
    refuse(
        &data,
        &bytes,
        &pair,
        &[one.clone(), encode(&earlier)].concat(),
    );
    let mut negative = deeds.items[0];
    negative.day = -1;
    refuse(
        &data,
        &bytes,
        &pair,
        &[encode(&negative), one.clone()].concat(),
    );
    let mut bad_kind = deeds.items[1];
    bad_kind.kind = 3;
    refuse(
        &data,
        &bytes,
        &pair,
        &[one.clone(), encode(&bad_kind)].concat(),
    );
    let mut unknown = deeds.items[1];
    unknown.other = sim(50);
    refuse(
        &data,
        &bytes,
        &pair,
        &[one.clone(), encode(&unknown)].concat(),
    );
    // House day 1: day 2 is after today, so the save is refused (ADR-0031 section 7).
    let mut future = deeds.items[1];
    future.day = 2;
    refuse(&data, &bytes, &pair, &[one, encode(&future)].concat());

    let owned = [encode(&sim(1)), encode(&beliefs)].concat();
    let mut sure = beliefs.clone();
    sure[0].certainty = 900;
    assert!(
        World::load(
            &spliced(&bytes, &owned, &[encode(&sim(1)), encode(&sure)].concat()),
            data.clone()
        )
        .is_ok()
    );
    let mut over = beliefs.clone();
    over[0].certainty = 1001;
    refuse(
        &data,
        &bytes,
        &owned,
        &[encode(&sim(1)), encode(&over)].concat(),
    );
    let mut refuted = beliefs.clone();
    refuted[0].certainty = 500;
    refuted[0].basis = Basis::Widerlegt;
    refuse(
        &data,
        &bytes,
        &owned,
        &[encode(&sim(1)), encode(&refuted)].concat(),
    );
    let mut secret = beliefs.clone();
    secret[0].secret = true;
    refuse(
        &data,
        &bytes,
        &owned,
        &[encode(&sim(1)), encode(&secret)].concat(),
    );
}

/// Two beliefs on one person: swapping them, or giving them one key, is refused. A child
/// under 6 holds neither list (ADR-0031 section 7).
#[test]
fn a_child_under_six_holds_no_belief_and_two_beliefs_stay_sorted() {
    let quiet = without_departure(7_200);
    let born = [-60 * year(), -32 * year(), -10 * year()];
    let (mut run, _) = day::Run::family(quiet.clone(), 0x5EED, "test:social", &born, 0);
    let arrival = arrive(&mut run, MORNING, &[(1, FRIDGE), (2, DINING), (3, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    run.submit(command(3, FRIDGE));
    run.submit(command(1, DINING));
    let gossip_at = first + 1 + WITHIN_ROOM + TALK_DELAY;
    run.advance(gossip_at);
    let beliefs = run.world.state().house().social.beliefs.clone();
    let heard = &beliefs[&sim(1)];
    assert_eq!(heard.len(), 2);
    assert!(heard[0].key() < heard[1].key());
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    assert!(World::load(&bytes, quiet.clone()).is_ok());

    let mut swapped = heard.clone();
    swapped.swap(0, 1);
    let owned = [encode(&sim(1)), encode(heard)].concat();
    refuse(
        &quiet,
        &bytes,
        &owned,
        &[encode(&sim(1)), encode(&swapped)].concat(),
    );
    let mut duplicated = heard.clone();
    duplicated[1].statement = duplicated[0].statement;
    assert_eq!(encode(heard).len(), encode(&duplicated).len());
    refuse(
        &quiet,
        &bytes,
        &owned,
        &[encode(&sim(1)), encode(&duplicated)].concat(),
    );

    let young = data(7_200, 3);
    let (mut run, _) = day::Run::family(
        young.clone(),
        0x5EED,
        "test:social",
        &[-30 * year(), -32 * year(), -3 * year()],
        0,
    );
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    run.advance(arrival + TALK_DELAY);
    let now = run.world.closed_through().as_secs() + 1;
    let born = run.world.state().born(sim(3)).expect("the child");
    let age = (now - born.as_secs()).div_euclid(year());
    assert!(age < 6);
    assert!(
        !run.world
            .state()
            .house()
            .social
            .beliefs
            .contains_key(&sim(3))
    );
    assert!(
        run.world
            .state()
            .house()
            .social
            .beliefs
            .contains_key(&sim(2))
    );
    let beliefs = run.world.state().house().social.beliefs.clone();
    let mut forged = beliefs.clone();
    let list = forged.remove(&sim(2)).expect("an adult");
    forged.insert(sim(3), list);
    assert_eq!(encode(&beliefs).len(), encode(&forged).len());
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    assert!(World::load(&bytes, young.clone()).is_ok());
    refuse(&young, &bytes, &encode(&beliefs), &encode(&forged));

    let deeds = run.world.state().house().social.deeds.clone();
    assert!(!deeds.contains_key(&sim(3)));
    let holder = [1_u64, 2]
        .into_iter()
        .find(|id| deeds.contains_key(&sim(*id)))
        .expect("a deed");
    let mut forged_deeds = deeds.clone();
    let own = forged_deeds.remove(&sim(holder)).expect("the teaser");
    forged_deeds.insert(sim(3), own);
    assert_eq!(encode(&deeds).len(), encode(&forged_deeds).len());
    refuse(&young, &bytes, &encode(&deeds), &encode(&forged_deeds));
}

/// Whether `owner` still lists a deed of house day `day`.
fn holds_day(run: &Run, owner: u64, day: i64) -> bool {
    (run.world.state().house().social.deeds.get(&sim(owner)))
        .is_some_and(|deeds| deeds.items.iter().any(|deed| deed.day == day))
}

/// The 06:00 at which the house day counted from `since` becomes `day`.
fn morning_of(since: SimTime, day: i64) -> i64 {
    let base = (since.as_secs() - 6 * 3600).div_euclid(86_400);
    21_600 + (day + base) * 86_400
}

/// One tease, then at most one talk a day, so the cap of 64 never closes a day. Every offer
/// lasts twelve hours and nobody leaves for work, so those thirty house days stay inside the
/// decision budget. On the morning 29 house days later the deed is still known. On the morning
/// 30 house days later it is still stored and inert: not known, and not material for gossip
/// (ADR-0031 section 1, addendum 2026-11-11). That morning writes no talk. `dropped_day` stays
/// empty. Sitting down again records the next deed, and that record cuts the old day, still
/// without closing one. A save from the earlier morning, with that day rewritten to an older
/// one, loads. A day after that house day is refused.
#[test]
fn a_deed_stays_inert_after_thirty_house_days_and_is_cut_on_the_next_record() {
    let social = common::repository_social_with(|file| {
        file["limits"]["talks_per_day"] = serde_json::json!(1);
    });
    let repository = include_str!("../../../data/house/day.json");
    let day = repository.replace(
        "\"from_age\": 18,\n    \"until_age\": 67,",
        "\"from_age\": 66,\n    \"until_age\": 67,",
    );
    assert_ne!(day, repository, "the ages in the day file still bind");
    let built = scene(43_200, 2, 2, 43_200, &day, 1e-9, 43_200).with_social(social);
    let born = [-30 * year(), -32 * year()];
    let (mut run, home) = day::Run::family(built.clone(), 0x5EED, "test:social", &born, 0);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    let since = run.world.state().house().households[&home].since;
    let deed_day = house_day(t(first), since);
    assert_eq!(deed_day, 1);
    let owner = [1_u64, 2]
        .into_iter()
        .find(|id| holds_day(&run, *id, deed_day))
        .expect("the teaser");
    let keep = morning_of(since, deed_day + 29);
    let fall = morning_of(since, deed_day + 30);
    assert_eq!(house_day(t(keep), since), deed_day + 29);
    assert_eq!(house_day(t(fall), since), deed_day + 30);
    assert_eq!(house_day(t(keep - 1), since), deed_day + 28);

    run.advance(keep);
    assert!(
        holds_day(&run, owner, deed_day),
        "still inside on the earlier morning"
    );
    assert_eq!(
        house_day(t(run.world.closed_through().as_secs()), since),
        deed_day + 29
    );
    let window = built.social().expect("social").limits.deed_window_days;
    assert_eq!(window, 30, "ADR-0031 section 1");
    let inside = deed_day + 29;
    let record = run.world.state().house().social.deeds[&sim(owner)].clone();
    assert_eq!(record.dropped_day, None, "the cap has not closed a day");
    let old = *record
        .items
        .iter()
        .find(|deed| deed.day == deed_day)
        .expect("the deed");
    assert_eq!(
        record.knows(old.kind, old.other, deed_day, inside, window),
        DeedAnswer::Done,
        "the day and the 29 before it are still known"
    );
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    // A single deed is a few bytes and occurs more than once. The owner's list does not.
    let mut older = record.clone();
    let item = older
        .items
        .iter_mut()
        .find(|deed| deed.day == deed_day)
        .expect("the deed");
    item.day = deed_day - 1;
    assert!(item.day >= 0);
    let owned = [encode(&sim(owner)), encode(&record)].concat();
    let aged = [encode(&sim(owner)), encode(&older)].concat();
    assert_eq!(owned.len(), aged.len());
    let (loaded, _) =
        World::load(&spliced(&bytes, &owned, &aged), built.clone()).expect("an older day loads");
    assert!(
        loaded.state().house().social.deeds[&sim(owner)]
            .items
            .iter()
            .any(|deed| deed.day == deed_day - 1)
    );
    let mut ahead = record.clone();
    let last = ahead.items.last_mut().expect("a deed");
    last.day = inside + 1;
    let future = [encode(&sim(owner)), encode(&ahead)].concat();
    assert_eq!(owned.len(), future.len());
    refuse(&built, &bytes, &owned, &future);

    run.advance(fall - 1);
    assert!(
        holds_day(&run, owner, deed_day),
        "the day before the morning still holds it"
    );
    let talks = talked(&run).len();
    run.advance(fall);
    assert_eq!(talked(&run).len(), talks, "the morning writes no talk");
    let today = deed_day + 30;
    assert_eq!(house_day(t(fall), since), today);
    assert!(
        holds_day(&run, owner, deed_day),
        "thirty house days later it is still stored"
    );
    let inert = run.world.state().house().social.deeds[&sim(owner)].clone();
    assert_eq!(inert.dropped_day, None);
    assert_eq!(
        inert.knows(old.kind, old.other, deed_day, today, window),
        DeedAnswer::Unknown,
        "outside the window it is not known"
    );
    let statement = Statement::Tat {
        doer: sim(owner),
        kind: old.kind,
        other: old.other,
        day: deed_day,
    };
    let held = &run.world.state().house().social.beliefs[&old.other];
    let raw = gossip_material(old.other, held, sim(9), &[], &[]);
    assert!(
        raw.iter().any(|belief| belief.statement == statement),
        "the seen deed would be material for someone else"
    );
    let live = live_material(&raw, today, window, |_| inert.dropped_day);
    assert!(
        live.iter().all(|belief| belief.statement != statement),
        "outside the window it is not material"
    );

    // The same seat as the first Necken. Founders 0 and 1 still take topic 2: the talk memory
    // has faded, so Erinnern is closed, and the interest sums are unchanged.
    let again = arrive(&mut run, fall, &[(1, DINING), (2, DINING)]);
    let spoken_at = again + TALK_DELAY;
    assert_eq!(house_day(t(spoken_at), since), today);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(spoken_at)));
    run.advance(spoken_at);
    let spoken = talked(&run);
    assert!(spoken.len() > talks, "they talk again");
    assert_eq!(
        spoken.last().expect("the talk").payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 2,
            actor: Some(sim(owner)),
        }
    );
    assert!(
        !holds_day(&run, owner, deed_day),
        "the next recorded deed cuts it"
    );
    assert!(holds_day(&run, owner, today), "the new deed is today's");
    let left = &run.world.state().house().social.deeds[&sim(owner)];
    assert_eq!(
        left.dropped_day, None,
        "the calendar cut does not close a day"
    );
}

/// A frozen deed of house day 1 is after the new stay's house day 0, so the next move-in
/// cuts it (ADR-0031 section 1, addendum 2026-11-11). It is still there while the household
/// is outside.
#[test]
fn moving_in_again_judges_a_frozen_deed_against_the_new_house_day() {
    let (mut run, home) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    let since = run.world.state().house().households[&home].since;
    assert_eq!(house_day(t(first), since), 1);
    assert!(holds_day(&run, 1, 1));
    run.submit(Input::MoveOut(MoveOut { household: home }));
    run.advance(first + 86_400);
    assert!(
        holds_day(&run, 1, 1),
        "frozen while the household is outside"
    );
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let again = run.world.state().house().households[&home].since;
    assert_eq!(house_day(again, again), 0);
    assert!(!holds_day(&run, 1, 1), "day 1 is after the new house day");
}

/// A tease on the move-in day is house day 0. Moving out keeps it. Moving in a year later
/// cuts every deed of that stay, day 0 included, because the new stay counts from 0 again
/// (ADR-0031 section 1 and the addendum of 2026-11-11). A move-in touches the lists, so the
/// cut writes no entry. The belief that names day 0 stays, and that day of the new stay is
/// known not to have been done. She is 50, so her fertility factor is already 0 and the year
/// outside bears no child.
#[test]
fn a_move_in_day_tease_is_cut_when_the_household_moves_in_again() {
    let born = [-50 * year(), -52 * year()];
    let mut run = Run::founded(data(7_200, 2), 0x5EED, "test:social", &born);
    let home = run.together(2).expect("their household");
    let morning = 6 * 3600;
    run.advance(morning - 1);
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(morning);
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(morning));
    let arrival = arrive(&mut run, morning, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    assert_eq!(house_day(t(first), since), 0);
    run.advance(first);
    assert!(holds_day(&run, 1, 0), "the actor's deed is house day 0");
    let seen = neckt(1, 2, 0);
    for owner in [1_u64, 2] {
        assert_eq!(belief(&run, owner, seen).basis, Basis::Gesehen);
    }
    run.submit(Input::MoveOut(MoveOut { household: home }));
    assert!(holds_day(&run, 1, 0), "moving out freezes the deed");
    let moved_out = first + 1;
    run.advance(moved_out + year() - 1);
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let again = run.world.state().house().households[&home].since;
    assert_eq!(house_day(again, again), 0);
    assert!(
        !run.world.state().house().social.deeds.contains_key(&sim(1)),
        "day 0 of the earlier stay is cut with the list"
    );
    let window = run
        .world
        .data()
        .social()
        .expect("social")
        .limits
        .deed_window_days;
    assert_eq!(window, 30, "ADR-0031 section 1");
    assert_eq!(
        Deeds::default().knows(NECKT, sim(2), 0, 0, window),
        DeedAnswer::NotDone,
        "the new stay's day 0 was not done"
    );
    for owner in [1_u64, 2] {
        assert_eq!(
            belief(&run, owner, seen).basis,
            Basis::Gesehen,
            "the belief that names the old day stays"
        );
    }
}

/// A long meal, a one-minute snack and a long toilet, and no departure. Partner trust is
/// 600, the figure the ADR uses for a parent heard by a child. Housemate grudge starts at
/// 800 so it is still at least 700 after fade. Founder 0 always teases, so a played Necken
/// cannot plant that grudge, and a taken-in child is a housemate, not a son or daughter.
/// The repository's social file is not the one this run uses.
fn lie_data() -> sim_core::data::Data {
    let repository = include_str!("../../../data/house/day.json");
    let day = repository
        .replace(
            r#""school": {"from_age": 6, "until_age": 18"#,
            r#""school": {"from_age": 6, "until_age": 7"#,
        )
        .replace(
            "\"from_age\": 18,\n    \"until_age\": 67,",
            "\"from_age\": 7,\n    \"until_age\": 8,",
        );
    assert_ne!(day, repository, "the ages in the day file still bind");
    // Hunger in this test house falls by 1 an hour. An offer lasts at most 12 h, so the
    // chain parks them on the toilet again before it ends. Bedtime does not cut an action.
    scene(20_000, 3, 3, 60, &day, 1e-9, 43_200).with_social(common::repository_social_with(
        |social| {
            social["bonds"]["start"]["partner"][1] = json!(600);
            social["bonds"]["start"]["housemate"][1] = json!(600);
            social["bonds"]["start"]["housemate"][5] = json!(800);
        },
    ))
}

/// Woman of 30, man of 60, girl of 10. He is 60 so Necken is closed and Klatsch can win.
/// She is founder 0. The girl is the only other member, so she is W.
fn lie_born() -> [i64; 3] {
    [-30 * year(), -60 * year(), -10 * year()]
}

fn founder(index: u64) -> LineageKey {
    let tag = DataId::new("test:social").expect("tag");
    LineageKey::founder(&tag, index)
}

/// Moved in at 23:59:59 of day 0, as [`day::Run::family`].
fn lie_family() -> (Run, HouseholdId) {
    let (run, home) = day::Run::family(lie_data(), 0x5EED, "test:social", &lie_born(), 0);
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(86_399));
    (run, home)
}

/// Moved in at second 1000, before the morning of day 0, so a talk at 23 400 is house day 1.
fn lie_early() -> (Run, HouseholdId) {
    let mut run = day::Run::founded(lie_data(), 0x5EED, "test:social", &lie_born());
    run.advance(999);
    let home = run.together(3).expect("one household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(1_000));
    (run, home)
}

/// The couple at the table in time for a talk at `at`. `with_w` seats W one second before
/// it, so a deed of W is not material and the talks W's arrival plans are not due yet.
fn stage(run: &mut Run, at: i64, with_w: bool) {
    let arrival = arrive(
        run,
        at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(1, DINING), (2, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, at);
    if with_w {
        let joined = arrive(run, at - WITHIN_ROOM - 2, &[(3, DINING)]);
        assert_eq!(joined, at - 1);
    }
    run.advance(at);
}

/// Off the table. A command submitted at `now` acts on the next second.
fn to_toilet(run: &mut Run, now: i64, ids: &[u64]) {
    for &id in ids {
        run.submit(command(id, TOILET));
    }
    run.advance(now + 1 + WITHIN_ROOM);
}

fn caused_by(entry: &LogEntry, draw: DrawRef) -> bool {
    matches!(entry.causes.as_slice(), [Cause::Random(got)] if *got == draw)
}

fn event_cause(entry: &LogEntry) -> sim_core::ids::EventId {
    match entry.causes.as_slice() {
        [Cause::Event(id)] => *id,
        other => panic!("an earlier entry, got {other:?}"),
    }
}

fn told_lies(run: &Run) -> Vec<&LogEntry> {
    (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Told { lie: true, .. }))
        .collect()
}

fn refutations(run: &Run) -> Vec<&LogEntry> {
    (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Refuted { .. }))
        .collect()
}

/// The reference's first miss for founder 0 toward founder 1: second 22 800, u = 68 860.
/// The grudge is a reason, Klatsch is open, and a miss says nothing.
#[test]
fn a_missed_lie_draw_says_nothing() {
    let (mut run, _) = lie_early();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET)],
    );
    let at = 22_800;
    stage(&mut run, at, false);
    assert_eq!(talked(&run).len(), 1);
    assert_eq!(
        talked(&run)[0].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    assert!(told_lies(&run).is_empty());
    assert!(refutations(&run).is_empty());
    let draw = DrawRef::from(lie_draw_key(founder(0), t(at), founder(1)));
    assert!(run.log.iter().all(|entry| !caused_by(entry, draw)));
    assert!(!run.world.state().house().social.deeds.contains_key(&sim(1)));
    assert_eq!(bond(&run, 2, 3)[TRUST], 600);
    assert!(memory_about(&run, 1, 2, at));
    assert!(memory_about(&run, 2, 1, at));
}

/// The reference's first hit for founder 0 toward founder 1: second 23 400, u = 22 823.
/// Said at 900, heard at 660 from trust 600, and the housemate edge steps once.
#[test]
fn a_lie_at_the_first_hit_is_heard_at_660() {
    let (mut run, home) = lie_early();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET)],
    );
    let at = 23_400;
    stage(&mut run, at, false);
    let since = run.world.state().house().households[&home].since;
    let day = house_day(t(at), since);
    assert_eq!(day, 1);
    let said = laestert(3, 2, day);
    let told = told_lies(&run);
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].t, t(at));
    assert_eq!(told[0].subjects, vec![Subject::Sim(sim(1))]);
    assert_eq!(
        told[0].payload,
        LogPayload::Told {
            to: sim(2),
            statement: said,
            certainty: 900,
            lie: true,
        }
    );
    let draw = DrawRef::from(lie_draw_key(founder(0), t(at), founder(1)));
    assert!(caused_by(told[0], draw));
    let heard_entry = (run.log.iter())
        .find(|entry| {
            entry.t == t(at)
                && entry.subjects == vec![Subject::Sim(sim(2))]
                && matches!(
                    entry.payload,
                    LogPayload::Believed { statement, certainty: 660, .. } if statement == said
                )
        })
        .expect("the hearer believes it");
    assert_eq!(
        day::number(&event_cause(heard_entry)),
        day::number(&told[0].id)
    );
    let stepped = (run.log.iter())
        .find(|entry| {
            entry.t == t(at)
                && entry.subjects == vec![Subject::Sim(sim(2))]
                && matches!(entry.payload, LogPayload::BondChanged { other, .. } if other == sim(3))
        })
        .expect("the edge to W");
    assert_eq!(
        day::number(&event_cause(stepped)),
        day::number(&heard_entry.id)
    );
    let heard = belief(&run, 2, said);
    assert_eq!(heard.certainty, 660);
    assert_eq!(heard.basis, Basis::Gehoert);
    assert_eq!(heard.source, Some(sim(1)));
    assert_eq!(heard.origin, Some(sim(1)));
    assert_eq!(heard.since, t(at));
    assert!(!heard.secret);
    assert!(
        run.world.state().house().social.beliefs[&sim(1)]
            .iter()
            .all(|held| held.statement != said)
    );
    // Grudge fades by 10 a day from the move-in, then the heard step adds 100.
    let elapsed = at - since.as_secs();
    let grudge = u16::try_from(800 - 10 * elapsed / 86_400 + 100).expect("grudge");
    assert_eq!(grudge, 898);
    assert_eq!(bond(&run, 2, 3)[TRUST], 520);
    assert_eq!(bond(&run, 2, 3)[GRUDGE], grudge);
    let deeds = &run.world.state().house().social.deeds[&sim(1)];
    assert_eq!(deeds.items.len(), 2);
    assert_eq!(deeds.items[0].kind, LUEGT);
    assert_eq!(deeds.items[0].other, sim(2));
    assert_eq!(deeds.items[0].day, day);
    assert_eq!(deeds.items[1].kind, LAESTERT);
    assert_eq!(deeds.items[1].other, sim(3));
    assert_eq!(deeds.items[1].day, day);
    let seen = laestert(1, 3, day);
    for owner in [1_u64, 2] {
        let held = belief(&run, owner, seen);
        assert_eq!(held.certainty, 1000);
        assert_eq!(held.basis, Basis::Gesehen);
    }
    assert!(
        run.world
            .state()
            .house()
            .social
            .beliefs
            .get(&sim(3))
            .is_none_or(|list| list.iter().all(|held| held.statement != seen))
    );
    assert!(refutations(&run).is_empty());
    assert_eq!(
        talked(&run)[0].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
}

/// The reference's three hits of founder 0 toward founder 1 in three house days:
/// 114 000, 229 800 and 324 000. Trust 600 falls by 80 each time. At 360 the choice
/// avoids W, and `core:company` names her.
#[test]
fn three_lies_end_at_avoidance() {
    let (mut run, home) = lie_family();
    let since = run.world.state().house().households[&home].since;
    let park = |run: &mut Run, at: i64| {
        arrive(run, at, &[(1, TOILET), (2, TOILET), (3, TOILET)]);
    };
    park(&mut run, MORNING);
    let talks = [114_000_i64, 229_800, 324_000];
    let trusts = [520_u16, 440, 360];
    // The toilet lasts 12 h. These seconds restart it before it ends and before the next talk.
    let bridges = [150_000_i64, 190_000, 270_000, 310_000];
    for (index, &at) in talks.iter().enumerate() {
        stage(&mut run, at, index > 0);
        let day = house_day(t(at), since);
        assert_eq!(day, i64::try_from(index + 1).expect("day"));
        let said = laestert(3, 2, day);
        assert_eq!(belief(&run, 2, said).certainty, 660);
        assert_eq!(belief(&run, 2, said).basis, Basis::Gehoert);
        assert_eq!(bond(&run, 2, 3)[TRUST], trusts[index]);
        let told = (told_lies(&run).into_iter())
            .find(|entry| entry.t == t(at))
            .expect("the lie");
        assert_eq!(told.subjects, vec![Subject::Sim(sim(1))]);
        assert!(caused_by(
            told,
            DrawRef::from(lie_draw_key(founder(0), t(at), founder(1)))
        ));
        if let Some(&next) = talks.get(index + 1) {
            to_toilet(&mut run, at, &[1, 2, 3]);
            for &bridge in bridges
                .iter()
                .filter(|&&bridge| bridge > at && bridge < next)
            {
                park(&mut run, bridge);
            }
        }
    }
    assert_eq!(told_lies(&run).len(), 3);
    assert_eq!(talked(&run).len(), 3);
    assert!(refutations(&run).is_empty());
    run.submit(command(2, FRIDGE));
    run.advance(talks[2] + 200);
    assert_eq!(bond(&run, 2, 3)[TRUST], 360);
    assert_eq!(talked(&run).len(), 3, "no further talk before W's");
    let company = (run.log.iter())
        .filter(|entry| entry.t.as_secs() > talks[2])
        .filter(|entry| entry.subjects == vec![Subject::Sim(sim(2))])
        .find(|entry| matches!(entry.payload, LogPayload::Company { .. }))
        .expect("his choice");
    match &company.payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(row.object, DINING);
            assert_eq!(row.avoided, Some(sim(3)));
            assert_eq!(row.avoid, 8_000);
            assert_eq!(row.liked, Some(sim(1)));
            assert_eq!(row.company, 8_000);
        }
        other => panic!("core:company, got {other:?}"),
    }
}

fn refuted_memory(run: &Run, owner: u64, other: u64, at: i64) -> bool {
    (run.world.state().house().social.memories.get(&sim(owner))).is_some_and(|list| {
        list.iter().any(|memory| {
            memory.kind == REFUTED
                && memory.at == t(at)
                && memory.since == t(at)
                && memory.strength == 700
                && memory.fade == 50
                && memory.valence == -300
                && memory.with == [Some(sim(other)), None]
        })
    })
}

/// After the lie at 23 400, W and the hearer talk at 27 000. The reference's refutation
/// draw of founder 2 toward founder 1 hits (u = 218 846). One refutation, then the edge
/// to the liar.
#[test]
fn the_doer_refutes_the_lie() {
    let (mut run, home) = lie_early();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET)],
    );
    let lie_at = 23_400;
    stage(&mut run, lie_at, false);
    let since = run.world.state().house().households[&home].since;
    let day = house_day(t(lie_at), since);
    let said = laestert(3, 2, day);
    assert_eq!(belief(&run, 2, said).certainty, 660);
    to_toilet(&mut run, lie_at, &[1, 2]);
    let at = 27_000;
    let arrival = arrive(
        &mut run,
        at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(2, DINING), (3, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, at);
    run.advance(at);
    assert_eq!(refutations(&run).len(), 1);
    let refuted = refutations(&run)[0];
    assert_eq!(refuted.t, t(at));
    assert_eq!(refuted.subjects, vec![Subject::Sim(sim(2))]);
    assert_eq!(
        refuted.payload,
        LogPayload::Refuted {
            by: sim(3),
            statement: said,
            source: Some(sim(1)),
        }
    );
    let draw = DrawRef::from(refute_draw_key(founder(2), t(at), founder(1)));
    assert!(caused_by(refuted, draw));
    let held = belief(&run, 2, said);
    assert_eq!(held.certainty, 0);
    assert_eq!(held.basis, Basis::Widerlegt);
    assert_eq!(held.since, t(at));
    assert_eq!(held.source, Some(sim(3)));
    assert_eq!(held.origin, Some(sim(3)));
    assert!(!held.secret);
    assert_eq!(held.statement, said);
    let stepped = (run.log.iter())
        .find(|entry| {
            entry.t == t(at)
                && entry.subjects == vec![Subject::Sim(sim(2))]
                && matches!(entry.payload, LogPayload::BondChanged { other, .. } if other == sim(1))
        })
        .expect("the edge to the liar");
    assert_eq!(day::number(&event_cause(stepped)), day::number(&refuted.id));
    assert_eq!(bond(&run, 2, 1)[TRUST], 480);
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 150);
    assert!(run.log.iter().all(|entry| {
        entry.t != t(at) || !matches!(entry.payload, LogPayload::Believed { .. })
    }));
    let remembered: Vec<u16> = (run.log.iter())
        .filter(|entry| entry.t == t(at))
        .filter_map(|entry| match entry.payload {
            LogPayload::Remembered { kind, .. } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(remembered, vec![TALK, TALK, REFUTED, REFUTED]);
    assert!(memory_about(&run, 2, 3, at));
    assert!(memory_about(&run, 3, 2, at));
    assert!(refuted_memory(&run, 2, 3, at));
    assert!(refuted_memory(&run, 3, 2, at));
    assert_eq!(
        talked(&run).last().expect("the talk").payload,
        LogPayload::Talked {
            other: sim(3),
            object: DINING,
            topic: 5,
            actor: Some(sim(3)),
        }
    );
}

/// The same seat at 30 600. The reference's refutation draw misses (u = 533 784), so the
/// lie stays and the liar's edge does not move.
#[test]
fn a_missed_refutation_leaves_the_lie() {
    let (mut run, home) = lie_early();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET)],
    );
    let lie_at = 23_400;
    stage(&mut run, lie_at, false);
    let since = run.world.state().house().households[&home].since;
    let said = laestert(3, 2, house_day(t(lie_at), since));
    to_toilet(&mut run, lie_at, &[1, 2]);
    let at = 30_600;
    let arrival = arrive(
        &mut run,
        at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(2, DINING), (3, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, at);
    run.advance(at);
    assert!(refutations(&run).is_empty());
    let draw = DrawRef::from(refute_draw_key(founder(2), t(at), founder(1)));
    assert!(run.log.iter().all(|entry| !caused_by(entry, draw)));
    let held = belief(&run, 2, said);
    assert_eq!(held.certainty, 660);
    assert_eq!(held.basis, Basis::Gehoert);
    assert_eq!(held.source, Some(sim(1)));
    assert_eq!(held.origin, Some(sim(1)));
    assert_eq!(bond(&run, 2, 1)[TRUST], 600);
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 0);
    assert!(!refuted_memory(&run, 2, 3, at));
    assert!(!refuted_memory(&run, 3, 2, at));
    assert!(memory_about(&run, 2, 3, at));
}

/// Four people, and the dining table offers only Alltag and Klatsch. Necken, Lob and
/// Entschuldigung would otherwise take the seat before a gossip.
fn lie_data_four() -> sim_core::data::Data {
    let repository = include_str!("../../../data/house/day.json");
    let day = repository
        .replace(
            r#""school": {"from_age": 6, "until_age": 18"#,
            r#""school": {"from_age": 6, "until_age": 7"#,
        )
        .replace(
            "\"from_age\": 18,\n    \"until_age\": 67,",
            "\"from_age\": 7,\n    \"until_age\": 8,",
        );
    assert_ne!(day, repository, "the ages in the day file still bind");
    scene(20_000, 4, 3, 60, &day, 1e-9, 43_200).with_social(common::repository_social_with(
        |social| {
            social["bonds"]["start"]["partner"][1] = json!(600);
            social["bonds"]["start"]["housemate"][1] = json!(600);
            social["bonds"]["start"]["housemate"][5] = json!(800);
            let topics = social["topics"].as_array_mut().expect("topics");
            for (index, topic) in topics.iter_mut().enumerate() {
                if index == 0 || index == 3 {
                    continue;
                }
                let furniture = topic["furniture"].as_array_mut().expect("furniture");
                furniture.retain(|kind| kind != "core:dining_table");
            }
        },
    ))
}

/// Woman of 30, man of 60, girl of 10, and a taken-in girl of 12. He is 60 so Necken is
/// closed. The girl of 12 is under 18, so she is taken into this household.
fn lie_born_four() -> [i64; 4] {
    [-30 * year(), -60 * year(), -10 * year(), -12 * year()]
}

/// Moved in at second 1000, before the morning of day 0, so a talk at 23 400 is house day 1.
fn lie_early_four() -> (Run, HouseholdId) {
    let mut run = day::Run::founded(lie_data_four(), 0x5EED, "test:social", &lie_born_four());
    run.advance(999);
    let home = run.together(4).expect("one household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(1_000));
    (run, home)
}

fn talked_at(run: &Run, at: i64) -> LogPayload {
    run.log
        .iter()
        .find(|entry| entry.t == t(at) && matches!(entry.payload, LogPayload::Talked { .. }))
        .expect("the talk")
        .payload
        .clone()
}

/// A lie, then the refutation of the hearer's copy, then the same gossip really done later
/// that house day. The statement is true. X still holds it as `widerlegt` and sits with Y,
/// who still holds it. The refutation draw would hit. It writes nothing: no refutation, no
/// draw, and the source bond does not move. A day outside the doer's window is the unit
/// case in `the_lie_and_the_refutation_match_the_reference`.
///
/// Seconds are the reference's, seed `0x5EED`. 23 400 lies (founder 0 toward 1, u = 22 823).
/// 27 000 passes it on (founder 1 toward 0 misses, u = 883 351). 27 800 refutes the hearer's
/// copy (founder 2 toward 1 hits, u = 289 620). 28 600 tells the doer what was seen, and her
/// refutation of the remaining copy misses (u = 664 049). 29 400 she tells it onward and the
/// lie draw misses (u = 724 145), so the deed is recorded. 30 600 would refute (founder 1
/// toward 0, u = 482 675) and the lie draw misses (u = 210 480).
#[test]
fn a_holder_cannot_refute_a_statement_that_is_true_now() {
    let (mut run, home) = lie_early_four();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET), (4, TOILET)],
    );
    let since = run.world.state().house().households[&home].since;

    let lie_at = 23_400;
    stage(&mut run, lie_at, false);
    let day = house_day(t(lie_at), since);
    assert_eq!(day, 1);
    let said = laestert(3, 2, day);
    assert_eq!(belief(&run, 2, said).certainty, 660);
    assert_eq!(belief(&run, 2, said).basis, Basis::Gehoert);
    assert_eq!(belief(&run, 2, said).source, Some(sim(1)));
    assert_eq!(
        talked_at(&run, lie_at),
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    to_toilet(&mut run, lie_at, &[1, 2]);

    // The hearer passes the lie on. His own lie draw misses, so the woman holds it too,
    // and both see him gossip about the girl.
    let pass_at = 27_000;
    stage(&mut run, pass_at, false);
    let passed_lie = DrawRef::from(lie_draw_key(founder(1), t(pass_at), founder(0)));
    assert!(
        run.log
            .iter()
            .filter(|entry| entry.t == t(pass_at))
            .all(|entry| !caused_by(entry, passed_lie))
    );
    assert!(told_lies(&run).iter().all(|entry| entry.t != t(pass_at)));
    let heard = belief(&run, 1, said);
    assert_eq!(heard.certainty, 484);
    assert_eq!(heard.basis, Basis::Gehoert);
    assert_eq!(heard.source, Some(sim(2)));
    assert_eq!(heard.origin, Some(sim(1)));
    assert_eq!(belief(&run, 2, said).basis, Basis::Gehoert);
    let seen_man = laestert(2, 3, day);
    assert_eq!(belief(&run, 1, seen_man).certainty, 1000);
    assert_eq!(belief(&run, 1, seen_man).basis, Basis::Gesehen);
    assert_eq!(
        talked_at(&run, pass_at),
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    to_toilet(&mut run, pass_at, &[1, 2]);

    // The girl refutes the hearer's copy. The woman's copy stays.
    let refute_at = 27_800;
    let arrival = arrive(
        &mut run,
        refute_at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(2, DINING), (3, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, refute_at);
    run.advance(refute_at);
    assert_eq!(refutations(&run).len(), 1);
    assert_eq!(refutations(&run)[0].t, t(refute_at));
    let refuted = belief(&run, 2, said);
    assert_eq!(refuted.certainty, 0);
    assert_eq!(refuted.basis, Basis::Widerlegt);
    assert_eq!(belief(&run, 1, said).certainty, 484);
    assert_eq!(belief(&run, 1, said).basis, Basis::Gehoert);
    assert_eq!(bond(&run, 2, 1)[TRUST], 480);
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 150);
    assert_eq!(
        talked_at(&run, refute_at),
        LogPayload::Talked {
            other: sim(3),
            object: DINING,
            topic: 0,
            actor: None,
        }
    );
    to_toilet(&mut run, refute_at, &[2, 3]);

    // The woman tells the girl the seen deed. The fourth person sits in time to see the
    // telling, so that deed is not material when the girl repeats it, and stands up before
    // her own talks. The girl's refutation of the remaining copy misses.
    let tell_at = 28_600;
    let seated = arrive(
        &mut run,
        tell_at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(1, DINING), (3, DINING)],
    );
    assert_eq!(seated + TALK_DELAY, tell_at);
    let joined = arrive(&mut run, seated, &[(4, DINING)]);
    assert!(joined < tell_at, "she is seated before the telling");
    run.advance(tell_at);
    assert_eq!(talked(&run).len(), 4);
    let girl_heard = belief(&run, 3, seen_man);
    assert_eq!(girl_heard.certainty, 733);
    assert_eq!(girl_heard.basis, Basis::Gehoert);
    let seen_woman = laestert(1, 2, day);
    assert_eq!(belief(&run, 4, seen_woman).certainty, 1000);
    assert_eq!(belief(&run, 4, seen_woman).basis, Basis::Gesehen);
    assert!(
        run.world
            .state()
            .house()
            .social
            .beliefs
            .get(&sim(4))
            .is_none_or(|list| list.iter().all(|held| held.statement != seen_man))
    );
    assert_eq!(belief(&run, 1, said).certainty, 484);
    assert_eq!(belief(&run, 1, said).basis, Basis::Gehoert);
    let missed = DrawRef::from(refute_draw_key(founder(2), t(tell_at), founder(0)));
    assert!(run.log.iter().all(|entry| !caused_by(entry, missed)));
    assert_eq!(
        talked_at(&run, tell_at),
        LogPayload::Talked {
            other: sim(3),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    to_toilet(&mut run, tell_at, &[1, 3, 4]);

    // The girl tells the fourth person. Her lie draw misses, so she says what the man did,
    // and that records her own gossip about him. The lie is now a true deed.
    let deed_at = 29_400;
    let arrival = arrive(
        &mut run,
        deed_at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(3, DINING), (4, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, deed_at);
    run.advance(deed_at);
    let deed_lie = DrawRef::from(lie_draw_key(founder(2), t(deed_at), founder(3)));
    assert!(
        run.log
            .iter()
            .filter(|entry| entry.t == t(deed_at))
            .all(|entry| !caused_by(entry, deed_lie))
    );
    assert!(told_lies(&run).iter().all(|entry| entry.t != t(deed_at)));
    let deeds = &run.world.state().house().social.deeds[&sim(3)];
    assert_eq!(
        deeds.knows(LAESTERT, sim(2), day, day, 30),
        DeedAnswer::Done
    );
    assert_eq!(belief(&run, 2, said).basis, Basis::Widerlegt);
    assert_eq!(belief(&run, 1, said).basis, Basis::Gehoert);
    assert_eq!(
        talked_at(&run, deed_at),
        LogPayload::Talked {
            other: sim(4),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    to_toilet(&mut run, deed_at, &[3, 4]);

    let man_to_woman = bond(&run, 2, 1);
    let woman_to_man = bond(&run, 1, 2);
    assert_eq!(man_to_woman[TRUST], 480);
    assert_eq!(man_to_woman[GRUDGE], 150);
    assert_eq!(woman_to_man[TRUST], 600);
    assert_eq!(woman_to_man[GRUDGE], 0);

    // X with Y. He holds the statement widerlegt and she still holds it, and it is true.
    // The draw would hit. Nothing is refuted and neither bond moves.
    let again = 30_600;
    stage(&mut run, again, false);
    let key = refute_draw_key(founder(1), t(again), founder(0));
    let u = talk_ppm(WorldSeed::new(0x5EED), key);
    assert!(u < 500_000, "the refutation draw would hit, u = {u}");
    let draw = DrawRef::from(key);
    assert!(run.log.iter().all(|entry| !caused_by(entry, draw)));
    assert_eq!(refutations(&run).len(), 1);
    assert!(refutations(&run).iter().all(|entry| entry.t != t(again)));
    let still = belief(&run, 1, said);
    assert_eq!(still.certainty, 484);
    assert_eq!(still.basis, Basis::Gehoert);
    assert_eq!(still.source, Some(sim(2)));
    assert_eq!(still.origin, Some(sim(1)));
    assert!(!still.secret);
    assert_eq!(belief(&run, 2, said).certainty, 0);
    assert_eq!(belief(&run, 2, said).basis, Basis::Widerlegt);
    assert_eq!(bond(&run, 2, 1), man_to_woman);
    assert_eq!(bond(&run, 1, 2), woman_to_man);
    assert_eq!(
        talked_at(&run, again),
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
}
