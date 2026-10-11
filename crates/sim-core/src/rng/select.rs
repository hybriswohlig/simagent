//! Choice from a set (ADR-0001 section 5). Each admissible candidate gets a rank from
//! `absorb`, the only exception to "bits only from ChaCha": the rank serves the order
//! and nothing else, and one generator per candidate would cost far more.

use alloc::vec::Vec;

use super::{Draws, PurposeTag, RANK, SubjectKey, WorldSeed, absorb};
use crate::lineage::LineageKey;

/// The rank of `candidate` for `seeker` (a lineage key or the world) on `occasion`.
fn rank(
    seed: WorldSeed,
    tag: PurposeTag,
    seeker: impl Into<SubjectKey>,
    occasion: u64,
    candidate: LineageKey,
) -> u64 {
    absorb(
        RANK,
        0,
        &[
            seed.get(),
            tag.get(),
            seeker.into().get(),
            occasion,
            candidate.get(),
        ],
    )
}

impl Draws {
    /// The `k` candidates with the smallest rank, ascending by `(rank, lineage key)`;
    /// all of them if there are fewer than `k`. An additional candidate changes the
    /// result only if its rank falls among the `k` smallest. The candidates are
    /// distinct (they come from the lineage index).
    pub fn k_smallest(
        &self,
        tag: PurposeTag,
        seeker: LineageKey,
        occasion: u64,
        candidates: impl IntoIterator<Item = LineageKey>,
        k: usize,
    ) -> Vec<LineageKey> {
        let mut ranked: Vec<(u64, LineageKey)> = candidates
            .into_iter()
            .map(|candidate| (rank(self.seed, tag, seeker, occasion, candidate), candidate))
            .collect();
        ranked.sort_unstable();
        ranked.truncate(k);
        ranked.into_iter().map(|(_, candidate)| candidate).collect()
    }

    /// All `candidates` ascending by `(rank, lineage key)` with the derivation of
    /// `k_smallest` for the subject `subject` (ADR-0018 section 1: `SubjectKey::WORLD` for
    /// the order of a round; a seeker's key for her list of candidates).
    pub fn ranked(
        &self,
        tag: PurposeTag,
        subject: SubjectKey,
        occasion: u64,
        candidates: impl IntoIterator<Item = LineageKey>,
    ) -> Vec<LineageKey> {
        let mut ranked: Vec<(u64, LineageKey)> = candidates
            .into_iter()
            .map(|candidate| {
                (
                    rank(self.seed, tag, subject, occasion, candidate),
                    candidate,
                )
            })
            .collect();
        ranked.sort_unstable();
        ranked.into_iter().map(|(_, candidate)| candidate).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::rng::tags;
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;
    use std::collections::BTreeSet;

    fn founders() -> Vec<LineageKey> {
        let tag = DataId::new("test:fork_no_density").expect("valid");
        (0..8)
            .map(|index| LineageKey::founder(&tag, index))
            .collect()
    }

    /// The order of a round and a list of candidates (ADR-0018 section 1), from
    /// `tools/world_reference.py` (`market_order`, `market_candidates`): seed 0xff, month
    /// 7, the founders of `test:clan`.
    #[test]
    fn ranked_orders_like_the_reference() {
        let tag = DataId::new("test:clan").expect("valid");
        let key = |index: u64| LineageKey::founder(&tag, index);
        let draws = Draws::new(WorldSeed::new(0xff));
        let order = draws.ranked(tags::MARKET_ORDER, SubjectKey::WORLD, 7, (0..6).map(key));
        let ids: Vec<u64> = order.iter().map(|k| k.get()).collect();
        assert_eq!(
            ids,
            [
                0x030f_c55b_9597_25fa,
                0xc4f5_6b66_3468_9905,
                0xa7fc_4f83_dacf_d279,
                0x3e8b_5cde_ad66_f5f6,
                0xedfa_4535_9651_22f1,
                0xd0a1_58c1_6220_f6dd,
            ]
        );
        let seeker = SubjectKey::from(key(0));
        let list = draws.ranked(tags::PAIRING_RANK, seeker, 7, (1..6).map(key));
        let ids: Vec<u64> = list.iter().map(|k| k.get()).collect();
        assert_eq!(
            ids,
            [
                0xa7fc_4f83_dacf_d279,
                0xc4f5_6b66_3468_9905,
                0xedfa_4535_9651_22f1,
                0x3e8b_5cde_ad66_f5f6,
                0x030f_c55b_9597_25fa,
            ]
        );
        // With a seeker's key the order is that of `k_smallest`.
        assert_eq!(
            draws.k_smallest(tags::PAIRING_RANK, key(0), 7, (1..6).map(key), 5),
            list
        );
    }

    /// Values from tools/rng_reference.py (lines `ranks` and `k_smallest`): seed 0x5eed,
    /// tag PAIRING_RANK, founder 0 seeks among founders 1 to 7 in round 3.
    #[test]
    fn k_smallest_known_answers() {
        let all = founders();
        let seed = WorldSeed::new(0x5eed);
        let ranks = [
            0xe2ac_0cdb_ab1c_5b5d_u64,
            0x6b26_1bfd_2f0b_feaf,
            0xe474_88bd_300e_a69b,
            0xf834_7d83_980c_68c7,
            0x3682_3562_8164_a5d2,
            0x683b_7ce1_7db3_1b62,
            0x6548_3be8_76e1_559f,
        ];
        for (candidate, expected) in all[1..].iter().zip(ranks) {
            assert_eq!(
                rank(seed, tags::PAIRING_RANK, all[0], 3, *candidate),
                expected
            );
        }
        let draws = Draws::new(seed);
        let chosen =
            |k: usize| draws.k_smallest(tags::PAIRING_RANK, all[0], 3, all[1..].iter().copied(), k);
        let of = |indices: &[usize]| -> Vec<LineageKey> {
            indices.iter().map(|index| all[*index]).collect()
        };
        assert_eq!(chosen(0), of(&[]));
        assert_eq!(chosen(1), of(&[5]));
        assert_eq!(chosen(3), of(&[5, 7, 6]));
        assert_eq!(chosen(7), of(&[5, 7, 6, 2, 1, 3, 4]));
        assert_eq!(chosen(9), of(&[5, 7, 6, 2, 1, 3, 4]), "fewer than k: all");
        // The order of the candidates does not matter.
        let reversed = draws.k_smallest(
            tags::PAIRING_RANK,
            all[0],
            3,
            all[1..].iter().rev().copied(),
            3,
        );
        assert_eq!(reversed, of(&[5, 7, 6]));
        // Another round, another seeker, another tag and another seed rank differently.
        let base = rank(seed, tags::PAIRING_RANK, all[0], 3, all[1]);
        assert_ne!(rank(seed, tags::PAIRING_RANK, all[0], 4, all[1]), base);
        assert_ne!(rank(seed, tags::PAIRING_RANK, all[2], 3, all[1]), base);
        assert_ne!(rank(seed, tags::DEATH_BUDGET, all[0], 3, all[1]), base);
        assert_ne!(
            rank(
                WorldSeed::new(0x5eee),
                tags::PAIRING_RANK,
                all[0],
                3,
                all[1]
            ),
            base
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            rng_seed: RngSeed::Fixed(0x5c4e_d002),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// ADR-0001 `extra_candidate_changes_only_own_slot`: an additional candidate
        /// changes the choice only if its rank falls among the k smallest. Then it
        /// displaces exactly the largest element so far; with fewer than k candidates
        /// it is only added.
        #[test]
        fn extra_candidate_changes_only_own_slot(
            seed in any::<u64>(),
            occasion in any::<u64>(),
            indices in prop::collection::btree_set(0_u64..64, 2..40),
            k in 0_usize..12,
        ) {
            let tag = DataId::new("test:fork_no_density").expect("valid");
            let mut keys: Vec<LineageKey> = indices
                .iter()
                .map(|index| LineageKey::founder(&tag, *index))
                .collect();
            let seeker = keys.pop().expect("at least two");
            let extra = keys.pop().expect("at least two");
            let draws = Draws::new(WorldSeed::new(seed));
            let before = draws.k_smallest(tags::PAIRING_RANK, seeker, occasion, keys.iter().copied(), k);
            let mut with_extra = keys.clone();
            with_extra.insert(0, extra);
            let after = draws.k_smallest(tags::PAIRING_RANK, seeker, occasion, with_extra, k);

            // The result is sorted by (rank, key), has no duplicates and the right size.
            let order = |key: &LineageKey| {
                (rank(WorldSeed::new(seed), tags::PAIRING_RANK, seeker, occasion, *key), *key)
            };
            prop_assert_eq!(before.len(), k.min(keys.len()));
            prop_assert_eq!(after.len(), k.min(keys.len() + 1));
            prop_assert!(after.windows(2).all(|pair| order(&pair[0]) < order(&pair[1])));
            prop_assert!(before.iter().all(|key| keys.contains(key)));

            // The choice itself, by another way than sorting: a candidate is chosen
            // exactly if fewer than k candidates order before it.
            let chosen_by_count = |pool: &[LineageKey]| -> BTreeSet<LineageKey> {
                pool.iter()
                    .copied()
                    .filter(|key| pool.iter().filter(|other| order(other) < order(key)).count() < k)
                    .collect()
            };
            let before_by_count = chosen_by_count(&keys);
            prop_assert_eq!(before.iter().copied().collect::<BTreeSet<_>>(), before_by_count);
            let mut pool = keys.clone();
            pool.push(extra);
            let after_by_count = chosen_by_count(&pool);
            prop_assert_eq!(after.iter().copied().collect::<BTreeSet<_>>(), after_by_count);

            let before_set: BTreeSet<LineageKey> = before.iter().copied().collect();
            let after_set: BTreeSet<LineageKey> = after.iter().copied().collect();
            if after_set.contains(&extra) {
                let displaced: Vec<&LineageKey> = before_set.difference(&after_set).collect();
                if before.len() < k {
                    prop_assert!(displaced.is_empty(), "fewer than k: only added");
                } else {
                    let largest = before.last().expect("k > 0");
                    prop_assert_eq!(displaced, [largest]);
                    prop_assert!(order(&extra) < order(largest), "its rank is among the k smallest");
                }
                let mut without: Vec<LineageKey> = after.clone();
                without.retain(|key| *key != extra);
                prop_assert_eq!(&without[..], &before[..without.len()]);
            } else {
                prop_assert_eq!(&after, &before);
                // The extra candidate ranks behind everything chosen.
                prop_assert!(after.iter().all(|key| order(key) < order(&extra)));
            }
        }
    }
}
