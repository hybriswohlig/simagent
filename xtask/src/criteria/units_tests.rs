//! The units of the criteria on hand-written pedigrees (design note of the criteria kit,
//! section 4 and section 7 item 2). Every expected anchor and value is derived by hand from
//! the note's rules, never from a run.

use super::*;
use crate::criteria::export::{CoupleRow, Header, RawGenome};
use crate::criteria::stats::{mc1r_expectation, real};

const Y: i64 = YEAR;
/// A year in seconds, as the exposure of the life table counts it.
const YS: u64 = 31_449_600;

fn header(end: i64) -> Header {
    Header {
        core_version: 1,
        data_hash: 0,
        world_seed: 6,
        pool_seed: None,
        end,
        traits: vec![
            ("core:growth".to_owned(), 0),
            ("core:pigment".to_owned(), 0),
            ("core:cognition".to_owned(), 60 * Y),
        ],
        longevity: "core:longevity".to_owned(),
        mc1r: "core:mc1r".to_owned(),
        persons: 0,
        couples: 0,
    }
}

/// A town person without parents. `z` and `gl` are read off the key, so every value names
/// its person: `z_h` = key, `z_pig` = key + 1000, `z_K` = key + 2000, `gl` = key + 3000.
fn person(key: u64, female: bool, born: i64) -> Row {
    let k = real(key);
    Row {
        key,
        gkey: key,
        female,
        born,
        died: None,
        emig: None,
        mother: None,
        father: None,
        pool: 0,
        arrived: None,
        mult: Some(0),
        brought: 0,
        genome: RawGenome {
            maternal: [0; 3],
            paternal: [0; 3],
            x: 0,
            paternal_x: None,
            y: None,
            mt: 0,
        },
        z: [k, k + 1000.0, k + 2000.0],
        gl: k + 3000.0,
        mc1r: 0,
    }
}

fn child(key: u64, female: bool, born: i64, mother: u64, father: u64) -> Row {
    Row {
        mother: Some(mother),
        father: Some(father),
        ..person(key, female, born)
    }
}

fn arrival(key: u64, female: bool, born: i64, arrived: i64) -> Row {
    Row {
        pool: 1,
        arrived: Some(arrived),
        mult: None,
        ..person(key, female, born)
    }
}

fn couple(formed: i64, woman: u64, man: u64) -> CoupleRow {
    CoupleRow {
        woman,
        man,
        formed,
        carried: false,
    }
}

fn export(end: i64, mut rows: Vec<Row>, couples: Vec<CoupleRow>) -> Export {
    rows.sort_by_key(|row| row.key);
    let mut header = header(end);
    header.persons = u64::try_from(rows.len()).expect("a count");
    header.couples = u64::try_from(couples.len()).expect("a count");
    Export {
        header,
        rows,
        couples,
    }
}

fn town(export: &Export) -> Town<'_> {
    Town::new(export).expect("a valid town")
}

fn reals(keys: &[u64]) -> Vec<f64> {
    keys.iter().map(|key| real(*key)).collect()
}

#[test]
fn the_unit_hash_reads_the_keys_as_little_endian_words() {
    // xxh3-64 of no bytes (`LOG_HASH_EMPTY` in `crates/sim-core/tests/projection.rs`).
    assert_eq!(unit_hash(&[]), 0x2d06_8005_38d3_94c2);
    let words = [8, 7, 6, 5, 4, 3, 2, 1, 0xff, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        unit_hash(&[0x0102_0304_0506_0708, 0xff]),
        sim_core::hash::stable_hash64(&words)
    );
    assert_ne!(unit_hash(&[1, 2]), unit_hash(&[2, 1]));
}

#[test]
fn a_town_needs_its_traits_arrival_seconds_mothers_and_partners() {
    let family = || {
        vec![
            person(1, true, -20 * Y),
            person(2, false, -20 * Y),
            child(3, true, 5 * Y, 1, 2),
        ]
    };
    let with = |extra: Row| {
        let mut rows = family();
        rows.push(extra);
        Town::new(&export(10 * Y, rows, vec![])).err()
    };
    assert!(Town::new(&export(10 * Y, family(), vec![couple(0, 1, 2)])).is_ok());
    for (at, name) in [
        (0, "core:growth"),
        (1, "core:pigment"),
        (2, "core:cognition"),
    ] {
        let mut renamed = export(10 * Y, family(), vec![]);
        renamed.header.traits[at].0 = "core:height".to_owned();
        assert_eq!(
            Town::new(&renamed).err(),
            Some(format!("the export has no trait {name}"))
        );
    }
    assert_eq!(
        with(Row {
            arrived: None,
            ..arrival(4, true, -30 * Y, 0)
        }),
        Some("arrival 0000000000000004 has no arrival second".to_owned())
    );
    // A birth in the town needs its mother among the rows: none, a record's key, a pool child.
    let motherless = "0000000000000005 is born in the town without a mother among the rows";
    for mother in [None, Some(0x99)] {
        let row = Row {
            mother,
            ..person(5, false, 0)
        };
        assert_eq!(with(row), Some(motherless.to_owned()));
    }
    let pool_child = Row {
        pool: 2,
        ..person(5, true, 3 * Y)
    };
    assert_eq!(with(pool_child), Some(motherless.to_owned()));
    // A founder born before the start and an arrival need none.
    assert_eq!(with(person(6, false, -1)), None);
    assert_eq!(with(arrival(7, true, 5 * Y, 30 * Y)), None);
    for (woman, man) in [(1, 0x99), (0x98, 2)] {
        assert_eq!(
            Town::new(&export(10 * Y, family(), vec![couple(0, woman, man)])).err(),
            Some(format!(
                "couple {woman:016x} and {man:016x}: a partner is not a row"
            ))
        );
    }
}

#[test]
fn the_traits_are_found_by_name() {
    let rows = vec![
        person(1, true, -20 * Y),
        person(2, false, -20 * Y),
        child(3, true, 5 * Y, 1, 2),
    ];
    let mut shuffled = export(10 * Y, rows, vec![couple(0, 1, 2)]);
    // [pigment, cognition, growth]: z_pig = key, z_K = key + 1000, z_h = key + 2000.
    shuffled.header.traits.rotate_left(1);
    let town = town(&shuffled);
    assert_eq!(town.p3().values, vec![(2001.5, 2003.0)]);
    assert_eq!(town.p9_polygenic().values, vec![(1.0, 3.0)]);
    assert_eq!(
        town.couple_pairs(Trait::Cognition).values,
        vec![(1001.0, 1002.0)]
    );
    assert_eq!(
        town.couple_pairs(Trait::Growth).values,
        vec![(2001.0, 2002.0)]
    );
}

#[test]
fn a_family_is_a_pair_of_rows_with_town_born_children_ordered_by_first_born_key() {
    let rows = vec![
        person(1, true, -30 * Y),
        person(2, false, -30 * Y),
        person(3, true, -30 * Y),
        person(4, false, -30 * Y),
        // An arrival is never a child, even with the parent keys of a family.
        Row {
            mother: Some(1),
            father: Some(2),
            ..arrival(0x10, true, -10 * Y, 0)
        },
        child(20, false, 10 * Y, 1, 2),
        child(21, true, 5 * Y, 1, 2),
        child(15, true, 8 * Y, 3, 4),
        // A pool child whose father is only a record has no family.
        Row {
            pool: 2,
            ..child(30, true, 6 * Y, 3, 0x99)
        },
        // Nor has a child without a father key.
        Row {
            father: None,
            ..child(31, false, 7 * Y, 1, 2)
        },
    ];
    let export = export(30 * Y, rows, vec![]);
    assert_eq!(
        town(&export).families().to_vec(),
        vec![
            Family {
                mother: 3,
                father: 4,
                children: vec![15],
            },
            Family {
                mother: 1,
                father: 2,
                children: vec![21, 20],
            },
        ]
    );
}

#[test]
fn dz_twins_take_the_first_born_by_multiple_index_not_by_key() {
    let rows = vec![
        person(1, true, -30 * Y),
        person(2, false, -30 * Y),
        Row {
            mult: Some(1),
            ..child(30, true, 5 * Y, 1, 2)
        },
        child(31, false, 5 * Y, 1, 2),
        child(32, false, 7 * Y, 1, 2),
    ];
    let export = export(30 * Y, rows, vec![]);
    let town = town(&export);
    assert_eq!(town.families()[0].children, vec![31, 30, 32]);
    assert_eq!(town.p3().anchors, vec![31]);
    assert_eq!(town.p5_siblings().anchors, vec![31, 30]);
}

/// Three families: MZ twins then a sibling (parents 1, 2); MZ twins alone (3, 4); one
/// child (5, 6).
fn sibships() -> Export {
    let mz = |key, gkey, mult, mother, father| Row {
        gkey,
        mult: Some(mult),
        ..child(key, true, 5 * Y, mother, father)
    };
    let rows = vec![
        person(1, true, -30 * Y),
        person(2, false, -30 * Y),
        person(3, true, -30 * Y),
        person(4, false, -30 * Y),
        person(5, true, -30 * Y),
        person(6, false, -30 * Y),
        mz(10, 0x77, 0, 1, 2),
        mz(11, 0x77, 1, 1, 2),
        child(12, false, 8 * Y, 1, 2),
        mz(20, 0x88, 0, 3, 4),
        mz(21, 0x88, 1, 3, 4),
        child(30, true, 7 * Y, 5, 6),
    ];
    export(40 * Y, rows, vec![])
}

#[test]
fn p5_siblings_skip_an_mz_co_twin() {
    let units = town(&sibships()).p5_siblings();
    assert_eq!(units.anchors, vec![10, 12]);
    assert_eq!(units.values, vec![(10.0, 12.0)]);
}

#[test]
fn p3_pairs_the_midparent_with_the_first_born() {
    let units = town(&sibships()).p3();
    assert_eq!(units.anchors, vec![10, 20, 30]);
    assert_eq!(units.values, vec![(1.5, 10.0), (3.5, 20.0), (5.5, 30.0)]);
}

#[test]
fn p9_pairs_the_mother_and_the_father_in_turn_with_the_first_born() {
    let units = town(&sibships()).p9_polygenic();
    assert_eq!(units.anchors, vec![1, 10, 4, 20, 5, 30]);
    assert_eq!(
        units.values,
        vec![(1001.0, 1010.0), (1004.0, 1020.0), (1005.0, 1030.0)]
    );
}

#[test]
fn p5_strangers_skip_shared_grandparents_and_shared_parents() {
    let rows = vec![
        // Two arrivals whose `PoolOrigin` mothers are the same record 0xa0, not a row.
        Row {
            mother: Some(0xa0),
            father: Some(0xa1),
            ..arrival(1, true, -30 * Y, 0)
        },
        person(2, false, -30 * Y),
        person(3, true, -30 * Y),
        Row {
            mother: Some(0xa0),
            father: Some(0xb1),
            ..arrival(4, false, -30 * Y, 0)
        },
        person(6, true, -30 * Y),
        person(7, true, -30 * Y),
        person(8, false, -30 * Y),
        person(9, true, -30 * Y),
        person(10, false, -30 * Y),
        person(11, true, -30 * Y),
        person(12, false, -30 * Y),
        person(13, false, -30 * Y),
        child(100, true, 5 * Y, 1, 2),   // pending
        child(101, true, 5 * Y, 3, 4),   // a cousin of 100 through 0xa0: dropped
        child(102, true, 5 * Y, 6, 2),   // a half-sib of 100 through 2: dropped
        child(103, true, 5 * Y, 7, 8),   // emitted with 100
        child(104, true, 5 * Y, 9, 10),  // pending
        child(105, true, 5 * Y, 11, 12), // emitted with 104
        child(106, true, 5 * Y, 11, 13), // pending at the end: no pair
    ];
    let export = export(40 * Y, rows, vec![]);
    let units = town(&export).p5_strangers();
    assert_eq!(units.anchors, vec![100, 103, 104, 105]);
    assert_eq!(units.values, vec![(100.0, 103.0), (104.0, 105.0)]);
}

fn parent(key: u64, female: bool, mc1r: u8) -> Row {
    Row {
        mc1r,
        ..person(key, female, -30 * Y)
    }
}

fn kid(key: u64, born: i64, mother: u64, father: u64, mc1r: u8) -> Row {
    Row {
        mc1r,
        ..child(key, true, born, mother, father)
    }
}

/// Family X: Aa x Aa with three children, the second affected. Family Y: aa x Aa with MZ
/// twins (the one of multiple index 0 has the larger key), both affected, then an
/// unaffected child.
fn mc1r_families() -> Vec<Row> {
    vec![
        parent(0x10, true, 1),
        parent(0x11, false, 1),
        kid(0x100, Y, 0x10, 0x11, 0),
        kid(0x101, 2 * Y, 0x10, 0x11, 2),
        kid(0x102, 3 * Y, 0x10, 0x11, 1),
        parent(0x20, true, 2),
        parent(0x21, false, 1),
        Row {
            gkey: 0x2000,
            mult: Some(1),
            ..kid(0x200, 4 * Y, 0x20, 0x21, 2)
        },
        Row {
            gkey: 0x2000,
            ..kid(0x201, 4 * Y, 0x20, 0x21, 2)
        },
        kid(0x202, 6 * Y, 0x20, 0x21, 1),
    ]
}

#[test]
fn p9_mc1r_takes_families_with_an_affected_child() {
    let mut rows = mc1r_families();
    // Six families of unaffected parents and one more whose father is Y's father: 17
    // distinct parents with 5 copies, q = 5 / 34.
    for i in 0..6 {
        rows.extend([
            parent(0x30 + 2 * i, true, 0),
            parent(0x31 + 2 * i, false, 0),
            kid(0x300 + i, 5 * Y, 0x30 + 2 * i, 0x31 + 2 * i, 0),
        ]);
    }
    rows.extend([parent(0x3c, true, 0), kid(0x306, 5 * Y, 0x3c, 0x21, 0)]);
    let export = export(40 * Y, rows, vec![]);
    let mc1r = town(&export).p9_mc1r().expect("q in the band");
    assert_eq!(mc1r.q.to_bits(), (5.0_f64 / 34.0).to_bits());
    // X: Aa x Aa, three zygotes (E_3), anchored at its affected child. Y: an aa parent, two
    // zygotes (GK-4), anchored at the twin of multiple index 0.
    assert_eq!(mc1r.units.anchors, vec![0x101, 0x201]);
    assert_eq!(
        mc1r.units.values,
        vec![(true, mc1r_expectation(3)), (false, mc1r_expectation(2))]
    );
    assert_eq!(mc1r.families, vec![0, 1]);
    let mut unaffected = vec![true; 9];
    unaffected[1] = false;
    assert_eq!(mc1r.unaffected, unaffected);
}

#[test]
fn p9_mc1r_outside_the_band_reports_q_and_no_sample() {
    let export = export(40 * Y, mc1r_families(), vec![]);
    // Four distinct parents with five copies.
    assert_eq!(
        town(&export).p9_mc1r().err(),
        Some(Finding::Mc1rFrequency { q: 0.625 })
    );
    // 25 families of 50 parents: the band [0.13, 0.17] holds its ends.
    for (copies, inside) in [(12, false), (13, true), (17, true), (18, false)] {
        let mut rows = Vec::new();
        for i in 0..25 {
            let (mother, father) = (0x1000 + 2 * i, 0x1001 + 2 * i);
            rows.extend([
                parent(mother, true, u8::from(mother - 0x1000 < copies)),
                parent(father, false, u8::from(father - 0x1000 < copies)),
                kid(0x2000 + i, 5 * Y, mother, father, 0),
            ]);
        }
        let export = export(40 * Y, rows, vec![]);
        let q = real(copies) / 100.0;
        match town(&export).p9_mc1r() {
            Ok(mc1r) => {
                assert!(inside, "{copies}");
                assert_eq!(mc1r.q.to_bits(), q.to_bits());
                assert!(mc1r.units.anchors.is_empty());
            }
            Err(finding) => {
                assert!(!inside, "{copies}");
                assert_eq!(finding, Finding::Mc1rFrequency { q });
            }
        }
    }
}

#[test]
fn p4_takes_each_mothers_first_child_of_the_decade_without_move_up() {
    let rows = vec![
        person(1, true, -20 * Y),
        person(2, true, -20 * Y),
        person(3, true, -20 * Y),
        arrival(4, true, -15 * Y, 0),
        person(5, false, -20 * Y),
        person(6, false, -20 * Y),
        person(7, false, -20 * Y),
        person(8, true, -20 * Y),
        // Mother 1: the oldest emigrates at 17, the next does not move up.
        Row {
            emig: Some(28 * Y),
            ..child(10, true, 11 * Y, 1, 5)
        },
        child(11, true, 12 * Y, 1, 5),
        // Mother 2: dies exactly at the 18th-birthday second (counts); her next child is
        // born exactly at the start of decade 2.
        Row {
            died: Some(31 * Y),
            ..child(20, true, 13 * Y, 2, 5)
        },
        child(50, true, 20 * Y, 2, 5),
        // Mother 3, remarried: two families, one unit, the earlier child.
        child(30, true, 15 * Y, 3, 6),
        child(31, true, 14 * Y, 3, 7),
        // An arrival's pool child with a record-only father.
        Row {
            pool: 2,
            ..child(40, true, 16 * Y, 4, 0x99)
        },
        // Mother 8: a child born exactly at the start of decade 1, and one in decade 2
        // who turns 18 only after the end.
        child(80, true, 10 * Y, 8, 7),
        child(81, true, 22 * Y, 8, 7),
    ];
    let export = export(40 * Y - 1, rows, vec![]);
    let town = town(&export);
    for decade in 0..DECADES {
        let want: Vec<u64> = match decade {
            1 => vec![20, 31, 40, 80],
            2 => vec![50],
            _ => vec![],
        };
        let units = town.p4_city(decade);
        assert_eq!(units.anchors, want, "decade {decade}");
        assert_eq!(units.values, reals(&want), "decade {decade}");
    }
    let families_of = |mother| {
        town.families()
            .iter()
            .filter(|family| family.mother == mother)
            .count()
    };
    assert_eq!((families_of(3), families_of(4)), (2, 0));
}

/// A village of five founders. Block 0: fathers 4, 5, mothers 1, 2, 3 (12 emigrates and
/// still counts). Block 1: no births. Block 2: father 11, mothers 10, 13, 14.
fn village_rows() -> Vec<Row> {
    let founder = |key, female| arrival(key, female, -20 * Y, 0);
    vec![
        founder(1, true),
        founder(2, true),
        founder(3, true),
        founder(4, false),
        founder(5, false),
        child(10, true, Y, 1, 4),
        child(11, false, 2 * Y, 2, 4),
        Row {
            emig: Some(20 * Y),
            ..child(12, true, 3 * Y, 3, 5)
        },
        child(13, true, 4 * Y, 1, 5),
        child(14, true, 5 * Y, 2, 5),
        child(20, true, 61 * Y, 10, 11),
        child(21, true, 62 * Y, 10, 11),
        Row {
            died: Some(70 * Y),
            ..child(22, true, 64 * Y, 13, 11)
        },
        child(23, true, 85 * Y, 13, 11),
        child(24, true, 70 * Y, 14, 11),
        // Block 3, after the measured block.
        child(25, true, 90 * Y, 14, 11),
    ]
}

#[test]
fn the_village_measures_the_last_block_whose_youngest_reach_18() {
    let export = export(108 * Y - 1, village_rows(), vec![]);
    let village = town(&export).village().expect("a village");
    // g* = (108 - 18) div 30 - 1 = 2: the birth years 60 to 89.
    assert_eq!(village.block, 2);
    // Mother 10: her first (20), not 21. Mother 13: her first (22) dies at 6, 23 does
    // not move up. Mother 14: 24; 25 is born in block 3.
    assert_eq!(village.units.anchors, vec![20, 24]);
    assert_eq!(village.units.values, vec![20.0, 24.0]);
    assert_eq!(village.blocks, vec![(2, 3), (1, 3)]);
}

#[test]
fn a_village_with_one_unit_or_nobody_alive_is_a_finding() {
    let one: Vec<Row> = village_rows()
        .into_iter()
        .filter(|row| row.key != 24)
        .collect();
    // Living at the end: 14 of the 16 rows (12 emigrated, 22 died), less 24.
    assert_eq!(
        town(&export(108 * Y - 1, one, vec![])).village().err(),
        Some(Finding::Village {
            units: 1,
            living: 13
        })
    );
    let dead: Vec<Row> = village_rows()
        .into_iter()
        .map(|row| Row {
            died: row.died.or(Some(107 * Y)),
            ..row
        })
        .collect();
    assert_eq!(
        town(&export(108 * Y - 1, dead, vec![])).village().err(),
        Some(Finding::Village {
            units: 2,
            living: 0
        })
    );
}

/// Couples of town-born persons, arrivals and a pool child, with two carried couples.
fn couples_town() -> Export {
    let rows = vec![
        person(1, true, -20 * Y),
        person(2, true, -21 * Y),
        person(3, true, -25 * Y),
        arrival(4, true, -35 * Y, 0),
        Row {
            pool: 2,
            ..child(5, true, Y, 4, 0x99)
        },
        person(6, true, -30 * Y),
        person(7, true, -20 * Y),
        person(11, false, -23 * Y),
        person(12, false, -23 * Y - Y / 2),
        person(13, false, -2 * Y),
        arrival(14, false, -40 * Y, 0),
        arrival(15, false, -33 * Y, 0),
        person(16, false, -24 * Y),
    ];
    let carried = |formed, woman, man| CoupleRow {
        carried: true,
        ..couple(formed, woman, man)
    };
    let couples = vec![
        carried(0, 4, 14),
        carried(5 * Y, 7, 16),
        couple(10 * Y, 1, 11),
        couple(11 * Y, 2, 11),
        couple(12 * Y, 1, 12),
        couple(13 * Y, 3, 12),
        couple(14 * Y, 5, 13),
        couple(17 * Y, 6, 15),
    ];
    export(40 * Y, rows, couples)
}

#[test]
fn pair_correlations_take_first_partnerships_of_town_born_partners() {
    let export = couples_town();
    let town = town(&export);
    // (7, 16) is carried; (2, 11) is 11's second, (1, 12) 1's second, (3, 12) 12's
    // second; (6, 15) has an arrival.
    let growth = town.couple_pairs(Trait::Growth);
    assert_eq!(growth.anchors, vec![1, 11, 5, 13]);
    assert_eq!(growth.values, vec![(1.0, 11.0), (5.0, 13.0)]);
    let cognition = town.couple_pairs(Trait::Cognition);
    assert_eq!(cognition.anchors, growth.anchors);
    assert_eq!(cognition.values, vec![(2001.0, 2011.0), (2005.0, 2013.0)]);
}

#[test]
fn the_age_gap_takes_every_couple_not_carried() {
    let units = town(&couples_town()).age_gap();
    assert_eq!(
        units.anchors,
        vec![1, 11, 2, 11, 1, 12, 3, 12, 5, 13, 6, 15]
    );
    assert_eq!(units.values, vec![3.0, 2.0, 3.5, -1.5, 3.0, 3.0]);
}

#[test]
fn the_first_partnership_is_the_earliest_couple_and_650_months_is_never() {
    let rows = vec![
        person(1, true, -5 * Y),
        person(2, true, -5 * Y),
        person(3, true, -5 * Y),
        Row {
            died: Some(44 * Y),
            ..person(4, true, -5 * Y)
        },
        arrival(5, true, -30 * Y, 0),
        child(6, true, 15 * Y, 1, 30),
        person(7, true, -5 * Y),
        person(8, true, -5 * Y),
        person(20, false, -5 * Y),
        person(21, false, -5 * Y),
        arrival(30, false, -40 * Y, 0),
    ];
    let couples = vec![
        couple(-5 * Y + 100, 7, 30),
        couple(18 * Y + 3 * MONTH + 10, 1, 30),
        couple(20 * Y, 1, 20),
        couple(45 * Y - 1, 8, 30),
        couple(45 * Y, 2, 30),
    ];
    let export = export(60 * Y - 1, rows, couples);
    let town = town(&export);
    // Women reaching 50 (4 dies at 49, 5 arrived, 6 turns 50 after the end): 7 in month
    // 0, 1 in month 23 * 13 + 3 = 302 (not her second couple), 8 in month 649; 2 at
    // exactly 650 months and 3 without a couple never.
    let women = town.first_partnerships(true);
    assert_eq!(women.anchors, vec![1, 2, 3, 7, 8]);
    let mut hist = [0; MONTHS];
    hist[0] = 1;
    hist[302] = 1;
    hist[649] = 1;
    assert_eq!((women.hist, women.never), (hist, 2));
    // Men: 20 in month 25 * 13 = 325, 21 never, 30 arrived.
    let men = town.first_partnerships(false);
    let mut hist = [0; MONTHS];
    hist[325] = 1;
    assert_eq!((men.anchors, men.hist, men.never), (vec![20, 21], hist, 1));
}

#[test]
fn births_count_what_a_woman_brought_and_her_town_children_before_50() {
    let rows = vec![
        // Arrived at 20 with two births, one town child before 50, one at 50.
        Row {
            brought: 2,
            ..arrival(1, true, -20 * Y, 0)
        },
        // Arrived at 51: out.
        Row {
            brought: 3,
            ..arrival(2, true, -51 * Y, 0)
        },
        person(3, false, -20 * Y),
        // Three town children, twins among them.
        person(4, true, -10 * Y),
        // Dies at 40: out.
        Row {
            died: Some(30 * Y),
            ..person(5, true, -10 * Y)
        },
        child(10, false, 5 * Y, 1, 3),
        child(11, false, 30 * Y, 1, 3),
        child(12, false, 2 * Y, 4, 3),
        child(13, false, 5 * Y, 4, 3),
        Row {
            mult: Some(1),
            ..child(14, false, 5 * Y, 4, 3)
        },
        child(15, false, 8 * Y, 5, 3),
    ];
    let export = export(60 * Y - 1, rows, vec![]);
    let units = town(&export).births();
    assert_eq!(units.anchors, vec![1, 4]);
    assert_eq!(units.values, vec![3, 3]);
}

#[test]
fn early_births_count_every_town_born_child_of_a_mother_under_18() {
    let rows = vec![
        person(1, true, -17 * Y),
        arrival(2, true, -15 * Y, 0),
        person(3, false, -30 * Y),
        child(10, true, 0, 1, 3),
        child(11, true, Y, 1, 3),
        // A pool child with a record-only father.
        Row {
            pool: 2,
            ..child(20, true, 2 * Y, 2, 0x99)
        },
        child(21, true, 10 * Y, 2, 3),
        Row {
            mult: Some(1),
            ..child(22, true, 10 * Y, 2, 3)
        },
    ];
    let export = export(30 * Y, rows, vec![]);
    let units = town(&export).early_births();
    // Mother's age 17, exactly 18, 17, 25, 25.
    assert_eq!(units.anchors, vec![10, 11, 20, 21, 22]);
    assert_eq!(units.values, vec![true, false, true, false, false]);
}

#[test]
fn the_life_table_counts_seconds_and_deaths_in_the_period_by_age() {
    let rows = vec![
        // Dies before the period starts.
        Row {
            died: Some(99 * Y),
            ..person(9, true, -10 * Y)
        },
        // Crosses the period start at 29 and her 30th birthday 1000 s later.
        Row {
            died: Some(100 * Y + 5000),
            ..child(1, true, 70 * Y + 1000, 9, 0x99)
        },
        // Arrives at 100 and a half.
        arrival(2, true, 49 * Y + Y / 2, 150 * Y),
        Row {
            emig: Some(120 * Y),
            ..child(3, true, 95 * Y, 9, 0x99)
        },
        // Dies at the first second of the period, at 90.
        Row {
            died: Some(100 * Y),
            ..child(4, true, 10 * Y, 9, 0x99)
        },
        // Alive at the end.
        child(6, true, 175 * Y, 2, 0x99),
        Row {
            died: Some(150 * Y + 10),
            ..child(7, false, 60 * Y, 9, 0x99)
        },
        Row {
            emig: Some(99 * Y),
            ..person(8, false, -10 * Y)
        },
    ];
    let export = export(180 * Y - 1, rows, vec![]);
    let town = town(&export);
    let women = town.life_table(true);
    let mut exposure = [0; AGES];
    exposure[..25].fill(YS);
    exposure[29] = 1000;
    exposure[30] = 4000;
    exposure[100] = 30 * YS;
    let mut deaths = [0; AGES];
    deaths[30] = 1;
    deaths[90] = 1;
    assert_eq!(women.anchors, vec![1, 2, 3, 4, 6]);
    assert_eq!((women.exposure, women.deaths), (exposure, deaths));
    let men = town.life_table(false);
    let mut exposure = [0; AGES];
    exposure[40..90].fill(YS);
    exposure[90] = 10;
    let mut deaths = [0; AGES];
    deaths[90] = 1;
    assert_eq!(men.anchors, vec![7]);
    assert_eq!((men.exposure, men.deaths), (exposure, deaths));
}

#[test]
fn the_longevity_sample_takes_town_born_lives_ended_in_the_town() {
    let rows = vec![
        Row {
            died: Some(99 * Y),
            ..person(9, true, -10 * Y)
        },
        Row {
            died: Some(80 * Y + Y / 2),
            ..child(1, true, 0, 9, 0x99)
        },
        Row {
            emig: Some(40 * Y),
            ..child(2, true, 10 * Y, 9, 0x99)
        },
        child(3, true, 20 * Y, 9, 0x99),
        // Born at 90 years: out.
        child(4, true, 90 * Y, 9, 0x99),
        Row {
            died: Some(70 * Y),
            ..arrival(5, true, 10 * Y, 30 * Y)
        },
        Row {
            died: Some(100 * Y),
            ..child(6, true, 89 * Y, 9, 0x99)
        },
        Row {
            died: Some(60 * Y),
            ..child(7, false, 5 * Y, 9, 0x99)
        },
    ];
    let export = export(200 * Y - 1, rows, vec![]);
    let town = town(&export);
    let women = town.longevity(true);
    assert_eq!(women.units.anchors, vec![1, 6]);
    assert_eq!(women.units.values, vec![(80.5, 3001.0), (11.0, 3006.0)]);
    assert_eq!((women.emigrated, women.living), (1, 1));
    let men = town.longevity(false);
    assert_eq!(men.units.anchors, vec![7]);
    assert_eq!(men.units.values, vec![(55.0, 3007.0)]);
    assert_eq!((men.emigrated, men.living), (0, 0));
}
