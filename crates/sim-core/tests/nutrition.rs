//! The loss of height from nutrition (genetics specification 5.4, ADR-0013 sections 3 and
//! 4): `v_i(t) = k · ∫_0^t ω_i(a) · (1 − N(birth + a)) da` with `k` = 5 cm per deficit
//! year and the windows 1 below 2 years, 0.5 from 2 to 5, 0.25 from `θ_i − 2` to
//! `θ_i + 1` but not below 5. The expected values are those of the specification or
//! written out by hand from it.

mod common;

use sim_core::finite::FiniteF64;
use sim_core::growth::nutrition_loss;
use sim_core::params::NutritionYear;
use sim_core::time::{DAY, SimDuration, SimTime, YEAR, year_start};

/// A pubertal timing far from every window of these tests but the one it names.
const THETA: f64 = 13.0;

fn year(year: i64, index: f64) -> NutritionYear {
    NutritionYear {
        year,
        index: FiniteF64::try_new(index).expect("finite"),
    }
}

/// The loss of a person born at `born` at the age of `age`, with the timing `theta`.
fn loss(years: &[NutritionYear], born: SimTime, age: SimDuration, theta: f64) -> f64 {
    let data = common::data();
    nutrition_loss(years, &data.genetics().nutrition, born, age, theta)
}

fn adult() -> SimDuration {
    YEAR * 25
}

#[test]
fn the_calibration_holds_the_values_of_the_specification() {
    let n = common::data().genetics().nutrition;
    // [K5.4a] and the windows of 5.4.
    assert_eq!(n.per_deficit_year, 5.0);
    assert_eq!(
        [n.weight_infant, n.weight_child, n.weight_puberty],
        [1.0, 0.5, 0.25]
    );
    assert_eq!([n.age_infant, n.age_child], [2.0, 5.0]);
    assert_eq!([n.puberty_before, n.puberty_after], [2.0, 1.0]);
}

/// Without a year below 1 there is no loss; a year with 1 counts as none.
#[test]
fn full_years_cost_nothing() {
    let born = year_start(0);
    assert_eq!(loss(&[], born, adult(), THETA), 0.0);
    assert_eq!(loss(&[year(1, 1.0)], born, adult(), THETA), 0.0);
}

/// [W5.4a] and [W5.4b]: one year with 0.7 in the first two years of life costs 1.50 cm,
/// with 0.5 it costs 2.50 cm; 0 costs `k` = 5 cm.
#[test]
fn one_year_in_the_first_window() {
    let born = year_start(0);
    for (index, cm) in [(0.7, 1.5), (0.5, 2.5), (0.0, 5.0)] {
        let at = loss(&[year(1, index)], born, adult(), THETA);
        assert!((at - cm).abs() < 1e-9, "{index}: {at}");
    }
}

/// [W5.4c]: a shock year `Y` (0.7) costs every person born in `Y − 1` exactly 1.50 cm,
/// whatever the birthday.
#[test]
fn the_shock_cohort_loses_the_same() {
    let shock = [year(10, 0.7)];
    let first = year_start(9);
    for born in [
        first,
        first + DAY * 100,
        first + DAY * 363 + SimDuration::from_secs(86_399),
    ] {
        let at = loss(&shock, born, adult(), THETA);
        assert!((at - 1.5).abs() < 1e-9, "{}: {at}", born.as_secs());
    }
}

/// [W5.4d] to [W5.4f]: with births at noon of each of the 364 days of a year, the mean
/// loss of the cohorts `Y` (0.750 cm), `Y − 2` (1.125 cm) and `Y − 4` (0.750 cm).
#[test]
fn the_other_cohorts_lose_by_their_birthday() {
    let shock = [year(10, 0.7)];
    for (cohort, mean) in [(10, 0.75), (8, 1.125), (6, 0.75)] {
        let total: f64 = (0..364)
            .map(|day| {
                let born = year_start(cohort) + DAY * day + SimDuration::from_secs(43_200);
                loss(&shock, born, adult(), THETA)
            })
            .sum();
        let at = total / 364.0;
        assert!((at - mean).abs() < 1e-9, "cohort {cohort}: {at}");
    }
}

/// 5.4: the loss counts from the deficit on and stays: half the shock year is half the
/// loss, and nothing is lost before the year.
#[test]
fn the_loss_grows_through_the_year_and_stays() {
    let shock = [year(1, 0.7)];
    let born = year_start(0);
    assert_eq!(loss(&shock, born, YEAR, THETA), 0.0);
    let half = loss(&shock, born, YEAR + DAY * 182, THETA);
    assert!((half - 0.75).abs() < 1e-9, "{half}");
    let end = loss(&shock, born, YEAR * 2, THETA);
    assert!((end - 1.5).abs() < 1e-9, "{end}");
    assert_eq!(
        loss(&shock, born, YEAR * 80, THETA).to_bits(),
        end.to_bits()
    );
}

/// 5.4: from 5 years on only the puberty window counts, `θ_i − 2` to `θ_i + 1`, with 0.25:
/// a whole year inside it costs 5 · 0.3 · 0.25 = 0.375 cm, a year between the windows
/// nothing, and a year half in it half that.
#[test]
fn the_puberty_window_follows_theta() {
    let born = year_start(0);
    let at = |shock_year: i64, theta: f64| loss(&[year(shock_year, 0.7)], born, adult(), theta);
    // Age 11 to 12 lies in [11, 14) for θ = 13.
    assert!((at(11, 13.0) - 0.375).abs() < 1e-9, "{}", at(11, 13.0));
    // Age 7 to 8 lies between the windows for θ = 13.
    assert_eq!(at(7, 13.0), 0.0);
    // Age 13 to 14 lies in [12, 15) for θ = 14, not at all for θ = 11 ([9, 12)).
    assert!((at(13, 14.0) - 0.375).abs() < 1e-9);
    assert_eq!(at(13, 11.0), 0.0);
    // θ = 11.5: the window [9.5, 12.5) holds half of age 9 to 10.
    assert!((at(9, 11.5) - 0.1875).abs() < 1e-9, "{}", at(9, 11.5));
    // The window reaches one year past θ: age 12 to 13 for θ = 12, not age 13 to 14.
    assert!((at(12, 12.0) - 0.375).abs() < 1e-9, "{}", at(12, 12.0));
    assert_eq!(at(13, 12.0), 0.0);
}

/// 5.4: below 5 years only the age windows count, even if the puberty window would start
/// earlier (θ_i = 6.5 starts it at 4.5): age 4 to 5 costs 0.5, age 5 to 6 costs 0.25.
#[test]
fn below_five_only_the_age_windows_count() {
    let born = year_start(0);
    let at = |shock_year: i64| loss(&[year(shock_year, 0.7)], born, adult(), 6.5);
    assert!((at(4) - 0.75).abs() < 1e-9, "{}", at(4));
    assert!((at(5) - 0.375).abs() < 1e-9, "{}", at(5));
    // Age 2 to 3 in the second window: 5 · 0.3 · 0.5.
    assert!((at(2) - 0.75).abs() < 1e-9, "{}", at(2));
}

/// A year long before the birth, at the edge of time, costs nothing and is skipped without
/// a time difference out of range; so is a year after the age reached.
#[test]
fn years_outside_the_life_cost_nothing() {
    let born = year_start(10);
    let years = [year(-1_000_000, 0.0), year(999_999, 0.0)];
    assert_eq!(loss(&years, born, adult(), THETA), 0.0);
}

/// Several years add up in the order of the list; years before the birth cost nothing.
#[test]
fn years_add_up() {
    let born = year_start(0);
    let years = [year(-3, 0.0), year(0, 0.9), year(1, 0.7), year(3, 0.5)];
    // Year 0 in window 1: 5 · 0.1 = 0.5; year 1 in window 1: 1.5; year 3 (age 3 to 4)
    // in window 2: 5 · 0.5 · 0.5 = 1.25.
    let at = loss(&years, born, adult(), THETA);
    assert!((at - 3.25).abs() < 1e-9, "{at}");
}
