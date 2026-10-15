//! P7 (genetics specification, test rules): couples correlate at 0.4 in cognitive ability
//! and at 0.2 in height (4.2), measured on the adult phenotypes of the two partners in
//! first partnerships, at least 1000 pairs, within 0.1267 on Fisher's z scale (W8.7a,
//! W8.7c). The correlation comes from the partner market alone (11.2): the weights `w_K`
//! and `w_H` of the repository's life course data were fitted to it by
//! `tools/life_reference.py`.
//!
//! The town: the test map `tests/golden/data_pairing` (40 own loci per trait, 4.1, 4.2),
//! the repository's life course data with the wish always 0 (no children: the founders
//! alone, P7 counts first partnerships) and no separation; 1500 women and 1500 men born
//! between 16 and 30 years before the start, their alleles at the trait loci drawn with
//! frequency 0.5. Thirty years of market.

mod common;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::input::{Founder, Input, Start};
use sim_core::lineage::LineageKey;
use sim_core::params::WorldParams;
use sim_core::phenotype::phenotype;
use sim_core::rng::{Draws, WorldSeed};
use sim_core::time::{SimDuration, SimTime};
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
/// Women and men each.
const PER_SEX: usize = 1500;
/// The pairs P7 counts (spec 8.1: unit pair, at least 1000).
const PAIRS: usize = 1000;
/// Gate seed of the founders (protected list; never reseeded).
const SEED_FOUNDERS: u64 = 0x5eed_0007;
/// Gate seed of the permutation of the negative twin (protected list; never reseeded).
const SEED_SHUFFLE: u64 = 0x5eed_0008;
const WORLD_SEED: u64 = 0x07;

const CALIBRATION: &str =
    include_str!("../../../tests/golden/data_pairing/calibration/genetics.json");
const GENOME_MAP: &str =
    include_str!("../../../tests/golden/data_pairing/genetics/genome_map.json");

/// The test map with the repository's life course data, the wish always 0 and no
/// separation.
fn town_data() -> Data {
    let (calibration, genome_map) = common::files_from(CALIBRATION, GENOME_MAP);
    let mut demography = common::demography();
    demography["values"]["core:separation_rate"]["value"] = serde_json::json!(0.0);
    demography["fertility_wish_shape"] = serde_json::json!([{"value": 1.0, "unit": "fraction"}]);
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

/// SplitMix64 (Vigna).
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The 80 trait loci of the test map.
fn trait_loci() -> Vec<DataId> {
    let map: serde_json::Value = serde_json::from_str(GENOME_MAP).expect("json");
    map["loci"]
        .as_array()
        .expect("loci")
        .iter()
        .filter_map(|locus| locus["id"].as_str())
        .filter(|id| id.starts_with("test:cognition_") || id.starts_with("test:height_"))
        .map(|id| DataId::new(id).expect("id"))
        .collect()
}

/// The women, then the men: born a whole number of seconds between 16 and 30 years before
/// the start, each allele at the trait loci the top bit of the next number.
fn founders() -> Vec<Founder> {
    let loci = trait_loci();
    let mut state = SEED_FOUNDERS;
    let span = u64::try_from(14 * Y).expect("positive");
    [Sex::Female, Sex::Male]
        .into_iter()
        .flat_map(|sex| std::iter::repeat_n(sex, PER_SEX))
        .map(|sex| {
            let offset = i64::try_from(splitmix(&mut state) % span).expect("small");
            let alleles = loci
                .iter()
                .filter_map(|locus| {
                    let copies = [splitmix(&mut state) >> 63, splitmix(&mut state) >> 63]
                        .map(|bit| u8::try_from(bit).expect("a bit"));
                    (copies != [0, 0]).then(|| LocusAlleles {
                        locus: locus.clone(),
                        copies: copies.to_vec(),
                    })
                })
                .collect();
            Founder {
                born: SimTime::from_secs(-16 * Y - offset),
                sex,
                alleles,
            }
        })
        .collect()
}

fn pearson(pairs: &[(f64, f64)]) -> f64 {
    let n = f64::from(u32::try_from(pairs.len()).expect("small"));
    let (mean_x, mean_y) = pairs
        .iter()
        .fold((0.0, 0.0), |(x, y), (a, b)| (x + a / n, y + b / n));
    let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
    for (a, b) in pairs {
        let (dx, dy) = (a - mean_x, b - mean_y);
        xy += dx * dy;
        xx += dx * dx;
        yy += dy * dy;
    }
    xy / sim_math::sqrt(xx * yy)
}

fn fisher(r: f64) -> f64 {
    0.5 * sim_math::ln((1.0 + r) / (1.0 - r))
}

/// The first `PAIRS` first partnerships of the town (neither had one before) as the
/// founder places of woman and man, and the adult phenotypes `[z_K, z_h]` of every
/// founder (4.2: the adult weights, here at 60 years).
fn town() -> (Vec<(usize, usize)>, Vec<[f64; 2]>) {
    let data = town_data();
    let founders = founders();
    let tag = DataId::new("test:pairing_town").expect("valid");
    let mut world = World::new(data.clone());
    world
        .submit(Input::Start(Start {
            world_seed: WORLD_SEED,
            founder_tag: tag.clone(),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders: founders.clone(),
            couples: Vec::new(),
            capacity: None,
            pool: None,
        }))
        .expect("accepted");
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(30 * Y - 1), &mut |_| false);
    let mut seen = std::collections::BTreeSet::new();
    let mut first = Vec::new();
    for partnership in world.pedigree().partnerships {
        let (woman, man) = (partnership.woman.get(), partnership.man.get());
        let fresh = !seen.contains(&woman) && !seen.contains(&man);
        seen.insert(woman);
        seen.insert(man);
        if fresh && first.len() < PAIRS {
            let place = |sim: u64| usize::try_from(sim - 1).expect("a founder");
            first.push((place(woman), place(man)));
        }
    }
    let draws = Draws::new(WorldSeed::new(WORLD_SEED));
    let traits = data.pairing_traits().map(|t| t.expect("named").clone());
    let adult = SimDuration::from_secs(60 * Y);
    let phenotypes = (0_u64..)
        .zip(&founders)
        .map(|(index, founder)| {
            let genome = Genome::with_alleles(founder.sex, &founder.alleles, &data).expect("valid");
            let key = LineageKey::founder(&tag, index);
            traits.clone().map(|t| {
                phenotype(
                    sim_core::phenotype::Environment::one(&draws),
                    &data,
                    &genome,
                    key,
                    None,
                    &t,
                    adult,
                )
                .expect("a phenotype")
            })
        })
        .collect();
    (first, phenotypes)
}

/// Whether the correlations of the pairs meet P7: cognition 0.4 and height 0.2 within
/// `4 / √(n − 3)` on Fisher's z scale.
fn p7(pairs: &[(usize, usize)], phenotypes: &[[f64; 2]]) -> [(f64, bool); 2] {
    let n = f64::from(u32::try_from(pairs.len()).expect("small"));
    let tolerance = 4.0 / sim_math::sqrt(n - 3.0);
    [(0, 0.4), (1, 0.2)].map(|(t, target)| {
        let values: Vec<(f64, f64)> = pairs
            .iter()
            .map(|&(woman, man)| (phenotypes[woman][t], phenotypes[man][t]))
            .collect();
        let r = pearson(&values);
        (r, (fisher(r) - fisher(target)).abs() <= tolerance)
    })
}

/// P7 and its negative twin (spec 8.1: for relations the twin pairs at random). The
/// tolerance for 1000 pairs is 0.1267 (W8.7a, W8.7c).
#[test]
fn p7_couples_correlate_in_cognition_and_height() {
    let (pairs, phenotypes) = town();
    assert_eq!(pairs.len(), PAIRS, "first partnerships in thirty years");
    let tolerance = 4.0 / sim_math::sqrt(f64::from(u32::try_from(PAIRS).expect("small")) - 3.0);
    assert!((tolerance - 0.1267).abs() < 0.000_05);
    let [(cognition, cognition_ok), (height, height_ok)] = p7(&pairs, &phenotypes);
    assert!(cognition_ok, "cognition: correlation {cognition}, want 0.4");
    assert!(height_ok, "height: correlation {height}, want 0.2");
    // The twin: the same women with the men of a fixed random permutation.
    let mut men: Vec<usize> = pairs.iter().map(|&(_, man)| man).collect();
    let mut state = SEED_SHUFFLE;
    for i in (1..men.len()).rev() {
        let bound = u64::try_from(i + 1).expect("small");
        let j = usize::try_from(splitmix(&mut state) % bound).expect("small");
        men.swap(i, j);
    }
    let shuffled: Vec<(usize, usize)> = pairs
        .iter()
        .zip(men)
        .map(|(&(woman, _), man)| (woman, man))
        .collect();
    let [(r_k, ok_k), (r_h, ok_h)] = p7(&shuffled, &phenotypes);
    assert!(!ok_k, "the twin passes cognition: {r_k}");
    assert!(!ok_h, "the twin passes height: {r_h}");
}
