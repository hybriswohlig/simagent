//! The criteria of phase 1 that the meiosis alone decides (genetics specification 8.3):
//! P12 recombination, P13 new mutations, P15 X-linked, and the exact properties of the
//! meiosis (the tables of mt and Y, every new mutation in the meiosis entry). Their unit is the meiosis, whose
//! draws are independent per child (8.1, ADR-0001), so the tests draw meioses directly
//! from fixed parents. Expected values and tolerances come from the specification; the
//! tolerance is 4 standard errors at the actual sample size. Each test also runs on a
//! negative twin (8.1) on which it must be red. The seeds are gate seeds (protected
//! list, ADR-0000): never reseed to get green.

mod common;

use serde_json::{Value, json};
use sim_core::data::Chromosome;
use sim_core::data::Data;
use sim_core::genome::meiosis::{Meiosis, X_SEGMENT, Zygote};
use sim_core::genome::{Genome, PaternalSex, Strand};
use sim_core::lineage::GenomeKey;
use sim_core::rng::{Draws, WorldSeed};

const SEED_P12: u64 = 0x5eed_0000_0000_0012;
const SEED_P13: u64 = 0x5eed_0000_0000_0013;
const SEED_P15: u64 = 0x5eed_0000_0000_0015;
/// For the exact properties, which hold under every seed.
const SEED_EXACT: u64 = 0x5eed_0000_0000_0e8a;

fn key(n: u64) -> GenomeKey {
    serde_json::from_value(json!(format!("{n:016x}"))).expect("key")
}

/// A genome from the words of its parts (the serde form of `Genome`).
fn genome(maternal: [u64; 3], paternal: [u64; 3], maternal_x: u16, from_father: Value) -> Genome {
    serde_json::from_value(json!({
        "maternal": maternal, "paternal": paternal, "maternal_x": maternal_x,
        "paternal_sex": from_father, "mt": 0
    }))
    .expect("a genome")
}

/// The zygotes of `mother` and `father` with the genome keys in `keys`.
fn zygotes(
    seed: u64,
    data: &Data,
    mother: &Genome,
    father: &Genome,
    keys: core::ops::Range<u64>,
) -> Vec<Zygote> {
    let draws = Draws::new(WorldSeed::new(seed));
    keys.map(|k| Zygote::of(&draws, data, key(k), mother, father))
        .collect()
}

/// The share of `items` for which `test` holds.
fn share<T>(items: &[T], test: impl Fn(&T) -> bool) -> f64 {
    let hits = u32::try_from(items.iter().filter(|item| test(item)).count()).expect("small");
    f64::from(hits) / f64::from(u32::try_from(items.len()).expect("small"))
}

/// The repository's definitions with a female autosomal map of `morgans` per segment.
fn female_map(morgans: f64) -> Data {
    let (mut calibration, genome_map) = common::files();
    calibration["values"]["core:map_length_autosome_female"]["value"] = json!(morgans);
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// P12 (8.3): two loci 0.5 M apart recombine in maternal meioses at r(0.5) = 0.3161
/// [W2.3b]. Adjacent slots lie `L / 24` apart (2.3), so on a test map of 12 M per
/// segment they are 0.5 M apart. 5000 meioses, tolerance 0.0263 [W8.12a]. Twin: the two
/// loci on different segments, which recombine freely (1/2, the wrong model of
/// [W8.12b]).
#[test]
fn p12_loci_half_a_morgan_apart_recombine_at_the_haldane_rate() {
    let data = female_map(12.0);
    // The mother has every reference allele from her mother and every alternative one
    // from her father: a gamete is recombinant between two slots exactly when it carries
    // different alleles there. Segments 3 and 4 hold no locus of the map, so no
    // mutation touches them.
    let mother = genome([0; 3], [u64::MAX; 3], 0, json!({"x": 0}));
    let father = genome([0; 3], [0; 3], 0, json!({"y": 0}));
    let zygotes = zygotes(SEED_P12, &data, &mother, &father, 0..5000);
    let recombinant = |a: (u8, u8), b: (u8, u8)| {
        share(&zygotes, |zygote| {
            let gamete = zygote.genome.maternal;
            gamete.allele(a.0, a.1) != gamete.allele(b.0, b.1)
        })
    };
    let ours = recombinant((3, 10), (3, 11));
    assert!((ours - 0.3161).abs() <= 0.0263, "{ours}");
    let twin = recombinant((3, 10), (4, 10));
    assert!((twin - 0.3161).abs() > 0.0263, "{twin}");
}

/// P13 (8.3): at `Λ` = 0.0075 per meiosis, 20 000 meioses count on average 150 [W8.13a]
/// new mutations (Poisson): 150 autosomal loci at the standard rate 5e-5 [K2.4a], both
/// directions, so every locus is eligible on every gamete. Tolerance 49.0 [W8.13b]. Twin:
/// half the rate [W8.13c].
#[test]
fn p13_new_mutations_follow_the_total_rate() {
    let count = |rate: f64| {
        let data = common::autosomal_map(150, "both", rate);
        let mother = genome([0; 3], [0; 3], 0, json!({"x": 0}));
        let father = genome([0; 3], [0; 3], 0, json!({"y": 0}));
        let total: usize = zygotes(SEED_P13, &data, &mother, &father, 0..10_000)
            .iter()
            .map(|zygote| zygote.mother.mutations.len() + zygote.father.mutations.len())
            .sum();
        f64::from(u32::try_from(total).expect("small"))
    };
    let ours = count(5e-5);
    assert!((ours - 150.0).abs() <= 49.0, "{ours}");
    let twin = count(2.5e-5);
    assert!((twin - 150.0).abs() > 49.0, "{twin}");
}

/// Whether a child is a son.
fn is_son(zygote: &Zygote) -> bool {
    matches!(zygote.genome.paternal_sex, PaternalSex::Y(_))
}

/// P15 (8.3, 2.6): the sons of a carrier mother have the X allele with 1/2. Unit: son, at
/// least 1000; tolerance `4 · √(1/4 / n)`, 0.0632 [W8.15a] at 1000. Twin: a mixed data
/// set in which half the sons come from a mother without the allele (1/4, the wrong model
/// of [W8.15b]). Exactly: the sons of a father with the allele never inherit it, his
/// daughters always.
#[test]
fn p15_sons_of_carrier_mothers_have_it_with_one_half() {
    let data = common::data();
    let carrier = genome([0; 3], [0; 3], 1, json!({"x": 0}));
    let free_mother = genome([0; 3], [0; 3], 0, json!({"x": 0}));
    let free_father = genome([0; 3], [0; 3], 0, json!({"y": 0}));
    let affected = |sons: &[&Zygote]| share(sons, |son| son.genome.maternal_x.allele(0) == 1);
    let tolerance = |sons: &[&Zygote]| {
        let n = f64::from(u32::try_from(sons.len()).expect("small"));
        4.0 * sim_math::sqrt(0.25 / n)
    };
    let ours = zygotes(SEED_P15, &data, &carrier, &free_father, 0..2200);
    let sons: Vec<&Zygote> = ours.iter().filter(|zygote| is_son(zygote)).collect();
    assert!(sons.len() >= 1000, "{} sons", sons.len());
    let share_ours = affected(&sons);
    assert!((share_ours - 0.5).abs() <= tolerance(&sons), "{share_ours}");
    let others = zygotes(SEED_P15, &data, &free_mother, &free_father, 2200..4400);
    let mixed: Vec<&Zygote> = sons
        .iter()
        .copied()
        .chain(others.iter().filter(|zygote| is_son(zygote)))
        .collect();
    let share_twin = affected(&mixed);
    assert!((share_twin - 0.5).abs() > tolerance(&mixed), "{share_twin}");
    let affected_father = genome([0; 3], [0; 3], 1, json!({"y": 0}));
    for zygote in zygotes(SEED_P15, &data, &free_mother, &affected_father, 4400..4800) {
        match zygote.genome.paternal_sex {
            PaternalSex::X(x) => assert_eq!(x.allele(0), 1, "a daughter"),
            PaternalSex::Y(_) => assert_eq!(zygote.genome.maternal_x.allele(0), 0, "a son"),
        }
    }
}

/// Exact (8.3, 2.6): mt comes from the mother to every child and never from the father;
/// the Y of the father goes to every son, and no daughter has one.
#[test]
fn mt_and_y_follow_their_tables_exactly() {
    let data = common::data();
    let with_mt = |from_father: Value| {
        serde_json::from_value::<Genome>(json!({
            "maternal": [0, 0, 0], "paternal": [0, 0, 0], "maternal_x": 0,
            "paternal_sex": from_father, "mt": 1
        }))
        .expect("a genome")
    };
    let mother = with_mt(json!({"x": 0}));
    let father = genome([0; 3], [0; 3], 0, json!({"y": 1}));
    for zygote in zygotes(SEED_EXACT, &data, &mother, &father, 0..400) {
        assert_eq!(zygote.genome.mt.allele(0), 1, "mt of the mother");
        if let PaternalSex::Y(y) = zygote.genome.paternal_sex {
            assert_eq!(y.allele(0), 1, "the Y of the father");
        }
    }
    let mother = genome([0; 3], [0; 3], 0, json!({"x": 0}));
    let father = with_mt(json!({"y": 0}));
    for zygote in zygotes(SEED_EXACT, &data, &mother, &father, 400..800) {
        assert_eq!(zygote.genome.mt.allele(0), 0, "no mt of the father");
    }
}

/// The allele that the meiosis passes on at `slot` of `segment` before mutation (2.3):
/// the start strand, switched at every crossover below the slot's position.
fn passed_on(meiosis: &Meiosis, segment: u8, slot: u8, slots: u8) -> Strand {
    let record = &meiosis.segments[usize::from(segment)];
    assert_eq!(record.segment, segment);
    let position = (f64::from(slot) + 0.5) / f64::from(slots);
    let crossed = record
        .crossovers
        .iter()
        .filter(|c| c.get() < position)
        .count();
    match (record.start_strand, crossed % 2 == 1) {
        (Strand::Maternal, false) | (Strand::Paternal, true) => Strand::Maternal,
        (Strand::Paternal, false) | (Strand::Maternal, true) => Strand::Paternal,
    }
}

/// Exact (8.3, 2.4): the child is the recombination that the meiosis entries record plus
/// exactly the new mutations they list, each with its old allele. A map of 150 loci at a
/// high rate gives about 7.5 mutations per meiosis.
#[test]
fn every_new_mutation_is_in_the_meiosis_entry() {
    let data = common::autosomal_map(150, "both", 0.05);
    let mother = genome(
        [0x0f0f_0f0f_0f0f_0f0f; 3],
        [0x3333_3333_3333_3333; 3],
        0x5a5a,
        json!({"x": 0x0ff0}),
    );
    let father = genome(
        [0x5555_5555_5555_5555; 3],
        [0x0ff0_0ff0_0ff0_0ff0; 3],
        0x00ff,
        json!({"y": 0}),
    );
    let mut listed = 0;
    for zygote in zygotes(SEED_EXACT, &data, &mother, &father, 0..300) {
        for (meiosis, parent, child) in [
            (&zygote.mother, &mother, zygote.genome.maternal),
            (&zygote.father, &father, zygote.genome.paternal),
        ] {
            let mut expected: Vec<Vec<u8>> = (0..8)
                .map(|segment| {
                    (0..24)
                        .map(|slot| match passed_on(meiosis, segment, slot, 24) {
                            Strand::Maternal => parent.maternal.allele(segment, slot),
                            Strand::Paternal => parent.paternal.allele(segment, slot),
                        })
                        .collect()
                })
                .collect();
            for mutation in &meiosis.mutations {
                let locus = data.locus(&mutation.locus).expect("a locus of the map");
                assert_eq!(locus.chromosome, Chromosome::Autosome);
                let allele = &mut expected[usize::from(locus.segment)][usize::from(locus.slot)];
                assert_eq!((*allele, mutation.new), (mutation.old, 1 - mutation.old));
                *allele = mutation.new;
                listed += 1;
            }
            for segment in 0..8 {
                for slot in 0..24 {
                    assert_eq!(
                        child.allele(segment, slot),
                        expected[usize::from(segment)][usize::from(slot)],
                        "segment {segment} slot {slot}"
                    );
                }
            }
        }
        let PaternalSex::X(her_x) = mother.paternal_sex else {
            panic!("a mother");
        };
        for slot in 0..16 {
            let expected = match passed_on(&zygote.mother, X_SEGMENT, slot, 16) {
                Strand::Maternal => mother.maternal_x.allele(slot),
                Strand::Paternal => her_x.allele(slot),
            };
            assert_eq!(
                zygote.genome.maternal_x.allele(slot),
                expected,
                "X slot {slot}"
            );
        }
    }
    assert!(listed > 3000, "{listed} mutations listed");
}
