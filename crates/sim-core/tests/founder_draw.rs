//! The founders' draw at `Start` (Grok F1, P2 and P3; slice S2b of the map-4 switch): a
//! founder draws every locus it does not name, one draw per copy, allele 1 iff `u < q`, with
//! `q = d_ref / 2` at an additive locus and the pool's frequency at the seven pool loci;
//! every other locus, and the pool loci on data without a pool, keeps the reference allele.
//! `FOUNDER_ALLELE` (tag 28) with the world seed, subject the founder's lineage key,
//! occasion `str_key` of the locus, detail the copy as a gamete (0 and 1 on an autosome and
//! a woman's X, 0 on a man's X and on the mt).
//!
//! The known answers come from `tools/world_reference.py --founder-alleles`, written from
//! the design note and not from this code. Its scenario: the template of map 4
//! (`tests/golden/map4_template` with the repository's life course data) with `pool_b` and
//! the frequencies of `tests/golden/pool/pool_frequencies.json`, world seed 0x28, founder
//! tag `test:founder_allele`, the switch on, no couples; founder 0 a woman of 30, founder 1
//! a man of 32, both naming nothing, founder 2 a woman of 30 naming `core:herc2` `[0, 0]`.
//! The same founders on the template without a pool, on `tests/golden/data_traits` and on
//! map 3. Two more cases of the reference (its notes on the extra cases): the boundary
//! `u < q` on `data_traits` with `d_ref` of `test:g1` at `2u` and one ulp above, and a Y
//! copy, `core:herc2` moved to Y with `pool_b` frozen on that map. Their data are built here
//! from the fixtures by the reference's edits and pinned by the reference's data hashes.

mod common;

use sim_core::data::Data;
use sim_core::data_id::{DataId, str_key};
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::hash::stable_hash64;
use sim_core::ids::InputSeq;
use sim_core::input::{Founder, Input, InputRecord, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::log_hash;
use sim_core::params::WorldParams;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::SimTime;
use sim_core::world::World;

const Y: i64 = 31_449_600;
const SEED: u64 = 0x28;
const TAG: &str = "test:founder_allele";
const POOL_B: &[u8] = include_bytes!("../../../tests/golden/pool/pool_b.bin");

const TEMPLATE_DATA_HASH: u64 = 0x5ae3_d70c_71cf_063a;
const POOL_B_DATA_HASH: u64 = 0x6bc8_a5d6_8de3_6a5e;
/// The lineage keys of founders 0, 1 and 2.
const KEYS: [u64; 3] = [
    0x922e_affa_24f5_8fdb,
    0x51c7_1a22_46b2_2623,
    0xa904_e092_9077_7fa8,
];
/// The seven loci of the scenario's known answers: `str_key` of the locus and the bits of
/// `q` there (`d_ref / 2` at the two additive loci, the file's frequency at the pool loci).
const LOCI: [(&str, u64, u64); 7] = [
    (
        "core:growth_01",
        0x2a73_891c_20c4_1279,
        0x3fe0_0000_0000_0000,
    ),
    (
        "core:cognition_01",
        0xaeb6_98d3_5543_b3f5,
        0x3fe0_0000_0000_0000,
    ),
    (
        "core:disease_ar",
        0x727e_1194_b5a9_969a,
        0x3f94_7ae1_47ae_147b,
    ),
    ("core:herc2", 0x1dfc_28e7_a4b1_6f26, 0x3fe8_0000_0000_0000),
    ("core:mc1r", 0x1aef_ffd3_1b7e_de41, 0x3fc3_3333_3333_3333),
    (
        "core:disease_xr",
        0xc9f4_ef4e_8af7_035b,
        0x3f30_624d_d2f1_a9fc,
    ),
    (
        "core:disease_mt",
        0xea12_3f53_bab7_bcf5,
        0x3f1f_7510_4d55_1d69,
    ),
];
/// Each draw of founders 0 and 1 at those loci: (founder, locus, gamete, bits of `u`,
/// allele).
const DRAWS: [(usize, &str, u64, u64, u8); 25] = [
    (0, "core:growth_01", 0, 0x3fb6_7d01_ec75_93e8, 1),
    (0, "core:growth_01", 1, 0x3fe2_500f_6e66_2d0c, 0),
    (0, "core:cognition_01", 0, 0x3fe8_da59_f56f_ff38, 0),
    (0, "core:cognition_01", 1, 0x3fc5_a0db_0780_ba74, 1),
    (0, "core:disease_ar", 0, 0x3fb1_2a65_d41a_3c50, 0),
    (0, "core:disease_ar", 1, 0x3feb_cbd5_8a49_297a, 0),
    (0, "core:herc2", 0, 0x3fd2_8e0b_bb64_1430, 1),
    (0, "core:herc2", 1, 0x3fd8_9373_9016_9d36, 1),
    (0, "core:mc1r", 0, 0x3fe3_ec49_831b_004c, 0),
    (0, "core:mc1r", 1, 0x3fe4_57f9_9f93_1b03, 0),
    (0, "core:disease_xr", 0, 0x3fe2_0445_406a_e3c5, 0),
    (0, "core:disease_xr", 1, 0x3fe2_95d4_61ac_07c9, 0),
    (0, "core:disease_mt", 0, 0x3fde_beab_1279_3468, 0),
    (1, "core:growth_01", 0, 0x3fef_8bab_357e_5e99, 0),
    (1, "core:growth_01", 1, 0x3fc5_c5a9_2fe4_551c, 1),
    (1, "core:cognition_01", 0, 0x3fe6_2007_3299_6b1d, 0),
    (1, "core:cognition_01", 1, 0x3fe6_bfaa_318f_020c, 0),
    (1, "core:disease_ar", 0, 0x3fdf_7270_28dc_40ee, 0),
    (1, "core:disease_ar", 1, 0x3fe7_77b5_335e_bdce, 0),
    (1, "core:herc2", 0, 0x3fb7_929c_388f_7f10, 1),
    (1, "core:herc2", 1, 0x3fdb_78fa_727a_5140, 1),
    (1, "core:mc1r", 0, 0x3fe1_fc04_3cd9_6f44, 0),
    (1, "core:mc1r", 1, 0x3fdc_1085_6b3f_b970, 0),
    (1, "core:disease_xr", 0, 0x3fe4_b266_b1fb_25b6, 0),
    (1, "core:disease_mt", 0, 0x3fec_7d54_a59d_ecb8, 0),
];
/// Each founder's genome after `Start` (postcard) and its number of alleles 1.
const GENOMES: [(&str, usize); 3] = [
    (
        "928d80eabc8188b718c0b2d880aebf92c091018680939280e48787a001849d80acf5849834e083c380eec3a7e07386c0d08180f8a6854000000000",
        124,
    ),
    (
        "8a9680bcfa85b8e90fa098c380e8e6abf0e4018080c88880fcf68560da98808be084a8ca37a086d28086bd91f0b50184c0a08f80fc89852000010000",
        130,
    ),
    (
        "f09b80d4ac84b88435a0ac9680e0c583f0398480d395808cba06fe96808e9c81b08924e0d19d808adaaaf07d8080998d8094e18f800100000000",
        124,
    ),
];
/// Founder 2's two draws at `core:herc2` had she not named it: `u` of gametes 0 and 1.
const HERC2_UNNAMED: [f64; 2] = [0.397_323_819_894_934_6, 0.064_500_562_588_431_51];
const START_INPUT: (usize, u64) = (65, 0x5af2_b992_de9a_4803);
const START_INPUT_HEX: &str = "0101002813746573743a666f756e6465725f616c6c656c65010003ffefe383070000ffffe1bf070100ffefe3830700010a636f72653a6865726332020000000000";
const STATE_HASH_AFTER_START: u64 = 0x821a_1320_c251_ebe8;
const LOG_HASH_AFTER_START: u64 = 0x5faf_e3d3_0194_5d35;
const SAVE_AFTER_START: (usize, u64) = (552, 0x8d0e_14cf_c031_64df);
/// The genomes of the same founders on the template without a pool: the additive loci
/// drawn as above, the seven pool loci at the reference allele.
const GENOMES_WITHOUT_POOL: [&str; 3] = [
    "928d80eabc8188b718c0b29880aebf92c091018680939280e48787a001849d80acf5849834e0838380eec3a7e07386c0d08180f8a6854000000000",
    "8a9680bcfa85b8e90fa0988380e8e6abf0e4018080c88880fcf68560da98808be084a8ca37a086928086bd91f0b50184c0a08f80fc89852000010000",
    "f09b80d4ac84b88435a0ac9680e0c583f0398480d395808cba06fe96808e9c81b08924e0d19d808adaaaf07d8080998d8094e18f800100000000",
];
/// The draws of founders 0 and 1 on `data_traits`: (founder, locus, gamete, bits of `q`,
/// bits of `u`, allele); `test:g2` (a threshold locus) and `core:disease_ar` are not drawn.
const TRAITS_DRAWS: [(usize, &str, u64, u64, u64, u8); 12] = [
    (
        0,
        "test:g1",
        0,
        0x3fe4_0000_0000_0000,
        0x3fe5_2625_2abe_a831,
        0,
    ),
    (
        0,
        "test:g1",
        1,
        0x3fe4_0000_0000_0000,
        0x3fe2_8e19_5550_f0b4,
        1,
    ),
    (
        0,
        "test:f1",
        0,
        0x3fe0_0000_0000_0000,
        0x3fe5_9507_f620_38d6,
        0,
    ),
    (
        0,
        "test:f1",
        1,
        0x3fe0_0000_0000_0000,
        0x3fd5_bb99_20b4_62c8,
        1,
    ),
    (
        0,
        "test:p1",
        0,
        0x0000_0000_0000_0000,
        0x3fc5_3a11_9dd1_d48c,
        0,
    ),
    (
        0,
        "test:p1",
        1,
        0x0000_0000_0000_0000,
        0x3fd3_51a0_f548_2588,
        0,
    ),
    (
        1,
        "test:g1",
        0,
        0x3fe4_0000_0000_0000,
        0x3feb_9fd0_5279_9628,
        0,
    ),
    (
        1,
        "test:g1",
        1,
        0x3fe4_0000_0000_0000,
        0x3fe4_93ea_5b32_4fd4,
        0,
    ),
    (
        1,
        "test:f1",
        0,
        0x3fe0_0000_0000_0000,
        0x3fb8_e340_ee77_7058,
        1,
    ),
    (
        1,
        "test:f1",
        1,
        0x3fe0_0000_0000_0000,
        0x3fd2_e800_9e2d_10d4,
        1,
    ),
    (
        1,
        "test:p1",
        0,
        0x0000_0000_0000_0000,
        0x3fe5_4b17_c97a_3caf,
        0,
    ),
    (
        1,
        "test:p1",
        1,
        0x0000_0000_0000_0000,
        0x3fea_2fda_868c_42fa,
        0,
    ),
];
const TRAITS_GENOMES: [&str; 2] = [
    "0000008080808880808008000000000000",
    "808080808080800800008080808080808008000000010000",
];
/// The share of allele 1 at `core:growth_01`, gamete 0, over founders 0 to 9999 (`q` 0.5).
const SHARE_FOUNDERS: u64 = 10_000;
const SHARE_ONES: usize = 4948;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn id(text: &str) -> DataId {
    DataId::new(text).expect("id")
}

fn template_definitions() -> sim_core::data::Definitions {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    );
    common::definitions(calibration, genome_map).expect("definitions")
}

/// The scenario's data: the template with `pool_b` and the file's frequencies.
fn pool_data() -> Data {
    let data = Data::with_pool(
        template_definitions(),
        POOL_B,
        common::pool_frequencies("pool_b"),
    )
    .expect("checked");
    assert_eq!(data.hash(), POOL_B_DATA_HASH);
    data
}

/// Founders 0 and 1 of the scenario, and founder 2 naming `core:herc2` `[0, 0]` if `named`.
fn founders(named: bool) -> Vec<Founder> {
    let herc2 = LocusAlleles {
        locus: id("core:herc2"),
        copies: vec![0, 0],
    };
    let founder = |years: i64, sex: Sex, alleles: Vec<LocusAlleles>| Founder {
        born: SimTime::from_secs(-years * Y),
        sex,
        alleles,
    };
    vec![
        founder(30, Sex::Female, Vec::new()),
        founder(32, Sex::Male, Vec::new()),
        founder(
            30,
            Sex::Female,
            if named { vec![herc2] } else { Vec::new() },
        ),
    ]
}

fn start(founders: Vec<Founder>) -> Start {
    Start {
        world_seed: SEED,
        founder_tag: id(TAG),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders,
        couples: Vec::new(),
        capacity: None,
        pool: None,
    }
}

/// A world on `data` after the `Start` of these founders.
fn started(data: Data, founders: Vec<Founder>) -> World {
    let mut world = World::new(data);
    world
        .submit(Input::Start(start(founders)))
        .expect("accepted");
    world
}

/// Each founder's genome after `Start`, from the pedigree.
fn genomes(world: &World) -> Vec<Genome> {
    let data = world.data();
    world
        .pedigree()
        .persons
        .iter()
        .map(|person| Genome::with_alleles(person.sex, &person.alleles, data).expect("fits"))
        .collect()
}

/// The copies of `genome` at `locus`.
fn copies(data: &Data, genome: &Genome, locus: &str) -> Vec<u8> {
    genome.copies(data.locus(&id(locus)).expect("a locus"))
}

/// The number of alleles 1 of `genome` over the map.
fn ones(data: &Data, genome: &Genome) -> usize {
    let loci = &data.genome_map().loci;
    loci.iter()
        .map(|locus| genome.dose(locus))
        .map(usize::from)
        .sum()
}

fn unit(founder: usize, locus: &str, gamete: u64) -> f64 {
    let index = u64::try_from(founder).expect("small");
    Draws::new(WorldSeed::new(SEED)).unit(DrawKey {
        tag: tags::FOUNDER_ALLELE,
        subject: SubjectKey::from(LineageKey::founder(&id(TAG), index)),
        occasion: str_key(&id(locus)),
        detail: gamete,
    })
}

/// The tag and the addresses of the draw: tag 28, the founders' keys, `str_key` of each
/// locus, and each draw's `u` bit for bit, with `allele = u < q`.
#[test]
fn the_draws_are_the_references() {
    assert_eq!(tags::FOUNDER_ALLELE.get(), 28);
    assert!(
        include_str!("../../../tests/golden/rng_tags.txt")
            .lines()
            .any(|line| line == "tag 28 FOUNDER_ALLELE")
    );
    for (index, key) in (0_u64..).zip(KEYS) {
        assert_eq!(LineageKey::founder(&id(TAG), index).get(), key);
    }
    for (locus, key, _) in LOCI {
        assert_eq!(str_key(&id(locus)), key, "{locus}");
    }
    for (founder, locus, gamete, u, allele) in DRAWS {
        let (_, _, q) = LOCI.iter().find(|row| row.0 == locus).expect("a locus");
        let drawn = unit(founder, locus, gamete);
        assert_eq!(drawn.to_bits(), u, "{founder} {locus} {gamete}");
        assert_eq!(
            u8::from(drawn < f64::from_bits(*q)),
            allele,
            "{founder} {locus}"
        );
    }
    for (gamete, u) in (0_u64..).zip(HERC2_UNNAMED) {
        assert_eq!(unit(2, "core:herc2", gamete).to_bits(), u.to_bits());
    }
}

/// On the scenario's data each founder draws every locus it does not name: the alleles of
/// the known answers at the seven loci (a man one X copy, everyone one mt copy), the
/// genomes, the `Start` input, the state, the log and the save after it.
#[test]
fn a_founder_draws_every_locus_it_does_not_name() {
    let start_record = postcard::to_allocvec(&InputRecord {
        t: SimTime::from_secs(-1),
        seq: InputSeq::try_from(1).expect("valid"),
        input: Input::Start(start(founders(true))),
    })
    .expect("encodes");
    assert_eq!(
        (start_record.len(), stable_hash64(&start_record)),
        START_INPUT
    );
    assert_eq!(start_record, hex(START_INPUT_HEX));
    let mut world = started(pool_data(), founders(true));
    let data = pool_data();
    let genomes = genomes(&world);
    assert_eq!(genomes.len(), 3);
    for (founder, locus, gamete, _, allele) in DRAWS {
        let at = copies(&data, &genomes[founder], locus);
        let copy = usize::try_from(gamete).expect("small");
        assert_eq!(at[copy], allele, "{founder} {locus} {gamete}");
    }
    assert_eq!(copies(&data, &genomes[1], "core:disease_xr").len(), 1);
    for genome in &genomes {
        assert_eq!(copies(&data, genome, "core:disease_mt").len(), 1);
    }
    for (genome, (bytes, count)) in genomes.iter().zip(GENOMES) {
        assert_eq!(postcard::to_allocvec(genome).expect("encodes"), hex(bytes));
        assert_eq!(ones(&data, genome), count);
    }
    assert_eq!(world.state_hash(), STATE_HASH_AFTER_START);
    assert_eq!(log_hash(&world.take_log()), LOG_HASH_AFTER_START);
    let save = world.save(&[]).expect("saves");
    assert_eq!((save.len(), stable_hash64(&save)), SAVE_AFTER_START);
    // The save of founders that drew loads again, to the same state: the loader rebuilds a
    // founder as `Start` builds it, drawn loci included.
    let (loaded, _) = World::load(&save, pool_data()).expect("loads");
    assert_eq!(loaded.state_hash(), STATE_HASH_AFTER_START);
}

/// Founder 2 names `core:herc2` `[0, 0]` and keeps it, though her draws there give `[1, 1]`:
/// unnamed she has `[1, 1]`, and every other locus is the same.
#[test]
fn a_named_locus_is_not_drawn() {
    let data = pool_data();
    let named = genomes(&started(pool_data(), founders(true)));
    let unnamed = genomes(&started(pool_data(), founders(false)));
    assert_eq!(copies(&data, &named[2], "core:herc2"), [0, 0]);
    assert_eq!(copies(&data, &unnamed[2], "core:herc2"), [1, 1]);
    for locus in &data.genome_map().loci {
        if locus.id.as_str() != "core:herc2" {
            assert_eq!(
                named[2].copies(locus),
                unnamed[2].copies(locus),
                "{}",
                locus.id.as_str()
            );
        }
    }
    assert_eq!(&named[..2], &unnamed[..2]);
}

/// On the template without a pool there are no frequencies: the seven pool loci keep the
/// reference allele, the additive loci are drawn as with the pool.
#[test]
fn data_without_a_pool_draw_nothing_at_the_pool_loci() {
    let data = Data::new(template_definitions()).expect("checked");
    assert_eq!(data.hash(), TEMPLATE_DATA_HASH);
    let world = started(data.clone(), founders(true));
    let genomes = genomes(&world);
    for (genome, bytes) in genomes.iter().zip(GENOMES_WITHOUT_POOL) {
        assert_eq!(postcard::to_allocvec(genome).expect("encodes"), hex(bytes));
        for locus in sim_core::pool::pool_loci() {
            let at = genome.copies(data.locus(&locus).expect("a locus"));
            assert!(at.iter().all(|copy| *copy == 0), "{}", locus.as_str());
        }
    }
}

/// On `data_traits`: `q = d_ref / 2` at the three additive loci (0.625, 0.5 and 0, which
/// never gives 1); the threshold locus `test:g2` and the recessive `core:disease_ar` are not
/// drawn.
#[test]
fn only_the_additive_loci_are_drawn() {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    let data =
        Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("ok");
    let world = started(data.clone(), founders(true)[..2].to_vec());
    let genomes = genomes(&world);
    for (founder, locus, gamete, q, u, allele) in TRAITS_DRAWS {
        let drawn = unit(founder, locus, gamete);
        assert_eq!(drawn.to_bits(), u, "{founder} {locus} {gamete}");
        let weights = data.locus(&id(locus)).expect("a locus").action.weights();
        let d_ref = weights.expect("additive").d_ref.get();
        assert_eq!((d_ref / 2.0).to_bits(), q, "{locus}");
        let copy = usize::try_from(gamete).expect("small");
        assert_eq!(copies(&data, &genomes[founder], locus)[copy], allele);
    }
    for (genome, bytes) in genomes.iter().zip(TRAITS_GENOMES) {
        assert_eq!(postcard::to_allocvec(genome).expect("encodes"), hex(bytes));
        assert_eq!(copies(&data, genome, "test:g2"), [0, 0]);
        assert_eq!(copies(&data, genome, "core:disease_ar"), [0, 0]);
    }
}

/// Map 3 has no additive locus and no frequencies: nothing is drawn, every founder has the
/// reference genome.
#[test]
fn map_3_draws_nothing() {
    let data = common::data();
    assert_eq!(data.hash(), 0xadab_5027_57b4_045c);
    let world = started(data, founders(true)[..2].to_vec());
    for person in world.pedigree().persons {
        assert!(person.alleles.is_empty(), "{:?}", person.sim);
    }
}

/// The share of allele 1 over 10 000 founders at `core:growth_01`, gamete 0, `q` 0.5: the
/// reference's count, within 4 standard errors of `q` (genetics specification 8.1).
#[test]
fn the_share_of_alleles_is_q() {
    let index = |n: u64| usize::try_from(n).expect("small");
    let ones = (0..SHARE_FOUNDERS)
        .filter(|&n| unit(index(n), "core:growth_01", 0) < 0.5)
        .count();
    assert_eq!(ones, SHARE_ONES);
    let share = f64::from(u32::try_from(ones).expect("small")) / 10_000.0;
    assert!((share - 0.5).abs() <= 4.0 * 0.005);
}

/// The boundary of `u < q` (`extra/NOTES.md` section 1): `data_traits` with `d_ref` of
/// `test:g1` set to `2u` of founder 0's gamete 0, so `q = u` exactly, and to one ulp above:
/// (`d_ref` bits, data hash, copies at `test:g1`, genome).
const BOUNDARY: [(u64, u64, [u8; 2], &str); 2] = [
    (
        0x3ff5_2625_2abe_a831,
        0xcfe6_3f1f_c396_321e,
        [0, 1],
        "0000008080808880808008000000000000",
    ),
    (
        0x3ff5_2625_2abe_a832,
        0x425e_f7dd_e6e6_4b5f,
        [1, 1],
        "8080800800008080808880808008000000000000",
    ),
];
/// `u` of founder 0 at `test:g1`, gamete 0 (`str_key` `b0bb66411a85e80a`).
const BOUNDARY_U: u64 = 0x3fe5_2625_2abe_a831;

/// A Y locus (`extra/NOTES.md` section 2): the template with `core:herc2` moved to Y,
/// segment 0, slot 0, before the mt; the hash of its definitions, `pool_b` with its
/// `frozen_hash` set to it (length, xxh3), the data hash with that pool and the file's
/// frequencies.
const Y_DEFINITIONS_HASH: u64 = 0xf52e_7556_29ce_4bcf;
const Y_POOL: (usize, u64) = (4257, 0x6adb_7d67_0559_4ad3);
const Y_DATA_HASH: u64 = 0x894c_c097_7a02_a07c;
/// A man's Y draws: the bits of `u` at detail 1 (the rule) and at detail 0, and his Y copy.
type YDraws = (u64, u64, u8);
/// The founders of the Y case (all naming nothing): their sex, their key, and a man's Y
/// draws.
const Y_FOUNDERS: [(Sex, u64, Option<YDraws>); 5] = [
    (Sex::Female, 0x922e_affa_24f5_8fdb, None),
    (
        Sex::Male,
        0x51c7_1a22_46b2_2623,
        Some((0x3fdb_78fa_727a_5140, 0x3fb7_929c_388f_7f10, 1)),
    ),
    (Sex::Female, 0xa904_e092_9077_7fa8, None),
    (Sex::Female, 0x17df_1305_a9ee_47c8, None),
    (
        Sex::Male,
        0x740c_242e_ca89_e03a,
        Some((0x3fca_5099_b11f_a028, 0x3fef_ab99_5afc_23b3, 1)),
    ),
];

/// A draw gives 1 only if `u` lies strictly below `q`: at `q = u` founder 0 keeps 0 at
/// `test:g1`, gamete 0; one ulp higher she has 1. The variant data are the reference's
/// (their data hashes).
#[test]
fn a_draw_gives_one_only_below_q() {
    assert_eq!(unit(0, "test:g1", 0).to_bits(), BOUNDARY_U);
    assert_eq!(str_key(&id("test:g1")), 0xb0bb_6641_1a85_e80a);
    for (d_ref, data_hash, at_g1, genome) in BOUNDARY {
        let (calibration, mut genome_map) = common::files_from(
            include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
            include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
        );
        let loci = genome_map["loci"].as_array_mut().expect("loci");
        let g1 = loci
            .iter_mut()
            .find(|locus| locus["id"] == "test:g1")
            .expect("test:g1");
        g1["action"]["additive"]["d_ref"] = serde_json::json!(f64::from_bits(d_ref));
        let definitions = common::definitions(calibration, genome_map).expect("definitions");
        let data = Data::new(definitions).expect("checked");
        assert_eq!(data.hash(), data_hash);
        let world = started(data.clone(), founders(true)[..1].to_vec());
        let genomes = genomes(&world);
        assert_eq!(copies(&data, &genomes[0], "test:g1"), at_g1);
        assert_eq!(copies(&data, &genomes[0], "test:f1"), [0, 1]);
        for locus in ["core:disease_ar", "test:g2", "test:p1"] {
            assert_eq!(copies(&data, &genomes[0], locus), [0, 0], "{locus}");
        }
        assert_eq!(
            postcard::to_allocvec(&genomes[0]).expect("encodes"),
            hex(genome)
        );
    }
}

/// A pool locus on Y (an additive locus there is no valid data): a woman has no copy and
/// draws nothing; a man draws his Y copy at detail 1. For man 4 detail 0 would give 0.
#[test]
fn a_man_draws_his_y_copy_at_detail_1() {
    let (calibration, mut genome_map) = common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    );
    let loci = genome_map["loci"].as_array_mut().expect("loci");
    let at = loci
        .iter()
        .position(|locus| locus["id"] == "core:herc2")
        .expect("core:herc2");
    let mut herc2 = loci.remove(at);
    herc2["chromosome"] = serde_json::json!("y");
    herc2["segment"] = serde_json::json!(0);
    herc2["slot"] = serde_json::json!(0);
    let mt = loci
        .iter()
        .position(|locus| locus["chromosome"] == "mt")
        .expect("an mt locus");
    loci.insert(mt, herc2);
    let definitions = || common::definitions(calibration.clone(), genome_map.clone());
    let plain = Data::new(definitions().expect("definitions")).expect("checked");
    assert_eq!(plain.hash(), Y_DEFINITIONS_HASH);
    let mut pool = sim_core::pool::Pool::from_bytes(POOL_B).expect("pool_b");
    pool.frozen_hash = Y_DEFINITIONS_HASH;
    let pool = pool.to_bytes();
    assert_eq!((pool.len(), stable_hash64(&pool)), Y_POOL);
    let data = Data::with_pool(
        definitions().expect("definitions"),
        &pool,
        common::pool_frequencies("pool_b"),
    )
    .expect("checked");
    assert_eq!(data.hash(), Y_DATA_HASH);
    let q = data.pool().expect("a pool").frequencies()[&id("core:herc2")].get();
    let founders = Y_FOUNDERS
        .iter()
        .map(|&(sex, _, _)| Founder {
            born: SimTime::from_secs(if sex == Sex::Female { -30 * Y } else { -32 * Y }),
            sex,
            alleles: Vec::new(),
        })
        .collect();
    let world = started(data.clone(), founders);
    let genomes = genomes(&world);
    for (index, (genome, (sex, key, man))) in genomes.iter().zip(Y_FOUNDERS).enumerate() {
        let n = u64::try_from(index).expect("small");
        assert_eq!(LineageKey::founder(&id(TAG), n).get(), key);
        assert_eq!(genome.sex(), sex);
        let at_y = copies(&data, genome, "core:herc2");
        match man {
            None => assert!(at_y.is_empty(), "founder {index}"),
            Some((u1, u0, allele)) => {
                assert_eq!(
                    unit(index, "core:herc2", 1).to_bits(),
                    u1,
                    "founder {index}"
                );
                assert_eq!(
                    unit(index, "core:herc2", 0).to_bits(),
                    u0,
                    "founder {index}"
                );
                assert_eq!(at_y, [allele], "founder {index}");
                assert_eq!(u8::from(f64::from_bits(u1) < q), allele);
            }
        }
    }
    let (_, _, man_4) = Y_FOUNDERS[4];
    let (_, u0, _) = man_4.expect("a man");
    assert_eq!(
        u8::from(f64::from_bits(u0) < q),
        0,
        "detail 0 gives the other allele"
    );
}
