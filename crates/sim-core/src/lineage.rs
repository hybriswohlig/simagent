//! Identity across forks (ADR-0001 section 4). A lineage key is fixed at conception (for
//! founders and immigrants at their creation), saved and never computed again. It names
//! the same person in every fork that shares the conception; the `SimId` is only a
//! counter and differs between forks. The world seed does not enter: this is identity,
//! not chance. Keys are built only here.

use alloc::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::data_id::{DataId, str_key};
use crate::rng::{CHILD, FOUNDER, IMMIGRANT, PREVIEW, absorb};

/// The key of one person across forks. In JSON 16 hex digits, in postcard the plain
/// u64 (ADR-0004 section 3). Reading a saved key is the only way to get one that was
/// not built here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineageKey(#[serde(with = "crate::hex_u64")] u64);

impl LineageKey {
    /// A founder: `index` is the position from 0 in the founder list of the resolved
    /// scenario, which is never reordered.
    pub fn founder(founder_tag: &DataId, index: u64) -> LineageKey {
        LineageKey(absorb(FOUNDER, 0, &[str_key(founder_tag), index]))
    }

    /// A child of its biological parents. `conception_number` counts all conceptions of
    /// the mother from 0 (with any father, also ended pregnancies, multiples as one; a
    /// founder starts at the number of her earlier children). `multiple_index` from 0.
    pub fn child(
        mother: LineageKey,
        father: LineageKey,
        conception_number: u64,
        multiple_index: u64,
    ) -> LineageKey {
        LineageKey(absorb(
            CHILD,
            0,
            &[mother.0, father.0, conception_number, multiple_index],
        ))
    }

    /// A hypothetical child of a couple, shown before a pairing (Grok's round 44, K1):
    /// `index` from 0 to 3. No person of a run ever has this key.
    pub fn preview(mother: LineageKey, father: LineageKey, index: u64) -> LineageKey {
        LineageKey(absorb(PREVIEW, 0, &[mother.0, father.0, index]))
    }

    /// An immigrant (provisional until the ADR on immigration): the year of arrival and
    /// the number within that year, from 0.
    pub fn immigrant(founder_tag: &DataId, arrival_year: u64, number_in_year: u64) -> LineageKey {
        LineageKey(absorb(
            IMMIGRANT,
            0,
            &[str_key(founder_tag), arrival_year, number_in_year],
        ))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The key all meiosis draws of a person are addressed by: the own lineage key, for
/// identical multiples that of the first child of the zygote. Meiosis and mutation
/// depend only on the genomes of the parents and on these draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenomeKey(LineageKey);

impl GenomeKey {
    /// `first_child` is the first child of the zygote: the person itself unless it is a
    /// later identical multiple.
    pub const fn of_zygote(first_child: LineageKey) -> GenomeKey {
        GenomeKey(first_child)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// All lineage keys of a world: the living, the archive and the running pregnancies. It
/// is not saved; loading builds it again.
#[derive(Debug, Default)]
pub struct LineageIndex {
    keys: BTreeSet<LineageKey>,
}

impl LineageIndex {
    pub const fn new() -> LineageIndex {
        LineageIndex {
            keys: BTreeSet::new(),
        }
    }

    /// Adds the key of a new founder, immigrant or conception. A key that is already
    /// there breaks the invariant `lineage_key_unique`.
    pub fn insert(&mut self, key: LineageKey) {
        if !self.keys.insert(key) {
            panic!("invariant lineage_key_unique: {:016x}", key.get());
        }
    }

    pub fn contains(&self, key: LineageKey) -> bool {
        self.keys.contains(&key)
    }

    /// Takes out the key of a pregnancy that ended with the death of the mother: it is
    /// no longer in the living, the archive or a pregnancy.
    pub fn remove(&mut self, key: LineageKey) {
        self.keys.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag() -> DataId {
        DataId::new("test:fork_no_density").expect("valid")
    }

    /// ADR-0001 `key_rules`: the keys against tools/rng_reference.py.
    #[test]
    fn key_rules() {
        let tag = tag();
        let founders = [
            0xcfd3_d455_1fb3_63c2_u64,
            0x1e0b_b476_2d39_fa10,
            0x67f9_1ca5_ac92_0b44,
            0xfebc_bfdf_1511_ac38,
        ];
        for (index, expected) in (0_u64..).zip(founders) {
            assert_eq!(
                LineageKey::founder(&tag, index).get(),
                expected,
                "founder {index}"
            );
        }
        let other = DataId::new("test:base").expect("valid");
        assert_ne!(LineageKey::founder(&other, 0), LineageKey::founder(&tag, 0));

        let mother = LineageKey::founder(&tag, 0);
        let father = LineageKey::founder(&tag, 1);
        let child = |conception, multiple| LineageKey::child(mother, father, conception, multiple);
        assert_eq!(child(0, 0).get(), 0xaa82_2181_2fa6_6dd0);
        assert_eq!(child(0, 1).get(), 0x83c3_1937_33be_1f72);
        assert_eq!(child(1, 0).get(), 0x308d_db93_9866_2649);
        // Mother and father are not interchangeable.
        assert_eq!(
            LineageKey::child(father, mother, 0, 0).get(),
            0x3eea_b302_2594_ef9a
        );

        assert_eq!(
            LineageKey::immigrant(&tag, 5, 0).get(),
            0x0426_879e_a2d0_7419
        );
        assert_eq!(
            LineageKey::immigrant(&tag, 5, 1).get(),
            0x9015_2f84_4b85_ce0c
        );
        assert_eq!(
            LineageKey::immigrant(&tag, 6, 0).get(),
            0x3edb_a8dd_3912_f5db
        );
    }

    /// ADR-0001 `key_rules`: identical twins share the genome key, fraternal twins do
    /// not; both pairs have two lineage keys.
    #[test]
    fn twins() {
        let tag = tag();
        let mother = LineageKey::founder(&tag, 0);
        let father = LineageKey::founder(&tag, 1);
        let first = LineageKey::child(mother, father, 0, 0);
        let second = LineageKey::child(mother, father, 0, 1);
        assert_ne!(first, second);
        // Identical: one zygote, its first child is `first`.
        assert_eq!(GenomeKey::of_zygote(first), GenomeKey::of_zygote(first));
        assert_eq!(GenomeKey::of_zygote(first).get(), first.get());
        // Fraternal: two zygotes, each child is the first of its own.
        assert_ne!(GenomeKey::of_zygote(first), GenomeKey::of_zygote(second));
    }

    /// ADR-0001 `key_rules`: a repeated conception number breaks `lineage_key_unique`.
    #[test]
    #[should_panic(expected = "invariant lineage_key_unique: aa8221812fa66dd0")]
    fn a_repeated_conception_number_breaks_the_invariant() {
        let tag = tag();
        let mother = LineageKey::founder(&tag, 0);
        let father = LineageKey::founder(&tag, 1);
        let mut index = LineageIndex::default();
        index.insert(mother);
        index.insert(father);
        index.insert(LineageKey::child(mother, father, 0, 0));
        index.insert(LineageKey::child(mother, father, 1, 0));
        index.insert(LineageKey::child(mother, father, 0, 0));
    }

    #[test]
    fn the_index_knows_its_keys() {
        let tag = tag();
        let mut index = LineageIndex::default();
        assert!(!index.contains(LineageKey::founder(&tag, 0)));
        index.insert(LineageKey::founder(&tag, 0));
        index.insert(LineageKey::founder(&tag, 1));
        assert!(index.contains(LineageKey::founder(&tag, 0)));
        assert!(index.contains(LineageKey::founder(&tag, 1)));
        assert!(!index.contains(LineageKey::founder(&tag, 2)));
    }
}
