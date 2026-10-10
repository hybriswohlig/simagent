//! The yearly census (ADR-0004 section 8): values from the state, never from the log
//! and never from earlier rows.

use super::DomainState;
use crate::time::{SimTime, calendar, year_start};

/// One row of `census.csv`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CensusRow {
    pub year: i64,
    /// Sims in the active population at the end of the year.
    pub living: u64,
    /// Seconds lived in this year by all sims, archive included.
    pub lived_secs: u64,
}

/// The row of the year that contains `t`; the observer runs in its last second.
pub(crate) fn census_row(domain: &DomainState, t: SimTime) -> CensusRow {
    let year = calendar(t).year;
    let (from, to) = (year_start(year), year_start(year + 1));
    // Seconds of `[born, left)` inside the year; the second of leaving is not lived.
    let overlap = |born: SimTime, left: SimTime| -> u64 {
        let (start, end) = (born.max(from), left.min(to));
        if end <= start {
            return 0;
        }
        u64::try_from((end - start).as_secs())
            .unwrap_or_else(|_| panic!("invariant census.negative_span"))
    };
    let living = domain.sims.values().map(|sim| overlap(sim.born, to));
    let archived = domain
        .archive
        .values()
        .map(|sim| overlap(sim.born, sim.left));
    let mut lived_secs: u64 = 0;
    for span in living.chain(archived) {
        lived_secs = lived_secs
            .checked_add(span)
            .unwrap_or_else(|| panic!("invariant census.lived_secs_overflow"));
    }
    CensusRow {
        year,
        living: u64::try_from(domain.sims.len())
            .unwrap_or_else(|_| panic!("invariant census.count_range")),
        lived_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::genome::{Genome, Sex};
    use crate::ids::SimId;
    use crate::lineage::LineageKey;
    use crate::state::{ArchivedSim, LeftBy, Sim};
    use crate::time::{DAY, SimTime, year_start};

    fn sim(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn key(n: u64) -> LineageKey {
        LineageKey::founder(&DataId::new("test:census").expect("valid"), n)
    }

    /// The census reads only the times.
    fn living(n: u64, born: SimTime) -> Sim {
        Sim {
            born,
            lineage_key: key(n),
            sex: Sex::Female,
            genome: Genome::reference(Sex::Female),
            afflictions: alloc::vec::Vec::new(),
            conceptions: 0,
            parents: None,
        }
    }

    fn archived(n: u64, born: SimTime, left: SimTime) -> ArchivedSim {
        ArchivedSim {
            born,
            left,
            left_by: LeftBy::Death { cause: 7 },
            lineage_key: key(n),
            sex: Sex::Male,
            genome: Genome::reference(Sex::Male),
            afflictions: alloc::vec::Vec::new(),
            conceptions: 0,
            parents: None,
        }
    }

    /// Hand-built state, by hand for year 3 (a year has 364 days):
    /// - sim 1 is born on day 10 of year 3 and lives: a year less 10 days.
    /// - sim 2 was born 10 years before the start and left on day 100 of year 3: it is
    ///   archived and lived 100 days (the second of leaving is not lived).
    /// - sim 3 was born one second before the start and lives: the whole year.
    /// - sim 4 left in the last second of year 2: nothing in year 3.
    #[test]
    fn the_census_counts_partial_years_and_the_archive() {
        let day = DAY.as_secs();
        let year_secs = u64::try_from(364 * day).expect("positive");
        let day_secs = u64::try_from(day).expect("positive");
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let mut domain = DomainState::new();
        domain.sims.insert(sim(1), living(1, at(3, 10 * day)));
        domain
            .archive
            .insert(sim(2), archived(2, at(-10, 0), at(3, 100 * day)));
        domain.sims.insert(sim(3), living(3, at(0, -1)));
        domain
            .archive
            .insert(sim(4), archived(4, at(-10, 0), at(3, -1)));
        let last_second = |year: i64| at(year + 1, -1);
        assert_eq!(
            census_row(&domain, last_second(3)),
            CensusRow {
                year: 3,
                living: 2,
                lived_secs: (year_secs - 10 * day_secs) + 100 * day_secs + year_secs,
            }
        );
        assert_eq!(
            census_row(&domain, last_second(4)),
            CensusRow {
                year: 4,
                living: 2,
                lived_secs: 2 * year_secs,
            }
        );
        // Year 2: sims 2, 3 and 4 (until its last second); sim 1 is not born yet.
        assert_eq!(
            census_row(&domain, last_second(2)),
            CensusRow {
                year: 2,
                living: 2,
                lived_secs: year_secs + year_secs + (year_secs - 1),
            }
        );
    }

    /// A negative year: the row carries the year of `t`, not year 0.
    #[test]
    fn the_census_year_follows_the_calendar() {
        let domain = DomainState::new();
        let row = census_row(&domain, SimTime::from_secs(-1));
        assert_eq!(
            row,
            CensusRow {
                year: -1,
                living: 0,
                lived_secs: 0,
            }
        );
    }
}
