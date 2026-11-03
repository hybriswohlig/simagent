//! The house part of the state and its checks on load (ADR-0024 section 7 and its addendum
//! of 2026-11-02): a legal state of a household in the repository's flat passes, and each
//! broken rule is named.

mod common;

use std::collections::BTreeMap;

use sim_core::data_id::DataId;
use sim_core::house::data::{HouseData, Need};
use sim_core::house::need::NeedSample;
use sim_core::house::state::{
    Action, Broken, Decisions, Due, Duty, HouseState, InHouse, MovedIn, Place, consistent,
    crossings,
};
use sim_core::ids::{HouseholdId, SimId};
use sim_core::time::{SimTime, YEAR, day_index};

/// The second the loaded world runs next: day 400, 12:00.
const NOW: i64 = 400 * 86_400 + 12 * 3_600;

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn household(id: u64) -> HouseholdId {
    HouseholdId::try_from(id).expect("valid")
}

fn houses() -> HouseData {
    common::repository_houses()
}

/// Sims 1, 2 and 6 (adults), 3 (8 years) and 5 (5 years) in household 1, sim 4 (an adult) in
/// household 2; households 1 and 2 live, 3 has ended.
fn member(id: SimId) -> Option<(HouseholdId, SimTime)> {
    let born = |years: i64| t(NOW - years * YEAR.as_secs());
    match id.get() {
        1 | 2 | 6 => Some((household(1), born(40))),
        3 => Some((household(1), born(8))),
        4 => Some((household(2), born(40))),
        5 => Some((household(1), born(5))),
        _ => None,
    }
}

fn alive(id: HouseholdId) -> bool {
    id.get() <= 2
}

/// Morning values at 6:00 of today with the awake rates of an adult.
fn needs() -> [NeedSample; 5] {
    let morning = t(NOW - 6 * 3_600);
    [(600, -100), (900, -40), (500, -120), (700, -30), (800, -60)]
        .map(|(value, rate)| NeedSample::new(value, morning, rate))
}

/// The same with a child's awake energy (6 to 12).
fn child_needs() -> [NeedSample; 5] {
    let mut needs = needs();
    needs[1] = NeedSample::new(900, needs[1].at(), -50);
    needs
}

/// Fun at 800 ten minutes ago, rising by `rate` since: watching or reading from then on.
fn fun_since(rate: i32) -> NeedSample {
    NeedSample::new(800, t(NOW - 600), rate)
}

fn person(place: Place, action: Option<Action>) -> InHouse {
    InHouse {
        needs: needs(),
        place,
        action,
        due: None,
        decisions: Decisions {
            day: day_index(t(NOW)),
            count: 3,
        },
    }
}

/// The flat (rooms: hall 0, kitchen 1, living 2, bath 3, bedroom 4, kids 5; objects:
/// table 0, fridge 1, sofa 2, tv 3, bookshelf 4, toilet 5, shower 6, bed 7, single bed 8,
/// toy chest 9). Sim 2 watches television on place 0 (fun +400 for ten minutes), sim 3 (8)
/// walks the 7 s from the living room to the fridge, whose one place is so taken, and sim 1
/// idles in the living room waiting for the fridge.
fn legal() -> HouseState {
    let tv = Action {
        object: 3,
        offer: 0,
        slot: 0,
    };
    let fridge = Action {
        object: 1,
        offer: 0,
        slot: 0,
    };
    let walk = Place::Walking {
        from: 2,
        to: 1,
        depart: t(NOW - 3),
        arrive: t(NOW + 4),
    };
    let mut watching = person(Place::Room(2), Some(tv));
    watching.needs[4] = fun_since(400);
    let mut walking = person(walk, Some(fridge));
    walking.needs = child_needs();
    HouseState {
        households: BTreeMap::from([(
            household(1),
            MovedIn {
                house: DataId::new("core:flat").expect("id"),
                morning: true,
            },
        )]),
        sims: BTreeMap::from([
            (sim(1), person(Place::Room(2), None)),
            (sim(2), watching),
            (sim(3), walking),
        ]),
        waits: BTreeMap::from([((household(1), 1), vec![(t(NOW - 60), sim(1))])]),
        holds: BTreeMap::new(),
    }
}

fn checked(change: impl FnOnce(&mut HouseState)) -> Result<(), Broken> {
    let mut state = legal();
    change(&mut state);
    consistent(&state, Some(&houses()), t(NOW), alive, member)
}

fn in_house(state: &mut HouseState, id: u64) -> &mut InHouse {
    state.sims.get_mut(&sim(id)).expect("in the house")
}

#[test]
fn a_legal_house_passes() {
    assert_eq!(checked(|_| {}), Ok(()));
    // The empty house of every run without `MoveIn`, with or without house data.
    let empty = HouseState::default();
    assert_eq!(consistent(&empty, None, t(NOW), alive, member), Ok(()));
    // Before the first morning nobody has needs; a child of 5 never has.
    let before = checked(|s| {
        s.sims.clear();
        s.waits.clear();
        s.households
            .values_mut()
            .for_each(|moved| moved.morning = false);
    });
    assert_eq!(before, Ok(()));
    // A hold for the waiter instead of the waiting position; away with no rate.
    let held = checked(|s| {
        s.waits.clear();
        s.holds.insert((household(1), 7, 1), sim(1));
    });
    assert_eq!(held, Ok(()));
    let away = checked(|s| {
        let one = in_house(s, 1);
        one.place = Place::Away;
        one.needs = one
            .needs
            .map(|need| NeedSample::new(need.value(), t(NOW), 0));
        s.waits.clear();
    });
    assert_eq!(away, Ok(()));
}

#[test]
fn the_households_are_checked() {
    let many = checked(|s| {
        for id in 2..=65 {
            let moved = MovedIn {
                house: DataId::new("core:flat").expect("id"),
                morning: false,
            };
            s.households.insert(household(id), moved);
        }
    });
    assert_eq!(many, Err(Broken::TooManyHouseholds));
    let ended = checked(|s| {
        let moved = MovedIn {
            house: DataId::new("core:flat").expect("id"),
            morning: false,
        };
        s.households.insert(household(3), moved);
    });
    assert_eq!(ended, Err(Broken::HouseholdGone));
    let unknown = checked(|s| {
        s.households.get_mut(&household(1)).expect("moved in").house =
            DataId::new("core:castle").expect("id");
    });
    assert_eq!(unknown, Err(Broken::HouseUnknown));
    let no_data = consistent(&legal(), None, t(NOW), alive, member);
    assert_eq!(no_data, Err(Broken::HouseUnknown));
}

#[test]
fn the_persons_are_checked() {
    // Not living, in a household not moved in, before its morning, under 6.
    let dead = checked(|s| {
        s.sims.insert(sim(9), person(Place::Room(0), None));
    });
    assert_eq!(dead, Err(Broken::NotMember));
    let elsewhere = checked(|s| {
        s.sims.insert(sim(4), person(Place::Room(0), None));
    });
    assert_eq!(elsewhere, Err(Broken::NotMember));
    let early = checked(|s| {
        s.households
            .values_mut()
            .for_each(|moved| moved.morning = false);
    });
    assert_eq!(early, Err(Broken::NotMember));
    let young = checked(|s| {
        s.sims.insert(sim(5), person(Place::Room(0), None));
    });
    assert_eq!(young, Err(Broken::TooYoung));
    // A value above 1000, a sample from a later second.
    let full = checked(|s| {
        let one = in_house(s, 1);
        one.needs[4] =
            serde_json::from_str(&format!(r#"{{"value":1001,"at":{},"rate":0}}"#, NOW - 1))
                .expect("a sample");
    });
    assert_eq!(full, Err(Broken::NeedValue));
    let later = checked(|s| in_house(s, 1).needs[0] = NeedSample::new(600, t(NOW + 1), -100));
    assert_eq!(later, Err(Broken::NeedTime));
    assert_eq!(
        checked(|s| in_house(s, 1).needs[0] = NeedSample::new(600, t(NOW), -100)),
        Ok(())
    );
}

#[test]
fn places_and_actions_are_checked() {
    assert_eq!(
        checked(|s| in_house(s, 1).place = Place::Room(6)),
        Err(Broken::RoomUnknown)
    );
    let backwards = checked(|s| {
        in_house(s, 3).place = Place::Walking {
            from: 2,
            to: 1,
            depart: t(NOW + 4),
            arrive: t(NOW + 3),
        };
    });
    assert_eq!(backwards, Err(Broken::Walk));
    // A walk with an action leads to its object's room, one without to the entrance.
    let astray = checked(|s| {
        in_house(s, 3).place = Place::Walking {
            from: 2,
            to: 3,
            depart: t(NOW - 3),
            arrive: t(NOW + 3),
        };
    });
    assert_eq!(astray, Err(Broken::Walk));
    // Walking out instead of to the fridge, which sim 1 then no longer waits for: to the
    // entrance 3 s, out 5 s.
    let out = |to: u8| {
        checked(move |s| {
            s.waits.clear();
            let three = in_house(s, 3);
            three.action = None;
            three.place = Place::Walking {
                from: 2,
                to,
                depart: t(NOW - 3),
                arrive: t(NOW + 5),
            };
        })
    };
    assert_eq!(out(0), Ok(()));
    assert_eq!(out(1), Err(Broken::Walk));
    // Reading (fun +300) or watching (+400) in the living room.
    let action = |object: u16, offer: u16, slot: u8| {
        checked(move |s| {
            let one = in_house(s, 1);
            one.action = Some(Action {
                object,
                offer,
                slot,
            });
            one.needs[4] = fun_since(if object == 3 { 400 } else { 300 });
            s.waits.clear();
        })
    };
    assert_eq!(action(4, 0, 1), Ok(()));
    assert_eq!(action(10, 0, 0), Err(Broken::ObjectUnknown));
    assert_eq!(action(4, 1, 0), Err(Broken::OfferUnknown));
    assert_eq!(action(4, 0, 2), Err(Broken::SlotUnknown));
    // The place sim 2 holds at the tv, and a hold on a taken place.
    assert_eq!(action(3, 0, 0), Err(Broken::SlotTaken));
    let held = checked(|s| {
        s.waits.clear();
        s.holds.insert((household(1), 3, 0), sim(1));
    });
    assert_eq!(held, Err(Broken::SlotTaken));
}

/// A walk is as long as the house makes it and began before the clock; the rates are the
/// awake ones, at an object those the offer sets the offer's, written down together, each by
/// the age band of its second (ADR-0026 sections 1 and 4).
#[test]
fn walks_and_rates_are_checked() {
    let walk = |depart: i64, arrive: i64| {
        checked(move |s| {
            in_house(s, 3).place = Place::Walking {
                from: 2,
                to: 1,
                depart: t(depart),
                arrive: t(arrive),
            };
        })
    };
    assert_eq!(walk(NOW - 1, NOW + 6), Ok(()));
    assert_eq!(walk(NOW - 3, NOW + 5), Err(Broken::Walk));
    assert_eq!(walk(NOW, NOW + 7), Err(Broken::Walk));
    let rate = |change: fn(&mut HouseState)| checked(change);
    // Idle with another hunger rate; a child with an adult's energy.
    let hungry = rate(|s| in_house(s, 1).needs[0] = NeedSample::new(600, t(NOW - 60), -90));
    assert_eq!(hungry, Err(Broken::Rate));
    let grown = rate(|s| in_house(s, 3).needs[1] = NeedSample::new(900, t(NOW - 60), -40));
    assert_eq!(grown, Err(Broken::Rate));
    // At 40 at that second sim 1 has the adult rate; one written when she was 6 the child's.
    let young = rate(|s| {
        let at = t(NOW - 34 * YEAR.as_secs());
        in_house(s, 1).needs[1] = NeedSample::new(900, at, -50);
    });
    assert_eq!(young, Ok(()));
    // Watching with fun at the awake rate, or with hunger rising.
    let bored = rate(|s| in_house(s, 2).needs[4] = NeedSample::new(800, t(NOW - 60), -60));
    assert_eq!(bored, Err(Broken::Rate));
    let fed = rate(|s| in_house(s, 2).needs[0] = NeedSample::new(600, t(NOW - 60), 400));
    assert_eq!(fed, Err(Broken::Rate));
    // Asleep in the bed every need has the sleeping rate of an adult, all from one second.
    let asleep = |late: usize| {
        checked(move |s| {
            s.waits.clear();
            let one = in_house(s, 1);
            one.place = Place::Room(4);
            one.action = Some(Action {
                object: 7,
                offer: 0,
                slot: 0,
            });
            let rates = [-40, 85, -40, -10, 0];
            for (at, need) in one.needs.iter_mut().enumerate() {
                let since = if at == late { NOW - 60 } else { NOW - 600 };
                *need = NeedSample::new(need.value(), t(since), rates[at]);
            }
        })
    };
    assert_eq!(asleep(5), Ok(()));
    assert_eq!(asleep(3), Err(Broken::Rate));
}

#[test]
fn away_due_and_decisions_are_checked() {
    let away = |change: fn(&mut InHouse)| {
        checked(move |s| {
            let one = in_house(s, 1);
            one.place = Place::Away;
            one.needs = one
                .needs
                .map(|need| NeedSample::new(need.value(), t(NOW), 0));
            change(one);
            s.waits.clear();
        })
    };
    assert_eq!(away(|_| {}), Ok(()));
    let rate = away(|one| one.needs[2] = NeedSample::new(500, t(NOW), -1));
    assert_eq!(rate, Err(Broken::AwayBusy));
    let busy = away(|one| {
        one.action = Some(Action {
            object: 4,
            offer: 0,
            slot: 0,
        });
    });
    assert_eq!(busy, Err(Broken::AwayBusy));
    let duty = away(|one| {
        one.due = Some(Due {
            place: Duty::Work,
            depart: t(NOW - 600),
            back: t(NOW + 3_600),
        });
    });
    assert_eq!(duty, Err(Broken::AwayBusy));
    // A postponed departure of someone at home: before its return.
    let postponed = |depart: i64, back: i64| {
        checked(move |s| {
            in_house(s, 2).due = Some(Due {
                place: Duty::School,
                depart: t(depart),
                back: t(back),
            });
        })
    };
    assert_eq!(postponed(NOW - 600, NOW + 3_600), Ok(()));
    assert_eq!(postponed(NOW + 3_600, NOW + 3_600), Err(Broken::DueOrder));
    // At most 200 choices on a day not after today.
    let decisions = |day: i64, count: u16| {
        checked(move |s| in_house(s, 1).decisions = Decisions { day, count })
    };
    let today = day_index(t(NOW));
    assert_eq!(decisions(today, 200), Ok(()));
    assert_eq!(decisions(today - 3, 7), Ok(()));
    assert_eq!(decisions(today, 201), Err(Broken::Decisions));
    assert_eq!(decisions(today + 1, 0), Err(Broken::Decisions));
}

#[test]
fn waiters_and_holds_are_checked() {
    let waits = |list: Vec<(i64, u64)>| {
        checked(move |s| {
            let list = list.into_iter().map(|(at, id)| (t(at), sim(id))).collect();
            s.waits.insert((household(1), 1), list);
        })
    };
    assert_eq!(waits(vec![(NOW - 60, 1)]), Ok(()));
    // Not in the house, of another household, busy, away or walking out.
    assert_eq!(waits(vec![(NOW - 60, 9)]), Err(Broken::Waiter));
    assert_eq!(waits(vec![(NOW - 60, 2)]), Err(Broken::Waiter));
    let other = checked(|s| {
        s.households.insert(
            household(2),
            MovedIn {
                house: DataId::new("core:house").expect("id"),
                morning: true,
            },
        );
        s.sims.insert(sim(4), person(Place::Room(0), None));
        s.waits
            .insert((household(1), 1), vec![(t(NOW - 60), sim(4))]);
    });
    assert_eq!(other, Err(Broken::Waiter));
    let gone = checked(|s| {
        let one = in_house(s, 1);
        one.place = Place::Away;
        one.needs = one
            .needs
            .map(|need| NeedSample::new(need.value(), t(NOW), 0));
    });
    assert_eq!(gone, Err(Broken::Waiter));
    let leaving = checked(|s| {
        in_house(s, 1).place = Place::Walking {
            from: 2,
            to: 0,
            depart: t(NOW - 3),
            arrive: t(NOW + 5),
        };
    });
    assert_eq!(leaving, Err(Broken::Waiter));
    // An object the house lacks, a waiting list out of order, a person queued twice.
    let nowhere = checked(|s| {
        let list = s.waits.remove(&(household(1), 1)).expect("a list");
        s.waits.insert((household(1), 10), list);
    });
    assert_eq!(nowhere, Err(Broken::ObjectUnknown));
    let order = checked(|s| {
        s.sims.insert(sim(6), person(Place::Room(2), None));
        s.waits.insert(
            (household(1), 1),
            vec![(t(NOW - 10), sim(6)), (t(NOW - 60), sim(1))],
        );
    });
    assert_eq!(order, Err(Broken::WaitOrder));
    let tie = checked(|s| {
        s.sims.insert(sim(6), person(Place::Room(2), None));
        s.waits.insert(
            (household(1), 1),
            vec![(t(NOW - 60), sim(1)), (t(NOW - 60), sim(6))],
        );
    });
    assert_eq!(tie, Ok(()));
    let twice = checked(|s| {
        s.holds.insert((household(1), 8, 0), sim(1));
    });
    assert_eq!(twice, Err(Broken::QueuedTwice));
    let two_lists = checked(|s| {
        s.waits
            .insert((household(1), 8), vec![(t(NOW - 30), sim(1))]);
    });
    assert_eq!(two_lists, Err(Broken::QueuedTwice));
    // A hold on a place the object lacks.
    let no_place = checked(|s| {
        s.waits.clear();
        s.holds.insert((household(1), 8, 1), sim(1));
    });
    assert_eq!(no_place, Err(Broken::SlotUnknown));
    // Waiting for an object with a free place: a freed place is held for the first waiter
    // in the same handler (ADR-0026 section 5).
    let free = checked(|s| {
        let three = in_house(s, 3);
        three.action = None;
        three.place = Place::Room(2);
    });
    assert_eq!(free, Err(Broken::WaitFree));
    let held_free = checked(|s| {
        let three = in_house(s, 3);
        three.action = None;
        three.place = Place::Room(2);
        s.sims.insert(sim(6), person(Place::Room(2), None));
        s.holds.insert((household(1), 1, 0), sim(6));
    });
    assert_eq!(held_free, Ok(()));
}

/// The crossings a change plans (ADR-0026 section 1), by hand: the morning values at 6:00
/// with an adult's awake rates fall past every threshold below them, `at + ⌈3600 · (value −
/// level) / |rate|⌉`, in the order of the needs and of the thresholds; two hours later those
/// at or before that second are gone; a rising need and a person away plan none.
#[test]
fn the_crossings_after_a_change() {
    let six = t(NOW - 6 * 3_600);
    let at = |secs: i64| t(NOW - 6 * 3_600 + secs);
    let morning = person(Place::Room(0), None);
    let all: Vec<(Need, u16, SimTime)> = crossings(&morning, six).collect();
    let expected = [
        (Need::Hunger, 400, 7_200),
        (Need::Hunger, 150, 16_200),
        (Need::Hunger, 50, 19_800),
        (Need::Energy, 700, 18_000),
        (Need::Energy, 400, 45_000),
        (Need::Energy, 150, 67_500),
        (Need::Energy, 50, 76_500),
        (Need::Bladder, 400, 3_000),
        (Need::Bladder, 150, 10_500),
        (Need::Bladder, 50, 13_500),
        (Need::Hygiene, 400, 36_000),
        (Need::Hygiene, 150, 66_000),
        (Need::Hygiene, 50, 78_000),
        (Need::Fun, 700, 6_000),
        (Need::Fun, 400, 24_000),
        (Need::Fun, 150, 39_000),
        (Need::Fun, 50, 45_000),
    ]
    .map(|(need, level, secs)| (need, level, at(secs)));
    assert_eq!(all, expected);
    // At 8:00 hunger is at 400 in that very second; it, bladder at 400 and fun at 700 have
    // come.
    let later: Vec<_> = crossings(&morning, at(7_200)).collect();
    assert_eq!(later.len(), 14);
    assert_eq!(
        later,
        expected[1..7]
            .iter()
            .chain(&expected[8..13])
            .chain(&expected[14..])
            .copied()
            .collect::<Vec<_>>()
    );
    // Eating from 8:00: hunger rises, the others fall as before.
    let mut eating = morning.clone();
    eating.needs[0] = eating.needs[0].with_rate(at(7_200), 2_400);
    let fed: Vec<_> = crossings(&eating, at(7_200)).collect();
    assert_eq!(fed, later[2..]);
    // Away every rate is 0.
    let mut away = person(Place::Away, None);
    away.needs = away.needs.map(|need| need.with_rate(six, 0));
    assert_eq!(crossings(&away, six).count(), 0);
}
