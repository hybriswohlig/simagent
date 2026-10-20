//! The percentile of the genetics card (ADR-0021 section 5, Grok G2 and L1): `p = 100 Φ(z)`;
//! from the middle up "größer als `min(⌊p⌋, 99)`", below "kleiner als `min(⌊100 − p⌋, 99)`".
//! The expected values from tables of Φ: Φ(0.5) = 0.69146, Φ(1) = 0.84134, Φ(0.1) = 0.53983,
//! Φ(2.32) = 0.98983, Φ(2.33) = 0.99010, Φ(6) = 1 − 1e-9.

use sim_core::presentation::card::{Percentile, percentile};
use sim_math::probit;

#[test]
fn the_percentile_by_hand() {
    let up = |n| Percentile { up: true, n };
    let down = |n| Percentile { up: false, n };
    let cases = [
        (0.0, up(50)),
        (-0.0, up(50)),
        (0.5, up(69)),
        (1.0, up(84)),
        (-1.0, down(84)),
        (-0.1, down(53)),
        (2.32, up(98)),
        (2.33, up(99)),
        (-2.33, down(99)),
        (6.0, up(99)),
        (-6.0, down(99)),
    ];
    for (z, expected) in cases {
        assert_eq!(percentile(z), expected, "{z}");
    }
}

/// A `z` that is a threshold itself counts: the core's rule is "at or above the threshold"
/// (`probit(j / 100) ≤ z` up, `≥ z` down), which may differ from `⌊100 Φ(z)⌋` within the
/// tolerance of `probit`; the reference keeps every test value away from such a `z`.
#[test]
fn a_threshold_itself_counts() {
    assert_eq!(percentile(probit(0.6)), Percentile { up: true, n: 60 });
    assert_eq!(percentile(probit(0.4)), Percentile { up: false, n: 60 });
    assert_eq!(percentile(probit(0.99)), Percentile { up: true, n: 99 });
}

/// The status of a disease (Grok G4, L7, M1, M4) by hand: what the genotype found, the switch
/// at the second shown and the age then. A person born at the start of year −10 with the
/// onset 45.9 reaches it in the second `−10 · YEAR + ⌈45.9 · YEAR⌉`, in year 35, at 45 whole
/// years; with the onset 0 at its birth, in year −10, at 0 (the web's "seit der Geburt").
#[test]
fn the_status_of_a_disease_by_hand() {
    use sim_core::presentation::card::{DiseaseStatus, Found, disease_status};
    use sim_core::time::{SimTime, YEAR};
    let y = YEAR.as_secs();
    let born = SimTime::from_secs(-10 * y);
    let at = |years: i64| SimTime::from_secs(born.as_secs() + years * y);
    let cases = [
        (Found::Not, true, 50, DiseaseStatus::NotAffected),
        (Found::Carrier, true, 50, DiseaseStatus::Carrier),
        (
            Found::PenetranceFallen,
            true,
            50,
            DiseaseStatus::PenetranceFallen,
        ),
        (
            Found::PenetranceFallen,
            false,
            50,
            DiseaseStatus::PenetranceFallen,
        ),
        (Found::Onset(45.9), false, 50, DiseaseStatus::Resting),
        (Found::Onset(45.9), false, 40, DiseaseStatus::Resting),
        (Found::Onset(45.9), true, 40, DiseaseStatus::NotYet),
        (
            Found::Onset(45.9),
            true,
            50,
            DiseaseStatus::Affected {
                year: 35,
                age_years: 45,
            },
        ),
        (
            Found::Onset(0.0),
            true,
            0,
            DiseaseStatus::Affected {
                year: -10,
                age_years: 0,
            },
        ),
    ];
    for (found, on, years, expected) in cases {
        assert_eq!(
            disease_status(found, on, born, at(years)),
            expected,
            "{found:?} {on} {years}"
        );
    }
}
