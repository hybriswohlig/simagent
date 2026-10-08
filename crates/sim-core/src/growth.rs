//! The mean growth curve (genetics specification 5.1, ADR-0010): the infancy term up to 2
//! years, Preece-Baines model 1 from 3 years, a smoothstep in between; the timing moves
//! the spurt as in 5.2. A pure function of the curve, the age and the timing.

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
