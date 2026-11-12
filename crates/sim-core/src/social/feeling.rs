//! The feelings of a person (ADR-0032): four channels, joy, anger, sadness and fear, each 0 to
//! 1000, and a slow temper from -1000 to +1000. Everything is analytic and in integers, `i64`
//! with the division toward 0; nothing ticks and there is no event. A channel falls linearly per
//! day from the second it was written down, the temper returns to 0 per day. An occasion reads
//! all five values at its second, adds its steps, clamps once and writes only if something
//! changed. The state side (`state::feeling`) finds the occasions and writes the entries; this
//! module is the pure part: the values at a second, the change by an occasion, the contagion of
//! anger and the temper's scale on the choice's company.

use serde::{Deserialize, Serialize};

use crate::social::data::{Band, Contagion, FeelingRules};
use crate::time::{DAY, SimTime};

/// The index of joy in the four channels.
pub const JOY: usize = 0;
/// The index of anger.
pub const ANGER: usize = 1;
/// The index of sadness.
pub const SADNESS: usize = 2;
/// The index of fear.
pub const FEAR: usize = 3;

/// The most a channel can be (ADR-0032 section 1).
pub const MOST: u16 = 1000;
/// The most the temper can be, up or down (ADR-0032 section 2).
pub const TEMPER_MOST: i16 = 1000;

/// The stored feelings of a person: the four channels as they were at `since`, and the temper as
/// it was at `temper_since` (ADR-0032 sections 1 and 2: the channels share one `since`, the temper
/// has its own). Every write sets both to the same second, so they are equal wherever the core
/// writes; they are two fields because the ADR gives each value its own second. An entry whose
/// five values are 0 is not stored: a person without one has all feelings at 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feel {
    pub channels: [u16; 4],
    pub since: SimTime,
    pub temper: i16,
    pub temper_since: SimTime,
}

/// One row an occasion brings for one person: the steps of the table on joy, anger, sadness and
/// fear, and the affection of the person to the one the row's band names, if the row has a band
/// and the person has a bond to it (ADR-0032 section 3). Without one the factor is 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub steps: [i16; 4],
    pub affection: Option<u16>,
}

/// `max(0, w - (rate · max(0, t - since)) / 86 400)` in `i64`, the division toward 0 (ADR-0032
/// section 1).
pub fn channel_at(w: u16, rate: u16, since: SimTime, t: SimTime) -> u16 {
    let elapsed = (t.as_secs() - since.as_secs()).max(0);
    let lost = i64::from(rate).saturating_mul(elapsed) / DAY.as_secs();
    let left = (i64::from(w) - lost).max(0);
    u16::try_from(left).unwrap_or_else(|_| panic!("invariant feeling.channel"))
}

/// `|s(t)| = max(0, |s| - (mood_return · max(0, t - since)) / 86 400)` with the sign of `s`
/// (ADR-0032 section 2). At magnitude 0 the sign does not matter: the result is 0.
pub fn temper_at(s: i16, mood_return: u16, since: SimTime, t: SimTime) -> i16 {
    let elapsed = (t.as_secs() - since.as_secs()).max(0);
    let lost = i64::from(mood_return).saturating_mul(elapsed) / DAY.as_secs();
    let left = (i64::from(s).abs() - lost).max(0);
    let left = i16::try_from(left).unwrap_or_else(|_| panic!("invariant feeling.temper"));
    if s < 0 { -left } else { left }
}

impl Feel {
    /// The four channels and the temper read at `t`.
    pub fn at(&self, t: SimTime, rules: &FeelingRules) -> ([u16; 4], i16) {
        let rates = rules.rates.all();
        let channels =
            core::array::from_fn(|c| channel_at(self.channels[c], rates[c], self.since, t));
        (
            channels,
            temper_at(self.temper, rules.mood_return, self.temper_since, t),
        )
    }

    /// Whether all five values are 0 as stored (such an entry is not kept).
    pub fn is_zero(&self) -> bool {
        self.channels == [0; 4] && self.temper == 0
    }
}

/// The four channels and the temper of a person at `t`, from its stored feelings or all 0 for a
/// person without any.
pub fn read(stored: Option<&Feel>, t: SimTime, rules: &FeelingRules) -> ([u16; 4], i16) {
    stored.map_or(([0; 4], 0), |feel| feel.at(t, rules))
}

/// A step with the relationship band applied: `(step · numerator) / denominator` in `i64` toward
/// 0 when the affection is at least the band's threshold, also for a negative step (-200 becomes
/// -300); the step itself otherwise and without an affection (ADR-0032 section 3).
pub fn band_step(step: i16, affection: Option<u16>, band: &Band) -> i64 {
    match affection {
        Some(affection) if affection >= band.min_affection => {
            i64::from(step) * i64::from(band.numerator) / i64::from(band.denominator)
        }
        _ => i64::from(step),
    }
}

/// One occasion for one person (ADR-0032 sections 1 to 3): the channels and the temper are read
/// at `now`, each step of each row gets its band, all rows are summed, each channel is clamped
/// once to 0 to 1000, the temper moves by `(Δjoy - Δanger - Δsadness - Δfear) / 4` toward 0 of the
/// clamped changes and is clamped to -1000 to 1000. Returns the feelings to store, written at
/// `now`, or `None` if no clamped channel and not the temper changed: then nothing is written,
/// `since` stays, and there is no entry.
pub fn apply(
    stored: Option<&Feel>,
    now: SimTime,
    rows: &[Row],
    rules: &FeelingRules,
) -> Option<Feel> {
    let (before, temper) = read(stored, now, rules);
    let mut total = [0_i64; 4];
    for row in rows {
        for (sum, step) in total.iter_mut().zip(row.steps) {
            *sum += band_step(step, row.affection, &rules.band);
        }
    }
    let after: [u16; 4] = core::array::from_fn(|c| {
        let value = (i64::from(before[c]) + total[c]).clamp(0, i64::from(MOST));
        u16::try_from(value).unwrap_or_else(|_| panic!("invariant feeling.clamp"))
    });
    let delta: [i64; 4] = core::array::from_fn(|c| i64::from(after[c]) - i64::from(before[c]));
    let step = (delta[JOY] - delta[ANGER] - delta[SADNESS] - delta[FEAR]) / 4;
    let moved = (i64::from(temper) + step).clamp(-i64::from(TEMPER_MOST), i64::from(TEMPER_MOST));
    let moved = i16::try_from(moved).unwrap_or_else(|_| panic!("invariant feeling.temper"));
    if after == before && moved == temper {
        return None;
    }
    Some(Feel {
        channels: after,
        since: now,
        temper: moved,
        temper_since: now,
    })
}

/// The anger a person gets from the push of someone who arrives with `arriver_anger` (ADR-0032
/// section 4): with the arriver's anger at least `min_anger`, a person whose anger is below it
/// gets `max(anger, min(anger + step, cap))`; one at `min_anger` or above, and everyone for a
/// calmer arriver, keeps theirs. `None` for no push at all. The cap is below `min_anger`, so
/// nobody a push reaches can push on. Anger never falls, even when `cap` is below it.
pub fn infected(arriver_anger: u16, anger: u16, rules: &Contagion) -> Option<u16> {
    if arriver_anger < rules.min_anger {
        return None;
    }
    if anger >= rules.min_anger {
        return Some(anger);
    }
    Some(anger.max(anger.saturating_add(rules.step).min(rules.cap)))
}

/// `company · (1000 + temper) / 1000` in `i64` toward 0 (ADR-0032 section 5): at -1000 the
/// company is gone, at +1000 it doubles.
pub fn scaled_company(company: i64, temper: i16) -> i64 {
    company * (1000 + i64::from(temper)) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::data::SocialData;

    fn rules() -> FeelingRules {
        let raw: SocialData =
            serde_json::from_str(include_str!("../../../../data/social/social.json"))
                .expect("the file");
        raw.checked().expect("checked").feelings
    }

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    const DAY: i64 = 86_400;

    fn row(steps: [i16; 4]) -> Row {
        Row {
            steps,
            affection: None,
        }
    }

    /// Acceptance 1: joy 800 at 300 per day is 0 at 800 · 86 400 / 300 = 230 400, 1 a second before.
    #[test]
    fn a_channel_falls_linearly_to_zero() {
        assert_eq!(800 * DAY / 300, 230_400);
        assert_eq!(channel_at(800, 300, t(0), t(230_400)), 0);
        assert_eq!(channel_at(800, 300, t(0), t(230_399)), 1);
        // No fade before `since`, shifted `since`, never below 0, the other rates, truncation.
        assert_eq!(channel_at(800, 300, t(500), t(100)), 800);
        assert_eq!(channel_at(800, 300, t(1000), t(1000 + 230_400)), 0);
        assert_eq!(channel_at(800, 300, t(0), t(400 * DAY)), 0);
        assert_eq!(channel_at(500, 150, t(0), t(DAY)), 350);
        assert_eq!(channel_at(500, 400, t(0), t(DAY)), 100);
        assert_eq!(channel_at(500, 400, t(0), t(100)), 500);
        assert_eq!(channel_at(500, 400, t(0), t(216)), 499);
    }

    /// Acceptance 2: a birth is joy +800 and temper +200; the temper is 1 a second before two
    /// days and 0 after. The temper keeps its sign on the way back.
    #[test]
    fn the_temper_returns_to_zero_with_its_sign() {
        assert_eq!(temper_at(200, 100, t(0), t(2 * DAY - 1)), 1);
        assert_eq!(temper_at(200, 100, t(0), t(2 * DAY)), 0);
        assert_eq!(temper_at(200, 100, t(0), t(2 * DAY + 5000)), 0);
        assert_eq!(temper_at(-500, 100, t(0), t(DAY)), -400);
        assert_eq!(temper_at(-500, 100, t(0), t(4 * DAY)), -100);
        assert_eq!(temper_at(-500, 100, t(0), t(5 * DAY)), 0);
        assert_eq!(temper_at(-500, 100, t(0), t(400 * DAY)), 0);
        assert_eq!(temper_at(500, 100, t(0), t(DAY)), 400);
        assert_eq!(temper_at(-37, 100, t(0), t(21_600)), -12);
        assert_eq!(temper_at(-37, 100, t(0), t(43_200)), 0);
        assert_eq!(temper_at(1000, 10_000, t(0), t(8640)), 0);
        assert_eq!(temper_at(-200, 100, t(100), t(50)), -200);
    }

    #[test]
    fn a_birth_is_joy_800_and_temper_200() {
        let rules = rules();
        let birth = rules.steps(crate::social::data::Occasion::Birth);
        assert_eq!(birth, [800, 0, 0, 0]);
        let mother = apply(None, t(0), &[row(birth)], &rules).expect("written");
        assert_eq!(
            (
                mother.channels,
                mother.temper,
                mother.since,
                mother.temper_since
            ),
            ([800, 0, 0, 0], 200, t(0), t(0))
        );
        assert_eq!(mother.at(t(2 * DAY - 1), &rules).1, 1);
        assert_eq!(mother.at(t(2 * DAY), &rules).1, 0);
    }

    /// Acceptance 3 and the band.
    #[test]
    fn the_band_and_the_temper_step_of_acceptance_3() {
        let rules = rules();
        let praise = |affection| Row {
            steps: [150, 0, 0, 0],
            affection,
        };
        let lob = apply(None, t(100), &[praise(Some(640))], &rules).expect("written");
        assert_eq!((lob.channels[JOY], lob.temper), (225, 56));
        let stored = Feel {
            channels: [0, 500, 0, 0],
            since: t(0),
            temper: 0,
            temper_since: t(0),
        };
        let apology = |affection| Row {
            steps: [50, -200, 0, 0],
            affection,
        };
        let ap = apply(Some(&stored), t(0), &[apology(Some(600))], &rules).expect("written");
        assert_eq!((ap.channels, ap.temper), ([75, 200, 0, 0], 93));
        let ap = apply(Some(&stored), t(0), &[apology(Some(599))], &rules).expect("written");
        assert_eq!(ap.channels, [50, 300, 0, 0]);
        // Anger +150 alone: temper -37. A Talk with joy +50 and sadness +100: -12, summed first.
        let anger = apply(None, t(5), &[row([0, 150, 0, 0])], &rules).expect("written");
        assert_eq!((anger.channels, anger.temper), ([0, 150, 0, 0], -37));
        let talk = apply(
            None,
            t(5),
            &[row([50, 0, 0, 0]), row([0, 0, 100, 0])],
            &rules,
        );
        let talk = talk.expect("written");
        assert_eq!((talk.channels, talk.temper), ([50, 0, 100, 0], -12));
        // One by one it would be 12 and then -25: -13.
        let one = apply(None, t(5), &[row([50, 0, 0, 0])], &rules).expect("written");
        let two = apply(Some(&one), t(5), &[row([0, 0, 100, 0])], &rules).expect("written");
        assert_eq!(two.temper, 12 - 25);
        // Band edges: 600 yes, 599 no, no person no band, negative toward 0, per row before the sum.
        let band = &rules.band;
        assert_eq!(
            (
                band_step(150, Some(600), band),
                band_step(150, Some(599), band)
            ),
            (225, 150)
        );
        assert_eq!(band_step(150, None, band), 150);
        assert_eq!(
            (
                band_step(-200, Some(700), band),
                band_step(-1, Some(700), band),
                band_step(1, Some(700), band)
            ),
            (-300, -1, 1)
        );
        let sum = apply(
            None,
            t(0),
            &[
                Row {
                    steps: [150, 0, 0, 0],
                    affection: Some(700),
                },
                row([40, 0, 0, 0]),
            ],
            &rules,
        )
        .expect("written");
        assert_eq!(sum.channels[JOY], 265);
        let two_ones = Row {
            steps: [1, 0, 0, 0],
            affection: Some(700),
        };
        let ones = apply(None, t(0), &[two_ones, two_ones], &rules).expect("written");
        assert_eq!(ones.channels[JOY], 2);
    }

    /// Acceptance 6: the partner's death with an affection of 700 is sadness 1000 (clamped from
    /// 1500) and fear +300; the temper is (0 - 1000 - 300) / 4 = -325.
    #[test]
    fn the_death_of_a_partner_clamps() {
        let rules = rules();
        let steps = rules.steps(crate::social::data::Occasion::PartnerLost);
        let survivor = Row {
            steps,
            affection: Some(700),
        };
        let felt = apply(None, t(3000), &[survivor], &rules).expect("written");
        assert_eq!((felt.channels, felt.temper), ([0, 0, 1000, 300], -325));
        let kin = rules.steps(crate::social::data::Occasion::RelativeDied);
        let plain = apply(
            None,
            t(3000),
            &[Row {
                steps: kin,
                affection: Some(500),
            }],
            &rules,
        );
        assert_eq!(plain.expect("written").channels, [0, 0, 800, 100]);
        let near = apply(
            None,
            t(3000),
            &[Row {
                steps: kin,
                affection: Some(600),
            }],
            &rules,
        );
        assert_eq!(near.expect("written").channels, [0, 0, 1000, 150]);
    }

    /// The other rows of the table and the clamps.
    #[test]
    fn the_table_and_the_clamps() {
        use crate::social::data::Occasion::*;
        let rules = rules();
        let temper = |occasion| {
            apply(None, t(0), &[row(rules.steps(occasion))], &rules)
                .expect("written")
                .temper
        };
        assert_eq!(temper(NeedEmpty), -62);
        assert_eq!(temper(Separated), -225);
        assert_eq!(temper(PartnershipBegan), 150);
        // Joy at 1000 plus 800 changes nothing: no write.
        let full = Feel {
            channels: [1000, 0, 0, 0],
            since: t(0),
            temper: 0,
            temper_since: t(0),
        };
        assert_eq!(
            apply(Some(&full), t(0), &[row([800, 0, 0, 0])], &rules),
            None
        );
        let nine = Feel {
            channels: [900, 0, 0, 0],
            ..full
        };
        let nine = apply(Some(&nine), t(0), &[row([800, 0, 0, 0])], &rules).expect("written");
        assert_eq!((nine.channels[JOY], nine.temper), (1000, 25));
        // Anger 500 decays to 200 in a day, -300 clamps to 0: the delta is -200, not the step.
        let angry = Feel {
            channels: [0, 500, 0, 0],
            ..full
        };
        let calmed = apply(
            Some(&angry),
            t(DAY),
            &[Row {
                steps: [50, -200, 0, 0],
                affection: Some(700),
            }],
            &rules,
        )
        .expect("written");
        assert_eq!(
            (calmed.channels, calmed.temper, calmed.since),
            ([75, 0, 0, 0], 68, t(DAY))
        );
        // The temper clamps both ways.
        let hot = Feel {
            temper: 900,
            channels: [0; 4],
            ..full
        };
        assert_eq!(
            apply(Some(&hot), t(0), &[row([800, 0, 0, 0])], &rules)
                .expect("w")
                .temper,
            1000
        );
        let cold = Feel {
            temper: -900,
            channels: [0; 4],
            ..full
        };
        assert_eq!(
            apply(Some(&cold), t(0), &[row([0, 1000, 0, 0])], &rules)
                .expect("w")
                .temper,
            -1000
        );
        // Anger at 0 minus 200 is no change.
        assert_eq!(apply(None, t(0), &[row([0, -200, 0, 0])], &rules), None);
    }

    /// Reads are analytic at `now`; a write resets `since`; a no-change occasion keeps the old
    /// values and `since` even after a day of fading; channels that change with a temper step of 0
    /// are still a write.
    #[test]
    fn reads_are_analytic_and_a_write_resets_since() {
        let rules = rules();
        let aged = Feel {
            channels: [800, 0, 0, 0],
            since: t(1000),
            temper: 100,
            temper_since: t(1000),
        };
        let new = apply(Some(&aged), t(1000 + DAY), &[row([0, 0, 200, 0])], &rules).expect("w");
        assert_eq!(
            (new.channels, new.temper, new.since),
            ([500, 0, 200, 0], -50, t(1000 + DAY))
        );
        assert_eq!(
            apply(Some(&aged), t(1000 + DAY), &[row([0; 4])], &rules),
            None
        );
        let same = Feel {
            channels: [4, 4, 0, 0],
            since: t(0),
            temper: 0,
            temper_since: t(0),
        };
        let chan = apply(Some(&same), t(0), &[row([4, 4, 0, 0])], &rules).expect("w");
        assert_eq!((chan.channels, chan.temper), ([8, 8, 0, 0], 0));
        let still = Feel {
            channels: [0, 0, 3, 0],
            since: t(7),
            temper: 0,
            temper_since: t(7),
        };
        assert_eq!(apply(Some(&still), t(99), &[row([0; 4])], &rules), None);
        assert!(apply(Some(&still), t(99), &[row([0; 4])], &rules).is_none());
        // `read` of nothing is all 0.
        assert_eq!(read(None, t(5), &rules), ([0; 4], 0));
    }

    /// Acceptance 4: an arrival with anger 720 in a room with A (0), B (650) and C (800) gives A
    /// 150, B 699, C 800; B (now 699) infects nobody on arrival.
    #[test]
    fn contagion_is_one_push_capped_below_the_threshold() {
        let rules = rules();
        let c = &rules.contagion;
        assert_eq!(infected(720, 0, c), Some(150));
        assert_eq!(infected(720, 650, c), Some(699));
        assert_eq!(infected(720, 800, c), Some(800));
        assert_eq!(infected(699, 0, c), None);
        assert_eq!(infected(700, 0, c), Some(150));
        assert_eq!(infected(900, 699, c), Some(699));
        assert_eq!(infected(900, 550, c), Some(699));
        assert_eq!(infected(900, 700, c), Some(700));
        assert_eq!(infected(900, 1000, c), Some(1000));
        // The temper follows the delta (150 -> -37, 49 -> -12).
        let temper = |anger: u16, now: u16| {
            let stored = Feel {
                channels: [0, anger, 0, 0],
                since: t(0),
                temper: 0,
                temper_since: t(0),
            };
            let delta = i16::try_from(i32::from(now) - i32::from(anger)).expect("small");
            apply(Some(&stored), t(0), &[row([0, delta, 0, 0])], &rules)
                .expect("w")
                .temper
        };
        assert_eq!((temper(0, 150), temper(650, 699)), (-37, -12));
    }

    /// Accepted data may set `cap` under the anger a push reaches (`min_anger` 700, `step` 150,
    /// `cap` 600). ADR-0032 section 4: that push does not lower 650 to 600.
    #[test]
    fn contagion_never_lowers_anger_when_the_cap_is_below_it() {
        let rules = Contagion {
            min_anger: 700,
            step: 150,
            cap: 600,
        };
        assert_eq!(infected(900, 650, &rules), Some(650));
    }

    /// Acceptance 5: a temper of -500 halves the company, -1000 removes it, +1000 doubles it; the
    /// division goes toward 0.
    #[test]
    fn the_temper_scales_the_company() {
        assert_eq!(scaled_company(8000, -500), 4000);
        assert_eq!(scaled_company(8000, -1), 7992);
        assert_eq!(scaled_company(8000, 0), 8000);
        assert_eq!(scaled_company(8000, -1000), 0);
        assert_eq!(scaled_company(8000, 1000), 16_000);
        assert_eq!(scaled_company(1, 500), 1);
        assert_eq!(scaled_company(-1, -500), 0);
        assert_eq!(scaled_company(0, 700), 0);
    }
}
