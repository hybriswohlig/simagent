//! The criteria report (design note of the criteria kit, sections 3 to 6): per check its
//! `n`, minimum, unit hash, value, target, tolerance, verdict, finding, power and negative
//! twin, in the schema of `tools/criteria_reference.py` (its NOTES, section 2). Floats are
//! written as their bits, so two reports compare exactly; a check lists in `inexact` the
//! fields that pass through `ln` or `Φ` (within `1e-12 · max(1, |x|)`).
//!
//! `cargo xtask criteria report <export.ndjson> [<out.json>]` writes it, by default as
//! `report.json` beside the export.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sim_core::hash::stable_hash64;
use sim_math::{ln, sqrt};

use super::export::read;
use super::stats::{
    LifeFinding, MIN_FISHER, MIN_MC1R, MIN_P3, MIN_P4, SEED_P3, SEED_P5, SEED_P9_MC1R,
    SEED_P9_POLYGENIC, Share, fisher, fisher_tolerance, life_table, mc1r_share, mc1r_tolerance,
    mean, mean_se, never_tolerance, p3_tolerance, p4_tolerance, partnership_median, pearson,
    permutation, power, real, slope, variance, village_expectation,
};
use super::units::{DECADES, LifeInput, Mc1r, Town, Trait, Units, Village, YEAR, unit_hash};

const USAGE: &str = "usage: cargo xtask criteria report <export.ndjson> [<out.json>]";

/// A decision this close to its boundary could flip under the 1e-12 rule (section 2).
const FRAGILE: f64 = 1e-9;
/// The bands of F11 (K11), inclusive: early births and the age gap in years.
const EARLY: [f64; 2] = [0.001, 0.005];
const GAP: [f64; 2] = [2.5, 4.0];
/// `h²` of `core:growth` in the village's expectation (K4.2a).
const VILLAGE_H2: f64 = 0.8;
/// The fields of a correlation check through `ln` or `Φ`.
const FISHER_INEXACT: [&str; 4] = ["value", "target", "power", "twin.value"];

/// A check's verdict: green, red with a finding, or neither (report only); fragile if the
/// margin that decided it lies within `FRAGILE` of the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Verdict {
    pass: Option<bool>,
    finding: Option<&'static str>,
    fragile: bool,
}

/// The verdict of a check that is reported only (F11 men).
const REPORT_ONLY: Verdict = Verdict {
    pass: None,
    finding: None,
    fragile: false,
};

impl Verdict {
    fn red(finding: &'static str) -> Verdict {
        Verdict {
            pass: Some(false),
            finding: Some(finding),
            fragile: false,
        }
    }

    fn of(outside: bool, margin: f64) -> Verdict {
        Verdict {
            pass: Some(!outside),
            finding: outside.then_some("outside"),
            fragile: margin.abs() <= FRAGILE,
        }
    }
}

/// `|value − target| ≤ tol` at `n` units: red below the minimum, then if the value or the
/// tolerance is undefined, then outside.
fn against(n: usize, min: usize, value: Option<f64>, target: f64, tol: Option<f64>) -> Verdict {
    if n < min {
        return Verdict::red("below_minimum");
    }
    match (value, tol) {
        (Some(value), Some(tol)) => {
            let off = (value - target).abs();
            Verdict::of(off > tol, off - tol)
        }
        _ => Verdict::red("undefined"),
    }
}

/// `value` in the inclusive band `[lo, hi]`.
fn within(value: Option<f64>, [lo, hi]: [f64; 2]) -> Verdict {
    let Some(value) = value else {
        return Verdict::red("undefined");
    };
    let (below, above) = (value - lo, hi - value);
    let margin = if below.abs() < above.abs() {
        below
    } else {
        above
    };
    Verdict::of(below < 0.0 || above < 0.0, margin)
}

fn bits(x: f64) -> Value {
    json!(format!("{:016x}", x.to_bits()))
}

fn maybe(x: Option<f64>) -> Value {
    x.map_or(Value::Null, bits)
}

fn hash(anchors: &[u64]) -> Value {
    json!(format!("{:016x}", unit_hash(anchors)))
}

fn size(n: usize) -> f64 {
    real(u64::try_from(n).unwrap_or_else(|_| panic!("invariant criteria.count")))
}

/// Fisher's z of a correlation the spec names; every one lies in `(−1, 1)`.
fn rho(r: f64) -> f64 {
    fisher(r).unwrap_or_else(|| panic!("invariant criteria.rho: {r}"))
}

/// The fields of the objects in turn, as one object.
fn merged<const N: usize>(parts: [Value; N]) -> Value {
    let mut all = Map::new();
    for part in parts {
        if let Value::Object(fields) = part {
            all.extend(fields);
        }
    }
    all.into()
}

/// What every check names: itself, its sample size and minimum, its anchors' hash and its
/// fields through `ln` or `Φ`.
fn head(name: &str, n: usize, min: usize, anchors: &[u64], inexact: &[&str]) -> Value {
    json!({"check": name, "n": n, "min_n": min, "units": hash(anchors), "inexact": inexact})
}

/// A decision: the value, its tolerance and the verdict.
fn decision(value: Option<f64>, tol: Option<f64>, verdict: Verdict) -> Value {
    json!({
        "value": maybe(value), "tol": maybe(tol), "pass": verdict.pass,
        "finding": verdict.finding, "fragile": verdict.fragile,
    })
}

/// A negative twin: its value against its check's target within the tolerance given.
fn twin(n: usize, min: usize, value: Option<f64>, target: f64, tol: Option<f64>) -> Value {
    decision(value, tol, against(n, min, value, target, tol))
}

/// A check's decision of `value` against `target` within `tol`.
fn judged(n: usize, min: usize, value: Option<f64>, target: f64, tol: Option<f64>) -> Value {
    merged([
        twin(n, min, value, target, tol),
        json!({"target": bits(target)}),
    ])
}

/// The pairs with their second values in the gate seed's fixed permutation.
fn permuted(pairs: &[(f64, f64)], seed: u64) -> Vec<(f64, f64)> {
    let order = permutation(pairs.len(), seed);
    (pairs.iter().zip(&order))
        .map(|(pair, j)| (pair.0, pairs[*j].1))
        .collect()
}

/// P3: the slope of the first-born's `z_h` on the midparent against 0.8, power against 1
/// with the alternative's error; the twin pairs each midparent with a permuted first-born.
fn p3(town: &Town) -> Value {
    let units = town.p3();
    let n = units.values.len();
    let tol = p3_tolerance(n);
    let se = (n > 0).then(|| sqrt((1.0 - 1.0 * 1.0 * 0.6) / (0.6 * size(n))));
    let twin_slope = slope(&permuted(&units.values, SEED_P3));
    merged([
        head("p3", n, MIN_P3, &units.anchors, &["power"]),
        judged(n, MIN_P3, slope(&units.values), 0.8, tol),
        json!({
            "power": maybe(tol.zip(se).and_then(|(tol, se)| power(0.8, tol, 1.0, se))),
            "twin": twin(n, MIN_P3, twin_slope, 0.8, tol),
        }),
    ])
}

/// P4 in the town, decade `k`: `ln V` of `z_h` against 0 within `4 sqrt(2 / (n − 1))`
/// (undefined unless `V > 0`), power against `ln 0.664` with the same error; the twin
/// scales every `z_h` by `sqrt(0.664)`.
fn decade(k: i64, units: &Units<f64>) -> Value {
    let n = units.values.len();
    let tol = p4_tolerance(n);
    let log = |values: &[f64]| {
        let v = variance(values);
        (v, v.filter(|v| *v > 0.0).map(ln))
    };
    let (v, value) = log(&units.values);
    let scale = sqrt(0.664);
    let scaled: Vec<f64> = units.values.iter().map(|z| z * scale).collect();
    let (twin_v, twin_value) = log(&scaled);
    let inexact = ["value", "power", "twin.value"];
    merged([
        head(&format!("p4_city_{k}"), n, MIN_P4, &units.anchors, &inexact),
        judged(n, MIN_P4, value, 0.0, tol),
        json!({
            "k": k,
            "variance": maybe(v),
            "power": maybe(tol.and_then(|tol| power(0.0, tol, ln(0.664), tol / 4.0))),
            "twin": merged([
                twin(n, MIN_P4, twin_value, 0.0, tol),
                json!({"variance": maybe(twin_v)}),
            ]),
        }),
    ])
}

/// P4 in the town: green iff every decade is; the twin test likewise.
fn p4_city(town: &Town) -> Value {
    let decades: Vec<Value> = (0..DECADES).map(|k| decade(k, &town.p4_city(k))).collect();
    let all = |pass: fn(&Value) -> &Value| decades.iter().all(|d| pass(d) == &json!(true));
    json!({
        "check": "p4_city",
        "pass": all(|d| &d["pass"]),
        "twin_pass": all(|d| &d["twin"]["pass"]),
        "decades": decades,
    })
}

/// Pearson's r of the pairs and its Fisher z.
fn fisher_z(pairs: &[(f64, f64)]) -> (Option<f64>, Option<f64>) {
    let r = pearson(pairs);
    (r, r.and_then(fisher))
}

/// A correlation on Fisher's scale against `target` within `4 / sqrt(n − 3)`, power against
/// `alt` with the error `1 / sqrt(n − 3)` of both models.
fn correlation(name: &str, units: &Units<(f64, f64)>, target: f64, alt: f64) -> Value {
    let n = units.values.len();
    let (r, value) = fisher_z(&units.values);
    let tol = fisher_tolerance(n);
    merged([
        head(name, n, MIN_FISHER, &units.anchors, &FISHER_INEXACT),
        judged(n, MIN_FISHER, value, target, tol),
        json!({
            "r": maybe(r),
            "power": maybe(tol.and_then(|tol| power(target, tol, alt, tol / 4.0))),
        }),
    ])
}

/// The twin of a correlation: the pairs given, against `target` within their tolerance.
fn correlation_twin(pairs: &[(f64, f64)], target: f64) -> Value {
    let (n, (r, value)) = (pairs.len(), fisher_z(pairs));
    let twin = twin(n, MIN_FISHER, value, target, fisher_tolerance(n));
    merged([twin, json!({"r": maybe(r)})])
}

/// P5: siblings against 0.514, their twin with the second siblings permuted; strangers
/// against 0, their twin the sibling pairs taken as strangers.
fn p5(town: &Town) -> [Value; 2] {
    let (siblings, strangers) = (town.p5_siblings(), town.p5_strangers());
    let target = rho(0.514);
    let twin = correlation_twin(&permuted(&siblings.values, SEED_P5), target);
    let as_strangers = correlation_twin(&siblings.values, 0.0);
    let units = json!({"units": hash(&siblings.anchors)});
    [
        merged([
            correlation("p5_siblings", &siblings, target, rho(0.257)),
            json!({"twin": twin}),
        ]),
        merged([
            correlation("p5_strangers", &strangers, 0.0, rho(0.2)),
            json!({"twin": merged([as_strangers, units])}),
        ]),
    ]
}

/// P9 polygenic: a parent in turn with the first-born on `z_pig` against 0.4; the twin
/// permutes the first-borns.
fn p9_polygenic(town: &Town) -> Value {
    let units = town.p9_polygenic();
    let target = rho(0.4);
    let twin = correlation_twin(&permuted(&units.values, SEED_P9_POLYGENIC), target);
    merged([
        correlation("p9_polygenic", &units, target, rho(0.2)),
        json!({"twin": twin}),
    ])
}

/// P9 on `core:mc1r` (K5): the share of affected families with unaffected parents against
/// `Σ E_f / N` within `4 sqrt(Σ E_f (1 − E_f)) / N`, power against 0.5 with the
/// alternative's error; red with `q` alone outside its band. The twin gives the sample
/// family of canonical index `i` (among all families) the parents of family `π[i]`.
fn mc1r(sample: &Mc1r) -> Value {
    let values = &sample.units.values;
    let n = values.len();
    let q = json!({
        "q": bits(sample.q), "q_parents": sample.parents, "q_copies": sample.copies,
        "affected": n,
    });
    let anchors = &sample.units.anchors;
    if !sample.in_band() {
        return merged([
            head("p9_mc1r", n, MIN_MC1R, anchors, &[]),
            decision(None, None, Verdict::red("q_outside")),
            json!({"target": null, "twin": {"pass": false, "finding": "q_outside"}}),
            q,
        ]);
    }
    let order = permutation(sample.unaffected.len(), SEED_P9_MC1R);
    let drawn: Vec<(bool, f64)> = (sample.families.iter().zip(values))
        .map(|(i, unit)| (sample.unaffected[order[*i]], unit.1))
        .collect();
    let yes = |units: &[(bool, f64)]| units.iter().filter(|unit| unit.0).count();
    // Each share against its own expectation and tolerance (the twin keeps every `E_f`);
    // without an affected family there is no share, and n = 0 is below the minimum.
    let judge = |share: Option<Share>| {
        let verdict = share.map_or(Verdict::red("below_minimum"), |s| {
            let tol = Some(mc1r_tolerance(&s));
            against(n, MIN_MC1R, Some(s.value), s.expected, tol)
        });
        decision(
            share.map(|s| s.value),
            share.map(|s| mc1r_tolerance(&s)),
            verdict,
        )
    };
    let (share, twin_share) = (mc1r_share(values), mc1r_share(&drawn));
    let power = share.and_then(|s| {
        power(
            s.expected,
            mc1r_tolerance(&s),
            0.5,
            sqrt(0.5 * 0.5 / size(n)),
        )
    });
    merged([
        head("p9_mc1r", n, MIN_MC1R, anchors, &["power"]),
        judge(share),
        q,
        json!({
            "target": maybe(share.map(|s| s.expected)),
            "k": yes(values),
            "sum_e": bits(values.iter().fold(0.0, |sum, unit| sum + unit.1)),
            "sum_var": bits(values.iter().fold(0.0, |sum, unit| sum + unit.1 * (1.0 - unit.1))),
            "power": maybe(power),
            "twin": merged([judge(twin_share), json!({"k": yes(&drawn)})]),
        }),
    ])
}

/// F11 median age at the first partnership (against 23.5 within `4 se`) and the share
/// never partnered by 50 (against 0.10 within `4 sqrt(0.09 / n)`) of town-born women; the
/// men's are reported only.
fn partnerships(town: &Town, female: bool) -> [Value; 2] {
    let sample = town.first_partnerships(female);
    let (n, sex) = (sample.anchors.len(), if female { "f" } else { "m" });
    let median = partnership_median(&sample.hist, sample.never);
    let (value, se) = (median.map(|m| m.median), median.map(|m| m.se));
    let tol = se.map(|se| 4.0 * se);
    let share = (n > 0).then(|| real(sample.never) / size(n));
    let never_tol = never_tolerance(n);
    let verdicts = if female {
        [
            against(n, 0, value, 23.5, tol),
            against(n, 0, share, 0.1, never_tol),
        ]
    } else {
        [REPORT_ONLY; 2]
    };
    let only = if female {
        json!({})
    } else {
        json!({"report_only": true})
    };
    let head = |name: &str| head(&format!("{name}_{sex}"), n, 0, &sample.anchors, &[]);
    [
        merged([
            head("f11_median"),
            decision(value, tol, verdicts[0]),
            json!({"target": bits(23.5), "se": maybe(se), "never": sample.never}),
            only.clone(),
        ]),
        merged([
            head("f11_never"),
            decision(share, never_tol, verdicts[1]),
            json!({"target": bits(0.1), "never": sample.never}),
            only,
        ]),
    ]
}

/// F11 births per woman reaching 50 in the town, against 2.20 within `4 sqrt(V / n)`.
fn births(units: &Units<u64>) -> Value {
    let n = units.values.len();
    let values: Vec<f64> = units.values.iter().map(|births| real(*births)).collect();
    let se = mean_se(&units.values).map(|(_, se)| se);
    merged([
        head("f11_births", n, 0, &units.anchors, &[]),
        judged(n, 0, mean(&values), 2.2, se.map(|se| 4.0 * se)),
        json!({
            "variance": maybe(variance(&values)),
            "se": maybe(se),
            "total": units.values.iter().sum::<u64>(),
        }),
    ])
}

/// A band check: no target, no tolerance.
fn banded(head: Value, value: Option<f64>, band: [f64; 2]) -> Value {
    let fields = json!({"target": null, "band": [bits(band[0]), bits(band[1])]});
    merged([head, decision(value, None, within(value, band)), fields])
}

/// F11 share of births to mothers under 18, in its band.
fn early_births(units: &Units<bool>) -> Value {
    let n = units.values.len();
    let early = units.values.iter().filter(|early| **early).count();
    let value = (n > 0).then(|| size(early) / size(n));
    let head = head("f11_early_births", n, 0, &units.anchors, &[]);
    merged([banded(head, value, EARLY), json!({"early": early})])
}

/// F11 mean age gap of the couples in years, in its band.
fn age_gap(units: &Units<f64>) -> Value {
    let head = head("f11_age_gap", units.values.len(), 0, &units.anchors, &[]);
    banded(head, mean(&units.values), GAP)
}

/// The name a life table's finding has in the report.
fn named(finding: LifeFinding) -> &'static str {
    match finding {
        LifeFinding::BadLengths => "bad_lengths",
        LifeFinding::ZeroExposure => "zero_exposure",
        LifeFinding::NoOpenDeaths => "no_open_deaths",
        LifeFinding::QAtLeastOne => "q_at_least_one",
        LifeFinding::LUnderflow => "l_underflow",
        LifeFinding::NotFinite => "not_finite",
    }
}

/// F11 life expectancy at birth of one sex against `target` within `4 se`, with the
/// table's input and the ages without exposure.
fn life(name: &str, target: f64, input: &LifeInput) -> Value {
    let n = input.anchors.len();
    let table = life_table(&input.exposure, &input.deaths);
    let (value, se) = match table {
        Ok(table) => (Some(table.e0), Some(table.se)),
        Err(_) => (None, None),
    };
    let tol = se.map(|se| 4.0 * se);
    let verdict = match table {
        Ok(_) => against(n, 0, value, target, tol),
        Err(finding) => Verdict::red(named(finding)),
    };
    let zero: Vec<usize> = (input.exposure.iter().enumerate())
        .filter(|(_, e)| **e == 0)
        .map(|(x, _)| x)
        .collect();
    merged([
        head(name, n, 0, &input.anchors, &[]),
        decision(value, tol, verdict),
        json!({
            "target": bits(target), "se": maybe(se), "exposure": &input.exposure[..],
            "deaths": &input.deaths[..], "zero_exposure": zero,
        }),
    ])
}

/// F11 `h²_T` of one sex (GK-2): Fisher's z of `r(T, gl)` against `F(sqrt 0.2)`, no
/// minimum and no power; `h²_T = r²`.
fn longevity(town: &Town, female: bool) -> Value {
    let sample = town.longevity(female);
    let n = sample.units.values.len();
    let (r, value) = fisher_z(&sample.units.values);
    let name = format!("f11_h2t_{}", if female { "f" } else { "m" });
    merged([
        head(&name, n, 0, &sample.units.anchors, &["value", "target"]),
        judged(n, 0, value, rho(sqrt(0.2)), fisher_tolerance(n)),
        json!({
            "r": maybe(r), "power": null, "h2": maybe(r.map(|r| r * r)),
            "emigrated": sample.emigrated, "living": sample.living,
        }),
    ])
}

/// The village block of this export (G4, G5, K2): its units' variance and expectation for
/// the village check over the seeds. Red without a block, with fewer than two units or
/// with nobody alive at the end.
fn village(years: i64, village: Option<&Village>) -> Value {
    let Some(village) = village else {
        return json!({
            "check": "village_block", "end_years": years, "g_star": null, "n": 0,
            "units": hash(&[]), "variance": null, "expectation": null, "blocks": [],
            "living": null, "finding": "no_block",
        });
    };
    let n = village.units.values.len();
    let finding = if n < 2 {
        Some("fewer_than_two_units")
    } else {
        (village.living == 0).then_some("no_living")
    };
    let pairs: Vec<(u64, u64)> = (village.blocks.iter()).map(|(_, m, f)| (*m, *f)).collect();
    let blocks: Vec<Value> = (village.blocks.iter())
        .map(|(g, m, f)| json!([g, m, f]))
        .collect();
    json!({
        "check": "village_block", "end_years": years, "g_star": village.block, "n": n,
        "units": hash(&village.units.anchors), "variance": maybe(variance(&village.units.values)),
        "expectation": bits(village_expectation(VILLAGE_H2, &pairs)), "blocks": blocks,
        "living": village.living, "finding": finding,
    })
}

/// The report of an export, given as the text of its file (its hash names it).
pub fn report(text: &str) -> Result<Value, String> {
    let export = read(text)?;
    let town = Town::new(&export)?;
    let end = export.header.end;
    if (end + 1).rem_euclid(YEAR) != 0 {
        return Err(format!("the end {end} is not the last second of a year"));
    }
    let firsts: Vec<u64> = town
        .families()
        .iter()
        .map(|family| family.children[0])
        .collect();
    let mut checks = vec![p3(&town), p4_city(&town)];
    checks.extend(p5(&town));
    checks.extend([
        p9_polygenic(&town),
        mc1r(&town.p9_mc1r()),
        correlation(
            "f11_r_k",
            &town.couple_pairs(Trait::Cognition),
            rho(0.4),
            rho(0.2),
        ),
        correlation("f11_r_h", &town.couple_pairs(Trait::Growth), rho(0.2), 0.0),
    ]);
    checks.extend(partnerships(&town, true));
    checks.extend(partnerships(&town, false));
    checks.extend([
        births(&town.births()),
        early_births(&town.early_births()),
        age_gap(&town.age_gap()),
        life("f11_e0_f", 83.43, &town.life_table(true)),
        life("f11_e0_m", 78.80, &town.life_table(false)),
        longevity(&town, true),
        longevity(&town, false),
    ]);
    Ok(json!({
        "format": "simagent.criteria.report",
        "version": 1,
        "export": format!("{:016x}", stable_hash64(text.as_bytes())),
        "end": end,
        "persons": export.rows.len(),
        "couples": export.couples.len(),
        "families": firsts.len(),
        "family_units": hash(&firsts),
        "checks": checks,
        "village_block": village((end + 1) / YEAR, town.village().as_ref()),
    }))
}

/// Where the report goes: `out` if given, else `report.json` beside the export.
fn destination(export: &Path, out: Option<&str>) -> PathBuf {
    out.map_or_else(|| export.with_file_name("report.json"), PathBuf::from)
}

/// `cargo xtask criteria report <export.ndjson> [<out.json>]`.
pub fn command(args: &[String]) -> Result<(), String> {
    let [which, export, rest @ ..] = args else {
        return Err(USAGE.into());
    };
    if which != "report" || rest.len() > 1 {
        return Err(USAGE.into());
    }
    let export = Path::new(export);
    let text = std::fs::read_to_string(export).map_err(|e| format!("{}: {e}", export.display()))?;
    let out = destination(export, rest.first().map(String::as_str));
    let mut json = serde_json::to_string_pretty(&report(&text)?).map_err(|e| e.to_string())?;
    json.push('\n');
    std::fs::write(&out, json).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("criteria report: {}", out.display());
    Ok(())
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
