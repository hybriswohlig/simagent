//! The fork comparison (ADR-0021 section 3 and its addendum of 2026-10-23): two pedigrees
//! compared by lineage key, never by sim or partnership (ADR-0001 section 6). Per key in
//! both the fields of a life where they differ, and the keys of only one. A read of two
//! views; it changes no result.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::Serialize;

use crate::data_id::Code;
use crate::ids::SimId;
use crate::lineage::LineageKey;
use crate::pedigree::{End, Pedigree};
use crate::time::SimTime;

/// The version of the form of the comparison.
pub const DIFF_FORMAT: u32 = 1;

/// Two runs compared, `a` and `b` in the order given: the keys of only one, by key, and
/// each key in both whose lives differ, by key.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diff {
    pub format_version: u32,
    pub a_only: Vec<LineageKey>,
    pub b_only: Vec<LineageKey>,
    pub changed: Vec<Changed>,
}

/// A key whose life differs between the runs: which fields, and the life in each.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Changed {
    pub lineage_key: LineageKey,
    /// The fields that differ, in the order of [`Life`]; never empty.
    pub differs: Vec<Field>,
    pub a: Life,
    pub b: Life,
}

/// A field of a [`Life`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Born,
    Died,
    CauseOfDeath,
    Emigrated,
    Partnerships,
    Children,
}

/// What a run says of a person, by keys alone.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Life {
    pub born: SimTime,
    pub died: Option<SimTime>,
    pub cause_of_death: Option<Code>,
    pub emigrated: Option<SimTime>,
    /// By the second they formed, then by the partner's key.
    pub partnerships: Vec<Bond>,
    /// The persons of the pedigree whose mother or father the person is; at most the number
    /// of persons, so the count never saturates.
    pub children: u64,
}

/// A partnership of a person, the other by key.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bond {
    pub partner: LineageKey,
    pub formed: SimTime,
    pub ended: Option<SimTime>,
    pub ended_by: Option<End>,
}

/// The comparison of the runs of the pedigrees `a` and `b`.
pub fn diff(a: &Pedigree, b: &Pedigree) -> Diff {
    let (lives_a, lives_b) = (lives(a), lives(b));
    let only = |one: &BTreeMap<LineageKey, Life>, other: &BTreeMap<LineageKey, Life>| {
        (one.keys())
            .filter(|key| !other.contains_key(key))
            .copied()
            .collect()
    };
    let changed = (lives_a.iter())
        .filter_map(|(key, a)| {
            let b = lives_b.get(key)?;
            let differs = differing(a, b);
            (!differs.is_empty()).then(|| Changed {
                lineage_key: *key,
                differs,
                a: a.clone(),
                b: b.clone(),
            })
        })
        .collect();
    Diff {
        format_version: DIFF_FORMAT,
        a_only: only(&lives_a, &lives_b),
        b_only: only(&lives_b, &lives_a),
        changed,
    }
}

/// The fields in which `a` and `b` differ, in the order of [`Life`].
fn differing(a: &Life, b: &Life) -> Vec<Field> {
    [
        (Field::Born, a.born != b.born),
        (Field::Died, a.died != b.died),
        (Field::CauseOfDeath, a.cause_of_death != b.cause_of_death),
        (Field::Emigrated, a.emigrated != b.emigrated),
        (Field::Partnerships, a.partnerships != b.partnerships),
        (Field::Children, a.children != b.children),
    ]
    .into_iter()
    .filter_map(|(field, differs)| differs.then_some(field))
    .collect()
}

/// The life of each person of `pedigree`, by key. A parent is a sim of the pedigree, else
/// the key its pool origin names (a child of a pool pregnancy, an arrival).
fn lives(pedigree: &Pedigree) -> BTreeMap<LineageKey, Life> {
    let key_of: BTreeMap<SimId, LineageKey> = (pedigree.persons.iter())
        .map(|person| (person.sim, person.lineage_key))
        .collect();
    let key = |sim: SimId| {
        key_of
            .get(&sim)
            .copied()
            .unwrap_or_else(|| panic!("invariant diff.sim_of_the_pedigree"))
    };
    let mut children: BTreeMap<LineageKey, u64> = BTreeMap::new();
    for person in &pedigree.persons {
        let pool = person.pool.as_ref();
        let mother = (person.mother.map(key)).or(pool.and_then(|from| from.mother_key));
        let father = (person.father.map(key)).or(pool.and_then(|from| from.father_key));
        for parent in [mother, father].into_iter().flatten() {
            let count = children.entry(parent).or_insert(0);
            *count = count.saturating_add(1);
        }
    }
    let mut bonds: BTreeMap<LineageKey, Vec<Bond>> = BTreeMap::new();
    for couple in &pedigree.partnerships {
        let (woman, man) = (key(couple.woman), key(couple.man));
        for (person, partner) in [(woman, man), (man, woman)] {
            bonds.entry(person).or_default().push(Bond {
                partner,
                formed: couple.formed,
                ended: couple.ended,
                ended_by: couple.ended_by,
            });
        }
    }
    (pedigree.persons.iter())
        .map(|person| {
            let mut partnerships = bonds.remove(&person.lineage_key).unwrap_or_default();
            partnerships.sort_by_key(|bond| (bond.formed, bond.partner));
            let life = Life {
                born: person.born,
                died: person.died,
                cause_of_death: person.cause_of_death.clone(),
                emigrated: person.emigrated,
                partnerships,
                children: children.get(&person.lineage_key).copied().unwrap_or(0),
            };
            (person.lineage_key, life)
        })
        .collect()
}
