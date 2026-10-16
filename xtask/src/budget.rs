//! The event budget (ADR-0004 section 8): log entries per person-year in the gate run.
//! The limits are tolerances of the protected list (ADR-0000).

use serde_json::Value;

/// At most this many log entries per person-year.
pub const MAX_ENTRIES_PER_PERSON_YEAR: u64 = 2;

/// A single year counts only from this many person-years on; build-up and small
/// denominators count in the total only.
pub const MIN_PERSON_YEARS: u64 = 20;

/// Seconds of a game year of 364 days.
const YEAR_SECS: i64 = 364 * 86_400;

/// The times of the log entries in the text of an `events.ndjson`.
pub fn entry_times(events: &str) -> Result<Vec<i64>, String> {
    let mut times = Vec::new();
    for (line, number) in events.lines().zip(1..) {
        let time = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|entry| entry.get("t").and_then(Value::as_i64))
            .ok_or_else(|| format!("events.ndjson line {number} has no time"))?;
        times.push(time);
    }
    Ok(times)
}

/// The rows `(year, lived_secs)` in the text of a `census.csv`: its first three columns
/// are those of every run (ADR-0004 section 8), the body columns follow (ADR-0020 7).
pub fn census_rows(census: &str) -> Result<Vec<(i64, u64)>, String> {
    let mut lines = census.lines().zip(1..);
    match lines.next() {
        Some((header, _))
            if header
                .strip_prefix("year,living,lived_secs")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(',')) => {}
        Some(_) => return Err("census.csv does not start with year,living,lived_secs".to_string()),
        None => return Err("census.csv has no row".to_string()),
    }
    let mut rows = Vec::new();
    for (line, number) in lines {
        let columns: Vec<&str> = line.split(',').collect();
        let row = match columns.as_slice() {
            [year, _living, lived_secs, ..] => year.parse().ok().zip(lived_secs.parse().ok()),
            _ => None,
        };
        rows.push(
            row.ok_or_else(|| format!("census.csv line {number} is not year,living,lived_secs"))?,
        );
    }
    if rows.is_empty() {
        return Err("census.csv has no row".to_string());
    }
    Ok(rows)
}

/// The violations of the budget: `entry_times` are the times of all log entries,
/// `census` the rows `(year, lived_secs)` of `census.csv`.
pub fn violations(entry_times: &[i64], census: &[(i64, u64)]) -> Vec<String> {
    let year_secs = YEAR_SECS.unsigned_abs();
    // `entries` in `lived_secs / YEAR_SECS` person-years are over the limit if
    // entries * YEAR_SECS > limit * lived_secs: whole numbers, no division.
    let over = |entries: u64, lived_secs: u64| {
        u128::from(entries) * u128::from(year_secs)
            > u128::from(MAX_ENTRIES_PER_PERSON_YEAR) * u128::from(lived_secs)
    };
    let count = |year: Option<i64>| {
        let matching = entry_times
            .iter()
            .filter(|time| year.is_none_or(|year| time.div_euclid(YEAR_SECS) == year))
            .count();
        u64::try_from(matching).unwrap_or(u64::MAX)
    };
    let mut found = Vec::new();
    for &(year, lived_secs) in census {
        if year < 1 || lived_secs < MIN_PERSON_YEARS * year_secs {
            continue;
        }
        let entries = count(Some(year));
        if over(entries, lived_secs) {
            found.push(format!(
                "year {year}: {entries} log entries in {} person-years: more than {MAX_ENTRIES_PER_PERSON_YEAR} per person-year",
                lived_secs / year_secs
            ));
        }
    }
    let total_lived: u64 = census.iter().map(|(_, lived_secs)| lived_secs).sum();
    let total_entries = count(None);
    if over(total_entries, total_lived) {
        found.push(format!(
            "{total_entries} log entries in {} person-years: more than {MAX_ENTRIES_PER_PERSON_YEAR} per person-year in the whole run",
            total_lived / year_secs
        ));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const Y: i64 = YEAR_SECS;

    fn year_secs() -> u64 {
        u64::try_from(Y).expect("positive")
    }

    /// `people` living through the whole year.
    fn lived(people: u64) -> u64 {
        people * year_secs()
    }

    /// `count` entries spread over year `year`.
    fn entries(year: i64, count: i64) -> Vec<i64> {
        (0..count).map(|i| year * Y + i).collect()
    }

    /// The dummy world of step 2: three sims with one birthday each per year, and four
    /// entries of the start in year 0.
    #[test]
    fn the_dummy_world_is_within_the_budget() {
        let mut times = entries(0, 4);
        let mut census = Vec::new();
        for year in 0..10 {
            times.extend(entries(year, 3));
            census.push((year, lived(3)));
        }
        assert_eq!(violations(&times, &census), Vec::<String>::new());
    }

    #[test]
    fn the_whole_run_has_a_budget() {
        // 61 entries in 30 person-years: more than 2 per person-year.
        let census: Vec<(i64, u64)> = (0..10).map(|year| (year, lived(3))).collect();
        let times = entries(4, 61);
        assert_eq!(
            violations(&times, &census),
            vec!["61 log entries in 30 person-years: more than 2 per person-year in the whole run"]
        );
        // Exactly 2 per person-year is within the budget.
        assert_eq!(violations(&entries(4, 60), &census), Vec::<String>::new());
    }

    #[test]
    fn a_year_with_enough_person_years_has_a_budget() {
        // Year 3 has 20 person-years and 41 entries; the run as a whole stays below 2.
        let census: Vec<(i64, u64)> = (0..10).map(|year| (year, lived(20))).collect();
        let times = entries(3, 41);
        assert_eq!(
            violations(&times, &census),
            vec!["year 3: 41 log entries in 20 person-years: more than 2 per person-year"]
        );
        assert_eq!(violations(&entries(3, 40), &census), Vec::<String>::new());
    }

    /// Build-up and small denominators count only in the total (ADR-0004 section 8).
    #[test]
    fn year_zero_and_small_years_count_only_in_the_total() {
        // Year 0 with 20 person-years and 41 entries: no rule for a single year.
        let census: Vec<(i64, u64)> = (0..10).map(|year| (year, lived(20))).collect();
        assert_eq!(violations(&entries(0, 41), &census), Vec::<String>::new());
        // A year one second short of 20 person-years: no rule for a single year.
        let mut census: Vec<(i64, u64)> = (0..10).map(|year| (year, lived(20))).collect();
        census[3].1 -= 1;
        assert_eq!(violations(&entries(3, 41), &census), Vec::<String>::new());
    }

    #[test]
    fn a_run_without_person_years_has_no_entries_to_spare() {
        let census = vec![(0, 0), (1, 0)];
        assert_eq!(violations(&[], &census), Vec::<String>::new());
        assert_eq!(
            violations(&[5], &census),
            vec!["1 log entries in 0 person-years: more than 2 per person-year in the whole run"]
        );
    }

    #[test]
    fn the_run_files_are_read() {
        let events = concat!(
            r#"{"id":1,"t":0,"subjects":[],"causes":[{"input":1}],"payload":{"core:started":{"founders":1}}}"#,
            "\n",
            r#"{"id":3,"t":31449599,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
            "\n",
        );
        assert_eq!(entry_times(events), Ok(vec![0, 31_449_599]));
        assert_eq!(entry_times(""), Ok(Vec::new()));
        assert_eq!(
            entry_times("{\"id\":1}\n"),
            Err("events.ndjson line 1 has no time".to_string())
        );
        assert_eq!(
            entry_times("{\"t\":0}\nnot json\n"),
            Err("events.ndjson line 2 has no time".to_string())
        );
        let census = "year,living,lived_secs\n0,1,31449600\n1,3,94348800\n";
        assert_eq!(
            census_rows(census),
            Ok(vec![(0, 31_449_600), (1, 94_348_800)])
        );
        assert_eq!(
            census_rows("year,living,lived_secs\n"),
            Err("census.csv has no row".to_string())
        );
        assert_eq!(census_rows(""), Err("census.csv has no row".to_string()));
        // The body columns follow the three (ADR-0020 section 7).
        assert_eq!(
            census_rows("year,living,lived_secs,body_evaluated\n0,1,31449600,1\n"),
            Ok(vec![(0, 31_449_600)])
        );
        assert_eq!(
            census_rows("year,living,lived_secsx\n0,1,5\n"),
            Err("census.csv does not start with year,living,lived_secs".to_string())
        );
        assert_eq!(
            census_rows("living,year,lived_secs\n0,1,5\n"),
            Err("census.csv does not start with year,living,lived_secs".to_string())
        );
        assert_eq!(
            census_rows("year,living,lived_secs\n0,1\n"),
            Err("census.csv line 2 is not year,living,lived_secs".to_string())
        );
        assert_eq!(
            census_rows("year,living,lived_secs\n0,1,x\n"),
            Err("census.csv line 2 is not year,living,lived_secs".to_string())
        );
    }

    /// Person-years are whole lived seconds, not people: half a year counts half.
    #[test]
    fn partial_years_count_by_seconds() {
        let census = vec![(0, 0), (1, 20 * year_secs() + year_secs() / 2)];
        // 20.5 person-years allow 41 entries in year 1.
        assert_eq!(violations(&entries(1, 41), &census), Vec::<String>::new());
        assert_eq!(
            violations(&entries(1, 42), &census).len(),
            2,
            "the year and the total"
        );
    }
}
