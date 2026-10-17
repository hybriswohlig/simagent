//! The reader of the pool file (design note of the pool file, section 7) against the files
//! of `tools/pool_reference.py` in `tests/golden/pool/`: every pool file of the reference
//! reads and writes back byte for byte, and each rejected file breaks exactly its rule.

use sim_core::genome::Sex;
use sim_core::lineage::LineageKey;
use sim_core::pool::{Pool, PoolError};
use sim_core::time::SimTime;

const FILES: [(&str, &[u8]); 6] = [
    (
        "pool_a",
        include_bytes!("../../../tests/golden/pool/pool_a.bin"),
    ),
    (
        "pool_b",
        include_bytes!("../../../tests/golden/pool/pool_b.bin"),
    ),
    (
        "pool_c",
        include_bytes!("../../../tests/golden/pool/pool_c.bin"),
    ),
    (
        "pool_d",
        include_bytes!("../../../tests/golden/pool/pool_d.bin"),
    ),
    (
        "pool_e",
        include_bytes!("../../../tests/golden/pool/pool_e.bin"),
    ),
    (
        "pool_f",
        include_bytes!("../../../tests/golden/pool/pool_f.bin"),
    ),
];

/// The reference's rejected files (`pool_reject`), each made from `pool_b` with one change; the
/// record rules' from `pool_c` (design note of the pool's records).
const REJECTED: [(&[u8], PoolError); 18] = [
    (
        include_bytes!("../../../tests/golden/pool/reject/magic.bin"),
        PoolError::Magic,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/version.bin"),
        PoolError::Version,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/trailing_bytes.bin"),
        PoolError::TrailingBytes,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/household_without_adult.bin"),
        PoolError::HouseholdWithoutAdult,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/members_out_of_order.bin"),
        PoolError::MembersOutOfOrder,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/ancestors_out_of_order.bin"),
        PoolError::AncestorsOutOfOrder,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/parent_not_a_record.bin"),
        PoolError::ParentNotARecord,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/partnership_not_members.bin"),
        PoolError::PartnershipNotMembers,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/frozen_at.bin"),
        PoolError::FrozenAt,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/partnership_sex.bin"),
        PoolError::PartnershipSex,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/pregnancy_mother.bin"),
        PoolError::PregnancyMother,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/birth_at.bin"),
        PoolError::BirthAt,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/conception_at.bin"),
        PoolError::ConceptionAt,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/conception_month.bin"),
        PoolError::ConceptionMonth,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/separation_at.bin"),
        PoolError::SeparationAt,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/leave_home_at.bin"),
        PoolError::LeaveHomeAt,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/carried_time.bin"),
        PoolError::CarriedTime,
    ),
    (
        include_bytes!("../../../tests/golden/pool/reject/past_time.bin"),
        PoolError::PastTime,
    ),
];

#[test]
fn every_pool_file_reads_and_writes_back() {
    for (name, bytes) in FILES {
        let pool = Pool::from_bytes(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(pool.to_bytes() == bytes, "{name}: written back differently");
        assert!(!pool.households.is_empty(), "{name}");
    }
}

#[test]
fn each_rejected_pool_file_breaks_its_rule() {
    for (bytes, rule) in REJECTED {
        assert_eq!(Pool::from_bytes(bytes).err(), Some(rule), "{rule}");
    }
}

/// The stamps of each pool run (`<run>_stamp` of the reference): the hash of its run's data
/// and the hash of the frozen data it was extracted from (design note of the pool's
/// records, section 6), `pool_c`'s from its scenario `father_dies`, every other the
/// template's.
const STAMPS: [(&str, u64, u64); 6] = [
    ("pool_a", 0xb17a_fdcd_2961_75ac, 0x5ae3_d70c_71cf_063a),
    ("pool_b", 0xb17a_fdcd_2961_75ac, 0x5ae3_d70c_71cf_063a),
    ("pool_c", 0xeb9f_b406_aa14_2f6a, 0xe2f2_44a7_e4cd_db5f),
    ("pool_d", 0xb17a_fdcd_2961_75ac, 0x5ae3_d70c_71cf_063a),
    ("pool_e", 0xb17a_fdcd_2961_75ac, 0x5ae3_d70c_71cf_063a),
    ("pool_f", 0xb17a_fdcd_2961_75ac, 0x5ae3_d70c_71cf_063a),
];

/// Format 2 carries `frozen_hash` right after `data_hash`; the reader reads both stamps.
#[test]
fn each_pool_file_carries_its_stamps() {
    for ((name, bytes), (stamped, data_hash, frozen_hash)) in FILES.iter().zip(STAMPS) {
        assert_eq!(*name, stamped);
        let pool = Pool::from_bytes(bytes).expect("a pool");
        assert_eq!(pool.data_hash, data_hash, "{name}");
        assert_eq!(pool.frozen_hash, frozen_hash, "{name}");
    }
}

/// A file of format 1 (`reject/version.bin`: `pool_b` with the version 1 in place of 2) is
/// rejected with `pool.version` before its body is read (design note of the pool's
/// records, section 6).
#[test]
fn a_pool_file_of_format_1_is_rejected() {
    let magic = sim_core::pool::MAGIC.len();
    let (_, pool_b) = FILES[1];
    let (version, _) = REJECTED[1];
    assert_eq!(pool_b[magic], 2);
    assert_eq!(version[magic], 1);
    assert_eq!(version[..magic], pool_b[..magic]);
    assert_eq!(version[magic + 1..], pool_b[magic + 1..]);
    let error = Pool::from_bytes(version).expect_err("format 1");
    assert_eq!(error, PoolError::Version);
    assert_eq!(error.to_string(), "pool.version");
}

/// A file cut short does not decode.
#[test]
fn a_pool_file_cut_short_does_not_decode() {
    let (_, bytes) = FILES[1];
    let short = &bytes[..bytes.len() - 1];
    assert_eq!(Pool::from_bytes(short).err(), Some(PoolError::Decode));
}

/// The bytes of `pool_b` with the varint at `at` written one byte longer (the last byte
/// gets the continuation bit, a zero byte follows): the same value, not the core's form.
fn overlong_at(at: usize) -> Vec<u8> {
    let (_, bytes) = FILES[1];
    let mut end = at;
    while bytes[end] & 0x80 != 0 {
        end += 1;
    }
    let mut changed = bytes[..end].to_vec();
    changed.push(bytes[end] | 0x80);
    changed.push(0);
    changed.extend_from_slice(&bytes[end + 1..]);
    changed
}

/// An overlong varint, of the version or of the first field (the pool seed), decodes to
/// the same values but is not what the core writes: rejected, so that an accepted file's
/// hash is the hash of its pool (review).
#[test]
fn a_noncanonical_pool_file_is_rejected() {
    let magic = sim_core::pool::MAGIC.len();
    for at in [magic, magic + 1] {
        assert_eq!(
            Pool::from_bytes(&overlong_at(at)).err(),
            Some(PoolError::NonCanonical),
            "varint at {at}"
        );
    }
}

/// Each rule's code, as the design note names it (section 7).
#[test]
fn each_pool_rule_has_its_code() {
    let codes = [
        (PoolError::Magic, "pool.magic"),
        (PoolError::Version, "pool.version"),
        (PoolError::Decode, "pool.decode"),
        (PoolError::TrailingBytes, "pool.trailing_bytes"),
        (
            PoolError::HouseholdWithoutAdult,
            "pool.household_without_adult",
        ),
        (PoolError::MembersOutOfOrder, "pool.members_out_of_order"),
        (
            PoolError::AncestorsOutOfOrder,
            "pool.ancestors_out_of_order",
        ),
        (PoolError::ParentNotARecord, "pool.parent_not_a_record"),
        (
            PoolError::PartnershipNotMembers,
            "pool.partnership_not_members",
        ),
        (PoolError::NonCanonical, "pool.non_canonical"),
        (PoolError::FrozenAt, "pool.frozen_at"),
        (PoolError::PartnershipSex, "pool.partnership_sex"),
        (PoolError::PregnancyMother, "pool.pregnancy_mother"),
        (PoolError::BirthAt, "pool.birth_at"),
        (PoolError::ConceptionAt, "pool.conception_at"),
        (PoolError::ConceptionMonth, "pool.conception_month"),
        (PoolError::SeparationAt, "pool.separation_at"),
        (PoolError::LeaveHomeAt, "pool.leave_home_at"),
        (PoolError::CarriedTime, "pool.carried_time"),
        (PoolError::PastTime, "pool.past_time"),
    ];
    for (error, code) in codes {
        assert_eq!(error.to_string(), code);
    }
}

/// `pool_b` read, changed by `change`, written again: the reader's verdict on it.
fn changed(change: impl Fn(&mut Pool)) -> Result<Pool, PoolError> {
    let (_, bytes) = FILES[1];
    let mut pool = Pool::from_bytes(bytes).expect("pool_b");
    change(&mut pool);
    Pool::from_bytes(&pool.to_bytes())
}

/// A key that is no record: `pool_b`'s largest record key plus one (keys are unique).
fn not_a_record(pool: &Pool) -> LineageKey {
    let largest = pool.ancestors.last().expect("records").lineage_key;
    serde_json::from_str(&format!("\"{:016x}\"", largest.get() + 1)).expect("a key")
}

/// "Strictly ascending": a member or a record twice is out of order (mutants).
#[test]
fn a_pool_member_or_record_twice_is_out_of_order() {
    let twice_member = changed(|pool| {
        let household = pool
            .households
            .iter_mut()
            .find(|h| h.members.len() == 1)
            .expect("a household of one");
        let member = household.members[0].clone();
        household.members.push(member);
    });
    assert_eq!(twice_member.err(), Some(PoolError::MembersOutOfOrder));
    let twice_record = changed(|pool| {
        let record = pool.ancestors[0];
        pool.ancestors.insert(0, record);
    });
    assert_eq!(twice_record.err(), Some(PoolError::AncestorsOutOfOrder));
}

/// One unknown parent is enough: a member's, a record's or an unborn child's father alone
/// that is no record (mutants).
#[test]
fn one_pool_parent_that_is_no_record_is_rejected() {
    let member = changed(|pool| {
        let unknown = not_a_record(pool);
        let person = pool
            .households
            .iter_mut()
            .flat_map(|h| h.members.iter_mut())
            .find(|m| m.father.is_some())
            .expect("a member with a father");
        person.father = Some(unknown);
    });
    assert_eq!(member.err(), Some(PoolError::ParentNotARecord));
    let record = changed(|pool| {
        let unknown = not_a_record(pool);
        let record = pool
            .ancestors
            .iter_mut()
            .find(|r| r.father.is_some())
            .expect("a record with a father");
        record.father = Some(unknown);
    });
    assert_eq!(record.err(), Some(PoolError::ParentNotARecord));
    let unborn = changed(|pool| {
        let unknown = not_a_record(pool);
        let pregnancy = pool
            .households
            .iter_mut()
            .flat_map(|h| h.pregnancies.iter_mut())
            .next()
            .expect("a pregnancy");
        pregnancy.father = unknown;
    });
    assert_eq!(unborn.err(), Some(PoolError::ParentNotARecord));
}

/// `pool_c` read, changed by `change`, written again: the reader's verdict on it. `pool_c`
/// has partnerships, pregnancies and carried series, which the record rules read.
fn changed_c(change: impl Fn(&mut Pool)) -> Result<Pool, PoolError> {
    let (_, bytes) = FILES[2];
    let mut pool = Pool::from_bytes(bytes).expect("pool_c");
    change(&mut pool);
    Pool::from_bytes(&pool.to_bytes())
}

/// The first household with a pregnancy, and the first whose woman carries a series.
fn with_pregnancy(pool: &Pool) -> usize {
    pool.households
        .iter()
        .position(|h| !h.pregnancies.is_empty())
        .expect("a pregnancy")
}

fn with_series(pool: &Pool) -> (usize, usize) {
    pool.households
        .iter()
        .enumerate()
        .find_map(|(h, household)| {
            household
                .members
                .iter()
                .position(|m| m.conception_at.is_some())
                .map(|m| (h, m))
        })
        .expect("a carried series")
}

fn shifted(t: SimTime, by: i64) -> SimTime {
    SimTime::from_secs(t.as_secs() + by)
}

/// The record rules one condition at a time (design note of the pool's records, section 1;
/// review): each change breaks exactly the rule named, the boundary just inside passes.
#[test]
fn each_record_rule_rejects_each_of_its_conditions() {
    let pool = Pool::from_bytes(FILES[2].1).expect("pool_c");
    let p = with_pregnancy(&pool);
    let mother = pool.households[p].pregnancies[0].mother;
    let (s, w) = with_series(&pool);
    let at_home = |pool: &Pool| {
        pool.households
            .iter()
            .enumerate()
            .find_map(|(h, household)| {
                household
                    .members
                    .iter()
                    .position(|m| !m.moved_out)
                    .map(|m| (h, m))
            })
            .expect("one at home")
    };
    let (hh, mh) = at_home(&pool);
    let moved = pool
        .households
        .iter()
        .enumerate()
        .find_map(|(h, household)| {
            household
                .members
                .iter()
                .position(|m| m.moved_out && m.lineage_key != mother)
                .map(|m| (h, m))
        })
        .expect("one moved out");
    let flip = |sex: Sex| match sex {
        Sex::Female => Sex::Male,
        Sex::Male => Sex::Female,
    };
    let cases: Vec<(&str, Result<Pool, PoolError>, Option<PoolError>)> = vec![
        // N2: the mother a member, female, once, in the members' order.
        (
            "mother no member",
            changed_c(|pool| {
                let stranger = pool.households[p + 1].members[0].lineage_key;
                pool.households[p].pregnancies[0].mother = stranger;
            }),
            Some(PoolError::PregnancyMother),
        ),
        (
            // A single mother: with a partnership her sex would break N1 first.
            "mother male",
            changed_c(|pool| {
                let single = pool
                    .households
                    .iter_mut()
                    .find(|h| !h.pregnancies.is_empty() && h.partnership.is_none())
                    .expect("a single mother");
                let key = single.pregnancies[0].mother;
                let her = single
                    .members
                    .iter_mut()
                    .find(|m| m.lineage_key == key)
                    .expect("her");
                her.sex = flip(her.sex);
            }),
            Some(PoolError::PregnancyMother),
        ),
        (
            "two pregnancies of one mother",
            changed_c(|pool| {
                let again = pool.households[p].pregnancies[0].clone();
                pool.households[p].pregnancies.push(again);
            }),
            Some(PoolError::PregnancyMother),
        ),
        // P2 both ways.
        (
            "birth without pregnancy",
            changed_c(|pool| {
                let at = shifted(pool.frozen_at, 1);
                pool.households[hh].members[mh].birth_at = Some(at);
            }),
            Some(PoolError::BirthAt),
        ),
        (
            "pregnancy without birth",
            changed_c(|pool| {
                let her = pool.households[p]
                    .members
                    .iter_mut()
                    .find(|m| m.lineage_key == mother)
                    .expect("her");
                her.birth_at = None;
            }),
            Some(PoolError::BirthAt),
        ),
        // P1: only the partnership's woman, not pregnant.
        (
            "series of a pregnant woman",
            changed_c(|pool| {
                let at = pool.households[s].members[w].conception_at;
                let her = pool.households[p]
                    .members
                    .iter_mut()
                    .find(|m| m.lineage_key == mother)
                    .expect("her");
                her.conception_at = at;
            }),
            Some(PoolError::ConceptionAt),
        ),
        (
            "series of another member",
            changed_c(|pool| {
                let at = pool.households[s].members[w].conception_at;
                let other = (0..pool.households[s].members.len())
                    .find(|m| *m != w)
                    .expect("another member");
                pool.households[s].members[w].conception_at = None;
                pool.households[s].members[other].conception_at = at;
            }),
            Some(PoolError::ConceptionAt),
        ),
        // N3 and P5's boundary for the series.
        (
            "series off the month grid",
            changed_c(|pool| {
                let at = pool.households[s].members[w].conception_at.expect("set");
                pool.households[s].members[w].conception_at = Some(shifted(at, 1));
            }),
            Some(PoolError::ConceptionMonth),
        ),
        // P4 both ways.
        (
            "leaving home planned for one moved out",
            changed_c(|pool| {
                let at = shifted(pool.frozen_at, 1);
                pool.households[moved.0].members[moved.1].leave_home_at = Some(at);
            }),
            Some(PoolError::LeaveHomeAt),
        ),
        (
            "no leaving home for one at home",
            changed_c(|pool| pool.households[hh].members[mh].leave_home_at = None),
            Some(PoolError::LeaveHomeAt),
        ),
        // P5 for each field: at the freeze rejected, one second later accepted.
        (
            "series at the freeze",
            changed_c(|pool| pool.households[s].members[w].conception_at = Some(pool.frozen_at)),
            Some(PoolError::CarriedTime),
        ),
        (
            "birth at the freeze",
            changed_c(|pool| {
                let at = pool.frozen_at;
                let her = pool.households[p]
                    .members
                    .iter_mut()
                    .find(|m| m.lineage_key == mother)
                    .expect("her");
                her.birth_at = Some(at);
            }),
            Some(PoolError::CarriedTime),
        ),
        (
            "birth just after the freeze",
            changed_c(|pool| {
                let at = shifted(pool.frozen_at, 1);
                let her = pool.households[p]
                    .members
                    .iter_mut()
                    .find(|m| m.lineage_key == mother)
                    .expect("her");
                her.birth_at = Some(at);
            }),
            None,
        ),
        (
            "leaving home at the freeze",
            changed_c(|pool| {
                pool.households[hh].members[mh].leave_home_at = Some(pool.frozen_at);
            }),
            Some(PoolError::CarriedTime),
        ),
        (
            "leaving home just after the freeze",
            changed_c(|pool| {
                let at = shifted(pool.frozen_at, 1);
                pool.households[hh].members[mh].leave_home_at = Some(at);
            }),
            None,
        ),
        // N4: born, formed and conceived at the freeze pass, one second later not.
        (
            "born at the freeze",
            changed_c(|pool| pool.households[hh].members[mh].born = pool.frozen_at),
            None,
        ),
        (
            "partnership formed after the freeze",
            changed_c(|pool| {
                let at = shifted(pool.frozen_at, 1);
                let pair = pool.households[s].partnership.as_mut().expect("a pair");
                pair.formed = at;
            }),
            Some(PoolError::PastTime),
        ),
        (
            "conceived at the freeze",
            changed_c(|pool| pool.households[p].pregnancies[0].conceived = pool.frozen_at),
            None,
        ),
        (
            "conceived after the freeze",
            changed_c(|pool| {
                pool.households[p].pregnancies[0].conceived = shifted(pool.frozen_at, 1);
            }),
            Some(PoolError::PastTime),
        ),
        // Precedence: a pool breaking the freeze and a later rule reports the freeze.
        (
            "freeze off a year start and a past time",
            changed_c(|pool| {
                pool.households[p].pregnancies[0].conceived = shifted(pool.frozen_at, 2);
                pool.frozen_at = shifted(pool.frozen_at, 1);
            }),
            Some(PoolError::FrozenAt),
        ),
    ];
    for (name, verdict, expected) in cases {
        assert_eq!(verdict.err(), expected, "{name}");
    }
}

/// N1 and N2 by one partner and by the order of two mothers (mutants): one partner of the
/// wrong sex is enough; two pregnancies of different mothers pass in the members' order and
/// break `pregnancy_mother` the other way round.
#[test]
fn one_partner_and_the_order_of_two_mothers_decide() {
    let pool = Pool::from_bytes(FILES[2].1).expect("pool_c");
    let (s, _) = with_series(&pool);
    let flip_partner = |woman: bool| {
        changed_c(move |pool| {
            let household = &mut pool.households[s];
            let pair = household.partnership.expect("a pair");
            let key = if woman { pair.woman } else { pair.man };
            let partner = household
                .members
                .iter_mut()
                .find(|m| m.lineage_key == key)
                .expect("the partner");
            partner.sex = match partner.sex {
                Sex::Female => Sex::Male,
                Sex::Male => Sex::Female,
            };
        })
    };
    assert_eq!(flip_partner(true).err(), Some(PoolError::PartnershipSex));
    assert_eq!(flip_partner(false).err(), Some(PoolError::PartnershipSex));
    // Two mothers in one household: a couple with a pregnancy, the partnership taken away
    // (with the woman's separation), the man made the second, female mother.
    let h = pool
        .households
        .iter()
        .position(|household| !household.pregnancies.is_empty() && household.partnership.is_some())
        .expect("a couple with a pregnancy");
    let two_mothers = |in_order: bool| {
        changed_c(move |pool| {
            let frozen_at = pool.frozen_at;
            let household = &mut pool.households[h];
            let pair = household.partnership.take().expect("the pair");
            for member in &mut household.members {
                member.separation_at = None;
            }
            let place = |key| {
                household
                    .members
                    .iter()
                    .position(|m| m.lineage_key == key)
                    .expect("a member")
            };
            let (first_place, other) = (place(household.pregnancies[0].mother), place(pair.man));
            let him = &mut household.members[other];
            him.sex = Sex::Female;
            him.birth_at = Some(shifted(frozen_at, 1));
            let mut second = household.pregnancies[0].clone();
            second.mother = pair.man;
            if (first_place < other) == in_order {
                household.pregnancies.push(second);
            } else {
                household.pregnancies.insert(0, second);
            }
        })
    };
    assert_eq!(two_mothers(true).err(), None);
    assert_eq!(two_mothers(false).err(), Some(PoolError::PregnancyMother));
}
