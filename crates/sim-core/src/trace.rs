//! The trace per sim (ADR-0004 section 6): off by default; what is traced is neither an
//! input nor state and is not saved. Trace entries have no EventId and go to their own
//! buffer. A trace entry is built in a closure that never runs while the trace is off
//! for its sim, so tracing cannot change a run.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::time::SimTime;

/// Which sims are traced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TraceFilter {
    #[default]
    Off,
    All,
    Sims(BTreeSet<SimId>),
}

impl TraceFilter {
    pub fn includes(&self, sim: SimId) -> bool {
        let _ = sim;
        false
    }
}

/// What a trace entry says. The variant name is its stable code; new variants only go
/// to the end (`tests/golden/log_codes.txt`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TracePayload {
    /// Dummy trace of step 2: the next birthday of the sim was planned for `next`.
    #[serde(rename = "core:birthday_planned")]
    BirthdayPlanned { next: SimTime },
}

/// One entry of the trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEntry {
    pub t: SimTime,
    pub sim: SimId,
    pub payload: TracePayload,
}

/// The filter and the buffer. Neither is state: not hashed, not saved.
#[derive(Default)]
pub(crate) struct TraceBuffer {
    filter: TraceFilter,
    entries: Vec<TraceEntry>,
}

impl TraceBuffer {
    pub(crate) fn set_filter(&mut self, filter: TraceFilter) {
        self.filter = filter;
    }

    pub(crate) fn take(&mut self) -> Vec<TraceEntry> {
        core::mem::take(&mut self.entries)
    }

    /// Records what `make` returns if `sim` is traced; otherwise `make` never runs.
    pub(crate) fn record(&mut self, t: SimTime, sim: SimId, make: impl FnOnce() -> TracePayload) {
        let _ = (t, sim, make, &mut self.entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn planned(next: i64) -> TracePayload {
        TracePayload::BirthdayPlanned {
            next: SimTime::from_secs(next),
        }
    }

    #[test]
    fn the_filter_selects_sims() {
        assert!(!TraceFilter::Off.includes(sim(1)));
        assert!(!TraceFilter::default().includes(sim(1)));
        assert!(TraceFilter::All.includes(sim(1)));
        let some = TraceFilter::Sims([sim(2), sim(5)].into_iter().collect());
        assert!(some.includes(sim(2)) && some.includes(sim(5)));
        assert!(!some.includes(sim(1)) && !some.includes(sim(3)));
        assert!(!TraceFilter::Sims(BTreeSet::new()).includes(sim(1)));
    }

    /// ADR-0004 section 6: the closure never runs for a sim that is not traced.
    #[test]
    fn an_untraced_sim_runs_no_closure() {
        let mut buffer = TraceBuffer::default();
        buffer.record(SimTime::START, sim(1), || panic!("the trace is off"));
        buffer.set_filter(TraceFilter::Sims([sim(2)].into_iter().collect()));
        buffer.record(SimTime::START, sim(1), || panic!("sim 1 is not traced"));
        assert!(buffer.take().is_empty());
    }

    #[test]
    fn a_traced_sim_gets_its_entries_in_order() {
        let mut buffer = TraceBuffer::default();
        buffer.set_filter(TraceFilter::All);
        buffer.record(SimTime::from_secs(5), sim(2), || planned(7));
        buffer.record(SimTime::from_secs(6), sim(1), || planned(8));
        let entries = buffer.take();
        assert_eq!(
            entries,
            [
                TraceEntry {
                    t: SimTime::from_secs(5),
                    sim: sim(2),
                    payload: planned(7),
                },
                TraceEntry {
                    t: SimTime::from_secs(6),
                    sim: sim(1),
                    payload: planned(8),
                },
            ]
        );
        assert!(buffer.take().is_empty(), "taking hands the buffer over");
        // The filter stays after a take.
        buffer.record(SimTime::from_secs(9), sim(3), || planned(10));
        assert_eq!(buffer.take().len(), 1);
    }
}
