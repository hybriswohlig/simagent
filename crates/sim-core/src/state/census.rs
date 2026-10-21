//! The yearly census (ADR-0004 section 8): values from the state, never from the log
//! and never from earlier rows; the one value from outside is the number of persons
//! `Start` made (ADR-0021, addendum 2026-10-21).

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, LeftBy, death};
use crate::body_contract::BodyCensus;
use crate::data::Data;
use crate::data_id::Code;
use crate::time::{SimTime, calendar, year_start};

/// One row of `census.csv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CensusRow {
    pub year: i64,
    /// Sims in the active population at the end of the year.
    pub living: u64,
    /// Seconds lived in this year by all sims, archive included.
    pub lived_secs: u64,
    /// The living's bodies at the end of the year (ADR-0020 section 7).
    pub body: BodyCensus,
    /// Who was born, arrived, left or died in the year (ADR-0021, addendum 2026-10-21).
    pub population: PopulationCensus,
}

/// The population columns of a row (ADR-0021, addendum 2026-10-21), over the living and
/// the archive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PopulationCensus {
    /// Born in the town: of two town parents or of a pregnancy from the pool.
    pub births: u64,
    /// Arrived from the pool, without the persons `Start` made.
    pub arrivals: u64,
    pub emigrations: u64,
    /// Per cause of death, in the order of `world::death_causes`.
    pub deaths: Vec<u64>,
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
        body: BodyCensus::default(),
        population: PopulationCensus::default(),
    }
}

/// The population of the year that contains `t`. `started` is the number of persons
/// `Start` made, the SimIds 1 to it: they arrive in second 0 like an arrival of that
/// second and are none (the one value from outside the state, ADR-0021).
pub(crate) fn population(
    domain: &DomainState,
    data: &Data,
    started: u64,
    t: SimTime,
) -> PopulationCensus {
    let year = calendar(t).year;
    let (from, to) = (year_start(year), year_start(year + 1));
    let within = |at: SimTime| from <= at && at < to;
    let causes: Vec<u64> = death::causes(data).iter().map(Code::key).collect();
    let mut row = PopulationCensus {
        deaths: vec![0; causes.len()],
        ..PopulationCensus::default()
    };
    let living = (domain.sims.iter()).map(|(id, sim)| (id, sim.born, &sim.parents, &sim.pool));
    let archived = (domain.archive.iter()).map(|(id, sim)| (id, sim.born, &sim.parents, &sim.pool));
    for (id, born, parents, pool) in living.chain(archived) {
        let in_town = parents.is_some() || pool.as_ref().is_some_and(|o| o.born_pool.is_none());
        if in_town && within(born) {
            row.births += 1;
        }
        let arrived = pool
            .as_ref()
            .is_some_and(|o| o.born_pool.is_some() && within(o.arrived));
        if arrived && id.get() > started {
            row.arrivals += 1;
        }
    }
    for sim in (domain.archive.values()).filter(|sim| within(sim.left)) {
        match sim.left_by {
            LeftBy::Emigration => row.emigrations += 1,
            LeftBy::Death { cause } => {
                let at = (causes.iter())
                    .position(|key| *key == cause)
                    .unwrap_or_else(|| panic!("invariant census.cause"));
                row.deaths[at] += 1;
            }
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::genome::{Genome, Sex};
    use crate::ids::SimId;
    use crate::lineage::LineageKey;
    use crate::state::{ArchivedSim, LeftBy, Parents, PoolOrigin, Sim};
    use crate::time::{DAY, SimTime, year_start};

    fn sim(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    /// Born in the town: two parents of the town.
    fn parents() -> Option<Parents> {
        Some(Parents {
            mother: sim(1),
            father: sim(1),
            conception: 0,
        })
    }

    /// From the pool: `born_pool` its birth in the pool run (`None` for a child of a
    /// pregnancy from the pool born in the town), `arrived` its second.
    fn pool(born_pool: Option<SimTime>, arrived: SimTime) -> Option<PoolOrigin> {
        Some(PoolOrigin {
            genome_key: crate::lineage::GenomeKey::of_zygote(key(0)),
            born_pool,
            arrived,
            arrival_year: 0,
            mother: Some(key(0)),
            father: Some(key(0)),
            social_parents: alloc::vec::Vec::new(),
        })
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
            births: 0,
            partnerships: 0,
            household: crate::ids::HouseholdId::try_from(1).expect("valid"),
            moved_out: false,
            carer: None,
            parents: None,
            social_parents: alloc::vec::Vec::new(),
            pool: None,
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
            births: 0,
            partnerships: 0,
            household: crate::ids::HouseholdId::try_from(1).expect("valid"),
            moved_out: false,
            parents: None,
            social_parents: alloc::vec::Vec::new(),
            pool: None,
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
                body: BodyCensus::default(),
                population: PopulationCensus::default(),
            }
        );
        assert_eq!(
            census_row(&domain, last_second(4)),
            CensusRow {
                year: 4,
                living: 2,
                lived_secs: 2 * year_secs,
                body: BodyCensus::default(),
                population: PopulationCensus::default(),
            }
        );
        // Year 2: sims 2, 3 and 4 (until its last second); sim 1 is not born yet.
        assert_eq!(
            census_row(&domain, last_second(2)),
            CensusRow {
                year: 2,
                living: 2,
                lived_secs: year_secs + year_secs + (year_secs - 1),
                body: BodyCensus::default(),
                population: PopulationCensus::default(),
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
                body: BodyCensus::default(),
                population: PopulationCensus::default(),
            }
        );
    }

    /// The causes of death of the repository's data, by hand from
    /// `data/calibration/genetics.json`: senescence and accident, then the diseases with the
    /// effect `death` or `death_in_two_steps` in the order of the data (`core:disease_mt` is
    /// `visible` and kills nobody).
    #[test]
    fn the_causes_of_death_come_in_the_order_of_the_addendum() {
        let data = crate::data::testing::data();
        let causes = crate::state::death::causes(&data);
        let codes: alloc::vec::Vec<&str> = causes.iter().map(|code| code.as_str()).collect();
        assert_eq!(
            codes,
            [
                "core:senescence",
                "core:accident",
                "core:disease_ar",
                "core:disease_ad_late",
                "core:disease_ad_partial",
                "core:disease_xr",
                "core:disease_liability",
            ]
        );
    }

    /// The population of year 3 and of year 2 by hand (a year has 364 days); the deaths in
    /// the order of `the_causes_of_death_come_in_the_order_of_the_addendum`:
    /// - sim 1, a founder made by `Start`, counts nowhere.
    /// - sims 2 and 3 are born in year 3, in the town and of a pregnancy from the pool.
    /// - sim 4 arrives in year 3.
    /// - sim 5 is born in year 3 and dies of `core:disease_ar` in it.
    /// - sim 6 dies of an accident in the last second of year 3.
    /// - sim 7 dies of senescence in the last second of year 2.
    /// - sim 8 leaves the town in year 3.
    /// - sim 9 is born in the last second of year 2, sim 10 arrives in year 2.
    #[test]
    fn the_census_counts_births_arrivals_emigrations_and_deaths_by_cause() {
        let data = crate::data::testing::data();
        let day = DAY.as_secs();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let cause = |code: &str| crate::data_id::Code::new(code).expect("a code").key();
        let died = |n: u64, born: SimTime, left: SimTime, code: &str| ArchivedSim {
            left_by: LeftBy::Death { cause: cause(code) },
            ..archived(n, born, left)
        };
        let mut domain = DomainState::new();
        domain.sims.insert(sim(1), living(1, at(-10, 0)));
        let child = |n: u64, born: SimTime| Sim {
            parents: parents(),
            ..living(n, born)
        };
        domain.sims.insert(sim(2), child(2, at(3, 10 * day)));
        domain.sims.insert(
            sim(3),
            Sim {
                pool: pool(None, at(3, 20 * day)),
                ..living(3, at(3, 20 * day))
            },
        );
        domain.sims.insert(
            sim(4),
            Sim {
                pool: pool(Some(at(-20, 0)), at(3, 30 * day)),
                ..living(4, at(-20, 0))
            },
        );
        domain.archive.insert(
            sim(5),
            ArchivedSim {
                parents: parents(),
                ..died(5, at(3, 40 * day), at(3, 50 * day), "core:disease_ar")
            },
        );
        domain
            .archive
            .insert(sim(6), died(6, at(-30, 0), at(4, -1), "core:accident"));
        domain
            .archive
            .insert(sim(7), died(7, at(-70, 0), at(3, -1), "core:senescence"));
        domain.archive.insert(
            sim(8),
            ArchivedSim {
                left_by: LeftBy::Emigration,
                ..archived(8, at(-30, 0), at(3, 100 * day))
            },
        );
        domain.sims.insert(sim(9), child(9, at(3, -1)));
        domain.sims.insert(
            sim(10),
            Sim {
                pool: pool(Some(at(-20, 0)), at(2, 5 * day)),
                ..living(10, at(-20, 0))
            },
        );
        let last_second = |year: i64| at(year + 1, -1);
        assert_eq!(
            population(&domain, &data, 1, last_second(3)),
            PopulationCensus {
                births: 3,
                arrivals: 1,
                emigrations: 1,
                deaths: alloc::vec![0, 1, 1, 0, 0, 0, 0],
            }
        );
        assert_eq!(
            population(&domain, &data, 1, last_second(2)),
            PopulationCensus {
                births: 1,
                arrivals: 1,
                emigrations: 0,
                deaths: alloc::vec![1, 0, 0, 0, 0, 0, 0],
            }
        );
    }

    /// In year 0 the persons `Start` made (SimIds 1 to 3, from the pool with `arrived = 0`
    /// like an arrival of second 0) are no arrivals; sims 4 and 5, arrived in second 0,
    /// are (ADR-0021, addendum 2026-10-21).
    #[test]
    fn what_start_made_is_no_arrival() {
        let data = crate::data::testing::data();
        let born = SimTime::from_secs(-1000);
        let mut domain = DomainState::new();
        for n in 1..=5 {
            domain.sims.insert(
                sim(n),
                Sim {
                    pool: pool(Some(born), SimTime::START),
                    ..living(n, born)
                },
            );
        }
        let row = population(&domain, &data, 3, year_start(1) - DAY);
        assert_eq!(row.arrivals, 2);
    }
}
