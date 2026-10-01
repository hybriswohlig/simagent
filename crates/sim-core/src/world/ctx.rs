//! The handler context (ADR-0004 section 7): the only way domain code writes log
//! entries, schedules events and gets new IDs. It wraps the scheduler's `Ctx`.

use alloc::vec;
use alloc::vec::Vec;

use super::event::EventPayload;
use crate::ids::{Counter, EventId, SimId};
use crate::log::{Cause, Causes, LogEntry, LogPayload, PlanRef, Subject};
use crate::time::{Ctx, EventHandle, SimDuration, SimTime};

/// One counter per entity kind (ADR-0004 section 1). Only `SimCtx` allocates.
pub(super) struct IdCounters {
    sim: Counter,
}

impl IdCounters {
    pub(super) const fn new() -> IdCounters {
        IdCounters {
            sim: Counter::new(),
        }
    }

    /// The counters of a loaded world.
    pub(super) fn restore(sim: Counter) -> IdCounters {
        IdCounters { sim }
    }

    /// The SimId the next sim gets.
    pub(super) fn next_sim(&self) -> u64 {
        self.sim.peek()
    }

    /// Number of SimIds handed out so far.
    pub(super) fn sims_allocated(&self) -> u64 {
        self.sim.peek() - 1
    }
}

/// The domain log buffer and its EventId counter: pure output. There is no read
/// access, only `take` (ADR-0004 section 6).
pub(super) struct LogBuffer {
    next_event: Counter,
    entries: Vec<LogEntry>,
}

impl LogBuffer {
    pub(super) const fn new() -> LogBuffer {
        LogBuffer {
            next_event: Counter::new(),
            entries: Vec::new(),
        }
    }

    pub(super) fn take(&mut self) -> Vec<LogEntry> {
        core::mem::take(&mut self.entries)
    }

    /// The empty buffer of a loaded world; its first entry gets the saved next ID.
    pub(super) fn restore(next_event: Counter) -> LogBuffer {
        LogBuffer {
            next_event,
            entries: Vec::new(),
        }
    }

    /// True if nothing waits to be taken. A save needs that (ADR-0004 section 6).
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The EventId the next entry gets.
    pub(super) fn next_event(&self) -> u64 {
        self.next_event.peek()
    }

    /// A buffer whose first entry gets the ID `first` (ADR-0004 test 6).
    #[cfg(test)]
    pub(super) fn starting_at(first: u64) -> LogBuffer {
        LogBuffer {
            next_event: Counter::starting_at(first),
            entries: Vec::new(),
        }
    }
}

/// What a domain handler or an applied input may do. `cancel` and `reschedule`
/// (ADR-0004 section 7) arrive with their first user, the death rule.
pub(crate) struct SimCtx<'a, 'q> {
    ctx: &'a mut Ctx<'q, EventPayload, PlanRef>,
    ids: &'a mut IdCounters,
    log: &'a mut LogBuffer,
    trigger: Cause,
}

impl<'a, 'q> SimCtx<'a, 'q> {
    pub(super) fn new(
        ctx: &'a mut Ctx<'q, EventPayload, PlanRef>,
        ids: &'a mut IdCounters,
        log: &'a mut LogBuffer,
        trigger: Cause,
    ) -> SimCtx<'a, 'q> {
        SimCtx {
            ctx,
            ids,
            log,
            trigger,
        }
    }

    /// The first open second.
    pub(crate) fn now(&self) -> SimTime {
        self.ctx.now()
    }

    /// Why this handler runs: the plan of the running event or the applied input.
    pub(crate) fn trigger(&self) -> Cause {
        self.trigger.clone()
    }

    /// Writes one log entry with `t = now`.
    pub(crate) fn emit(
        &mut self,
        payload: LogPayload,
        subjects: Vec<Subject>,
        causes: Causes,
    ) -> EventId {
        let id = self.log.next_event.allocate_event();
        self.log.entries.push(LogEntry {
            id,
            t: self.ctx.now(),
            subjects,
            causes,
            payload,
        });
        id
    }

    /// A new sim: allocates its ID and writes its creation entry, so every allocation
    /// has exactly one creation entry (ADR-0004 section 1).
    #[track_caller]
    pub(crate) fn create_sim(&mut self, payload: LogPayload, causes: Causes) -> (SimId, EventId) {
        let id = self.ids.sim.allocate_sim();
        let entry = self.emit(payload, vec![Subject::Sim(id)], causes);
        (id, entry)
    }

    /// Plans one event; `by` is the entry that gives the reason.
    #[track_caller]
    pub(crate) fn schedule(
        &mut self,
        at: SimTime,
        payload: EventPayload,
        by: EventId,
    ) -> EventHandle {
        let meta = self.plan_ref(by);
        self.ctx.schedule(at, payload, meta)
    }

    /// Plans a periodic series; `by` is the entry that sets it up.
    #[track_caller]
    pub(crate) fn schedule_periodic(
        &mut self,
        first: SimTime,
        period: SimDuration,
        payload: EventPayload,
        by: EventId,
    ) -> EventHandle {
        let meta = self.plan_ref(by);
        self.ctx.schedule_periodic(first, period, payload, meta)
    }

    fn plan_ref(&self, by: EventId) -> PlanRef {
        PlanRef {
            by,
            planned_at: self.ctx.now(),
        }
    }
}
