//! Birth (ADR-0004 sections 1 and 7, B18): the pregnancy ends after its months; the
//! child enters the world with the next SimId, its parents and the genome fixed at its
//! conception.

use super::{DomainState, Parents, Sim, Slot, conception, death, disease, partnership, start};
use crate::data::Data;
use crate::data_id::Code;
use crate::ids::SimId;
use crate::log::{Cause, Causes, LogPayload};
use crate::trace::TracePayload;
use crate::world::{EventPayload, SimCtx};

/// The pregnancy of `mother` ends: the child is born now. Its creation entry `core:birth`
/// names child, mother and father, with the plan of the conception and the rule
/// `core:gestation` as causes. The child's first birthday comes a year later, its causes
/// of death act from now on, its diseases are fixed now (ADR-0017 section 3). Her count of
/// live births grows by one; if she is in an open partnership, her attempts begin again
/// after the pause (ADR-0017 section 4).
pub(crate) fn on_birth(
    domain: &mut DomainState,
    data: &Data,
    mother: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The index keeps the handle of this birth until her next conception or her death:
    // handles are never used again, so cancelling it then changes nothing.
    let Some(pregnancy) = domain.pregnancies.remove(&mother) else {
        panic!("invariant birth.without_pregnancy: sim {}", mother.get());
    };
    let switch_on = domain
        .world_params
        .as_ref()
        .unwrap_or_else(|| panic!("invariant birth.before_start"))
        .hereditary_diseases;
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant birth.before_start"));
    let now = ctx.now();
    // The rule of a gestation, the cause of the birth next to the plan.
    let rule = Code::new("core:gestation").unwrap_or_else(|_| panic!("invariant birth.rule_code"));
    let (child, born) = ctx.create_child(
        LogPayload::Birth {
            lineage_key: pregnancy.lineage_key,
        },
        [mother, pregnancy.father],
        Causes::new(ctx.trigger()).with(Cause::Rule(rule)),
    );
    domain.sims.insert(
        child,
        Sim {
            born: now,
            lineage_key: pregnancy.lineage_key,
            sex: pregnancy.sex,
            genome: pregnancy.genome,
            afflictions: disease::afflictions(
                data,
                seed,
                pregnancy.lineage_key,
                pregnancy.sex,
                &pregnancy.genome,
                switch_on,
                now,
            ),
            conceptions: 0,
            births: 0,
            parents: Some(Parents {
                mother,
                father: pregnancy.father,
                conception: pregnancy.conception,
            }),
        },
    );
    let next = start::first_birthday(now, now);
    let handle = ctx.schedule(next, EventPayload::Birthday { sim: child }, born);
    domain.pending.insert((child, Slot::Birthday), handle);
    ctx.trace(child, || TracePayload::BirthdayPlanned { next });
    death::plan_deaths(domain, data, child, born, ctx);
    let Some(her) = domain.sims.get_mut(&mother) else {
        panic!("invariant unknown_entity_id: sim {}", mother.get());
    };
    her.births = her
        .births
        .checked_add(1)
        .unwrap_or_else(|| panic!("invariant birth.count_range"));
    if partnership::open_partnership(domain, mother).is_some() {
        let pause = data.demography().fertility.pause_months;
        let start = conception::series_after_birth(now, pause);
        conception::plan_attempts(domain, data, mother, born, start, ctx);
    }
}
