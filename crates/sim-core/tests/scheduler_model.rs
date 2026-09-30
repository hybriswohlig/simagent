//! Model-based test of the scheduler (ADR-0002): random operation sequences run against
//! the real scheduler and against a naive reference (a vector with linear minimum
//! search and its own implementation of the order and the micro-step rules, without
//! `EventKey::cmp`). After every operation both must agree on the firing sequence,
//! outcomes, cancel results, clock, next insertion number and pending queue.

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use sim_core::time::{
    AdvanceOutcome, Ctx, EventClass, EventHandle, EventKind, EventSubject, Fired, Handler, Payload,
    Scheduler, SimDuration, SimTime,
};

/// A test payload. Firing a domain event spawns children (up to depth 3) at `now + delay`
/// and may cancel a known handle.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ev {
    id: u64,
    class: u8,
    subject: u64,
    depth: u8,
    spawn: Vec<(u8, u8)>,
    cancel: Option<u8>,
}

const CLASSES: [EventClass; 4] = [
    EventClass::BIRTH,
    EventClass::GENERAL,
    EventClass::DEATH,
    EventClass::OBSERVER,
];
const MAX_DEPTH: u8 = 3;

impl Payload for Ev {
    fn class(&self) -> EventClass {
        CLASSES[usize::from(self.class)]
    }
    fn subject(&self) -> EventSubject {
        EventSubject::sim(self.subject)
    }
    fn kind(&self) -> EventKind {
        EventKind(u16::from(self.class) + 1)
    }
}

/// Children of a fired domain event: (delay in seconds, payload).
fn children(ev: &Ev) -> Vec<(i64, Ev)> {
    if ev.depth >= MAX_DEPTH || ev.class == 3 {
        return Vec::new();
    }
    ev.spawn
        .iter()
        .enumerate()
        .map(|(i, &(delay, class))| {
            let child = Ev {
                id: ev.id * 8 + u64::try_from(i).expect("small") + 1,
                class,
                subject: ev.subject,
                depth: ev.depth + 1,
                spawn: ev.spawn.clone(),
                cancel: None,
            };
            (i64::from(delay), child)
        })
        .collect()
}

/// One fired event: (second, micro step, insertion number, id, cancel result).
type Fire = (i64, u16, u64, u64, Option<bool>);

/// The real side: handler state shared with the reference by construction.
#[derive(Default)]
struct Real {
    handles: Vec<EventHandle>,
    fired: Vec<Fire>,
}

impl Handler<Ev, ()> for Real {
    type Out = Vec<Fire>;

    fn on_domain(&mut self, ev: Fired<Ev, ()>, ctx: &mut Ctx<'_, Ev, ()>) {
        for (delay, child) in children(&ev.payload) {
            let at = ctx.now() + SimDuration::from_secs(delay);
            self.handles.push(ctx.schedule(at, child, ()));
        }
        let cancelled = match ev.payload.cancel {
            Some(k) if !self.handles.is_empty() => {
                let handle = self.handles[usize::from(k) % self.handles.len()];
                Some(ctx.cancel(handle))
            }
            _ => None,
        };
        self.fired.push((
            ev.key.time.as_secs(),
            ev.key.micro,
            ev.handle.seq(),
            ev.payload.id,
            cancelled,
        ));
    }

    fn on_observer(&self, ev: &Fired<Ev, ()>, _sched: &Scheduler<Ev, ()>, out: &mut Vec<Fire>) {
        out.push((
            ev.key.time.as_secs(),
            ev.key.micro,
            ev.handle.seq(),
            ev.payload.id,
            None,
        ));
    }
}

#[derive(Clone, Debug)]
struct RefEntry {
    time: i64,
    micro: u16,
    seq: u64,
    ev: Ev,
    period: Option<i64>,
}

/// The naive reference scheduler.
struct Reference {
    closed: i64,
    next_seq: u64,
    entries: Vec<RefEntry>,
    handles: Vec<u64>,
    fired: Vec<Fire>,
    observed: Vec<Fire>,
}

/// Class codes and phases of the reference, written down independently.
fn class_code(class: u8) -> u16 {
    [200, 500, 900, 1000][usize::from(class)]
}

fn phase(class: u8) -> u8 {
    u8::from(class == 3)
}

/// Sort key of the reference: (time, phase, micro, class, subject kind, subject, kind, seq).
fn order(e: &RefEntry) -> (i64, u8, u16, u16, u8, u64, u16, u64) {
    (
        e.time,
        phase(e.ev.class),
        e.micro,
        class_code(e.ev.class),
        1,
        e.ev.subject,
        u16::from(e.ev.class) + 1,
        e.seq,
    )
}

impl Reference {
    fn new() -> Self {
        Reference {
            closed: -1,
            next_seq: 1,
            entries: Vec::new(),
            handles: Vec::new(),
            fired: Vec::new(),
            observed: Vec::new(),
        }
    }

    /// Inserts with the micro-step rule; `trigger` is (time, phase, micro) of the firing
    /// event, `None` for inputs.
    fn insert(
        &mut self,
        time: i64,
        ev: Ev,
        period: Option<i64>,
        trigger: Option<(i64, u8, u16)>,
    ) -> u64 {
        let micro = match trigger {
            Some((t, p, m)) if time == t && phase(ev.class) == p => m + 1,
            _ => 0,
        };
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(RefEntry {
            time,
            micro,
            seq,
            ev,
            period,
        });
        seq
    }

    fn cancel(&mut self, seq: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.seq != seq);
        self.entries.len() != before
    }

    fn pending(&self, seq: u64) -> bool {
        self.entries.iter().any(|e| e.seq == seq)
    }

    fn min_index(&self) -> Option<usize> {
        (0..self.entries.len()).min_by_key(|&i| order(&self.entries[i]))
    }

    fn advance(&mut self, limit: i64, stop_after: Option<usize>) -> AdvanceOutcome {
        while let Some(i) = self.min_index().filter(|&i| self.entries[i].time <= limit) {
            let second = self.entries[i].time;
            self.closed = second - 1;
            while let Some(i) = self.min_index().filter(|&i| self.entries[i].time == second) {
                let entry = self.entries.remove(i);
                if let Some(period) = entry.period {
                    self.entries.push(RefEntry {
                        time: entry.time + period,
                        micro: 0,
                        ..entry.clone()
                    });
                }
                if entry.ev.class == 3 {
                    self.observed
                        .push((entry.time, entry.micro, entry.seq, entry.ev.id, None));
                    continue;
                }
                let trigger = Some((entry.time, 0, entry.micro));
                for (delay, child) in children(&entry.ev) {
                    let seq = self.insert(second + delay, child, None, trigger);
                    self.handles.push(seq);
                }
                let cancelled = match entry.ev.cancel {
                    Some(k) if !self.handles.is_empty() => {
                        let seq = self.handles[usize::from(k) % self.handles.len()];
                        Some(self.cancel(seq))
                    }
                    _ => None,
                };
                self.fired
                    .push((entry.time, entry.micro, entry.seq, entry.ev.id, cancelled));
            }
            self.closed = second;
            if stop_after.is_some_and(|n| self.fired.len() >= n) {
                return AdvanceOutcome::Stopped {
                    second: SimTime::from_secs(second),
                };
            }
        }
        self.closed = limit;
        AdvanceOutcome::ReachedLimit
    }

    /// Pending entries as (time, micro, seq, id, period) in the reference order.
    fn projection(&self) -> Vec<(i64, u16, u64, u64, Option<i64>)> {
        let mut sorted = self.entries.clone();
        sorted.sort_by_key(order);
        sorted
            .iter()
            .map(|e| (e.time, e.micro, e.seq, e.ev.id, e.period))
            .collect()
    }
}

#[derive(Clone, Debug)]
enum Op {
    Input { offset: u8, ev: Ev },
    Periodic { offset: u8, period: u8, ev: Ev },
    Cancel { handle: u8 },
    Reschedule { handle: u8, offset: u8 },
    Advance { by: u8 },
    AdvanceWithStop { by: u8, after: u8 },
    SaveLoad,
}

fn ev_strategy() -> impl Strategy<Value = Ev> {
    (
        0u8..4,
        0u64..4,
        0u8..MAX_DEPTH,
        prop::collection::vec((0u8..3, 0u8..4), 0..3),
        prop::option::of(0u8..16),
    )
        .prop_map(|(class, subject, depth, spawn, cancel)| Ev {
            id: 0,
            class,
            subject,
            depth,
            spawn,
            cancel,
        })
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0u8..20, ev_strategy()).prop_map(|(offset, ev)| Op::Input { offset, ev }),
        1 => (0u8..10, 1u8..8, ev_strategy()).prop_map(|(offset, period, ev)| Op::Periodic { offset, period, ev }),
        1 => any::<u8>().prop_map(|handle| Op::Cancel { handle }),
        1 => (any::<u8>(), 0u8..20).prop_map(|(handle, offset)| Op::Reschedule { handle, offset }),
        3 => (0u8..15).prop_map(|by| Op::Advance { by }),
        1 => (0u8..15, 1u8..6).prop_map(|(by, after)| Op::AdvanceWithStop { by, after }),
        1 => Just(Op::SaveLoad),
    ]
}

fn assert_same(
    sched: &Scheduler<Ev, ()>,
    real: &Real,
    observed: &[Fire],
    model: &Reference,
    step: usize,
) {
    assert_eq!(
        real.fired, model.fired,
        "step {step}: domain firing sequence"
    );
    assert_eq!(
        observed,
        model.observed.as_slice(),
        "step {step}: observer sequence"
    );
    assert_eq!(
        sched.closed_through().as_secs(),
        model.closed,
        "step {step}: clock"
    );
    let snapshot = sched.snapshot();
    assert_eq!(
        snapshot.next_seq, model.next_seq,
        "step {step}: next insertion number"
    );
    let pending: Vec<(i64, u16, u64, u64, Option<i64>)> = snapshot
        .entries
        .iter()
        .map(|e| {
            (
                e.time.as_secs(),
                e.micro,
                e.seq,
                e.payload.id,
                e.period.map(SimDuration::as_secs),
            )
        })
        .collect();
    assert_eq!(pending, model.projection(), "step {step}: pending queue");
}

fn run(ops: Vec<Op>) {
    let mut sched: Scheduler<Ev, ()> = Scheduler::new();
    let mut real = Real::default();
    let mut observed: Vec<Fire> = Vec::new();
    let mut model = Reference::new();
    for (step, op) in ops.into_iter().enumerate() {
        let root_id = 1_000_000 * (u64::try_from(step).expect("small") + 1);
        let now = model.closed + 1;
        match op {
            Op::Input { offset, mut ev } => {
                ev.id = root_id;
                let at = now + i64::from(offset);
                real.handles.push(sched.input_ctx().schedule(
                    SimTime::from_secs(at),
                    ev.clone(),
                    (),
                ));
                let seq = model.insert(at, ev, None, None);
                model.handles.push(seq);
            }
            Op::Periodic {
                offset,
                period,
                mut ev,
            } => {
                ev.id = root_id;
                let at = now + i64::from(offset);
                let period = i64::from(period);
                let handle = sched.input_ctx().schedule_periodic(
                    SimTime::from_secs(at),
                    SimDuration::from_secs(period),
                    ev.clone(),
                    (),
                );
                real.handles.push(handle);
                let seq = model.insert(at, ev, Some(period), None);
                model.handles.push(seq);
            }
            Op::Cancel { handle } if !model.handles.is_empty() => {
                let k = usize::from(handle) % model.handles.len();
                let real_result = sched.input_ctx().cancel(real.handles[k]);
                assert_eq!(
                    real_result,
                    model.cancel(model.handles[k]),
                    "step {step}: cancel result"
                );
            }
            Op::Reschedule { handle, offset } if !model.handles.is_empty() => {
                let k = usize::from(handle) % model.handles.len();
                let real_pending = sched.pending_key(real.handles[k]).is_some();
                assert_eq!(
                    real_pending,
                    model.pending(model.handles[k]),
                    "step {step}: pending before reschedule"
                );
                if real_pending {
                    let at = now + i64::from(offset);
                    let new =
                        sched
                            .input_ctx()
                            .reschedule(real.handles[k], SimTime::from_secs(at), ());
                    real.handles.push(new);
                    let old = model.handles[k];
                    let index = model
                        .entries
                        .iter()
                        .position(|e| e.seq == old)
                        .expect("pending");
                    let entry = model.entries.remove(index);
                    let seq = model.insert(at, entry.ev, entry.period, None);
                    model.handles.push(seq);
                }
            }
            Op::Advance { by } => {
                let limit = model.closed + i64::from(by);
                let outcome = sched.advance_until(
                    &mut real,
                    &mut observed,
                    SimTime::from_secs(limit),
                    &mut |_| false,
                );
                assert_eq!(outcome, model.advance(limit, None), "step {step}: outcome");
            }
            Op::AdvanceWithStop { by, after } => {
                let limit = model.closed + i64::from(by);
                let target = real.fired.len() + usize::from(after);
                let outcome = sched.advance_until(
                    &mut real,
                    &mut observed,
                    SimTime::from_secs(limit),
                    &mut |r: &Real| r.fired.len() >= target,
                );
                assert_eq!(
                    outcome,
                    model.advance(limit, Some(target)),
                    "step {step}: outcome with stop"
                );
            }
            Op::SaveLoad => {
                sched = Scheduler::restore(sched.snapshot()).expect("a snapshot always loads");
            }
            Op::Cancel { .. } | Op::Reschedule { .. } => {}
        }
        assert_same(&sched, &real, &observed, &model, step);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1_000,
        rng_seed: RngSeed::Fixed(0x5c4e_d002),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn scheduler_matches_reference_model(ops in prop::collection::vec(op_strategy(), 0..200)) {
        run(ops);
    }
}
