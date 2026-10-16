//! The export of a world for the criteria (design note of the criteria kit, section 1), from
//! public views only: the pedigree, the phenotype view, the genome of the pedigree's
//! alleles, the data's pool, and two observers of the log (the genome keys of town children
//! and the multiple index of each town conception). The core does not change.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
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

type Result<T> = std::result::Result<T, String>;

const YEAR: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

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

fn hex(value: u64) -> String {
    format!("\"{value:016x}\"")
}

fn bits(value: f64) -> String {
    hex(value.to_bits())
}

fn or_null(value: Option<String>) -> String {
    value.unwrap_or_else(|| "null".to_owned())
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

/// The raw genome as section 1 writes it: `{"m":[3 hex],"p":[3 hex],"x":…,"px":…,"y":…,"mt":…}`,
/// read from the genome's own serialised form.
fn genome_json(genome: &Genome) -> Result<String> {
    let value = serde_json::to_value(genome).map_err(|e| e.to_string())?;
    let words = |name: &str| -> Result<String> {
        let words = value[name]
            .as_array()
            .filter(|words| words.len() == 3)
            .ok_or_else(|| format!("genome: {name}"))?;
        let words: Vec<String> = words
            .iter()
            .map(|word| word.as_u64().map(hex).ok_or("genome: a word"))
            .collect::<std::result::Result<_, _>>()?;
        Ok(format!("[{}]", words.join(",")))
    };
    let number = |value: &Value| value.as_u64().map(|n| n.to_string());
    let paternal = &value["paternal_sex"];
    Ok(format!(
        "{{\"m\":{},\"p\":{},\"x\":{},\"px\":{},\"y\":{},\"mt\":{}}}",
        words("maternal")?,
        words("paternal")?,
        number(&value["maternal_x"]).ok_or("genome: x")?,
        or_null(number(&paternal["x"])),
        or_null(number(&paternal["y"])),
        number(&value["mt"]).ok_or("genome: mt")?,
    ))
}

fn id(text: &str) -> Result<DataId> {
    DataId::new(text).map_err(|e| format!("{text}: {e:?}"))
}

/// One person's line, its fields in the order of section 1.
fn person_line(
    world: &World,
    person: &Person,
    key_of: &BTreeMap<SimId, LineageKey>,
    observer: &Observer,
    facts: &PoolFacts,
) -> Result<String> {
    let data = world.data();
    let key = person.lineage_key;
    let parent = |sim: Option<SimId>, from_pool: Option<LineageKey>| {
        sim.and_then(|sim| key_of.get(&sim).copied())
            .or(from_pool)
            .map(|key| hex(key.get()))
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
    let multiple = match kind {
        0 => observer.multiple.get(&key).copied(),
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
    let z = TRAITS
        .iter()
        .map(|(name, age)| {
            let age = SimDuration::from_secs(*age);
            let value = world.state().phenotype(person.sim, &id(name)?, age);
            value
                .map(bits)
                .ok_or_else(|| format!("no {name} of {:016x}", key.get()))
        })
        .collect::<Result<Vec<String>>>()?;
    let gl = genome
        .genetic_value(&id(LONGEVITY)?, data)
        .ok_or("no longevity trait")?;
    let mc1r = genome.dose(data.locus(&id(MC1R)?).ok_or("no mc1r locus")?);
    Ok(format!(
        "{{\"k\":\"p\",\"key\":{},\"gkey\":{},\"sex\":\"{}\",\"born\":{},\"died\":{},\"emig\":{},\"mother\":{},\"father\":{},\"pool\":{kind},\"arrived\":{},\"mult\":{},\"brought\":{brought},\"genome\":{},\"z\":[{}],\"gl\":{},\"mc1r\":{mc1r}}}",
        hex(key.get()),
        hex(gkey),
        if person.sex == Sex::Female { "f" } else { "m" },
        person.born.as_secs(),
        or_null(person.died.map(|t| t.as_secs().to_string())),
        or_null(person.emigrated.map(|t| t.as_secs().to_string())),
        or_null(parent(person.mother, pool.and_then(|o| o.mother_key))),
        or_null(parent(person.father, pool.and_then(|o| o.father_key))),
        or_null(
            pool.filter(|_| kind == 1)
                .map(|o| o.arrived.as_secs().to_string())
        ),
        or_null(multiple.map(|n| n.to_string())),
        genome_json(&genome)?,
        z.join(","),
        bits(gl),
    ))
}

/// The export of `world` run to `end` (its last second, inclusive), with `observer` fed
/// every batch of its log: the header, the persons in ascending lineage key, the couples
/// in ascending `(formed, woman, man)`, one line each.
pub fn write(world: &World, observer: &Observer, end: SimTime) -> Result<String> {
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
    let mut persons: Vec<(u64, String)> = pedigree
        .persons
        .iter()
        .map(|person| {
            let line = person_line(world, person, &key_of, observer, &facts)?;
            Ok((person.lineage_key.get(), line))
        })
        .collect::<Result<_>>()?;
    persons.sort_by_key(|(key, _)| *key);
    let mut couples: Vec<(i64, u64, u64, bool)> = pedigree
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
            Ok((couple.formed.as_secs(), woman.get(), man.get(), carried))
        })
        .collect::<Result<_>>()?;
    couples.sort_by_key(|(formed, woman, man, _)| (*formed, *woman, *man));
    let traits: Vec<String> = TRAITS
        .iter()
        .map(|(name, age)| format!("[\"{name}\",{age}]"))
        .collect();
    let mut text = format!(
        "{{\"format\":\"simagent.criteria\",\"version\":1,\"core_version\":{CORE_VERSION},\"data_hash\":{},\"world_seed\":{},\"pool_seed\":{},\"end\":{},\"year\":{YEAR},\"month\":{MONTH},\"traits\":[{}],\"longevity\":\"{LONGEVITY}\",\"mc1r\":\"{MC1R}\",\"persons\":{},\"couples\":{}}}\n",
        hex(data.hash()),
        hex(world_seed),
        or_null(data.pool().map(|pool| hex(pool.pool().pool_seed))),
        end.as_secs(),
        traits.join(","),
        persons.len(),
        couples.len(),
    );
    for (_, line) in persons {
        text.push_str(&line);
        text.push('\n');
    }
    for (formed, woman, man, carried) in couples {
        text.push_str(&format!(
            "{{\"k\":\"c\",\"woman\":{},\"man\":{},\"formed\":{formed},\"carried\":{carried}}}\n",
            hex(woman),
            hex(man),
        ));
    }
    Ok(text)
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
