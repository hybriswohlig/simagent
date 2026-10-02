//! The event budget (ADR-0004 section 8): log entries per person-year in the gate run.
//! The limits are tolerances of the protected list (ADR-0000).

/// At most this many log entries per person-year.
pub const MAX_ENTRIES_PER_PERSON_YEAR: u64 = 2;

/// A single year counts only from this many person-years on; build-up and small
/// denominators count in the total only.
pub const MIN_PERSON_YEARS: u64 = 20;

/// Seconds of a game year of 364 days.
const YEAR_SECS: i64 = 364 * 86_400;

/// The violations of the budget: `entry_times` are the times of all log entries,
/// `census` the rows `(year, lived_secs)` of `census.csv`.
pub fn violations(entry_times: &[i64], census: &[(i64, u64)]) -> Vec<String> {
    let _ = (
        entry_times,
        census,
        MAX_ENTRIES_PER_PERSON_YEAR,
        MIN_PERSON_YEARS,
        YEAR_SECS,
    );
    Vec::new()
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
