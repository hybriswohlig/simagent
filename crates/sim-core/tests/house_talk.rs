//! A talk at sociable furniture (ADR-0030 section 3). The expected seconds, topics and bond
//! steps are the reference's and the data's: a walk inside one room is 5 s (ADR-0025), the
//! talk is planned 600 s after the later arrival, and the next one 3600 s after a talk that
//! took place. Recall of Erinnern is the unit test `a_recall_adds_300_and_stops_at_1000`:
//! no interest pin of `test:social` selects that topic, and a bond cannot be planted here.

mod common;

use common::day::{self, MORNING, Run};
use sim_core::canonical::PROJECTION_VERSION;
use sim_core::data_id::{Code, DataId};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{Command, Input, MoveIn, MoveOut};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::mortality::accident_age;
use sim_core::rng::{DrawKey, Draws, WorldSeed, tags};
use sim_core::social::belief::{Basis, Belief, LAESTERT, NECKT, Statement};
use sim_core::social::bond::{GRUDGE, TRUST};
use sim_core::social::data::TALK;
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
) -> sim_core::data::Data {
    let kinds = format!(
        r#"[
      {{"id": "core:dining_table", "slots": {dining_slots}, "offers": [
        {{"id": "eat", "rates": {{"hunger": 1}}, "seconds": {meal}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:fridge", "slots": 4, "offers": [
        {{"id": "snack", "rates": {{"hunger": 1}}, "seconds": {fridge_secs}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:toilet", "slots": 4, "offers": [
        {{"id": "use", "rates": {{"bladder": 1}}, "seconds": 60, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:shower", "slots": 4, "offers": [
        {{"id": "shower", "rates": {{"hygiene": 1}}, "seconds": 60, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:sofa", "slots": 1, "offers": [
        {{"id": "sit", "rates": {{"fun": 1}}, "seconds": 60, "min_age": 0, "max_age": 130}}]}},
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
    scene(meal, 3, 3, meal, &day, 1e-9)
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
    refuse(&data, &bytes, &pair, &[one, encode(&unknown)].concat());

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
