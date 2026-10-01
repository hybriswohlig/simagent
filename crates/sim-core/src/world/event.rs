//! The event payload of the world (ADR-0002): plain data in the scheduler queue.

use crate::ids::SimId;
use crate::time::{EventClass, EventKind, EventSubject, Payload};

/// What can be scheduled. Variants arrive with their handler and only go to the end;
/// kind codes are never renumbered (`tests/golden/event_codes.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventPayload {
    /// Periodic observer in the last second of every year (ADR-0002).
    YearEnd,
    /// Dummy domain event of step 2: a sim's birthday, re-planned by its handler.
    Birthday { sim: SimId },
}

impl EventPayload {
    pub const YEAR_END: EventKind = EventKind(1);
    pub const BIRTHDAY: EventKind = EventKind(2);
}

impl Payload for EventPayload {
    fn class(&self) -> EventClass {
        match self {
            EventPayload::YearEnd => EventClass::OBSERVER,
            EventPayload::Birthday { .. } => EventClass::GENERAL,
        }
    }

    fn subject(&self) -> EventSubject {
        match self {
            EventPayload::YearEnd => EventSubject::WORLD,
            EventPayload::Birthday { sim } => EventSubject::sim(sim.get()),
        }
    }

    fn kind(&self) -> EventKind {
        match self {
            EventPayload::YearEnd => EventPayload::YEAR_END,
            EventPayload::Birthday { .. } => EventPayload::BIRTHDAY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0002: the year end runs in the observer phase, after every domain event of
    /// its second, and belongs to the world.
    #[test]
    fn year_end_is_a_world_observer() {
        let payload = EventPayload::YearEnd;
        assert_eq!(payload.class(), EventClass::OBSERVER);
        assert_eq!(payload.class().phase(), 1);
        assert_eq!(payload.subject(), EventSubject::WORLD);
        assert_eq!(payload.kind(), EventKind(1));
    }

    #[test]
    fn birthday_is_a_general_event_of_its_sim() {
        let sim = SimId::try_from(7).expect("valid");
        let payload = EventPayload::Birthday { sim };
        assert_eq!(payload.class(), EventClass::GENERAL);
        assert_eq!(payload.class().phase(), 0);
        assert_eq!(payload.subject(), EventSubject::sim(7));
        assert_eq!(payload.kind(), EventKind(2));
    }
}
