//! The possible children of a couple, shown before a pairing (Grok's round 44, K1 to K4):
//! four hypothetical children from the core's own meiosis, two daughters and two sons, with
//! their bodies at 25 without environment (`C` = `E` = 0), their looks in words and their
//! traits in steps. Their keys come from the couple and the index alone, so the same couple
//! shows the same four children; nothing is drawn into the run, and they are not the child
//! of the next conception.

use alloc::vec::Vec;

use serde::Serialize;

use crate::body_contract::{
    BodyData, BodyInputs, BodyParams, BodyTraits, bio_age, body_params, mc1r,
};
use crate::data_id::DataId;
use crate::genome::meiosis::Zygote;
use crate::genome::{Genome, Sex};
use crate::ids::SimId;
use crate::input::ArrangePartnership;
use crate::lineage::{GenomeKey, LineageKey};
use crate::phenotype::{Environment, Parents, phenotype_parts};
use crate::rng::{Draws, WorldSeed};
use crate::state::partnership::check_arrange;
use crate::time::{SimDuration, SimTime, YEAR};
use crate::world::World;

use super::card::facts;
use super::looks::{Looks, looks_of};
use super::traits::{Traits, beauty, traits_of};

/// A possible child: its sex, its body at 25 without environment, its looks and traits.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PreviewChild {
    pub sex: Sex,
    pub body: BodyParams,
    pub looks: Looks,
    pub traits: Traits,
}

fn years(n: i64) -> SimDuration {
    YEAR.checked_mul(n)
        .unwrap_or_else(|| panic!("invariant preview.age"))
}

impl World {
    /// The four possible children of `mother` and `father`; `None` unless an
    /// `ArrangePartnership` of the two would be accepted now (Grok's round 44, K7: only the
    /// candidates of a pairing), or if the data has no body.
    pub fn preview_children(&self, mother: SimId, father: SimId) -> Option<Vec<PreviewChild>> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let now = self.closed_through();
        let pair = ArrangePartnership {
            woman: mother,
            man: father,
        };
        check_arrange(domain, data, now, &pair).ok()?;
        let (_, _, her, her_key) = facts(domain, now, mother)?;
        let (_, _, his, his_key) = facts(domain, now, father)?;
        let body = BodyData::new(data).ok()?;
        let draws = Draws::new(WorldSeed::new(domain.world_seed?));
        let none = Draws::new(WorldSeed::new(0));
        let neutral = Environment {
            individual: &none,
            shared: &none,
            neutral: true,
        };
        let parents = Some(Parents {
            mother: her_key,
            father: his_key,
        });
        let locus = |name: DataId| data.genome_map().loci.iter().find(|l| l.id == name);
        let dose = |genome: &Genome, name: DataId| locus(name).map_or(0, |l| genome.dose(l));
        let children = (0..4_u64).map(|index| {
            let key = LineageKey::preview(her_key, his_key, index);
            let sex = if index < 2 { Sex::Female } else { Sex::Male };
            let zygote = Zygote::with_sex(&draws, data, GenomeKey::of_zygote(key), her, his, sex);
            let genome = zygote.genome;
            let z = |t, age: SimDuration| {
                let t = DataId::new(t).unwrap_or_else(|_| panic!("invariant preview.trait_id"));
                phenotype_parts(neutral, data, &genome, key, parents, &t, age).map(|p| p.z())
            };
            let age = years(25);
            let traits_z = BodyTraits::of_person(&body, neutral, &genome, key, parents);
            let bio = bio_age(data, sex, &genome, age);
            let inputs = BodyInputs::with_traits(
                &body,
                &genome,
                traits_z,
                SimTime::from_secs(0),
                age,
                &[],
                bio,
            );
            let shown = body_params(&body, &inputs).params;
            let looks = looks_of(
                &shown,
                dose(&genome, mc1r()),
                z("core:growth", age)?,
                z("core:cognition", age)?,
            );
            let adult = years(100);
            let names = [
                "core:face_01",
                "core:face_02",
                "core:face_03",
                "core:face_04",
                "core:face_05",
                "core:face_06",
                "core:face_07",
                "core:face_08",
                "core:face_09",
                "core:face_10",
                "core:face_11",
                "core:face_12",
            ];
            let mut faces = [0.0; 12];
            for (face, t) in faces.iter_mut().zip(names) {
                *face = z(t, adult)?;
            }
            let typical = z(
                if sex == Sex::Female {
                    "core:estrogen"
                } else {
                    "core:androgen"
                },
                adult,
            )?;
            let traits = traits_of(
                z("core:cognition", adult)?,
                z("core:muscle", adult)?,
                beauty(&faces, typical),
            );
            Some(PreviewChild {
                sex,
                body: shown,
                looks,
                traits,
            })
        });
        children.collect()
    }
}
