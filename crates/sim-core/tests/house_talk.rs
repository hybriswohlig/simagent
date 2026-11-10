//! A talk at sociable furniture (ADR-0030 section 3). The expected seconds, topics and bond
//! steps are the reference's and the data's: a walk inside one room is 5 s (ADR-0025), the
//! talk is planned 600 s after the later arrival, and the next one 3600 s after a talk that
//! took place. Recall of Erinnern is the unit test `a_recall_adds_300_and_stops_at_1000`:
//! no interest pin of `test:social` selects that topic, and a bond cannot be planted here.

mod common;

use common::day::{self, MORNING, Run};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{Command, Input, MoveIn, MoveOut};
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::social::bond::GRUDGE;
use sim_core::social::data::TALK;
use sim_core::time::{SimTime, YEAR};

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
    let kinds = format!(
        r#"[
      {{"id": "core:dining_table", "slots": 2, "offers": [
        {{"id": "eat", "rates": {{"hunger": 1}}, "seconds": {meal}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:fridge", "slots": 4, "offers": [
        {{"id": "snack", "rates": {{"hunger": 1}}, "seconds": 60, "min_age": 0, "max_age": 130}}]}},
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
    let day = include_str!("../../../data/house/day.json");
    let houses = common::houses_from(&day::rates(1, 0, 0), day, &kinds, &[&house]);
    common::death_data(1e-9, None, serde_json::json!([]))
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

/// Commands `seats` at 06:00:01, after the morning's own walks have begun, and runs to the
/// arrival. Returns that second. The walk is the 5 s inside the hall.
fn seat(run: &mut Run, seats: &[(u64, u16)]) -> i64 {
    run.advance(MORNING);
    let now = MORNING + 1;
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

/// A death in the talk's second is later than the talk. The talk counts; its successor does not
/// survive the death.
#[test]
fn a_death_in_the_same_second_comes_after_the_talk() {
    let (mut run, _) = couple(7_200);
    let arrival = seat(&mut run, &[(1, DINING), (2, DINING)]);
    let at = arrival + TALK_DELAY;
    run.advance(at - 1);
    let by = run.log.last().expect("an entry").id;
    run.world.inject_accident(sim(2), by);
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
