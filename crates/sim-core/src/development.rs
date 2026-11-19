//! Growing up (ADR-0033 sections 1, 3 and 4): the second from which a person counts as
//! raised in a house, the age weight `w` that follows from it, and the lived experience `D`
//! per temperament axis that enters the temperament phenotype in place of part of `E`; and
//! the upbringing table (section 5) whose occasions step `D`.

use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::time::{SimDuration, SimTime, YEAR};

/// The five temperament axes in the order of ADR-0033 section 5: openness,
/// conscientiousness, extraversion, agreeableness, neuroticism. `D` and the development
/// values of the calibration come in this order. A code in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Openness,
    Conscientiousness,
    Extraversion,
    Agreeableness,
    Neuroticism,
}

impl Axis {
    /// The axes in their order.
    pub const ALL: [Axis; 5] = [
        Axis::Openness,
        Axis::Conscientiousness,
        Axis::Extraversion,
        Axis::Agreeableness,
        Axis::Neuroticism,
    ];

    /// The trait of the axis in the genome map.
    pub fn trait_id(self) -> DataId {
        let id = match self {
            Axis::Openness => "core:openness",
            Axis::Conscientiousness => "core:conscientiousness",
            Axis::Extraversion => "core:extraversion",
            Axis::Agreeableness => "core:agreeableness",
            Axis::Neuroticism => "core:neuroticism",
        };
        DataId::new(id).unwrap_or_else(|_| panic!("invariant development.axis_id"))
    }

    /// The axis of a trait; `None` for a trait that is no temperament axis. The same names as
    /// `trait_id`, compared without building an id for each phenotype.
    pub fn of(t: &DataId) -> Option<Axis> {
        match t.as_str() {
            "core:openness" => Some(Axis::Openness),
            "core:conscientiousness" => Some(Axis::Conscientiousness),
            "core:extraversion" => Some(Axis::Extraversion),
            "core:agreeableness" => Some(Axis::Agreeableness),
            "core:neuroticism" => Some(Axis::Neuroticism),
            _ => None,
        }
    }

    /// Its place in `Axis::ALL`.
    pub const fn index(self) -> usize {
        match self {
            Axis::Openness => 0,
            Axis::Conscientiousness => 1,
            Axis::Extraversion => 2,
            Axis::Agreeableness => 3,
            Axis::Neuroticism => 4,
        }
    }
}

/// The bound of `D` in thousandths: it stays in −3000 to 3000 (ADR-0033 section 4).
pub const EXPERIENCE_BOUND: i32 = 3000;

/// One saturating step of `D` (ADR-0033 section 4), `step` already age-scaled: toward 3000 for
/// a positive step, `D + step · (3000 − D) / 3000`, toward −3000 for a negative one,
/// `D + step · (3000 + D) / 3000`, in `i64` with division toward 0, held to the bound.
pub fn saturate(d: i32, step: i32) -> i32 {
    let (d, step, bound) = (i64::from(d), i64::from(step), i64::from(EXPERIENCE_BOUND));
    let room = if step >= 0 { bound - d } else { bound + d };
    let out = (d + step * room / bound).clamp(-bound, bound);
    i32::try_from(out).unwrap_or_else(|_| panic!("invariant development.saturate"))
}

/// The age factor of a step at the age `age` of the child at the second of the occasion
/// (ADR-0033 section 4): the whole step under 11 years, half from 11, a quarter from 15, each as
/// `step · n / 4` with division toward 0; from 18 on nothing.
pub fn age_factor(step: i32, age: SimDuration) -> i32 {
    let years = |n: i64| n * YEAR.as_secs();
    let quarters = match age.as_secs() {
        secs if secs < years(11) => 4,
        secs if secs < years(15) => 2,
        secs if secs < years(18) => 1,
        _ => 0,
    };
    step * quarters / 4
}

/// An occasion of the upbringing table (ADR-0033 section 5), a code in the data and the log:
/// a parent praises the child; a parent or an older sibling teases it or apologizes to it; it
/// came home from school; it took part in a talk from 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Upbringing {
    Praise,
    Tease,
    Apology,
    School,
    Talk,
}

/// A row of the table: an occasion and its step per axis, −1000 to 1000, an absent axis 0.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpbringingRow {
    pub occasion: Upbringing,
    #[serde(default)]
    pub openness: i16,
    #[serde(default)]
    pub conscientiousness: i16,
    #[serde(default)]
    pub extraversion: i16,
    #[serde(default)]
    pub agreeableness: i16,
    #[serde(default)]
    pub neuroticism: i16,
}

impl UpbringingRow {
    /// The step of `axis`.
    pub const fn step(&self, axis: Axis) -> i16 {
        match axis {
            Axis::Openness => self.openness,
            Axis::Conscientiousness => self.conscientiousness,
            Axis::Extraversion => self.extraversion,
            Axis::Agreeableness => self.agreeableness,
            Axis::Neuroticism => self.neuroticism,
        }
    }
}

/// The upbringing table of `social/development.json` (ADR-0033 section 5): its rows in order.
/// An empty table changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpbringingTable {
    pub rows: Vec<UpbringingRow>,
}

/// One change of `D` at an occasion: `axis` from `before` to `after` by `step` (after the age
/// factor), from the row of `reached[by]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stepped {
    pub axis: Axis,
    pub before: i32,
    pub after: i32,
    pub step: i32,
    pub by: usize,
}

impl UpbringingTable {
    /// The changes of the occasions `reached` to `experience` of a child of `age` (ADR-0033
    /// sections 4 and 5): per axis in order, the rows in table order whose occasion is in
    /// `reached` (in its order), each step age-scaled and saturated. A step that changes
    /// nothing is left out. `experience` holds `D` afterwards.
    pub fn steps(
        &self,
        reached: &[Upbringing],
        age: SimDuration,
        experience: &mut [i32; 5],
    ) -> Vec<Stepped> {
        let mut out = Vec::new();
        for axis in Axis::ALL {
            for row in &self.rows {
                for (by, occasion) in reached.iter().enumerate() {
                    let step = row.step(axis);
                    if row.occasion != *occasion || step == 0 {
                        continue;
                    }
                    let step = age_factor(i32::from(step), age);
                    let before = experience[axis.index()];
                    let after = saturate(before, step);
                    if after != before {
                        experience[axis.index()] = after;
                        out.push(Stepped {
                            axis,
                            before,
                            after,
                            step,
                            by,
                        });
                    }
                }
            }
        }
        out
    }
}

/// The most rows a table may have.
pub const UPBRINGING_ROWS: usize = 32;

/// The bound of a step in the table.
pub const UPBRINGING_STEP: i16 = 1000;

/// A table the core refuses: more than 32 rows, or a step outside −1000 to 1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpbringingError;

impl fmt::Display for UpbringingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("development.rows")
    }
}

impl UpbringingTable {
    /// The table if it keeps its bounds.
    pub fn checked(self) -> Result<UpbringingTable, UpbringingError> {
        let bound = -UPBRINGING_STEP..=UPBRINGING_STEP;
        let fits = |row: &UpbringingRow| Axis::ALL.iter().all(|a| bound.contains(&row.step(*a)));
        if self.rows.len() <= UPBRINGING_ROWS && self.rows.iter().all(fits) {
            Ok(self)
        } else {
            Err(UpbringingError)
        }
    }
}

/// The age up to which a person is raised and `D` can change: 18 years.
pub const RAISED_UNTIL: SimDuration = SimDuration::from_secs(18 * YEAR.as_secs());

/// What a person has of growing up in a house (ADR-0033 sections 3 and 4): the first second
/// in which it belonged, under 18, to a household living in a house, and `D` per axis in
/// thousandths. Set once and kept, also after a later move or death.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Raised {
    pub since: SimTime,
    pub experience: [i32; 5],
}

impl Raised {
    /// Raised from `since` on, with nothing lived yet (`D = 0`).
    pub const fn new(since: SimTime) -> Raised {
        Raised {
            since,
            experience: [0; 5],
        }
    }

    /// Whether it fits a person born at `born`: raised at an age of 0 to under 18, and each
    /// `D` within its bound. That it lies before the clock the loader checks.
    pub fn fits(&self, born: SimTime) -> bool {
        let age = self.since.checked_since(born);
        let young = age.is_some_and(|age| age.as_secs() >= 0 && age < RAISED_UNTIL);
        let bounded =
            (self.experience.iter()).all(|d| (-EXPERIENCE_BOUND..=EXPERIENCE_BOUND).contains(d));
        young && bounded
    }
}

/// What the phenotype reads of growing up: the age at which the person was raised, if it
/// was, and `D` per axis. `NONE` for one never raised and for every view without a run (the
/// sheet, the preview): `w = 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lived {
    pub raised_at_age: Option<SimDuration>,
    pub experience: [i32; 5],
}

impl Lived {
    /// Never raised in a house.
    pub const NONE: Lived = Lived {
        raised_at_age: None,
        experience: [0; 5],
    };

    /// What a person born at `born` has of `raised`.
    pub fn of(raised: Option<&Raised>, born: SimTime) -> Lived {
        let Some(raised) = raised else {
            return Lived::NONE;
        };
        Lived {
            raised_at_age: raised.since.checked_since(born),
            experience: raised.experience,
        }
    }
}

/// The age weight `w` at the age `age` of one raised from the age `raised_at_age` on
/// (ADR-0033 section 3): `clamp((min(age, 18 years) − raised_at_age) / 18 years, 0, 1)`, an
/// `f64` from the whole seconds; 0 for one never raised. This is
/// `(min(t, born + 18 years) − raised_since) / 18 years` with both sides taken from `born`.
pub fn weight(age: SimDuration, raised_at_age: Option<SimDuration>) -> f64 {
    let Some(raised) = raised_at_age else {
        return 0.0;
    };
    let upper = age.as_secs().min(RAISED_UNTIL.as_secs());
    // Always exact: the time range (2^45 s) lies far below 2^53.
    let exact = |secs: i64| {
        sim_math::i64_to_f64_exact(secs).unwrap_or_else(|| panic!("invariant time.out_of_range"))
    };
    let w = exact(upper.saturating_sub(raised.as_secs())) / exact(RAISED_UNTIL.as_secs());
    // Never NaN: both seconds are exact and the year is not 0.
    w.clamp(0.0, 1.0)
}
