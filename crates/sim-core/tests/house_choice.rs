//! The points of a choice (ADR-0026 section 3), by hand in the repository's flat: rooms hall 0,
//! kitchen 1, living 2, bath 3, bedroom 4, kids 5, every room 3 to 5 s from the hall (kitchen 4,
//! living 3, bath 3, bedroom 5, kids 5) and 5 s to an object in one's own room; objects table 0,
//! fridge 1, sofa 2, tv 3, bookshelf 4, toilet 5, shower 6, bed 7, single bed 8, toy chest 9.
//! Points: `Σ (1000 − v) · gain − 50 · walking seconds`, the gain `min(rate · length / 3600,
//! 1000 − v)` for a need the offer raises, and `(151 − v) · 10⁹` for each critical need
//! (at most 150) it raises. An offer of sleep (sofa 2, bed 7, single bed 8) is open only in the
//! night, its length what is left of it (ADR-0026, addendum of 2026-11-04); in the night the
//! choice takes what raises a critical need first, then a bed, then the sofa, then waits for a
//! bed.

mod common;

use sim_core::data_id::DataId;
use sim_core::house::choice::{Choice, ENOUGH, Scored, best, choose, score};
use sim_core::house::data::{House, HouseData};

/// The seconds of a whole night: 8 hours from 22:00 for 18 to 66, 10 from 20:00 for 6 to 17,
/// 8.5 from 21:30 from 67 on.
const ADULT_NIGHT: i64 = 8 * 3_600;
const CHILD_NIGHT: i64 = 10 * 3_600;
const OLD_NIGHT: i64 = 8 * 3_600 + 1_800;

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
/// fun 800) in the hall, by day: sofa and beds are closed. The toy chest is for 6 to 12.
#[test]
fn an_adult_at_the_morning_values() {
    let houses = houses();
    let morning = [600, 900, 500, 700, 800];
    let scored = score(flat(&houses), &houses, 30, morning, 0, None);
    let expected = [
        // Table: hunger +1 800 for 20 min, 600 but 400 to full: 400 · 400, 4 s.
        (0, 160_000 - 200),
        // Fridge: hunger +2 400 for 5 min, 200: 400 · 200, 4 s.
        (1, 80_000 - 200),
        // Television: fun +400 for 45 min, 300 but 200 to full: 200 · 200, 3 s.
        (3, 40_000 - 150),
        // Bookshelf: fun +300 for an hour, the same.
        (4, 40_000 - 150),
        // Toilet: bladder +12 000 for at most 5 min, 1 000 but 500 to full: 500 · 500, 3 s.
        (5, 250_000 - 150),
        // Shower: hygiene +2 800 for 15 min, 700 but 300 to full: 300 · 300, 3 s.
        (6, 90_000 - 150),
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
    let chose = |busy: &[u16]| {
        choose(
            (flat(&houses), &houses),
            (30, morning),
            &scored,
            busy,
            false,
        )
    };
    assert_eq!(chose(&[]), Choice::Take(toilet));
    // By day, nothing free that brings enough: waiting for the best taken object that would
    // (the toilet), or idle if none would (only television and bookshelf).
    assert_eq!(chose(&taken), Choice::Wait(5));
    let low = &scored[2..4];
    let idle = choose((flat(&houses), &houses), (30, morning), low, &[], false);
    assert_eq!(idle, Choice::Idle);
}

/// The same adult in the night, 8 hours of it left: sofa and beds are open, and a bed wins
/// though the toilet brings more, nothing being critical. Sofa: energy +50 for 8 h is 400, 100
/// to full: 100 · 100, 3 s; bed and single bed: +85 for 8 h is 680, 100 · 100, 5 s. The sofa
/// has more points than the beds, yet it comes only when every bed is taken, and with all
/// taken the person waits for the best bed, by the smaller object; regardless of 40 000.
#[test]
fn in_the_night_a_bed_comes_first() {
    let houses = houses();
    let morning = [600, 900, 500, 700, 800];
    let scored = score(flat(&houses), &houses, 30, morning, 0, Some(ADULT_NIGHT));
    let expected = [
        (0, 160_000 - 200),
        (1, 80_000 - 200),
        (2, 10_000 - 150),
        (3, 40_000 - 150),
        (4, 40_000 - 150),
        (5, 250_000 - 150),
        (6, 90_000 - 150),
        (7, 10_000 - 250),
        (8, 10_000 - 250),
    ];
    assert_eq!(points(&scored), expected);
    let chose = |busy: &[u16]| choose((flat(&houses), &houses), (30, morning), &scored, busy, true);
    let at = |object: u16, points: i64| {
        Choice::Take(Scored {
            object,
            offer: 0,
            points,
        })
    };
    assert_eq!(chose(&[]), at(7, 9_750));
    assert_eq!(chose(&[7]), at(8, 9_750));
    assert_eq!(chose(&[7, 8]), at(2, 9_850));
    assert_eq!(chose(&[2, 7, 8]), Choice::Wait(7));
    assert_eq!(chose(&[0, 1, 2, 3, 4, 5, 6, 7, 8]), Choice::Wait(7));
}

/// A child of 8 in the kids' room with fun critical at 100 and the rest at 700: what raises
/// fun gets `(151 − 100) · 10⁹`. The toy chest stands in its room. In the night, 10 hours of it
/// left, the critical need still comes before a bed.
#[test]
fn a_critical_need_comes_first() {
    let houses = houses();
    let needs = [700, 700, 700, 700, 100];
    let critical = 51_000_000_000;
    let day = [
        // Table: 600 but 300 to full: 300 · 300; kids to kitchen through the hall, 9 s.
        (0, 90_000 - 450),
        // Fridge: 200 · 300, 9 s.
        (1, 60_000 - 450),
        // Television: fun 300 of 900: 900 · 300; kids to living 8 s.
        (3, critical + 270_000 - 400),
        // Bookshelf: the same.
        (4, critical + 270_000 - 400),
        // Toilet: bladder 300 to full: 300 · 300; kids to bath 8 s.
        (5, 90_000 - 400),
        // Shower: hygiene 300 to full: 300 · 300.
        (6, 90_000 - 400),
        // Toy chest: fun +600 for 30 min, 300: 900 · 300, 5 s.
        (9, critical + 270_000 - 250),
    ];
    let scored = score(flat(&houses), &houses, 8, needs, 5, None);
    assert_eq!(points(&scored), day);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(9));
    let scored = score(flat(&houses), &houses, 8, needs, 5, Some(CHILD_NIGHT));
    let mut night = day.to_vec();
    // Sofa: energy +50 for 10 h, 500, 300 to full: 300 · 300, 8 s; bed: +80 asleep (6 to 12)
    // for 10 h, 300 to full, 10 s; the single bed in her room, 5 s.
    night.extend([(2, 90_000 - 400), (7, 90_000 - 500), (8, 90_000 - 250)]);
    night.sort_unstable();
    assert_eq!(points(&scored), night);
    let chosen = choose((flat(&houses), &houses), (8, needs), &scored, &[], true);
    let toys = Scored {
        object: 9,
        offer: 0,
        points: critical + 270_000 - 250,
    };
    assert_eq!(chosen, Choice::Take(toys));
    // With what raises fun taken, the single bed; the critical need waits for no object.
    let busy = [3, 4, 9];
    let chosen = choose((flat(&houses), &houses), (8, needs), &scored, &busy, true);
    let bed = Scored {
        object: 8,
        offer: 0,
        points: 90_000 - 250,
    };
    assert_eq!(chosen, Choice::Take(bed));
}

/// An adult in the living room at hunger 120 and energy 40, both critical: the lower one
/// comes first, in the night, 8 hours of it left. Bed and single bed tie, the smaller object
/// wins. By day nothing raising energy is open, and the table wins.
#[test]
fn the_lower_critical_need_wins() {
    let houses = houses();
    let needs = [120, 40, 700, 700, 700];
    let scored = score(flat(&houses), &houses, 40, needs, 2, Some(ADULT_NIGHT));
    let hungry = 31_000_000_000;
    let tired = 111_000_000_000;
    let expected = [
        // Table: hunger 600 of 880: 880 · 600; living to kitchen 7 s.
        (0, hungry + 528_000 - 350),
        // Fridge: 880 · 200.
        (1, hungry + 176_000 - 350),
        // Sofa in the room: energy +50 for 8 h, 400 of 960: 960 · 400, 5 s.
        (2, tired + 384_000 - 250),
        // Television and bookshelf: fun 300 to full of 300: 300 · 300, 5 s.
        (3, 90_000 - 250),
        (4, 90_000 - 250),
        // Toilet and shower: 300 · 300, living to bath 6 s.
        (5, 90_000 - 300),
        (6, 90_000 - 300),
        // Bed and single bed: energy +85 for 8 h, 680 of 960: 960 · 680, 8 s.
        (7, tired + 652_800 - 400),
        (8, tired + 652_800 - 400),
    ];
    assert_eq!(points(&scored), expected);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(7));
    assert_eq!(best(&scored, &[7]).map(|chosen| chosen.object), Some(8));
    let chose = |scored: &[Scored], night: bool| match choose(
        (flat(&houses), &houses),
        (40, needs),
        scored,
        &[],
        night,
    ) {
        Choice::Take(chosen) => chosen.object,
        other => panic!("{other:?}"),
    };
    assert_eq!(chose(&scored, true), 7);
    let by_day = score(flat(&houses), &houses, 40, needs, 2, None);
    assert_eq!(chose(&by_day, false), 0);
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

/// The sleeping rates follow the age band (review): in the bedroom with energy at 0 the bed
/// fills the whole night of each band at +80 for 8 (10 h), +70 for 15 (10 h), +85 for 30 (8 h)
/// and +90 for 70 (8.5 h), so 800, 700, 680 and 765; energy at 0 is critical, `151 · 10⁹`,
/// and the bed stands in the room, 5 s.
#[test]
fn the_sleeping_rates_follow_the_age_band() {
    let houses = houses();
    for (age, night, gain) in [
        (8, CHILD_NIGHT, 800),
        (15, CHILD_NIGHT, 700),
        (30, ADULT_NIGHT, 680),
        (70, OLD_NIGHT, 765),
    ] {
        let needs = [700, 0, 700, 700, 700];
        let scored = score(flat(&houses), &houses, age, needs, 4, Some(night));
        let bed = scored.iter().find(|scored| scored.object == 7);
        let expected = 151_000_000_000 + 1_000 * gain - 250;
        assert_eq!(bed.map(|bed| bed.points), Some(expected), "{age}");
    }
}
