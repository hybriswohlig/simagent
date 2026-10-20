//! Names (ADR-0021 section 3). The core holds no text, so it gives indices into lists it
//! does not know: a first name drawn over the person's lineage key and sex, a surname drawn
//! over the root of the paternal chain. Read only: no draw changes the state, nothing goes
//! into a hash or a save, and a fork names everybody as its parent does.

use core::num::NonZeroU64;

use crate::genome::Sex;
use crate::ids::SimId;
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::world::World;

/// The index of a person's first name in a list of `len` names of its sex.
pub fn first_name_index(seed: WorldSeed, key: LineageKey, sex: Sex, len: NonZeroU64) -> u64 {
    let occasion = match sex {
        Sex::Female => 0,
        Sex::Male => 1,
    };
    let draw = DrawKey {
        tag: tags::FIRST_NAME,
        subject: SubjectKey::from(key),
        occasion,
        detail: 0,
    };
    Draws::new(seed).uniform_below(draw, len.get())
}

/// The index of the surname of a paternal chain with the root `root` in a list of `len`.
pub fn surname_index(seed: WorldSeed, root: LineageKey, len: NonZeroU64) -> u64 {
    let draw = DrawKey {
        tag: tags::SURNAME,
        subject: SubjectKey::from(root),
        occasion: 0,
        detail: 0,
    };
    Draws::new(seed).uniform_below(draw, len.get())
}

impl World {
    /// The root of the paternal chain of `sim`, living or archived: its father in the run,
    /// his father, and so on; from a person of the pool without a father in the run, the
    /// fathers in the pool from record to record; the root is the first person without a
    /// known father (a father's key without a record is that person). `None` for an
    /// unknown sim.
    pub fn name_root(&self, sim: SimId) -> Option<LineageKey> {
        let domain = self.state().domain();
        let mut current = sim;
        loop {
            let (key, parents, pool) = match domain.sims.get(&current) {
                Some(person) => (person.lineage_key, person.parents, person.pool.as_ref()),
                None => {
                    let gone = domain.archive.get(&current)?;
                    (gone.lineage_key, gone.parents, gone.pool.as_ref())
                }
            };
            if let Some(parents) = parents {
                current = parents.father;
                continue;
            }
            let Some(father) = pool.and_then(|origin| origin.father) else {
                return Some(key);
            };
            let records = self.data().pool();
            let bound = records.map_or(0, |pool| pool.pool().ancestors.len());
            let father_of = |key| {
                records
                    .and_then(|pool| pool.record(key))
                    .and_then(|record| record.father)
            };
            return Some(chain_root(father, father_of, bound));
        }
    }
}

/// The end of a chain of fathers from `start`: the first key `father_of` names no father
/// for. At most `bound` steps (the number of records): a chain that runs longer can only
/// be a cycle in the pool's records, and ends where the bound stops it.
pub fn chain_root(
    start: LineageKey,
    father_of: impl Fn(LineageKey) -> Option<LineageKey>,
    bound: usize,
) -> LineageKey {
    let mut key = start;
    for _ in 0..bound {
        match father_of(key) {
            Some(father) => key = father,
            None => break,
        }
    }
    key
}
