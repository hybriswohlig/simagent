//! The growth curve (genetics specification 5.1, 5.2): the mean curve (ADR-0010) with the
//! infancy term up to 2 years, Preece-Baines model 1 from 3 years and a smoothstep in
//! between, the timing moving the spurt; the individual curve scales it and moves its
//! timing by the phenotypes (ADR-0012). Pure functions, no state.

use crate::data::GrowthCurve;
use crate::time::SimDuration;

/// The infancy term `I(t) = L0 + B · (1 − e^(−c·t))`.
fn infancy(curve: &GrowthCurve, t: f64) -> f64 {
    curve.l0 + curve.b * (1.0 - sim_math::exp(-curve.c * t))
}

/// Preece-Baines model 1 with the timing `theta`: the gap `h1 − hθ` scaled by
/// `exp(−s0 · (θ_i − θ))`, so that childhood stays and only the spurt moves (5.2).
fn preece_baines(curve: &GrowthCurve, t: f64, theta: f64) -> f64 {
    let gap = (curve.h1 - curve.h_theta) * sim_math::exp(-curve.s0 * (theta - curve.theta));
    curve.h1
        - 2.0 * gap
            / (sim_math::exp(curve.s0 * (t - theta)) + sim_math::exp(curve.s1 * (t - theta)))
}

/// The height in cm at `age` with the timing `theta` (5.1, 5.2); the mean curve has
/// `theta = curve.theta`.
pub fn mean_height(curve: &GrowthCurve, age: SimDuration, theta: f64) -> f64 {
    let t = age.as_years();
    if t <= 2.0 {
        return infancy(curve, t);
    }
    if t >= 3.0 {
        return preece_baines(curve, t, theta);
    }
    let u = t - 2.0;
    let w = 3.0 * (u * u) - 2.0 * ((u * u) * u);
    (1.0 - w) * infancy(curve, t) + w * preece_baines(curve, t, theta)
}

/// The bound of the phenotypes `z_h` and `z_p` as inputs of the individual curve
/// (ADR-0012 section 1); it changes only by a new ADR.
pub const CURVE_INPUT_BOUND: f64 = 6.0;

/// `z` held at `±CURVE_INPUT_BOUND`, and whether it was.
fn held(z: f64) -> (f64, bool) {
    if z > CURVE_INPUT_BOUND {
        (CURVE_INPUT_BOUND, true)
    } else if z < -CURVE_INPUT_BOUND {
        (-CURVE_INPUT_BOUND, true)
    } else {
        (z, false)
    }
}

/// The growth curve of one person (5.2, ADR-0012): the timing `theta` (`θ_i`, years), the
/// target height `target` (`h1_i`, cm), the scale `kappa` (`κ_i`) and whether an input was
/// held at the bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndividualCurve {
    curve: GrowthCurve,
    pub theta: f64,
    pub target: f64,
    pub kappa: f64,
    pub clamped: bool,
}

impl IndividualCurve {
    /// The curve for the phenotypes of target height `z_h` and pubertal timing `z_p`, both
    /// finite (ADR-0009 section 2), held at the bound (ADR-0012 sections 1 and 3).
    pub fn new(curve: GrowthCurve, z_h: f64, z_p: f64) -> IndividualCurve {
        let (z_h, held_h) = held(z_h);
        let (z_p, held_p) = held(z_p);
        let theta = curve.theta + curve.theta_sd * z_p;
        let target = (curve.h1 + curve.sd * z_h) + curve.gamma * (theta - curve.theta);
        IndividualCurve {
            curve,
            theta,
            target,
            kappa: target / curve.h1,
            clamped: held_h || held_p,
        }
    }

    /// The height in cm at `age` before the loss from nutrition (5.4): `κ_i · H(t; θ_i)`.
    pub fn height(&self, age: SimDuration) -> f64 {
        self.kappa * mean_height(&self.curve, age, self.theta)
    }
}
