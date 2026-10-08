//! The phenotype of a trait (genetics specification 4.2, ADR-0009 sections 3 and 4). The
//! expected values take the draws at the addresses of section 3 and do the arithmetic of
//! section 4 written out, with `exp` and `sqrt` from `sim-math`; the tests compare bits.

mod common;

use sim_core::data::Data;
use sim_core::data_id::{DataId, str_key};
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::lineage::LineageKey;
use sim_core::phenotype::{Parents, individual_environment, phenotype, shared_environment};
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{SimDuration, YEAR};

fn id(text: &str) -> DataId {
    DataId::new(text).expect("id")
}

fn founder(index: u64) -> LineageKey {
    LineageKey::founder(&id("test:clan"), index)
}

/// The fixture map with traits and phenotype rows (`tests/golden/data_traits`): face with
/// h2 0.7 and c2 0 at every age, mood with h2 0.5 at every age and c2 from 0.15 (child) to
/// 0.05 (adult); growth has a genetic value but no phenotype.
fn fixture() -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

fn woman(data: &Data) -> Genome {
    let alleles = [("test:g1", [1, 1]), ("test:f1", [1, 0]), ("test:p1", [0, 1])]
        .map(|(locus, copies)| LocusAlleles {
            locus: id(locus),
            copies: copies.to_vec(),
        });
    Genome::with_alleles(Sex::Female, &alleles, data).expect("fits")
}

/// `C_t` at the address of ADR-0009 section 3: subject mother, occasion the trait, detail
/// father.
fn c(draws: &Draws, t: &str, mother: LineageKey, father: LineageKey) -> f64 {
    draws.normal(
        DrawKey {
            tag: tags::ENV_SHARED,
            subject: SubjectKey::from(mother),
            occasion: str_key(&id(t)),
            detail: father.get(),
        },
        0.0,
        1.0,
    )
}

/// `E_t`: subject the person, occasion the trait, detail 0.
fn e(draws: &Draws, t: &str, own: LineageKey) -> f64 {
    draws.normal(
        DrawKey {
            tag: tags::ENV_INDIVIDUAL,
            subject: SubjectKey::from(own),
            occasion: str_key(&id(t)),
            detail: 0,
        },
        0.0,
        1.0,
    )
}

/// Section 4 written out: the weights at age `x` (years), each held in 0 to 1, then the
/// sum from the left.
fn by_hand(weights: [f64; 4], x: f64, g: f64, c: f64, e: f64) -> f64 {
    let [h2_child, c2_child, h2_adult, c2_adult] = weights;
    let s = 1.0 / (1.0 + sim_math::exp((12.0 - x) / 2.0));
    let held = |w: f64| {
        if w > 0.0 {
            if w > 1.0 { 1.0 } else { w }
        } else {
            0.0
        }
    };
    let h2 = h2_child + (h2_adult - h2_child) * s;
    let c2 = c2_child + (c2_adult - c2_child) * s;
    let e2 = (1.0 - h2) - c2;
    ((sim_math::sqrt(held(h2)) * g) + (sim_math::sqrt(held(c2)) * c))
        + (sim_math::sqrt(held(e2)) * e)
}

const MOOD: [f64; 4] = [0.5, 0.15, 0.5, 0.05];
const FACE: [f64; 4] = [0.7, 0.0, 0.7, 0.0];

#[test]
fn phenotype_follows_the_specification() {
    let data = fixture();
    let draws = Draws::new(WorldSeed::new(0x5eed_0000_0000_0042));
    let genome = woman(&data);
    let (mother, father, own) = (founder(1), founder(2), founder(3));
    let parents = Some(Parents { mother, father });
    let value = |t: &str, age: SimDuration| {
        phenotype(&draws, &data, &genome, own, parents, &id(t), age).expect("a phenotype")
    };
    let g = |t: &str| genome.genetic_value(&id(t), &data).expect("a trait");
    // At 12 years s is exactly 1/2; as a child and as an adult the weights move.
    for (age, x) in [(YEAR * 12, 12.0), (SimDuration::from_secs(0), 0.0), (YEAR * 30, 30.0)] {
        let mood = by_hand(MOOD, x, g("test:mood"), c(&draws, "test:mood", mother, father),
            e(&draws, "test:mood", own));
        assert_eq!(value("test:mood", age).to_bits(), mood.to_bits(), "mood at {x}");
        let face = by_hand(FACE, x, g("test:face"), c(&draws, "test:face", mother, father),
            e(&draws, "test:face", own));
        assert_eq!(value("test:face", age).to_bits(), face.to_bits(), "face at {x}");
    }
    // A part of a year counts as such: 12 years and a half.
    let half = YEAR * 12 + SimDuration::from_secs(YEAR.as_secs() / 2);
    let mood = by_hand(MOOD, 12.5, g("test:mood"), c(&draws, "test:mood", mother, father),
        e(&draws, "test:mood", own));
    assert_eq!(value("test:mood", half).to_bits(), mood.to_bits());
    // A trait without a row has a genetic value but no phenotype; an unknown one neither.
    let none = |t: &str| phenotype(&draws, &data, &genome, own, parents, &id(t), YEAR);
    assert_eq!(none("test:growth"), None);
    assert_eq!(none("test:other"), None);
    // Without parents in the run the person is its own mother and father.
    let alone = phenotype(&draws, &data, &genome, own, None, &id("test:mood"), YEAR * 40);
    let expected = by_hand(MOOD, 40.0, g("test:mood"), c(&draws, "test:mood", own, own),
        e(&draws, "test:mood", own));
    assert_eq!(alone.map(f64::to_bits), Some(expected.to_bits()));
}

/// Specification 4.2: full siblings share `C_t`, half siblings do not, people without
/// parents in the run share it with nobody; `E_t` is each person's own. The draws depend
/// only on the seed and the keys, so a fork with the same parents draws the same.
#[test]
fn full_siblings_share_the_shared_environment() {
    let draws = Draws::new(WorldSeed::new(0x5eed_0000_0000_0043));
    let t = id("test:mood");
    let (mother, father, other_father) = (founder(1), founder(2), founder(4));
    let both = Some(Parents { mother, father });
    let half = Some(Parents { mother, father: other_father });
    let (first, second) = (founder(10), founder(11));
    assert_eq!(
        shared_environment(&draws, &t, first, both),
        shared_environment(&draws, &t, second, both)
    );
    assert_ne!(
        shared_environment(&draws, &t, first, both),
        shared_environment(&draws, &t, second, half)
    );
    assert_ne!(
        shared_environment(&draws, &t, mother, None),
        shared_environment(&draws, &t, father, None)
    );
    assert_ne!(
        individual_environment(&draws, &t, first),
        individual_environment(&draws, &t, second)
    );
    // Another trait draws another environment.
    assert_ne!(
        shared_environment(&draws, &t, first, both),
        shared_environment(&draws, &id("test:face"), first, both)
    );
    let fork = Draws::new(WorldSeed::new(0x5eed_0000_0000_0043));
    assert_eq!(
        shared_environment(&fork, &t, first, both),
        shared_environment(&draws, &t, first, both)
    );
}
