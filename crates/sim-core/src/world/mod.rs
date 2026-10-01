//! The world (ADR-0004): private state and one way in. `submit` applies an input at an
//! event boundary and records it in the input log. Nothing else changes state.

mod ctx;
mod event;

pub(crate) use ctx::SimCtx;
pub use event::EventPayload;

use alloc::vec::Vec;

use crate::ids::{Counter, InputSeq, SimId};
use crate::input::{Input, InputRecord, InputRejected, InputStamp};
use crate::log::{Cause, LogEntry, PlanRef};
use crate::state::{self, DomainState};
use crate::time::{Ctx, RunStats, Scheduler, SimTime};
use ctx::{IdCounters, LogBuffer};

/// The input log: the only replay source (ADR-0004 section 6). It lives outside the
/// handler state, so no handler can reach it.
struct InputLog {
    next_seq: Counter,
    records: Vec<InputRecord>,
}

pub struct World {
    sched: Scheduler<EventPayload, PlanRef>,
    state: WorldState,
    inputs: InputLog,
}

/// The handler side of the scheduler: the domain state plus the counters and the log
/// buffer that `SimCtx` hands to domain code. Read-only from outside.
pub struct WorldState {
    domain: DomainState,
    ids: IdCounters,
    log: LogBuffer,
}

impl Default for World {
    fn default() -> World {
        World::new()
    }
}

impl World {
    pub fn new() -> World {
        World {
            sched: Scheduler::new(),
            state: WorldState {
                domain: DomainState::new(),
                ids: IdCounters::new(),
                log: LogBuffer::new(),
            },
            inputs: InputLog {
                next_seq: Counter::new(),
                records: Vec::new(),
            },
        }
    }

    /// The last closed second (ADR-0002); `-1` before the first advance.
    pub fn closed_through(&self) -> SimTime {
        self.sched.closed_through()
    }

    /// The only way in from outside (ADR-0004 section 4). A rejected input changes
    /// nothing and takes no position in the input log.
    pub fn submit(&mut self, input: Input) -> Result<InputStamp, InputRejected> {
        let t = self.sched.closed_through();
        check(&self.state.domain, t, &input)?;
        let seq = self.inputs.next_seq.allocate_input();
        let mut ctx = self.sched.input_ctx();
        self.state.apply(&input, seq, &mut ctx);
        self.inputs.records.push(InputRecord { t, seq, input });
        Ok(InputStamp { t, seq })
    }

    /// Hands the buffered domain log over; the core never reads it (ADR-0004 section 6).
    pub fn take_log(&mut self) -> Vec<LogEntry> {
        self.state.log.take()
    }

    pub fn input_log(&self) -> &[InputRecord] {
        &self.inputs.records
    }

    pub fn state(&self) -> &WorldState {
        &self.state
    }

    pub fn stats(&self) -> &RunStats {
        self.sched.stats()
    }

    pub fn pending_events(&self) -> usize {
        self.sched.pending_len()
    }
}

/// Pure function of state and input (ADR-0004 section 4).
fn check(
    domain: &DomainState,
    closed_through: SimTime,
    input: &Input,
) -> Result<(), InputRejected> {
    match input {
        Input::Start(start) => state::start::check_start(domain, closed_through, start),
    }
}

impl WorldState {
    fn apply(&mut self, input: &Input, seq: InputSeq, ctx: &mut Ctx<'_, EventPayload, PlanRef>) {
        let mut ctx = SimCtx::new(ctx, &mut self.ids, &mut self.log, Cause::Input(seq));
        match input {
            Input::Start(start) => state::start::apply_start(&mut self.domain, start, &mut ctx),
        }
    }

    /// `None` until `Start` was applied.
    pub fn world_seed(&self) -> Option<u64> {
        self.domain.world_seed
    }

    pub fn living(&self) -> u64 {
        u64::try_from(self.domain.sims.len())
            .unwrap_or_else(|_| panic!("invariant world.count_range"))
    }

    pub fn sim_ids_allocated(&self) -> u64 {
        self.ids.sims_allocated()
    }

    /// Birth time of a living sim.
    pub fn born(&self, id: SimId) -> Option<SimTime> {
        self.domain.sims.get(&id).map(|sim| sim.born)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Founder, RejectCode, Start};
    use crate::time::{SchedulerSnapshot, YEAR};

    fn start(born: &[i64]) -> Input {
        Input::Start(Start {
            world_seed: 7,
            founder_tag: crate::data_id::DataId::new("test:world").expect("valid"),
            founders: born
                .iter()
                .map(|&secs| Founder {
                    born: SimTime::from_secs(secs),
                })
                .collect(),
        })
    }

    /// ADR-0002: the year end series starts at `year_start(1) - 1 s` with period YEAR,
    /// planned by `Start` with `core:started` (entry 1) as its `by`.
    #[test]
    fn start_plans_the_year_end_series() {
        let mut world = World::new();
        world.submit(start(&[-5, -1])).expect("accepted");
        let snapshot = world.sched.snapshot();
        assert_eq!(snapshot.next_seq, 2);
        assert_eq!(snapshot.entries.len(), 1);
        let entry = &snapshot.entries[0];
        assert_eq!(entry.time, SimTime::from_secs(31_449_599));
        assert_eq!(entry.seq, 1);
        assert_eq!(entry.payload, EventPayload::YearEnd);
        assert_eq!(entry.period, Some(YEAR));
        assert_eq!(
            serde_json::to_string(&entry.meta).expect("json"),
            r#"{"by":1,"planned_at":0}"#
        );
    }

    /// A world whose clock already moved cannot start: the year end series could lie
    /// in the past, and an input must never break an invariant (ADR-0004 section 4).
    #[test]
    fn start_after_the_clock_moved_is_rejected() {
        let moved = Scheduler::restore(SchedulerSnapshot {
            closed_through: SimTime::START,
            next_seq: 1,
            entries: Vec::new(),
        })
        .expect("valid snapshot");
        let mut world = World {
            sched: moved,
            ..World::new()
        };
        let rejected = world.submit(start(&[-1])).expect_err("rejected");
        assert_eq!(rejected.code, RejectCode::ClockNotAtStart);
        assert_eq!(world.inputs.next_seq.peek(), 1);
        assert!(world.inputs.records.is_empty());
        assert_eq!(world.stats().contexts, 0);
        assert_eq!(world.state.world_seed(), None);
        assert!(world.take_log().is_empty());
    }

    /// ADR-0004 section 4: a rejected input takes no sequence number.
    #[test]
    fn a_rejected_input_keeps_the_sequence_number() {
        let mut world = World::new();
        world.submit(start(&[0])).expect_err("founder not born");
        assert_eq!(world.inputs.next_seq.peek(), 1);
        world.submit(start(&[-1])).expect("accepted");
        assert_eq!(world.inputs.next_seq.peek(), 2);
        world.submit(start(&[-1])).expect_err("already started");
        assert_eq!(world.inputs.next_seq.peek(), 2);
    }
}
