//! The selection, the bound and the record of the reference population's clamp rate on rows
//! built by hand; every expected value follows from ADR-0005 section 8 and ADR-0020 section 7
//! (ten years, earlier years one at a time below 100 000 evaluations, at most 0.1 percent per
//! clamped parameter).

use sim_core::body_contract::{BodyCensus, CLAMPED_KEYS};
use sim_core::world::CensusRow;

use super::*;
use crate::ordered::Json;

/// A record as `record` writes it (the gate reads it once the pool is in the repository).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Record {
    pub pool_hash: u64,
    pub data_hash: u64,
    pub core_version: u32,
    pub rows: Vec<Row>,
}

/// Reads a record: exactly the fields `record` writes, in its order, hashes as 16 lower-case
/// hex digits, counts as non-negative integers.
pub(crate) fn read_record(text: &str) -> Result<Record> {
    let json = Json::parse(text)?;
    let [pool_hash, data_hash, core_version, rows] = fields(
        &json,
        ["pool_hash", "data_hash", "core_version", "rows"],
        "the record",
    )?;
    let rows = rows
        .as_array()
        .ok_or("rows: not an array")?
        .iter()
        .map(read_row)
        .collect::<Result<Vec<_>>>()?;
    Ok(Record {
        pool_hash: hex(pool_hash, "pool_hash")?,
        data_hash: hex(data_hash, "data_hash")?,
        core_version: u32::try_from(count(core_version, "core_version")?)
            .map_err(|_| "core_version: out of range".to_owned())?,
        rows,
    })
}

fn read_row(json: &Json) -> Result<Row> {
    let [year, living, evaluated, clamped, curve_clamped] = fields(
        json,
        ["year", "living", "evaluated", "clamped", "curve_clamped"],
        "a row",
    )?;
    let keys = CLAMPED_KEYS.map(|key| key.as_str());
    let counts = fields(clamped, keys, "clamped")?;
    let mut by_key = [0; 21];
    for (slot, (value, key)) in by_key.iter_mut().zip(counts.into_iter().zip(keys)) {
        *slot = count(value, key)?;
    }
    let year = match year {
        Json::Number(number) => number.as_i64(),
        _ => None,
    }
    .ok_or("year: not an integer")?;
    Ok(Row {
        year,
        living: count(living, "living")?,
        evaluated: count(evaluated, "evaluated")?,
        clamped: by_key,
        curve_clamped: count(curve_clamped, "curve_clamped")?,
    })
}

/// The values of an object that holds exactly `names`, in this order.
fn fields<'a, const N: usize>(
    json: &'a Json,
    names: [&str; N],
    what: &str,
) -> Result<[&'a Json; N]> {
    let Json::Object(entries) = json else {
        return Err(format!("{what}: not an object"));
    };
    let found: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
    if found != names {
        return Err(format!("{what}: the fields {found:?}, not {names:?}"));
    }
    Ok(std::array::from_fn(|index| &entries[index].1))
}

fn count(json: &Json, name: &str) -> Result<u64> {
    json.as_u64()
        .ok_or_else(|| format!("{name}: not a non-negative integer"))
}

fn hex(json: &Json, name: &str) -> Result<u64> {
    let text = json.as_str().unwrap_or_default();
    let digits = text.len() == 16
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !digits {
        return Err(format!("{name}: not 16 lower-case hex digits"));
    }
    u64::from_str_radix(text, 16).map_err(|error| format!("{name}: {error}"))
}

/// A census row of `year` with `evaluated` bodies and no clamp.
fn row(year: i64, evaluated: u64) -> CensusRow {
    CensusRow {
        year,
        living: evaluated,
        lived_secs: 0,
        body: BodyCensus {
            evaluated,
            ..BodyCensus::default()
        },
    }
}

/// The census of a run of `years` years, `evaluated` bodies in every year.
fn run(years: i64, evaluated: u64) -> Vec<CensusRow> {
    (0..years).map(|year| row(year, evaluated)).collect()
}

fn years_of(rows: &[Row]) -> Vec<i64> {
    rows.iter().map(|row| row.year).collect()
}

#[test]
fn the_sample_is_the_last_ten_years() {
    let selected = select(&run(750, 40_000), 750).expect("a selection");
    assert_eq!(years_of(&selected), (740..750).collect::<Vec<_>>());
    assert_eq!(evaluations(&selected), 400_000);
}

#[test]
fn earlier_years_join_one_at_a_time_while_below_the_sample() {
    // Ten years give 90 000; year 19 gives 99 000, still below; year 18 reaches 108 000.
    let selected = select(&run(30, 9_000), 30).expect("a selection");
    assert_eq!(years_of(&selected), (18..30).collect::<Vec<_>>());
    // Exactly 100 000 in ten years is enough.
    let selected = select(&run(30, 10_000), 30).expect("a selection");
    assert_eq!(years_of(&selected), (20..30).collect::<Vec<_>>());
    // One below in ten years takes one more.
    let mut rows = run(30, 10_000);
    rows[29].body.evaluated = 9_999;
    let selected = select(&rows, 30).expect("a selection");
    assert_eq!(years_of(&selected), (19..30).collect::<Vec<_>>());
}

#[test]
fn a_short_run_gives_all_its_years_and_a_finding() {
    let selected = select(&run(12, 1_000), 12).expect("a selection");
    assert_eq!(years_of(&selected), (0..12).collect::<Vec<_>>());
    assert_eq!(
        findings(&selected),
        vec!["the sample holds 12000 evaluations, fewer than 100000"]
    );
}

#[test]
fn the_rows_must_be_the_runs_years_without_a_gap() {
    assert!(select(&[], 750).is_err());
    let mut gap = run(750, 40_000);
    gap.remove(745);
    assert!(select(&gap, 750).is_err());
    // The last row must be the run's last year end, `years − 1`.
    assert!(select(&run(749, 40_000), 750).is_err());
    assert!(select(&run(751, 40_000)[1..], 750).is_err());
    let mut unordered = run(750, 40_000);
    unordered.swap(3, 4);
    assert!(select(&unordered, 750).is_err());
}

#[test]
fn a_parameter_may_clamp_one_in_a_thousand() {
    // 400 000 evaluations: 400 clamped is exactly 0.1 percent and passes, 401 does not.
    let mut rows = select(&run(750, 40_000), 750).expect("a selection");
    rows[0].clamped[3] = 399;
    rows[9].clamped[3] = 1;
    rows[5].clamped[20] = 400;
    rows[2].curve_clamped = 400_000;
    assert_eq!(findings(&rows), Vec::<String>::new());
    rows[9].clamped[3] = 2;
    assert_eq!(
        findings(&rows),
        vec![format!(
            "{}: 401 of 400000 clamped, above 0.1 percent",
            CLAMPED_KEYS[3].as_str()
        )]
    );
}

#[test]
fn the_record_reads_back_what_it_wrote() {
    let mut rows = select(&run(750, 40_000), 750).expect("a selection");
    rows[1].clamped[0] = 7;
    rows[4].clamped[12] = 3;
    rows[8].curve_clamped = 11;
    rows[8].living = 40_002;
    let text = record(0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210, &rows);
    let read = read_record(&text).expect("a record");
    assert_eq!(read.pool_hash, 0x0123_4567_89ab_cdef);
    assert_eq!(read.data_hash, 0xfedc_ba98_7654_3210);
    assert_eq!(read.core_version, sim_core::version::CORE_VERSION);
    assert_eq!(read.rows, rows);
    assert!(text.starts_with("{\n  \"pool_hash\": \"0123456789abcdef\",\n"));
    assert!(text.ends_with("}\n"));
}

#[test]
fn the_record_rejects_what_it_never_writes() {
    let rows = select(&run(750, 40_000), 750).expect("a selection");
    let text = record(1, 2, &rows);
    let first = CLAMPED_KEYS[0].as_str();
    let second = CLAMPED_KEYS[1].as_str();
    for wrong in [
        text.replacen("\"pool_hash\"", "\"pool\"", 1),
        text.replacen("\"rows\": [", "\"extra\": 1,\n  \"rows\": [", 1),
        text.replacen(
            "\"curve_clamped\": 0",
            "\"curve_clamped\": 0,\n      \"more\": 0",
            1,
        ),
        // A clamped key missing, unknown or out of order.
        text.replacen(&format!("\"{first}\": 0,"), "", 1),
        text.replacen(&format!("\"{first}\""), "\"height\"", 1),
        text.replacen(&format!("\"{first}\""), "\"tmp\"", 1)
            .replacen(&format!("\"{second}\""), &format!("\"{first}\""), 1)
            .replacen("\"tmp\"", &format!("\"{second}\""), 1),
        text.replacen("\"0000000000000001\"", "\"1\"", 1),
        text.replacen("\"evaluated\": 40000", "\"evaluated\": -1", 1),
        text.replacen("\"evaluated\": 40000", "\"evaluated\": 4e4", 1),
    ] {
        assert_ne!(wrong, text);
        assert!(read_record(&wrong).is_err(), "{wrong}");
    }
}
