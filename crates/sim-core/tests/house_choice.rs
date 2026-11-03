//! The points of a choice (ADR-0026 section 3), by hand in the repository's flat: rooms hall 0,
//! kitchen 1, living 2, bath 3, bedroom 4, kids 5, every room 3 to 5 s from the hall (kitchen 4,
//! living 3, bath 3, bedroom 5, kids 5) and 5 s to an object in one's own room; objects table 0,
//! fridge 1, sofa 2, tv 3, bookshelf 4, toilet 5, shower 6, bed 7, single bed 8, toy chest 9.
//! Points: `Σ (1000 − v) · gain − 50 · walking seconds`, the gain `min(rate · length / 3600,
//! 1000 − v)` for a need the offer raises, and `(151 − v) · 10⁹` for each critical need
//! (at most 150) it raises.

mod common;

use sim_core::data_id::DataId;
use sim_core::house::choice::{ENOUGH, Scored, best, score};
use sim_core::house::data::{House, HouseData};

fn houses() -> HouseData {
    common::repository_houses()
}

fn flat(houses: &HouseData) -> &House {
    houses
        .house(&DataId::new("core:flat").expect("id"))
        .expect("the flat")
}

fn points(scored: &[Scored]) -> Vec<(u16, i64)> {
    scored
        .iter()
        .map(|scored| {
            assert_eq!(scored.offer, 0, "one offer per kind");
            (scored.object, scored.points)
        })
        .collect()
}

/// An adult of 30 at the morning values (hunger 600, energy 900, bladder 500, hygiene 700,
/// fun 800) in the hall. The toy chest is for 6 to 12.
#[test]
fn an_adult_at_the_morning_values() {
    let houses = houses();
    let scored = score(flat(&houses), &houses, 30, [600, 900, 500, 700, 800], 0);
    let expected = [
        // Table: hunger +1 800 for 20 min, 600 but 400 to full: 400 · 400, 4 s.
        (0, 160_000 - 200),
        // Fridge: hunger +2 400 for 5 min, 200: 400 · 200, 4 s.
        (1, 80_000 - 200),
        // Sofa: energy +50 asleep for at most 12 h, 600 but 100 to full; the other sleeping
        // rates do not raise anything (fun 0): 100 · 100, 3 s.
        (2, 10_000 - 150),
        // Television: fun +400 for 45 min, 300 but 200 to full: 200 · 200, 3 s.
        (3, 40_000 - 150),
        // Bookshelf: fun +300 for an hour, the same.
        (4, 40_000 - 150),
        // Toilet: bladder +12 000 for at most 5 min, 1 000 but 500 to full: 500 · 500, 3 s.
        (5, 250_000 - 150),
        // Shower: hygiene +2 800 for 15 min, 700 but 300 to full: 300 · 300, 3 s.
        (6, 90_000 - 150),
        // Bed and single bed: energy +85 asleep (18 to 66) for 12 h, 100 to full, 5 s.
        (7, 10_000 - 250),
        (8, 10_000 - 250),
    ];
    assert_eq!(points(&scored), expected);
    let toilet = Scored {
        object: 5,
        offer: 0,
        points: 249_850,
    };
    assert_eq!(best(&scored, &[]), Some(toilet));
    // With the toilet taken the table comes next.
    assert_eq!(best(&scored, &[5]).map(|chosen| chosen.object), Some(0));
    // Television and bookshelf tie; with all else taken the smaller object wins, and it does
    // not bring enough to get up for.
    let taken = [0, 1, 2, 5, 6, 7, 8];
    let tv = best(&scored, &taken).expect("a free offer");
    assert_eq!((tv.object, tv.points), (3, 39_850));
    assert!(tv.points < ENOUGH);
}

/// A child of 8 in the kids' room with fun critical at 100 and the rest at 700: what raises
/// fun gets `(151 − 100) · 10⁹`. The toy chest stands in its room.
#[test]
fn a_critical_need_comes_first() {
    let houses = houses();
    let scored = score(flat(&houses), &houses, 8, [700, 700, 700, 700, 100], 5);
    let critical = 51_000_000_000;
    let expected = [
        // Table: 600 but 300 to full: 300 · 300; kids to kitchen through the hall, 9 s.
        (0, 90_000 - 450),
        // Fridge: 200 · 300, 9 s.
        (1, 60_000 - 450),
        // Sofa: energy +50, 600 but 300: 300 · 300; kids to living 8 s.
        (2, 90_000 - 400),
        // Television: fun 300 of 900: 900 · 300.
        (3, critical + 270_000 - 400),
        // Bookshelf: the same.
        (4, critical + 270_000 - 400),
        // Toilet: bladder 300 to full: 300 · 300; kids to bath 8 s.
        (5, 90_000 - 400),
        // Shower: hygiene 300 to full: 300 · 300.
        (6, 90_000 - 400),
        // Bed: energy +60 asleep (6 to 12) for 12 h, 300 to full: 300 · 300; 10 s.
        (7, 90_000 - 500),
        // Single bed in her room: 5 s.
        (8, 90_000 - 250),
        // Toy chest: fun +600 for 30 min, 300: 900 · 300, 5 s.
        (9, critical + 270_000 - 250),
    ];
    assert_eq!(points(&scored), expected);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(9));
}

/// An adult in the living room at hunger 120 and energy 40, both critical: the lower one
/// comes first. Bed and single bed tie, the smaller object wins.
#[test]
fn the_lower_critical_need_wins() {
    let houses = houses();
    let scored = score(flat(&houses), &houses, 40, [120, 40, 700, 700, 700], 2);
    let hungry = 31_000_000_000;
    let tired = 111_000_000_000;
    let expected = [
        // Table: hunger 600 of 880: 880 · 600; living to kitchen 7 s.
        (0, hungry + 528_000 - 350),
        // Fridge: 880 · 200.
        (1, hungry + 176_000 - 350),
        // Sofa in the room: energy 600 of 960: 960 · 600, 5 s.
        (2, tired + 576_000 - 250),
        // Television and bookshelf: fun 300 to full of 300: 300 · 300, 5 s.
        (3, 90_000 - 250),
        (4, 90_000 - 250),
        // Toilet and shower: 300 · 300, living to bath 6 s.
        (5, 90_000 - 300),
        (6, 90_000 - 300),
        // Bed and single bed: energy 1 020 of 960: 960 · 960, 8 s.
        (7, tired + 921_600 - 400),
        (8, tired + 921_600 - 400),
    ];
    assert_eq!(points(&scored), expected);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(7));
    assert_eq!(best(&scored, &[7]).map(|chosen| chosen.object), Some(8));
}

/// A tie goes to the smaller object, then the smaller offer; taken objects do not count;
/// nothing free, nothing chosen.
#[test]
fn ties_and_taken_objects() {
    let scored = |object: u16, offer: u16, points: i64| Scored {
        object,
        offer,
        points,
    };
    let all = [
        scored(2, 0, 10),
        scored(3, 0, 50),
        scored(3, 1, 50),
        scored(4, 0, 50),
        scored(6, 0, -5),
    ];
    assert_eq!(best(&all, &[]), Some(scored(3, 0, 50)));
    assert_eq!(best(&all, &[3]), Some(scored(4, 0, 50)));
    assert_eq!(best(&all, &[3, 4]), Some(scored(2, 0, 10)));
    assert_eq!(best(&all, &[2, 3, 4]), Some(scored(6, 0, -5)));
    assert_eq!(best(&all, &[2, 3, 4, 6]), None);
    assert_eq!(best(&[], &[]), None);
}
