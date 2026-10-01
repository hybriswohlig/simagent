//! The yearly census (ADR-0004 section 8): values from the state, never from the log
//! and never from earlier rows.

use alloc::vec::Vec;

use super::{ArchivedSim, DomainState};
use crate::time::SimTime;

/// One row of `census.csv`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CensusRow {
    pub year: i64,
    /// Sims in the active population at the end of the year.
    pub living: u64,
    /// Seconds lived in this year by all sims, archive included.
    pub lived_secs: u64,
}

/// What the observer `YearEnd` writes: output, not state (ADR-0002).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YearOut {
    pub census: Vec<CensusRow>,
}

/// The row of the year that contains `t`; the observer runs in its last second.
pub(crate) fn census_row(domain: &DomainState, t: SimTime) -> CensusRow {
    let _ = (domain, t, |a: &ArchivedSim| (a.born, a.left));
    CensusRow {
        year: 0,
        living: 0,
        lived_secs: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SimId;
    use crate::state::Sim;
    use crate::time::{DAY, YEAR, year_start};

    fn sim(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    /// Hand-built state, by hand for year 3 (YEAR = 31 449 600 s, DAY = 86 400 s):
    /// - sim 1 is born on day 10 of year 3 and lives: YEAR - 10 days = 30 585 600.
    /// - sim 2 was born 10 years before the start and left on day 100 of year 3: it is
    ///   archived and lived 100 days = 8 640 000 (the second of leaving is not lived).
    /// - sim 3 was born one second before the start and lives: the whole year.
    /// - sim 4 left in the last second of year 2: nothing in year 3.
    #[test]
    fn the_census_counts_partial_years_and_the_archive() {
        let day = DAY.as_secs();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let mut domain = DomainState::new();
        domain.sims.insert(
            sim(1),
            Sim {
                born: at(3, 10 * day),
            },
        );
        domain.archive.insert(
            sim(2),
            ArchivedSim {
                born: at(-10, 0),
                left: at(3, 100 * day),
            },
        );
        domain.sims.insert(sim(3), Sim { born: at(0, -1) });
        domain.archive.insert(
            sim(4),
            ArchivedSim {
                born: at(-10, 0),
                left: at(3, -1),
            },
        );
        let last_second = |year: i64| at(year + 1, -1);
        assert_eq!(
            census_row(&domain, last_second(3)),
            CensusRow {
                year: 3,
                living: 2,
                lived_secs: 30_585_600 + 8_640_000 + 31_449_600,
            }
        );
        assert_eq!(
            census_row(&domain, last_second(4)),
            CensusRow {
                year: 4,
                living: 2,
                lived_secs: 2 * 31_449_600,
            }
        );
        // Year 2: sims 2, 3 and 4 (until its last second); sim 1 is not born yet.
        assert_eq!(
            census_row(&domain, last_second(2)),
            CensusRow {
                year: 2,
                living: 2,
                lived_secs: 31_449_600 + 31_449_600 + (31_449_600 - 1),
            }
        );
        assert_eq!(YEAR.as_secs(), 31_449_600);
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
