//! The fork comparison (ADR-0021 section 3 and its addenda of 2026-10-23, 2026-10-24 and
//! 2026-10-28): two pedigrees
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
use crate::state::{disease, onset};
use crate::time::SimTime;

/// The version of the form of the comparison: 3 with the diseases counted per run, the
/// hereditary lives marked and the diseases at rest (ADR-0021, addendum of 2026-10-28).
pub const DIFF_FORMAT: u32 = 3;

/// Two runs compared, `a` and `b` in the order given: the hereditary diseases counted, the
/// keys of only one, by key, and each key in both whose lives differ, by key.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diff {
    pub format_version: u32,
    /// Per disease of the data, in their order, a row once one of its four numbers is above
    /// 0 (Grok's round 41, V1).
    pub summary: Vec<DiseaseCount>,
    pub a_only: Vec<LineageKey>,
    pub b_only: Vec<LineageKey>,
    pub changed: Vec<Changed>,
}

/// A disease of the data in both runs: the living that show it at the pedigree's second, and
/// everybody of the pedigree who died of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiseaseCount {
    pub disease: Code,
    pub a: Count,
    pub b: Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Count {
    pub affected: u64,
    pub died: u64,
}

/// A key whose life differs between the runs: which fields, and the life in each.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Changed {
    pub lineage_key: LineageKey,
    /// The fields that differ, in the order of [`Life`]; never empty.
    pub differs: Vec<Field>,
    /// The diseases differ, or a cause of death is a disease of the data (Grok's round 41,
    /// V1): the page shows these lives first.
    pub hereditary: bool,
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
    Diseases,
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
    /// The hereditary diseases whose onset is reached at the pedigree's second, in the order
    /// of the data (Grok's round 34, E1), whose onset second lies within the clock: those that
    /// show, and those that rest because the switch is off then (round 41, V2). One not
    /// reached is the same in both runs.
    pub diseases: Vec<Onset>,
}

/// A disease with its onset reached, with the calendar year and the whole years of its
/// onset as the card reads them (ADR-0017 section 3), and whether it rests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Onset {
    pub disease: Code,
    pub year: i64,
    pub age_years: i64,
    pub rests: bool,
}

/// A partnership of a person, the other by key.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bond {
    pub partner: LineageKey,
    pub formed: SimTime,
    pub ended: Option<SimTime>,
    pub ended_by: Option<End>,
}

/// The comparison of the runs of the pedigrees `a` and `b`, whose clocks stand at `clocks`,
/// with the diseases of the data in their order.
pub fn diff(a: &Pedigree, b: &Pedigree, clocks: (SimTime, SimTime), diseases: &[Code]) -> Diff {
    let (lives_a, lives_b) = (lives(a, clocks.0), lives(b, clocks.1));
    let of_a_disease =
        |cause: &Option<Code>| cause.as_ref().is_some_and(|code| diseases.contains(code));
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
                hereditary: differs.contains(&Field::Diseases)
                    || of_a_disease(&a.cause_of_death)
                    || of_a_disease(&b.cause_of_death),
                differs,
                a: a.clone(),
                b: b.clone(),
            })
        })
        .collect();
    let summary = (diseases.iter())
        .filter_map(|code| {
            let (a, b) = (count(&lives_a, code), count(&lives_b, code));
            let any = [a.affected, a.died, b.affected, b.died]
                .iter()
                .any(|n| *n > 0);
            any.then(|| DiseaseCount {
                disease: code.clone(),
                a,
                b,
            })
        })
        .collect();
    Diff {
        format_version: DIFF_FORMAT,
        summary,
        a_only: only(&lives_a, &lives_b),
        b_only: only(&lives_b, &lives_a),
        changed,
    }
}

/// The living (neither dead nor gone) that show `code`, and all who died of it.
fn count(lives: &BTreeMap<LineageKey, Life>, code: &Code) -> Count {
    let number =
        |n: usize| u64::try_from(n).unwrap_or_else(|_| panic!("invariant diff.count_range"));
    let shows =
        |life: &&Life| (life.diseases.iter()).any(|onset| !onset.rests && onset.disease == *code);
    let living = |life: &&Life| life.died.is_none() && life.emigrated.is_none();
    Count {
        affected: number(lives.values().filter(living).filter(shows).count()),
        died: number(
            (lives.values())
                .filter(|life| life.cause_of_death.as_ref() == Some(code))
                .count(),
        ),
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
        (Field::Diseases, a.diseases != b.diseases),
    ]
    .into_iter()
    .filter_map(|(field, differs)| differs.then_some(field))
    .collect()
}

/// The life of each person of `pedigree`, whose clock stands at `clock`, by key. A parent is a
/// sim of the pedigree, else the key its pool origin names (a child of a pool pregnancy, an
/// arrival).
fn lives(pedigree: &Pedigree, clock: SimTime) -> BTreeMap<LineageKey, Life> {
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
                diseases: (person.diseases.iter())
                    // An onset second beyond the clock (reached by the years at its last
                    // second, rounded up past it) never comes (ADR-0002).
                    .filter(|shown| onset::onset_second(person.born, shown.onset).is_some())
                    .filter_map(|shown| {
                        // At the second the pedigree shows the person (its death, its
                        // emigration, else the clock, as `World::pedigree`), an onset reached
                        // while the switch is off rests.
                        let second =
                            (person.died.or(person.emigrated)).unwrap_or(clock.max(SimTime::START));
                        let rests =
                            !shown.affected && disease::age(person.born, second) >= shown.onset;
                        let (year, age_years) = onset::since(person.born, shown.onset);
                        (shown.affected || rests).then(|| Onset {
                            disease: shown.disease.clone(),
                            year,
                            age_years,
                            rests,
                        })
                    })
                    .collect(),
            };
            (person.lineage_key, life)
        })
        .collect()
}
