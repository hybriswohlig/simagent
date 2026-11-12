//! The points of a choice (ADR-0026 section 3), by hand in the repository's flat: rooms hall 0,
//! kitchen 1, living 2, bath 3, bedroom 4, kids 5, every room 3 to 5 s from the hall (kitchen 4,
//! living 3, bath 3, bedroom 5, kids 5) and 5 s to an object in one's own room; objects table 0,
//! fridge 1, sofa 2, tv 3, bookshelf 4, toilet 5, shower 6, bed 7, single bed 8, toy chest 9.
//! Points: `(Σ weight · gain) · (100 + mood) / 100 − 50 · walking seconds`, the gain
//! `min(rate · length / 3600, 1000 − v)` for a need the offer raises, its weight `max(0,
//! comfort − v)` with the comfort of `data/house/needs.json` (hunger 700, energy 1000, bladder
//! 700, hygiene 900, fun 1000; ADR-0026, addendum of 2026-11-04 on comfort and the time-filler),
//! and `(151 − v) · 10⁹` for each critical need (at most 150) it raises. An offer of sleep (sofa
//! 2, bed 7, single bed 8) is open only in the night, its length what is left of it (ADR-0026,
//! addendum of 2026-11-04, the night); in the night the choice takes what raises a critical need
//! first, then a bed, then the sofa, then waits for a bed. The sofa's second offer, `sit` (fun
//! +120 for 30 min, 60), is open by day and in the night. The mood of an offer (the addendum of 2026-11-04 on the
//! mood) comes to `score` from outside: the hand values without it pass `no_mood`, those with it
//! give it by offer; the mood itself is the addressed draw `mood`, tested by its properties.

mod common;

use sim_core::data_id::DataId;
use sim_core::house::choice::{
    Bonus, Choice, ENOUGH, Scored, Tier, best, choose, in_tier, mood, score,
};
use sim_core::house::data::{BedFor, House, HouseData};
use sim_core::lineage::LineageKey;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::SimTime;

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json` (ADR-0026,
/// addendum of 2026-11-04 on the mood): from −10 to +10 percent.
const MOOD: u8 = 10;

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

/// The beds of `tier` in the flat (ADR-0026, addendum of 2026-11-04 after the playtest): any bed
/// for one without a partner of 18 or more, the bed 7 and the single bed 8; the single bed for a
/// child.
fn beds(houses: &HouseData, tier: Tier) -> Vec<u16> {
    let flat = flat(houses);
    (0..10)
        .filter(|&object| in_tier(flat, houses, tier, object))
        .collect()
}

/// No mood for any offer: the hand values of the points as the needs and the walk give them.
fn no_mood(_: u16, _: u16) -> i8 {
    0
}

/// The points by object, in the order of the objects and their offers: one offer per kind, but
/// the sofa's `sleep` (0) and `sit` (1).
fn points(scored: &[Scored]) -> Vec<(u16, i64)> {
    scored
        .iter()
        .map(|scored| {
            assert!(scored.offer == 0 || (scored.object, scored.offer) == (2, 1));
            (scored.object, scored.points)
        })
        .collect()
}

/// An adult of 30 at the morning values (hunger 600, energy 900, bladder 500, hygiene 700,
/// fun 800) in the hall, by day: the sofa's sleep and the beds are closed. The toy chest is for
/// 6 to 12.
#[test]
fn an_adult_at_the_morning_values() {
    let houses = houses();
    let morning = [600, 900, 500, 700, 800];
    let scored = score(
        flat(&houses),
        &houses,
        (30, morning, 0),
        (None, None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let expected = [
        // Table: hunger +1 800 for 20 min, 600 but 400 to full, weighing 100: 100 · 400, 4 s.
        (0, 40_000 - 200),
        // Fridge: hunger +2 400 for 5 min, 200: 100 · 200, 4 s.
        (1, 20_000 - 200),
        // The sofa's `sit`: fun +120 for 30 min, 60, weighing 200: 200 · 60, 3 s.
        (2, 12_000 - 150),
        // Television: fun +400 for 45 min, 300 but 200 to full: 200 · 200, 3 s.
        (3, 40_000 - 150),
        // Bookshelf: fun +300 for an hour, the same.
        (4, 40_000 - 150),
        // Toilet: bladder +12 000 for at most 5 min, 1 000 but 500 to full, weighing 200: 200 ·
        // 500, 3 s.
        (5, 100_000 - 150),
        // Shower: hygiene +2 800 for 15 min, 700 but 300 to full, weighing 200: 200 · 300, 3 s.
        (6, 60_000 - 150),
    ];
    assert_eq!(points(&scored), expected);
    let toilet = Scored {
        object: 5,
        offer: 0,
        mood: 0,
        points: 99_850,
    };
    assert_eq!(best(&scored, &[]), Some(toilet));
    // With the toilet taken the shower comes next.
    assert_eq!(best(&scored, &[5]).map(|chosen| chosen.object), Some(6));
    // Television and bookshelf tie; with all else taken the smaller object wins, and it does
    // not bring enough to get up for.
    let taken = [0, 1, 2, 5, 6, 7, 8];
    let tv = best(&scored, &taken).expect("a free offer");
    assert_eq!((tv.object, tv.points), (3, 39_850));
    assert!(tv.points < ENOUGH);
    let chose = |busy: &[u16]| choose((flat(&houses), &houses), (30, morning), &scored, busy, None);
    assert_eq!(chose(&[]), Choice::Take(toilet));
    // By day, nothing free that brings enough: waiting for the best taken object that would
    // (the toilet), or idle if none would (only television and bookshelf).
    assert_eq!(chose(&taken), Choice::Wait(5));
    let low = &scored[2..5];
    let idle = choose((flat(&houses), &houses), (30, morning), low, &[], None);
    assert_eq!(idle, Choice::Idle);
}

/// The same adult in the night, 8 hours of it left, hunger and the bladder at their comfort 700:
/// sofa and beds are open, and a bed wins though the shower brings more, nothing being critical.
/// Table, fridge and toilet weigh nothing at 700: only their walks, 4, 4 and 3 s. Sofa: energy
/// +50 for 8 h is 400, 100 to full: 100 · 100, 3 s; its `sit` 200 · 60, 3 s; television and
/// bookshelf 200 · 200, 3 s; the shower 200 · 300, 3 s; bed and single bed: +85 for 8 h is 680,
/// 100 · 100, 5 s. The sofa has more points than the beds, yet it comes only when every bed is
/// taken, and with all taken the person waits for the best bed, by the smaller object; regardless
/// of 40 000. At the morning values, hunger 600 and the bladder 500 under their comfort, what
/// fills them comes before the bed (ADR-0026, addendum of 2026-11-05 on the time-filler's
/// length): the toilet (200 · 500 − 150), with it taken the table (100 · 400 − 200), with both
/// the fridge (100 · 200 − 200), with all three the bed; not the shower, hygiene is no need of
/// the night.
#[test]
fn in_the_night_a_bed_comes_first() {
    let houses = houses();
    let comfort = [700, 900, 700, 700, 800];
    let scored = score(
        flat(&houses),
        &houses,
        (30, comfort, 0),
        (Some(ADULT_NIGHT), None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let expected = [
        (0, -200),
        (1, -200),
        (2, 10_000 - 150),
        (2, 12_000 - 150),
        (3, 40_000 - 150),
        (4, 40_000 - 150),
        (5, -150),
        (6, 60_000 - 150),
        (7, 10_000 - 250),
        (8, 10_000 - 250),
    ];
    assert_eq!(points(&scored), expected);
    let any = beds(&houses, Tier::Any);
    assert_eq!(any, [7, 8]);
    let night = Some(any.as_slice());
    let chose = |values: [u16; 5], scored: &[Scored], busy: &[u16]| {
        choose((flat(&houses), &houses), (30, values), scored, busy, night)
    };
    let at = |object: u16, points: i64| {
        Choice::Take(Scored {
            object,
            offer: 0,
            mood: 0,
            points,
        })
    };
    assert_eq!(chose(comfort, &scored, &[]), at(7, 9_750));
    assert_eq!(chose(comfort, &scored, &[7]), at(8, 9_750));
    assert_eq!(chose(comfort, &scored, &[7, 8]), at(2, 9_850));
    assert_eq!(chose(comfort, &scored, &[2, 7, 8]), Choice::Wait(7));
    let all = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    assert_eq!(chose(comfort, &scored, &all), Choice::Wait(7));
    let morning = [600, 900, 500, 700, 800];
    let hungry = score(
        flat(&houses),
        &houses,
        (30, morning, 0),
        (Some(ADULT_NIGHT), None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    assert_eq!(chose(morning, &hungry, &[]), at(5, 100_000 - 150));
    assert_eq!(chose(morning, &hungry, &[5]), at(0, 40_000 - 200));
    assert_eq!(chose(morning, &hungry, &[0, 5]), at(1, 20_000 - 200));
    assert_eq!(chose(morning, &hungry, &[0, 1, 5]), at(7, 9_750));
}

/// A child of 8 in the kids' room with fun critical at 100 and the rest at 700: what raises
/// fun gets `(151 − 100) · 10⁹`. Hunger and the bladder at their comfort weigh nothing, hygiene
/// 200, energy 300, fun 900. The toy chest stands in its room. In the night, 10 hours of it left,
/// the critical need still comes before a bed.
#[test]
fn a_critical_need_comes_first() {
    let houses = houses();
    let needs = [700, 700, 700, 700, 100];
    let critical = 51_000_000_000;
    let day = [
        // Table: hunger at its comfort, 0; kids to kitchen through the hall, 9 s.
        (0, -450),
        // Fridge: the same, 9 s.
        (1, -450),
        // The sofa's `sit`: fun 60 of 900: 900 · 60; kids to living 8 s.
        (2, critical + 54_000 - 400),
        // Television: fun 300 of 900: 900 · 300; kids to living 8 s.
        (3, critical + 270_000 - 400),
        // Bookshelf: the same.
        (4, critical + 270_000 - 400),
        // Toilet: the bladder at its comfort, 0; kids to bath 8 s.
        (5, -400),
        // Shower: hygiene 300 to full, weighing 200: 200 · 300.
        (6, 60_000 - 400),
        // Toy chest: fun +600 for 30 min, 300: 900 · 300, 5 s.
        (9, critical + 270_000 - 250),
    ];
    let scored = score(
        flat(&houses),
        &houses,
        (8, needs, 5),
        (None, None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    assert_eq!(points(&scored), day);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(9));
    let scored = score(
        flat(&houses),
        &houses,
        (8, needs, 5),
        (Some(CHILD_NIGHT), None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let mut night = day.to_vec();
    // Sofa: energy +50 for 10 h, 500, 300 to full: 300 · 300, 8 s; bed: +80 asleep (6 to 12)
    // for 10 h, 300 to full, 10 s; the single bed in her room, 5 s.
    night.extend([(2, 90_000 - 400), (7, 90_000 - 500), (8, 90_000 - 250)]);
    night.sort_unstable();
    assert_eq!(points(&scored), night);
    let children = beds(&houses, Tier::For(BedFor::Children));
    assert_eq!(children, [8]);
    let night = Some(children.as_slice());
    let chosen = choose((flat(&houses), &houses), (8, needs), &scored, &[], night);
    let toys = Scored {
        object: 9,
        offer: 0,
        mood: 0,
        points: critical + 270_000 - 250,
    };
    assert_eq!(chosen, Choice::Take(toys));
    // With what raises fun taken, the sofa's `sit` too, the single bed; the critical need waits
    // for no object.
    let busy = [2, 3, 4, 9];
    let chosen = choose((flat(&houses), &houses), (8, needs), &scored, &busy, night);
    let bed = Scored {
        object: 8,
        offer: 0,
        mood: 0,
        points: 90_000 - 250,
    };
    assert_eq!(chosen, Choice::Take(bed));
}

/// An adult in the living room at hunger 120 and energy 40, both critical: the lower one
/// comes first, in the night, 8 hours of it left. Bed and single bed tie, the smaller object
/// wins. By day nothing raising energy is open, and the table wins. Hunger weighs 580 from its
/// comfort 700, energy 960, the bladder at its comfort nothing, hygiene 200, fun 300.
#[test]
fn the_lower_critical_need_wins() {
    let houses = houses();
    let needs = [120, 40, 700, 700, 700];
    let scored = score(
        flat(&houses),
        &houses,
        (40, needs, 2),
        (Some(ADULT_NIGHT), None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let hungry = 31_000_000_000;
    let tired = 111_000_000_000;
    let expected = [
        // Table: hunger 600 of 880: 580 · 600; living to kitchen 7 s.
        (0, hungry + 348_000 - 350),
        // Fridge: 580 · 200.
        (1, hungry + 116_000 - 350),
        // Sofa in the room: energy +50 for 8 h, 400 of 960: 960 · 400, 5 s.
        (2, tired + 384_000 - 250),
        // The sofa's `sit`: fun 60 of 300: 300 · 60, 5 s.
        (2, 18_000 - 250),
        // Television and bookshelf: fun 300 to full of 300: 300 · 300, 5 s.
        (3, 90_000 - 250),
        (4, 90_000 - 250),
        // Toilet: the bladder at its comfort, 0; shower: 200 · 300; living to bath 6 s.
        (5, -300),
        (6, 60_000 - 300),
        // Bed and single bed: energy +85 for 8 h, 680 of 960: 960 · 680, 8 s.
        (7, tired + 652_800 - 400),
        (8, tired + 652_800 - 400),
    ];
    assert_eq!(points(&scored), expected);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(7));
    assert_eq!(best(&scored, &[7]).map(|chosen| chosen.object), Some(8));
    let any = beds(&houses, Tier::Any);
    let chose = |scored: &[Scored], night: bool| match choose(
        (flat(&houses), &houses),
        (40, needs),
        scored,
        &[],
        night.then_some(any.as_slice()),
    ) {
        Choice::Take(chosen) => chosen.object,
        other => panic!("{other:?}"),
    };
    assert_eq!(chose(&scored, true), 7);
    let by_day = score(
        flat(&houses),
        &houses,
        (40, needs, 2),
        (None, None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    assert_eq!(chose(&by_day, false), 0);
}

/// A tie goes to the smaller object, then the smaller offer; taken objects do not count;
/// nothing free, nothing chosen.
#[test]
fn ties_and_taken_objects() {
    let scored = |object: u16, offer: u16, points: i64| Scored {
        object,
        offer,
        mood: 0,
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
        let scored = score(
            flat(&houses),
            &houses,
            (age, needs, 4),
            (Some(night), None),
            no_mood,
            0,
            |_| Bonus::NONE,
        );
        let bed = scored.iter().find(|scored| scored.object == 7);
        let expected = 151_000_000_000 + 1_000 * gain - 250;
        assert_eq!(bed.map(|bed| bed.points), Some(expected), "{age}");
    }
}

/// A case of the mood: world seed, founder of `test:mood`, second, object, offer.
type Case = (u64, u64, i64, u16, u16);

/// A part of the address, named, and how a case changes in it alone.
type Change = (&'static str, fn(Case) -> Case);

/// The mood of `case`.
fn mood_of((seed, founder, second, object, offer): Case) -> i8 {
    let key = LineageKey::founder(&DataId::new("test:mood").expect("id"), founder);
    mood(seed, key, SimTime::from_secs(second), (object, offer), MOOD)
}

/// The mood of `case` from its address as the addendum on the mood gives it, the specification
/// of the draw: `uniform_below(21)` of `CHOICE_MOOD` with the world seed, subject the lineage
/// key, occasion the second, detail `(object << 32) | offer`, less 10.
fn specified((seed, founder, second, object, offer): Case) -> i64 {
    let key = LineageKey::founder(&DataId::new("test:mood").expect("id"), founder);
    let address = DrawKey {
        tag: tags::CHOICE_MOOD,
        subject: SubjectKey::from(key),
        occasion: u64::try_from(second).expect("after the start"),
        detail: (u64::from(object) << 32) | u64::from(offer),
    };
    let drawn = Draws::new(WorldSeed::new(seed)).uniform_below(address, 21);
    i64::try_from(drawn).expect("small") - 10
}

/// The mood is an addressed draw (ADR-0026, addendum of 2026-11-04 on the mood; ADR-0001): from
/// −10 to +10 percent, every value of it; the same address gives the same mood, whatever is
/// drawn in between; the address is the addendum's; and each of its parts counts, changed alone
/// it changes the mood of at least one of four fixed cases.
#[test]
fn the_mood_is_an_addressed_draw() {
    let day: sim_core::house::data::DayRules =
        serde_json::from_str(include_str!("../../../data/house/day.json")).expect("the day");
    assert_eq!(day.choice_mood_percent, MOOD);
    let cases: [Case; 4] = [
        (7, 0, 93_600, 5, 0),
        (7, 1, 93_600, 0, 0),
        (0x5eed, 2, 1_000_000, 9, 0),
        (1, 3, 0, 2, 1),
    ];
    for case in cases {
        let (seed, founder, second, object, offer) = case;
        let first = mood_of(case);
        let _ = mood_of((seed + 1, founder + 1, second + 1, object, offer));
        assert_eq!(mood_of(case), first, "{case:?}");
        assert_eq!(i64::from(first), specified(case), "{case:?}");
    }
    let changes: [Change; 5] = [
        ("seed", |(seed, f, s, o, n)| (seed + 1, f, s, o, n)),
        ("lineage key", |(seed, f, s, o, n)| (seed, f + 10, s, o, n)),
        ("second", |(seed, f, s, o, n)| (seed, f, s + 1, o, n)),
        ("object", |(seed, f, s, o, n)| (seed, f, s, o + 1, n)),
        ("offer", |(seed, f, s, o, n)| (seed, f, s, o, n + 1)),
    ];
    for (part, change) in changes {
        let moved = cases
            .iter()
            .any(|&case| mood_of(change(case)) != mood_of(case));
        assert!(moved, "the {part} changes no mood");
    }
    let mut seen = [0_u32; 21];
    for second in 0..2_000 {
        for object in 0..10 {
            let drawn = mood_of((7, 0, second, object, 0));
            assert!((-10..=10).contains(&drawn), "{drawn}");
            seen[usize::try_from(drawn + 10).expect("in range")] += 1;
        }
    }
    assert!(seen.iter().all(|&count| count > 0), "{seen:?}");
}

/// The mood scales what an offer fills, truncated toward zero, never the walk nor what a
/// critical need adds. A child of 8 in the bedroom with fun at 600 and the rest full, by day:
/// television, bookshelf and toy chest each fill fun by 300 of 400, 400 · 300 = 120 000; the
/// television and the bookshelf stand in the living room, 8 s, the toy chest in the kids' room,
/// 10 s. At the moods −10, +10 and +7: 108 000 − 400, 132 000 − 400 and 128 400 − 500 =
/// 127 900. The table fills nothing: 0 − 450 (kitchen, 9 s) at its mood +10, like the fridge at
/// 0; toilet and shower 0 − 400 (bath, 8 s); the sofa's `sit` fills 60 of 400, 24 000 at its mood
/// 0, less 8 s to the living room. Without the moods television and bookshelf tie at
/// 119 600 and the television wins; with them the bookshelf. With fun critical at 100 each fills
/// 900 · 300 = 270 000: 243 000, 297 000 and 288 900, each with `51 · 10⁹` unscaled. At fun 997
/// each fills 3 · 3 = 9: 8.1, 9.9 and 9.63, truncated 8, 9 and 9.
#[test]
fn the_mood_scales_what_the_offer_fills() {
    let houses = houses();
    let moods = |object: u16, _: u16| match object {
        0 | 4 => 10,
        3 => -10,
        9 => 7,
        _ => 0,
    };
    let at_fun = |fun: u16| [1_000, 1_000, 1_000, 1_000, fun];
    let scored = score(
        flat(&houses),
        &houses,
        (8, at_fun(600), 4),
        (None, None),
        moods,
        0,
        |_| Bonus::NONE,
    );
    let expected = [
        (0, -450),
        (1, -450),
        (2, 24_000 - 400),
        (3, 108_000 - 400),
        (4, 132_000 - 400),
        (5, -400),
        (6, -400),
        (9, 128_400 - 500),
    ];
    assert_eq!(points(&scored), expected);
    let by_object: Vec<(u16, i8)> = (scored.iter())
        .map(|scored| (scored.object, scored.mood))
        .collect();
    let moods_by_object = [
        (0, 10),
        (1, 0),
        (2, 0),
        (3, -10),
        (4, 10),
        (5, 0),
        (6, 0),
        (9, 7),
    ];
    assert_eq!(by_object, moods_by_object);
    assert_eq!(best(&scored, &[]).map(|chosen| chosen.object), Some(4));
    let plain = score(
        flat(&houses),
        &houses,
        (8, at_fun(600), 4),
        (None, None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let plain_best = best(&plain, &[]).map(|chosen| (chosen.object, chosen.points));
    assert_eq!(plain_best, Some((3, 119_600)));
    let fun = |values: [u16; 5]| -> Vec<(u16, i64)> {
        let scored = score(
            flat(&houses),
            &houses,
            (8, values, 4),
            (None, None),
            moods,
            0,
            |_| Bonus::NONE,
        );
        (points(&scored).into_iter())
            .filter(|(object, _)| [3, 4, 9].contains(object))
            .collect()
    };
    let critical = 51_000_000_000;
    assert_eq!(
        fun(at_fun(100)),
        [
            (3, 243_000 - 400 + critical),
            (4, 297_000 - 400 + critical),
            (9, 288_900 - 500 + critical),
        ]
    );
    assert_eq!(fun(at_fun(997)), [(3, 8 - 400), (4, 9 - 400), (9, 9 - 500)]);
}

/// A critical need still always wins, whatever the moods: an adult in the hall at hunger 150,
/// critical by one point, and bladder 151, not critical. The toilet fills 849 of 849, weighing
/// 549 from the bladder's comfort 700: 466 101, at the mood +10 512 711 (truncated) − 150; the
/// table fills 600 of 850, weighing 550: 330 000, at −10 297 000 − 200, with `1 · 10⁹` for the
/// hunger; the fridge 200 of 850 at −10, 99 000 − 200 and the same `10⁹`.
#[test]
fn a_critical_need_wins_whatever_the_mood() {
    let houses = houses();
    let needs = [150, 1_000, 151, 1_000, 1_000];
    let moods = |object: u16, _: u16| if object == 5 { 10 } else { -10 };
    let scored = score(
        flat(&houses),
        &houses,
        (30, needs, 0),
        (None, None),
        moods,
        0,
        |_| Bonus::NONE,
    );
    let of = |object: u16| (scored.iter()).find(|scored| scored.object == object);
    let critical = 1_000_000_000;
    assert_eq!(
        of(0).map(|table| table.points),
        Some(297_000 - 200 + critical)
    );
    assert_eq!(
        of(1).map(|fridge| fridge.points),
        Some(99_000 - 200 + critical)
    );
    assert_eq!(of(5).map(|toilet| toilet.points), Some(512_711 - 150));
    let table = Scored {
        object: 0,
        offer: 0,
        mood: -10,
        points: 297_000 - 200 + critical,
    };
    let chosen = choose((flat(&houses), &houses), (30, needs), &scored, &[], None);
    assert_eq!(chosen, Choice::Take(table));
}

/// The floor of 40 000 and the night's order read the points with the mood. The adult of 30 at
/// the morning values in the hall, by day, with all but television and bookshelf taken: at the
/// mood +1 the television brings 40 000 · 101 / 100 − 150 = 40 250, enough to get up for, where
/// without a mood it waits for the toilet (`an_adult_at_the_morning_values`). In the night, 8
/// hours of it left, hunger and the bladder at their comfort (else what fills them comes first,
/// `in_the_night_a_bed_comes_first`), bed and single bed tie at 9 750 without a mood; the single
/// bed at +1 brings 10 100 − 250 = 9 850 and comes first; the sofa at +10, 11 000 − 150 = 10 850,
/// still comes only when every bed is taken.
#[test]
fn the_floor_and_the_beds_take_the_mood() {
    let houses = houses();
    let morning = [600, 900, 500, 700, 800];
    let moods = |object: u16, _: u16| match object {
        2 => 10,
        3 | 8 => 1,
        _ => 0,
    };
    let any = beds(&houses, Tier::Any);
    let chose = |scored: &[Scored], busy: &[u16], night: bool| {
        let night = night.then_some(any.as_slice());
        choose((flat(&houses), &houses), (30, morning), scored, busy, night)
    };
    let at = |object: u16, mood: i8, points: i64| {
        Choice::Take(Scored {
            object,
            offer: 0,
            mood,
            points,
        })
    };
    let by_day = score(
        flat(&houses),
        &houses,
        (30, morning, 0),
        (None, None),
        moods,
        0,
        |_| Bonus::NONE,
    );
    assert_eq!(
        chose(&by_day, &[0, 1, 2, 5, 6, 7, 8], false),
        at(3, 1, 40_250)
    );
    let comfort = [700, 900, 700, 700, 800];
    let night = score(
        flat(&houses),
        &houses,
        (30, comfort, 0),
        (Some(ADULT_NIGHT), None),
        moods,
        0,
        |_| Bonus::NONE,
    );
    let in_bed = |busy: &[u16]| {
        let beds = Some(any.as_slice());
        choose((flat(&houses), &houses), (30, comfort), &night, busy, beds)
    };
    assert_eq!(in_bed(&[]), at(8, 1, 9_850));
    assert_eq!(in_bed(&[8]), at(7, 0, 9_750));
    assert_eq!(in_bed(&[7, 8]), at(2, 10, 10_850));
}
