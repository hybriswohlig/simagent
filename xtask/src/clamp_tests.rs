//! The selection, the check and the record of the reference population's clamp rate. The
//! selection and the check against the cases of the independent reference
//! (`tools/census_reference.py`, `selection_cases` in `tests/golden/census/census.json`:
//! ADR-0005 section 8 and ADR-0020 section 7, ten years, earlier years one at a time below
//! 100 000 evaluations, at most 0.1 percent per clamped parameter); the record on rows built
//! by hand.

use sim_core::body_contract::{BodyCensus, CLAMPED_KEYS};
use sim_core::world::{CensusRow, PopulationCensus};

use super::*;
use crate::ordered::Json;

type Result<T> = std::result::Result<T, String>;

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
        population: PopulationCensus::default(),
    }
}

/// The census of a run of `years` years, `evaluated` bodies in every year.
fn run(years: i64, evaluated: u64) -> Vec<CensusRow> {
    (0..years).map(|year| row(year, evaluated)).collect()
}

fn years_of(rows: &[Row]) -> Vec<i64> {
    rows.iter().map(|row| row.year).collect()
}

const CENSUS: &str = include_str!("../../tests/golden/census/census.json");

fn cases() -> Vec<serde_json::Value> {
    let census: serde_json::Value = serde_json::from_str(CENSUS).expect("json");
    census["selection_cases"].as_array().expect("cases").clone()
}

/// A row of a case: `[year, evaluated, {key: clamped}]`.
fn case_row(entry: &serde_json::Value) -> Row {
    let mut clamped = [0; 21];
    for (key, count) in entry[2].as_object().expect("counts") {
        let place = CLAMPED_KEYS
            .iter()
            .position(|known| known.as_str() == key)
            .expect("a clamped key");
        clamped[place] = count.as_u64().expect("a count");
    }
    let evaluated = entry[1].as_u64().expect("evaluated");
    Row {
        year: entry[0].as_i64().expect("year"),
        living: evaluated,
        evaluated,
        clamped,
        curve_clamped: 0,
    }
}

/// Every case of the reference's bound and record check: the same sum, the same verdict and
/// the same first reason (nine cases, among them exactly 0.1 percent, one more, 99 999 in ten
/// years, an eleventh year, a gap, the last year missing, a year not needed, nine years).
#[test]
fn the_check_decides_each_case_as_the_reference() {
    let mut checked = 0;
    for case in cases() {
        let Some(entries) = case["rows"].as_array() else {
            continue;
        };
        let rows: Vec<Row> = entries.iter().map(case_row).collect();
        let last = case["last_year"].as_i64().expect("last year");
        let verdict = check(&rows, last);
        let name = case["name"].as_str().expect("name");
        assert_eq!(verdict.n, case["n"].as_u64().expect("n"), "{name}");
        assert_eq!(verdict.reason.as_deref(), case["reason"].as_str(), "{name}");
        assert_eq!(verdict.reason.is_none(), case["green"] == true, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 9);
}

/// The reference's two selections from runs of 750 years: 40 000 a year takes the last ten,
/// 9 000 a year twelve (108 000), stopping as soon as the sum reaches 100 000.
#[test]
fn the_selection_is_the_references() {
    let years = |name: &str| -> Vec<i64> {
        let case = cases()
            .into_iter()
            .find(|case| case["name"] == name)
            .expect("the case");
        case["years"]
            .as_array()
            .expect("years")
            .iter()
            .map(|year| year.as_i64().expect("a year"))
            .collect()
    };
    let rows = select(&run(750, 40_000), 749);
    assert_eq!(years_of(&rows), years("select_rows_40000"));
    assert_eq!(check(&rows, 749).reason, None);
    let rows = select(&run(750, 9_000), 749);
    assert_eq!(years_of(&rows), years("select_rows_9000"));
    assert_eq!(check(&rows, 749).reason, None);
}

/// The selection walks back from the last year and stops at a missing year or the run's
/// start; the check then names what is wrong (review: a leading gap, a short run).
#[test]
fn the_selection_stops_at_a_gap_and_the_check_names_it() {
    let mut gap = run(750, 40_000);
    gap.remove(745);
    let rows = select(&gap, 749);
    assert_eq!(years_of(&rows), vec![749, 748, 747, 746]);
    assert_eq!(
        check(&rows, 749).reason.as_deref(),
        Some("fewer_than_ten_years")
    );
    let rows = select(&run(749, 40_000), 749);
    assert!(rows.is_empty());
    assert_eq!(
        check(&rows, 749).reason.as_deref(),
        Some("not_the_last_year")
    );
    // Five years of 30 000 hold 150 000 but are not ten censuses.
    let rows = select(&run(5, 30_000), 4);
    assert_eq!(years_of(&rows), vec![4, 3, 2, 1, 0]);
    assert_eq!(
        check(&rows, 4).reason.as_deref(),
        Some("fewer_than_ten_years")
    );
    // Thirty years of 1 000: all of them, and still too few evaluations.
    let rows = select(&run(30, 1_000), 29);
    assert_eq!(rows.len(), 30);
    assert_eq!(
        check(&rows, 29).reason.as_deref(),
        Some("fewer_than_100000")
    );
    assert_eq!(check(&rows, 29).n, 30_000);
}

#[test]
fn the_record_reads_back_what_it_wrote() {
    let mut rows = select(&run(750, 40_000), 749);
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
    let rows = select(&run(750, 40_000), 749);
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

fn repository_file(path: &str) -> Vec<u8> {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .join(path);
    std::fs::read(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()))
}

/// The record of the repository's pool, `data/pool/reference_census.json`, read and bound to
/// `data/pool/pool.bin`: its `pool_hash` is the pool's hash and its `data_hash` the pool's
/// run data hash. Its `core_version` is that of the pool run (38), not the current one.
fn repository_record() -> Record {
    let text =
        String::from_utf8(repository_file("data/pool/reference_census.json")).expect("UTF-8");
    let record = read_record(&text).expect("a record");
    let pool =
        sim_core::pool::Pool::from_bytes(&repository_file("data/pool/pool.bin")).expect("the pool");
    assert_eq!(record.pool_hash, pool.hash());
    assert_eq!(record.data_hash, pool.data_hash);
    record
}

/// The clamp rate of the reference population (ADR-0020 section 7) as a gate: the record of
/// the repository's pool is green, with the evaluations and the clamp counts of the
/// independent reference (`census_reference.check_record`, last year 749): years 749 to 740,
/// `n` 399 982, `body_fat_z` 17, `chest_depth_z` 10, `face_axis_02` 10, `face_axis_08` 10,
/// every other key 0, no held curve.
#[test]
fn the_repository_pool_meets_the_clamp_rate() {
    let record = repository_record();
    assert_eq!(record.pool_hash, 0xac11_f329_cd5d_a77a);
    assert_eq!(record.data_hash, 0xe96b_e897_ae69_84a0);
    assert_eq!(
        years_of(&record.rows),
        (740..=749).rev().collect::<Vec<i64>>()
    );
    let verdict = check(&record.rows, 749);
    assert_eq!(verdict.reason, None);
    assert_eq!(verdict.n, 399_982);
    let expected = |key: &str| match key {
        "body_fat_z" => 17,
        "chest_depth_z" | "face_axis_02" | "face_axis_08" => 10,
        _ => 0,
    };
    for (place, key) in CLAMPED_KEYS.iter().enumerate() {
        let count: u64 = record.rows.iter().map(|row| row.clamped[place]).sum();
        assert_eq!(count, expected(key.as_str()), "{}", key.as_str());
    }
    let held: u64 = record.rows.iter().map(|row| row.curve_clamped).sum();
    assert_eq!(held, 0);
}

/// The negative twin of the gate: the same rows with the count of `body_fat_z` raised to 400,
/// just above `n / 1000` = 399.982, are red; at 399 they are still green.
#[test]
fn the_clamp_gate_turns_red_just_above_the_bound() {
    let record = repository_record();
    let place = CLAMPED_KEYS
        .iter()
        .position(|key| key.as_str() == "body_fat_z")
        .expect("a clamped key");
    let raised_to = |total: u64| {
        let mut rows = record.rows.clone();
        let others: u64 = rows[1..].iter().map(|row| row.clamped[place]).sum();
        rows[0].clamped[place] = total - others;
        check(&rows, 749)
    };
    assert_eq!(raised_to(399).reason, None);
    let red = raised_to(400);
    assert_eq!(red.n, 399_982);
    assert_eq!(red.reason.as_deref(), Some("rate body_fat_z"));
}
