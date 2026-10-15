//! The one event scheduler (ADR-0002). Events are ordered totally by (time, phase,
//! micro step, class, subject, kind, insertion number); time only advances through
//! `advance_until`. The queue holds data only: a payload `P` and companion data `M`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cmp::Ordering;

use super::{SimDuration, SimTime};

/// Highest micro step inside one (second, phase); beyond it is `scheduler.cascade_limit`.
pub const MAX_MICRO_STEP: u16 = 64;

/// Event class; fixes the phase and the order within a micro step. Codes are never
/// renumbered (golden file `tests/golden/event_codes.txt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventClass(u16);

impl EventClass {
    /// Immigration runs first in its second, before births, the market, conceptions and
    /// deaths (ADR-0020 section 5): the immigrants take part in them.
    pub const IMMIGRATION: EventClass = EventClass(100);
    pub const BIRTH: EventClass = EventClass(200);
    pub const GENERAL: EventClass = EventClass(500);
    pub const DEATH: EventClass = EventClass(900);
    /// The soft capacity runs after the births and deaths of its second (ADR-0020
    /// section 6).
    pub const EMIGRATION: EventClass = EventClass(950);
    /// Observers such as the census run after every domain event of their second.
    pub const OBSERVER: EventClass = EventClass(1000);

    pub const fn code(self) -> u16 {
        self.0
    }

    /// 0 for domain events, 1 for observers.
    pub const fn phase(self) -> u8 {
        if self.0 >= Self::OBSERVER.0 { 1 } else { 0 }
    }
}

/// Who an event is about: a kind code and an ID (ADR-0004). Not the random-draw
/// `SubjectKey` of ADR-0001.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventSubject {
    kind: u8,
    id: u64,
}

impl EventSubject {
    pub const WORLD: EventSubject = EventSubject { kind: 0, id: 0 };

    pub const fn sim(id: u64) -> EventSubject {
        EventSubject { kind: 1, id }
    }

    pub const fn partnership(id: u64) -> EventSubject {
        EventSubject { kind: 2, id }
    }

    pub const fn household(id: u64) -> EventSubject {
        EventSubject { kind: 3, id }
    }

    pub const fn kind_code(self) -> u8 {
        self.kind
    }
}

/// Fixed code per payload variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventKind(pub u16);

/// What the scheduler needs to know about a payload to order it.
pub trait Payload: Clone {
    fn class(&self) -> EventClass;
    fn subject(&self) -> EventSubject;
    fn kind(&self) -> EventKind;
}

/// Total order of events. Companion data never takes part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventKey {
    pub time: SimTime,
    pub phase: u8,
    pub micro: u16,
    pub class: EventClass,
    pub subject: EventSubject,
    pub kind: EventKind,
    pub seq: u64,
}

impl EventKey {
    fn raw(&self) -> (i64, u8, u16, u16, u8, u64, u16, u64) {
        (
            self.time.as_secs(),
            self.phase,
            self.micro,
            self.class.0,
            self.subject.kind,
            self.subject.id,
            self.kind.0,
            self.seq,
        )
    }
}

impl Ord for EventKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw().cmp(&other.raw())
    }
}

impl PartialOrd for EventKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Handle of a pending event or of a whole periodic series. The insertion number is
/// global and never reused, so a stale handle never hits a newer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventHandle(u64);

impl EventHandle {
    pub const fn seq(self) -> u64 {
        self.0
    }

    /// The handle of the pending event with this insertion number, for an index that a
    /// load builds again from the queue.
    pub(crate) const fn from_seq(seq: u64) -> EventHandle {
        EventHandle(seq)
    }
}

/// An event taken from the queue.
#[derive(Clone, Debug)]
pub struct Fired<P, M> {
    pub key: EventKey,
    pub handle: EventHandle,
    pub payload: P,
    pub meta: M,
}

/// The world side of the scheduler: domain handlers change state and may schedule;
/// observers only read and write to `Out`, which is output, never state.
pub trait Handler<P, M> {
    type Out;
    fn on_domain(&mut self, ev: Fired<P, M>, ctx: &mut Ctx<'_, P, M>);
    fn on_observer(&self, ev: &Fired<P, M>, sched: &Scheduler<P, M>, out: &mut Self::Out);
}

/// Result of `advance_until`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceOutcome {
    ReachedLimit,
    Stopped { second: SimTime },
}

/// Deterministic diagnostics; neither hashed nor saved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    pub domain_calls: u64,
    pub observer_calls: u64,
    pub contexts: u64,
    events: BTreeMap<(u16, u16), u64>,
}

impl RunStats {
    /// Events fired with this class and kind.
    pub fn events_of(&self, class: EventClass, kind: EventKind) -> u64 {
        self.events.get(&(class.0, kind.0)).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
struct Entry<P, M> {
    payload: P,
    period: Option<SimDuration>,
    meta: M,
}

/// The queue as plain data, for saving and hashing (ADR-0002, ADR-0003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerSnapshot<P, M> {
    pub closed_through: SimTime,
    pub next_seq: u64,
    pub entries: Vec<SnapshotEntry<P, M>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotEntry<P, M> {
    pub time: SimTime,
    pub micro: u16,
    pub seq: u64,
    pub payload: P,
    pub period: Option<SimDuration>,
    pub meta: M,
}

/// Why a saved queue was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    NotAscending,
    DuplicateSeq,
    SeqNotBelowNext,
    NotInFuture,
    MicroNotZero,
    PeriodTooShort,
}

/// The event queue and the clock.
#[derive(Clone, Debug)]
pub struct Scheduler<P, M> {
    closed_through: SimTime,
    next_seq: u64,
    queue: BTreeMap<EventKey, Entry<P, M>>,
    index: BTreeMap<u64, EventKey>,
    stats: RunStats,
}

impl<P: Payload, M: Clone> Default for Scheduler<P, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Payload, M: Clone> Scheduler<P, M> {
    pub fn new() -> Self {
        Scheduler {
            closed_through: SimTime::START - SimDuration::from_secs(1),
            next_seq: 1,
            queue: BTreeMap::new(),
            index: BTreeMap::new(),
            stats: RunStats::default(),
        }
    }

    /// The last second whose events have all run.
    pub fn closed_through(&self) -> SimTime {
        self.closed_through
    }

    pub fn pending_len(&self) -> usize {
        self.queue.len()
    }

    pub fn pending_key(&self, handle: EventHandle) -> Option<EventKey> {
        self.index.get(&handle.0).copied()
    }

    pub fn pending_handle_time(&self, handle: EventHandle) -> Option<SimTime> {
        self.pending_key(handle).map(|key| key.time)
    }

    pub fn stats(&self) -> &RunStats {
        &self.stats
    }

    /// Runs every event up to and including `limit` in key order; the only function
    /// that moves the clock. Afterwards `closed_through = limit`, unless `stop` (checked
    /// after each second with events, never inside a second) ends the run early.
    #[track_caller]
    pub fn advance_until<H: Handler<P, M>>(
        &mut self,
        h: &mut H,
        out: &mut H::Out,
        limit: SimTime,
        stop: &mut dyn FnMut(&H) -> bool,
    ) -> AdvanceOutcome {
        if limit < self.closed_through {
            panic!("invariant scheduler.clock_backwards");
        }
        while let Some(second) = self.next_time().filter(|&time| time <= limit) {
            // Entering the second: everything before it is closed.
            self.closed_through = second - SimDuration::from_secs(1);
            while let Some(fired) = self.pop_next(second) {
                self.stats.count(fired.key.class, fired.key.kind);
                if fired.key.phase == 0 {
                    self.stats.domain_calls += 1;
                    self.stats.contexts += 1;
                    let current = Some(fired.key);
                    let mut ctx = Ctx {
                        sched: self,
                        now: second,
                        current,
                    };
                    h.on_domain(fired, &mut ctx);
                } else {
                    self.stats.observer_calls += 1;
                    h.on_observer(&fired, self, out);
                }
            }
            self.closed_through = second;
            if stop(h) {
                return AdvanceOutcome::Stopped { second };
            }
        }
        self.closed_through = limit;
        AdvanceOutcome::ReachedLimit
    }

    fn next_time(&self) -> Option<SimTime> {
        self.queue.first_key_value().map(|(key, _)| key.time)
    }

    /// Takes the next event of `second`. A periodic series gets its next occurrence
    /// (same insertion number, same companion data) before its handler runs.
    fn pop_next(&mut self, second: SimTime) -> Option<Fired<P, M>> {
        if self.next_time()? != second {
            return None;
        }
        let (key, entry) = self.queue.pop_first()?;
        self.index.remove(&key.seq);
        if let Some(period) = entry.period {
            let next = EventKey {
                time: key.time + period,
                micro: 0,
                ..key
            };
            self.queue.insert(next, entry.clone());
            self.index.insert(key.seq, next);
        }
        Some(Fired {
            key,
            handle: EventHandle(key.seq),
            payload: entry.payload,
            meta: entry.meta,
        })
    }

    /// Context for applying an input: no triggering event, `now` is the first open
    /// second. In sim-core only `World::submit` calls this (ADR-0004).
    pub fn input_ctx(&mut self) -> Ctx<'_, P, M> {
        self.stats.contexts += 1;
        Ctx {
            now: self.closed_through + SimDuration::from_secs(1),
            current: None,
            sched: self,
        }
    }

    /// The queue as plain data in key order. Taken between two calls of
    /// `advance_until`, every pending event has micro step 0.
    pub fn snapshot(&self) -> SchedulerSnapshot<P, M> {
        let entries = self
            .queue
            .iter()
            .map(|(key, entry)| SnapshotEntry {
                time: key.time,
                micro: key.micro,
                seq: key.seq,
                payload: entry.payload.clone(),
                period: entry.period,
                meta: entry.meta.clone(),
            })
            .collect();
        SchedulerSnapshot {
            closed_through: self.closed_through,
            next_seq: self.next_seq,
            entries,
        }
    }

    /// Rebuilds a scheduler from a snapshot; keys are recomputed from the payloads.
    pub fn restore(snapshot: SchedulerSnapshot<P, M>) -> Result<Self, LoadError> {
        let mut sched = Scheduler {
            closed_through: snapshot.closed_through,
            next_seq: snapshot.next_seq,
            queue: BTreeMap::new(),
            index: BTreeMap::new(),
            stats: RunStats::default(),
        };
        let mut previous: Option<EventKey> = None;
        for entry in snapshot.entries {
            if entry.micro != 0 {
                return Err(LoadError::MicroNotZero);
            }
            if entry.time <= snapshot.closed_through {
                return Err(LoadError::NotInFuture);
            }
            if entry.seq >= snapshot.next_seq {
                return Err(LoadError::SeqNotBelowNext);
            }
            if entry
                .period
                .is_some_and(|period| period < SimDuration::from_secs(1))
            {
                return Err(LoadError::PeriodTooShort);
            }
            let class = entry.payload.class();
            let key = EventKey {
                time: entry.time,
                phase: class.phase(),
                micro: 0,
                class,
                subject: entry.payload.subject(),
                kind: entry.payload.kind(),
                seq: entry.seq,
            };
            if previous.is_some_and(|previous| previous >= key) {
                return Err(LoadError::NotAscending);
            }
            if sched.index.insert(entry.seq, key).is_some() {
                return Err(LoadError::DuplicateSeq);
            }
            sched.queue.insert(
                key,
                Entry {
                    payload: entry.payload,
                    period: entry.period,
                    meta: entry.meta,
                },
            );
            previous = Some(key);
        }
        Ok(sched)
    }
}

impl RunStats {
    fn count(&mut self, class: EventClass, kind: EventKind) {
        *self.events.entry((class.0, kind.0)).or_insert(0) += 1;
    }
}

/// The scheduler part of a handler context.
pub struct Ctx<'a, P, M> {
    sched: &'a mut Scheduler<P, M>,
    now: SimTime,
    current: Option<EventKey>,
}

impl<P: Payload, M: Clone> Ctx<'_, P, M> {
    /// The first open second.
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// Key of the triggering event; `None` in the input context.
    pub fn current_key(&self) -> Option<EventKey> {
        self.current
    }

    /// Plans an event. In a handler of second t: before t, or at t in an earlier
    /// phase, is `scheduler.schedule_in_past`; at t in the same phase it runs in the next
    /// micro step. In the input context nothing may be planned before `now`.
    #[track_caller]
    pub fn schedule(&mut self, at: SimTime, payload: P, meta: M) -> EventHandle {
        self.insert(at, payload, None, meta)
    }

    /// Plans a periodic series (a tick). One handle stands for the whole series.
    #[track_caller]
    pub fn schedule_periodic(
        &mut self,
        first: SimTime,
        period: SimDuration,
        payload: P,
        meta: M,
    ) -> EventHandle {
        if period < SimDuration::from_secs(1) {
            panic!("invariant scheduler.period_too_short");
        }
        self.insert(first, payload, Some(period), meta)
    }

    /// Removes a pending event (or the pending occurrence of a series, which ends it).
    /// `false` if it has already run or was cancelled.
    pub fn cancel(&mut self, handle: EventHandle) -> bool {
        match self.sched.index.remove(&handle.0) {
            Some(key) => {
                self.sched.queue.remove(&key);
                true
            }
            None => false,
        }
    }

    /// Cancels a pending event and plans it anew at `at` with the same payload and
    /// period under a new handle.
    #[track_caller]
    pub fn reschedule(&mut self, handle: EventHandle, at: SimTime, meta: M) -> EventHandle {
        let Some(key) = self.sched.index.remove(&handle.0) else {
            panic!("invariant scheduler.reschedule_not_pending");
        };
        let Some(entry) = self.sched.queue.remove(&key) else {
            panic!("invariant scheduler.index_consistent");
        };
        self.insert(at, entry.payload, entry.period, meta)
    }

    #[track_caller]
    fn insert(
        &mut self,
        at: SimTime,
        payload: P,
        period: Option<SimDuration>,
        meta: M,
    ) -> EventHandle {
        let class = payload.class();
        let micro = self.micro_step_for(at, class.phase());
        let seq = self.sched.next_seq;
        self.sched.next_seq += 1;
        let key = EventKey {
            time: at,
            phase: class.phase(),
            micro,
            class,
            subject: payload.subject(),
            kind: payload.kind(),
            seq,
        };
        self.sched.queue.insert(
            key,
            Entry {
                payload,
                period,
                meta,
            },
        );
        self.sched.index.insert(seq, key);
        EventHandle(seq)
    }

    #[track_caller]
    fn micro_step_for(&self, at: SimTime, phase: u8) -> u16 {
        let Some(trigger) = self.current else {
            if at < self.now {
                panic!("invariant input_schedules_past");
            }
            return 0;
        };
        match (at.cmp(&trigger.time), phase.cmp(&trigger.phase)) {
            (Ordering::Less, _) | (Ordering::Equal, Ordering::Less) => {
                panic!("invariant scheduler.schedule_in_past")
            }
            (Ordering::Greater, _) | (Ordering::Equal, Ordering::Greater) => 0,
            (Ordering::Equal, Ordering::Equal) => {
                let micro = trigger.micro + 1;
                if micro > MAX_MICRO_STEP {
                    panic!("invariant scheduler.cascade_limit (trigger {trigger:?})");
                }
                micro
            }
        }
    }
}
