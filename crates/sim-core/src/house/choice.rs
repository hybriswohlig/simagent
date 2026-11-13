//! The points of a choice (ADR-0026 section 3): every offer of the house open at the person's
//! age and the second, by object and offer, with the person's mood for it, and the best one
//! among the objects with a free place; in the night first what is critical, then a bed of the
//! person's tier, then any bed (ADR-0026, addenda of 2026-11-04). The second an idle person
//! chooses next (its addendum after the playtest). A need weighs below its comfort, an offer that
//! would run into today's departure is closed, and one idle by day takes a time-filler (its
//! addendum on comfort and the time-filler), the one with the highest mood; in the night what
//! fills hunger or the bladder under its comfort comes before a bed (its addendum of 2026-11-05 on
//! the time-filler's length). Pure functions, so that "Why?" can compute a choice again from its
//! entry and the loader the next choice from a save.

use alloc::vec::Vec;

use crate::house::data::{
    AGE_BANDS, BedFor, House, HouseData, KindDef, NEEDS, Need, NeedRates, OfferDef, age_band,
};
use crate::house::day::{next_night, night};
use crate::house::need::{FULL, NeedSample};
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::social::feeling::scaled_company;
use crate::time::{HOUR, MAX_ABS_SECS, MINUTE, SimTime, YEAR};

/// The best offer must bring at least this many points; below it the person idles.
pub const ENOUGH: i64 = 40_000;

/// A need at or below this value is critical.
pub const CRITICAL: u16 = 150;

/// The points one second of walking costs (ADR-0025 section 4).
const PER_WALKING_SECOND: i64 = 50;

/// Per point a critical need lies below 151, more than any other sum can bring.
const PER_CRITICAL_POINT: i64 = 1_000_000_000;

/// The shortest action (ADR-0026 section 4).
const SHORTEST_SECS: i64 = 60;

/// An offer of an object with the person's mood for it, in percent, and its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scored {
    pub object: u16,
    pub offer: u16,
    pub mood: i8,
    pub points: i64,
}

/// What the people at a furniture add and take for it in a choice (ADR-0030 section 4): `company`
/// the points for sitting with a liked person, `avoid` the points off for sitting with a distrusted
/// one, both from the data and not negative. `score` scales the company with the chooser's temper
/// (ADR-0032 section 5) and leaves the avoid as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bonus {
    pub company: i64,
    pub avoid: i64,
}

impl Bonus {
    /// Nobody there who counts.
    pub const NONE: Bonus = Bonus {
        company: 0,
        avoid: 0,
    };

    /// Only company.
    pub const fn company(company: i64) -> Bonus {
        Bonus { company, avoid: 0 }
    }

    /// Only avoid.
    pub const fn avoid(avoid: i64) -> Bonus {
        Bonus { company: 0, avoid }
    }
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
/// percent: `(Σ weight · gain) · (100 + mood) / 100 − 50 · walking seconds`, truncated toward
/// zero, and `(151 − v) · 10⁹` for each critical need the offer raises; the mood scales neither
/// the walk, the bonus of the furniture, nor that (ADR-0026, addendum of 2026-11-04 on the mood).
/// `bonus` is company and avoid for that furniture (ADR-0030 section 4); the net, the company
/// scaled by `temper`, the chooser's, as `company · (1000 + temper) / 1000` toward 0 (ADR-0032
/// section 5) minus the avoid, is added after the scaled needs like the walk. A fallback offer is scored here, so it gets the bonus too. The gain of a need the offer
/// raises is `min(rate · length / 3600, 1000 − v)`, with the length `seconds` or `max_seconds`
/// and the rates of the age band of `age`; its weight `max(0, comfort − v)` (the addendum on
/// comfort and the time-filler). An offer of sleep is open only in the person's night,
/// `night_left` the seconds of it still to come, and that is its length (ADR-0026, addendum of
/// 2026-11-04 on the night); outside it, `None`, it is closed and not scored. An offer that would
/// still run at today's departure, `depart_in` seconds away, is closed too (`crosses_departure`).
/// Nothing for an age without needs.
pub fn score(
    house: &House,
    data: &HouseData,
    (age, values, room): (u8, [u16; 5], u8),
    (night_left, depart_in): (Option<i64>, Option<i64>),
    moods: impl Fn(u16, u16) -> i8,
    temper: i16,
    bonus: impl Fn(u16) -> Bonus,
) -> Vec<Scored> {
    let Some(band) = age_band(age) else {
        return Vec::new();
    };
    let mut scored = Vec::new();
    for (object, _) in (0_u16..).zip(&house.def().objects) {
        let kind = kind_of(house, data, object);
        let walk_secs = house.walk_to(room, object);
        let walk = i64::from(walk_secs) * PER_WALKING_SECOND;
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
            if crosses_departure(def, data.rates(), (age, values), walk_secs, depart_in) {
                continue;
            }
            let mood = moods(object, offer);
            let furniture = bonus(object);
            let net = scaled_company(furniture.company, temper) - furniture.avoid;
            let points = points(def, (data.rates(), band, length), values, walk, mood, net);
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
/// `values`, `walk` the points its walk costs, with the mood `mood` and the furniture's
/// `bonus` (`score`).
fn points(
    def: &OfferDef,
    (rates, band, length): (&NeedRates, usize, i64),
    values: [u16; 5],
    walk: i64,
    mood: i8,
    bonus: i64,
) -> i64 {
    let (mut filled, mut critical) = (0, 0);
    for (need, value) in NEEDS.into_iter().zip(values) {
        let rate = i64::from(def.rate(rates, need, band));
        if rate <= 0 {
            continue;
        }
        let missing = i64::from(FULL.saturating_sub(value));
        let weight = i64::from(rates.comfort(need).saturating_sub(value));
        filled += weight * (rate * length / HOUR.as_secs()).min(missing);
        if value <= CRITICAL {
            critical += i64::from(CRITICAL + 1 - value) * PER_CRITICAL_POINT;
        }
    }
    filled * (100 + i64::from(mood)) / 100 - walk + bonus + critical
}

/// The seconds offer `def` lasts begun with the needs `values` in the age band `band`, as its
/// action would (`house::state::action_end`): its `seconds`, or until the need it fills is full
/// from its value in `values`, at least 60 s and at most `max_seconds`. `None` for an offer of
/// sleep, which lasts its night.
pub fn lasts(def: &OfferDef, rates: &NeedRates, band: usize, values: [u16; 5]) -> Option<i64> {
    match (def.seconds, def.until_full, def.max_seconds) {
        (Some(seconds), _, _) => Some(i64::from(seconds)),
        (None, Some(need), Some(longest)) => {
            let at = NEEDS.iter().position(|&each| each == need)?;
            let missing = i64::from(FULL.saturating_sub(values[at])) * HOUR.as_secs();
            let rate = i64::from(def.rate(rates, need, band));
            let full = match rate > 0 {
                true => (missing + rate - 1) / rate,
                false => i64::from(longest),
            };
            Some(full.max(SHORTEST_SECS).min(i64::from(longest)))
        }
        _ => None,
    }
}

/// The most seconds offer `def`, chosen in the age band `band` with the needs `values` and
/// `walk_secs` from its object, runs from its choice to its end: its walk, then its length
/// (`lasts`) from its arrival, the need it fills lowered first by the most the walk can lower
/// it. A walk keeps the rates the person had at its choice, awake ones; at the fastest fall of
/// the awake rates of any age band the need loses at most `⌈fall · walk_secs / 3600⌉` on it
/// (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler). `None` for an offer of
/// sleep, which lasts its night.
pub fn runs_for(
    def: &OfferDef,
    rates: &NeedRates,
    band: usize,
    values: [u16; 5],
    walk_secs: u32,
) -> Option<i64> {
    let mut arrival = values;
    let fills = def
        .until_full
        .and_then(|need| NEEDS.iter().position(|&each| each == need));
    if let Some(at) = fills {
        let fastest = (0..AGE_BANDS.len())
            .map(|band| -i64::from(rates.awake.rate(NEEDS[at], band)))
            .fold(0, i64::max);
        let fall = (fastest * i64::from(walk_secs) + HOUR.as_secs() - 1) / HOUR.as_secs();
        let lowered = (i64::from(values[at]) - fall).max(0);
        arrival[at] = u16::try_from(lowered).unwrap_or(0);
    }
    Some(i64::from(walk_secs) + lasts(def, rates, band, arrival)?)
}

/// Whether offer `def`, chosen at `age` with the needs `values` and `walk_secs` from its object,
/// could still run at today's departure, `depart_in` seconds after the choice: the most it runs
/// (`runs_for`) reaches it; unless it raises a need critical now (ADR-0026, addendum of
/// 2026-11-04 on comfort and the time-filler). Never without a departure still to come today.
pub fn crosses_departure(
    def: &OfferDef,
    rates: &NeedRates,
    (age, values): (u8, [u16; 5]),
    walk_secs: u32,
    depart_in: Option<i64>,
) -> bool {
    let (Some(depart_in), Some(band)) = (depart_in, age_band(age)) else {
        return false;
    };
    let Some(runs) = runs_for(def, rates, band, values, walk_secs) else {
        return false;
    };
    let urgent = (NEEDS.into_iter().zip(values))
        .any(|(need, value)| value <= CRITICAL && def.raises(rates, need, age));
    !urgent && runs >= depart_in
}

/// A time-filler free for one idle by day, with the person's mood for it and its walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filler {
    pub object: u16,
    pub offer: u16,
    pub mood: i8,
    pub walk_secs: u32,
}

/// The time-fillers free for one idle by day (ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length): the `fallback` offers open at `age` of the objects not in `busy`, each
/// with the person's mood for it in the second of the choice, `moods(object, offer)` (the same
/// draw its points take), and its walk from `room`; the highest mood first, then the shortest
/// walk, then the smaller object and the smaller offer. Today's departure closes none of them.
pub fn fillers(
    (house, data): (&House, &HouseData),
    (age, room): (u8, u8),
    busy: &[u16],
    moods: impl Fn(u16, u16) -> i8,
) -> Vec<Filler> {
    let mut free = Vec::new();
    for (object, _) in (0_u16..).zip(&house.def().objects) {
        if busy.contains(&object) {
            continue;
        }
        let kind = kind_of(house, data, object);
        let walk_secs = house.walk_to(room, object);
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            if !def.fallback || def.sleep || !(def.min_age..=def.max_age).contains(&age) {
                continue;
            }
            free.push(Filler {
                object,
                offer,
                mood: moods(object, offer),
                walk_secs,
            });
        }
    }
    free.sort_by_key(|filler| {
        (
            core::cmp::Reverse(filler.mood),
            filler.walk_secs,
            filler.object,
            filler.offer,
        )
    });
    free
}

/// The time-filler of one idle by day (ADR-0026, addendum of 2026-11-05 on the time-filler's
/// length): the first of `fillers`, the one with the highest mood; none if no such offer is free
/// or at an age without needs.
pub fn filler(
    (house, data): (&House, &HouseData),
    (age, room): (u8, u8),
    busy: &[u16],
    moods: impl Fn(u16, u16) -> i8,
) -> Option<(u16, u16)> {
    age_band(age)?;
    let first = fillers((house, data), (age, room), busy, moods)
        .into_iter()
        .next()?;
    Some((first.object, first.offer))
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
/// that raises a need critical now, if any; else the best free offer that fills hunger or the
/// bladder while that need is under its comfort (`comforts`, by its points without the critical
/// part; its addendum of 2026-11-05 on the time-filler's length); else the best free bed of the
/// tier; else the best free bed, any furniture but the sofa that sleeps, regardless of `ENOUGH`;
/// else the best free sleeping place on the sofa (ADR-0025: who finds no free bed sleeps on the
/// sofa); else waiting for the best bed with no free place; else idle. The mood ranks within
/// each of these alone. Ties as in `best`.
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
    // Before the bed, what fills hunger or the bladder under its comfort, ranked without the
    // part a critical need adds (the critical ones came first).
    let comforting: Vec<Scored> = (scored.iter())
        .filter(|scored| comforts(offer_of(house, data, scored), rates, (age, values)))
        .map(|scored| Scored {
            points: scored.points
                - urgent_part(offer_of(house, data, scored), rates, (age, values)),
            ..*scored
        })
        .collect();
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
        comforting,
        among(&tiered),
        among(&sleeps(false)),
        among(&sleeps(true)),
    ]
    .into_iter()
    .find_map(|candidates| best(&candidates, busy));
    if let Some(chosen) = first_free {
        // The points as scored, with the critical part.
        let scored = (scored.iter())
            .find(|scored| (scored.object, scored.offer) == (chosen.object, chosen.offer));
        return Choice::Take(scored.copied().unwrap_or(chosen));
    }
    let beds: Vec<Scored> = (among(&sleeps(false)).into_iter())
        .filter(|scored| busy.contains(&scored.object))
        .collect();
    match best(&beds, &[]) {
        Some(waited) => Choice::Wait(waited.object),
        None => Choice::Idle,
    }
}

/// The needs the night's choice looks after before the bed (ADR-0026, addendum of 2026-11-05 on
/// the time-filler's length): hunger and the bladder, not hygiene.
pub const BEFORE_BED: [Need; 2] = [Need::Hunger, Need::Bladder];

/// Whether offer `def`, open at `age`, fills a need of `BEFORE_BED` that stands under its comfort
/// in `values` (ADR-0026, addendum of 2026-11-05 on the time-filler's length): no offer of sleep,
/// and its gain for that need over its length (`score`) is above 0, so a choice of it brings the
/// need up.
pub fn comforts(def: &OfferDef, rates: &NeedRates, (age, values): (u8, [u16; 5])) -> bool {
    let (Some(band), false) = (age_band(age), def.sleep) else {
        return false;
    };
    if !(def.min_age..=def.max_age).contains(&age) {
        return false;
    }
    let length = def.seconds.or(def.max_seconds);
    let length = i64::from(length.unwrap_or_else(|| panic!("invariant house.offer_length")));
    (NEEDS.into_iter().zip(values))
        .filter(|(need, _)| BEFORE_BED.contains(need))
        .any(|(need, value)| {
            let rate = i64::from(def.rate(rates, need, band));
            let missing = i64::from(FULL.saturating_sub(value));
            let gain = (rate * length / HOUR.as_secs()).min(missing);
            value < rates.comfort(need) && rate > 0 && gain > 0
        })
}

/// The part of the points of offer `def` for the needs `values` at `age` that critical needs add
/// (`score`): `(151 − v) · 10⁹` for each need at or below `CRITICAL` it raises.
pub fn urgent_part(def: &OfferDef, rates: &NeedRates, (age, values): (u8, [u16; 5])) -> i64 {
    let Some(band) = age_band(age) else {
        return 0;
    };
    (NEEDS.into_iter().zip(values))
        .filter(|&(need, value)| value <= CRITICAL && def.rate(rates, need, band) > 0)
        .map(|(_, value)| i64::from(CRITICAL + 1 - value) * PER_CRITICAL_POINT)
        .sum()
}

/// The second an idle person chooses next (ADR-0026, addendum of 2026-11-04 after the
/// playtest), born at `born`, with the needs `needs` standing in `room` and the objects for
/// which `free` holds having a free place, from `start` on, planned in the second `after` (at
/// most `start`): the first second at which an offer of one of these objects brings `ENOUGH`
/// with the person's mood of that second, `moods(second, object, offer)`. Per offer only whole
/// minutes are weighed, from the first second it brings `ENOUGH` at the most mood
/// (`choice_mood_percent`) to the first it surely does at the least, which counts if no minute
/// before it does; the earliest over the offers. Not at or after the person's next bedtime, which
/// makes it choose anyway; its next birthday, where the offers and their rates may change, comes
/// first if it is earlier. An offer is weighed only before the first second at which it could
/// still run at today's departure (`crosses_departure`), `departure(start)` the departure of the
/// day planned last at or before the search's start; a need it raises turning critical makes the
/// person choose at its crossing anyway. The plan's second is `after`, but never before the
/// second before the search's start, and the bedtime, the birthday and the departure are those
/// after it: a loaded world plans in its clock what the run planned in its second, and both see
/// the same bounds. A search that would start in a night looks from the night's end on if the
/// plan's second lies in that night; if the night begins after it, its start is the bedtime,
/// which comes first, and nothing is planned (ADR-0026, addendum of 2026-11-04 on comfort and the
/// time-filler). `None` if nothing comes, or beyond the end of time. The needs keep the rates of
/// their samples; the sample of a need that rises is weighed at its value at `start` for the most
/// and full for the least, so both bounds only grow with time. Every offer is weighed with bonus
/// 0. The idle plan subtracts avoid as well ([`next_choice_drag`]); with avoid 0 the two agree.
pub fn next_choice(
    (house, data): (&House, &HouseData),
    (born, needs, room): (SimTime, &[NeedSample; 5], u8),
    free: impl Fn(u16) -> bool,
    (start, after): (SimTime, SimTime),
    moods: impl Fn(SimTime, u16, u16) -> i8,
    departure: impl Fn(SimTime) -> Option<SimTime>,
) -> Option<SimTime> {
    next_choice_drag(
        (house, data),
        (born, needs, room),
        free,
        (start, after),
        moods,
        departure,
        |_, _| 0,
    )
}

/// As [`next_choice`], and the idle plan when someone is avoided (ADR-0030, addendum of
/// 2026-11-13). `drag(Some(second), object)` is `min(0, scaled company − avoid)` at the second
/// being weighed, never above the net the choice itself adds. `drag(None, object)` is the
/// smallest drag in the window (company 0, avoid in full). The two bounds stay monotone and are
/// bisected ([`first_at`]): `t+` with bonus 0, as [`next_choice`], and `t−` with that smallest
/// drag and the least mood. Between them every whole minute is weighed on its own, with that
/// minute's mood and `drag(Some(minute), object)`, and the first that brings `ENOUGH` wins; no
/// minute before `t−` leaves `t−`. A score that is enough with the drag is enough in the choice.
/// With avoid 0, `drag` is 0 and this is [`next_choice`].
pub(crate) fn next_choice_drag(
    (house, data): (&House, &HouseData),
    (born, needs, room): (SimTime, &[NeedSample; 5], u8),
    free: impl Fn(u16) -> bool,
    (start, after): (SimTime, SimTime),
    moods: impl Fn(SimTime, u16, u16) -> i8,
    departure: impl Fn(SimTime) -> Option<SimTime>,
    drag: impl Fn(Option<i64>, u16) -> i64,
) -> Option<SimTime> {
    let rules = data.day();
    let planned = |start: SimTime| {
        let before = (start.as_secs().checked_sub(1)).and_then(|secs| SimTime::try_from(secs).ok());
        before.map_or(after, |before| after.max(before))
    };
    let start = match night(rules, born, start) {
        // A night that begins after the plan's second: its bedtime comes first.
        Some(night) if night.from > planned(start) => return None,
        Some(night) => night.until?,
        None => start,
    };
    let after = planned(start);
    let depart = departure(start).filter(|&depart| depart > after);
    let age = age(born, after)?;
    let band = age_band(age)?;
    let bedtime = next_night(rules, born, after).map(|night| night.from);
    let birthday = (born.as_secs())
        .checked_add(YEAR.as_secs() * (i64::from(age) + 1))
        .and_then(|secs| SimTime::try_from(secs).ok());
    let end = MAX_ABS_SECS + 1;
    let bound = |at: Option<SimTime>| at.map_or(end, SimTime::as_secs);
    let limit = bound(bedtime).min(bound(birthday));
    let percent =
        i8::try_from(rules.choice_mood_percent).unwrap_or_else(|_| panic!("invariant house.mood"));
    let values_at = |at: i64| needs.map(|need| need.value_at(SimTime::from_secs(at)));
    let mut first: Option<i64> = None;
    for (object, _) in (0_u16..).zip(&house.def().objects) {
        if !free(object) {
            continue;
        }
        let kind = kind_of(house, data, object);
        let walk_secs = house.walk_to(room, object);
        let walk = i64::from(walk_secs) * PER_WALKING_SECOND;
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            // By day an offer of sleep is closed, and the search ends before the night.
            if def.sleep || !(def.min_age..=def.max_age).contains(&age) {
                continue;
            }
            let length = i64::from(def.seconds.or(def.max_seconds).unwrap_or_else(|| {
                panic!("invariant house.offer_length");
            }));
            let weighed = (data.rates(), band, length);
            let reaches = |at: i64, mood: i8, bound: Option<u16>, bonus: i64| {
                let at = SimTime::from_secs(at);
                let values = core::array::from_fn(|n| match (needs[n].rate() > 0, bound) {
                    (true, Some(value)) => value,
                    (true, None) => needs[n].value_at(start),
                    (false, _) => needs[n].value_at(at),
                });
                points(def, weighed, values, walk, mood, bonus) >= ENOUGH
            };
            // Only before the departure closes it; it runs longer the later it is chosen.
            let closes = |at: i64| {
                let runs = runs_for(def, data.rates(), band, values_at(at), walk_secs);
                depart
                    .zip(runs)
                    .is_some_and(|(depart, runs)| runs >= depart.as_secs() - at)
            };
            let open = first_at(start.as_secs(), limit, closes).unwrap_or(limit);
            let until = first.unwrap_or(limit).min(limit).min(open);
            // Bonus 0: company never pulls a choice earlier, and the bound only grows.
            let most = |at: i64| reaches(at, percent, None, 0);
            let Some(from) = first_at(start.as_secs(), until, most) else {
                continue;
            };
            // Company 0 and avoid in full, with the least mood: still monotone, so bisected.
            let floor = drag(None, object);
            let least = |at: i64| reaches(at, -percent, Some(FULL), floor);
            let surely = first_at(from, until, least);
            let mut minute = from.div_euclid(MINUTE.as_secs()) * MINUTE.as_secs();
            if minute < from {
                minute += MINUTE.as_secs();
            }
            let mut found = surely;
            while minute < surely.unwrap_or(until) {
                let mood = moods(SimTime::from_secs(minute), object, offer);
                if points(
                    def,
                    weighed,
                    values_at(minute),
                    walk,
                    mood,
                    drag(Some(minute), object),
                ) >= ENOUGH
                {
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
