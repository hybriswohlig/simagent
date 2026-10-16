//! The export of a world for the criteria (design note of the criteria kit, section 1), from
//! public views only: the pedigree, the phenotype view, the genome of the pedigree's
//! alleles, the data's pool, and two observers of the log (the genome keys of town children
//! and the multiple index of each town conception). The core does not change.

use std::collections::{BTreeMap, BTreeSet};

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::{Genome, Sex};
use sim_core::ids::SimId;
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::pedigree::Person;
use sim_core::pool::GenomeKeys;
use sim_core::time::{SimDuration, SimTime};
use sim_core::version::CORE_VERSION;
use sim_core::world::World;

use super::export::{CoupleRow, Header, RawGenome, Row, couple_line, header_line, person_line};

type Result<T> = std::result::Result<T, String>;

const YEAR: i64 = 31_449_600;

/// The traits of `z` and the age each is read at: growth and pigment have equal child and
/// adult weights (age 0), cognition is read at 60 years as P7 reads it.
const TRAITS: [(&str, i64); 3] = [
    ("core:growth", 0),
    ("core:pigment", 0),
    ("core:cognition", 60 * YEAR),
];
const LONGEVITY: &str = "core:longevity";
const MC1R: &str = "core:mc1r";

/// What the export reads from the log: the genome key of every town child, and per town
/// conception the number of earlier `core:conception` entries with its second and mother.
#[derive(Clone, Debug, Default)]
pub struct Observer {
    keys: GenomeKeys,
    multiple: BTreeMap<LineageKey, u64>,
    earlier: BTreeMap<(SimTime, SimId), u64>,
}

impl Observer {
    /// Reads one batch of the log, in order.
    pub fn observe(&mut self, entries: &[LogEntry]) {
        self.keys.observe(entries);
        for entry in entries {
            if let LogPayload::Conception { lineage_key } = &entry.payload {
                let Some(Subject::Sim(mother)) = entry.subjects.first().copied() else {
                    panic!("invariant criteria.conception_subjects");
                };
                let before = self.earlier.entry((entry.t, mother)).or_insert(0);
                self.multiple.insert(*lineage_key, *before);
                *before += 1;
            }
        }
    }
}

/// What the pool data say about its persons: the live births an arrival brought, the place
/// of each unborn child among the children of its pregnancy, and the open partnerships.
struct PoolFacts {
    brought: BTreeMap<LineageKey, u64>,
    place: BTreeMap<LineageKey, u64>,
    partnerships: BTreeSet<(LineageKey, LineageKey)>,
}

fn pool_facts(data: &Data) -> PoolFacts {
    let mut facts = PoolFacts {
        brought: BTreeMap::new(),
        place: BTreeMap::new(),
        partnerships: BTreeSet::new(),
    };
    let Some(pool) = data.pool() else {
        return facts;
    };
    for household in &pool.pool().households {
        for member in &household.members {
            facts
                .brought
                .insert(member.lineage_key, u64::from(member.births));
        }
        for pregnancy in &household.pregnancies {
            for (place, child) in (0_u64..).zip(&pregnancy.children) {
                facts.place.insert(child.lineage_key, place);
            }
        }
        if let Some(partnership) = &household.partnership {
            facts
                .partnerships
                .insert((partnership.woman, partnership.man));
        }
    }
    facts
}

/// The raw genome of section 1, read from the genome's own serialised form: the two
/// autosome sets, the maternal X, the paternal X or Y, the mitochondria.
fn raw_genome(genome: &Genome) -> Result<RawGenome> {
    let value = serde_json::to_value(genome).map_err(|e| e.to_string())?;
    let words = |name: &str| -> Result<[u64; 3]> {
        let word = |i: usize| {
            value[name][i]
                .as_u64()
                .ok_or_else(|| format!("genome: {name}"))
        };
        match value[name].as_array().map(Vec::len) {
            Some(3) => Ok([word(0)?, word(1)?, word(2)?]),
            _ => Err(format!("genome: {name}")),
        }
    };
    let paternal = &value["paternal_sex"];
    Ok(RawGenome {
        maternal: words("maternal")?,
        paternal: words("paternal")?,
        x: value["maternal_x"].as_u64().ok_or("genome: x")?,
        paternal_x: paternal["x"].as_u64(),
        y: paternal["y"].as_u64(),
        mt: value["mt"].as_u64().ok_or("genome: mt")?,
    })
}

fn id(text: &str) -> Result<DataId> {
    DataId::new(text).map_err(|e| format!("{text}: {e:?}"))
}

/// One person's row (section 1). A town child born in the run must have its conception in
/// the observer, or the observer missed a batch of the log.
fn person_row(
    world: &World,
    person: &Person,
    key_of: &BTreeMap<SimId, LineageKey>,
    observer: &Observer,
    facts: &PoolFacts,
) -> Result<Row> {
    let data = world.data();
    let key = person.lineage_key;
    let parent = |sim: Option<SimId>, from_pool: Option<LineageKey>| {
        sim.and_then(|sim| key_of.get(&sim).copied())
            .or(from_pool)
            .map(LineageKey::get)
    };
    let pool = person.pool.as_ref();
    let kind = match pool {
        None => 0,
        Some(origin) if origin.born_pool.is_some() => 1,
        Some(_) => 2,
    };
    let gkey = match pool {
        Some(origin) => origin.genome_key.get(),
        None => observer.keys.of(key).get(),
    };
    let mult = match kind {
        0 if person.mother.is_some() => Some(
            *observer
                .multiple
                .get(&key)
                .ok_or_else(|| format!("no conception of {:016x} in the log", key.get()))?,
        ),
        2 => Some(
            *facts
                .place
                .get(&key)
                .ok_or("a pool child without a pregnancy")?,
        ),
        _ => None,
    };
    let brought = if kind == 1 {
        *facts
            .brought
            .get(&key)
            .ok_or("an arrival not in the pool")?
    } else {
        0
    };
    let genome = Genome::with_alleles(person.sex, &person.alleles, data)
        .map_err(|e| format!("genome of {:016x}: {e:?}", key.get()))?;
    let mut z = [0.0; 3];
    for (value, (name, age)) in z.iter_mut().zip(TRAITS) {
        *value = world
            .state()
            .phenotype(person.sim, &id(name)?, SimDuration::from_secs(age))
            .ok_or_else(|| format!("no {name} of {:016x}", key.get()))?;
    }
    Ok(Row {
        key: key.get(),
        gkey,
        female: person.sex == Sex::Female,
        born: person.born.as_secs(),
        died: person.died.map(SimTime::as_secs),
        emig: person.emigrated.map(SimTime::as_secs),
        mother: parent(person.mother, pool.and_then(|o| o.mother_key)),
        father: parent(person.father, pool.and_then(|o| o.father_key)),
        pool: kind,
        arrived: pool.filter(|_| kind == 1).map(|o| o.arrived.as_secs()),
        mult,
        brought,
        genome: raw_genome(&genome)?,
        z,
        gl: genome
            .genetic_value(&id(LONGEVITY)?, data)
            .ok_or("no longevity trait")?,
        mc1r: genome.dose(data.locus(&id(MC1R)?).ok_or("no mc1r locus")?),
    })
}

/// The export of `world` run to `end` (its last second, inclusive; the world must have
/// closed exactly that second), with `observer` fed every batch of its log: the header, the
/// persons in ascending lineage key, the couples in ascending `(formed, woman, man)`.
pub fn write(world: &World, observer: &Observer, end: SimTime) -> Result<String> {
    if world.closed_through() != end {
        return Err(format!(
            "the world closed {} seconds, not the end {}",
            world.closed_through().as_secs(),
            end.as_secs()
        ));
    }
    let data = world.data();
    let pedigree = world.pedigree();
    let world_seed = world.state().world_seed().ok_or("the world has no Start")?;
    let key_of: BTreeMap<SimId, LineageKey> = pedigree
        .persons
        .iter()
        .map(|person| (person.sim, person.lineage_key))
        .collect();
    let arrived: BTreeMap<LineageKey, SimTime> = pedigree
        .persons
        .iter()
        .filter_map(|person| {
            let origin = person.pool.as_ref()?;
            origin
                .born_pool
                .map(|_| (person.lineage_key, origin.arrived))
        })
        .collect();
    let facts = pool_facts(data);
    let mut rows: Vec<Row> = pedigree
        .persons
        .iter()
        .map(|person| person_row(world, person, &key_of, observer, &facts))
        .collect::<Result<_>>()?;
    rows.sort_by_key(|row| row.key);
    let mut couples: Vec<CoupleRow> = pedigree
        .partnerships
        .iter()
        .map(|couple| {
            let woman = *key_of.get(&couple.woman).ok_or("a couple's woman")?;
            let man = *key_of.get(&couple.man).ok_or("a couple's man")?;
            // Carried: the pool held the two as a household's open partnership, she
            // arrived, and it was formed by her arrival (a pair formed at the arrival
            // second by the market is not carried unless the pool holds it).
            let carried = facts.partnerships.contains(&(woman, man))
                && arrived.get(&woman).is_some_and(|at| couple.formed <= *at);
            Ok(CoupleRow {
                woman: woman.get(),
                man: man.get(),
                formed: couple.formed.as_secs(),
                carried,
            })
        })
        .collect::<Result<_>>()?;
    couples.sort_by_key(|couple| (couple.formed, couple.woman, couple.man));
    let header = Header {
        core_version: u64::from(CORE_VERSION),
        data_hash: data.hash(),
        world_seed,
        pool_seed: data.pool().map(|pool| pool.pool().pool_seed),
        end: end.as_secs(),
        traits: TRAITS
            .iter()
            .map(|(name, age)| ((*name).to_owned(), *age))
            .collect(),
        longevity: LONGEVITY.to_owned(),
        mc1r: MC1R.to_owned(),
        persons: u64::try_from(rows.len()).map_err(|e| e.to_string())?,
        couples: u64::try_from(couples.len()).map_err(|e| e.to_string())?,
    };
    let mut text = header_line(&header) + "\n";
    for row in &rows {
        text.push_str(&person_line(row));
        text.push('\n');
    }
    for couple in &couples {
        text.push_str(&couple_line(couple));
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
