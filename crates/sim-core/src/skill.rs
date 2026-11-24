//! Skills of growing up (ADR-0033 section 6): language, reading and play, practised in
//! effective seconds. A practice second counts by the window of its skill at the age of the
//! second and by the diligence from conscientiousness; each 10 effective hours crossed are a
//! milestone with the level of the learning function (genetics specification 11.5) there.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::learning::{LearningParams, level};
use crate::time::{SimDuration, YEAR};

/// A skill, a code in the log: language, reading, play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skill {
    Language,
    Reading,
    Play,
}

impl Skill {
    /// The skills in their order, the order of the practice seconds of a person.
    pub const ALL: [Skill; 3] = [Skill::Language, Skill::Reading, Skill::Play];

    /// Its place in [`Skill::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Skill::Language => 0,
            Skill::Reading => 1,
            Skill::Play => 2,
        }
    }

    /// The trait whose phenotype is its talent: cognition for language and reading, openness
    /// for play.
    pub fn talent(self) -> DataId {
        let id = match self {
            Skill::Language | Skill::Reading => "core:cognition",
            Skill::Play => "core:openness",
        };
        DataId::new(id).unwrap_or_else(|_| panic!("invariant skill.talent_id"))
    }

    /// The window factor of a practice second at the age `age`: language in full from 0 to under
    /// 6 years, else a quarter; reading in full from 6 to under 12, else a quarter; play in full
    /// from 6 to under 12, else a half.
    pub fn window(self, age: SimDuration) -> f64 {
        let years = |n: i64| n * YEAR.as_secs();
        let secs = age.as_secs();
        let (inside, outside) = match self {
            Skill::Language => (secs < years(6), 0.25),
            Skill::Reading => ((years(6)..years(12)).contains(&secs), 0.25),
            Skill::Play => ((years(6)..years(12)).contains(&secs), 0.5),
        };
        if inside { 1.0 } else { outside }
    }
}

/// The age at which a talent is read: 30 years. With it the weight `w` of the lived experience is
/// its value at the 18th birthday (ADR-0033 section 6).
pub const TALENT_AGE: SimDuration = SimDuration::from_secs(30 * YEAR.as_secs());

/// Effective hours between two milestones: 10.
pub const MILESTONE_SECONDS: u64 = 10 * 3600;

/// The cap of a level before it is counted in thousandths.
const LEVEL_CAP: f64 = 1e12;

/// `floor(x)` as `−ceil(−x)`, exact for every finite `x`; `0.0 − x` keeps a zero positive.
fn floor(x: f64) -> f64 {
    0.0 - sim_math::ceil(0.0 - x)
}

/// A count of seconds or hours as an exact `f64`.
fn exact(n: u64) -> f64 {
    (i64::try_from(n).ok())
        .and_then(sim_math::i64_to_f64_exact)
        .unwrap_or_else(|| panic!("invariant skill.range"))
}

/// The diligence factor `f = clamp(1 + 0.2 · z_C, 0.5, 1.5)` from conscientiousness `z_C`.
pub fn diligence(z_c: f64) -> f64 {
    if !z_c.is_finite() {
        panic!("invariant skill.diligence");
    }
    (1.0 + 0.2 * z_c).clamp(0.5, 1.5)
}

/// The effective seconds of `raw` practice seconds, `(raw · window) · f` as `f64` from left to
/// right, rounded down to whole seconds.
pub fn effective(raw: u64, window: f64, f: f64) -> u64 {
    // Exact: practice runs within the time range (2^45 s), far below 2^53.
    let out = floor((exact(raw) * window) * f);
    (sim_math::to_i64_exact(out).and_then(|n| u64::try_from(n).ok()))
        .unwrap_or_else(|| panic!("invariant skill.effective"))
}

/// A level in thousandths: `floor(min(level, 10^12) · 1000)`; `level` is finite and not negative.
pub fn level_milli(level: f64) -> i64 {
    let capped = if level > LEVEL_CAP { LEVEL_CAP } else { level };
    let out = floor(capped * 1000.0);
    sim_math::to_i64_exact(out).unwrap_or_else(|| panic!("invariant skill.level"))
}

/// A milestone crossed: the boundary in effective hours (10, 20, …) and the level there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Milestone {
    pub hours: u64,
    pub level_milli: i64,
}

/// The milestones crossed when the effective seconds go from `before` to `after` with the talent
/// `z`: one per boundary of 10 effective hours with `before < boundary <= after`, ascending, each
/// with the level at exactly that boundary.
pub fn milestones(z: f64, (before, after): (u64, u64), params: &LearningParams) -> Vec<Milestone> {
    let mut out = Vec::new();
    let mut k = before / MILESTONE_SECONDS + 1;
    while k
        .checked_mul(MILESTONE_SECONDS)
        .is_some_and(|edge| edge <= after)
    {
        let hours = 10 * k;
        out.push(Milestone {
            hours,
            level_milli: level_milli(level(z, exact(hours), params)),
        });
        k += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    // The play values of specification 11.5, as in tools/development_reference.py.
    const PARAMS: LearningParams = LearningParams {
        kappa: 0.25,
        nu: 0.2,
        tau0: 1000.0,
    };

    fn years(n: i64) -> SimDuration {
        SimDuration::from_secs(n * YEAR.as_secs())
    }

    fn at(secs: i64) -> SimDuration {
        SimDuration::from_secs(secs)
    }

    // Expected values: tools/development_reference.py, "Effective practice seconds (6)".
    #[test]
    fn the_window_and_diligence_scale_practice_as_the_reference() {
        assert_eq!(effective(3600, Skill::Reading.window(years(13)), 1.0), 900);
        assert_eq!(effective(3600, Skill::Reading.window(years(8)), 1.0), 3600);
        assert_eq!(effective(3600, Skill::Play.window(years(13)), 1.0), 1800);
        assert_eq!(Skill::Play.window(at(years(12).as_secs() - 1)), 1.0);
        assert_eq!(Skill::Play.window(years(12)), 0.5);
        assert_eq!(effective(600, Skill::Language.window(years(3)), 1.0), 600);
        assert_eq!(effective(600, Skill::Language.window(years(7)), 1.0), 150);
        assert_eq!(Skill::Language.window(at(years(6).as_secs() - 1)), 1.0);
        assert_eq!(Skill::Language.window(years(6)), 0.25);
        assert_eq!(Skill::Reading.window(at(years(6).as_secs() - 1)), 0.25);
        assert_eq!(Skill::Reading.window(years(6)), 1.0);
        assert_eq!(diligence(0.0), 1.0);
        assert_eq!(diligence(5.0), 1.5);
        assert_eq!(diligence(-5.0), 0.5);
        assert!((diligence(2.0) - 1.4).abs() <= 1.4 * 1e-15);
        assert_eq!(effective(600, 1.0, diligence(5.0)), 900);
        assert_eq!(effective(600, 1.0, diligence(-5.0)), 300);
        assert_eq!(effective(7, 0.5, 1.0), 3);
    }

    // Expected values: tools/development_reference.py, "level_milli and milestones (6)".
    #[test]
    fn milestones_cross_each_ten_hours_with_the_level_there() {
        assert_eq!(level_milli(level(0.0, 1000.0, &PARAMS)), 632);
        assert_eq!(level_milli(level(1.0, 1000.0, &PARAMS)), 905);
        assert_eq!(level_milli(level(-1.5, 2000.0, &PARAMS)), 531);
        assert_eq!(level_milli(level(2.5, 5000.0, &PARAMS)), 1867);
        assert_eq!(level_milli(0.0), 0);
        assert_eq!(level_milli(1e300), 1_000_000_000_000_000);
        let m = |hours, level_milli| Milestone { hours, level_milli };
        assert_eq!(
            milestones(0.0, (30_000, 80_000), &PARAMS),
            vec![m(10, 9), m(20, 19)]
        );
        assert_eq!(milestones(0.0, (0, 36_000), &PARAMS), vec![m(10, 9)]);
        assert_eq!(milestones(0.0, (36_000, 36_001), &PARAMS), vec![]);
        assert_eq!(milestones(0.0, (0, 35_999), &PARAMS), vec![]);
        let three = vec![m(10, 9), m(20, 19), m(30, 29)];
        assert_eq!(milestones(0.0, (0, 110_000), &PARAMS), three);
        assert_eq!(milestones(1.0, (0, 36_000), &PARAMS), vec![m(10, 15)]);
    }

    #[test]
    fn the_talents_are_cognition_and_openness() {
        assert_eq!(Skill::Language.talent().as_str(), "core:cognition");
        assert_eq!(Skill::Reading.talent().as_str(), "core:cognition");
        assert_eq!(Skill::Play.talent().as_str(), "core:openness");
        assert_eq!(TALENT_AGE, years(30));
    }
}
