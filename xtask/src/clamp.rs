//! The clamp rate of the reference population (ADR-0005 section 8, ADR-0020 section 7):
//! the last ten yearly censuses of the pool run, earlier years one at a time while they hold
//! fewer than 100 000 evaluations; per clamped parameter at most 0.1 percent of them. The
//! pool command writes the selected rows as `reference_census.json` next to `pool.bin`, also
//! when the bound fails (the finding then goes to Grok); the gate reads the record once the
//! pool is in the repository (design note of the census, section 6).

use sim_core::body_contract::CLAMPED_KEYS;
use sim_core::world::CensusRow;

use crate::ordered::Json;

type Result<T> = std::result::Result<T, String>;

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

/// The sample of a run of `years` years from its census: the rows must be the run's year
/// ends in order without a gap, the last one the year `years − 1` (the run's last second
/// never reaches its year end). Takes the last ten rows, then earlier ones one at a time
/// while they hold fewer than `SAMPLE` evaluations; a run too short for the sample gives all
/// its rows (and `findings` names the shortfall).
pub fn select(census: &[CensusRow], years: i64) -> Result<Vec<Row>> {
    let last = census.last().ok_or("the run has no census")?;
    if last.year != years - 1 {
        return Err(format!(
            "the census ends in year {}, the run's last year end is {}",
            last.year,
            years - 1
        ));
    }
    if let Some(pair) = census
        .windows(2)
        .find(|pair| pair[1].year != pair[0].year + 1)
    {
        return Err(format!(
            "the census jumps from year {} to {}",
            pair[0].year, pair[1].year
        ));
    }
    let mut from = census.len().saturating_sub(LAST_YEARS);
    let mut sum = sum_of(census[from..].iter().map(|row| row.body.evaluated));
    while sum < SAMPLE && from > 0 {
        from -= 1;
        sum = sum_of([sum, census[from].body.evaluated]);
    }
    Ok(census[from..].iter().map(Row::of).collect())
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

/// What breaks the criterion: a sample below `SAMPLE`, and every clamped parameter above 0.1
/// percent of the evaluations; empty when it holds.
pub fn findings(rows: &[Row]) -> Vec<String> {
    let n = evaluations(rows);
    let mut found = Vec::new();
    if n < SAMPLE {
        found.push(format!(
            "the sample holds {n} evaluations, fewer than {SAMPLE}"
        ));
    }
    for (index, key) in CLAMPED_KEYS.iter().enumerate() {
        let count = sum_of(rows.iter().map(|row| row.clamped[index]));
        if PER_MILLE * u128::from(count) > u128::from(n) {
            found.push(format!(
                "{}: {count} of {n} clamped, above 0.1 percent",
                key.as_str()
            ));
        }
    }
    found
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
