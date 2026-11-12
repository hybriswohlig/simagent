//! Company and avoid in a choice (ADR-0030 section 4). The bonus is outside the mood, after the
//! scaled needs, like the walk, and a fallback offer is scored so it gets it too. `+company` once
//! if affection + familiarity toward someone at that sociable furniture is at least 600, `−avoid`
//! once if trust in someone who sits there is under 400; both can count. Company counts a
//! household member who sits or is on the way (a reserved place). Avoid counts only someone who
//! has arrived. A person with no bond is skipped.
//!
//! The points below are by hand, from ADR-0026 and `tools/social_reference.py`
//! (`check_choice_bonus`), not from a run of this crate. In the repository flat an adult in the
//! hall at the morning values has the television at 39 850 and the table at 39 800 (the same
//! figures as `house_choice.rs`). In the evening house the weight of fun is 320 and the walk
//! inside the living room costs 250.

mod common;

use common::day::{MORNING, Run};
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::{Bonus, Choice, ENOUGH, Scored, choose, score};
use sim_core::house::data::HouseData;
use sim_core::house::state::{Action, Place};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input};
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::social::bond::{self, choice_bonus};
use sim_core::social::data::SocialData;
use sim_core::social::feeling::Feel;
use sim_core::time::{DAY, HOUR};

/// Hunger 600, energy 900, bladder 500, hygiene 700, fun 800.
const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

/// The choice rules of `data/social/social.json`.
const RULES: (i64, i64, u16, u16) = (8_000, 8_000, 600, 400);

/// 18:00 of day 1. Fun has fallen 120 from 800, so it is 680 and weighs 320.
const EVENING: i64 = MORNING + 12 * HOUR.as_secs();
const EVENING_NEEDS: [u16; 5] = [600, 900, 500, 700, 680];

/// Sofa and television from the living room, mood 0: `320 · gain − 250`.
const SOFA_ALONE: i64 = 320 * 125 - 250;
const TV_ALONE: i64 = 320 * 126 - 250;
const SOFA_WITH: i64 = SOFA_ALONE + 8_000;

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("id")
}

fn no_mood(_: u16, _: u16) -> i8 {
    0
}

fn points_of(scored: &[Scored], object: u16, offer: u16) -> i64 {
    scored
        .iter()
        .find(|scored| scored.object == object && scored.offer == offer)
        .expect("the offer")
        .points
}

fn edge(affection: u16, trust: u16, familiarity: u16) -> [u16; 6] {
    [affection, trust, 400, 0, familiarity, 0]
}

/// `present` need not be sorted. `edges` are the chooser's bonds; a missing one is no bond.
fn bonus(present: &[u64], edges: &[(u64, [u16; 6])]) -> bond::Company {
    let there: Vec<SimId> = present.iter().copied().map(sim).collect();
    choice_bonus(
        &there,
        |other| {
            edges
                .iter()
                .find(|(id, _)| sim(*id) == other)
                .map(|(_, values)| *values)
        },
        RULES,
    )
}

/// Company on the television crosses 40 000; the mood does not scale the bonus; a fallback
/// offer and a net of 0 are scored like any other offer.
#[test]
fn company_crosses_the_threshold_and_stays_outside_the_mood() {
    let houses = common::repository_houses();
    let house = houses
        .house(&DataId::new("core:flat").expect("id"))
        .expect("flat");
    let sit = &houses
        .kind(&house.def().objects[2].kind)
        .expect("sofa")
        .offers[1];
    assert!(sit.fallback, "the sofa's sit is the repository's fallback");
    let morning = (30, MORNING_VALUES, 0);
    let open = (None, None);
    let plain = score(house, &houses, morning, open, no_mood, 0, |_| Bonus::NONE);
    // Television: fun weighs 200 and the gain is capped at the 200 still missing; 3 s from the hall.
    assert_eq!(points_of(&plain, 3, 0), 200 * 200 - 150);
    assert_eq!(200 * 200 - 150, 39_850);
    const { assert!(39_850 < ENOUGH) };
    let with_tv = score(house, &houses, morning, open, no_mood, 0, |object| {
        if object == 3 {
            Bonus::company(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&with_tv, 3, 0), 39_850 + 8_000);
    const { assert!(39_850 + 8_000 >= ENOUGH) };
    // Toilet and shower are the only morning offers already past 40 000. Both taken, the
    // choice waits for the toilet; it does not take the television until company carries it over.
    let busy = [5, 6];
    assert_eq!(
        choose((house, &houses), (30, MORNING_VALUES), &plain, &busy, None),
        Choice::Wait(5)
    );
    assert_eq!(
        choose(
            (house, &houses),
            (30, MORNING_VALUES),
            &with_tv,
            &busy,
            None
        ),
        Choice::Take(Scored {
            object: 3,
            offer: 0,
            mood: 0,
            points: 47_850,
        })
    );

    // Sit: fun 200 · 60, mood +10 scales only that, then the walk of 3 s. The bonus stays 8 000.
    let warm = |object, offer| {
        if (object, offer) == (2, 1) { 10 } else { 0 }
    };
    let cool = score(house, &houses, morning, open, warm, 0, |_| Bonus::NONE);
    let warmed = score(house, &houses, morning, open, warm, 0, |object| {
        if object == 2 {
            Bonus::company(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&cool, 2, 1), 12_000 * 110 / 100 - 150);
    assert_eq!(12_000 * 110 / 100 - 150, 13_050);
    assert_eq!(points_of(&warmed, 2, 1), 13_050 + 8_000);
    assert_eq!((13_050 + 8_000) - 13_050, 8_000);
    let seated = score(house, &houses, morning, open, no_mood, 0, |object| {
        if object == 2 {
            Bonus::company(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&seated, 2, 1), (12_000 - 150) + 8_000);

    let cancelled = score(house, &houses, morning, open, no_mood, 0, |object| {
        // Equal company and avoid: the scored bonus is their difference.
        if object == 2 {
            Bonus {
                company: RULES.0,
                avoid: RULES.1,
            }
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&cancelled, 2, 1), points_of(&plain, 2, 1));
    let avoided = score(house, &houses, morning, open, no_mood, 0, |object| {
        if object == 2 {
            Bonus::avoid(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&seated, 2, 1) - points_of(&plain, 2, 1), 8_000);
    assert_eq!(points_of(&plain, 2, 1) - points_of(&avoided, 2, 1), 8_000);
}

/// 8 000 outweighs the table's extra second of walking and does not outweigh a critical need.
#[test]
fn company_outweighs_a_longer_walk_and_never_a_critical_need() {
    let houses = common::repository_houses();
    let house = houses
        .house(&DataId::new("core:flat").expect("id"))
        .expect("flat");
    let morning = (30, MORNING_VALUES, 0);
    let open = (None, None);
    let plain = score(house, &houses, morning, open, no_mood, 0, |_| Bonus::NONE);
    // Table 100 · 400 − 4 s, television 200 · 200 − 3 s: the television leads by the walk, 50.
    assert_eq!(points_of(&plain, 0, 0), 100 * 400 - 200);
    assert_eq!(points_of(&plain, 3, 0), 200 * 200 - 150);
    assert_eq!((100 * 400 - 200, 200 * 200 - 150), (39_800, 39_850));
    let with_table = score(house, &houses, morning, open, no_mood, 0, |object| {
        if object == 0 {
            Bonus::company(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&with_table, 0, 0), 39_800 + 8_000);
    assert!(points_of(&with_table, 0, 0) > points_of(&plain, 3, 0));
    let busy = [5, 6];
    assert_eq!(
        choose(
            (house, &houses),
            (30, MORNING_VALUES),
            &with_table,
            &busy,
            None
        ),
        Choice::Take(Scored {
            object: 0,
            offer: 0,
            mood: 0,
            points: 47_800,
        })
    );

    // Hunger 150 is critical. The table raises it; the sofa does not, so 8 000 stays ordinary.
    let hungry = [150, 1_000, 700, 900, 800];
    let with_sofa = score(
        house,
        &houses,
        (30, hungry, 0),
        open,
        no_mood,
        0,
        |object| {
            if object == 2 {
                Bonus::company(8_000)
            } else {
                Bonus::NONE
            }
        },
    );
    let table = 550 * 600 - 200 + 1_000_000_000;
    let sofa = 200 * 60 - 150 + 8_000;
    assert_eq!(table, 1_000_329_800);
    assert_eq!(sofa, 19_850);
    assert_eq!(points_of(&with_sofa, 0, 0), table);
    assert_eq!(points_of(&with_sofa, 2, 1), sofa);
    assert!(table > sofa);
    assert_eq!(
        choose((house, &houses), (30, hungry), &with_sofa, &[], None),
        Choice::Take(Scored {
            object: 0,
            offer: 0,
            mood: 0,
            points: table,
        })
    );
}

/// The ties of `tools/social_reference.py` `check_choice_bonus`.
#[test]
fn liked_and_avoided_follow_the_reference() {
    let company = bonus(&[5], &[(5, edge(400, 450, 200))]);
    assert_eq!(
        (
            company.liked,
            company.company,
            company.avoided,
            company.avoid
        ),
        (Some(sim(5)), 8_000, None, 0)
    );
    let short = bonus(&[5], &[(5, edge(400, 450, 199))]);
    assert_eq!(
        (short.liked, short.company, short.avoided, short.avoid),
        (None, 0, None, 0)
    );
    // Sums 700, 700 and 600, given out of order: the largest sum, then the smaller id.
    let tied = bonus(
        &[6, 4, 5],
        &[
            (4, edge(400, 450, 300)),
            (5, edge(450, 450, 250)),
            (6, edge(500, 450, 100)),
        ],
    );
    assert_eq!(tied.liked, Some(sim(4)));
    let larger = bonus(
        &[4, 5],
        &[(4, edge(400, 450, 300)), (5, edge(500, 450, 300))],
    );
    assert_eq!(larger.liked, Some(sim(5)));
    let shy = bonus(&[8, 7], &[(7, edge(0, 399, 0)), (8, edge(0, 399, 0))]);
    assert_eq!(
        (shy.liked, shy.avoided, shy.avoid, shy.company - shy.avoid),
        (None, Some(sim(7)), 8_000, -8_000)
    );
    let trust = bonus(&[7, 8], &[(7, edge(0, 400, 0)), (8, edge(0, 320, 0))]);
    assert_eq!((trust.avoided, trust.avoid), (Some(sim(8)), 8_000));
    let both = bonus(&[9, 5], &[(5, edge(500, 450, 300)), (9, edge(0, 320, 0))]);
    assert_eq!(
        (both.liked, both.company, both.avoided, both.avoid),
        (Some(sim(5)), 8_000, Some(sim(9)), 8_000)
    );
    assert_eq!(both.company - both.avoid, 0);
    let none = bonus(&[5], &[]);
    assert_eq!(
        (none.liked, none.company, none.avoided, none.avoid),
        (None, 0, None, 0)
    );
}

const REPOSITORY_DAY: &str = include_str!("../../../data/house/day.json");

/// Weekday mornings, no mood, and no work at 30: a duty would close the evening offers.
fn day() -> String {
    let mut day: Value = serde_json::from_str(REPOSITORY_DAY).expect("day");
    day["choice_mood_percent"] = json!(0);
    day["work"]["from_age"] = json!(120);
    day["work"]["until_age"] = json!(130);
    day["night"]["until_weekend"] = day["night"]["until"].clone();
    day.to_string()
}

fn offer(id: &str, need: &str, rate: i32, seconds: u32) -> Value {
    json!({
        "id": id,
        "rates": {need: rate},
        "seconds": seconds,
        "min_age": 0,
        "max_age": 130
    })
}

fn kinds() -> Value {
    kinds_with(126, 60)
}

/// The kinds with the television's fun `tv` an hour (the repository test house: 126).
fn kinds_with_tv(tv: i32) -> Value {
    kinds_with(tv, 60)
}

/// As [`kinds`], with the table's eat lasting `table_seconds`. A long eat keeps him seated
/// at her next choice; the gain stays a few points, under the television.
fn kinds_with_table(table_seconds: u32) -> Value {
    kinds_with(126, table_seconds)
}

/// The kinds with the television's fun `tv` and the table's eat lasting `table_seconds`.
fn kinds_with(tv: i32, table_seconds: u32) -> Value {
    json!([
        {"id": "core:sofa", "slots": 2, "offers": [offer("sit", "fun", 125, 3_600)]},
        {"id": "core:tv", "slots": 1, "offers": [offer("watch", "fun", tv, 3_600)]},
        {"id": "test:bed", "slots": 2, "bed_for": "couples", "offers": [
            {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
        {"id": "core:dining_table", "slots": 1, "offers": [offer("eat", "hunger", 1, table_seconds)]},
        {"id": "core:fridge", "slots": 1, "offers": [offer("snack", "hunger", 1, 60)]},
        {"id": "core:toilet", "slots": 1, "offers": [offer("use", "bladder", 1, 60)]},
        {"id": "core:shower", "slots": 1, "offers": [offer("shower", "hygiene", 1, 60)]}
    ])
}

/// Hall 0 is the entrance and is not stayed in, so an idle morning walks to the living room
/// and the next choice is weighed from there. Sofa 0 and television 1 are both in that room,
/// and neither offer is a fallback, so the morning does not sit anyone down.
fn house() -> String {
    json!({
        "id": "test:company", "capacity": 2,
        "rooms": [
            {"id": "hall", "entrance": true, "dwell": false},
            {"id": "living"},
            {"id": "bath"},
            {"id": "bedroom"}
        ],
        "child_day_room": "living", "child_night_room": "bedroom", "dwell_room": "living",
        "doors": [
            {"a": "hall", "b": "living", "seconds": 3},
            {"a": "hall", "b": "bath", "seconds": 3},
            {"a": "hall", "b": "bedroom", "seconds": 4}
        ],
        "objects": [
            {"kind": "core:sofa", "room": "living"},
            {"kind": "core:tv", "room": "living"},
            {"kind": "test:bed", "room": "bedroom"},
            {"kind": "core:dining_table", "room": "hall"},
            {"kind": "core:fridge", "room": "hall"},
            {"kind": "core:toilet", "room": "bath"},
            {"kind": "core:shower", "room": "bath"}
        ]
    })
    .to_string()
}

fn company_houses() -> HouseData {
    houses_with_kinds(&kinds())
}

fn houses_with_kinds(kinds: &Value) -> HouseData {
    common::houses_from(
        &common::day::rates(0, 0, -10),
        &day(),
        &kinds.to_string(),
        &[&house()],
    )
}

fn company_data(social: SocialData) -> Data {
    common::death_data(1e-9, None, json!([]))
        .with_houses(company_houses())
        .with_social(social)
}

/// The evening house with the table's eat lasting `table_seconds`.
fn company_data_table(social: SocialData, table_seconds: u32) -> Data {
    let houses = common::houses_from(
        &common::day::rates(0, 0, -10),
        &day(),
        &kinds_with_table(table_seconds).to_string(),
        &[&house()],
    );
    common::death_data(1e-9, None, json!([]))
        .with_houses(houses)
        .with_social(social)
}

fn about(entry: &LogEntry, id: u64) -> bool {
    entry.subjects == [Subject::Sim(sim(id))]
}

fn command(id: u64, object: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer: 0,
    })
}

/// The partners have chosen. He was told to the sofa in this same second, so he is on the way
/// and has not arrived; her choice then sees him there.
fn evening(social: SocialData) -> Run {
    let (mut run, _) = Run::moved_in(company_data(social));
    run.advance(EVENING - 1);
    run.submit(command(2, 0));
    run.advance(EVENING);
    run
}

fn her_choice(run: &Run) -> (usize, &LogEntry) {
    let at = (run.log.iter().position(|entry| {
        about(entry, 1)
            && entry.t.as_secs() == EVENING
            && matches!(
                entry.payload,
                LogPayload::ActionStarted {
                    commanded: false,
                    ..
                }
            )
    }))
    .expect("her choice at 18:00");
    (at, &run.log[at])
}

/// Morning choices idle in an empty house: no sociable furniture has anyone at it, so the
/// choice writes no `core:company`.
#[test]
fn no_company_entry_while_nobody_sits() {
    let (mut run, _) = Run::moved_in(company_data(common::repository_social()));
    run.advance(MORNING);
    assert!(
        run.log
            .iter()
            .all(|entry| !matches!(entry.payload, LogPayload::Company { .. }))
    );
    assert!(
        run.log
            .iter()
            .any(|entry| { about(entry, 1) && matches!(entry.payload, LogPayload::Idle { .. }) })
    );
    assert!(
        run.log
            .iter()
            .any(|entry| { about(entry, 2) && matches!(entry.payload, LogPayload::Idle { .. }) })
    );
}

/// At 18:00 the television alone brings 40 070 and the sofa 39 750. His being on the way to the
/// sofa adds 8 000, and she sits with him. The entry comes before her `core:action_started`.
#[test]
fn partners_in_the_evening_sit_together() {
    assert_eq!((SOFA_ALONE, TV_ALONE, SOFA_WITH), (39_750, 40_070, 47_750));
    const { assert!(SOFA_ALONE < ENOUGH && TV_ALONE >= ENOUGH && SOFA_WITH > TV_ALONE) };
    let houses = company_houses();
    let home = houses
        .house(&DataId::new("test:company").expect("id"))
        .expect("house");
    let there = (30, EVENING_NEEDS, 1);
    let open = (None, None);
    let alone = score(home, &houses, there, open, no_mood, 0, |_| Bonus::NONE);
    let joined = score(home, &houses, there, open, no_mood, 0, |object| {
        if object == 0 {
            Bonus::company(8_000)
        } else {
            Bonus::NONE
        }
    });
    assert_eq!(points_of(&alone, 0, 0), SOFA_ALONE);
    assert_eq!(points_of(&alone, 1, 0), TV_ALONE);
    assert_eq!(points_of(&joined, 0, 0), SOFA_WITH);
    assert_eq!(
        choose((home, &houses), (30, EVENING_NEEDS), &alone, &[], None),
        Choice::Take(Scored {
            object: 1,
            offer: 0,
            mood: 0,
            points: TV_ALONE,
        })
    );
    assert_eq!(
        choose((home, &houses), (30, EVENING_NEEDS), &joined, &[], None),
        Choice::Take(Scored {
            object: 0,
            offer: 0,
            mood: 0,
            points: SOFA_WITH,
        })
    );

    let run = evening(common::repository_social());
    let companies: Vec<_> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Company { .. }))
        .collect();
    assert_eq!(
        companies.len(),
        1,
        "only her evening choice has anyone seated"
    );
    let (at, started) = her_choice(&run);
    let company = &run.log[at - 1];
    assert!(about(company, 1));
    assert_eq!(company.t.as_secs(), EVENING);
    assert_eq!(
        format!("{:?}", company.causes),
        format!("{:?}", started.causes)
    );
    match &company.payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(row.object, 0);
            assert_eq!(row.liked, Some(sim(2)));
            assert_eq!(row.company, 8_000);
            assert_eq!(row.avoided, None);
            assert_eq!(row.avoid, 0);
        }
        other => panic!("core:company, got {other:?}"),
    }
    match &started.payload {
        LogPayload::ActionStarted {
            object: 0,
            offer: 0,
            slot: 1,
            room: 1,
            walk_secs: 5,
            needs: EVENING_NEEDS,
            commanded: false,
            filler: false,
            ..
        } => {}
        other => panic!("she sits on the sofa, got {other:?}"),
    }
    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert_eq!(
        him.action,
        Some(Action {
            object: 0,
            offer: 0,
            slot: 0,
            filler: false,
        })
    );
    assert!(
        matches!(him.place, Place::Walking { arrive, .. } if arrive.as_secs() == EVENING + 5),
        "he was on the way, not yet at the sofa"
    );
    assert!(run.log.iter().all(|entry| {
        !(about(entry, 2) && matches!(entry.payload, LogPayload::Reached { .. }))
    }));
}

/// Trust 320 is avoid once he sits, and the partner sum is still company, so the two cancel.
/// On the way only company counts, and the table stays under the television, so she takes it.
/// An hour of it leaves fun at 806, weight 194, and the television at 24 194, under 40 000, so
/// the next choice is idle. He still sits, and the row is written with both components, net 0.
#[test]
fn both_components_are_logged_when_they_cancel() {
    assert_eq!(familiarity_at(EVENING), 597);
    assert!(700 + familiarity_at(EVENING) >= 600);
    let later = EVENING + 5 + 3_600;
    assert!(EVENING + 3 < later && later < EVENING + 3 + 7_200);
    assert_eq!(familiarity_at(later), 597);

    let social = common::repository_social_with(|social| {
        social["bonds"]["start"]["partner"][1] = json!(320);
    });
    let (mut run, _) = Run::moved_in(company_data_table(social, 7_200));
    run.advance(EVENING - 1);
    run.submit(command(2, 3));
    run.advance(EVENING);

    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(
        matches!(him.place, Place::Walking { .. }),
        "he is on the way, so avoid does not apply yet"
    );
    let (at, started) = her_choice(&run);
    match &started.payload {
        LogPayload::ActionStarted {
            object: 1,
            offer: 0,
            slot: 0,
            needs: EVENING_NEEDS,
            commanded: false,
            ..
        } => {}
        other => panic!("the television still wins, got {other:?}"),
    }
    match &run.log[at - 1].payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (3, Some(sim(2)), 8_000, None, 0)
            );
        }
        other => panic!("core:company while he walks, got {other:?}"),
    }

    // Five seconds of walking at −10 an hour do not move fun. The hour of television adds its
    // rate, 126. Weight is 1 000 − 806.
    let tv_later = (1_000 - (680 + 126)) * 126 - 250;
    assert_eq!(tv_later, 24_194);
    const { assert!(24_194 < ENOUGH) };
    run.advance(later);
    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(matches!(him.place, Place::Room(_)));
    assert_eq!(him.action.map(|action| action.object), Some(3));
    let at = (run.log.iter().rposition(|entry| {
        about(entry, 1)
            && entry.t.as_secs() == later
            && matches!(entry.payload, LogPayload::Company { .. })
    }))
    .expect("her choice once he sits");
    match &run.log[at].payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (3, Some(sim(2)), 8_000, Some(sim(2)), 8_000)
            );
            assert_eq!(row.company - row.avoid, 0);
        }
        other => panic!("core:company once he sits, got {other:?}"),
    }
    assert!(matches!(run.log[at + 1].payload, LogPayload::Idle { .. }));
}

/// The partners' bond starts at the move-in, 23:59:59 of day 0. Familiarity falls by 5 a day.
fn familiarity_at(t: i64) -> i64 {
    const MOVE_IN: i64 = DAY.as_secs() - 1;
    600 - 5 * (t - MOVE_IN) / 86_400
}

/// Tom is told to the table. While he walks, trust 300 is company and not avoid; once he
/// has arrived, avoid applies too. The table's eat is long enough that he still sits at
/// her next choice, and they are not at the same furniture, so no talk moves the bond.
#[test]
fn a_walker_is_company_and_only_a_seated_person_is_avoided() {
    // Affection stays 700. At 18:00 familiarity is 597, so the sum is 1 297.
    assert_eq!(familiarity_at(EVENING), 597);
    assert!(700 + familiarity_at(EVENING) >= 600);
    // She reaches the television after 5 s and watches for 3 600 s. He reaches the hall
    // table after 3 s. The eat of 7 200 s still runs at her next choice, and familiarity
    // has not fallen another step.
    let later = EVENING + 5 + 3_600;
    assert!(EVENING + 3 < later && later < EVENING + 3 + 7_200);
    assert_eq!(familiarity_at(later), 597);

    let social = common::repository_social_with(|social| {
        social["bonds"]["start"]["partner"][1] = json!(300);
    });
    let (mut run, _) = Run::moved_in(company_data_table(social, 7_200));
    run.advance(EVENING - 1);
    run.submit(command(2, 3));
    run.advance(EVENING);

    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(
        matches!(him.place, Place::Walking { arrive, .. } if arrive.as_secs() == EVENING + 3),
        "he is on the way to the table"
    );
    assert_eq!(him.action.map(|action| action.object), Some(3));
    let (at, started) = her_choice(&run);
    match &started.payload {
        LogPayload::ActionStarted {
            object: 1,
            commanded: false,
            ..
        } => {}
        other => panic!("the television still wins while he walks, got {other:?}"),
    }
    match &run.log[at - 1].payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (3, Some(sim(2)), 8_000, None, 0),
                "on the way he is company and not avoid"
            );
        }
        other => panic!("core:company while he walks, got {other:?}"),
    }

    // Five seconds of walking at −10 an hour do not move fun. The hour of television adds its
    // rate, 126, so fun is 806 and the television is 24 194, under 40 000. The choice is idle.
    let tv_later = (1_000 - (680 + 126)) * 126 - 250;
    assert_eq!(tv_later, 24_194);
    const { assert!(24_194 < ENOUGH) };
    run.advance(later);
    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(
        matches!(him.place, Place::Room(_)),
        "he has arrived and still sits"
    );
    assert_eq!(him.action.map(|action| action.object), Some(3));
    let at = (run.log.iter().rposition(|entry| {
        about(entry, 1)
            && entry.t.as_secs() == later
            && matches!(entry.payload, LogPayload::Company { .. })
    }))
    .expect("her choice once he sits");
    match &run.log[at].payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (3, Some(sim(2)), 8_000, Some(sim(2)), 8_000),
                "once he sits, avoid applies and company stays"
            );
        }
        other => panic!("core:company once he sits, got {other:?}"),
    }
    assert!(matches!(run.log[at + 1].payload, LogPayload::Idle { .. }));
}

/// The evening of `evening` on `data`, with the woman's temper at `temper` (from a save with it
/// written in: no input puts one there, and the occasions would need a day to bring it).
fn evening_in(data: Data, temper: Option<i16>) -> Run {
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(EVENING - 1);
    if let Some(temper) = temper {
        let moment = t(EVENING - 1);
        let feel = Feel {
            channels: [0; 4],
            since: moment,
            temper,
            temper_since: moment,
        };
        run.world = common::with_feelings(&run.world, &run.out.year_hashes, &data, &[(1, feel)]);
    }
    run.submit(command(2, 0));
    run.advance(EVENING);
    run
}

fn t(secs: i64) -> sim_core::time::SimTime {
    sim_core::time::SimTime::from_secs(secs)
}

/// Her `core:company` row for the sofa and the object she takes.
fn her_row_and_object(run: &Run) -> (sim_core::log::Company, u16) {
    let (at, started) = her_choice(run);
    let LogPayload::Company { objects } = &run.log[at - 1].payload else {
        panic!("core:company before her choice");
    };
    let LogPayload::ActionStarted { object, .. } = started.payload else {
        panic!("her choice");
    };
    (objects[0].clone(), object)
}

/// ADR-0032 section 5 and acceptance 5. The company is scaled by the chooser's temper,
/// `company · (1000 + s) / 1000` toward 0: -500 halves 8 000 to 4 000, -1000 removes it, +1000
/// doubles it; the avoid stays. By hand in the evening of the sofa (39 750 alone) and a television
/// of 145 an hour (320 · 145 - 250 = 46 150): with him on the sofa she sits there at temper 0
/// (47 750) and takes the television at -500 (43 750). The row of `core:company` names the scaled
/// value, 4 000, also in the second where the temper does not change the choice, and a company of
/// 0 stays a row at -1000. `CHOICE_MOOD` is not touched (the day's mood is 0 here).
#[test]
fn the_temper_halves_the_company_and_the_television_wins() {
    let houses = houses_with_kinds(&kinds_with_tv(145));
    let home = houses
        .house(&DataId::new("test:company").expect("id"))
        .expect("house");
    let there = (30, EVENING_NEEDS, 1);
    let open = (None, None);
    let with_him = |temper: i16, avoid: i64| {
        score(home, &houses, there, open, no_mood, temper, |object| {
            if object == 0 {
                Bonus {
                    company: 8_000,
                    avoid,
                }
            } else {
                Bonus::NONE
            }
        })
    };
    let tv = 320 * 145 - 250;
    assert_eq!(tv, 46_150);
    assert_eq!(points_of(&with_him(0, 0), 1, 0), tv);
    // 8 000 at temper 0, 4 000 at -500, 0 at -1 000, 16 000 at +1 000; toward 0 in between.
    for (temper, company) in [
        (0, 8_000),
        (-500, 4_000),
        (-1000, 0),
        (1000, 16_000),
        (-1, 7_992),
    ] {
        assert_eq!(
            points_of(&with_him(temper, 0), 0, 0),
            SOFA_ALONE + company,
            "temper {temper}"
        );
    }
    // The avoid is not scaled: at -500 the company 4 000 against the avoid 8 000.
    assert_eq!(
        points_of(&with_him(-500, 8_000), 0, 0),
        SOFA_ALONE + 4_000 - 8_000
    );
    assert_eq!(
        points_of(&with_him(1000, 8_000), 0, 0),
        SOFA_ALONE + 16_000 - 8_000
    );
    let take = |temper: i16| {
        let scored = with_him(temper, 0);
        match choose((home, &houses), (30, EVENING_NEEDS), &scored, &[], None) {
            Choice::Take(chosen) => (chosen.object, chosen.points),
            other => panic!("a choice, got {other:?}"),
        }
    };
    assert_eq!(take(0), (0, SOFA_ALONE + 8_000));
    assert_eq!(take(-500), (1, tv));
    assert_eq!(SOFA_ALONE + 4_000, 43_750);

    // The same in a run of the repository's test house (a television of 126, 40 070 alone), her
    // temper from a save: the row names the scaled company; at -500 she still sits with him
    // (39 750 + 4 000 = 43 750 against 40 070), at -1 000 the company is 0, the row stays, and
    // the television (40 070) beats the sofa alone (39 750).
    let data = company_data(common::repository_social());
    let plain = evening_in(data.clone(), None);
    let (row, object) = her_row_and_object(&plain);
    assert_eq!((object, row.company, row.liked), (0, 8_000, Some(sim(2))));
    let low = evening_in(data.clone(), Some(-500));
    let (row, object) = her_row_and_object(&low);
    assert_eq!(
        (object, row.company, row.liked, row.avoid),
        (0, 4_000, Some(sim(2)), 0)
    );
    let gone = evening_in(data, Some(-1000));
    let (row, object) = her_row_and_object(&gone);
    assert_eq!((object, row.company, row.liked), (1, 0, Some(sim(2))));
}
