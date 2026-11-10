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
use sim_core::social::bond::GRUDGE;
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
    assert_eq!(counts(&run, 1), (1, 2));
    assert_eq!(counts(&run, 2), (1, 3));
    let since = run.world.state().house().households[&home].since;
    let day = sim_core::social::talk::house_day(t(first), since);
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
    assert_eq!(counts(&run, 1), (2, 4));
    assert_eq!(counts(&run, 2), (2, 6));
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
    assert_eq!(counts(&run, 1), (1, 2));
    assert_eq!(counts(&run, 2), (1, 3));
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

/// Six Necken leave him 18 entries, so 16 of the day's 34 are free and the next talk is
/// planned (ADR-0030 sections 3 and 5). It is due in the second the meals end. The child
/// sits down first and teases each parent, which writes him 3 more entries. 13 free is
/// below the 16 a talk needs, so that end does not run the talk: the counts, the grudge
/// and the last talk stay, and nothing is planned. The end still cancels the event.
///
/// Day 0 move-in is 86 399 and the first morning is 108 000. The walk starts at 108 001
/// and reaches the table at 108 006. Talks fall at 108 606 and every 3 600 s: the sixth at
/// 126 606, the seventh at 130 206. The meal is 22 200 s, and 108 006 + 22 200 = 130 206.
/// The child is sent at 126 607, arrives at 126 612 and talks at 127 212; those successors
/// are at 130 812, after the meal. Necken writes 2 entries for the actor and 3 for the
/// target. She acts in the six couple talks (12) and with the child (14). He is the target
/// of the six (18) and of the child's tease (21). The child writes 3, then 2. His grudge toward
/// her is 6 * 60 = 360; one hour fades none (`10 * 3 600 / 86 400` = 0). The house day at
/// 130 206 is 1, so the counters do not reset.
#[test]
fn an_action_end_in_the_same_second_skips_a_talk_without_sixteen_free_entries() {
    const MEAL: u32 = 22_200;
    let born = [-30 * year(), -32 * year(), -10 * year()];
    let (mut run, home) =
        day::Run::family(without_departure(MEAL), 0x5EED, "test:social", &born, 0);
    let arrival = arrive(&mut run, MORNING, &[(1, DINING), (2, DINING), (3, FRIDGE)]);
    let first = arrival + TALK_DELAY;
    let sixth = first + 5 * PAIR_GAP;
    let seventh = sixth + PAIR_GAP;
    assert_eq!(arrival, 108_006);
    assert_eq!(sixth, 126_606);
    assert_eq!(seventh, 130_206);
    assert_eq!(seventh, arrival + i64::from(MEAL));
    let since = run.world.state().house().households[&home].since;
    assert_eq!(since, t(86_399));
    assert_eq!(sim_core::social::talk::house_day(t(seventh), since), 1);

    run.advance(sixth);
    assert_eq!(talked(&run).len(), 6);
    assert_eq!(counts(&run, 1), (6, 12));
    assert_eq!(counts(&run, 2), (6, 18));
    assert_eq!(counts(&run, 3), (0, 0));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 360);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(seventh)));

    run.submit(command(3, DINING));
    let child_arrival = sixth + 1 + WITHIN_ROOM;
    let child_talk = child_arrival + TALK_DELAY;
    assert_eq!(child_arrival, 126_612);
    assert_eq!(child_talk, 127_212);
    assert!(child_talk + PAIR_GAP > seventh);
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
    assert_eq!(joined, (sixth + 1, walk, DINING));
    assert!(
        talked(&run)
            .iter()
            .all(|entry| { matches!(entry.payload, LogPayload::Talked { topic: 2, .. }) })
    );
    assert_eq!(talked(&run).len(), 8);
    assert_eq!(counts(&run, 1), (7, 14));
    assert_eq!(counts(&run, 2), (7, 21));
    assert_eq!(counts(&run, 3), (2, 5));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 360);
    assert_eq!(run.world.talk_at(sim(1), sim(2)), Some(t(seventh)));

    run.advance(seventh);
    assert_eq!(counts(&run, 1), (7, 14));
    assert_eq!(counts(&run, 2), (7, 21));
    assert_eq!(counts(&run, 3), (2, 5));
    assert_eq!(bond(&run, 2, 1)[GRUDGE], 360);
    assert_eq!(talked(&run).len(), 8);
    assert!(talked(&run).iter().all(|entry| entry.t != t(seventh)));
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    assert_eq!(
        run.world.state().house().households[&home].last_talk[&(sim(1), sim(2))],
        t(sixth)
    );
    assert!((run.log.iter()).any(|entry| {
        entry.t == t(seventh) && matches!(entry.payload, LogPayload::ActionEnded { .. })
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
    assert_eq!(counts(&run, 1), (1, 2));
    assert_eq!(run.world.talk_at(sim(1), sim(2)), None);
    assert!(!run.world.state().house().sims.contains_key(&sim(2)));
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
/// not first, and a Talk of someone who is not there. A save of core 65 or projection 21
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
    old_core[core_at] = 0x41;
    assert_eq!(load(&old_core), Some(LoadError::ForeignCoreVersion));
    let mut old_projection = bytes.clone();
    old_projection[proj_at] = 0x15;
    assert_eq!(
        load(&old_projection),
        Some(LoadError::ForeignProjectionVersion)
    );
    assert!(load(&bytes).is_none());
}
