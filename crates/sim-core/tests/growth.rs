//! The mean growth curve (genetics specification 5.1): the infancy term up to 2 years,
//! Preece-Baines model 1 from 3 years, a smoothstep in between; the timing `theta` moves
//! the spurt as in 5.2. The expected values are the values of the specification within
//! the rounding it prints them with.

mod common;

use sim_core::genome::Sex;
use sim_core::growth::{CURVE_INPUT_BOUND, IndividualCurve, mean_height};
use sim_core::time::{SimDuration, YEAR};

/// WHO Child Growth Standards (2006), length/height-for-age medians at 3 and 5 years
/// (expanded tables, days 1095 and 1826), as in `tools/spec_reference.py`.
const WHO_3_5: [(Sex, f64, f64); 2] = [
    (Sex::Male, 96.0674, 109.9593),
    (Sex::Female, 95.0344, 109.4189),
];

/// `days` days of the clock (ADR-0002: 364 days per year).
fn days(days: i64) -> SimDuration {
    SimDuration::from_secs(days * 86_400)
}

/// The mean curve of one sex at `t` whole years, with its own timing.
fn mean(sex: Sex, t: i64) -> f64 {
    let curve = common::data().genetics().growth(sex);
    mean_height(&curve, YEAR * t, curve.theta)
}

#[test]
fn the_infancy_term_passes_the_who_medians() {
    for (sex, c, top) in [(Sex::Male, 0.762, 98.4), (Sex::Female, 0.696, 98.7)] {
        let curve = common::data().genetics().growth(sex);
        // [W5.2f], [W5.2q] to 3 decimals; [W5.2g], [W5.2r] to 1 decimal.
        assert!((curve.c - c).abs() <= 0.0005, "{sex:?} c {}", curve.c);
        assert!(
            (curve.l0 + curve.b - top).abs() <= 0.05,
            "{sex:?} {}",
            curve.l0 + curve.b
        );
        // Exactly through the medians at birth, 12 and 24 months (5.1).
        assert_eq!(mean(sex, 0), curve.l0);
        assert!((mean(sex, 1) - curve.l12).abs() < 1e-9, "{sex:?} 12 months");
        assert!((mean(sex, 2) - curve.l24).abs() < 1e-9, "{sex:?} 24 months");
    }
}

#[test]
fn the_mean_curve_meets_the_values_of_the_specification() {
    // Against the WHO medians at 3 and 5 years: [W5.2j], [W5.2k], [W5.2u], [W5.2v].
    let expected = [(-0.7, 1.7), (-1.5, 1.5)];
    for ((sex, who_3, who_5), (at_3, at_5)) in WHO_3_5.into_iter().zip(expected) {
        assert!(
            (mean(sex, 3) - who_3 - at_3).abs() <= 0.05,
            "{sex:?} 3 years"
        );
        assert!(
            (mean(sex, 5) - who_5 - at_5).abs() <= 0.05,
            "{sex:?} 5 years"
        );
    }
    // The sexes: 0.4 cm apart at 8 years [W6.1g], girls taller at 12 [W6.1h].
    let apart = |t| mean(Sex::Male, t) - mean(Sex::Female, t);
    assert!((apart(8) - 0.4).abs() <= 0.05, "{}", apart(8));
    assert!((apart(12) + 2.1).abs() <= 0.05, "{}", apart(12));
    // Adults approach h1 from below.
    for sex in [Sex::Male, Sex::Female] {
        let h1 = common::data().genetics().growth(sex).h1;
        let adult = mean(sex, 25);
        assert!(adult < h1 && adult > h1 - 0.5, "{sex:?} {adult}");
    }
}

/// 5.2: the curve rises for every timing, also three years early or late, on a grid of a
/// twentieth of a year from 0 to 25 years; it is continuous where the pieces meet.
#[test]
fn the_curve_rises_for_every_timing() {
    for sex in [Sex::Male, Sex::Female] {
        let curve = common::data().genetics().growth(sex);
        for shift in [0.0, -3.0, 3.0] {
            let theta = curve.theta + shift;
            let mut previous = f64::NEG_INFINITY;
            for k in 0..=500_u32 {
                let at = SimDuration::from_secs(i64::from(k) * YEAR.as_secs() / 20);
                let height = mean_height(&curve, at, theta);
                assert!(height > previous, "{sex:?} {shift} at {k}/20: {height}");
                previous = height;
            }
        }
        let near = |d: i64| mean_height(&curve, days(d), curve.theta);
        for joint in [2 * 364, 3 * 364] {
            assert!(
                (near(joint) - near(joint - 1)).abs() < 0.05,
                "{sex:?} {joint}"
            );
            assert!(
                (near(joint + 1) - near(joint)).abs() < 0.05,
                "{sex:?} {joint}"
            );
        }
    }
}

/// 5.1: between 2 and 3 years the curve blends with `w = 3u² − 2u³`, `u = t − 2`. At 2¼
/// years `w` is 0.15625 (a straight blend would give 0.25); the expected value writes out
/// the infancy term, Preece-Baines and the blend.
#[test]
fn the_blend_is_a_smoothstep() {
    for sex in [Sex::Male, Sex::Female] {
        let curve = common::data().genetics().growth(sex);
        let t = 2.25;
        let infancy = curve.l0 + curve.b * (1.0 - sim_math::exp(-curve.c * t));
        let pb = curve.h1
            - 2.0 * (curve.h1 - curve.h_theta)
                / (sim_math::exp(curve.s0 * (t - curve.theta))
                    + sim_math::exp(curve.s1 * (t - curve.theta)));
        let w = 0.156_25;
        let expected = (1.0 - w) * infancy + w * pb;
        let at = mean_height(&curve, days(2 * 364 + 91), curve.theta);
        assert!((at - expected).abs() < 1e-9, "{sex:?} {at} {expected}");
        assert!(
            (at - (0.75 * infancy + 0.25 * pb)).abs() > 0.01,
            "{sex:?} not straight"
        );
    }
}

/// 5.2: a year earlier or later moves the spurt, not the childhood, and barely the adult:
/// against the mean curve, one year early at 12 years +4.7 cm (boys, [W5.3a]) and +4.9
/// (girls, [W5.3d]), one year late -2.6 ([W5.3b]) and -4.4 ([W5.3e]), one year early at 18
/// still 0.3 ([W5.3c]) and 0.0 ([W5.3f]).
#[test]
fn the_timing_moves_the_spurt() {
    for (sex, early_12, late_12, early_18) in
        [(Sex::Male, 4.7, -2.6, 0.3), (Sex::Female, 4.9, -4.4, 0.0)]
    {
        let curve = common::data().genetics().growth(sex);
        let shift = |t: i64, by: f64| {
            mean_height(&curve, YEAR * t, curve.theta + by)
                - mean_height(&curve, YEAR * t, curve.theta)
        };
        assert!(
            (shift(12, -1.0) - early_12).abs() <= 0.05,
            "{sex:?} {}",
            shift(12, -1.0)
        );
        assert!(
            (shift(12, 1.0) - late_12).abs() <= 0.05,
            "{sex:?} {}",
            shift(12, 1.0)
        );
        assert!(
            (shift(18, -1.0) - early_18).abs() <= 0.05,
            "{sex:?} {}",
            shift(18, -1.0)
        );
    }
}

/// ADR-0010 section 2: with the largest rates the checks allow and an early timing, the
/// denominator of Preece-Baines overflows long before 130 years; the curve is then its
/// limit `h1`, never NaN or infinite.
#[test]
fn the_curve_stays_finite_to_130_years() {
    let (mut calibration, genome_map) = common::files();
    for (key, value) in [
        ("core:growth_s0_male", 9.0),
        ("core:growth_s1_male", 10.0),
        ("core:growth_theta_male", 0.5),
    ] {
        calibration["values"][key]["value"] = serde_json::json!(value);
    }
    let data = sim_core::data::Data::new(
        common::definitions(calibration, genome_map).expect("definitions"),
    )
    .expect("checked");
    let curve = data.genetics().growth(Sex::Male);
    for t in [3_i64, 50, 80, 130] {
        let height = mean_height(&curve, YEAR * t, curve.theta);
        assert!(height.is_finite(), "{t}: {height}");
    }
    assert_eq!(mean_height(&curve, YEAR * 130, curve.theta), curve.h1);
}

/// ADR-0012 section 1: `z_h` and `z_p` are held at ±6 each for itself, only as inputs of
/// the curve; exactly ±6 is not held.
#[test]
fn the_inputs_are_held_at_six() {
    assert_eq!(CURVE_INPUT_BOUND, 6.0);
    for sex in [Sex::Male, Sex::Female] {
        let curve = common::data().genetics().growth(sex);
        let new = |z_h, z_p| IndividualCurve::new(curve, z_h, z_p);
        for (z_h, z_p, held_h, held_p) in [
            (7.0, 0.0, 6.0, 0.0),
            (-7.0, 0.0, -6.0, 0.0),
            (0.0, 7.0, 0.0, 6.0),
            (1.5, -1e300, 1.5, -6.0),
            (8.0, -9.0, 6.0, -6.0),
        ] {
            let outside = new(z_h, z_p);
            let held = new(held_h, held_p);
            assert!(outside.clamped, "{sex:?} {z_h} {z_p}");
            assert!(!held.clamped, "{sex:?} {held_h} {held_p}");
            assert_eq!(outside.theta.to_bits(), held.theta.to_bits());
            assert_eq!(outside.target.to_bits(), held.target.to_bits());
            assert_eq!(outside.kappa.to_bits(), held.kappa.to_bits());
            for t in [0, 2, 12, 40] {
                assert_eq!(
                    outside.height(YEAR * t).to_bits(),
                    held.height(YEAR * t).to_bits()
                );
            }
        }
        for (z_h, z_p) in [(6.0, 6.0), (-6.0, -6.0), (0.0, 0.0), (5.999, -5.999)] {
            assert!(!new(z_h, z_p).clamped, "{sex:?} {z_h} {z_p}");
        }
    }
}

/// ADR-0012 sections 2 and 3: at the four corners of the bounds, for both sexes, `θ_i`
/// and `h1_i` as the formula gives them with the values of the specification
/// (`θ ± 6 · σ_θ`, `h1 ± 6 · σ ± 6 · γ · σ_θ`, 5.2 and 6.1), and every height on a grid
/// of a quarter year up to 130 years finite and above 0; up to 25 years it rises (5.2).
#[test]
fn the_individual_curve_is_finite_and_positive_at_the_bounds() {
    // (sex, z_h, z_p, θ_i, h1_i): boys θ 14.02, σ_θ 0.81, h1 180.7, σ 7.01; girls 12.30,
    // 0.89, 167.3, 6.67; γ 1.
    let corners = [
        (Sex::Male, -6.0, -6.0, 9.16, 133.78),
        (Sex::Male, 6.0, 6.0, 18.88, 227.62),
        (Sex::Male, -6.0, 6.0, 18.88, 143.5),
        (Sex::Male, 6.0, -6.0, 9.16, 217.9),
        (Sex::Female, -6.0, -6.0, 6.96, 121.94),
        (Sex::Female, 6.0, 6.0, 17.64, 212.66),
        (Sex::Female, -6.0, 6.0, 17.64, 132.62),
        (Sex::Female, 6.0, -6.0, 6.96, 201.98),
    ];
    for (sex, z_h, z_p, theta, target) in corners {
        let curve = common::data().genetics().growth(sex);
        let individual = IndividualCurve::new(curve, z_h, z_p);
        assert!(
            (individual.theta - theta).abs() < 1e-9,
            "{sex:?} {z_h} {z_p}"
        );
        assert!(
            (individual.target - target).abs() < 1e-9,
            "{sex:?} {z_h} {z_p}"
        );
        assert!(
            (individual.kappa - target / curve.h1).abs() < 1e-12,
            "{sex:?} {z_h} {z_p}"
        );
        let mut previous = f64::NEG_INFINITY;
        for q in 0..=520_i64 {
            let at = SimDuration::from_secs(q * YEAR.as_secs() / 4);
            let height = individual.height(at);
            assert!(
                height.is_finite() && height > 0.0,
                "{sex:?} {z_h} {z_p} {q}/4"
            );
            if q <= 100 {
                assert!(height > previous, "{sex:?} {z_h} {z_p} {q}/4");
            }
            previous = height;
        }
        assert!(
            (individual.height(YEAR * 130) - target).abs() < 1e-9,
            "{sex:?} {z_h} {z_p}"
        );
    }
}

/// ADR-0012 section 3, specification 5.2: with `z = 0` the individual curve is the mean
/// curve, `κ` scales every height, and one year later maturity makes the adult `γ` = 1 cm
/// taller.
#[test]
fn the_individual_curve_follows_the_specification() {
    for (sex, sd, theta_sd) in [(Sex::Male, 7.01, 0.81), (Sex::Female, 6.67, 0.89)] {
        let curve = common::data().genetics().growth(sex);
        let mean = IndividualCurve::new(curve, 0.0, 0.0);
        assert_eq!(mean.kappa, 1.0);
        for k in 0..=60_i64 {
            let at = SimDuration::from_secs(k * YEAR.as_secs() / 2);
            assert_eq!(
                mean.height(at).to_bits(),
                mean_height(&curve, at, curve.theta).to_bits(),
                "{sex:?} {k}/2"
            );
        }
        let tall = IndividualCurve::new(curve, 1.0, 0.0);
        assert!((tall.target - (curve.h1 + sd)).abs() < 1e-9, "{sex:?}");
        for t in [0, 5, 12, 30] {
            let expected = (curve.h1 + sd) / curve.h1 * mean_height(&curve, YEAR * t, curve.theta);
            assert!(
                (tall.height(YEAR * t) - expected).abs() < 1e-9,
                "{sex:?} {t}"
            );
        }
        let late = IndividualCurve::new(curve, 0.0, 1.0 / theta_sd);
        assert!((late.theta - (curve.theta + 1.0)).abs() < 1e-9, "{sex:?}");
        assert!((late.target - (curve.h1 + 1.0)).abs() < 1e-9, "{sex:?}");
        let adult = late.height(YEAR * 40) - mean.height(YEAR * 40);
        assert!((adult - 1.0).abs() < 0.001, "{sex:?} {adult}");
    }
}
