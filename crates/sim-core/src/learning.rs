//! The learning function (genetics specification 11.5): the level after `h` hours of
//! practice for the talent `z`, `L(z, h) = e^(κ·z) · (1 − e^(−h / τ(z)))` with the time
//! constant `τ(z) = τ_0 · e^(−ν·z)`. A higher talent has the higher plateau `e^(κ·z)` and
//! the shorter time constant, so at every practice time the level is strictly increasing
//! in the talent (P8, 8.2). A pure function; the parameters come from the caller.

/// The parameters of the learning function (genetics specification 11.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LearningParams {
    /// `κ > 0`: one standard deviation of talent raises the plateau by the factor `e^κ`.
    pub kappa: f64,
    /// `ν ≥ 0`: one standard deviation of talent shortens the time constant by `e^(−ν)`.
    pub nu: f64,
    /// `τ_0 > 0`: the time constant in hours at the talent 0.
    pub tau0: f64,
}

/// The level after `hours` of practice for the talent `z` (genetics specification 11.5):
/// exactly +0 without practice, rising towards the plateau `e^(κ·z)`.
///
/// An invalid input breaks an invariant (`invariant learning.<name>`), NaN always: `z` not
/// finite, `hours` negative or not finite, `kappa` or `tau0` not positive and finite, `nu`
/// negative or not finite. A talent so large that the plateau overflows (with the values
/// of 11.5 above about 2800 standard deviations) breaks `invariant learning.range` once it
/// practises. An underflowing time constant gives the plateau, the limit of the formula.
/// `0.0 - x` instead of `-x` keeps a zero positive.
pub fn level(z: f64, hours: f64, params: &LearningParams) -> f64 {
    let LearningParams { kappa, nu, tau0 } = *params;
    if !z.is_finite() {
        panic!("invariant learning.z");
    }
    let valid_hours = hours >= 0.0 && hours.is_finite();
    if !valid_hours {
        panic!("invariant learning.hours");
    }
    let valid_kappa = kappa > 0.0 && kappa.is_finite();
    if !valid_kappa {
        panic!("invariant learning.kappa");
    }
    let valid_nu = nu >= 0.0 && nu.is_finite();
    if !valid_nu {
        panic!("invariant learning.nu");
    }
    let valid_tau0 = tau0 > 0.0 && tau0.is_finite();
    if !valid_tau0 {
        panic!("invariant learning.tau0");
    }
    // Without practice the level is 0 whatever the talent: an overflowing plateau or an
    // underflowing time constant must not turn it into NaN (`∞ · 0`, `0 / 0`).
    if hours == 0.0 {
        return 0.0;
    }
    let tau = tau0 * sim_math::exp(0.0 - nu * z);
    let level = sim_math::exp(kappa * z) * (1.0 - sim_math::exp(0.0 - hours / tau));
    if !level.is_finite() {
        panic!("invariant learning.range");
    }
    level
}
