//! The event payload of the world (ADR-0002): plain data in the scheduler queue.

use crate::time::{EventClass, EventKind, EventSubject, Payload};

/// What can be scheduled. Variants arrive with their handler and only go to the end;
/// kind codes are never renumbered (`tests/golden/event_codes.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventPayload {
    /// Periodic observer in the last second of every year (ADR-0002).
    YearEnd,
}

impl EventPayload {
    pub const YEAR_END: EventKind = EventKind(1);
}

impl Payload for EventPayload {
    fn class(&self) -> EventClass {
        match self {
            EventPayload::YearEnd => EventClass::OBSERVER,
        }
    }

    fn subject(&self) -> EventSubject {
        match self {
            EventPayload::YearEnd => EventSubject::WORLD,
        }
    }

    fn kind(&self) -> EventKind {
        match self {
            EventPayload::YearEnd => EventPayload::YEAR_END,
        }
    }
}
