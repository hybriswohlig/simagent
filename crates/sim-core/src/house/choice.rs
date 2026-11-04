//! The points of a choice (ADR-0026 section 3): every offer of the house open at the person's
//! age and the second, by object and offer, and the best one among the objects with a free
//! place; in the night first what is critical, then a bed (ADR-0026, addendum of 2026-11-04).
//! Pure functions, so that "Why?" can compute a choice again from its entry.

use alloc::vec::Vec;

use crate::house::data::{House, HouseData, KindDef, NEEDS, OfferDef, age_band};
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
/// most 255; `None` before the birth.
pub fn age(born: SimTime, at: SimTime) -> Option<u8> {
    let years = (at.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    (years >= 0).then(|| u8::try_from(years).unwrap_or(u8::MAX))
}

/// The points of every offer of `house` open at `age` and now, for a person with the needs
/// `values` in `room`, by object and then offer: `Σ (1000 − v) · gain − 50 · walking
/// seconds`, and `(151 − v) · 10⁹` for each critical need the offer raises. The gain of a need
/// the offer raises is `min(rate · length / 3600, 1000 − v)`, with the length `seconds` or
/// `max_seconds` and the rates of the age band of `age`. An offer of sleep is open only in the
/// person's night, `night_left` the seconds of it still to come, and that is its length
/// (ADR-0026, addendum of 2026-11-04); outside it, `None`, it is closed and not scored.
/// Nothing for an age without needs.
pub fn score(
    house: &House,
    data: &HouseData,
    age: u8,
    values: [u16; 5],
    room: u8,
    night_left: Option<i64>,
) -> Vec<Scored> {
    let Some(band) = age_band(age) else {
        return Vec::new();
    };
    let mut scored = Vec::new();
    for (object, _) in (0_u16..).zip(&house.def().objects) {
        let kind = kind_of(house, data, object);
        let walk = i64::from(house.walk_to(room, object)) * PER_WALKING_SECOND;
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            if !(def.min_age..=def.max_age).contains(&age) {
                continue;
            }
            let length = match (def.sleep, def.seconds.or(def.max_seconds)) {
                (true, _) => match night_left {
                    Some(left) => left,
                    None => continue,
                },
                (false, Some(length)) => i64::from(length),
                (false, None) => panic!("invariant house.offer_length"),
            };
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

/// What a choice comes to (ADR-0026 sections 3 and 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Walk to this offer.
    Take(Scored),
    /// Idle, waiting for this object with no free place.
    Wait(u16),
    /// Idle.
    Idle,
}

/// The kind of an object of the house.
fn kind_of<'a>(house: &House, data: &'a HouseData, object: u16) -> &'a KindDef {
    (house.def().objects.get(usize::from(object)))
        .and_then(|def| data.kind(&def.kind))
        .unwrap_or_else(|| panic!("invariant house.unknown"))
}

fn offer_of<'a>(house: &House, data: &'a HouseData, scored: &Scored) -> &'a OfferDef {
    let kind = kind_of(house, data, scored.object);
    (kind.offers.get(usize::from(scored.offer)))
        .unwrap_or_else(|| panic!("invariant house.unknown"))
}

/// The choice among `scored`, the objects in `busy` without a free place, for a person of
/// `age` with the needs `values` (ADR-0026 sections 3 and 5). By day: the best free offer if
/// it brings `ENOUGH`, else waiting for the best of a fully taken object if that would, else
/// idle. In the night, `night` (ADR-0026, addendum of 2026-11-04): the best free offer that
/// raises a need critical now, if any; else the best free bed, any furniture but the sofa
/// that sleeps, regardless of `ENOUGH`; else the best free sleeping place on the sofa (ADR-0025:
/// who finds no free bed sleeps on the sofa); else waiting for the best bed with no free
/// place; else idle. Ties as in `best`.
pub fn choose(
    (house, data): (&House, &HouseData),
    (age, values): (u8, [u16; 5]),
    scored: &[Scored],
    busy: &[u16],
    night: bool,
) -> Choice {
    if !night {
        if let Some(chosen) = best(scored, busy).filter(|chosen| chosen.points >= ENOUGH) {
            return Choice::Take(chosen);
        }
        let taken: Vec<Scored> = (scored.iter())
            .filter(|scored| busy.contains(&scored.object))
            .copied()
            .collect();
        return match best(&taken, &[]).filter(|best| best.points >= ENOUGH) {
            Some(waited) => Choice::Wait(waited.object),
            None => Choice::Idle,
        };
    }
    let rates = data.rates();
    let critical = |scored: &&Scored| {
        let offer = offer_of(house, data, scored);
        (NEEDS.into_iter().zip(values))
            .any(|(need, value)| value <= CRITICAL && offer.raises(rates, need, age))
    };
    let sofa = |scored: &Scored| kind_of(house, data, scored.object).id.as_str() == "core:sofa";
    let sleeps = |sofa_too: bool| {
        move |scored: &&Scored| offer_of(house, data, scored).sleep && sofa(scored) == sofa_too
    };
    let among = |keep: &dyn Fn(&&Scored) -> bool| -> Vec<Scored> {
        scored.iter().filter(keep).copied().collect()
    };
    let first_free = [
        among(&critical),
        among(&sleeps(false)),
        among(&sleeps(true)),
    ]
    .into_iter()
    .find_map(|candidates| best(&candidates, busy));
    if let Some(chosen) = first_free {
        return Choice::Take(chosen);
    }
    let beds: Vec<Scored> = (among(&sleeps(false)).into_iter())
        .filter(|scored| busy.contains(&scored.object))
        .collect();
    match best(&beds, &[]) {
        Some(waited) => Choice::Wait(waited.object),
        None => Choice::Idle,
    }
}
