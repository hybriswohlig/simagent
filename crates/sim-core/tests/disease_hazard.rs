//! The hazard of a hereditary disease in two steps (genetics specification 3.1, 3.2,
//! ADR-0017 section 3): the age at death from a budget and the hazard used up between two
//! ages. Expected values come from the specification and from Python, never from a run of
//! this crate.

use sim_core::mortality::{StepHazard, disease_age, disease_hazard_used};

/// `core:disease_ar` (3.2): `H_18` = 0.02 (K3.2b), so `h0` = 0.02 / 18 (W3.2j), and `h1`
/// such that `H(65) = ln 2` (K3.2l): `h1 = (ln 2 − 0.02) / 47` (W3.2k).
const CYSTIC: StepHazard = StepHazard {
    before: 0.001_111_111_111_111_111_1,
    change_age: 18.0,
    after: 0.014_322_280_437_445_644,
};

/// `core:disease_xr` (3.2): `H_18` = 0.01 (K3.2m), `h1` = 0.005 (K3.2h).
const HAEMOPHILIA: StepHazard = StepHazard {
    before: 0.000_555_555_555_555_555_6,
    change_age: 18.0,
    after: 0.005,
};

/// The ages at death against Python's `math` (independent of this crate):
///
/// ```text
/// python3 -c "
/// import math
/// def age(x0,B,h0,s,h1):
///     if x0>=s: return x0+B/h1 if h1>0 else None
///     H=h0*(s-x0)
///     if B<=H and H>0: return x0+B/h0
///     return s+(B-H)/h1 if h1>0 else None
/// h0=0.02/18; h1=(math.log(2)-0.02)/47
/// print(age(0,0.01,h0,18,h1), age(0,math.log(2),h0,18,h1), age(20,0.5,h0,18,h1),
///       age(10,0.05,h0,18,h1), age(5,0.1,0,18,0.04), age(45,1,0.04,0,0.04),
///       age(3,0.004,0.01/18,18,0.005))"
/// ```
#[test]
fn w3_1_ages_at_death_meet_python() {
    let close = |got: f64, want: f64| (got - want).abs() <= 1e-13 * want;
    let edge = StepHazard {
        before: 0.0,
        change_age: 18.0,
        after: 0.04,
    };
    let constant = StepHazard {
        before: 0.04,
        change_age: 0.0,
        after: 0.04,
    };
    let cases = [
        // Used up before the change.
        (0.0, 0.01, CYSTIC, 9.0),
        // The median of 3.2: `H(65) = ln 2`.
        (0.0, core::f64::consts::LN_2, CYSTIC, 65.0),
        // From an origin after the change.
        (20.0, 0.5, CYSTIC, 54.910_641_652_620_384),
        // Across the change.
        (10.0, 0.05, CYSTIC, 20.870_430_535_882_12),
        // An `h0` of 0 is the old edge: nobody dies before the change.
        (5.0, 0.1, edge, 20.5),
        // A constant hazard from its onset.
        (45.0, 1.0, constant, 70.0),
        (3.0, 0.004, HAEMOPHILIA, 10.2),
    ];
    for (origin, budget, hazard, want) in cases {
        let got = disease_age(origin, budget, hazard).expect("a death");
        assert!(
            close(got, want),
            "{origin} {budget} {hazard:?}: {got}, want {want}"
        );
    }
}

/// The budget of `H_18` runs out exactly at 18 (K3.2b, K3.2m), so a person dies of it
/// before 18 with `1 − e^(−H_18)` (W3.2l 0.01980, W3.2n 0.00995): the share of budgets
/// below `H_18`.
#[test]
fn w3_2_the_budget_of_h18_runs_out_at_18() {
    let at = disease_age(0.0, 0.02, CYSTIC).expect("a death");
    assert!((at - 18.0).abs() <= 1e-13 * 18.0, "{at}");
    let at = disease_age(0.0, 0.01, HAEMOPHILIA).expect("a death");
    assert!((at - 18.0).abs() <= 1e-13 * 18.0, "{at}");
    assert!(disease_age(0.0, 0.019_999, CYSTIC).expect("a death") < 18.0);
    assert!(disease_age(0.0, 0.020_001, CYSTIC).expect("a death") > 18.0);
}

/// Without a hazard after the change a budget beyond what is used before it is never
/// used up.
#[test]
fn without_a_hazard_after_the_change_the_rest_never_kills() {
    let early = StepHazard {
        before: 0.1,
        change_age: 10.0,
        after: 0.0,
    };
    assert!(disease_age(0.0, 0.5, early).is_some());
    assert_eq!(disease_age(0.0, 1.5, early), None);
    assert_eq!(disease_age(12.0, 0.1, early), None);
}

/// A budget of 0 dies at the origin, but with `h0` of 0 not before the change: nobody
/// dies of it before `s` (3.1).
#[test]
fn a_budget_of_zero_dies_at_the_origin_or_at_the_edge() {
    let edge = StepHazard {
        before: 0.0,
        change_age: 18.0,
        after: 0.04,
    };
    assert_eq!(disease_age(5.0, 0.0, edge), Some(18.0));
    assert_eq!(disease_age(5.0, 0.0, CYSTIC), Some(5.0));
    assert_eq!(disease_age(20.0, 0.0, CYSTIC), Some(20.0));
    // The smallest positive `h0` over a short stretch: with any positive `h0` a budget of 0
    // dies at the origin (review: the rule reads `h0`, not the product `H`).
    let short = StepHazard {
        before: 1e-9,
        change_age: 0.25,
        after: 0.0,
    };
    assert_eq!(disease_age(0.0, 0.0, short), Some(0.0));
}

/// Every accepted input gives a finite age (review): the largest budget over the smallest
/// hazard, from the oldest origin.
#[test]
fn every_accepted_input_gives_a_finite_age() {
    let slow = StepHazard {
        before: 1e-9,
        change_age: 130.0,
        after: 1e-9,
    };
    for origin in [0.0, 129.0, 1e6] {
        let age = disease_age(origin, 1000.0, slow).expect("a death");
        assert!(age.is_finite(), "{origin}: {age}");
    }
    let fast = StepHazard {
        before: 1000.0,
        change_age: 0.0,
        after: 1000.0,
    };
    assert!(disease_hazard_used(fast, 0.0, 1e6).is_finite());
}

/// The hazard used up between two ages (3.1, the switch counts only the time it is on):
/// `h0` on the part before the change, `h1` on the part after it.
///
/// ```text
/// python3 -c "
/// import math
/// h0=0.02/18; h1=(math.log(2)-0.02)/47
/// cum=lambda a,b: h0*max(0.0,min(b,18)-min(a,18))+h1*max(0.0,b-max(a,18))
/// print(cum(0,18), cum(10,30), cum(25,40))"
/// ```
#[test]
fn w3_1_the_hazard_used_up_meets_python() {
    let close = |got: f64, want: f64| (got - want).abs() <= 1e-15 + 1e-13 * want;
    let cases = [
        (0.0, 18.0, 0.02),
        (10.0, 30.0, 0.180_756_254_138_236_62),
        (25.0, 40.0, 0.214_834_206_561_684_64),
        (7.0, 7.0, 0.0),
    ];
    for (from, to, want) in cases {
        let got = disease_hazard_used(CYSTIC, from, to);
        assert!(close(got, want), "{from} {to}: {got}, want {want}");
    }
}

/// What is used up from an origin to an age is the budget that dies at that age.
#[test]
fn the_hazard_used_up_and_the_age_at_death_agree() {
    for origin in [0.0, 4.5, 18.0, 30.0] {
        for age in [5.0, 17.9, 18.0, 18.1, 44.0, 90.0] {
            if age <= origin {
                continue;
            }
            let budget = disease_hazard_used(CYSTIC, origin, age);
            let back = disease_age(origin, budget, CYSTIC).expect("a death");
            assert!((back - age).abs() <= 1e-12 * age, "{origin} {age}: {back}");
        }
    }
}

#[test]
fn inputs_outside_the_ranges_break_invariants() {
    let breaks = |call: Box<dyn FnOnce() + std::panic::UnwindSafe>, invariant: &str| {
        let payload = std::panic::catch_unwind(call).expect_err("an invariant break");
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .expect("a panic message");
        assert_eq!(message, format!("invariant {invariant}"));
    };
    let with = |before: f64, change_age: f64, after: f64| StepHazard {
        before,
        change_age,
        after,
    };
    let age_breaks = |origin: f64, budget: f64, hazard: StepHazard, invariant: &str| {
        breaks(
            Box::new(move || {
                let _ = disease_age(origin, budget, hazard);
            }),
            invariant,
        );
    };
    let used_breaks = |from: f64, to: f64| {
        breaks(
            Box::new(move || {
                let _ = disease_hazard_used(CYSTIC, from, to);
            }),
            "mortality.ages",
        );
    };
    age_breaks(0.0, 1.0, with(-1e-3, 18.0, 0.01), "mortality.disease");
    age_breaks(0.0, 1.0, with(0.01, 18.0, 1001.0), "mortality.disease");
    age_breaks(0.0, 1.0, with(0.01, -1.0, 0.01), "mortality.disease");
    age_breaks(0.0, 1.0, with(0.01, 131.0, 0.01), "mortality.disease");
    age_breaks(0.0, 1.0, with(f64::NAN, 18.0, 0.01), "mortality.disease");
    // Below the smallest hazard of a cause (review: `B / h` overflowed).
    age_breaks(18.0, 1.0, with(0.01, 18.0, 1e-310), "mortality.disease");
    age_breaks(
        0.0,
        0.0,
        with(f64::from_bits(1), 0.25, 0.0),
        "mortality.disease",
    );
    age_breaks(-1.0, 1.0, CYSTIC, "mortality.origin");
    age_breaks(1.1e6, 1.0, CYSTIC, "mortality.origin");
    age_breaks(0.0, -1.0, CYSTIC, "mortality.budget");
    used_breaks(10.0, 9.0);
    used_breaks(-1.0, 9.0);
    used_breaks(0.0, f64::INFINITY);
    used_breaks(0.0, f64::MAX);
}
