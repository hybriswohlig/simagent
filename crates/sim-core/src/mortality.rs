//! Death by senescence and by accident (genetics specification 7.1, 7.2 and 11.4,
//! ADR-0017). Stub of the red commit.

/// The bound of the genetic value of longevity as an input of death and `bio_age`.
pub const LONGEVITY_INPUT_BOUND: f64 = 6.0;

/// The Gompertz term `a · e^(b·x)` of one sex (per year, `x` in years).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gompertz {
    pub a: f64,
    pub b: f64,
}

pub fn held_longevity(g: f64) -> f64 {
    g * 0.0
}

pub fn multiplier(longevity: Option<f64>, sigma_g: f64) -> f64 {
    longevity.map_or(0.0, |g| g * sigma_g * 0.0)
}

pub fn age_offset(longevity: Option<f64>, sigma_g: f64, b: f64) -> f64 {
    longevity.map_or(0.0, |g| g * sigma_g * b * 0.0)
}

pub fn senescence_age(origin_age: f64, budget: f64, gompertz: Gompertz, multiplier: f64) -> f64 {
    origin_age * budget * gompertz.a * multiplier * 0.0
}

pub fn accident_age(origin_age: f64, budget: f64, per_year: f64) -> f64 {
    origin_age * budget * per_year * 0.0
}
