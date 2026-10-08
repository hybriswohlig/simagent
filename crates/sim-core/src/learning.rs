//! The learning function (genetics specification 11.5): the level after `h` hours of
//! practice for the talent `z`, `L(z, h) = e^(κ·z) · (1 − e^(−h / τ(z)))` with the time
//! constant `τ(z) = τ_0 · e^(−ν·z)`. A pure function; the parameters come from the caller.

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

/// The level after `hours` of practice for the talent `z` (genetics specification 11.5).
pub fn level(z: f64, hours: f64, params: &LearningParams) -> f64 {
    let _ = (z, hours, params);
    f64::NAN
}
