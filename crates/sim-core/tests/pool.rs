//! The reader of the pool file (design note of the pool file, section 7) against the files
//! of `tools/pool_reference.py` in `tests/golden/pool/`: every pool file of the reference
//! reads and writes back byte for byte, and each rejected file breaks exactly its rule.

use sim_core::pool::{Pool, PoolError};

const FILES: [(&str, &[u8]); 5] = [
    ("pool_a", include_bytes!("../../../tests/golden/pool/pool_a.bin")),
    ("pool_b", include_bytes!("../../../tests/golden/pool/pool_b.bin")),
    ("pool_c", include_bytes!("../../../tests/golden/pool/pool_c.bin")),
    ("pool_d", include_bytes!("../../../tests/golden/pool/pool_d.bin")),
    ("pool_e", include_bytes!("../../../tests/golden/pool/pool_e.bin")),
];

/// The reference's rejected files (`pool_reject`), each made from `pool_b` with one change.
const REJECTED: [(&[u8], PoolError); 8] = [
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
fn each_rejected_file_breaks_its_rule() {
    for (bytes, rule) in REJECTED {
        assert_eq!(Pool::from_bytes(bytes).err(), Some(rule), "{}", rule.code());
    }
}

/// A file cut short does not decode.
#[test]
fn a_file_cut_short_does_not_decode() {
    let (_, bytes) = FILES[1];
    let short = &bytes[..bytes.len() - 1];
    assert_eq!(Pool::from_bytes(short).err(), Some(PoolError::Decode));
}
