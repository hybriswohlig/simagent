//! Addressed random draws (ADR-0001): every draw is a pure function of the world seed
//! and a key. Neither the world nor a save nor the scheduler holds generator state; a
//! new draw somewhere never shifts another one.

mod select;
mod tag;
pub mod tags;

pub use tag::PurposeTag;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use sim_math::dist;

use crate::lineage::{GenomeKey, LineageKey};

/// The seed of a world (scenario field `world_seed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldSeed(u64);

impl WorldSeed {
    pub const fn new(seed: u64) -> WorldSeed {
        WorldSeed(seed)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Whose draw it is. Only from a lineage key or `WORLD`, never from a counter ID or a
/// slot (ADR-0001 section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubjectKey(u64);

impl SubjectKey {
    /// For tags that concern the world as a whole.
    pub const WORLD: SubjectKey = SubjectKey(0);

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<LineageKey> for SubjectKey {
    fn from(key: LineageKey) -> SubjectKey {
        let _ = key;
        SubjectKey::WORLD
    }
}

impl From<GenomeKey> for SubjectKey {
    fn from(key: GenomeKey) -> SubjectKey {
        let _ = key;
        SubjectKey::WORLD
    }
}

/// The address of one draw. `occasion` and `detail` come only from values that are the
/// same in every fork: absolute calendar indices, counters of the subject's own state,
/// lineage keys, small fixed numbers, `str_key` of a data ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawKey {
    pub tag: PurposeTag,
    pub subject: SubjectKey,
    pub occasion: u64,
    pub detail: u64,
}

/// Domains of `absorb`: eight ASCII bytes as a little-endian number.
pub(crate) const DRAW: u64 = u64::from_le_bytes(*b"sa.draw1");
pub(crate) const RANK: u64 = u64::from_le_bytes(*b"sa.rank1");
pub(crate) const CHILD: u64 = u64::from_le_bytes(*b"sa.chld1");
pub(crate) const FOUNDER: u64 = u64::from_le_bytes(*b"sa.fndr1");
pub(crate) const IMMIGRANT: u64 = u64::from_le_bytes(*b"sa.immi1");

/// The SplitMix64 finaliser. `wrapping_mul`: overflow checks are on in release too.
pub(crate) const fn mix64(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Mixes `words` into one value, per domain and lane.
pub(crate) fn absorb(domain: u64, lane: u64, words: &[u64]) -> u64 {
    let start = mix64(domain ^ lane.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    words.iter().fold(start, |h, word| mix64(h ^ word))
}

/// The ChaCha key of the address `[seed, tag, subject, occasion, detail]`: four lanes of
/// `absorb(DRAW, lane, address)`, little-endian, 32 bytes.
fn address_key(address: [u64; 5]) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (lane, chunk) in (0_u64..).zip(bytes.chunks_exact_mut(8)) {
        chunk.copy_from_slice(&absorb(DRAW, lane, &address).to_le_bytes());
    }
    bytes
}

/// The ChaCha key of a draw.
pub(crate) fn chacha_key(seed: WorldSeed, key: DrawKey) -> [u8; 32] {
    address_key([
        seed.get(),
        key.tag.get(),
        key.subject.get(),
        key.occasion,
        key.detail,
    ])
}

/// The first `N` words of the ChaCha8 key stream of `key`: stream 0, counter 0, so word
/// k is bytes 8k to 8k + 7 as little-endian.
fn key_words<const N: usize>(key: [u8; 32]) -> [u64; N] {
    const {
        assert!(
            N >= 1 && N <= 8,
            "a draw has 1 to 8 words: the first ChaCha block"
        );
    }
    let mut generator = ChaCha8Rng::from_seed(key);
    core::array::from_fn(|_| generator.next_u64())
}

/// The draws of one world. All methods take `&self`: there is no state to advance.
#[derive(Clone, Copy, Debug)]
pub struct Draws {
    seed: WorldSeed,
}

impl Draws {
    pub const fn new(seed: WorldSeed) -> Draws {
        Draws { seed }
    }

    /// The `N` words of the draw `key`, `1 <= N <= 8`. The same key gives the same
    /// words, whatever was drawn before.
    ///
    /// More than eight words do not compile (the budget of one ChaCha block); a larger
    /// need is spread over several keys:
    ///
    /// ```compile_fail
    /// use sim_core::rng::{tags, DrawKey, Draws, SubjectKey, WorldSeed};
    /// let key = DrawKey { tag: tags::DEATH_BUDGET, subject: SubjectKey::WORLD, occasion: 0, detail: 0 };
    /// let _ = Draws::new(WorldSeed::new(1)).words::<9>(key);
    /// ```
    ///
    /// ```compile_fail
    /// use sim_core::rng::{tags, DrawKey, Draws, SubjectKey, WorldSeed};
    /// let key = DrawKey { tag: tags::DEATH_BUDGET, subject: SubjectKey::WORLD, occasion: 0, detail: 0 };
    /// let _ = Draws::new(WorldSeed::new(1)).words::<0>(key);
    /// ```
    ///
    /// ```
    /// use sim_core::rng::{tags, DrawKey, Draws, SubjectKey, WorldSeed};
    /// let key = DrawKey { tag: tags::DEATH_BUDGET, subject: SubjectKey::WORLD, occasion: 0, detail: 0 };
    /// let draws = Draws::new(WorldSeed::new(1));
    /// assert_eq!(draws.words::<8>(key)[..2], draws.words::<2>(key));
    /// ```
    pub fn words<const N: usize>(&self, key: DrawKey) -> [u64; N] {
        key_words(chacha_key(self.seed, key))
    }

    /// Uniform on `0..n`, `n >= 1` (one word).
    pub fn uniform_below(&self, key: DrawKey, n: u64) -> u64 {
        let [w] = self.words(key);
        dist::uniform_below(w, n)
    }

    /// True with probability `p` in [0, 1] (one word).
    pub fn bernoulli(&self, key: DrawKey, p: f64) -> bool {
        let [w] = self.words(key);
        dist::bernoulli(w, p)
    }

    /// Exponential with `rate > 0` (one word).
    pub fn exponential(&self, key: DrawKey, rate: f64) -> f64 {
        let [w] = self.words(key);
        dist::exponential(w, rate)
    }

    /// Normal with `mean` and `sd >= 0` (two words).
    pub fn normal(&self, key: DrawKey, mean: f64, sd: f64) -> f64 {
        let [w0, w1] = self.words(key);
        dist::normal(w0, w1, mean, sd)
    }

    /// Poisson with `lambda` in [0, 30] (one word).
    pub fn poisson(&self, key: DrawKey, lambda: f64) -> u32 {
        let [w] = self.words(key);
        dist::poisson(w, lambda)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::{DataId, str_key};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    /// The pinned tag with this value. A tag exists only through the pinned list, so a
    /// test cannot make one up either.
    fn pinned(value: u64) -> Option<PurposeTag> {
        tags::ALL
            .iter()
            .map(|(tag, _)| *tag)
            .find(|tag| tag.get() == value)
    }

    /// A known answer: seed, `[tag, subject, occasion, detail]`, ChaCha key, words.
    type Case = (u64, [u64; 4], &'static [u8; 64], [u64; 8]);

    /// 64 lowercase hex digits as 32 bytes.
    fn unhex(digits: &[u8; 64]) -> [u8; 32] {
        let nibble = |digit: u8| match digit {
            b'0'..=b'9' => digit - b'0',
            b'a'..=b'f' => digit - b'a' + 10,
            other => panic!("not a lowercase hex digit: {other}"),
        };
        core::array::from_fn(|k| (nibble(digits[2 * k]) << 4) | nibble(digits[2 * k + 1]))
    }

    /// ADR-0001: the zero key gives the first block of test vector TC1 (256-bit key, 8
    /// rounds) of draft-strombergson-chacha-test-vectors-01. Bytes copied from the
    /// draft, not from a run.
    #[test]
    fn chacha8_reference_vector() {
        let block: [u8; 64] = [
            0x3e, 0x00, 0xef, 0x2f, 0x89, 0x5f, 0x40, 0xd6, 0x7f, 0x5b, 0xb8, 0xe8, 0x1f, 0x09,
            0xa5, 0xa1, 0x2c, 0x84, 0x0e, 0xc3, 0xce, 0x9a, 0x7f, 0x3b, 0x18, 0x1b, 0xe1, 0x88,
            0xef, 0x71, 0x1a, 0x1e, 0x98, 0x4c, 0xe1, 0x72, 0xb9, 0x21, 0x6f, 0x41, 0x9f, 0x44,
            0x53, 0x67, 0x45, 0x6d, 0x56, 0x19, 0x31, 0x4a, 0x42, 0xa3, 0xda, 0x86, 0xb0, 0x01,
            0x38, 0x7b, 0xfd, 0xb8, 0x0e, 0x0c, 0xfe, 0x42,
        ];
        let expected: [u64; 8] = core::array::from_fn(|k| {
            let mut word = [0; 8];
            word.copy_from_slice(&block[8 * k..8 * k + 8]);
            u64::from_le_bytes(word)
        });
        assert_eq!(expected[0], 0xd640_5f89_2fef_003e);
        assert_eq!(key_words::<8>([0; 32]), expected);
        assert_eq!(key_words::<1>([0; 32]), [expected[0]]);
        assert_eq!(key_words::<3>([0; 32]), expected[..3]);
    }

    /// ADR-0001: the first three outputs of splitmix64.c (Vigna) with start state 0.
    #[test]
    fn mix64_reference() {
        let gamma: u64 = 0x9e37_79b9_7f4a_7c15;
        let outputs = [1_u64, 2, 3].map(|k| mix64(k.wrapping_mul(gamma)));
        assert_eq!(
            outputs,
            [
                0xe220_a839_7b1d_cdaf,
                0x6e78_9e6a_a1b9_65f4,
                0x06c4_5d18_8009_454f
            ]
        );
    }

    /// Values from tools/rng_reference.py.
    #[test]
    fn absorb_known_answers() {
        let tag = str_key(&DataId::new("test:fork_no_density").expect("valid"));
        assert_eq!(absorb(FOUNDER, 0, &[tag, 0]), 0xcfd3_d455_1fb3_63c2);
        assert_eq!(absorb(RANK, 0, &[7, 5, 11, 13, 17]), 0x7030_8ebc_37d8_4ef5);
        assert_eq!(
            DRAW, 0x3177_6172_642e_6173,
            "the bytes of sa.draw1, little-endian"
        );
        // The domains differ in their bytes 3 to 6: `draw`, `rank`, `chld`, `fndr`, `immi`.
        assert_eq!(RANK, 0x316b_6e61_722e_6173);
        assert_eq!(CHILD, 0x3164_6c68_632e_6173);
        assert_eq!(FOUNDER, 0x3172_646e_662e_6173);
        assert_eq!(IMMIGRANT, 0x3169_6d6d_692e_6173);
    }

    /// ADR-0001 section 1: a subject is a lineage key (for meiosis the genome key) or
    /// the world.
    #[test]
    fn subjects_come_from_lineage_keys() {
        let tag = DataId::new("test:fork_no_density").expect("valid");
        let founder = LineageKey::founder(&tag, 0);
        assert_eq!(SubjectKey::from(founder).get(), founder.get());
        assert_eq!(SubjectKey::from(founder).get(), 0xcfd3_d455_1fb3_63c2);
        let other = LineageKey::founder(&tag, 1);
        assert_eq!(
            SubjectKey::from(GenomeKey::of_zygote(other)).get(),
            0x1e0b_b476_2d39_fa10
        );
    }

    /// ADR-0001: six fixed draw keys give their ChaCha keys and all eight words. Expected
    /// values from tools/rng_reference.py (an own ChaCha8, itself checked against TC1),
    /// written from the ADR alone.
    #[test]
    fn draw_key_known_answers() {
        let base = str_key(&DataId::new("test:base").expect("valid"));
        assert_eq!(base, 0x8b10_65ec_5b12_5f0b);
        assert_eq!(SubjectKey::WORLD, SubjectKey(0), "ADR-0001 section 1");
        let cases: [Case; 6] = [
            (
                0,
                [1, 0, 0, 0],
                b"9d993f1db982e868666f7147c2937fee885f05dd66d375225bd754184c7f587c",
                [
                    0x7729_5f8d_fed2_f427,
                    0xc9dc_21ed_1c5d_0056,
                    0xf632_a5dc_0e37_5236,
                    0x0e60_f459_6ae6_9752,
                    0x17e7_0bde_146f_7fbb,
                    0x8a49_8d82_f850_f49b,
                    0x4158_9dea_c79c_18f9,
                    0x23ae_40a5_7f91_0542,
                ],
            ),
            (
                1,
                [1, 0, 0, 0],
                b"10269ee609d4431fbd78e5b0cf9ca0fed1624e843fe009df445193885ab3ce62",
                [
                    0x97fb_9d9a_6737_d38a,
                    0x6d3e_4d87_a534_8032,
                    0x1910_eca7_e4ec_156a,
                    0x3a67_9682_e45e_72f4,
                    0x1ca8_2ab4_7ebd_b131,
                    0x1d38_3c64_64bb_2a86,
                    0xa230_8868_33e4_0073,
                    0x6bae_4c46_426d_b1bd,
                ],
            ),
            (
                0,
                [2, 0, 0, 0],
                b"fc3b46af4ea46640d605f0f1772b073516e171e309d7c6f50f5397d6d9481dd8",
                [
                    0x75a5_a2f3_f8ae_86c1,
                    0x2fb7_2810_fb7b_d626,
                    0x26fe_ff43_3c70_169c,
                    0xa4f7_f9f5_a14f_0435,
                    0x595f_cbb6_6941_5986,
                    0xade3_0b72_7f1b_a4e3,
                    0xb8c1_9a1c_926e_4d6c,
                    0xd8bb_ffd0_ff39_82e9,
                ],
            ),
            (
                0x0123_4567_89ab_cdef,
                [3, 0xfedc_ba98_7654_3210, 7, (5 << 32) | 9],
                b"3e6462278e219359501a21b05c6b5333fc4ba6335031d664d972137331d1e4ed",
                [
                    0x421b_c5a5_8249_0b42,
                    0x1eb3_f317_e709_0652,
                    0xc747_5c77_284d_33ca,
                    0xd049_23ae_4386_fd4a,
                    0x6aca_1168_af3c_1b4a,
                    0x86ed_0fd8_cb26_206c,
                    0x3603_2cf5_3313_faaf,
                    0x17ea_d0cc_7699_6a34,
                ],
            ),
            (
                u64::MAX,
                [0xffff_ffff, u64::MAX, u64::MAX, u64::MAX],
                b"7eba6c984ad75faa6be45a3f2c04b9c80f5ce23b8308d8cb8389db973e3eca47",
                [
                    0xdf52_bd7f_b7aa_1ca1,
                    0x7040_9af7_b8cf_4bc8,
                    0xcb50_bc4c_4636_70a0,
                    0xfde3_f5fd_6f06_e156,
                    0xc7ab_d4b2_d05c_3c94,
                    0x285b_9d25_799c_91a1,
                    0xa665_0883_249b_1487,
                    0x97bd_0756_91f3_e194,
                ],
            ),
            (
                0x5eed,
                [4, 0, base, 0],
                b"928df35109f9a4f5d20fa14f05eda3f04bb2a414a9238e43c147be49e704f952",
                [
                    0x5c6e_4534_67bf_d0f8,
                    0xe826_84dd_c43b_90d7,
                    0xb333_6ecd_52eb_d970,
                    0xfd62_3e00_8024_0739,
                    0x9d9b_9e08_e332_1719,
                    0x531c_14f7_0b2a_15a6,
                    0x2793_8797_7177_2e77,
                    0xb839_0044_7923_dece,
                ],
            ),
        ];
        let mut typed = 0;
        for (seed, [tag, subject, occasion, detail], chacha, words) in cases {
            let key = address_key([seed, tag, subject, occasion, detail]);
            assert_eq!(key, unhex(chacha), "seed {seed:x}, tag {tag:x}");
            assert_eq!(key_words::<8>(key), words, "seed {seed:x}, tag {tag:x}");
            // The same through the typed path, where the tag is a pinned one.
            let Some(tag) = pinned(tag) else { continue };
            let draw = DrawKey {
                tag,
                subject: SubjectKey(subject),
                occasion,
                detail,
            };
            let seed = WorldSeed::new(seed);
            assert_eq!(chacha_key(seed, draw), unhex(chacha), "{draw:?}");
            assert_eq!(Draws::new(seed).words::<8>(draw), words, "{draw:?}");
            assert_eq!(Draws::new(seed).words::<2>(draw), words[..2], "{draw:?}");
            typed += 1;
        }
        assert_eq!(
            typed, 5,
            "every case with a pinned tag also runs through DrawKey"
        );
    }

    /// ADR-0001 sections 1 and 3: a draw is its distribution applied to the first words
    /// of the key. The exact values are from tools/rng_reference.py (lines `chain`).
    /// exponential and normal are compared with the functions of sim-math on the words;
    /// their known answers are there.
    #[test]
    fn draws_apply_the_distributions_to_the_first_words() {
        let base = str_key(&DataId::new("test:base").expect("valid"));
        // seed, [tag, subject, occasion, detail], uniform_below 7 and 2^40 + 3,
        // bernoulli 0.3 and 0.6, poisson 5
        let cases = [
            (0, [1, 0, 0, 0], 3, 511_795_236_352, false, true, 5),
            (1, [1, 0, 0, 0], 4, 652_761_471_592, false, true, 5),
            (0, [2, 0, 0, 0], 3, 505_290_093_562, false, true, 5),
            (
                0x0123_4567_89ab_cdef,
                [3, 0xfedc_ba98_7654_3210, 7, (5 << 32) | 9],
                1,
                283_933_779_331,
                true,
                true,
                3,
            ),
            (0x5eed, [4, 0, base, 0], 2, 396_987_020_392, false, true, 4),
        ];
        for (seed, [tag, subject, occasion, detail], seven, large, rare, likely, count) in cases {
            let draws = Draws::new(WorldSeed::new(seed));
            let key = DrawKey {
                tag: pinned(tag).expect("a pinned tag"),
                subject: SubjectKey(subject),
                occasion,
                detail,
            };
            assert_eq!(draws.uniform_below(key, 7), seven, "{key:?}");
            assert_eq!(draws.uniform_below(key, (1 << 40) + 3), large, "{key:?}");
            assert_eq!(draws.bernoulli(key, 0.3), rare, "{key:?}");
            assert_eq!(draws.bernoulli(key, 0.6), likely, "{key:?}");
            assert_eq!(draws.poisson(key, 5.0), count, "{key:?}");
            let [w0, w1] = draws.words::<2>(key);
            assert_eq!(
                draws.exponential(key, 2.0).to_bits(),
                dist::exponential(w0, 2.0).to_bits(),
                "{key:?}"
            );
            assert_eq!(
                draws.normal(key, 1.0, 3.0).to_bits(),
                dist::normal(w0, w1, 1.0, 3.0).to_bits(),
                "{key:?}"
            );
            assert!(draws.exponential(key, 2.0) > 0.0, "{key:?}");
            assert_ne!(draws.normal(key, 1.0, 3.0), 1.0, "{key:?}");
        }
    }

    /// ADR-0001 `words_smoke`: 2^18 draw keys with a fixed tag, the world as subject and
    /// `occasion = i`. Each of the 64 bits of the first word is set in half of the cases,
    /// within 5 standard errors (sqrt(n) / 2 = 256).
    #[test]
    fn words_smoke() {
        const N: u64 = 1 << 18;
        let draws = Draws::new(WorldSeed::new(0x5eed));
        let mut set = [0_u64; 64];
        for occasion in 0..N {
            let [word] = draws.words::<1>(DrawKey {
                tag: tags::MEIOSIS_START_STRAND,
                subject: SubjectKey::WORLD,
                occasion,
                detail: 0,
            });
            for (bit, count) in set.iter_mut().enumerate() {
                *count += (word >> bit) & 1;
            }
        }
        for (bit, count) in set.iter().enumerate() {
            assert!(
                count.abs_diff(N / 2) <= 5 * 256,
                "bit {bit} is set {count} times of {N}"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            rng_seed: RngSeed::Fixed(0x5c4e_d001),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// ADR-0001 `draws_are_pure`: the same key gives the same words, whatever is
        /// drawn in between and in which order; every changed field and another seed
        /// change the ChaCha key.
        #[test]
        fn draws_are_pure(
            seed in any::<u64>(),
            tag in 0..tags::ALL.len(),
            fields in any::<[u64; 3]>(),
            others in prop::collection::vec((0..tags::ALL.len(), any::<[u64; 3]>()), 0..8),
            address in any::<[u64; 5]>(),
            change in 0_usize..5,
            delta in 1_u64..=u64::MAX,
        ) {
            let draws = Draws::new(WorldSeed::new(seed));
            let make = |tag: usize, f: [u64; 3]| DrawKey {
                tag: tags::ALL[tag].0,
                subject: SubjectKey(f[0]),
                occasion: f[1],
                detail: f[2],
            };
            let first = draws.words::<8>(make(tag, fields));
            for (other_tag, other) in &others {
                let _ = draws.words::<8>(make(*other_tag, *other));
            }
            prop_assert_eq!(draws.words::<8>(make(tag, fields)), first);
            for (other_tag, other) in others.iter().rev() {
                let _ = draws.words::<3>(make(*other_tag, *other));
            }
            prop_assert_eq!(
                Draws::new(WorldSeed::new(seed)).words::<8>(make(tag, fields)),
                first
            );

            // Any of seed, tag, subject, occasion and detail, over the whole u64 range.
            let mut changed = address;
            changed[change] = address[change].wrapping_add(delta);
            prop_assert_ne!(address_key(changed), address_key(address));
        }
    }
}
