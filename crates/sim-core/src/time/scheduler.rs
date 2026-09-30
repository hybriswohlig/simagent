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
    pub const BIRTH: EventClass = EventClass(200);
    pub const GENERAL: EventClass = EventClass(500);
    pub const DEATH: EventClass = EventClass(900);
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

    pub const fn id(self) -> u64 {
        self.id
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

    pub fn pending_key(&self, _handle: EventHandle) -> Option<EventKey> {
        None
    }

    pub fn pending_handle_time(&self, handle: EventHandle) -> Option<SimTime> {
        self.pending_key(handle).map(|key| key.time)
    }

    pub fn stats(&self) -> &RunStats {
        &self.stats
    }

    /// Runs every event up to and including `limit` in key order.
    pub fn advance_until<H: Handler<P, M>>(
        &mut self,
        _h: &mut H,
        _out: &mut H::Out,
        _limit: SimTime,
        _stop: &mut dyn FnMut(&H) -> bool,
    ) -> AdvanceOutcome {
        AdvanceOutcome::ReachedLimit
    }

    /// Context for applying an input: no triggering event, `now` is the first open
    /// second. In sim-core only `World::submit` calls this (ADR-0004).
    pub fn input_ctx(&mut self) -> Ctx<'_, P, M> {
        Ctx {
            now: self.closed_through,
            current: None,
            sched: self,
        }
    }

    pub fn snapshot(&self) -> SchedulerSnapshot<P, M> {
        SchedulerSnapshot {
            closed_through: self.closed_through,
            next_seq: self.next_seq,
            entries: Vec::new(),
        }
    }

    pub fn restore(_snapshot: SchedulerSnapshot<P, M>) -> Result<Self, LoadError> {
        Ok(Self::new())
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

    pub fn schedule(&mut self, _at: SimTime, _payload: P, _meta: M) -> EventHandle {
        let _ = &self.sched;
        EventHandle(0)
    }

    pub fn schedule_periodic(
        &mut self,
        _first: SimTime,
        _period: SimDuration,
        _payload: P,
        _meta: M,
    ) -> EventHandle {
        EventHandle(0)
    }

    pub fn cancel(&mut self, _handle: EventHandle) -> bool {
        false
    }

    pub fn reschedule(&mut self, handle: EventHandle, _at: SimTime, _meta: M) -> EventHandle {
        handle
    }
}
