//! The hereditary diseases of a sim (genetics specification 3.1 to 3.4, ADR-0017 section 3):
//! which ones it has and from what age, fixed once at its creation from its genotype, its
//! sex and two draws, whatever the switch; and how much of the hazard of a lethal one the
//! switch has let act. The hazard is that of `mortality` in two steps, zero before the
//! onset; the closed periods of the switch add up what they used, the open one runs on.

use alloc::vec::Vec;

use super::{Affliction, Exposure};
use crate::data::{Action, Carrier, Data, Disease, Effect, Locus, Onset};
use crate::data_id::str_key;
use crate::finite::FiniteF64;
use crate::genome::{Genome, Sex};
use crate::lineage::LineageKey;
use crate::mortality::{StepHazard, disease_age, disease_hazard_used};
use crate::phenotype::{Parents, phenotype};
use crate::rng::{DrawKey, Draws, PurposeTag, SubjectKey, WorldSeed, tags};
use crate::time::{SimDuration, SimTime};

/// The address of a draw of a person for a disease: subject the lineage key, occasion the
/// key of the disease code, detail 0 (genetics specification 3.1).
fn key(tag: PurposeTag, lineage_key: LineageKey, disease: &Disease) -> DrawKey {
    DrawKey {
        tag,
        subject: SubjectKey::from(lineage_key),
        occasion: str_key(&disease.code),
        detail: 0,
    }
}

/// The genotype can have the disease (3.4): recessive, every copy the sim has carries the
/// allele (a man's X has one copy); dominant, one copy does.
pub(crate) fn qualifies(genome: &Genome, locus: &Locus) -> bool {
    let copies = genome.copies(locus);
    match locus.action {
        Action::Recessive => !copies.is_empty() && copies.iter().all(|allele| *allele == 1),
        Action::Dominant => copies.contains(&1),
        Action::Additive(_) | Action::Threshold(_) => false,
    }
}

fn finite(x: f64) -> FiniteF64 {
    FiniteF64::try_new(x).unwrap_or_else(|_| panic!("invariant disease.finite"))
}

/// The afflictions of a sim created at `origin` (its birth, the start for a founder): one
/// per disease it has, in the order of the data. A lethal one gets an exposure that runs
/// from `origin` if the switch is on. `parents` are the lineage keys of its biological
/// parents in the run, for the phenotype of a trait of liability.
pub(crate) fn afflictions(
    data: &Data,
    seeds: super::Seeds,
    (lineage_key, parents): (LineageKey, Option<Parents>),
    sex: Sex,
    genome: &Genome,
    switch_on: bool,
    origin: SimTime,
) -> Vec<Affliction> {
    let draws = Draws::new(WorldSeed::new(seeds.life));
    let individual = Draws::new(WorldSeed::new(seeds.individual));
    let shared = Draws::new(WorldSeed::new(seeds.shared));
    let mut held = Vec::new();
    for disease in data.diseases() {
        let affected = match &disease.carrier {
            Carrier::Locus {
                locus,
                penetrance_female,
                penetrance_male,
            } => {
                let locus = data
                    .locus(locus)
                    .unwrap_or_else(|| panic!("invariant disease.locus"));
                let penetrance = match sex {
                    Sex::Female => penetrance_female,
                    Sex::Male => penetrance_male,
                };
                qualifies(genome, locus)
                    && draws.bernoulli(
                        key(tags::DISEASE_PENETRANCE, lineage_key, disease),
                        penetrance.value.get(),
                    )
            }
            // The phenotype of the trait of liability above the threshold (3.6, ADR-0020
            // section 2); its weights are equal in both ages, so the age does not matter.
            Carrier::Threshold {
                liability: Some(trait_id),
                ..
            } => {
                let threshold = data
                    .threshold(&disease.code)
                    .unwrap_or_else(|| panic!("invariant disease.threshold"));
                let liability = phenotype(
                    crate::phenotype::Environment {
                        individual: &individual,
                        shared: &shared,
                        neutral: false,
                    },
                    data,
                    genome,
                    lineage_key,
                    parents,
                    trait_id,
                    SimDuration::ZERO,
                )
                .unwrap_or_else(|| panic!("invariant disease.liability"));
                liability > threshold
            }
            // Without a trait of liability nobody has it (11.1).
            Carrier::Threshold {
                liability: None, ..
            } => false,
        };
        if !affected {
            continue;
        }
        let onset = match &disease.onset {
            Onset::Fixed { age } => age.value.get(),
            Onset::Normal {
                mean,
                sd,
                low,
                high,
            } => draws
                .normal(
                    key(tags::DISEASE_ONSET, lineage_key, disease),
                    mean.value.get(),
                    sd.value.get(),
                )
                .clamp(low.value.get(), high.value.get()),
        };
        held.push(Affliction {
            disease: str_key(&disease.code),
            onset: finite(onset),
            exposure: hazard(disease).map(|_| Exposure {
                used: finite(0.0),
                since: switch_on.then_some(origin),
            }),
        });
    }
    held
}

/// The hazard of a lethal disease in two steps; `None` for a visible one. A constant
/// hazard is the same step on both sides of the age 0.
pub(crate) fn hazard(disease: &Disease) -> Option<StepHazard> {
    match &disease.effect {
        Effect::Death { per_year } => Some(StepHazard {
            before: per_year.value.get(),
            change_age: 0.0,
            after: per_year.value.get(),
        }),
        Effect::DeathInTwoSteps {
            before,
            change_age,
            after,
        } => Some(StepHazard {
            before: before.value.get(),
            change_age: change_age.value.get(),
            after: after.value.get(),
        }),
        Effect::Visible => None,
    }
}

/// The disease of the data with this key.
pub(crate) fn of_key(data: &Data, key: u64) -> Option<&Disease> {
    data.diseases()
        .iter()
        .find(|disease| str_key(&disease.code) == key)
}

/// The larger of two ages (`f64::max` is not used in the core, ADR-0003).
fn later(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// The age of a sim born `born` at `at`, in years.
pub(crate) fn age(born: SimTime, at: SimTime) -> f64 {
    at.checked_since(born)
        .unwrap_or_else(|| panic!("invariant disease.age"))
        .as_years()
}

/// What a period of the switch from `from` to `to` uses up: the hazard from the onset on,
/// nothing before it.
pub(crate) fn period_used(
    hazard: StepHazard,
    onset: f64,
    born: SimTime,
    from: SimTime,
    to: SimTime,
) -> f64 {
    let start = later(age(born, from), onset);
    let end = age(born, to);
    if end > start {
        disease_hazard_used(hazard, start, end)
    } else {
        0.0
    }
}

/// The budget of a person for a disease: the same address as every cause of death
/// (`DEATH_BUDGET`, occasion the key of the code).
pub(crate) fn budget(
    seed: u64,
    (lineage_key, detail): (LineageKey, u64),
    disease: &Disease,
) -> f64 {
    let key = super::death::budget_key(lineage_key, str_key(&disease.code), detail);
    Draws::new(WorldSeed::new(seed)).exponential(key, 1.0)
}

/// When a sim born `born` dies of a lethal affliction if nothing else comes first: while
/// its exposure runs, from the later of its start and the onset with what the closed
/// periods left of the budget, counted in years from the second the period began and
/// rounded up to the second; never while the switch is off, never if no hazard is left
/// for the rest, never beyond the clock.
pub(crate) fn due(
    seed: u64,
    born: SimTime,
    person: (LineageKey, u64),
    affliction: &Affliction,
    disease: &Disease,
) -> Option<SimTime> {
    let hazard = hazard(disease)?;
    let exposure = affliction.exposure?;
    let since = exposure.since?;
    let running = age(born, since);
    let start = later(running, affliction.onset.get());
    let left = (budget(seed, person, disease) - exposure.used.get()).clamp(0.0, f64::MAX);
    // A budget the closed periods used up kills in the second the switch goes on again,
    // whatever hazard the age has then (7.1: the budget is spent). A period used something
    // only after the onset, so the onset lies behind.
    if exposure.used.get() > 0.0 && left <= 0.0 {
        return Some(since);
    }
    let at = disease_age(start, left, hazard)?;
    // From the second the period began, so that no round trip through the age moves it:
    // a budget used up dies in the second the switch goes on again.
    since.checked_add(SimDuration::from_years_ceil(at - running)?)
}

/// The switch went off at `now`: the open period of a lethal affliction closes and adds
/// what it used.
pub(crate) fn close(affliction: &mut Affliction, disease: &Disease, born: SimTime, now: SimTime) {
    let (Some(hazard), Some(exposure)) = (hazard(disease), affliction.exposure.as_mut()) else {
        return;
    };
    let Some(since) = exposure.since else {
        panic!("invariant exposure.switch");
    };
    let used = period_used(hazard, affliction.onset.get(), born, since, now);
    exposure.used = finite(exposure.used.get() + used);
    exposure.since = None;
}
