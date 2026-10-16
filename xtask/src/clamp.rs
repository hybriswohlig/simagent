//! The clamp rate of the reference population (ADR-0005 section 8, ADR-0020 section 7):
//! the last ten yearly censuses of the pool run, earlier years one at a time while they hold
//! fewer than 100 000 evaluations; per clamped parameter at most 0.1 percent of them. The
//! pool command writes the selected rows as `reference_census.json` next to `pool.bin`, also
//! when the check fails (the command is then red and the finding goes to Grok); the gate
//! checks the record again once the pool is in the repository (design note of the census,
//! section 6). Selection and check follow the independent reference
//! (`tools/census_reference.py`).

use std::collections::BTreeMap;

use sim_core::body_contract::CLAMPED_KEYS;
use sim_core::world::CensusRow;

use crate::ordered::Json;

/// The evaluations the sample needs at least (ADR-0005 section 8; protected list).
pub const SAMPLE: u64 = 100_000;
/// The years the sample takes first, the last of the run.
const LAST_YEARS: usize = 10;
/// The bound: `PER_MILLE · clamped ≤ evaluated`, so 0.1 percent (ADR-0005 section 8;
/// protected list).
const PER_MILLE: u128 = 1_000;

/// One census row of the sample: the year, the living and their bodies at its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub year: i64,
    pub living: u64,
    pub evaluated: u64,
    /// Per key of `CLAMPED_KEYS`, in its order.
    pub clamped: [u64; 21],
    /// Bodies whose growth curve held an input (ADR-0012); recorded, never bounded.
    pub curve_clamped: u64,
}

impl Row {
    fn of(row: &CensusRow) -> Row {
        Row {
            year: row.year,
            living: row.living,
            evaluated: row.body.evaluated,
            clamped: row.body.clamped,
            curve_clamped: row.body.curve_clamped,
        }
    }
}

/// The sample of a run whose last year end is `last_year`, from its census: from that year
/// back, the last ten years, then earlier ones one at a time while they hold fewer than
/// `SAMPLE` evaluations, as long as the years follow each other (the reference's
/// `select_rows`). Whatever is wrong with it, `check` names.
pub fn select(census: &[CensusRow], last_year: i64) -> Vec<Row> {
    let by_year: BTreeMap<i64, &CensusRow> = census.iter().map(|row| (row.year, row)).collect();
    let mut chosen = Vec::new();
    let mut sum = 0;
    let mut year = last_year;
    while let Some(row) = by_year.get(&year) {
        if chosen.len() >= LAST_YEARS && sum >= SAMPLE {
            break;
        }
        sum = sum_of([sum, row.body.evaluated]);
        chosen.push(Row::of(row));
        year -= 1;
    }
    chosen
}

fn sum_of(values: impl IntoIterator<Item = u64>) -> u64 {
    values.into_iter().fold(0, |sum, value| {
        sum.checked_add(value)
            .unwrap_or_else(|| panic!("invariant clamp.sum"))
    })
}

/// The evaluations of the sample.
pub fn evaluations(rows: &[Row]) -> u64 {
    sum_of(rows.iter().map(|row| row.evaluated))
}

/// The verdict on a sample: its evaluations and, unless it is green, the first rule it
/// breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub n: u64,
    pub reason: Option<String>,
}

/// The check of a sample of a run whose last year end is `last_year` (the reference's
/// `check_record`), in this order: its years are `last_year`, `last_year − 1`, … without a
/// gap (`not_the_last_year`, `year_gap`), at least ten (`fewer_than_ten_years`), an eleventh
/// or later year only while the later ones hold fewer than `SAMPLE` (`year_not_needed`);
/// at least `SAMPLE` evaluations (`fewer_than_100000`); per clamped parameter
/// `1000 · clamped ≤ n` (`rate <key>`).
pub fn check(rows: &[Row], last_year: i64) -> Verdict {
    let mut ordered: Vec<&Row> = rows.iter().collect();
    ordered.sort_by_key(|row| core::cmp::Reverse(row.year));
    let n = evaluations(rows);
    let red = |reason: String| Verdict {
        n,
        reason: Some(reason),
    };
    if ordered.first().is_none_or(|row| row.year != last_year) {
        return red("not_the_last_year".into());
    }
    if ordered
        .windows(2)
        .any(|pair| pair[1].year != pair[0].year - 1)
    {
        return red("year_gap".into());
    }
    if ordered.len() < LAST_YEARS {
        return red("fewer_than_ten_years".into());
    }
    let mut later = 0;
    for (place, row) in ordered.iter().enumerate() {
        if place >= LAST_YEARS && later >= SAMPLE {
            return red("year_not_needed".into());
        }
        later = sum_of([later, row.evaluated]);
    }
    if n < SAMPLE {
        return red(format!("fewer_than_{SAMPLE}"));
    }
    for (place, key) in CLAMPED_KEYS.iter().enumerate() {
        let count = sum_of(rows.iter().map(|row| row.clamped[place]));
        if PER_MILLE * u128::from(count) > u128::from(n) {
            return red(format!("rate {}", key.as_str()));
        }
    }
    Verdict { n, reason: None }
}

/// The sample's rates, one line per clamped parameter.
pub fn print(rows: &[Row]) {
    let n = evaluations(rows);
    let years = rows.first().zip(rows.last());
    if let Some((first, last)) = years {
        println!(
            "clamp years {} to {} evaluations {n}",
            first.year, last.year
        );
    }
    for (index, key) in CLAMPED_KEYS.iter().enumerate() {
        let count = sum_of(rows.iter().map(|row| row.clamped[index]));
        println!("clamp {} {count}", key.as_str());
    }
    let held = sum_of(rows.iter().map(|row| row.curve_clamped));
    println!("clamp curve_clamped {held}");
}

/// The record of the sample: the pool's hash, the pool run's data hash, the core version
/// and the rows, each with its clamp counts in `CLAMPED_KEYS` order.
pub fn record(pool_hash: u64, data_hash: u64, rows: &[Row]) -> String {
    let row = |row: &Row| {
        let clamped = CLAMPED_KEYS
            .iter()
            .zip(row.clamped)
            .map(|(key, count)| (key.as_str(), Json::int(count)))
            .collect();
        Json::object(vec![
            ("year", Json::Number(row.year.into())),
            ("living", Json::int(row.living)),
            ("evaluated", Json::int(row.evaluated)),
            ("clamped", Json::object(clamped)),
            ("curve_clamped", Json::int(row.curve_clamped)),
        ])
    };
    Json::object(vec![
        ("pool_hash", Json::str(format!("{pool_hash:016x}"))),
        ("data_hash", Json::str(format!("{data_hash:016x}"))),
        (
            "core_version",
            Json::int(u64::from(sim_core::version::CORE_VERSION)),
        ),
        ("rows", Json::Array(rows.iter().map(row).collect())),
    ])
    .pretty()
    .unwrap_or_else(|error| panic!("invariant clamp.record: {error}"))
}

#[cfg(test)]
#[path = "clamp_tests.rs"]
pub(crate) mod tests;
