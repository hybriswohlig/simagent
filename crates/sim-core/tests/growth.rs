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
/// curve, `κ` scales every height, one year earlier or later maturity moves the spurt at
/// 12 years as in 5.2 (the height divided by `κ` against the mean curve: [W5.3a],
/// [W5.3b], [W5.3d], [W5.3e]), and one year later maturity makes the adult `γ` = 1 cm
/// taller.
#[test]
fn the_individual_curve_follows_the_specification() {
    for (sex, sd, theta_sd, early_12, late_12) in [
        (Sex::Male, 7.01, 0.81, 4.7, -2.6),
        (Sex::Female, 6.67, 0.89, 4.9, -4.4),
    ] {
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
        for (z_p, moved) in [(-1.0 / theta_sd, early_12), (1.0 / theta_sd, late_12)] {
            let individual = IndividualCurve::new(curve, 0.0, z_p);
            let at_12 = individual.height(YEAR * 12) / individual.kappa
                - mean_height(&curve, YEAR * 12, curve.theta);
            assert!((at_12 - moved).abs() <= 0.05, "{sex:?} {z_p}: {at_12}");
        }
        let late = IndividualCurve::new(curve, 0.0, 1.0 / theta_sd);
        assert!((late.theta - (curve.theta + 1.0)).abs() < 1e-9, "{sex:?}");
        assert!((late.target - (curve.h1 + 1.0)).abs() < 1e-9, "{sex:?}");
        let adult = late.height(YEAR * 40) - mean.height(YEAR * 40);
        assert!((adult - 1.0).abs() < 0.001, "{sex:?} {adult}");
    }
}

/// ADR-0012 section 3: with the widest values the checks allow (`σ_θ` 5 years, the largest
/// rates, an early and a late mean timing), every height of the four corners up to 130
/// years is finite.
#[test]
fn the_individual_curve_stays_finite_with_the_widest_values() {
    for theta in [0.5, 30.0] {
        let (mut calibration, genome_map) = common::files();
        for (key, value) in [
            ("core:growth_s0_male", 9.0),
            ("core:growth_s1_male", 10.0),
            ("core:growth_theta_male", theta),
            ("core:growth_theta_sd_male", 5.0),
        ] {
            calibration["values"][key]["value"] = serde_json::json!(value);
        }
        let data = sim_core::data::Data::new(
            common::definitions(calibration, genome_map).expect("definitions"),
        )
        .expect("checked");
        let curve = data.genetics().growth(Sex::Male);
        for (z_h, z_p) in [(-6.0, -6.0), (6.0, 6.0), (-6.0, 6.0), (6.0, -6.0)] {
            let individual = IndividualCurve::new(curve, z_h, z_p);
            assert!(individual.target > 0.0, "{theta} {z_h} {z_p}");
            for q in 0..=520_i64 {
                let at = SimDuration::from_secs(q * YEAR.as_secs() / 4);
                let height = individual.height(at);
                assert!(height.is_finite(), "{theta} {z_h} {z_p} {q}/4: {height}");
            }
        }
    }
}

/// ADR-0014 section 2, decided by Grok 4.7: the puberty progress is a smoothstep from
/// `θ_i − 2` to `θ_i + 2` (the half width 2 of the calibration): 0 before, 1 after, ½ at
/// `θ_i`, `3u² − 2u³` with `u = (x − θ_i + 2) / 4` in between, so 0.15625 one year
/// before and 0.84375 one year after.
#[test]
fn puberty_progress_is_a_smoothstep_around_theta() {
    let w = common::data().genetics().puberty_half_width;
    assert_eq!(w, 2.0);
    let theta = 13.0;
    let at = |years: i64| sim_core::growth::puberty_progress(YEAR * years, theta, w);
    for (years, p) in [
        (0, 0.0),
        (10, 0.0),
        (11, 0.0),
        (12, 0.15625),
        (13, 0.5),
        (14, 0.84375),
        (15, 1.0),
        (40, 1.0),
    ] {
        assert_eq!(
            at(years).to_bits(),
            f64::to_bits(p),
            "{years}: {}",
            at(years)
        );
    }
    // Another timing moves the curve: θ = 11.5 gives ½ at 11.5 years and u = 0.25 at 10.5.
    let half_year = SimDuration::from_secs(YEAR.as_secs() / 2);
    let early = |at: SimDuration| sim_core::growth::puberty_progress(at, 11.5, w);
    assert_eq!(early(YEAR * 11 + half_year), 0.5);
    assert_eq!(
        early(YEAR * 10 + half_year).to_bits(),
        0.15625_f64.to_bits()
    );
    assert_eq!(early(YEAR * 9 + half_year), 0.0);
    // Another half width moves both ends: with w = 1 the rise runs from 12 to 14, and at
    // 12.5 years u = 0.25.
    let narrow = |at: SimDuration| sim_core::growth::puberty_progress(at, theta, 1.0);
    assert_eq!(
        narrow(YEAR * 12 + half_year).to_bits(),
        0.15625_f64.to_bits()
    );
    assert_eq!(narrow(YEAR * 12), 0.0);
    assert_eq!(narrow(YEAR * 14), 1.0);
    // Between the quarter years it rises.
    let mut previous = -1.0;
    for q in 40..=64_i64 {
        let p = sim_core::growth::puberty_progress(
            SimDuration::from_secs(q * YEAR.as_secs() / 4),
            theta,
            w,
        );
        assert!(p >= previous, "{q}/4: {p}");
        previous = p;
    }
}

/// ADR-0014 section 2, decided by Grok 4.7 on Sorkin et al. (1999): the loss of height in
/// old age is `a · (b − 30)²`, 3/1600 cm per year² for men and 5/1600 for women: nothing up
/// to 30, 3 and 5 cm at 70, 4.6875 and 7.8125 cm at 80.
#[test]
fn height_loss_meets_sorkin() {
    let loss = common::data().genetics().height_loss;
    assert_eq!(
        [loss.onset, loss.male, loss.female],
        [30.0, 0.001_875, 0.003_125]
    );
    for (sex, at_70, at_80) in [(Sex::Male, 3.0, 4.6875), (Sex::Female, 5.0, 7.8125)] {
        for b in [0.0, 20.0, 30.0] {
            assert_eq!(
                sim_core::growth::height_loss(&loss, sex, b),
                0.0,
                "{sex:?} {b}"
            );
        }
        let seventy = sim_core::growth::height_loss(&loss, sex, 70.0);
        let eighty = sim_core::growth::height_loss(&loss, sex, 80.0);
        assert!((seventy - at_70).abs() < 1e-12, "{sex:?} {seventy}");
        assert!((eighty - at_80).abs() < 1e-12, "{sex:?} {eighty}");
        // At a fractional age the bits are those of `(a · (b − 30)) · (b − 30)`; at 61.1 the
        // other grouping `a · ((b − 30) · (b − 30))` gives other bits for both factors.
        let a = match sex {
            Sex::Male => 0.001_875,
            Sex::Female => 0.003_125,
        };
        let expected: f64 = a * (61.1 - 30.0) * (61.1 - 30.0);
        let at = sim_core::growth::height_loss(&loss, sex, 61.1);
        assert_eq!(at.to_bits(), expected.to_bits(), "{sex:?} {at}");
        let regrouped: f64 = a * ((61.1 - 30.0) * (61.1 - 30.0));
        assert_ne!(
            expected.to_bits(),
            regrouped.to_bits(),
            "{sex:?} the order shows"
        );
        // Faster with age: the decade from 70 to 80 costs more than the one before.
        let sixty = sim_core::growth::height_loss(&loss, sex, 60.0);
        assert!(eighty - seventy > seventy - sixty, "{sex:?}");
    }
}

/// ADR-0014: the loss reads its calibration: with the onset 40 and the factors 0.002 and
/// 0.004, nothing up to 40, 0.2 and 0.4 cm at 50.
#[test]
fn height_loss_reads_its_values() {
    let loss = sim_core::data::HeightLoss {
        onset: 40.0,
        male: 0.002,
        female: 0.004,
    };
    for (sex, at_50) in [(Sex::Male, 0.2), (Sex::Female, 0.4)] {
        assert_eq!(sim_core::growth::height_loss(&loss, sex, 40.0), 0.0);
        let at = sim_core::growth::height_loss(&loss, sex, 50.0);
        assert!((at - at_50).abs() < 1e-12, "{sex:?} {at}");
    }
}
