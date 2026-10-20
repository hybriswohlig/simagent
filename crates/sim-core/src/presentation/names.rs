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
            let Some(mut father) = pool.and_then(|origin| origin.father) else {
                return Some(key);
            };
            let records = self.data().pool();
            while let Some(next) = records
                .and_then(|pool| pool.record(father))
                .and_then(|record| record.father)
            {
                father = next;
            }
            return Some(father);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(hex: &str) -> LineageKey {
        serde_json::from_value(serde_json::Value::from(hex)).expect("a key")
    }

    fn len(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).expect("not zero")
    }

    /// The draws of `tools/rng_reference.py` (`uniform_below` of the first word at the
    /// address seed, tag, key, occasion, 0) for two keys and two seeds.
    #[test]
    fn the_indices_are_the_references_draws() {
        let cases = [
            (1, "4ee9ea8961efbe47", 102, 2, 272),
            (1, "3d147d76b5f097ff", 77, 197, 181),
            (0xa001, "4ee9ea8961efbe47", 29, 0, 152),
            (0xa001, "3d147d76b5f097ff", 128, 92, 48),
        ];
        for (seed, hex, female, male, surname) in cases {
            let seed = WorldSeed::new(seed);
            let k = key(hex);
            assert_eq!(
                first_name_index(seed, k, Sex::Female, len(200)),
                female,
                "{hex}"
            );
            assert_eq!(
                first_name_index(seed, k, Sex::Male, len(200)),
                male,
                "{hex}"
            );
            assert_eq!(surname_index(seed, k, len(300)), surname, "{hex}");
        }
        // A list of one has one name.
        assert_eq!(
            surname_index(WorldSeed::new(1), key("3d147d76b5f097ff"), len(1)),
            0
        );
    }
}
