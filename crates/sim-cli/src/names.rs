//! The names of persons (ADR-0021 section 3): the lists of `data/names`, embedded in the
//! binary like the pool and outside the data hash (names are presentation); the core draws
//! the places in them (`sim_core::presentation::names`). The export of a run carries them.

use std::num::NonZeroU64;

use serde::Serialize;
use sim_core::genome::Sex;
use sim_core::lineage::LineageKey;
use sim_core::pedigree::{Couple, Home, Person};
use sim_core::presentation::names::{first_name_index, surname_index};
use sim_core::rng::WorldSeed;
use sim_core::world::World;

use crate::error::CliError;

const FEMALE: &str = include_str!("../../../data/names/female.json");
const MALE: &str = include_str!("../../../data/names/male.json");
const SURNAMES: &str = include_str!("../../../data/names/surnames.json");

/// The version of the form of the export (`pedigree.json`): the core's pedigree (format 4)
/// with each person's names.
pub const EXPORT_FORMAT: u32 = 5;

/// The lists a name is taken from.
pub struct NameLists {
    female: Vec<String>,
    male: Vec<String>,
    surnames: Vec<String>,
}

/// A list of names, not empty.
fn list(text: &str, which: &str) -> Result<Vec<String>, CliError> {
    let names: Vec<String> = serde_json::from_str(text)
        .map_err(|e| CliError::new("names_invalid", format!("{which}: {e}")))?;
    if names.is_empty() {
        return Err(CliError::new("names_invalid", format!("{which}: no name")));
    }
    Ok(names)
}

impl NameLists {
    /// The lists of the binary.
    pub fn load() -> Result<NameLists, CliError> {
        Ok(NameLists {
            female: list(FEMALE, "female")?,
            male: list(MALE, "male")?,
            surnames: list(SURNAMES, "surnames")?,
        })
    }

    /// The first name of the person with `key` and `sex`, and the surname of the chain with
    /// `root`, drawn in the world with `seed`.
    pub fn name(
        &self,
        seed: WorldSeed,
        key: LineageKey,
        sex: Sex,
        root: LineageKey,
    ) -> (&str, &str) {
        let firsts = match sex {
            Sex::Female => &self.female,
            Sex::Male => &self.male,
        };
        (
            pick(firsts, |len| first_name_index(seed, key, sex, len)),
            pick(&self.surnames, |len| surname_index(seed, root, len)),
        )
    }
}

/// The name at the place `index` draws in `names`: a list `load` found not empty, and a
/// place below its length.
fn pick(names: &[String], index: impl FnOnce(NonZeroU64) -> u64) -> &str {
    let len = (u64::try_from(names.len()).ok())
        .and_then(NonZeroU64::new)
        .unwrap_or_else(|| panic!("invariant names.list_empty"));
    (usize::try_from(index(len)).ok())
        .and_then(|at| names.get(at))
        .map_or_else(|| panic!("invariant names.index"), String::as_str)
}

/// A person of the export: the core's fields, then its names.
#[derive(Serialize)]
struct Named<'a> {
    #[serde(flatten)]
    person: &'a Person,
    first_name: &'a str,
    surname: &'a str,
}

/// `pedigree.json` of a run (format 5): the core's pedigree, each person with its names.
#[derive(Serialize)]
pub struct Export<'a> {
    format_version: u32,
    persons: Vec<Named<'a>>,
    partnerships: &'a [Couple],
    households: &'a [Home],
}

/// The export of the pedigree `pedigree` of `world`, named from `lists`.
pub fn export<'a>(
    world: &World,
    pedigree: &'a sim_core::pedigree::Pedigree,
    lists: &'a NameLists,
) -> Result<Export<'a>, CliError> {
    let seed = (world.state().world_seed())
        .map(WorldSeed::new)
        .ok_or_else(|| CliError::new("save_rejected", "the world has not started"))?;
    let persons = (pedigree.persons.iter())
        .map(|person| {
            let root = world
                .name_root(person.sim)
                .ok_or_else(|| CliError::new("save_rejected", "a person without a chain"))?;
            let (first_name, surname) = lists.name(seed, person.lineage_key, person.sex, root);
            Ok(Named {
                person,
                first_name,
                surname,
            })
        })
        .collect::<Result<_, CliError>>()?;
    Ok(Export {
        format_version: EXPORT_FORMAT,
        persons,
        partnerships: &pedigree.partnerships,
        households: &pedigree.households,
    })
}
