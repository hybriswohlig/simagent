//! The pre-run (ADR-0020 section 3, design note of the pre-run): the founders from the
//! stationary life table, the measurement run iterated until the spread of five traits
//! settles, the frozen map, and the pool run. A tool outside the gate; its tests run small
//! worlds.

use std::path::Path;

use sim_core::data::{Data, Definitions};
use sim_core::data_id::{DataId, str_key};
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::input::{Founder, Input, Start};
use sim_core::lineage::LineageKey;
use sim_core::measure::Measurement;
use sim_core::mortality;
use sim_core::params::WorldParams;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{SimTime, YEAR};
use sim_core::world::{World, YearOut};

use crate::Result;
use crate::ordered::Json;

/// The seeds of the two runs (ADR-0020 section 3).
pub const MEASURE_SEED: u64 = 0x11b0_19a1;
pub const POOL_SEED: u64 = 0x11b0_19a2;
/// The founders and the capacity of the two runs: both 40 000 (Grok, H1, 2026-10-13: at
/// 4000 the spread of a run moved by 1 to 5 percent between realizations, above the 2
/// percent of the stop).
pub const MEASURE_FOUNDERS: u64 = 40_000;
pub const POOL_FOUNDERS: u64 = 40_000;
/// The years of both runs.
pub const YEARS: u64 = 750;
/// The founders' ages, inclusive.
const AGES: std::ops::RangeInclusive<u32> = 18..=60;
/// The five traits whose spread decides the stop (ADR-0020 section 3).
pub const DECIDING: [&str; 5] = [
    "core:growth",
    "core:cognition",
    "core:longevity",
    "core:puberty",
    "core:disease_liability",
];
const STOP: f64 = 0.02;
const REPORT: f64 = 0.05;
const MAX_RUNS: usize = 5;

/// The ids of the loci the pre-run leaves out: the disease loci, MC1R and HERC2 (they
/// carry only the reference allele there and neither segregate nor mutate).
fn left_out(locus: &Json) -> bool {
    locus.get("mark").and_then(Json::as_str) == Some("disease")
        || matches!(
            locus.get("id").and_then(Json::as_str),
            Some("core:mc1r" | "core:herc2")
        )
}

/// The files of a pre-run from a map of the template's form and its calibration: without
/// the left-out loci and without the Mendelian diseases (design note, section 1).
pub fn prerun_files(calibration: &Json, map: &Json) -> (Json, Json) {
    let mut map = map.clone();
    if let Some(loci) = map.get_mut("loci").and_then(Json::as_array_mut) {
        loci.retain(|locus| !left_out(locus));
    }
    let mut calibration = calibration.clone();
    if let Some(diseases) = calibration.get_mut("diseases").and_then(Json::as_array_mut) {
        diseases.retain(|disease| {
            disease
                .get("carrier")
                .and_then(|carrier| carrier.get("locus"))
                .is_none()
        });
    }
    (calibration, map)
}

/// The text fields of the data files (ADR-0007 section 2, ADR-0008 section 1): the core's
/// types take the rest.
const TEXT: [&str; 4] = ["source", "as_of", "note", "description"];

fn without_text(json: &Json) -> Json {
    match json {
        Json::Object(entries) => Json::Object(
            entries
                .iter()
                .filter(|(key, _)| !TEXT.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), without_text(value)))
                .collect(),
        ),
        Json::Array(items) => Json::Array(items.iter().map(without_text).collect()),
        other => other.clone(),
    }
}

/// The text of a file without its text fields, for serde_json and the core's types.
fn plain(json: &Json) -> Result<String> {
    without_text(json).pretty()
}

/// The checked data of a pre-run: the repository's life course data with `calibration`
/// and `map` (design note, section 1).
pub fn prerun_data(root: &Path, calibration: &Json, map: &Json) -> Result<Data> {
    let demography = crate::map4::read(root, "data/calibration/demography.json")?;
    let definitions = Definitions {
        demography: serde_json::from_str(&plain(&demography)?)
            .map_err(|error| format!("demography: {error}"))?,
        calibration: serde_json::from_str(&plain(calibration)?)
            .map_err(|error| format!("calibration: {error}"))?,
        genome_map: serde_json::from_str(&plain(map)?)
            .map_err(|error| format!("genome map: {error}"))?,
    };
    Data::new(definitions).map_err(|error| format!("the pre-run data: {error}"))
}

/// One group of founders: a sex and a whole age, its weight and its count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    pub sex: Sex,
    pub age: u32,
    pub weight: f64,
    pub count: u64,
}

/// The Makeham term, the Gompertz term and `σ_g` of a sex.
fn hazard_terms(data: &Data, sex: Sex) -> (f64, mortality::Gompertz, f64) {
    let genetics = data.genetics();
    match sex {
        Sex::Female => (
            genetics.accident_female,
            genetics.senescence_female,
            genetics.longevity_sigma_g,
        ),
        Sex::Male => (
            genetics.accident_male,
            genetics.senescence_male,
            genetics.longevity_sigma_g,
        ),
    }
}

/// `S̄_s(x)`: the survival to the exact age `x`, averaged over `g_L ~ N(0, 1)` by the
/// trapezoidal rule on `g_k = −8 + k/64` (design note, section 2).
pub fn survival(data: &Data, sex: Sex, x: f64) -> f64 {
    let (c, gompertz, sigma_g) = hazard_terms(data, sex);
    let root = sim_math::sqrt(2.0 * core::f64::consts::PI);
    let mut sum = 0.0;
    for k in 0..=1024_u32 {
        let g = f64::from(k) / 64.0 - 8.0;
        let m = mortality::multiplier(Some(g), sigma_g);
        let h = c * x + (gompertz.a * m / gompertz.b) * (sim_math::exp(gompertz.b * x) - 1.0);
        let density = sim_math::exp(-(g * g) / 2.0) / root;
        let weight = if k == 0 || k == 1024 {
            1.0 / 128.0
        } else {
            1.0 / 64.0
        };
        sum += weight * (density * sim_math::exp(-h));
    }
    sum
}

/// `∫_a^{a+1} S̄_s(x) dx` by Simpson's rule with 64 subintervals.
pub fn year_survival(data: &Data, sex: Sex, age: u32) -> f64 {
    let a = f64::from(age);
    let mut sum = 0.0;
    for i in 0..=64_u32 {
        let factor = if i == 0 || i == 64 {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        sum += factor * survival(data, sex, a + f64::from(i) / 64.0);
    }
    sum / (3.0 * 64.0)
}

/// The groups by sex and age with their weights `p_s · ∫ S̄` (design note, section 2),
/// women first, ages ascending, the counts still 0.
pub fn weights(data: &Data) -> Vec<Group> {
    [(Sex::Female, 20.0 / 41.0), (Sex::Male, 21.0 / 41.0)]
        .into_iter()
        .flat_map(|(sex, share)| {
            AGES.map(move |age| Group {
                sex,
                age,
                weight: share * year_survival(data, sex, age),
                count: 0,
            })
        })
        .collect()
}

/// The groups of `n` founders: counts by largest remainders of `n · w / Σ w`, the rest one
/// each to the largest remainders, ties women first, then the younger age.
pub fn counted(weighted: &[Group], n: u64) -> Vec<Group> {
    let mut all = weighted.to_vec();
    let total = all.iter().fold(0.0, |sum, group| sum + group.weight);
    let quota: Vec<f64> = all
        .iter()
        .map(|group| exact(n) * group.weight / total)
        .collect();
    let mut given = 0;
    for (group, q) in all.iter_mut().zip(&quota) {
        group.count = floor_u64(*q);
        given += group.count;
    }
    // The rest one each by the largest remainder; ties keep the order of `all` (a stable
    // sort).
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|&i, &j| {
        let rest = |k: usize| quota[k] - exact(all[k].count);
        rest(j).total_cmp(&rest(i))
    });
    for &index in order.iter().take(usize::try_from(n - given).unwrap_or(0)) {
        all[index].count += 1;
    }
    all
}

/// The groups of `n` founders.
pub fn groups(data: &Data, n: u64) -> Vec<Group> {
    counted(&weights(data), n)
}

fn exact(n: u64) -> f64 {
    i64::try_from(n)
        .ok()
        .and_then(sim_math::i64_to_f64_exact)
        .unwrap_or_else(|| panic!("a count below 2^53"))
}

fn floor_u64(q: f64) -> u64 {
    sim_math::to_i64_exact(q.floor())
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or_else(|| panic!("a count"))
}

/// The additive loci of a pre-run's map, in map order.
fn additive_loci(data: &Data) -> Vec<DataId> {
    data.genome_map()
        .loci
        .iter()
        .filter(|locus| matches!(locus.action, sim_core::data::Action::Additive(_)))
        .map(|locus| locus.id.clone())
        .collect()
}

/// The founder at place `index` of a run with these groups: women first, ages ascending,
/// `j` within a group; born `a · YEAR + ((2 j + 1) · YEAR) div (2 c)` before the start;
/// alleles at the additive loci by `PRERUN_ALLELE`. `None` beyond the last founder.
pub fn founder(
    data: &Data,
    groups: &[Group],
    tag: &DataId,
    seed: u64,
    index: u64,
) -> Option<Founder> {
    let mut before = 0;
    let (group, j) = groups.iter().find_map(|group| {
        let here = index.checked_sub(before).filter(|j| *j < group.count);
        before += group.count;
        here.map(|j| (group, j))
    })?;
    let draws = Draws::new(WorldSeed::new(seed));
    let key = LineageKey::founder(tag, index);
    let year = YEAR.as_secs();
    let c = i64::try_from(group.count).ok()?;
    let j = i64::try_from(j).ok()?;
    let born = i64::from(group.age) * year + ((2 * j + 1) * year).div_euclid(2 * c);
    let alleles = additive_loci(data)
        .into_iter()
        .filter_map(|locus| {
            let copy = |detail: u64| {
                let u = draws.unit(DrawKey {
                    tag: tags::PRERUN_ALLELE,
                    subject: SubjectKey::from(key),
                    occasion: str_key(&locus),
                    detail,
                });
                u8::from(u < 0.5)
            };
            let copies = [copy(0), copy(1)];
            (copies != [0, 0]).then(|| LocusAlleles {
                locus,
                copies: copies.to_vec(),
            })
        })
        .collect();
    Some(Founder {
        born: SimTime::from_secs(-born),
        sex: group.sex,
        alleles,
    })
}

/// All founders of a run with these groups, in their order.
pub fn founders(data: &Data, groups: &[Group], tag: &DataId, seed: u64) -> Vec<Founder> {
    (0..)
        .map_while(|index| founder(data, groups, tag, seed, index))
        .collect()
}

/// The `Start` of a pre-run: no couples, the switch on, no nutrition, the capacity `n`.
pub fn start(tag: &DataId, seed: u64, founders: Vec<Founder>, capacity: u64) -> Input {
    Input::Start(Start {
        world_seed: seed,
        founder_tag: tag.clone(),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders,
        couples: Vec::new(),
        capacity: Some(capacity),
    })
}

/// A run to `until` (inclusive); the world and its year output. The log is dropped year
/// by year: a run of 750 years would otherwise hold millions of entries.
pub fn run(data: Data, start: Input, until: SimTime) -> Result<(World, YearOut)> {
    let mut world = World::new(data);
    world
        .submit(start)
        .map_err(|rejected| format!("Start rejected: {:?}", rejected.code))?;
    world.take_log();
    let mut out = YearOut::default();
    let mut year_end = YEAR.as_secs() - 1;
    while year_end < until.as_secs() {
        world.advance_until(&mut out, SimTime::from_secs(year_end), &mut |_| false);
        world.take_log();
        year_end += YEAR.as_secs();
    }
    world.advance_until(&mut out, until, &mut |_| false);
    world.take_log();
    Ok((world, out))
}

/// One generation: the persons born in a block of 30 years (ADR-0020 section 8).
#[derive(Clone, Debug, PartialEq)]
pub struct Generation {
    pub block: i64,
    pub born: u64,
    pub mothers: u64,
    pub fathers: u64,
    pub n_e: f64,
    /// The variance of `A` (divisor `n`) per reported trait.
    pub variance: Vec<(String, f64)>,
}

/// The traits whose variance of `A` the report gives per generation.
const GENERATION_TRAITS: [&str; 2] = ["core:growth", "core:cognition"];

/// The generations of a run from its pedigree: everyone born in the run (living, dead and
/// emigrated; not the unborn), by block `born div 30 YEAR`; the distinct mothers and
/// fathers, `N_e = 4 N_m N_f / (N_m + N_f)`, and the variance of `A` by the run's map
/// (sums from +0.0 in ascending lineage key).
pub fn generations(world: &World) -> Result<Vec<Generation>> {
    let data = world.state().data();
    let pedigree = world.pedigree();
    let block_of = |born: SimTime| born.as_secs().div_euclid(30 * YEAR.as_secs());
    let mut blocks: std::collections::BTreeMap<i64, Vec<&sim_core::pedigree::Person>> =
        std::collections::BTreeMap::new();
    for person in pedigree.persons.iter().filter(|p| p.born.as_secs() >= 0) {
        blocks
            .entry(block_of(person.born))
            .or_default()
            .push(person);
    }
    let mut all = Vec::new();
    for (block, mut persons) in blocks {
        persons.sort_by_key(|person| person.lineage_key);
        let distinct = |pick: fn(&sim_core::pedigree::Person) -> Option<sim_core::ids::SimId>| {
            let set: std::collections::BTreeSet<_> =
                persons.iter().filter_map(|p| pick(p)).collect();
            u64::try_from(set.len()).unwrap_or(0)
        };
        let mothers = distinct(|p| p.mother);
        let fathers = distinct(|p| p.father);
        let n = exact(u64::try_from(persons.len()).unwrap_or(0));
        let mut variance = Vec::new();
        for t in GENERATION_TRAITS {
            let id = DataId::new(t).map_err(|_| format!("a trait id {t}"))?;
            let mut values = Vec::new();
            for person in &persons {
                let genome =
                    sim_core::genome::Genome::with_alleles(person.sex, &person.alleles, data)
                        .map_err(|_| format!("the alleles of sim {}", person.sim.get()))?;
                values.push(
                    genome
                        .additive_value(&id, data)
                        .ok_or("a trait of the map")?,
                );
            }
            let mean = values.iter().fold(0.0, |sum, a| sum + a) / n;
            let v = values
                .iter()
                .fold(0.0, |sum, a| sum + (a - mean) * (a - mean))
                / n;
            variance.push((t.to_owned(), v));
        }
        all.push(Generation {
            block,
            born: u64::try_from(persons.len()).unwrap_or(0),
            mothers,
            fathers,
            n_e: exact(4 * mothers * fathers) / exact(mothers + fathers),
            variance,
        });
    }
    Ok(all)
}

/// The map of the next run: every additive locus's `d_ref` its mean dose, every trait's
/// `sigma_ref` its measured `σ` (design note, section 4).
pub fn next_map(map: &Json, measurement: &Measurement) -> Json {
    let mut map = map.clone();
    if let Some(traits) = map.get_mut("traits").and_then(Json::as_array_mut) {
        for t in traits {
            let id = t.get("id").and_then(Json::as_str).unwrap_or("").to_owned();
            if let Some(spread) = measurement
                .traits
                .iter()
                .find(|s| s.trait_id.as_str() == id)
            {
                t.set("sigma_ref", Json::float(spread.sigma));
            }
        }
    }
    if let Some(loci) = map.get_mut("loci").and_then(Json::as_array_mut) {
        for locus in loci {
            let id = locus
                .get("id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned();
            if let Some(dose) = measurement.loci.iter().find(|l| l.locus.as_str() == id)
                && let Some(additive) = locus
                    .get_mut("action")
                    .and_then(|action| action.get_mut("additive"))
            {
                additive.set("d_ref", Json::float(dose.mean_dose));
            }
        }
    }
    map
}

/// `r = |σ_out − σ_in| / σ_in` per trait, in the order of the measurement.
pub fn ratios(map: &Json, measurement: &Measurement) -> Vec<(String, f64)> {
    let sigma_in = |id: &str| {
        map.get("traits")
            .and_then(Json::as_array)
            .and_then(|traits| {
                traits
                    .iter()
                    .find(|t| t.get("id").and_then(Json::as_str) == Some(id))
            })
            .and_then(|t| match t.get("sigma_ref") {
                Some(Json::Number(n)) => n.as_f64(),
                _ => None,
            })
            .unwrap_or(f64::NAN)
    };
    measurement
        .traits
        .iter()
        .map(|spread| {
            let id = spread.trait_id.as_str();
            let before = sigma_in(id);
            (id.to_owned(), (spread.sigma - before).abs() / before)
        })
        .collect()
}

/// Whether the five deciding traits all moved by less than 2 percent.
pub fn settled(ratios: &[(String, f64)]) -> bool {
    DECIDING.iter().all(|id| {
        ratios
            .iter()
            .find(|(t, _)| t == id)
            .is_some_and(|(_, r)| *r < STOP)
    })
}

/// What follows a run (ADR-0020 section 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// The five settled: freeze this run's measurement.
    Stop,
    /// Run again with this run's measurement.
    Continue,
    /// Run 5 without settling: no freeze, the question goes to Grok.
    NoFreeze,
}

pub fn decision(run: usize, ratios: &[(String, f64)]) -> Decision {
    if settled(ratios) {
        Decision::Stop
    } else if run >= MAX_RUNS {
        Decision::NoFreeze
    } else {
        Decision::Continue
    }
}

/// The other traits that moved by 5 percent or more, for the report.
pub fn reported(ratios: &[(String, f64)]) -> Vec<String> {
    ratios
        .iter()
        .filter(|(id, r)| !DECIDING.contains(&id.as_str()) && *r >= REPORT)
        .map(|(id, _)| id.clone())
        .collect()
}

/// The options of `cargo xtask prerun`.
pub struct Options {
    pub out: std::path::PathBuf,
    pub founders: u64,
    pub years: i64,
    /// For `pool`: the directory of the frozen map and its calibration.
    pub map: Option<std::path::PathBuf>,
}

fn print_measurement(run: usize, measurement: &Measurement, ratios: &[(String, f64)]) {
    println!("run {run} living {}", measurement.living);
    for (spread, (_, r)) in measurement.traits.iter().zip(ratios) {
        println!(
            "run {run} trait {} mean {:?} sigma {:?} r {:?}",
            spread.trait_id.as_str(),
            spread.mean,
            spread.sigma,
            r
        );
    }
    for dose in &measurement.loci {
        println!(
            "run {run} locus {} mean_dose {:?}",
            dose.locus.as_str(),
            dose.mean_dose
        );
    }
}

fn write_files(dir: &Path, calibration: &Json, map: &Json) -> Result {
    for (path, json) in [
        ("calibration/genetics.json", calibration),
        ("genetics/genome_map.json", map),
    ] {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&file, json.pretty()?)
            .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
    }
    Ok(())
}

/// The measurement run, iterated until the five deciding traits settle (design note,
/// section 4); writes the frozen map and its calibration to `options.out`.
pub fn measure(root: &Path, options: &Options) -> Result {
    let (calibration, template_map) = crate::map4::template(root)?;
    let tag = DataId::new("core:prerun_measure").map_err(|_| "a tag".to_owned())?;
    let until = SimTime::from_secs(options.years * YEAR.as_secs() - 1);
    let (first_calibration, first_map) = prerun_files(&calibration, &template_map);
    let first = prerun_data(root, &first_calibration, &first_map)?;
    let groups = groups(&first, options.founders);
    for group in &groups {
        println!(
            "group {:?} age {} weight {:?} count {}",
            group.sex, group.age, group.weight, group.count
        );
    }
    let founders = founders(&first, &groups, &tag, MEASURE_SEED);
    let mut map = template_map;
    for run_number in 1..=MAX_RUNS {
        let (run_calibration, run_map) = prerun_files(&calibration, &map);
        let data = prerun_data(root, &run_calibration, &run_map)?;
        println!("run {run_number} data_hash {:016x}", data.hash());
        let started = std::time::Instant::now();
        let input = start(&tag, MEASURE_SEED, founders.clone(), options.founders);
        let (world, out) = run(data, input, until)?;
        for row in &out.census {
            println!("run {run_number} year {} living {}", row.year, row.living);
        }
        let measurement = world.state().measure().ok_or("nobody lives at the end")?;
        let ratios = ratios(&map, &measurement);
        print_measurement(run_number, &measurement, &ratios);
        for id in reported(&ratios) {
            println!("run {run_number} report {id}");
        }
        for generation in generations(&world)? {
            println!(
                "run {run_number} generation {} born {} mothers {} fathers {} n_e {:?} var_a {:?}",
                generation.block,
                generation.born,
                generation.mothers,
                generation.fathers,
                generation.n_e,
                generation.variance
            );
        }
        println!(
            "run {run_number} seconds {:?}",
            started.elapsed().as_secs_f64()
        );
        let frozen = next_map(&map, &measurement);
        match decision(run_number, &ratios) {
            Decision::Stop => {
                write_files(&options.out, &calibration, &frozen)?;
                println!("frozen after run {run_number}");
                return Ok(());
            }
            Decision::Continue => map = frozen,
            Decision::NoFreeze => break,
        }
    }
    Err(format!(
        "no freeze: a deciding trait still moved by 2 percent or more after {MAX_RUNS} runs (a question for Grok)"
    ))
}

/// The pool run on the frozen map (design note, section 3): to `years · YEAR` inclusive;
/// its save goes to `options.out` for the pool slice.
pub fn pool(root: &Path, options: &Options) -> Result {
    let dir = options.map.as_ref().ok_or("pool needs --map <dir>")?;
    let read = |path: &str| -> Result<Json> {
        let text = std::fs::read_to_string(dir.join(path))
            .map_err(|error| format!("cannot read {path}: {error}"))?;
        Json::parse(&text)
    };
    let (calibration, map) = prerun_files(
        &read("calibration/genetics.json")?,
        &read("genetics/genome_map.json")?,
    );
    let data = prerun_data(root, &calibration, &map)?;
    println!("pool data_hash {:016x}", data.hash());
    let tag = DataId::new("core:prerun_pool").map_err(|_| "a tag".to_owned())?;
    let groups = groups(&data, options.founders);
    let founders = founders(&data, &groups, &tag, POOL_SEED);
    let input = start(&tag, POOL_SEED, founders, options.founders);
    let until = SimTime::from_secs(options.years * YEAR.as_secs());
    let (world, out) = run(data, input, until)?;
    for row in &out.census {
        println!("pool year {} living {}", row.year, row.living);
    }
    let save = world
        .save(&out.year_hashes)
        .map_err(|error| format!("cannot save: {error:?}"))?;
    std::fs::create_dir_all(&options.out)
        .map_err(|error| format!("cannot create {}: {error}", options.out.display()))?;
    std::fs::write(options.out.join("pool.save"), save)
        .map_err(|error| format!("cannot write the save: {error}"))
}

/// `cargo xtask prerun (measure | pool) --out <dir> [--founders N] [--years Y] [--map <dir>]`.
pub fn command(root: &Path, args: &[String]) -> Result {
    let (which, rest) = args.split_first().ok_or("prerun needs measure or pool")?;
    let value = |name: &str| {
        rest.iter()
            .position(|arg| arg == name)
            .and_then(|at| rest.get(at + 1))
            .filter(|value| !value.is_empty())
    };
    let number = |name: &str, default: u64| -> Result<u64> {
        value(name).map_or(Ok(default), |text| {
            text.parse()
                .map_err(|_| format!("{name} {text} is not a number"))
        })
    };
    let out = value("--out").ok_or("prerun needs --out <dir>")?;
    let (default_founders, run) = match which.as_str() {
        "measure" => (MEASURE_FOUNDERS, measure as fn(&Path, &Options) -> Result),
        "pool" => (POOL_FOUNDERS, pool as fn(&Path, &Options) -> Result),
        other => return Err(format!("prerun {other}: measure or pool")),
    };
    let years = i64::try_from(number("--years", YEARS)?).map_err(|_| "--years".to_owned())?;
    let options = Options {
        out: out.into(),
        founders: number("--founders", default_founders)?,
        years,
        map: value("--map").map(Into::into),
    };
    run(root, &options)
}

#[cfg(test)]
#[path = "prerun_tests.rs"]
mod tests;
