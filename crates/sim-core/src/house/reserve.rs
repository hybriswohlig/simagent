//! The house's reserve at hunger 0 (ADR-0026, addendum of 2026-11-06 on hunger at 0): an
//! integer sample like a need, sized once when a person's needs begin by the `body_fat_z` of its
//! body, draining only while hunger stays at 0 and the person is home, refilling only while an
//! offer raises hunger. It is no sixth need and adds nothing to a choice. Half or below it the
//! person is weak, at 0 wasted: walks take twice as long, commands only feed it, and energy falls
//! faster from the next change of its rates. Nothing of it reaches the dynasty (ADR-0024
//! section 3).

use serde::{Deserialize, Serialize};

use crate::house::need::{EMPTY, NeedSample};
use crate::time::{SimDuration, SimTime};

const HOUR_SECS: i128 = 3600;

/// The values of the reserve, per age band of `house::data::AGE_BANDS` where they differ (ADR-0026, addendum
/// of 2026-11-06 on hunger at 0): the capacity of a thin, an ordinary and a heavy body; the drain
/// per hour while hunger is at 0, awake and asleep (lying on the floor counts as asleep); the
/// refill per hour while an offer raises hunger; the extra of the energy rate when weak (half or
/// below) and when wasted (at 0); and how many times a walk's seconds a weak person takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveRules {
    pub capacity: [u16; 3],
    pub drain: Drain,
    pub refill: i32,
    pub weak_energy: i32,
    pub wasted_energy: i32,
    pub walk_factor: u32,
}

/// The drain of the reserve per hour and age band, awake and asleep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
    pub awake: [i32; 4],
    pub asleep: [i32; 4],
}

/// How a person stands with its reserve: hunger is at 0 and does not rise, and the reserve is
/// above half (`Draws`, "zehrt am Vorrat"), at half or below and above 0 (`Weak`,
/// "geschwächt"), or at 0 (`Wasted`, "ausgezehrt"). A code of the house answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Draws,
    Weak,
    Wasted,
}

impl Band {
    /// Whether a walk takes longer and a command only feeds.
    pub fn weak(self) -> bool {
        matches!(self, Band::Weak | Band::Wasted)
    }
}

/// A person's reserve: `value` at second `at`, changing by `rate` per hour until the next sample,
/// held between 0 and `capacity`; `energy` the extra the person's energy rate carries since it
/// was written down last (0, or the weak or wasted extra of the rules). In a save the value may
/// come above the capacity; the loader refuses that (`house::state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reserve {
    capacity: u16,
    value: u16,
    at: SimTime,
    rate: i32,
    energy: i32,
}

impl Reserve {
    /// A full reserve of `capacity` from `at` on, standing still, the energy rate without extra.
    pub const fn full(capacity: u16, at: SimTime) -> Reserve {
        Reserve {
            capacity,
            value: capacity,
            at,
            rate: 0,
            energy: 0,
        }
    }

    /// Panics with `house.reserve_value` above the capacity.
    #[track_caller]
    pub fn new(capacity: u16, (value, at, rate): (u16, SimTime, i32), energy: i32) -> Reserve {
        assert!(value <= capacity, "invariant house.reserve_value");
        Reserve {
            capacity,
            value,
            at,
            rate,
            energy,
        }
    }

    pub fn capacity(self) -> u16 {
        self.capacity
    }

    pub fn value(self) -> u16 {
        self.value
    }

    pub fn at(self) -> SimTime {
        self.at
    }

    pub fn rate(self) -> i32 {
        self.rate
    }

    /// The extra of the energy rate.
    pub fn energy(self) -> i32 {
        self.energy
    }

    /// The level of the weak band: half the capacity, rounded down.
    pub fn half(self) -> u16 {
        self.capacity / 2
    }

    /// `clamp(value + rate · s / 3600, 0, capacity)` with `s` the seconds since `at`, the
    /// division truncating towards zero, as a need's (`NeedSample::value_at`). Panics with
    /// `house.reserve_time` before `at`.
    #[track_caller]
    pub fn value_at(self, t: SimTime) -> u16 {
        let secs = (t - self.at).as_secs();
        assert!(secs >= 0, "invariant house.reserve_time");
        let moved = i128::from(self.rate) * i128::from(secs) / HOUR_SECS;
        let value = (i128::from(self.value) + moved).clamp(0, i128::from(self.capacity));
        u16::try_from(value).unwrap_or(self.capacity)
    }

    /// The reserve from `t` on with a new rate: the value is written down at `t` first.
    #[track_caller]
    pub fn with_rate(self, t: SimTime, rate: i32) -> Reserve {
        Reserve {
            value: self.value_at(t),
            at: t,
            rate,
            ..self
        }
    }

    /// The reserve with the energy extra `energy`.
    pub fn with_energy(self, energy: i32) -> Reserve {
        Reserve { energy, ..self }
    }

    /// The first second at which the value is at or below `level`:
    /// `at + ⌈3600 · (value − level) / |rate|⌉`. None if the value is already there, does not
    /// fall, or gets there only after the end of time.
    pub fn crossing(self, level: u16) -> Option<SimTime> {
        if self.rate >= 0 || self.value <= level {
            return None;
        }
        let distance = i64::from(self.value - level) * 3600;
        let speed = i64::from(self.rate).abs();
        self.at
            .checked_add(SimDuration::from_secs((distance + speed - 1) / speed))
    }

    /// The crossings of half and of 0 below the value while it falls, half first.
    pub fn crossings(self) -> impl Iterator<Item = (u16, SimTime)> {
        [self.half(), EMPTY]
            .into_iter()
            .filter_map(move |level| self.crossing(level).map(|at| (level, at)))
    }

    /// The band at `t` of a person with this reserve and the hunger `hunger`: none unless hunger
    /// is at 0 and does not rise; then by the value, at 0 wasted, at half or below weak, else
    /// drawing on it.
    pub fn band(self, hunger: NeedSample, t: SimTime) -> Option<Band> {
        if hunger.rate() > 0 || hunger.value_at(t) != EMPTY {
            return None;
        }
        Some(self.band_of(self.value_at(t)))
    }

    /// The band of the reserve at `value` while hunger is at 0: at 0 wasted, at half or below
    /// weak, else drawing on it.
    pub fn band_of(self, value: u16) -> Band {
        if value == EMPTY {
            Band::Wasted
        } else if value <= self.half() {
            Band::Weak
        } else {
            Band::Draws
        }
    }
}

/// What a person does, as far as the reserve's rate goes: away or on the way out, eating (an
/// offer raises hunger), asleep (a sleeping offer at its object, or lying on the floor), and
/// whether hunger is at 0 now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Doing {
    pub out: bool,
    pub eating: bool,
    pub asleep: bool,
    pub empty: bool,
}

/// The most times a walk's seconds a weak person may take.
const MOST_WALK_FACTOR: u32 = 10;

impl ReserveRules {
    /// Whether the values hold together (`house.day_reserve`): the capacities rise from at least 2,
    /// so each half lies above 0; the drain falls in every band, awake and asleep; the refill
    /// rises; the energy extras are at most 0, the wasted one at most the weak one; the walk
    /// factor lies from 1 to 10.
    pub(crate) fn valid(&self) -> bool {
        let [thin, ordinary, heavy] = self.capacity;
        let sizes = 2 <= thin && thin < ordinary && ordinary < heavy;
        let drains = (self.drain.awake.iter().chain(&self.drain.asleep)).all(|&rate| rate < 0);
        let extras = self.wasted_energy <= self.weak_energy && self.weak_energy <= 0;
        let walks = (1..=MOST_WALK_FACTOR).contains(&self.walk_factor);
        sizes && drains && self.refill > 0 && extras && walks
    }

    /// The capacity of the size `size` (0 thin, 1 ordinary, 2 heavy).
    pub fn capacity_of(&self, size: usize) -> u16 {
        self.capacity[size]
    }

    /// The reserve's rate in the age band `band` for a person doing `doing` with the reserve at
    /// `value` of `capacity`: out 0; eating the refill, 0 once full; hunger at 0 the drain,
    /// awake or asleep; else 0, so above 0 it holds still.
    pub fn rate(&self, doing: Doing, band: usize, (value, capacity): (u16, u16)) -> i32 {
        match doing {
            Doing { out: true, .. } => 0,
            Doing { eating: true, .. } if value < capacity => self.refill,
            Doing { eating: true, .. } => 0,
            Doing {
                empty: true,
                asleep: true,
                ..
            } => self.drain.asleep[band],
            Doing { empty: true, .. } => self.drain.awake[band],
            Doing { .. } => 0,
        }
    }

    /// The extra of the energy rate in the band `band`: weak `weak_energy`, wasted
    /// `wasted_energy` (instead, not both), else 0.
    pub fn energy(&self, band: Option<Band>) -> i32 {
        match band {
            Some(Band::Weak) => self.weak_energy,
            Some(Band::Wasted) => self.wasted_energy,
            Some(Band::Draws) | None => 0,
        }
    }

    /// The seconds a walk of `secs` takes a person in the band `band`: `walk_factor` times as
    /// many when weak or wasted, as an integer.
    #[track_caller]
    pub fn walk(&self, secs: u32, band: Option<Band>) -> u32 {
        match band {
            Some(band) if band.weak() => (secs.checked_mul(self.walk_factor))
                .unwrap_or_else(|| panic!("invariant house.walk_secs")),
            _ => secs,
        }
    }
}

#[cfg(test)]
mod tests {
    // The expected values come from the decision (ADR-0026, addendum of 2026-11-06 on hunger at
    // 0), by hand: the value `clamp(value + rate · s / 3600, 0, capacity)` truncated towards zero,
    // the crossing `at + ⌈3600 · (value − level) / |rate|⌉`.

    use alloc::vec::Vec;

    use super::*;

    /// The values of the decision, as `data/house/day.json` holds them.
    const RULES: ReserveRules = ReserveRules {
        capacity: [500, 1000, 1500],
        drain: Drain {
            awake: [-80, -60, -40, -50],
            asleep: [-32, -24, -16, -20],
        },
        refill: 960,
        weak_energy: -40,
        wasted_energy: -80,
        walk_factor: 2,
    };

    #[test]
    fn the_rules_of_the_day_file_are_the_decision() {
        let day: crate::house::data::DayRules =
            serde_json::from_str(include_str!("../../../../data/house/day.json")).expect("the day");
        assert_eq!(day.reserve, RULES);
        assert!(RULES.valid());
        let changed = |change: fn(&mut ReserveRules)| {
            let mut rules = RULES.clone();
            change(&mut rules);
            rules.valid()
        };
        assert!(!changed(|rules| rules.capacity = [1, 1000, 1500]));
        assert!(!changed(|rules| rules.capacity = [500, 500, 1500]));
        assert!(!changed(|rules| rules.drain.asleep[3] = 0));
        assert!(!changed(|rules| rules.refill = 0));
        assert!(!changed(|rules| rules.weak_energy = 1));
        assert!(!changed(|rules| rules.wasted_energy = -39));
        assert!(!changed(|rules| rules.walk_factor = 0));
        assert!(!changed(|rules| rules.walk_factor = 11));
        assert!(changed(|rules| rules.walk_factor = 1));
    }

    const fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    /// Hunger at 0 and falling, as while the reserve drains.
    fn starving(at: i64) -> NeedSample {
        NeedSample::new(EMPTY, t(at), -100)
    }

    #[test]
    fn an_ordinary_adult_awake_is_weak_after_12_h_30_and_wasted_after_25_h() {
        // 1 000 at −40: half after 45 000 s, 0 after 90 000 s.
        let reserve = Reserve::full(1000, t(0)).with_rate(t(0), RULES.drain.awake[2]);
        assert_eq!(reserve.crossing(500), Some(t(45_000)));
        assert_eq!(reserve.crossing(0), Some(t(90_000)));
        assert_eq!(reserve.value_at(t(44_999)), 501);
        assert_eq!(reserve.value_at(t(45_000)), 500);
        assert_eq!(reserve.value_at(t(89_999)), 1);
        assert_eq!(reserve.value_at(t(90_000)), 0);
        let all: Vec<(u16, SimTime)> = reserve.crossings().collect();
        assert_eq!(all, [(500, t(45_000)), (0, t(90_000))]);
    }

    #[test]
    fn a_thin_child_awake_is_weak_after_11_250_s_and_wasted_after_22_500_s() {
        let reserve = Reserve::full(RULES.capacity[0], t(7)).with_rate(t(7), RULES.drain.awake[0]);
        assert_eq!(reserve.half(), 250);
        assert_eq!(reserve.crossing(250), Some(t(7 + 11_250)));
        assert_eq!(reserve.crossing(0), Some(t(7 + 22_500)));
    }

    #[test]
    fn the_crossings_only_while_it_falls_and_below_the_value() {
        let falling = Reserve::new(1500, (750, t(0), -16), 0);
        // At half already: only 0, after ⌈3600 · 750 / 16⌉ = 168 750 s.
        let levels: Vec<(u16, SimTime)> = falling.crossings().collect();
        assert_eq!(levels, [(0, t(168_750))]);
        assert_eq!(Reserve::new(1500, (0, t(0), -16), 0).crossings().count(), 0);
        assert_eq!(Reserve::new(1500, (900, t(0), 0), 0).crossings().count(), 0);
        assert_eq!(
            Reserve::new(1500, (900, t(0), 960), 0).crossings().count(),
            0
        );
    }

    #[test]
    fn the_refill_restores_320_in_a_meal_and_80_in_a_snack_held_at_the_capacity() {
        let empty = Reserve::new(1000, (0, t(0), 0), 0);
        let eating = empty.with_rate(t(0), RULES.refill);
        assert_eq!(eating.value_at(t(1_200)), 320);
        assert_eq!(eating.value_at(t(300)), 80);
        // Four meals at the edge fill 1 000 from empty: 3 750 s at +960.
        assert_eq!(eating.value_at(t(3_749)), 999);
        assert_eq!(eating.value_at(t(3_750)), 1000);
        assert_eq!(eating.value_at(t(9_000)), 1000);
        // Falling below 0 holds at 0.
        assert_eq!(Reserve::new(500, (10, t(0), -80), 0).value_at(t(3_600)), 0);
    }

    #[test]
    fn a_new_rate_writes_the_value_down_first() {
        let reserve = Reserve::new(1000, (800, t(0), -40), -40).with_rate(t(1_800), -16);
        assert_eq!(reserve, Reserve::new(1000, (780, t(1_800), -16), -40));
        assert_eq!(reserve.with_energy(-80).energy(), -80);
    }

    #[test]
    fn the_bands_by_hunger_and_the_value() {
        let reserve = Reserve::new(1000, (501, t(0), 0), 0);
        assert_eq!(reserve.band(starving(0), t(0)), Some(Band::Draws));
        let at_half = Reserve::new(1000, (500, t(0), 0), 0);
        assert_eq!(at_half.band(starving(0), t(0)), Some(Band::Weak));
        let one = Reserve::new(1000, (1, t(0), 0), 0);
        assert_eq!(one.band(starving(0), t(0)), Some(Band::Weak));
        let none = Reserve::new(1000, (0, t(0), 0), 0);
        assert_eq!(none.band(starving(0), t(0)), Some(Band::Wasted));
        // Hunger above 0, or rising from 0 in the first seconds of a meal: no band.
        assert_eq!(none.band(NeedSample::new(1, t(0), -100), t(0)), None);
        assert_eq!(none.band(NeedSample::new(0, t(0), 1_800), t(0)), None);
        // Away hunger stands still at 0: the band stays.
        assert_eq!(
            none.band(NeedSample::new(0, t(0), 0), t(9)),
            Some(Band::Wasted)
        );
        // A capacity of 1 500 is weak from 750, one of 500 from 250.
        assert_eq!(Reserve::new(1500, (750, t(0), 0), 0).half(), 750);
        assert_eq!(
            Reserve::new(500, (251, t(0), 0), 0).band(starving(0), t(0)),
            Some(Band::Draws)
        );
    }

    #[test]
    fn the_rate_by_what_the_person_does() {
        let doing = |out, eating, asleep, empty| Doing {
            out,
            eating,
            asleep,
            empty,
        };
        // Hunger at 0: the drain of each band, asleep two fifths of awake.
        for (band, (awake, asleep)) in [(-80, -32), (-60, -24), (-40, -16), (-50, -20)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                RULES.rate(doing(false, false, false, true), band, (900, 1000)),
                awake
            );
            assert_eq!(
                RULES.rate(doing(false, false, true, true), band, (900, 1000)),
                asleep
            );
        }
        // Out, even with hunger at 0: still. Above 0: still.
        assert_eq!(
            RULES.rate(doing(true, false, false, true), 2, (900, 1000)),
            0
        );
        assert_eq!(
            RULES.rate(doing(false, false, false, false), 2, (900, 1000)),
            0
        );
        // Eating: +960 below the capacity, 0 once full.
        assert_eq!(
            RULES.rate(doing(false, true, false, false), 0, (999, 1000)),
            960
        );
        assert_eq!(
            RULES.rate(doing(false, true, false, false), 0, (1000, 1000)),
            0
        );
    }

    #[test]
    fn the_energy_extra_and_the_walk() {
        assert_eq!(RULES.energy(None), 0);
        assert_eq!(RULES.energy(Some(Band::Draws)), 0);
        assert_eq!(RULES.energy(Some(Band::Weak)), -40);
        assert_eq!(RULES.energy(Some(Band::Wasted)), -80);
        // An adult awake at −40 falls 120 an hour wasted; a teen asleep at +70 loses 10; a child
        // at +80 holds still.
        assert_eq!(-40 + RULES.energy(Some(Band::Wasted)), -120);
        assert_eq!(70 + RULES.energy(Some(Band::Wasted)), -10);
        assert_eq!(80 + RULES.energy(Some(Band::Wasted)), 0);
        assert_eq!(RULES.walk(13, None), 13);
        assert_eq!(RULES.walk(13, Some(Band::Draws)), 13);
        assert_eq!(RULES.walk(13, Some(Band::Weak)), 26);
        assert_eq!(RULES.walk(13, Some(Band::Wasted)), 26);
    }

    #[test]
    #[should_panic(expected = "invariant house.reserve_value")]
    fn a_value_above_the_capacity_breaks_an_invariant() {
        let _ = Reserve::new(500, (501, t(0), 0), 0);
    }
}
