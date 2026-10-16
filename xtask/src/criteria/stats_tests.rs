//! The estimators against the spec's W values and their definitions, and against the
//! vectors of `tools/criteria_reference.py` in `tests/golden/criteria/estimators.json`: bit
//! for bit where only `+ − × /` and `sqrt` enter, within `1e-12 · max(1, |x|)` through `ln`
//! and `Φ` (the fields the fixture lists as inexact).

use super::*;
use serde_json::Value;

const ESTIMATORS: &str = include_str!("../../../tests/golden/criteria/estimators.json");

/// The W values of the spec carry four digits (P7: within 5e-5).
fn near_w(value: f64, w: f64) -> bool {
    (value - w).abs() < 0.000_05
}

/// A value the case defines.
fn some(value: Option<f64>) -> f64 {
    value.expect("defined")
}

#[test]
fn the_tolerances_give_the_w_values_at_the_minimum_sizes() {
    let p3 = some(p3_tolerance(MIN_P3));
    let p4 = some(p4_tolerance(MIN_P4));
    let fisher_min = some(fisher_tolerance(MIN_FISHER));
    let all_p2 = vec![(true, mc1r_expectation(1)); MIN_MC1R];
    let mc1r = mc1r_tolerance(&mc1r_share(&all_p2).expect("families"));
    assert!(near_w(p3, 0.0906), "W8.3a: {p3}");
    assert!(near_w(p4, 0.2532), "W8.4a: {p4}");
    assert!(
        near_w(fisher_min, 0.1267),
        "W8.5a, W8.7a, W8.9f: {fisher_min}"
    );
    assert!(near_w(mc1r, 0.0896), "W8.9c: {mc1r}");
    // The note's six digits (design note section 6).
    for (value, six) in [(p3, 0.090_627), (p4, 0.253_236), (fisher_min, 0.126_681)] {
        assert!((value - six).abs() < 5e-7, "{value} against {six}");
    }
    assert!((mc1r - 0.089_553).abs() < 5e-7, "{mc1r}");
    // P4's band on the ratio of variances (W8.4b, W8.4c).
    assert!((sim_math::exp(p4) - 1.288).abs() < 5e-4);
    assert!((sim_math::exp(0.0 - p4) - 0.776).abs() < 5e-4);
}

#[test]
fn the_tolerances_follow_n() {
    // Four times the families halve P3's band; a quarter doubles it (exact scaling).
    let p3 = some(p3_tolerance(MIN_P3));
    assert_eq!(
        some(p3_tolerance(4 * MIN_P3)).to_bits(),
        (p3 / 2.0).to_bits()
    );
    assert_eq!(
        some(p3_tolerance(MIN_P3 / 4)).to_bits(),
        (p3 * 2.0).to_bits()
    );
    assert_eq!(some(p4_tolerance(3)), 4.0, "4 sqrt(2 / 2)");
    assert_eq!(some(p4_tolerance(9)), 2.0, "4 sqrt(2 / 8)");
    assert_eq!(some(fisher_tolerance(4)), 4.0, "4 / sqrt(1)");
    assert_eq!(some(fisher_tolerance(403)), 0.2, "4 / sqrt(400)");
    assert!(
        (some(never_tolerance(9)) - 0.4).abs() < 1e-15,
        "4 sqrt(0.01)"
    );
    assert!(
        (some(never_tolerance(100)) - 0.12).abs() < 1e-15,
        "4 sqrt(0.0009)"
    );
    let share = mc1r_share(&[(true, 0.5); 16]).expect("families");
    // 4 sqrt(16 · 0.25) / 16 = 0.5.
    assert_eq!(mc1r_tolerance(&share), 0.5);
}

#[test]
fn undefined_inputs_have_no_value() {
    assert_eq!(mean(&[]), None);
    assert_eq!(variance(&[1.0]), None);
    assert_eq!(pearson(&[]), None);
    assert_eq!(pearson(&[(1.0, 2.0)]), None);
    assert_eq!(pearson(&[(1.0, 2.0), (1.0, 3.0)]), None, "constant x");
    assert_eq!(pearson(&[(1.0, 2.0), (2.0, 2.0)]), None, "constant y");
    assert_eq!(slope(&[(1.0, 2.0)]), None);
    assert_eq!(slope(&[(1.0, 2.0), (1.0, 3.0)]), None, "constant x");
    assert_eq!(slope(&[(1.0, 2.0), (2.0, 2.0)]), Some(0.0), "constant y");
    for r in [1.0, -1.0, 1.5, f64::NAN] {
        assert_eq!(fisher(r), None, "{r}");
    }
    for se in [0.0, -1.0, f64::NAN] {
        assert_eq!(power(0.0, 0.1, 0.2, se), None, "{se}");
    }
    assert_eq!(p3_tolerance(0), None);
    assert_eq!(p4_tolerance(1), None);
    assert_eq!(fisher_tolerance(3), None);
    assert_eq!(never_tolerance(0), None);
    assert_eq!(mc1r_share(&[]), None);
    assert_eq!(mean_se(&[5]), None);
}

#[test]
fn the_powers_give_the_w_values() {
    // The power under the alternative's standard error, as the spec derives it
    // (`tools/spec_reference.py`, `slope` and `proportion`).
    let p3 = some(p3_tolerance(MIN_P3));
    let p3_alt = sqrt((1.0 - 1.0 * 1.0 * 0.6) / (0.6 * 2000.0));
    assert!(near_w(some(power(0.8, p3, 1.0, p3_alt)), 1.0), "W8.3b");
    let p4 = some(p4_tolerance(MIN_P4));
    let p4_power = some(power(0.0, p4, ln(0.664), p4 / 4.0));
    assert!(near_w(p4_power, 0.9932), "W8.4d: {p4_power}");
    let z = some(fisher_tolerance(MIN_FISHER));
    let se = z / 4.0;
    let f = |r: f64| some(fisher(r));
    let cases = [
        ("W8.5b siblings", f(0.514), f(0.257), 1.0),
        ("W8.5c strangers", 0.0, f(0.2), 0.9918),
        ("W8.9e polygenic, W8.7b", f(0.4), f(0.2), 0.9985),
        ("W8.7d height", f(0.2), 0.0, 0.9918),
    ];
    for (name, target, alt, w) in cases {
        let value = some(power(target, z, alt, se));
        assert!(near_w(value, w), "{name}: {value}");
    }
    let all_p2 = mc1r_share(&vec![(true, mc1r_expectation(1)); MIN_MC1R]).expect("families");
    let mc1r_alt = sqrt(0.5 * 0.5 / 400.0);
    let mc1r = some(power(
        all_p2.expected,
        mc1r_tolerance(&all_p2),
        0.5,
        mc1r_alt,
    ));
    assert!(near_w(mc1r, 1.0), "W8.9d: {mc1r}");
}

#[test]
fn the_expectation_of_p9_follows_k5() {
    // E_1 = p² (W8.9b); E_2 and E_3 as the note derives them from the formula.
    assert!((mc1r_expectation(1) - 0.7225).abs() < 1e-15);
    assert!(near_w(mc1r_expectation(2), 0.7574));
    assert!(near_w(mc1r_expectation(3), 0.7809));
    let more: Vec<f64> = (1..=8).map(mc1r_expectation).collect();
    assert!(
        more.windows(2).all(|pair| pair[0] < pair[1]),
        "rises with n: {more:?}"
    );
}

#[test]
fn phi_is_the_normal_distribution() {
    assert!((phi(0.0) - 0.5).abs() < 1e-15);
    for x in [0.3, 1.0, 1.5, 3.0, 6.0] {
        let sum = phi(x) + phi(0.0 - x);
        assert!((sum - 1.0).abs() < 1e-15, "{x}: {sum}");
        assert!(phi(x) > phi(x - 0.1));
    }
    assert!(phi(-40.0) < 1e-30);
    assert!((phi(40.0) - 1.0).abs() < 1e-15);
}

#[test]
fn the_moments_follow_their_definitions() {
    assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    assert!((some(variance(&[1.0, 2.0, 3.0, 4.0])) - 5.0 / 3.0).abs() < 1e-15);
    let line = [(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)];
    assert!((some(slope(&line)) - 2.0).abs() < 1e-15);
    assert!((some(pearson(&line)) - 1.0).abs() < 1e-15);
    let falling = [(0.0, 1.0), (1.0, 0.0), (2.0, -1.0)];
    assert!((some(pearson(&falling)) + 1.0).abs() < 1e-15);
    assert_eq!(fisher(0.0), Some(0.0));
    assert!((some(fisher(0.5)) + some(fisher(-0.5))).abs() < 1e-15);
    let (births, se) = mean_se(&[1, 2, 3]).expect("three women");
    assert!((births - 2.0).abs() < 1e-15 && (se - sqrt(1.0 / 3.0)).abs() < 1e-15);
}

#[test]
fn the_share_of_p9_sums_the_families() {
    let (e1, e2) = (mc1r_expectation(1), mc1r_expectation(2));
    let share = mc1r_share(&[(true, e1), (false, e2)]).expect("families");
    assert!((share.value - 0.5).abs() < 1e-15);
    assert!((share.expected - (e1 + e2) / 2.0).abs() < 1e-15);
    let se = sqrt(e1 * (1.0 - e1) + e2 * (1.0 - e2)) / 2.0;
    assert!((share.se - se).abs() < 1e-15);
}

#[test]
fn the_median_needs_half_and_the_life_table_needs_every_age() {
    let mut hist = [0; MONTHS];
    hist[300] = 4;
    assert!(partnership_median(&hist, 5).is_none(), "4 of 9 had one");
    let median = partnership_median(&hist, 4).expect("4 of 8");
    // half = 4 = before (0) + 4: (300 − 0.5 + 4 / 4) / 13.
    assert!((median.median - 300.5 / 13.0).abs() < 1e-12);
    assert!(
        life_table(&[1, 0, 1], &[0, 0, 1]).is_none(),
        "an age without exposure"
    );
    assert!(
        life_table(&[1, 1, 1], &[1, 1, 0]).is_none(),
        "no death in the open age"
    );
    assert!(life_table(&[1, 1], &[0, 1, 1]).is_none(), "lengths differ");
    assert!(life_table(&[], &[]).is_none());
    // One open age only: e_0 = 1 / m_ω = exposure in years per death.
    let year = 31_449_600;
    let open = life_table(&[4 * year], &[2]).expect("a table");
    assert!((open.e0 - 2.0).abs() < 1e-15, "{open:?}");
    // Var = D / (y² m⁴) = 2 / (16 · 1/16) = 2.
    assert!((open.se - sqrt(2.0)).abs() < 1e-15, "{open:?}");
}

/// The median's window `[lo, hi)` of months, each end checked: the expected values are the
/// note's formula with the window worked out by hand.
#[test]
fn the_median_window_has_both_ends() {
    // Inside: n 10, half 5, before month 300 two; med (299.5 + 3/5) / 13, window [287, 314)
    // holds 1 + 5 + 2; one month lower or higher it would hold 7.
    let mut hist = [0; MONTHS];
    for (month, n) in [(286, 1), (287, 1), (300, 5), (313, 2)] {
        hist[month] = n;
    }
    let median = partnership_median(&hist, 1).expect("a median");
    assert_eq!(median.median, (299.5 + 3.0 / 5.0) / 13.0);
    assert_eq!(median.se, sqrt(0.25 / 10.0) / (8.0 / 10.0 / (27.0 / 13.0)));
    // Clipped at 650: month 649 holds 4 of 5, med (648.5 + 2.5 / 4) / 13, window [636, 650).
    let mut hist = [0; MONTHS];
    hist[649] = 4;
    let median = partnership_median(&hist, 1).expect("a median");
    assert_eq!(median.se, sqrt(0.25 / 5.0) / (4.0 / 5.0 / (14.0 / 13.0)));
    // Clipped at 0: month 2 holds 4 of 5, med (1.5 + 2.5 / 4) / 13, window [0, 16).
    let mut hist = [0; MONTHS];
    hist[2] = 4;
    let median = partnership_median(&hist, 1).expect("a median");
    assert_eq!(median.median, (1.5 + 2.5 / 4.0) / 13.0);
    assert_eq!(median.se, sqrt(0.25 / 5.0) / (4.0 / 5.0 / (16.0 / 13.0)));
}

#[test]
fn the_village_reads_s_and_its_expectation() {
    assert!((village_expectation(0.8, &[]) - 1.0).abs() < 1e-15);
    // N_e = 4 · 50 · 50 / 100 = 100: 0.8 · (1 − 1/200) + 0.2.
    assert!((village_expectation(0.8, &[(50, 50)]) - 0.996).abs() < 1e-15);
    assert!(
        (village_expectation(0.8, &[(50, 50), (50, 50)]) - (0.8 * 0.995 * 0.995 + 0.2)).abs()
            < 1e-15
    );
    let wide: Vec<f64> = (0..20)
        .map(|i| if i % 2 == 0 { 0.9 } else { 1.1 })
        .collect();
    let check = village(&wide, &[0.98; 20]).expect("a check");
    assert!(check.pass && check.power < 0.9, "{check:?}");
    let narrow: Vec<f64> = (0..20)
        .map(|i| if i % 2 == 0 { 0.89 } else { 0.91 })
        .collect();
    let check = village(&narrow, &[0.9; 20]).expect("a check");
    assert!(check.pass && check.power > 0.9, "{check:?}");
    let check = village(&narrow, &[0.95; 20]).expect("a check");
    assert!(!check.pass, "{check:?}");
}

#[test]
fn a_village_without_variance_or_spread_is_red() {
    let wide: Vec<f64> = (0..20)
        .map(|i| if i % 2 == 0 { 0.9 } else { 1.1 })
        .collect();
    assert_eq!(
        village(&wide[..19], &[0.98; 19]),
        Err(VillageFinding::BelowMinimum)
    );
    assert_eq!(
        village(&wide, &[0.98; 19]),
        Err(VillageFinding::BelowMinimum)
    );
    let mut one_flat = wide.clone();
    one_flat[7] = 0.0;
    assert_eq!(
        village(&one_flat, &[0.98; 20]),
        Err(VillageFinding::NoVariance)
    );
    // Twenty halves sum exactly, so every deviation is 0 (twenty times 0.98 would leave
    // about 2e-16 of rounding as spread).
    assert_eq!(
        village(&[0.5; 20], &[0.5; 20]),
        Err(VillageFinding::NoSpread)
    );
}

#[test]
fn the_permutations_are_permutations() {
    for seed in [SEED_P3, SEED_P5, SEED_P9_POLYGENIC, SEED_P9_MC1R] {
        let mut order = permutation(1000, seed);
        assert_ne!(order, (0..1000).collect::<Vec<_>>());
        order.sort_unstable();
        assert_eq!(order, (0..1000).collect::<Vec<_>>());
    }
    assert_eq!(permutation(1, SEED_P3), vec![0]);
    assert!(permutation(0, SEED_P3).is_empty());
}

fn fixture() -> Value {
    serde_json::from_str(ESTIMATORS).expect("the fixture is JSON")
}

fn bits(value: &Value) -> f64 {
    let text = value.as_str().expect("16 hex digits");
    f64::from_bits(u64::from_str_radix(text, 16).expect("hex"))
}

fn floats(value: &Value) -> Vec<f64> {
    value.as_array().expect("a list").iter().map(bits).collect()
}

fn counts(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|n| n.as_u64().expect("a count"))
        .collect()
}

/// Bit for bit, with the name in the message.
fn exact(name: &str, value: f64, want: &Value) {
    let want = bits(want);
    assert_eq!(
        value.to_bits(),
        want.to_bits(),
        "{name}: {value} against {want}"
    );
}

/// Within `1e-12 · max(1, |x|)`, for values through `ln` or `Φ`.
fn close(name: &str, value: f64, want: &Value) {
    let want = bits(want);
    let scale = if want.abs() > 1.0 { want.abs() } else { 1.0 };
    assert!(
        (value - want).abs() <= 1e-12 * scale,
        "{name}: {value} against {want}"
    );
}

#[test]
fn the_reference_pins_the_moments_and_the_slope() {
    let fixture = fixture();
    let pearson_case = &fixture["pearson"];
    let pairs: Vec<(f64, f64)> = floats(&pearson_case["x"])
        .into_iter()
        .zip(floats(&pearson_case["y"]))
        .collect();
    let [xx, yy, xy] = moments(&pairs);
    exact("s_xx", xx, &pearson_case["s_xx"]);
    exact("s_yy", yy, &pearson_case["s_yy"]);
    exact("s_xy", xy, &pearson_case["s_xy"]);
    exact("r", some(pearson(&pairs)), &pearson_case["r"]);
    exact("slope", some(slope(&pairs)), &pearson_case["slope"]);
    let xs: Vec<f64> = pairs.iter().map(|pair| pair.0).collect();
    let ys: Vec<f64> = pairs.iter().map(|pair| pair.1).collect();
    exact("mean_x", some(mean(&xs)), &pearson_case["mean_x"]);
    exact("mean_y", some(mean(&ys)), &pearson_case["mean_y"]);
    exact(
        "variance_x",
        some(variance(&xs)),
        &pearson_case["variance_x"],
    );
    exact(
        "variance_y",
        some(variance(&ys)),
        &pearson_case["variance_y"],
    );
    for case in fixture["fisher"].as_array().expect("cases") {
        close("F", some(fisher(bits(&case["r"]))), &case["F"]);
    }
    let births = &fixture["births"];
    let (value, se) = mean_se(&counts(&births["values"])).expect("women");
    exact("births mean", value, &births["mean"]);
    exact("births se", se, &births["se"]);
    let values: Vec<f64> = counts(&births["values"]).into_iter().map(real).collect();
    exact(
        "births variance",
        some(variance(&values)),
        &births["variance"],
    );
}

#[test]
fn the_reference_pins_the_median_and_the_life_table() {
    let fixture = fixture();
    let case = &fixture["median"];
    let mut hist = [0; MONTHS];
    for (month, n) in case["hist"].as_object().expect("months") {
        hist[month.parse::<usize>().expect("a month")] = n.as_u64().expect("a count");
    }
    let never = case["never"].as_u64().expect("a count");
    let median = partnership_median(&hist, never).expect("a median");
    exact("median", median.median, &case["median"]);
    exact("median se", median.se, &case["se"]);
    let table = &fixture["life_table"];
    let result = life_table(&counts(&table["E"]), &counts(&table["D"])).expect("a table");
    exact("e_0", result.e0, &table["e0"]);
    exact("se of e_0", result.se, &table["se"]);
}

#[test]
fn the_reference_pins_p9_phi_and_the_powers() {
    let fixture = fixture();
    for n in 1..=5_u32 {
        exact(
            "E_n",
            mc1r_expectation(n),
            &fixture["p9_expectation"][n.to_string()],
        );
    }
    for (x, want) in fixture["phi"].as_object().expect("points") {
        close("phi", phi(x.parse().expect("a number")), want);
    }
    for case in fixture["power"].as_array().expect("cases") {
        let [c, tol, alt, se] = ["c", "tol", "alt", "se"].map(|name| bits(&case[name]));
        close("power", some(power(c, tol, alt, se)), &case["power"]);
    }
    let at_min = &fixture["tolerances_at_min"];
    let p3 = some(p3_tolerance(MIN_P3));
    let p4 = some(p4_tolerance(MIN_P4));
    let z = some(fisher_tolerance(MIN_FISHER));
    exact("p3", p3, &at_min["p3"]);
    exact("p4", p4, &at_min["p4"]);
    exact("fisher", z, &at_min["fisher"]);
    exact("never", some(never_tolerance(100)), &at_min["never_n_100"]);
    let all_p2 = mc1r_share(&vec![(true, mc1r_expectation(1)); MIN_MC1R]).expect("families");
    let mc1r = mc1r_tolerance(&all_p2);
    exact("mc1r", mc1r, &at_min["mc1r"]);
    // The W rows' powers, each with the standard error under its alternative.
    let se = 1.0 / sqrt(997.0);
    let f = |r: f64| some(fisher(r));
    let p3_alt = sqrt((1.0 - 1.0 * 1.0 * 0.6) / (0.6 * 2000.0));
    let powers = [
        ("p3_power", power(0.8, p3, 1.0, p3_alt)),
        ("p4_power", power(0.0, p4, ln(0.664), sqrt(2.0 / 499.0))),
        ("p5_siblings_power", power(f(0.514), z, f(0.257), se)),
        ("p5_strangers_power", power(0.0, z, f(0.2), se)),
        ("p9_polygenic_power", power(f(0.4), z, f(0.2), se)),
        (
            "p9_mc1r_power",
            power(all_p2.expected, mc1r, 0.5, sqrt(0.5 * 0.5 / 400.0)),
        ),
        ("f11_r_k_power", power(f(0.4), z, f(0.2), se)),
        ("f11_r_h_power", power(f(0.2), z, 0.0, se)),
    ];
    let rows = fixture["w"].as_array().expect("rows");
    for (label, value) in powers {
        let row = rows
            .iter()
            .find(|row| row["label"] == label)
            .expect("a row");
        close(label, some(value), &row["value"]);
    }
}

#[test]
fn the_reference_pins_the_village_and_the_permutations() {
    let fixture = fixture();
    let case = &fixture["village"];
    let blocks: Vec<(u64, u64)> = case["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|block| {
            let [_, fathers, mothers] = [0, 1, 2].map(|i| block[i].as_u64().expect("a count"));
            (fathers, mothers)
        })
        .collect();
    exact(
        "E_i",
        village_expectation(0.8, &blocks),
        &case["expectation"],
    );
    for name in ["village", "village_drifted"] {
        let case = &fixture[name];
        let (v, e) = (floats(&case["v"]), floats(&case["e"]));
        let check = village(&v, &e).expect("a check");
        let want = &case["aggregate"];
        exact("v_bar", check.mean_v, &want["v_bar"]);
        exact("e_bar", check.mean_e, &want["e_bar"]);
        exact("s", check.s, &want["s"]);
        exact("se", check.se, &want["se"]);
        exact("tol", 4.0 * check.se, &want["tol"]);
        close("power", check.power, &want["power"]);
        assert_eq!(Some(check.pass), want["pass"].as_bool(), "{name}");
        // The twin: every V_i divided by V̄ (mean 1, no drift).
        let ratios: Vec<f64> = v.iter().map(|v_i| v_i / check.mean_v).collect();
        let twin = village(&ratios, &e).expect("a check");
        exact("twin v_bar", twin.mean_v, &want["twin_v_bar"]);
        exact("twin se", twin.se, &want["twin_se"]);
        assert_eq!(Some(twin.pass), want["twin_pass"].as_bool(), "{name} twin");
    }
    let firsts = &fixture["splitmix_first"];
    for (seed, name) in [
        (SEED_P3, "p3"),
        (SEED_P5, "p5"),
        (SEED_P9_POLYGENIC, "p9_polygenic"),
        (SEED_P9_MC1R, "p9_mc1r"),
    ] {
        let want: Vec<usize> = counts(&fixture["permutations"][name])
            .into_iter()
            .map(|i| usize::try_from(i).expect("small"))
            .collect();
        assert_eq!(permutation(10, seed), want, "{name}");
        let mut state = seed;
        for word in firsts[name].as_array().expect("words") {
            let text = word.as_str().expect("hex");
            assert_eq!(
                splitmix(&mut state),
                u64::from_str_radix(text, 16).expect("hex"),
                "{name}"
            );
        }
    }
}

fn pairs_of(case: &Value) -> Vec<(f64, f64)> {
    floats(&case["x"])
        .into_iter()
        .zip(floats(&case["y"]))
        .collect()
}

fn index(value: &Value) -> usize {
    usize::try_from(value.as_u64().expect("a count")).expect("small")
}

/// Review round 1: the undefined inputs, the tolerances away from their minimum, the
/// median's window at both ends, and the village's findings, all from the reference.
#[test]
fn the_reference_pins_the_boundaries() {
    let fixture = fixture();
    let undefined = &fixture["undefined"];
    for case in undefined["mean"].as_array().expect("cases") {
        assert_eq!(mean(&floats(&case["x"])), None);
    }
    for case in undefined["variance"].as_array().expect("cases") {
        assert_eq!(variance(&floats(&case["x"])), None);
    }
    for case in undefined["pearson"].as_array().expect("cases") {
        assert_eq!(pearson(&pairs_of(case)), None, "{case}");
    }
    for case in undefined["slope"].as_array().expect("cases") {
        assert_eq!(slope(&pairs_of(case)), None, "{case}");
    }
    let flat = &undefined["slope_constant_y_is_zero"];
    exact("slope", some(slope(&pairs_of(flat))), &flat["slope"]);
    for r in floats(&undefined["fisher"]) {
        assert_eq!(fisher(r), None, "{r}");
    }
    for case in undefined["power"].as_array().expect("cases") {
        let [c, tol, alt, se] = ["c", "tol", "alt", "se"].map(|name| bits(&case[name]));
        assert_eq!(power(c, tol, alt, se), None, "{se}");
    }
    for n in undefined["tol_p3"].as_array().expect("sizes") {
        assert_eq!(p3_tolerance(index(n)), None);
    }
    for n in undefined["tol_log_var"].as_array().expect("sizes") {
        assert_eq!(p4_tolerance(index(n)), None);
    }
    for n in undefined["tol_fisher"].as_array().expect("sizes") {
        assert_eq!(fisher_tolerance(index(n)), None);
    }
    for n in undefined["tol_never"].as_array().expect("sizes") {
        assert_eq!(never_tolerance(index(n)), None);
    }
    assert_eq!(mc1r_share(&[]), None, "no family");
    let one = &undefined["births_one_value"];
    let values: Vec<f64> = counts(&one["values"]).into_iter().map(real).collect();
    exact("mean of one", some(mean(&values)), &one["mean"]);
    assert!(one["variance"].is_null() && one["se"].is_null());
    assert_eq!(variance(&values), None);
    assert_eq!(mean_se(&counts(&one["values"])), None);
    // Near-constant data stay defined: the mean of three times 0.1 is not 0.1.
    let near = &fixture["defined_near_constant"];
    let pairs = pairs_of(near);
    let xs: Vec<f64> = pairs.iter().map(|pair| pair.0).collect();
    exact("mean_x", some(mean(&xs)), &near["mean_x"]);
    exact("s_xx", moments(&pairs)[0], &near["s_xx"]);
    exact("r", some(pearson(&pairs)), &near["r"]);
    exact("slope", some(slope(&pairs)), &near["slope"]);
    let more = &fixture["tolerances_more"];
    for (n, want) in more["p3"].as_object().expect("sizes") {
        exact("p3", some(p3_tolerance(n.parse().expect("n"))), want);
    }
    for (n, want) in more["p4"].as_object().expect("sizes") {
        exact("p4", some(p4_tolerance(n.parse().expect("n"))), want);
    }
    for (n, want) in more["fisher"].as_object().expect("sizes") {
        exact(
            "fisher",
            some(fisher_tolerance(n.parse().expect("n"))),
            want,
        );
    }
    for (n, want) in more["never"].as_object().expect("sizes") {
        exact("never", some(never_tolerance(n.parse().expect("n"))), want);
    }
    let half = mc1r_share(&[(true, 0.5); 16]).expect("families");
    exact("mc1r 16", mc1r_tolerance(&half), &more["mc1r_16_half"]);
    for (name, case) in fixture["median_windows"].as_object().expect("cases") {
        let mut hist = [0; MONTHS];
        for (month, n) in case["hist"].as_object().expect("months") {
            hist[month.parse::<usize>().expect("a month")] = n.as_u64().expect("a count");
        }
        let never = case["never"].as_u64().expect("a count");
        let median = partnership_median(&hist, never).expect("a median");
        exact(name, median.median, &case["median"]);
        exact(name, median.se, &case["se"]);
    }
    for (name, case) in fixture["village_findings"].as_object().expect("cases") {
        let finding = match case["result"]["finding"].as_str() {
            Some("below_minimum") => VillageFinding::BelowMinimum,
            Some("no_variance") => VillageFinding::NoVariance,
            Some("no_spread") => VillageFinding::NoSpread,
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(
            village(&floats(&case["v"]), &floats(&case["e"])),
            Err(finding),
            "{name}"
        );
    }
}

/// Review round 2: the life table's edges, from the reference.
#[test]
fn the_reference_pins_the_life_table_edges() {
    let fixture = fixture();
    for (name, case) in fixture["life_table_edges"].as_object().expect("cases") {
        let table = life_table(&counts(&case["E"]), &counts(&case["D"]));
        if case["finding"].is_null() {
            let table = table.expect("a table");
            exact(name, table.e0, &case["e0"]);
            exact(name, table.se, &case["se"]);
        } else {
            assert_eq!(table, None, "{name}");
        }
    }
}
