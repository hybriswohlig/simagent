//! The biological age (genetics specification 7.4, ADR-0015): the calendar age up to 25,
//! then faster or slower by the offset `Δ`. A pure function; where `Δ` comes from is the
//! business of the model of death (7.2).

/// The age up to which the biological age is the calendar age (7.4).
const MATURE: f64 = 25.0;

/// The biological age in years at the calendar age `age` (years) with the offset `delta`
/// (years, positive is older) and the weight `weight` (7.4): `age` up to 25, then
/// `25 + (age − 25) · e^(weight · delta / 50)`.
pub fn bio_age(age: f64, delta: f64, weight: f64) -> f64 {
    if age <= MATURE {
        return age;
    }
    MATURE + (age - MATURE) * sim_math::exp(weight * delta / 50.0)
}
