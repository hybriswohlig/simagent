//! The soft capacity (ADR-0020 section 6): in the last second of each year, after its
//! births and deaths, households emigrate whole while the living exceed the capacity `K`.
//! With `P` living and `P > K`, each household with a member of 18 or more leaves with the
//! chance `e = min(0.5, P / K − 1)`; all see the same `P`, and their draws do not depend on
//! each other. Its members go to the archive, its open partnership ends, and everything
//! planned for them is cancelled.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, EndedBy, LeftBy, death, household, market, partnership};
use crate::data_id::Code;
use crate::ids::HouseholdId;
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::calendar;
use crate::world::SimCtx;

/// The age from which a member makes its household able to emigrate.
pub(crate) const ADULT_YEARS: i64 = 18;

/// The rule that names the emigration as a cause.
pub(crate) fn rule() -> Code {
    Code::new("core:capacity").unwrap_or_else(|_| panic!("invariant emigration.rule_code"))
}

/// The chance `e` that a household emigrates with `living` sims and the capacity
/// `capacity`; `None` while `living ≤ capacity` (nothing is drawn then).
pub(crate) fn rate(living: u64, capacity: u64) -> Option<f64> {
    if living <= capacity {
        return None;
    }
    let exact = |n: u64| {
        i64::try_from(n)
            .ok()
            .and_then(sim_math::i64_to_f64_exact)
            .unwrap_or_else(|| panic!("invariant emigration.count_range"))
    };
    let e = exact(living) / exact(capacity) - 1.0;
    Some(if e > 0.5 { 0.5 } else { e })
}

/// The draw of a household: subject its smallest lineage key of 18 or more, occasion the
/// calendar year.
fn draw_key(representative: LineageKey, year: u64) -> DrawKey {
    DrawKey {
        tag: tags::EMIGRATION,
        subject: SubjectKey::from(representative),
        occasion: year,
        detail: 0,
    }
}

/// The yearly event of the soft capacity `capacity`.
pub(crate) fn on_emigration(domain: &mut DomainState, capacity: u64, ctx: &mut SimCtx<'_, '_>) {
    let now = ctx.now();
    let living = u64::try_from(domain.sims.len())
        .unwrap_or_else(|_| panic!("invariant emigration.count_range"));
    let Some(e) = rate(living, capacity) else {
        return;
    };
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant emigration.before_start"));
    let draws = Draws::new(WorldSeed::new(seed));
    let year = u64::try_from(calendar(now).year)
        .unwrap_or_else(|_| panic!("invariant emigration.before_start"));
    // Every household decides before any leaves: all see the same `P`.
    let leaving: Vec<(HouseholdId, DrawKey)> = domain
        .members
        .iter()
        .filter_map(|(id, members)| {
            let representative = members
                .iter()
                .filter_map(|sim| domain.sims.get(sim))
                .filter(|person| market::of_age(person.born, now, ADULT_YEARS))
                .map(|person| person.lineage_key)
                .min()?;
            let key = draw_key(representative, year);
            draws.bernoulli(key, e).then_some((*id, key))
        })
        .collect();
    for (id, key) in leaving {
        let members: Vec<_> = domain
            .members
            .get(&id)
            .map(|members| members.iter().copied().collect())
            .unwrap_or_default();
        for sim in members {
            ctx.emit(
                LogPayload::Emigrated { household: id },
                vec![Subject::Sim(sim)],
                Causes::new(ctx.trigger())
                    .with(Cause::Rule(rule()))
                    .with(Cause::Random(DrawRef::from(key))),
            );
            death::archive(domain, sim, LeftBy::Emigration, ctx);
            partnership::end_with(domain, sim, EndedBy::Emigration, ctx);
            household::leave(domain, sim, id, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `e` at the edges, bit for bit as `tools/world_reference.py` gives it
    /// (`emigration_rate`): nothing up to `P = K`, `P / K − 1` above, held at 0.5 from
    /// `P = 1.5 K` on.
    #[test]
    fn the_rate_rises_above_the_capacity_and_is_held_at_one_half() {
        let bits = |living: u64, capacity: u64| rate(living, capacity).map(f64::to_bits);
        assert_eq!(rate(10, 10), None);
        assert_eq!(rate(9, 10), None);
        assert_eq!(bits(11, 10), Some(0x3fb9_9999_9999_99a0));
        assert_eq!(bits(14, 10), Some(0x3fd9_9999_9999_9998));
        assert_eq!(bits(15, 10), Some(0x3fe0_0000_0000_0000));
        assert_eq!(bits(30, 10), Some(0x3fe0_0000_0000_0000));
        assert_eq!(bits(8, 7), Some(0x3fc2_4924_9249_2490));
        assert_eq!(bits(17, 12), Some(0x3fda_aaaa_aaaa_aaac));
        assert_eq!(bits(101, 100), Some(0x3f84_7ae1_47ae_1480));
        assert_eq!(bits(149, 100), Some(0x3fdf_5c28_f5c2_8f5c));
        assert_eq!(bits(2, 1), Some(0x3fe0_0000_0000_0000));
    }
}
