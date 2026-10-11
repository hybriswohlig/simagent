//! Fertility as pure functions (genetics specification 5.6 and 11.3, ADR-0017 section 4):
//! the woman's factor from her menarche, the man's from his puberty timing, both falling
//! with age, the chance of an attempt, and the wish for children. The world rule reads
//! them; nothing here is state.

use crate::data::GrowthCurve;
use crate::growth::IndividualCurve;

/// The curves of fertility (ADR-0017 section 1): after 5.6, or the window of the test data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curves {
    Puberty(Puberty),
    /// The woman's factor is 1 from `from` until before `until` (years), the man's is 1.
    Window {
        from: f64,
        until: f64,
    },
}

/// The values of 5.6 in years (the floor a fraction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puberty {
    /// The offset of the menarche after `θ_i`: `normal(mean, sd)`, held on
    /// `[θ_i + low, θ_i + high]`.
    pub menarche_mean: f64,
    pub menarche_sd: f64,
    pub menarche_low: f64,
    pub menarche_high: f64,
    /// The woman's ramp from the menarche, and her decline from 1 to 0.
    pub female_ramp: f64,
    pub female_decline_from: f64,
    pub female_decline_to: f64,
    /// The man's ramp begins `lead` years before `θ_i`; his decline ends at the floor.
    pub male_lead: f64,
    pub male_ramp: f64,
    pub male_decline_from: f64,
    pub male_floor: f64,
    pub male_decline_to: f64,
}

/// The smoothstep of 5.1 and 6.2, evaluated as `growth::puberty_progress` does.
fn smoothstep(u: f64) -> f64 {
    if u <= 0.0 {
        0.0
    } else if u >= 1.0 {
        1.0
    } else {
        3.0 * (u * u) - 2.0 * ((u * u) * u)
    }
}

/// The puberty timing `θ_i` of a person with the phenotype `z_p` (5.2): `θ + σ_θ · z_p`
/// with `z_p` held on [−6, 6], as the individual growth curve has it. Without a trait of
/// the puberty timing the term drops (11.1): pass 0, and `θ_i` is `θ` exactly.
pub fn theta_i(curve: GrowthCurve, z_p: f64) -> f64 {
    IndividualCurve::new(curve, 0.0, z_p).theta
}

/// The age of the menarche (5.6): `θ_i + δ`, held on `[θ_i + low, θ_i + high]`; `δ` is the
/// draw `normal(mean, sd)` of `FERTILITY_MENARCHE`.
pub fn menarche(theta_i: f64, delta: f64, puberty: &Puberty) -> f64 {
    let low = theta_i + puberty.menarche_low;
    let high = theta_i + puberty.menarche_high;
    let m = theta_i + delta;
    let m = if m < low { low } else { m };
    if m > high { high } else { m }
}

/// The woman's factor at the age `age` (5.6): 0 before her menarche `menarche`, the
/// smoothstep over the ramp, then 1, times the decline from 1 to 0; under the window 1
/// from `from` until before `until`.
pub fn female_factor(age: f64, menarche: f64, curves: &Curves) -> f64 {
    match curves {
        Curves::Puberty(p) => {
            let ramp = smoothstep((age - menarche) / p.female_ramp);
            let (from, to) = (p.female_decline_from, p.female_decline_to);
            let decline = if age <= from {
                1.0
            } else if age >= to {
                0.0
            } else {
                (to - age) / (to - from)
            };
            ramp * decline
        }
        Curves::Window { from, until } => {
            if age >= *from && age < *until {
                1.0
            } else {
                0.0
            }
        }
    }
}

/// The man's factor at the age `age` (5.6): 0 until `lead` years before his `θ_i`, the
/// smoothstep over the ramp, then 1, times the decline from 1 down to the floor, where it
/// stays; under the window 1.
pub fn male_factor(age: f64, theta_i: f64, curves: &Curves) -> f64 {
    match curves {
        Curves::Puberty(p) => {
            let ramp = smoothstep((age - (theta_i - p.male_lead)) / p.male_ramp);
            let (from, to) = (p.male_decline_from, p.male_decline_to);
            let decline = if age <= from {
                1.0
            } else if age >= to {
                p.male_floor
            } else {
                1.0 - (1.0 - p.male_floor) * ((age - from) / (to - from))
            };
            ramp * decline
        }
        Curves::Window { .. } => 1.0,
    }
}

/// The chance of an attempt (11.3): the adult rate times both factors.
pub fn chance(adult_rate: f64, female: f64, male: f64) -> f64 {
    (adult_rate * female) * male
}

/// The wish for children (11.3) from the draw `u` of `FERTILITY_WISH` (in [0, 1)): the
/// smallest `n` with `u < P_0 + … + P_n`, `P_n ∝ p_n · e^(α·n)`; the last `n` if none.
///
/// # Panics
///
/// Invariant `fertility.wish` for an empty shape or one without a positive weight (the
/// data check rules both out).
pub fn wish(u: f64, shape: &[f64], tilt: f64) -> u32 {
    let weight = |(n, p): (u32, &f64)| *p * sim_math::exp(tilt * f64::from(n));
    let total = (0..).zip(shape).map(weight).fold(0.0, |sum, w| sum + w);
    let valid = total > 0.0 && total.is_finite();
    if !valid {
        panic!("invariant fertility.wish");
    }
    let mut acc = 0.0;
    let mut last = 0;
    for (n, p) in (0..).zip(shape) {
        acc += weight((n, p)) / total;
        last = n;
        if u < acc {
            return n;
        }
    }
    last
}
