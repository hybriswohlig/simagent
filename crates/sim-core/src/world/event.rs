//! The event payload of the world (ADR-0002): plain data in the scheduler queue.

use serde::{Deserialize, Serialize};

use crate::house::data::Need;
use crate::house::state::Duty;
use crate::ids::{HouseholdId, PartnershipId, SimId};
use crate::time::{EventClass, EventKind, EventSubject, Payload, SimTime};

/// What can be scheduled. Variants arrive with their handler and only go to the end;
/// kind codes are never renumbered (`tests/golden/event_codes.txt`). The nine house events
/// came before their handlers, so that the projection changes once for the house (ADR-0024,
/// addendum of 2026-11-02); until a handler comes, nothing plans its event and the loader
/// refuses it.
///
/// postcard encodes the variant index: it is part of the state hash, pinned by the
/// reference bytes in `tests/projection.rs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPayload {
    /// Periodic observer in the last second of every year (ADR-0002).
    YearEnd,
    /// Dummy domain event of step 2: a sim's birthday, re-planned by its handler.
    Birthday { sim: SimId },
    /// The hazard budget of `sim` for a cause of death runs out; `cause` is the
    /// `str_key` of its code (ADR-0003 5.2).
    Death { sim: SimId, cause: u64 },
    /// Monthly series: the woman of an open partnership may conceive.
    ConceptionAttempt { woman: SimId },
    /// The pregnancy of `mother` ends in a birth.
    Birth { mother: SimId },
    /// The budget of the separation of a partnership runs out (ADR-0018 section 3).
    Separation { partnership: PartnershipId },
    /// Monthly series: a round of the partner market (genetics specification 11.2,
    /// ADR-0018 section 1).
    MarketRound,
    /// `sim` reaches the age at which it leaves home (ADR-0018 section 4).
    LeaveHome { sim: SimId },
    /// Yearly series in the last second of each year: households may emigrate if the
    /// living exceed the soft capacity (ADR-0020 section 6).
    Emigration { capacity: u64 },
    /// Yearly series in the first second of each year: households of the pool arrive
    /// until they bring `persons` (ADR-0020 section 5).
    Immigration { persons: u64 },
    /// `sim` reaches the onset of a hereditary disease; `disease` is the `str_key` of its
    /// code (ADR-0017 section 3, addendum of 2026-10-20).
    DiseaseOnset { sim: SimId, disease: u64 },
    /// 6:00 of the first morning after a household moved in (ADR-0024 section 1).
    HouseMorning { household: HouseholdId },
    /// Daily series at 5:00: a person's duties of the day (ADR-0024 section 4).
    DayPlan { sim: SimId },
    /// A person is back from `place` (ADR-0024 section 4).
    Return { sim: SimId, place: Duty },
    /// A person leaves for `place` and is due back at `back`.
    Depart {
        sim: SimId,
        place: Duty,
        back: SimTime,
    },
    /// 45 minutes before a departure: ends sleep (ADR-0024 section 4).
    Wake { sim: SimId },
    /// A person reaches the end of a walk (ADR-0025 section 5, ADR-0026 section 4).
    Arrive { sim: SimId },
    /// A person's action ends (ADR-0026 section 4).
    ActionEnd { sim: SimId },
    /// A falling need of a person reaches `level` (ADR-0026 section 1).
    NeedCrossing { sim: SimId, need: Need, level: u16 },
    /// A person chooses (ADR-0026 sections 3 and 5).
    Decide { sim: SimId },
}

impl EventPayload {
    pub const YEAR_END: EventKind = EventKind(1);
    pub const BIRTHDAY: EventKind = EventKind(2);
    pub const DEATH: EventKind = EventKind(3);
    pub const CONCEPTION_ATTEMPT: EventKind = EventKind(4);
    pub const BIRTH: EventKind = EventKind(5);
    pub const SEPARATION: EventKind = EventKind(6);
    pub const MARKET_ROUND: EventKind = EventKind(7);
    pub const LEAVE_HOME: EventKind = EventKind(8);
    pub const EMIGRATION: EventKind = EventKind(9);
    pub const IMMIGRATION: EventKind = EventKind(10);
    pub const DISEASE_ONSET: EventKind = EventKind(11);
    pub const HOUSE_MORNING: EventKind = EventKind(12);
    pub const DAY_PLAN: EventKind = EventKind(13);
    pub const RETURN: EventKind = EventKind(14);
    pub const DEPART: EventKind = EventKind(15);
    pub const WAKE: EventKind = EventKind(16);
    pub const ARRIVE: EventKind = EventKind(17);
    pub const ACTION_END: EventKind = EventKind(18);
    pub const NEED_CROSSING: EventKind = EventKind(19);
    pub const DECIDE: EventKind = EventKind(20);

    /// Whether this is a house event (ADR-0024, addendum of 2026-11-02).
    pub fn is_house(&self) -> bool {
        matches!(
            self,
            EventPayload::HouseMorning { .. }
                | EventPayload::DayPlan { .. }
                | EventPayload::Return { .. }
                | EventPayload::Depart { .. }
                | EventPayload::Wake { .. }
                | EventPayload::Arrive { .. }
                | EventPayload::ActionEnd { .. }
                | EventPayload::NeedCrossing { .. }
                | EventPayload::Decide { .. }
        )
    }
}

impl Payload for EventPayload {
    fn class(&self) -> EventClass {
        match self {
            EventPayload::YearEnd => EventClass::OBSERVER,
            EventPayload::Birthday { .. }
            | EventPayload::ConceptionAttempt { .. }
            | EventPayload::Separation { .. }
            | EventPayload::MarketRound
            | EventPayload::LeaveHome { .. }
            | EventPayload::DiseaseOnset { .. }
            | EventPayload::HouseMorning { .. }
            | EventPayload::DayPlan { .. }
            | EventPayload::Return { .. }
            | EventPayload::Depart { .. }
            | EventPayload::Wake { .. }
            | EventPayload::Arrive { .. }
            | EventPayload::ActionEnd { .. }
            | EventPayload::NeedCrossing { .. }
            | EventPayload::Decide { .. } => EventClass::GENERAL,
            EventPayload::Death { .. } => EventClass::DEATH,
            EventPayload::Birth { .. } => EventClass::BIRTH,
            EventPayload::Emigration { .. } => EventClass::EMIGRATION,
            EventPayload::Immigration { .. } => EventClass::IMMIGRATION,
        }
    }

    fn subject(&self) -> EventSubject {
        match self {
            EventPayload::YearEnd
            | EventPayload::MarketRound
            | EventPayload::Emigration { .. }
            | EventPayload::Immigration { .. } => EventSubject::WORLD,
            EventPayload::Birthday { sim }
            | EventPayload::Death { sim, .. }
            | EventPayload::ConceptionAttempt { woman: sim }
            | EventPayload::Birth { mother: sim }
            | EventPayload::LeaveHome { sim }
            | EventPayload::DiseaseOnset { sim, .. }
            | EventPayload::DayPlan { sim }
            | EventPayload::Return { sim, .. }
            | EventPayload::Depart { sim, .. }
            | EventPayload::Wake { sim }
            | EventPayload::Arrive { sim }
            | EventPayload::ActionEnd { sim }
            | EventPayload::NeedCrossing { sim, .. }
            | EventPayload::Decide { sim } => EventSubject::sim(sim.get()),
            EventPayload::Separation { partnership } => {
                EventSubject::partnership(partnership.get())
            }
            EventPayload::HouseMorning { household } => EventSubject::household(household.get()),
        }
    }

    fn kind(&self) -> EventKind {
        match self {
            EventPayload::YearEnd => EventPayload::YEAR_END,
            EventPayload::Birthday { .. } => EventPayload::BIRTHDAY,
            EventPayload::Death { .. } => EventPayload::DEATH,
            EventPayload::ConceptionAttempt { .. } => EventPayload::CONCEPTION_ATTEMPT,
            EventPayload::Birth { .. } => EventPayload::BIRTH,
            EventPayload::Separation { .. } => EventPayload::SEPARATION,
            EventPayload::MarketRound => EventPayload::MARKET_ROUND,
            EventPayload::LeaveHome { .. } => EventPayload::LEAVE_HOME,
            EventPayload::Emigration { .. } => EventPayload::EMIGRATION,
            EventPayload::Immigration { .. } => EventPayload::IMMIGRATION,
            EventPayload::DiseaseOnset { .. } => EventPayload::DISEASE_ONSET,
            EventPayload::HouseMorning { .. } => EventPayload::HOUSE_MORNING,
            EventPayload::DayPlan { .. } => EventPayload::DAY_PLAN,
            EventPayload::Return { .. } => EventPayload::RETURN,
            EventPayload::Depart { .. } => EventPayload::DEPART,
            EventPayload::Wake { .. } => EventPayload::WAKE,
            EventPayload::Arrive { .. } => EventPayload::ARRIVE,
            EventPayload::ActionEnd { .. } => EventPayload::ACTION_END,
            EventPayload::NeedCrossing { .. } => EventPayload::NEED_CROSSING,
            EventPayload::Decide { .. } => EventPayload::DECIDE,
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
