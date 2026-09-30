//! Entity IDs and counters (ADR-0004 sections 1 and 3). One newtype over `NonZeroU64`
//! per entity kind, handed out in strictly increasing order and never reused. Counters
//! stop at 2^53 - 1 so that every ID stays exact in JavaScript.

use core::fmt;
use core::num::NonZeroU64;

use serde::{Deserialize, Serialize};

/// Largest value a counter hands out: 2^53 - 1 (ADR-0004 section 3).
pub const MAX_JS_SAFE: u64 = (1 << 53) - 1;

/// A value outside 1 ..= 2^53 - 1 where an ID or counter value was expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdOutOfRange;

impl fmt::Display for IdOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("id_out_of_range")
    }
}

fn checked(value: u64) -> Result<NonZeroU64, IdOutOfRange> {
    if value > MAX_JS_SAFE {
        return Err(IdOutOfRange);
    }
    NonZeroU64::new(value).ok_or(IdOutOfRange)
}

macro_rules! entity_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "u64", into = "u64")]
        pub struct $name(NonZeroU64);

        impl $name {
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }

        impl TryFrom<u64> for $name {
            type Error = IdOutOfRange;
            fn try_from(value: u64) -> Result<Self, IdOutOfRange> {
                checked(value).map($name)
            }
        }

        impl From<$name> for u64 {
            fn from(id: $name) -> u64 {
                id.0.get()
            }
        }
    };
}

entity_id!(
    /// A sim, from birth on; the unborn are addressed by their `lineage_key` (ADR-0001).
    SimId
);
entity_id!(
    /// Position of an input in the input log, from 1.
    InputSeq
);

/// ID of a log entry. Opaque: handlers only pass it on as a cause or `by`; there is no
/// comparison and no access to the number (ADR-0004 section 6).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct EventId(NonZeroU64);

impl TryFrom<u64> for EventId {
    type Error = IdOutOfRange;
    fn try_from(value: u64) -> Result<Self, IdOutOfRange> {
        checked(value).map(EventId)
    }
}

impl From<EventId> for u64 {
    fn from(id: EventId) -> u64 {
        id.0.get()
    }
}

/// A counter of one kind: first value 1, strictly increasing, never reset. Going
/// beyond 2^53 - 1 is the invariant break `counter_exceeds_js_safe`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Counter {
    next: u64,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the World uses the counters from step 2d-3 on")
)]
impl Counter {
    pub(crate) const fn new() -> Counter {
        Counter { next: 1 }
    }

    #[track_caller]
    pub(crate) fn allocate(&mut self) -> NonZeroU64 {
        let Ok(value) = checked(self.next) else {
            panic!("invariant counter_exceeds_js_safe: next {}", self.next);
        };
        self.next += 1;
        value
    }

    pub(crate) fn allocate_sim(&mut self) -> SimId {
        SimId(self.allocate())
    }

    pub(crate) fn allocate_event(&mut self) -> EventId {
        EventId(self.allocate())
    }

    pub(crate) fn allocate_input(&mut self) -> InputSeq {
        InputSeq(self.allocate())
    }

    /// The value the next allocation will return.
    pub(crate) fn peek(&self) -> u64 {
        self.next
    }

    #[cfg(test)]
    pub(crate) fn starting_at(next: u64) -> Counter {
        Counter { next }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_start_at_one_and_count_up() {
        let mut counter = Counter::new();
        assert_eq!(counter.allocate_sim().get(), 1);
        assert_eq!(counter.allocate_sim().get(), 2);
        assert_eq!(counter.peek(), 3);
        let mut events = Counter::new();
        assert_eq!(u64::from(events.allocate_event()), 1);
        let mut inputs = Counter::new();
        assert_eq!(inputs.allocate_input().get(), 1);
        assert_eq!(inputs.allocate_input().get(), 2);
    }

    /// ADR-0004 test 10.
    #[test]
    fn counter_js_safe_limit() {
        let mut counter = Counter::starting_at(MAX_JS_SAFE);
        assert_eq!(counter.allocate_sim().get(), MAX_JS_SAFE);
        let result = std::panic::catch_unwind(move || counter.allocate_sim());
        assert!(result.is_err(), "the next allocation must break");
    }

    #[test]
    #[should_panic(expected = "invariant counter_exceeds_js_safe")]
    fn beyond_the_limit_is_an_invariant_break() {
        let mut counter = Counter::starting_at(MAX_JS_SAFE + 1);
        let _ = counter.allocate();
    }

    #[test]
    fn ids_accept_only_one_to_the_js_safe_limit() {
        assert_eq!(SimId::try_from(0), Err(IdOutOfRange));
        assert_eq!(SimId::try_from(MAX_JS_SAFE + 1), Err(IdOutOfRange));
        assert_eq!(
            SimId::try_from(MAX_JS_SAFE).map(SimId::get),
            Ok(MAX_JS_SAFE)
        );
        assert_eq!(SimId::try_from(1).map(SimId::get), Ok(1));
    }
}
