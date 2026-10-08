//! Death by senescence and by accident (genetics specification 7.1, 7.2 and 11.4,
//! ADR-0017): the closed form of the age at death from a hazard budget, the genetic
//! factor of longevity held at ±6, and the criteria P14 and P16 (8.3) on synthetic
//! newborns without kinship. Expected values come from the specification and from
//! Python, never from a run of this crate.

use sim_core::mortality::{
    Gompertz, LONGEVITY_INPUT_BOUND, accident_age, age_offset, held_longevity, multiplier,
    senescence_age,
};
use sim_math::dist::{exponential, normal};

/// `σ_g` (W7.2a).
const SIGMA_G: f64 = 0.724;
/// Women (W7.2b, W7.2c) and men (W7.2d, W7.2e).
const WOMEN: Gompertz = Gompertz {
    a: 3.25e-6,
    b: 0.1182,
};
const MEN: Gompertz = Gompertz {
    a: 8.73e-6,
    b: 0.1118,
};
/// The Makeham terms (K7.1a, K7.1b), per year.
const ACCIDENT_WOMEN: f64 = 1.5e-4;
const ACCIDENT_MEN: f64 = 2.0e-4;

/// The ages at death against Python's `math` (independent of this crate):
///
/// ```text
/// python3 -c "import math; sen=lambda x0,B,a,b,m: math.log(math.exp(b*x0)+b*B/(a*m))/b; \
///   print(sen(0,1,3.25e-6,0.1182,1), sen(30,0.5,8.73e-6,0.1118,1), \
///   sen(0,2,3.25e-6,0.1182,math.exp(-0.724*1.5)), sen(64,0.01,8.73e-6,0.1118,math.exp(0.724*2)))"
/// ```
///
/// The core may rearrange the formula (7.1 is a statement about numbers, ADR-0003); both
/// sides stay within a few ulp of the exact value, so a relative 1e-13 holds.
#[test]
fn w7_1_ages_at_death_meet_python() {
    let close = |got: f64, want: f64| (got - want).abs() <= 1e-13 * want;
    let cases = [
        (0.0, 1.0, WOMEN, 1.0, 88.845_227_436_585_18),
        (30.0, 0.5, MEN, 1.0, 78.434_825_532_710_79),
        (
            0.0,
            2.0,
            WOMEN,
            multiplier(Some(1.5), SIGMA_G),
            103.897_040_684_885_16,
        ),
        (
            64.0,
            0.01,
            MEN,
            multiplier(Some(-2.0), SIGMA_G),
            64.207_782_683_835_45,
        ),
    ];
    for (x0, budget, gompertz, m, want) in cases {
        let got = senescence_age(x0, budget, gompertz, m);
        assert!(close(got, want), "{x0} {budget} {m}: {got}, want {want}");
    }
    assert!(close(
        accident_age(20.0, 0.7, ACCIDENT_WOMEN),
        4_686.666_666_666_667
    ));
    // No budget: death at once.
    assert_eq!(senescence_age(40.0, 0.0, MEN, 1.0), 40.0);
    assert_eq!(accident_age(40.0, 0.0, ACCIDENT_MEN), 40.0);
}

/// 7.2: `m = e^(−σ_g · g_L)`; without a trait of longevity `m` = 1 and `Δ` = 0 (11.1);
/// `Δ = −σ_g · g_L / b`, positive is older.
#[test]
fn w7_2_multiplier_and_offset() {
    assert_eq!(multiplier(None, SIGMA_G), 1.0);
    assert_eq!(multiplier(Some(0.0), SIGMA_G), 1.0);
    assert_eq!(
        multiplier(Some(1.0), SIGMA_G).to_bits(),
        sim_math::exp(-0.724).to_bits()
    );
    assert_eq!(age_offset(None, SIGMA_G, WOMEN.b), 0.0);
    assert_eq!(age_offset(Some(0.0), SIGMA_G, WOMEN.b), 0.0);
    // One standard deviation of g_L is 6.13 (W7.4a) and 6.48 (W7.4b) years of age.
    assert!((age_offset(Some(-1.0), SIGMA_G, WOMEN.b) - 6.13).abs() < 0.005);
    assert!((age_offset(Some(-1.0), SIGMA_G, MEN.b) - 6.48).abs() < 0.005);
    assert!(
        age_offset(Some(1.0), SIGMA_G, MEN.b) < 0.0,
        "a high g_L is younger"
    );
}

/// 7.2: `m` acts like a shift of age by `Δ`: dying from `x0` with `m` is dying from
/// `x0 + Δ` without it, `Δ` years earlier.
#[test]
fn w7_2_the_multiplier_is_a_shift_of_age() {
    for g in [-2.5, -0.4, 0.0, 1.2, 3.0] {
        // Origins from 40 on, so that `x0 + Δ` stays an age (|Δ| is at most 18.4 here).
        for (x0, budget) in [(40.0, 0.3), (55.0, 1.0), (75.0, 4.0)] {
            let m = multiplier(Some(g), SIGMA_G);
            let delta = age_offset(Some(g), SIGMA_G, WOMEN.b);
            let with = senescence_age(x0, budget, WOMEN, m);
            let shifted = senescence_age(x0 + delta, budget, WOMEN, 1.0) - delta;
            assert!((with - shifted).abs() < 1e-9, "{g} {x0}: {with} {shifted}");
        }
    }
}

/// 11.4: `g_L` held at ±6 (`LONGEVITY_INPUT_BOUND`), exactly ±6 not held; death and
/// `bio_age` read the held value.
#[test]
fn w11_4_longevity_is_held_at_six() {
    assert_eq!(LONGEVITY_INPUT_BOUND, 6.0);
    for (g, held) in [
        (0.0, 0.0),
        (5.9, 5.9),
        (6.0, 6.0),
        (-6.0, -6.0),
        (6.000_001, 6.0),
        (-40.0, -6.0),
        (1e300, 6.0),
    ] {
        assert_eq!(held_longevity(g), held, "{g}");
    }
    assert_eq!(
        multiplier(Some(1e300), SIGMA_G),
        multiplier(Some(6.0), SIGMA_G)
    );
    assert_eq!(
        age_offset(Some(-1e300), SIGMA_G, MEN.b),
        age_offset(Some(-6.0), SIGMA_G, MEN.b)
    );
    // The bound in years: 36.8 (W11.4b) and 38.9 (W11.4c).
    assert!((age_offset(Some(-7.0), SIGMA_G, WOMEN.b) - 36.8).abs() < 0.05);
    assert!((age_offset(Some(7.0), SIGMA_G, MEN.b) + 38.9).abs() < 0.05);
}

/// A very old founder (7.1, B19) dies at once instead of never: the closed form must not
/// overflow at a large origin age.
#[test]
fn a_very_old_origin_dies_at_once() {
    for x0 in [130.0, 1_000.0, 100_000.0] {
        let x = senescence_age(x0, 1.0, MEN, 1.0);
        assert!(x.is_finite() && x >= x0 && x - x0 < 1.0, "{x0}: {x}");
    }
}

#[test]
#[should_panic(expected = "invariant mortality.longevity")]
fn a_longevity_that_is_not_finite_breaks_the_invariant() {
    held_longevity(f64::NAN);
}

#[test]
#[should_panic(expected = "invariant mortality.budget")]
fn a_negative_budget_breaks_the_invariant() {
    senescence_age(0.0, -1.0, WOMEN, 1.0);
}

#[test]
#[should_panic(expected = "invariant mortality.gompertz")]
fn a_gompertz_term_of_zero_breaks_the_invariant() {
    senescence_age(0.0, 1.0, Gompertz { a: 0.0, b: 0.1 }, 1.0);
}

#[test]
#[should_panic(expected = "invariant mortality.multiplier")]
fn a_multiplier_of_zero_breaks_the_invariant() {
    senescence_age(0.0, 1.0, WOMEN, 0.0);
}

#[test]
#[should_panic(expected = "invariant mortality.origin")]
fn a_negative_origin_breaks_the_invariant() {
    accident_age(-1.0, 1.0, ACCIDENT_MEN);
}

#[test]
#[should_panic(expected = "invariant mortality.accident")]
fn an_accident_rate_of_zero_breaks_the_invariant() {
    accident_age(0.0, 1.0, 0.0);
}

#[test]
#[should_panic(expected = "invariant mortality.sigma")]
fn a_negative_sigma_breaks_the_invariant() {
    multiplier(Some(1.0), -0.1);
}

// ---------------------------------------------------------------------------------------
// P14 and P16 (8.3): 20 000 newborns without kinship per sex (8.1), g_L standard normal as
// in the reference population (4.1), budgets Exp(1) per cause (7.1). The words come from a
// splitmix64 stream with a fixed seed per sex (gate seeds, protected list; never reseeded).

/// The newborns of one sex per criterion (P14, P16: at least 20 000).
const NEWBORNS: u32 = 20_000;
const SEED_WOMEN: u64 = 0x5eed_0017_0014_0001;
const SEED_MEN: u64 = 0x5eed_0017_0014_0002;

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Life span and `g_L` of each newborn: the earlier of senescence and accident.
fn cohort(seed: u64, gompertz: Gompertz, accident: f64, sigma_g: f64) -> Vec<(f64, f64)> {
    let mut state = seed;
    (0..NEWBORNS)
        .map(|_| {
            let g = normal(splitmix(&mut state), splitmix(&mut state), 0.0, 1.0);
            let senescence = exponential(splitmix(&mut state), 1.0);
            let mishap = exponential(splitmix(&mut state), 1.0);
            let m = multiplier(Some(g), sigma_g);
            let old = senescence_age(0.0, senescence, gompertz, m);
            let hit = accident_age(0.0, mishap, accident);
            (if hit < old { hit } else { old }, g)
        })
        .collect()
}

fn survival(lives: &[(f64, f64)], age: f64) -> f64 {
    let alive = lives.iter().filter(|(life, _)| *life > age).count();
    f64::from(u32::try_from(alive).expect("small")) / f64::from(NEWBORNS)
}

/// Pearson correlation of life span and `g_L`.
fn correlation(lives: &[(f64, f64)]) -> f64 {
    let n = f64::from(NEWBORNS);
    let (mean_t, mean_g) = lives.iter().fold((0.0, 0.0), |(t, g), (life, value)| {
        (t + life / n, g + value / n)
    });
    let (mut tg, mut tt, mut gg) = (0.0, 0.0, 0.0);
    for (life, value) in lives {
        let (dt, dg) = (life - mean_t, value - mean_g);
        tg += dt * dg;
        tt += dt * dt;
        gg += dg * dg;
    }
    tg / sim_math::sqrt(tt * gg)
}

fn fisher(r: f64) -> f64 {
    0.5 * sim_math::ln((1.0 + r) / (1.0 - r))
}

/// The tolerance of 8.1: 4 standard errors of a share `p` among the newborns.
fn share_tolerance(p: f64) -> f64 {
    4.0 * sim_math::sqrt(p * (1.0 - p) / f64::from(NEWBORNS))
}

/// The survival of 7.3 (W7.3e to W7.3g and W7.3m to W7.3o).
const SURVIVAL_WOMEN: [(f64, f64); 3] = [(65.0, 0.918), (80.0, 0.660), (90.0, 0.330)];
const SURVIVAL_MEN: [(f64, f64); 3] = [(65.0, 0.859), (80.0, 0.518), (90.0, 0.204)];

/// P14: survival to 65, 80 and 90 per sex as in 7.3, each within 4 standard errors (for
/// women to 80 that is 0.0134, W8.14a).
#[test]
fn p14_survival_curve() {
    assert!((share_tolerance(0.660) - 0.0134).abs() < 0.000_05);
    for (seed, gompertz, accident, expected) in [
        (SEED_WOMEN, WOMEN, ACCIDENT_WOMEN, SURVIVAL_WOMEN),
        (SEED_MEN, MEN, ACCIDENT_MEN, SURVIVAL_MEN),
    ] {
        let lives = cohort(seed, gompertz, accident, SIGMA_G);
        for (age, share) in expected {
            let got = survival(&lives, age);
            assert!(
                (got - share).abs() <= share_tolerance(share),
                "{gompertz:?} survival to {age}: {got}, want {share}"
            );
        }
    }
}

/// P16: the correlation of life span and `g_L` is √0.2 = 0.4472 (W8.16a) per sex, within
/// 0.0283 on Fisher's z scale (W8.16b), and positive.
#[test]
fn p16_heritability_of_lifespan() {
    let tolerance = 4.0 / sim_math::sqrt(f64::from(NEWBORNS) - 3.0);
    assert!((tolerance - 0.0283).abs() < 0.000_05);
    for (seed, gompertz, accident) in [
        (SEED_WOMEN, WOMEN, ACCIDENT_WOMEN),
        (SEED_MEN, MEN, ACCIDENT_MEN),
    ] {
        let r = correlation(&cohort(seed, gompertz, accident, SIGMA_G));
        assert!(r > 0.0, "{r}");
        assert!(
            (fisher(r) - fisher(sim_math::sqrt(0.2))).abs() <= tolerance,
            "{gompertz:?}: correlation {r}, want 0.4472"
        );
    }
}

/// The negative twin of P14 and P16 (8.1): without the genetic factor (`σ_g` = 0) the
/// correlation is gone and women live too long to 80 (about 0.695 instead of 0.660), so
/// both criteria must fail.
#[test]
fn p14_p16_fail_without_the_genetic_factor() {
    let lives = cohort(SEED_WOMEN, WOMEN, ACCIDENT_WOMEN, 0.0);
    let tolerance = 4.0 / sim_math::sqrt(f64::from(NEWBORNS) - 3.0);
    let r = correlation(&lives);
    assert!(
        (fisher(r) - fisher(sim_math::sqrt(0.2))).abs() > tolerance,
        "P16 passes without the factor: {r}"
    );
    let to_80 = survival(&lives, 80.0);
    assert!(
        (to_80 - 0.660).abs() > share_tolerance(0.660),
        "P14 passes without the factor: {to_80}"
    );
}

#[test]
#[should_panic(expected = "invariant mortality.gompertz")]
fn a_gompertz_slope_of_zero_breaks_the_invariant() {
    senescence_age(0.0, 1.0, Gompertz { a: 1e-5, b: 0.0 }, 1.0);
}

#[test]
#[should_panic(expected = "invariant mortality.gompertz")]
fn an_offset_without_slope_breaks_the_invariant() {
    age_offset(Some(1.0), SIGMA_G, 0.0);
}

#[test]
#[should_panic(expected = "invariant mortality.gompertz")]
fn an_offset_with_a_negative_slope_breaks_the_invariant() {
    age_offset(Some(1.0), SIGMA_G, -0.1);
}
