//! Death by senescence and by accident (genetics specification 7.1, 7.2 and 11.4,
//! ADR-0017): the closed form of the age at death from a hazard budget, and the genetic
//! factor of longevity. Pure functions; the parameters come from the caller, the budgets
//! from the draws of the world (`DEATH_BUDGET`).
//!
//! Senescence is the Gompertz hazard `m · a · e^(b·x)` with the multiplier
//! `m = e^(−σ_g · ĝ)`, accident the constant Makeham hazard `λ`. From the origin age `x0`
//! with the budget `B` a person dies of senescence at `x = ln(e^(b·x0) + b · B / (a · m)) / b`
//! and of accident at `x0 + B / λ`. The multiplier acts like a shift of age by
//! `Δ = ln(m) / b = −σ_g · ĝ / b` years, the offset that `bio_age` reads (7.4); `ĝ` is
//! `g_L` held at `±LONGEVITY_INPUT_BOUND` (11.4), so death and `bio_age` read one `Δ`.

/// The bound of the genetic value of longevity as an input of death and `bio_age`
/// (genetics specification 11.4, ADR-0017); it changes only by a new ADR.
pub const LONGEVITY_INPUT_BOUND: f64 = 6.0;

/// The Gompertz term `a · e^(b·x)` of one sex (7.2): `a` per year, `b` per year of age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gompertz {
    pub a: f64,
    pub b: f64,
}

/// `g_L` held at `±LONGEVITY_INPUT_BOUND`; exactly ±6 is not held (11.4).
///
/// # Panics
///
/// Invariant `mortality.longevity` if `g` is not finite.
pub fn held_longevity(g: f64) -> f64 {
    if !g.is_finite() {
        panic!("invariant mortality.longevity");
    }
    g.clamp(0.0 - LONGEVITY_INPUT_BOUND, LONGEVITY_INPUT_BOUND)
}

/// `σ_g · ĝ`, 0 without a trait of longevity (11.1).
fn genetic_term(longevity: Option<f64>, sigma_g: f64) -> f64 {
    let valid = sigma_g >= 0.0 && sigma_g.is_finite();
    if !valid {
        panic!("invariant mortality.sigma");
    }
    longevity.map_or(0.0, |g| sigma_g * held_longevity(g))
}

/// The multiplier `m = e^(−σ_g · ĝ)` of the Gompertz term (7.2, `m_life` = 1 in Phase 1);
/// 1 without a trait of longevity (11.1).
///
/// # Panics
///
/// Invariant `mortality.sigma` if `sigma_g` is negative or not finite, and as
/// [`held_longevity`].
pub fn multiplier(longevity: Option<f64>, sigma_g: f64) -> f64 {
    sim_math::exp(0.0 - genetic_term(longevity, sigma_g))
}

/// The offset of biological age `Δ = −σ_g · ĝ / b` in years, positive is older (7.2, 7.4);
/// 0 without a trait of longevity (11.1).
///
/// # Panics
///
/// Invariant `mortality.gompertz` if `b` is not positive and finite, and as
/// [`multiplier`].
pub fn age_offset(longevity: Option<f64>, sigma_g: f64, b: f64) -> f64 {
    let valid = b > 0.0 && b.is_finite();
    if !valid {
        panic!("invariant mortality.gompertz");
    }
    (0.0 - genetic_term(longevity, sigma_g)) / b
}

fn check_origin_and_budget(origin_age: f64, budget: f64) {
    let valid_origin = origin_age >= 0.0 && origin_age.is_finite();
    if !valid_origin {
        panic!("invariant mortality.origin");
    }
    let valid_budget = budget >= 0.0 && budget.is_finite();
    if !valid_budget {
        panic!("invariant mortality.budget");
    }
}

/// The age at which a person dies of senescence when the cause acts from `origin_age` on
/// with the budget `budget` (7.1). Computed as
/// `x0 + ln(1 + b · B / (a · m) · e^(−b·x0)) / b`, the same number as the formula of 7.1
/// without its overflow: a very old origin dies at once instead of never.
///
/// # Panics
///
/// Invariants `mortality.origin` (negative or not finite), `mortality.budget` (negative or
/// not finite), `mortality.gompertz` (`a` or `b` not positive and finite),
/// `mortality.multiplier` (not positive and finite).
pub fn senescence_age(origin_age: f64, budget: f64, gompertz: Gompertz, multiplier: f64) -> f64 {
    check_origin_and_budget(origin_age, budget);
    let Gompertz { a, b } = gompertz;
    let valid = a > 0.0 && a.is_finite() && b > 0.0 && b.is_finite();
    if !valid {
        panic!("invariant mortality.gompertz");
    }
    let valid_multiplier = multiplier > 0.0 && multiplier.is_finite();
    if !valid_multiplier {
        panic!("invariant mortality.multiplier");
    }
    let ahead = b * budget / (a * multiplier) * sim_math::exp(0.0 - b * origin_age);
    origin_age + sim_math::ln(1.0 + ahead) / b
}

/// The age at which a person dies of accident when the constant hazard `per_year` acts
/// from `origin_age` on with the budget `budget` (7.1): `x0 + B / λ`.
///
/// # Panics
///
/// Invariants `mortality.origin`, `mortality.budget` as [`senescence_age`], and
/// `mortality.accident` if `per_year` is not positive and finite.
pub fn accident_age(origin_age: f64, budget: f64, per_year: f64) -> f64 {
    check_origin_and_budget(origin_age, budget);
    let valid = per_year > 0.0 && per_year.is_finite();
    if !valid {
        panic!("invariant mortality.accident");
    }
    origin_age + budget / per_year
}
