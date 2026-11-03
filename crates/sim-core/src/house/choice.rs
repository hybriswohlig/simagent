//! The points of a choice (ADR-0026 section 3): every offer of the house open at the person's
//! age, by object and offer, and the best one among the objects with a free place. Pure
//! functions, so that "Why?" can compute a choice again from its entry.

use alloc::vec::Vec;

use crate::house::data::{House, HouseData, NEEDS, age_band};
use crate::house::need::FULL;
use crate::time::{HOUR, SimTime, YEAR};

/// The best offer must bring at least this many points; below it the person idles.
pub const ENOUGH: i64 = 40_000;

/// A need at or below this value is critical.
pub const CRITICAL: u16 = 150;

/// The points one second of walking costs (ADR-0025 section 4).
const PER_WALKING_SECOND: i64 = 50;

/// Per point a critical need lies below 151, more than any other sum can bring.
const PER_CRITICAL_POINT: i64 = 1_000_000_000;

/// An offer of an object with its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scored {
    pub object: u16,
    pub offer: u16,
    pub points: i64,
}

/// The age a choice is made at (ADR-0026 section 3): whole years from `born` to `at`, at
/// most 255.
pub fn age(born: SimTime, at: SimTime) -> u8 {
    let years = (at.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    u8::try_from(years).unwrap_or(u8::MAX)
}

/// The points of every offer of `house` open at `age`, for a person with the needs `values`
/// in `room`, by object and then offer: `Σ (1000 − v) · gain − 50 · walking seconds`, and
/// `(151 − v) · 10⁹` for each critical need the offer raises. The gain of a need the offer
/// raises is `min(rate · length / 3600, 1000 − v)`, with the length `seconds` or
/// `max_seconds` and the rates of the age band of `age`. Nothing for an age without needs.
pub fn score(house: &House, data: &HouseData, age: u8, values: [u16; 5], room: u8) -> Vec<Scored> {
    let Some(band) = age_band(age) else {
        return Vec::new();
    };
    let mut scored = Vec::new();
    for (object, def) in (0_u16..).zip(&house.def().objects) {
        let Some(kind) = data.kind(&def.kind) else {
            panic!("invariant house.unknown");
        };
        let walk = i64::from(house.walk_to(room, object)) * PER_WALKING_SECOND;
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            if !(def.min_age..=def.max_age).contains(&age) {
                continue;
            }
            let Some(length) = def.seconds.or(def.max_seconds) else {
                panic!("invariant house.offer_length");
            };
            let length = i64::from(length);
            let mut points = -walk;
            for (need, value) in NEEDS.into_iter().zip(values) {
                let rate = i64::from(def.rate(data.rates(), need, band));
                if rate <= 0 {
                    continue;
                }
                let missing = i64::from(FULL.saturating_sub(value));
                points += missing * (rate * length / HOUR.as_secs()).min(missing);
                if value <= CRITICAL {
                    points += i64::from(CRITICAL + 1 - value) * PER_CRITICAL_POINT;
                }
            }
            scored.push(Scored {
                object,
                offer,
                points,
            });
        }
    }
    scored
}

/// The offer with the most points among the objects not in `busy`; a tie goes to the smaller
/// object, then the smaller offer. None if every object is busy.
pub fn best(scored: &[Scored], busy: &[u16]) -> Option<Scored> {
    let mut best: Option<Scored> = None;
    for candidate in scored
        .iter()
        .filter(|scored| !busy.contains(&scored.object))
    {
        if best.is_none_or(|best| candidate.points > best.points) {
            best = Some(*candidate);
        }
    }
    best
}
