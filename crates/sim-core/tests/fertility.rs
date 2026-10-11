//! Fertility as pure functions (genetics specification 5.6 and 11.3, ADR-0017 section 4):
//! the factors of woman and man, the menarche, the chance of an attempt and the wish.
//! Expected values come from the specification (W5.6a to W5.6c, W11.3a) and from
//! `tools/world_reference.py` (`tools/fertility_values.py`), never from a run of this crate.

mod common;

use sim_core::fertility::{
    Curves, Puberty, chance, female_factor, male_factor, menarche, theta_i, wish,
};

/// The curves of 5.6 (K5.6a to K5.6j) as the repository has them.
fn puberty() -> Puberty {
    match common::data().genetics().fertility {
        Curves::Puberty(p) => p,
        Curves::Window { .. } => panic!("the repository uses the curves of 5.6"),
    }
}

/// The wish shape of 11.3 (K11.3d).
const SHAPE: [f64; 5] = [0.10, 0.15, 0.40, 0.25, 0.10];

#[test]
fn the_repository_has_the_values_of_5_6() {
    assert_eq!(
        puberty(),
        Puberty {
            menarche_mean: 0.2,
            menarche_sd: 0.6,
            menarche_low: -1.0,
            menarche_high: 3.0,
            female_ramp: 3.0,
            female_decline_from: 30.0,
            female_decline_to: 50.0,
            male_lead: 0.7,
            male_ramp: 2.0,
            male_decline_from: 45.0,
            male_floor: 0.4,
            male_decline_to: 70.0,
        }
    );
}

/// W5.6a: 1.5 years after the menarche the woman's factor is 0.5; W5.6b: at 40 it is 0.5;
/// W5.6c: the man's at 57.5 is 0.7.
#[test]
fn w5_6_the_values_of_the_specification() {
    let curves = Curves::Puberty(puberty());
    assert_eq!(female_factor(14.0, 12.5, &curves), 0.5);
    assert_eq!(female_factor(40.0, 12.5, &curves), 0.5);
    let man = male_factor(57.5, 13.5, &curves);
    assert!((man - 0.7).abs() < 1e-12, "{man}");
}

/// The shape of the curves: the woman's factor is 0 up to her menarche, rises, is 1 from
/// the end of the ramp to 30, falls, and is 0 from 50 on; the man's is 0 until 0.7 years
/// before his `θ_i`, 1 from the end of his ramp to 45, and the floor from 70 on.
#[test]
fn the_curves_rise_hold_and_fall() {
    let curves = Curves::Puberty(puberty());
    let m = 12.5;
    let female = |age: f64| female_factor(age, m, &curves);
    assert_eq!(female(0.0), 0.0);
    assert_eq!(female(m), 0.0);
    assert!(female(m + 0.1) > 0.0);
    assert_eq!(female(m + 3.0), 1.0);
    assert_eq!(female(30.0), 1.0);
    assert_eq!(female(50.0), 0.0);
    assert_eq!(female(90.0), 0.0);
    let ages: Vec<f64> = (0..=600).map(|tenth| f64::from(tenth) / 10.0).collect();
    for pair in ages.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b <= 30.0 {
            assert!(female(b) >= female(a), "rises until 30: {a}");
        } else {
            assert!(female(b) <= female(a), "falls after 30: {a}");
        }
    }
    let theta = 13.5;
    let male = |age: f64| male_factor(age, theta, &curves);
    assert_eq!(male(theta - 0.7), 0.0);
    assert!(male(theta) > 0.0);
    assert_eq!(male(theta + 1.5), 1.0);
    assert_eq!(male(45.0), 1.0);
    assert_eq!(male(70.0), 0.4);
    assert_eq!(male(95.0), 0.4);
}

/// The menarche is `θ_i + δ`, held on `[θ_i − 1, θ_i + 3]`.
#[test]
fn the_menarche_is_held() {
    let p = puberty();
    assert_eq!(menarche(12.0, -1.5, &p), 11.0);
    assert_eq!(menarche(12.0, 3.5, &p), 15.0);
    assert_eq!(menarche(12.0, -1.0, &p), 11.0);
    assert_eq!(menarche(12.0, 0.25, &p), 12.25);
}

/// The window of the test data: the woman from `from` until before `until`, the man
/// always.
#[test]
fn the_window_of_the_test_data() {
    let window = Curves::Window {
        from: 18.0,
        until: 45.0,
    };
    assert_eq!(female_factor(17.9, 0.0, &window), 0.0);
    assert_eq!(female_factor(18.0, 0.0, &window), 1.0);
    assert_eq!(female_factor(44.9, 0.0, &window), 1.0);
    assert_eq!(female_factor(45.0, 0.0, &window), 0.0);
    assert_eq!(male_factor(10.0, 99.0, &window), 1.0);
}

/// Without a trait of the puberty timing `θ_i` is the `θ` of the curve, exactly (11.1);
/// with one it is held at ±6 like the growth curve.
#[test]
fn theta_i_without_and_with_the_trait() {
    let genetics = common::data().genetics();
    for curve in [genetics.growth_female, genetics.growth_male] {
        assert_eq!(theta_i(curve, 0.0), curve.theta);
        assert_eq!(theta_i(curve, -7.0), theta_i(curve, -6.0));
        assert!(theta_i(curve, 1.5) > curve.theta);
    }
}

/// The chance is the adult rate times both factors (11.3).
#[test]
fn the_chance_is_the_product() {
    assert_eq!(chance(0.2, 1.0, 1.0), 0.2);
    assert_eq!(chance(0.2, 0.0, 1.0), 0.0);
    assert_eq!(chance(0.2, 1.0, 0.0), 0.0);
}

/// W11.3a: without the tilt the wish has the mean 2.10 of its shape, and the draw `u`
/// picks the smallest `n` whose cumulative weight lies above it; the tilt moves the wish
/// up.
#[test]
fn w11_3a_the_wish() {
    let mean: f64 = (0..)
        .zip(SHAPE)
        .map(|(n, p): (u32, f64)| f64::from(n) * p)
        .sum();
    assert!((mean - 2.10).abs() < 1e-12, "{mean}");
    let untilted: Vec<u32> = [0.0, 0.05, 0.099, 0.2, 0.5, 0.8, 0.95, 0.999_999]
        .iter()
        .map(|u| wish(*u, &SHAPE, 0.0))
        .collect();
    assert_eq!(untilted, [0, 0, 0, 1, 2, 3, 4, 4]);
    for u in [0.05, 0.3, 0.6, 0.85, 0.95] {
        assert!(wish(u, &SHAPE, 0.311) >= wish(u, &SHAPE, 0.0), "{u}");
    }
    // A shape of the test data: always 40 (ADR-0017 section 5).
    let mut always = vec![0.0; 40];
    always.push(1.0);
    assert_eq!(wish(0.0, &always, 0.311), 40);
    assert_eq!(wish(0.999, &always, 0.311), 40);
}

/// The values of `tools/fertility_values.py` (the model of `tools/world_reference.py`),
/// bit for bit: `θ_i` on the repository's curves, the factors, the chance, and the wish
/// with its thresholds (11.3: `u < acc`, so a `u` on a threshold takes the next `n`).
#[test]
fn the_values_of_the_python_reference() {
    let genetics = common::data().genetics();
    let (women, men) = (genetics.growth_female, genetics.growth_male);
    assert_eq!(
        [0.0, 1.5, -7.0].map(|z| theta_i(women, z)),
        [12.3, 13.635_000_000_000_002, 6.960_000_000_000_001]
    );
    assert_eq!(
        [0.0, 1.5, -7.0].map(|z| theta_i(men, z)),
        [14.02, 15.235, 9.16]
    );
    let p = puberty();
    assert_eq!(
        [0.2, -1.5, 3.5, -1.0].map(|delta| menarche(12.0, delta, &p)),
        [12.2, 11.0, 15.0, 11.0]
    );
    let curves = Curves::Puberty(p);
    let female = [
        12.0, 12.5, 13.0, 14.0, 15.5, 16.0, 20.0, 30.0, 35.0, 40.0, 49.9, 50.0, 60.0,
    ]
    .map(|age| female_factor(age, 12.5, &curves));
    assert_eq!(
        female,
        [
            0.0,
            0.0,
            0.074_074_074_074_074_07,
            0.5,
            1.0,
            1.0,
            1.0,
            1.0,
            0.75,
            0.5,
            0.005_000_000_000_000_071,
            0.0,
            0.0
        ]
    );
    let male = [
        12.0, 12.8, 13.3, 13.8, 14.8, 30.0, 45.0, 50.0, 57.5, 70.0, 80.0,
    ]
    .map(|age| male_factor(age, 13.5, &curves));
    assert_eq!(
        male,
        [0.0, 0.0, 0.156_25, 0.5, 1.0, 1.0, 1.0, 0.88, 0.7, 0.4, 0.4]
    );
    assert_eq!(chance(0.2, 0.75, 0.5), 0.075_000_000_000_000_01);
    let us = [0.0, 0.05, 0.1, 0.25, 0.5, 0.65, 0.9, 0.99, 0.999_999];
    assert_eq!(
        us.map(|u| wish(u, &SHAPE, 0.311)),
        [0, 1, 1, 2, 2, 3, 4, 4, 4]
    );
    assert_eq!(
        us.map(|u| wish(u, &SHAPE, 0.0)),
        [0, 0, 1, 2, 2, 3, 4, 4, 4]
    );
    // The thresholds of the tilted wish: just below and at each.
    let thresholds: [f64; 4] = [
        0.049_206_408_370_637_1,
        0.149_940_972_006_932_13,
        0.516_558_163_078_150_6,
        0.829_280_157_250_367,
    ];
    for (n, at) in (0..).zip(thresholds) {
        let below = f64::from_bits(at.to_bits() - 1);
        assert_eq!(wish(below, &SHAPE, 0.311), n, "below threshold {n}");
        assert_eq!(wish(at, &SHAPE, 0.311), n + 1, "at threshold {n}");
    }
}

/// A shape without a positive weight breaks the invariant (the data check rules it out):
/// the wish never divides by a total of 0.
#[test]
#[should_panic(expected = "invariant fertility.wish")]
fn a_wish_without_a_positive_weight_breaks_the_invariant() {
    wish(0.5, &[0.0, 0.0], 0.0);
}
