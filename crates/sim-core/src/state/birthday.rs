//! Dummy domain event of step 2: one `core:birthday` entry per sim and year.

use super::DomainState;
use crate::ids::SimId;
use crate::world::SimCtx;

pub(crate) fn on_birthday(domain: &DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    let _ = (domain, sim, ctx);
}
