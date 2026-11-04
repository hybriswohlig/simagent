//! The points of a choice (ADR-0026 section 3): every offer of the house open at the person's
//! age and the second, by object and offer, with the person's mood for it, and the best one
//! among the objects with a free place; in the night first what is critical, then a bed of the
//! person's tier, then any bed (ADR-0026, addenda of 2026-11-04). The second an idle person
//! chooses next (its addendum after the playtest). Pure functions, so that "Why?" can compute a
//! choice again from its entry and the loader the next choice from a save.

use alloc::vec::Vec;

use crate::house::data::{
    AGE_BANDS, BedFor, House, HouseData, KindDef, NEEDS, NeedRates, OfferDef, age_band,
};
use crate::house::day::{next_night, night};
use crate::house::need::{FULL, NeedSample};
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{HOUR, MAX_ABS_SECS, MINUTE, SimTime, YEAR};

/// The best offer must bring at least this many points; below it the person idles.
pub const ENOUGH: i64 = 40_000;

/// A need at or below this value is critical.
pub const CRITICAL: u16 = 150;

/// The points one second of walking costs (ADR-0025 section 4).
const PER_WALKING_SECOND: i64 = 50;

/// Per point a critical need lies below 151, more than any other sum can bring.
const PER_CRITICAL_POINT: i64 = 1_000_000_000;

/// An offer of an object with the person's mood for it, in percent, and its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scored {
    pub object: u16,
    pub offer: u16,
    pub mood: i8,
    pub points: i64,
}

/// The age a choice is made at (ADR-0026 section 3): whole years from `born` to `at`, at
/// most 255; `None` before the birth.
pub fn age(born: SimTime, at: SimTime) -> Option<u8> {
    let years = (at.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    (years >= 0).then(|| u8::try_from(years).unwrap_or(u8::MAX))
}

/// The mood of the person with the lineage key `key` for offer `offer` of object `object` in its
/// choice at `at`, in percent from −`percent` to +`percent`, the day's `choice_mood_percent`
/// (ADR-0026, addendum of 2026-11-04 on the mood; its amplitude in the data since the addendum
/// on a need at 0): `uniform_below(2 · percent + 1) − percent` of `CHOICE_MOOD` with the world
/// seed, subject the lineage key, occasion the second `at`, detail `(object << 32) | offer`
/// (ADR-0001 section 1). The same address gives the same mood, so "Why?" finds it again.
pub fn mood(
    seed: u64,
    key: LineageKey,
    at: SimTime,
    (object, offer): (u16, u16),
    percent: u8,
) -> i8 {
    let second =
        u64::try_from(at.as_secs()).unwrap_or_else(|_| panic!("invariant house.before_start"));
    let address = DrawKey {
        tag: tags::CHOICE_MOOD,
        subject: SubjectKey::from(key),
        occasion: second,
        detail: (u64::from(object) << 32) | u64::from(offer),
    };
    let span = 2 * u64::from(percent) + 1;
    let drawn = Draws::new(WorldSeed::new(seed)).uniform_below(address, span);
    (i8::try_from(drawn).ok())
        .and_then(|drawn| drawn.checked_sub_unsigned(percent))
        .unwrap_or_else(|| panic!("invariant house.mood"))
}

/// The points of every offer of `house` open at `age` and now, for a person with the needs
/// `values` in `room`, by object and then offer, each with its mood `moods(object, offer)` in
/// percent: `(Σ (1000 − v) · gain) · (100 + mood) / 100 − 50 · walking seconds`, truncated
/// toward zero, and `(151 − v) · 10⁹` for each critical need the offer raises; the mood scales
/// neither the walk nor that (ADR-0026, addendum of 2026-11-04 on the mood). The gain of a need
/// the offer raises is `min(rate · length / 3600, 1000 − v)`, with the length `seconds` or
/// `max_seconds` and the rates of the age band of `age`. An offer of sleep is open only in the
/// person's night, `night_left` the seconds of it still to come, and that is its length
/// (ADR-0026, addendum of 2026-11-04 on the night); outside it, `None`, it is closed and not
/// scored. Nothing for an age without needs.
pub fn score(
    house: &House,
    data: &HouseData,
    age: u8,
    values: [u16; 5],
    room: u8,
    night_left: Option<i64>,
    moods: impl Fn(u16, u16) -> i8,
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
            let mood = moods(object, offer);
            let points = points(def, (data.rates(), band, length), values, walk, mood);
            scored.push(Scored {
                object,
                offer,
                mood,
                points,
            });
        }
    }
    scored
}

/// The points of one offer `def` of `length` seconds in the age band `band` for the needs
/// `values`, `walk` the points its walk costs, with the mood `mood` (`score`).
fn points(
    def: &OfferDef,
    (rates, band, length): (&NeedRates, usize, i64),
    values: [u16; 5],
    walk: i64,
    mood: i8,
) -> i64 {
    let (mut filled, mut critical) = (0, 0);
    for (need, value) in NEEDS.into_iter().zip(values) {
        let rate = i64::from(def.rate(rates, need, band));
        if rate <= 0 {
            continue;
        }
        let missing = i64::from(FULL.saturating_sub(value));
        filled += missing * (rate * length / HOUR.as_secs()).min(missing);
        if value <= CRITICAL {
            critical += i64::from(CRITICAL + 1 - value) * PER_CRITICAL_POINT;
        }
    }
    filled * (100 + i64::from(mood)) / 100 - walk + critical
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

/// The beds a person takes first in its night (ADR-0026, addendum of 2026-11-04 after the
/// playtest): its partner's bed for couples, the beds for couples or for children, or any bed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// The bed for couples on which the partner holds a place.
    Partner(u16),
    /// The beds of kinds for these sleepers.
    For(BedFor),
    /// Every bed.
    Any,
}

/// The age from which one is no child in the night's order: the first of the adults' band.
pub const ADULT: u8 = AGE_BANDS[2].0;

/// The tier of a person of `age` (ADR-0026, addendum of 2026-11-04 after the playtest): with an
/// open partnership with another living member of its household (`partnered`) the bed for
/// couples its partner holds a place on, `partner_bed`, or else every bed for couples; under
/// `ADULT` the beds for children; anyone else any bed.
pub fn tier(partnered: bool, partner_bed: Option<u16>, age: u8) -> Tier {
    match (partnered, partner_bed) {
        (true, Some(bed)) => Tier::Partner(bed),
        (true, None) => Tier::For(BedFor::Couples),
        (false, _) if age < ADULT => Tier::For(BedFor::Children),
        (false, _) => Tier::Any,
    }
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

/// Whether `object` of the house is in the beds of `tier`.
pub fn in_tier(house: &House, data: &HouseData, tier: Tier, object: u16) -> bool {
    let kind = kind_of(house, data, object);
    kind.is_bed()
        && match tier {
            Tier::Partner(bed) => object == bed,
            Tier::For(sleepers) => kind.bed_for == Some(sleepers),
            Tier::Any => true,
        }
}

/// The choice among `scored`, the objects in `busy` without a free place, for a person of
/// `age` with the needs `values` (ADR-0026 sections 3 and 5), by the points with the mood. By
/// day, `night` `None`: the best free offer if it brings `ENOUGH`, else waiting for the best of a
/// fully taken object if that would, else idle. In the night, `night` the beds of the person's
/// tier (`in_tier`; ADR-0026, addenda of 2026-11-04 on the night and after the playtest): the best free offer
/// that raises a need critical now, if any; else the best free bed of the tier; else the best
/// free bed, any furniture but the sofa that sleeps, regardless of `ENOUGH`; else the best free
/// sleeping place on the sofa (ADR-0025: who finds no free bed sleeps on the sofa); else
/// waiting for the best bed with no free place; else idle. The mood ranks within each of these
/// alone. Ties as in `best`.
pub fn choose(
    (house, data): (&House, &HouseData),
    (age, values): (u8, [u16; 5]),
    scored: &[Scored],
    busy: &[u16],
    night: Option<&[u16]>,
) -> Choice {
    let Some(tier) = night else {
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
    };
    let rates = data.rates();
    let critical = |scored: &&Scored| {
        let offer = offer_of(house, data, scored);
        (NEEDS.into_iter().zip(values))
            .any(|(need, value)| value <= CRITICAL && offer.raises(rates, need, age))
    };
    let sofa = |scored: &Scored| kind_of(house, data, scored.object).is_sofa();
    let sleeps = |sofa_too: bool| {
        move |scored: &&Scored| offer_of(house, data, scored).sleep && sofa(scored) == sofa_too
    };
    let tiered = |scored: &&Scored| sleeps(false)(scored) && tier.contains(&scored.object);
    let among = |keep: &dyn Fn(&&Scored) -> bool| -> Vec<Scored> {
        scored.iter().filter(keep).copied().collect()
    };
    let first_free = [
        among(&critical),
        among(&tiered),
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

/// The second an idle person chooses next (ADR-0026, addendum of 2026-11-04 after the
/// playtest), born at `born`, with the needs `needs` standing in `room` and the objects for
/// which `free` holds having a free place, from `start` on: the first second at which an offer
/// of one of these objects brings `ENOUGH` with the person's mood of that second,
/// `moods(second, object, offer)`. In the night it looks from the night's end on. Per offer
/// only whole minutes are weighed, from the first second it brings `ENOUGH` at the most mood
/// (`choice_mood_percent`) to the first it surely does at the least, which counts if no
/// minute before it does; the earliest over the offers. Not at or after the person's next
/// bedtime, which makes it choose anyway; its next birthday, where the offers and their rates
/// may change, comes first if it is earlier. `None` if nothing comes, or beyond the end of time.
/// The needs keep the rates of their samples; the sample of a need that rises is weighed at its
/// value at `start` for the most and full for the least, so both bounds only grow with time.
pub fn next_choice(
    (house, data): (&House, &HouseData),
    (born, needs, room): (SimTime, &[NeedSample; 5], u8),
    free: impl Fn(u16) -> bool,
    start: SimTime,
    moods: impl Fn(SimTime, u16, u16) -> i8,
) -> Option<SimTime> {
    let rules = data.day();
    let start = match night(rules, born, start) {
        Some(night) => night.until?,
        None => start,
    };
    let age = age(born, start)?;
    let band = age_band(age)?;
    let bedtime = next_night(rules, born, start).map(|night| night.from);
    let birthday = (born.as_secs())
        .checked_add(YEAR.as_secs() * (i64::from(age) + 1))
        .and_then(|secs| SimTime::try_from(secs).ok());
    let end = MAX_ABS_SECS + 1;
    let bound = |at: Option<SimTime>| at.map_or(end, SimTime::as_secs);
    let limit = bound(bedtime).min(bound(birthday));
    let percent =
        i8::try_from(rules.choice_mood_percent).unwrap_or_else(|_| panic!("invariant house.mood"));
    let mut first: Option<i64> = None;
    for (object, _) in (0_u16..).zip(&house.def().objects) {
        if !free(object) {
            continue;
        }
        let kind = kind_of(house, data, object);
        let walk = i64::from(house.walk_to(room, object)) * PER_WALKING_SECOND;
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            // By day an offer of sleep is closed, and the search ends before the night.
            if def.sleep || !(def.min_age..=def.max_age).contains(&age) {
                continue;
            }
            let length = i64::from(def.seconds.or(def.max_seconds).unwrap_or_else(|| {
                panic!("invariant house.offer_length");
            }));
            let weighed = (data.rates(), band, length);
            let reaches = |at: i64, mood: i8, bound: Option<u16>| {
                let at = SimTime::from_secs(at);
                let values = core::array::from_fn(|n| match (needs[n].rate() > 0, bound) {
                    (true, Some(value)) => value,
                    (true, None) => needs[n].value_at(start),
                    (false, _) => needs[n].value_at(at),
                });
                points(def, weighed, values, walk, mood) >= ENOUGH
            };
            let until = first.unwrap_or(limit).min(limit);
            let most = |at: i64| reaches(at, percent, None);
            let Some(from) = first_at(start.as_secs(), until, most) else {
                continue;
            };
            let least = |at: i64| reaches(at, -percent, Some(FULL));
            let surely = first_at(from, until, least);
            let mut minute = from.div_euclid(MINUTE.as_secs()) * MINUTE.as_secs();
            if minute < from {
                minute += MINUTE.as_secs();
            }
            let mut found = surely;
            while minute < surely.unwrap_or(until) {
                let mood = moods(SimTime::from_secs(minute), object, offer);
                let exact = |at: i64| {
                    let at = SimTime::from_secs(at);
                    points(
                        def,
                        weighed,
                        needs.map(|need| need.value_at(at)),
                        walk,
                        mood,
                    )
                };
                if exact(minute) >= ENOUGH {
                    found = Some(minute);
                    break;
                }
                minute += MINUTE.as_secs();
            }
            if let Some(found) = found {
                first = Some(first.map_or(found, |first| first.min(found)));
            }
        }
    }
    match first {
        Some(at) => SimTime::try_from(at).ok(),
        None => birthday.filter(|&birthday| bound(Some(birthday)) < bound(bedtime)),
    }
}

/// The first second in `from..until` at which `holds`, which turns true once and stays; `None`
/// if it does not before `until`.
fn first_at(from: i64, until: i64, holds: impl Fn(i64) -> bool) -> Option<i64> {
    if from >= until || !holds(until - 1) {
        return None;
    }
    let (mut low, mut high) = (from, until - 1);
    while low < high {
        let middle = low + (high - low) / 2;
        if holds(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some(low)
}
