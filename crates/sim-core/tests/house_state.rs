//! The house part of the state and its checks on load (ADR-0024 section 7 and its addendum
//! of 2026-11-02): a legal state of a household in the repository's flat passes, and each
//! broken rule is named.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use sim_core::data_id::DataId;
use sim_core::house::data::{HouseData, Need};
use sim_core::house::need::NeedSample;
use sim_core::house::state::{
    Action, Broken, Decisions, Due, Duty, HouseState, InHouse, MovedIn, Place, children_consistent,
    consistent, crossings,
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
        collapsed: false,
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
        filler: false,
    };
    let fridge = Action {
        object: 1,
        offer: 0,
        slot: 0,
        filler: false,
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
    checked_with(&houses(), change)
}

/// Checked as after an advance: no input acted at the clock.
fn checked_with(houses: &HouseData, change: impl FnOnce(&mut HouseState)) -> Result<(), Broken> {
    checked_after(houses, None, change)
}

/// Checked after an input at the clock, `choices` the sims whose choice it left due at `NOW`
/// (ADR-0024, addendum of 2026-11-04); `None` for no input at the clock.
fn checked_after(
    houses: &HouseData,
    choices: Option<&[u64]>,
    change: impl FnOnce(&mut HouseState),
) -> Result<(), Broken> {
    let mut state = legal();
    change(&mut state);
    let due: Option<BTreeSet<SimId>> = choices.map(|ids| ids.iter().map(|&id| sim(id)).collect());
    consistent(&state, Some(houses), t(NOW), due.as_ref(), alive, member)
}

/// The repository's houses, but fun rising awake from 67 on (+10 an hour).
fn houses_with_rising_fun() -> HouseData {
    let mut needs: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/house/needs.json")).expect("json");
    needs["awake"]["fun"][3] = serde_json::json!(10);
    common::houses_from(
        &needs.to_string(),
        include_str!("../../../data/house/day.json"),
        include_str!("../../../data/house/kinds.json"),
        &[
            include_str!("../../../data/house/houses/flat.json"),
            include_str!("../../../data/house/houses/house.json"),
            include_str!("../../../data/house/houses/terraced.json"),
        ],
    )
}

fn in_house(state: &mut HouseState, id: u64) -> &mut InHouse {
    state.sims.get_mut(&sim(id)).expect("in the house")
}

#[test]
fn a_legal_house_passes() {
    assert_eq!(checked(|_| {}), Ok(()));
    // The empty house of every run without `MoveIn`, with or without house data.
    let empty = HouseState::default();
    assert_eq!(
        consistent(&empty, None, t(NOW), None, alive, member),
        Ok(())
    );
    // Before the first morning nobody has needs; a child of 5 never has.
    let before = checked(|s| {
        s.sims.clear();
        s.waits.clear();
        s.households
            .values_mut()
            .for_each(|moved| moved.morning = false);
    });
    assert_eq!(before, Ok(()));
    // A hold for the waiter instead of the waiting position, her choice due after an input at
    // the clock; away with no rate.
    let held = checked_after(&houses(), Some(&[1]), |s| {
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
    let no_data = consistent(&legal(), None, t(NOW), None, alive, member);
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
    // entrance 3 s, out 5 s, every need still from the departure on (ADR-0024 section 4).
    let out = |to: u8, rate: i32| {
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
            three.needs = three
                .needs
                .map(|need| NeedSample::new(need.value(), t(NOW - 3), rate));
        })
    };
    assert_eq!(out(0, 0), Ok(()));
    assert_eq!(out(1, 0), Err(Broken::Walk));
    assert_eq!(out(0, -30), Err(Broken::AwayBusy));
    // No input begins a walk out (leaving is an event): one begun in the next second is refused
    // even after an input at the clock.
    let out_now = checked_after(&houses(), Some(&[]), |s| {
        s.waits.clear();
        let three = in_house(s, 3);
        three.action = None;
        three.place = Place::Walking {
            from: 2,
            to: 0,
            depart: t(NOW),
            arrive: t(NOW + 8),
        };
        three.needs = three
            .needs
            .map(|need| NeedSample::new(need.value(), t(NOW), 0));
    });
    assert_eq!(out_now, Err(Broken::Walk));
    // Leaving writes down all five needs at the second of the departure, the start of the walk.
    let late_need = checked(|s| {
        s.waits.clear();
        let three = in_house(s, 3);
        three.action = None;
        three.place = Place::Walking {
            from: 2,
            to: 0,
            depart: t(NOW - 3),
            arrive: t(NOW + 5),
        };
        three.needs = three
            .needs
            .map(|need| NeedSample::new(need.value(), t(NOW - 2), 0));
    });
    assert_eq!(late_need, Err(Broken::Left));
    // Reading (fun +300) or watching (+400) in the living room.
    let action = |object: u16, offer: u16, slot: u8| {
        checked(move |s| {
            let one = in_house(s, 1);
            one.action = Some(Action {
                object,
                offer,
                slot,
                filler: false,
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

/// A held place lasts only the second of the choice it plans (ADR-0026 section 5), and only an
/// input at the clock leaves one, for one whose choice it left due at `NOW` (ADR-0024, addendum
/// of 2026-11-04). Sim 3 idles in the living room instead of walking to the fridge, whose one
/// place is held for sim 1, who no longer waits: that loads only with her choice due after an
/// input. A choice is never due for one who waits, since choosing takes a person off every list:
/// sim 1 waiting for the fridge with her choice due is refused.
#[test]
fn holds_and_choices_are_checked() {
    let held = |s: &mut HouseState| {
        s.waits.clear();
        let three = in_house(s, 3);
        three.place = Place::Room(2);
        three.action = None;
        s.holds.insert((household(1), 1, 0), sim(1));
    };
    assert_eq!(checked_after(&houses(), Some(&[1]), held), Ok(()));
    assert_eq!(
        checked_after(&houses(), Some(&[]), held),
        Err(Broken::Choice)
    );
    assert_eq!(checked_after(&houses(), None, held), Err(Broken::Choice));
    assert_eq!(checked_after(&houses(), Some(&[]), |_| {}), Ok(()));
    assert_eq!(
        checked_after(&houses(), Some(&[1]), |_| {}),
        Err(Broken::Choice)
    );
}

/// A walk is as long as the house makes it and began before the clock, or in the next second
/// only after an input at the clock (a command, ADR-0024 section 5): an advance runs every
/// event of a second; the rates are the awake ones, at an object those the offer sets the
/// offer's, written down together, each by the age band of its second (ADR-0026 sections 1
/// and 4).
#[test]
fn walks_and_rates_are_checked() {
    let walk = |choices: Option<&[u64]>, depart: i64, arrive: i64| {
        checked_after(&houses(), choices, move |s| {
            in_house(s, 3).place = Place::Walking {
                from: 2,
                to: 1,
                depart: t(depart),
                arrive: t(arrive),
            };
        })
    };
    assert_eq!(walk(None, NOW - 1, NOW + 6), Ok(()));
    assert_eq!(walk(None, NOW - 3, NOW + 5), Err(Broken::Walk));
    assert_eq!(walk(None, NOW, NOW + 7), Err(Broken::Walk));
    assert_eq!(walk(Some(&[]), NOW, NOW + 7), Ok(()));
    assert_eq!(walk(Some(&[]), NOW + 1, NOW + 8), Err(Broken::Walk));
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
    // Asleep in the bed every need has the sleeping rate of an adult, all from one second,
    // and only in her night (ADR-0026, addendum of 2026-11-04): from 22:00 to 6:00 at 40, so at
    // 23:00 of today a sleep begun at 22:50 loads; one begun at 11:50, by day, does not.
    let night = NOW + 11 * 3_600;
    let asleep = |now: i64, late: usize| {
        let mut state = legal();
        state.waits.clear();
        let one = in_house(&mut state, 1);
        one.place = Place::Room(4);
        one.action = Some(Action {
            object: 7,
            offer: 0,
            slot: 0,
            filler: false,
        });
        let rates = [-40, 85, -40, -10, 0];
        for (at, need) in one.needs.iter_mut().enumerate() {
            let since = if at == late { now - 60 } else { now - 600 };
            *need = NeedSample::new(need.value(), t(since), rates[at]);
        }
        consistent(&state, Some(&houses()), t(now), None, alive, member)
    };
    assert_eq!(asleep(night, 5), Ok(()));
    assert_eq!(asleep(night, 3), Err(Broken::Rate));
    assert_eq!(asleep(NOW, 5), Err(Broken::Sleep));
    // On the way to the bed, from the living room (8 s), only from a second of her night.
    let to_bed = |depart: i64| {
        let mut state = legal();
        state.waits.clear();
        let one = in_house(&mut state, 1);
        one.place = Place::Walking {
            from: 2,
            to: 4,
            depart: t(depart),
            arrive: t(depart + 8),
        };
        one.action = Some(Action {
            object: 7,
            offer: 0,
            slot: 0,
            filler: false,
        });
        consistent(&state, Some(&houses()), t(depart + 3), None, alive, member)
    };
    assert_eq!(to_bed(night - 3), Ok(()));
    assert_eq!(to_bed(NOW - 3), Err(Broken::Sleep));
    assert_eq!(to_bed(NOW + 10 * 3_600 - 1), Err(Broken::Sleep));
    assert_eq!(to_bed(NOW + 10 * 3_600), Ok(()));
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
            filler: false,
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
    // Away all five needs are written down at one second, that of leaving.
    let apart = away(|one| one.needs[3] = NeedSample::new(700, t(NOW - 1), 0));
    assert_eq!(apart, Err(Broken::Left));
    // A postponed departure of someone at home: before its return, while an action raises a
    // need that was critical at the departure (ADR-0024 section 4). Sim 2 watches with fun
    // at 100 then, rising by 400 an hour: 126 four minutes later, 166 at the clock; once she
    // came to the television after it, her fun's second is after the departure.
    let postponed = |depart: i64, back: i64, fun: u16| {
        checked(move |s| {
            let two = in_house(s, 2);
            two.needs[4] = NeedSample::new(fun, t(NOW - 600), 400);
            two.due = Some(Due {
                place: Duty::School,
                depart: t(depart),
                back: t(back),
            });
        })
    };
    assert_eq!(postponed(NOW - 600, NOW + 3_600, 100), Ok(()));
    assert_eq!(postponed(NOW - 360, NOW + 3_600, 100), Ok(()));
    assert_eq!(postponed(NOW - 900, NOW + 3_600, 100), Ok(()));
    assert_eq!(
        postponed(NOW + 3_600, NOW + 3_600, 100),
        Err(Broken::DueOrder)
    );
    assert_eq!(postponed(NOW - 600, NOW + 3_600, 151), Err(Broken::Due));
    assert_eq!(postponed(NOW, NOW + 3_600, 100), Err(Broken::Due));
    // Came to the television after the departure with fun 200: fun only fell on the way
    // (awake −60 an hour at every age), so it was at least 200 then, not critical. Where fun
    // may rise awake (from 67 on, in other data), its value then is open.
    assert_eq!(postponed(NOW - 900, NOW + 3_600, 200), Err(Broken::Due));
    let rising = houses_with_rising_fun();
    let open = checked_with(&rising, |s| {
        let two = in_house(s, 2);
        two.needs[4] = NeedSample::new(200, t(NOW - 600), 400);
        two.due = Some(Due {
            place: Duty::School,
            depart: t(NOW - 900),
            back: t(NOW + 3_600),
        });
    });
    assert_eq!(open, Ok(()));
    // On the way to the fridge (sim 3, a child) since 3 s with hunger at 0 since the morning:
    // its departure 1 s ago may wait; a need written down after it may not, a walk does not
    // begin after a waiting departure.
    let on_the_way = |hunger_at: i64| {
        checked(move |s| {
            s.waits.clear();
            let three = in_house(s, 3);
            three.needs[0] = NeedSample::new(0, t(hunger_at), -100);
            three.due = Some(Due {
                place: Duty::School,
                depart: t(NOW - 1),
                back: t(NOW + 3_600),
            });
        })
    };
    assert_eq!(on_the_way(NOW - 3_600), Ok(()));
    assert_eq!(on_the_way(NOW), Err(Broken::Due));
    // Idle, or walking out, nothing postpones a departure.
    let idle = checked(|s| {
        s.waits.clear();
        in_house(s, 1).due = Some(Due {
            place: Duty::Work,
            depart: t(NOW - 600),
            back: t(NOW + 3_600),
        });
    });
    assert_eq!(idle, Err(Broken::Due));
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

/// One lying on the floor (ADR-0026, addendum of 2026-11-04 on a need at 0): sim 1 fell in the
/// living room ten minutes ago, every need written down then at the repository's sleeping rates
/// of an adult (hunger −40, bladder −40, hygiene −10, fun 0) and energy at 0 rising 50 an hour,
/// off the waiting list. That passes, with a waiting departure too; waiting or holding a place,
/// with an action, away, with energy not at 0 when written down, or at the awake rates is
/// refused.
#[test]
fn one_lying_is_checked() {
    let fell = t(NOW - 600);
    let lying = |change: fn(&mut InHouse)| {
        checked(move |s| {
            s.waits.clear();
            let one = in_house(s, 1);
            one.collapsed = true;
            one.needs = [(600, -40), (0, 50), (500, -40), (700, -10), (800, 0)]
                .map(|(value, rate)| NeedSample::new(value, fell, rate));
            change(one);
        })
    };
    assert_eq!(lying(|_| {}), Ok(()));
    let due = lying(|one| {
        one.due = Some(Due {
            place: Duty::Work,
            depart: t(NOW - 300),
            back: t(NOW + 3_600),
        });
    });
    assert_eq!(due, Ok(()));
    let waiting = checked(|s| {
        let one = in_house(s, 1);
        one.collapsed = true;
        one.needs = [(600, -40), (0, 50), (500, -40), (700, -10), (800, 0)]
            .map(|(value, rate)| NeedSample::new(value, fell, rate));
    });
    assert_eq!(waiting, Err(Broken::Waiter));
    let holding = checked_after(&houses(), Some(&[1]), |s| {
        s.waits.clear();
        s.holds.insert((household(1), 7, 1), sim(1));
        let one = in_house(s, 1);
        one.collapsed = true;
        one.needs = [(600, -40), (0, 50), (500, -40), (700, -10), (800, 0)]
            .map(|(value, rate)| NeedSample::new(value, fell, rate));
    });
    assert_eq!(holding, Err(Broken::Waiter));
    let busy = lying(|one| {
        one.action = Some(Action {
            object: 3,
            offer: 0,
            slot: 1,
            filler: false,
        });
    });
    assert_eq!(busy, Err(Broken::Collapse));
    let away = lying(|one| {
        one.place = Place::Away;
        one.needs = (one.needs).map(|need| NeedSample::new(need.value(), t(NOW - 600), 0));
    });
    assert_eq!(away, Err(Broken::Collapse));
    let rested = lying(|one| one.needs[1] = NeedSample::new(5, t(NOW - 600), 50));
    assert_eq!(rested, Err(Broken::Collapse));
    let awake = lying(|one| one.needs[2] = NeedSample::new(500, t(NOW - 600), -120));
    assert_eq!(awake, Err(Broken::Rate));
    let slow = lying(|one| one.needs[1] = NeedSample::new(0, t(NOW - 600), 85));
    assert_eq!(slow, Err(Broken::Rate));
}

/// While a departure waits (ADR-0024 section 4), a need at 0 may write down other needs later,
/// but only as a run does (ADR-0026, addendum of 2026-11-04 on a need at 0). The accident goes on
/// only on the way to the shower or at it, where the walk or the action raises hygiene: the
/// bladder full and, on the way, hygiene at most 50, both in one second. The sting writes
/// hygiene's 0 and fun in one second. Sim 1 walks from the living room to the shower with
/// hygiene critical at the departure (136 then), or showers since five minutes; sim 2 watches
/// with fun critical then; sim 3 walks to the fridge with hunger at 0. Everything else written
/// after the departure is refused.
#[test]
fn a_waiting_departure_keeps_what_a_need_at_0_writes() {
    let flat = DataId::new("core:flat").expect("id");
    let walk = i64::from(houses().house(&flat).expect("the flat").walk_to(2, 6));
    let due = |depart: i64| Due {
        place: Duty::Work,
        depart: t(depart),
        back: t(NOW + 3_600),
    };
    // On the way to the shower since 6 s, the departure 5 s ago.
    let to_shower = |change: fn(&mut InHouse)| {
        checked(move |s| {
            s.waits.clear();
            let one = in_house(s, 1);
            one.place = Place::Walking {
                from: 2,
                to: 3,
                depart: t(NOW - 6),
                arrive: t(NOW - 6 + walk),
            };
            one.action = Some(Action {
                object: 6,
                offer: 0,
                slot: 0,
                filler: false,
            });
            one.needs[3] = NeedSample::new(140, t(NOW - 600), -30);
            one.due = Some(due(NOW - 5));
            change(one);
        })
    };
    assert_eq!(to_shower(|_| {}), Ok(()));
    let wet = to_shower(|one| {
        one.needs[2] = NeedSample::new(1000, t(NOW), -120);
        one.needs[3] = NeedSample::new(50, t(NOW), -30);
    });
    assert_eq!(wet, Ok(()));
    let not_full = to_shower(|one| {
        one.needs[2] = NeedSample::new(999, t(NOW), -120);
        one.needs[3] = NeedSample::new(50, t(NOW), -30);
    });
    assert_eq!(not_full, Err(Broken::Due));
    let apart = to_shower(|one| {
        one.needs[2] = NeedSample::new(1000, t(NOW), -120);
        one.needs[3] = NeedSample::new(50, t(NOW - 2), -30);
    });
    assert_eq!(apart, Err(Broken::Due));
    let dirty = to_shower(|one| {
        one.needs[2] = NeedSample::new(1000, t(NOW), -120);
        one.needs[3] = NeedSample::new(51, t(NOW), -30);
    });
    assert_eq!(dirty, Err(Broken::Due));
    // The sting on the way, then the accident: hygiene's 0 written again by the accident, later.
    let stung_then_wet = to_shower(|one| {
        one.needs[4] = NeedSample::new(650, t(NOW - 3), -60);
        one.needs[2] = NeedSample::new(1000, t(NOW - 1), -120);
        one.needs[3] = NeedSample::new(0, t(NOW - 1), -30);
    });
    assert_eq!(stung_then_wet, Ok(()));
    // Showering since five minutes, the departure ten minutes ago: the accident there writes only
    // the bladder.
    let showering = |bladder: NeedSample| {
        checked(move |s| {
            s.waits.clear();
            let one = in_house(s, 1);
            one.place = Place::Room(3);
            one.action = Some(Action {
                object: 6,
                offer: 0,
                slot: 0,
                filler: false,
            });
            one.needs[3] = NeedSample::new(100, t(NOW - 300), 2_800);
            one.needs[2] = bladder;
            one.due = Some(due(NOW - 600));
        })
    };
    let morning = in_house(&mut legal(), 1).needs[2];
    assert_eq!(showering(morning), Ok(()));
    assert_eq!(showering(NeedSample::new(1000, t(NOW), -120)), Ok(()));
    assert_eq!(
        showering(NeedSample::new(999, t(NOW), -120)),
        Err(Broken::Due)
    );
    // Watching television the accident would have ended the watching, and the departure left.
    let watching = |need: usize, sample: NeedSample| {
        checked(move |s| {
            let two = in_house(s, 2);
            two.needs[4] = NeedSample::new(100, t(NOW - 600), 400);
            two.needs[need] = sample;
            two.due = Some(Due {
                place: Duty::School,
                depart: t(NOW - 600),
                back: t(NOW + 3_600),
            });
        })
    };
    let now = t(NOW);
    assert_eq!(
        watching(2, NeedSample::new(1000, now, -120)),
        Err(Broken::Due)
    );
    assert_eq!(watching(3, NeedSample::new(50, now, -30)), Err(Broken::Due));
    assert_eq!(
        watching(0, NeedSample::new(300, now, -100)),
        Err(Broken::Due)
    );
    // Hygiene reaching 0 on the way to the fridge: its 0 and fun in one second.
    let to_fridge = |hygiene: NeedSample, fun: NeedSample| {
        checked(move |s| {
            s.waits.clear();
            let three = in_house(s, 3);
            three.needs[0] = NeedSample::new(0, t(NOW - 3_600), -100);
            three.needs[3] = hygiene;
            three.needs[4] = fun;
            three.due = Some(due(NOW - 1));
        })
    };
    let stung = NeedSample::new(650, now, -60);
    assert_eq!(to_fridge(NeedSample::new(0, now, -30), stung), Ok(()));
    let earlier = NeedSample::new(0, t(NOW - 3_600), -30);
    assert_eq!(to_fridge(earlier, stung), Err(Broken::Due));
    assert_eq!(
        to_fridge(NeedSample::new(520, now, -30), stung),
        Err(Broken::Due)
    );
    // An accident on the way to the fridge would have ended the walk.
    let wet = |s: &mut HouseState| {
        s.waits.clear();
        let three = in_house(s, 3);
        three.needs[0] = NeedSample::new(0, t(NOW - 3_600), -100);
        three.needs[2] = NeedSample::new(1000, t(NOW), -120);
        three.needs[3] = NeedSample::new(50, t(NOW), -30);
        three.due = Some(due(NOW - 1));
    };
    assert_eq!(checked(wet), Err(Broken::Due));
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
        let one = in_house(s, 1);
        one.place = Place::Walking {
            from: 2,
            to: 0,
            depart: t(NOW - 3),
            arrive: t(NOW + 5),
        };
        one.needs = one
            .needs
            .map(|need| NeedSample::new(need.value(), t(NOW - 3), 0));
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
    let held_free = checked_after(&houses(), Some(&[6]), |s| {
        let three = in_house(s, 3);
        three.action = None;
        three.place = Place::Room(2);
        s.sims.insert(sim(6), person(Place::Room(2), None));
        s.holds.insert((household(1), 1, 0), sim(6));
    });
    assert_eq!(held_free, Ok(()));
}

/// The crossings a change plans (ADR-0026 section 1 and its addendum of 2026-11-04 on a need at
/// 0), by hand: the morning values at 6:00 with an adult's awake rates fall past every threshold
/// below them, 0 included, `at + ⌈3600 · (value − level) / |rate|⌉`, in the order of the needs
/// and of the thresholds; two hours later those at or before that second are gone; a rising need
/// and a person away plan none.
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
        (Need::Hunger, 0, 21_600),
        (Need::Energy, 700, 18_000),
        (Need::Energy, 400, 45_000),
        (Need::Energy, 150, 67_500),
        (Need::Energy, 50, 76_500),
        (Need::Energy, 0, 81_000),
        (Need::Bladder, 400, 3_000),
        (Need::Bladder, 150, 10_500),
        (Need::Bladder, 50, 13_500),
        (Need::Bladder, 0, 15_000),
        (Need::Hygiene, 400, 36_000),
        (Need::Hygiene, 150, 66_000),
        (Need::Hygiene, 50, 78_000),
        (Need::Hygiene, 0, 84_000),
        (Need::Fun, 700, 6_000),
        (Need::Fun, 400, 24_000),
        (Need::Fun, 150, 39_000),
        (Need::Fun, 50, 45_000),
        (Need::Fun, 0, 48_000),
    ]
    .map(|(need, level, secs)| (need, level, at(secs)));
    assert_eq!(all, expected);
    // At 8:00 hunger is at 400 in that very second; it, bladder at 400 and fun at 700 have
    // come.
    let later: Vec<_> = crossings(&morning, at(7_200)).collect();
    assert_eq!(later.len(), 19);
    assert_eq!(
        later,
        expected[1..9]
            .iter()
            .chain(&expected[10..17])
            .chain(&expected[18..])
            .copied()
            .collect::<Vec<_>>()
    );
    // Eating from 8:00: hunger rises, the others fall as before.
    let mut eating = morning.clone();
    eating.needs[0] = eating.needs[0].with_rate(at(7_200), 2_400);
    let fed: Vec<_> = crossings(&eating, at(7_200)).collect();
    assert_eq!(fed, later[3..]);
    // Away every rate is 0.
    let mut away = person(Place::Away, None);
    away.needs = away.needs.map(|need| need.with_rate(six, 0));
    assert_eq!(crossings(&away, six).count(), 0);
}

/// Rooms one does not stay in (ADR-0026, addendum of 2026-11-04 after the playtest): in the
/// flat the bath (room 3), from which one goes to the living room (room 2), 3 + 3 = 6 s through
/// the hall. Waiting in the bath for the fridge she may stand there; idle and waiting for nothing
/// she may not, unless her choice is due in the next second after an input at the clock. Her
/// walk to the living room without an action loads from the bath with its 6 s, begun before the
/// clock; from the kitchen, a room one stays in, with its 4 + 3 = 7 s it is refused, as is one of
/// 5 s from the bath or one begun in the next second.
#[test]
fn rooms_one_does_not_stay_in_are_checked() {
    let in_bath = |state: &mut HouseState| in_house(state, 1).place = Place::Room(3);
    assert_eq!(checked(in_bath), Ok(()));
    let alone_in_bath = |state: &mut HouseState| {
        in_bath(state);
        state.waits.clear();
    };
    assert_eq!(checked(alone_in_bath), Err(Broken::Dwell));
    assert_eq!(checked_after(&houses(), Some(&[1]), alone_in_bath), Ok(()));
    let walking = |from: u8, depart: i64, secs: i64| {
        move |state: &mut HouseState| {
            state.waits.clear();
            in_house(state, 1).place = Place::Walking {
                from,
                to: 2,
                depart: t(depart),
                arrive: t(depart + secs),
            };
        }
    };
    assert_eq!(checked(walking(3, NOW - 2, 6)), Ok(()));
    assert_eq!(checked(walking(1, NOW - 2, 7)), Err(Broken::Dwell));
    assert_eq!(checked(walking(3, NOW - 2, 5)), Err(Broken::Walk));
    assert_eq!(
        checked_after(&houses(), Some(&[]), walking(3, NOW, 6)),
        Err(Broken::Walk)
    );
}

/// A small child holds a place in its bed (ADR-0026, addendum of 2026-11-04 after the playtest):
/// sim 5, 5 years, the first place of the flat's beds in the kids' room, the single bed 8, from
/// 19:00 to before 7:00. Sim 3 asleep on it at 23:00 is refused, at 6:59:59 too; at the clock
/// 7:00:00 the hold is over; with no small child in the household it loads at night.
#[test]
fn a_bed_a_young_child_holds_is_checked() {
    let night = NOW + 11 * 3_600;
    let morning = NOW + 19 * 3_600;
    let mut state = legal();
    state.waits.clear();
    let sleeper = in_house(&mut state, 3);
    sleeper.place = Place::Room(5);
    sleeper.action = Some(Action {
        object: 8,
        offer: 0,
        slot: 0,
        filler: false,
    });
    let houses = houses();
    let check = |now: i64, young: usize| {
        children_consistent(&state, Some(&houses), t(now), |_| young, member)
    };
    assert_eq!(check(night, 1), Err(Broken::ChildBed));
    assert_eq!(check(morning, 1), Err(Broken::ChildBed));
    assert_eq!(check(morning + 1, 1), Ok(()));
    assert_eq!(check(night, 0), Ok(()));
}
