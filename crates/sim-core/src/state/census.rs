//! The yearly census (ADR-0004 section 8): values from the state, never from the log
//! and never from earlier rows; the one value from outside is the number of persons
//! `Start` made (ADR-0021, addendum 2026-10-21).

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, LeftBy, Parents, PoolOrigin, death, disease, market, whole_years};
use crate::body_contract::BodyCensus;
use crate::data::Data;
use crate::data_id::{Code, str_key};
use crate::genome::Sex;
use crate::ids::SimId;
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
    /// The switch, the affected, the heights and the newborns' inbreeding (ADR-0021,
    /// addendum 2026-10-21; Grok R1 to R3).
    pub inheritance: InheritanceCensus,
}

/// The inheritance columns of a row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InheritanceCensus {
    /// The switch of the hereditary diseases at the end of the year.
    pub diseases_on: bool,
    /// Per disease of the data, in its order: the living the card calls affected.
    pub affected: Vec<u64>,
    /// The living of at least 25 full years whose body the core evaluates, women then men.
    pub adults: [HeightMean; 2],
    /// The same of exactly 25 full years.
    pub age25: [HeightMean; 2],
    /// The mean inbreeding `F` of the year's newborns, times 10 000.
    pub inbreeding_e4: u64,
}

/// How many were measured, and the mean of their height now in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeightMean {
    pub count: u64,
    pub mm: u64,
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
        inheritance: InheritanceCensus::default(),
    }
}

/// Born in the town: of two town parents or of a pregnancy from the pool.
fn born_in_town(parents: Option<&Parents>, pool: Option<&PoolOrigin>) -> bool {
    parents.is_some() || pool.is_some_and(|origin| origin.born_pool.is_none())
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
        if born_in_town(parents.as_ref(), pool.as_ref()) && within(born) {
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

/// The inheritance of the year that contains `t`; `heights` the height now of each living
/// sim whose body the core evaluates, in ascending SimId. The means after the addendum's
/// contract (`mean_e`): each value scaled, added in ascending SimId, divided once.
pub(crate) fn inheritance(
    domain: &DomainState,
    data: &Data,
    t: SimTime,
    heights: &[(SimId, f64)],
) -> InheritanceCensus {
    let diseases_on = (domain.world_params.as_ref()).is_some_and(|p| p.hereditary_diseases);
    let keys: Vec<u64> = data.diseases().iter().map(|d| str_key(&d.code)).collect();
    let mut affected = vec![0; keys.len()];
    for sim in domain.sims.values().filter(|_| diseases_on) {
        let age = disease::age(sim.born, t);
        for affliction in (sim.afflictions.iter()).filter(|a| age >= a.onset.get()) {
            let at = (keys.iter())
                .position(|key| *key == affliction.disease)
                .unwrap_or_else(|| panic!("invariant census.disease"));
            affected[at] += 1;
        }
    }
    let (mut sums, mut counts) = ([[0.0; 2]; 2], [[0_u64; 2]; 2]);
    for &(id, cm) in heights {
        let sim = (domain.sims.get(&id)).unwrap_or_else(|| panic!("invariant census.measured"));
        let sex = match sim.sex {
            Sex::Female => 0,
            Sex::Male => 1,
        };
        for (group, member) in [
            (0, market::of_age(sim.born, t, 25)),
            (1, whole_years(sim.born, t) == 25),
        ] {
            if member {
                sums[group][sex] += cm * 10.0;
                counts[group][sex] += 1;
            }
        }
    }
    let means = |group: usize| {
        [0, 1].map(|sex| HeightMean {
            count: counts[group][sex],
            mm: mean_e(sums[group][sex], counts[group][sex]),
        })
    };
    let year = calendar(t).year;
    let (from, to) = (year_start(year), year_start(year + 1));
    let mut newborns: Vec<SimId> = (domain.sims.iter())
        .map(|(id, sim)| (*id, sim.born, sim.parents.as_ref(), sim.pool.as_ref()))
        .chain(
            (domain.archive.iter())
                .map(|(id, sim)| (*id, sim.born, sim.parents.as_ref(), sim.pool.as_ref())),
        )
        .filter(|(_, born, parents, pool)| {
            born_in_town(*parents, *pool) && from <= *born && *born < to
        })
        .map(|(id, ..)| id)
        .collect();
    newborns.sort_unstable();
    let mut inbred = 0.0;
    for &id in &newborns {
        let parents = domain
            .parent_keys(id)
            .unwrap_or_else(|| panic!("invariant census.parents"));
        let lookup = |key| market::kin_of(domain, data, key);
        inbred += crate::kinship::phi(parents.mother, parents.father, lookup) * 10_000.0;
    }
    let born = u64::try_from(newborns.len()).unwrap_or_else(|_| panic!("invariant census.count"));
    InheritanceCensus {
        diseases_on,
        affected,
        adults: means(0),
        age25: means(1),
        inbreeding_e4: mean_e(inbred, born),
    }
}

/// The addendum's mean (ADR-0021, 2026-10-21): `sum` divided once by `count`, then the
/// nearest whole number, exactly half away from zero (both are never negative here); 0
/// for nobody. Rounded with `ceil`, exact below 2^53.
pub(crate) fn mean_e(sum: f64, count: u64) -> u64 {
    if count == 0 {
        return 0;
    }
    let n = i64::try_from(count)
        .ok()
        .and_then(sim_math::i64_to_f64_exact)
        .unwrap_or_else(|| panic!("invariant census.mean_count"));
    let m = sum / n;
    let up = sim_math::ceil(m);
    let down = if up > m { up - 1.0 } else { up };
    let near = if m - down >= 0.5 { down + 1.0 } else { down };
    sim_math::to_i64_exact(near)
        .and_then(|whole| u64::try_from(whole).ok())
        .unwrap_or_else(|| panic!("invariant census.mean"))
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
                inheritance: InheritanceCensus::default(),
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
                inheritance: InheritanceCensus::default(),
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
                inheritance: InheritanceCensus::default(),
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
                inheritance: InheritanceCensus::default(),
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
        let code = |text| crate::data_id::Code::new(text).expect("a code");
        assert_eq!(
            crate::state::death::causes(&data),
            [
                code("core:senescence"),
                code("core:accident"),
                code("core:disease_ar"),
                code("core:disease_ad_late"),
                code("core:disease_ad_partial"),
                code("core:disease_xr"),
                code("core:disease_liability"),
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
    /// - sim 11 is born in the first second of year 3: in year 3, not in year 2 (the year
    ///   is half-open).
    #[test]
    fn the_census_counts_births_arrivals_emigrations_and_deaths_by_cause() {
        let data = crate::data::testing::data();
        let day = DAY.as_secs();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let cause = |text| crate::data_id::Code::new(text).expect("a code").key();
        let died = |n: u64, born: SimTime, left: SimTime, cause: u64| ArchivedSim {
            left_by: LeftBy::Death { cause },
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
                ..died(
                    5,
                    at(3, 40 * day),
                    at(3, 50 * day),
                    cause("core:disease_ar"),
                )
            },
        );
        domain.archive.insert(
            sim(6),
            died(6, at(-30, 0), at(4, -1), cause("core:accident")),
        );
        domain.archive.insert(
            sim(7),
            died(7, at(-70, 0), at(3, -1), cause("core:senescence")),
        );
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
        domain.sims.insert(sim(11), child(11, at(3, 0)));
        let last_second = |year: i64| at(year + 1, -1);
        assert_eq!(
            population(&domain, &data, 1, last_second(3)),
            PopulationCensus {
                births: 4,
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

    /// The living with `afflictions` (the key of the disease's code and its onset in years).
    fn afflicted(n: u64, born: SimTime, afflictions: &[(u64, f64)]) -> Sim {
        Sim {
            afflictions: (afflictions.iter())
                .map(|&(disease, onset)| crate::state::Affliction {
                    disease,
                    onset: crate::finite::FiniteF64::try_new(onset).expect("finite"),
                    exposure: None,
                })
                .collect(),
            ..living(n, born)
        }
    }

    fn switch(domain: &mut DomainState, on: bool) {
        domain.world_params = Some(crate::params::WorldParams {
            hereditary_diseases: on,
            nutrition: alloc::vec::Vec::new(),
        });
    }

    /// The affected at the end of year 3 by hand, the diseases in the order of the data
    /// (`core:disease_ar`, `core:disease_ad_late`, `core:disease_ad_partial`,
    /// `core:disease_xr`, `core:disease_mt`, `core:disease_liability`): the card's rule,
    /// the switch on and the age at least the onset.
    /// - sim 1, born 20 years before year 0: `core:disease_ar` from 0 years (affected) and
    ///   `core:disease_liability` from 25 (not yet: almost 24 years old).
    /// - sim 2, born 30 years before year 0: `core:disease_ad_late` from 30 and
    ///   `core:disease_mt` from 25 (both affected: almost 34 years old).
    /// - sim 3 is archived with `core:disease_ar` and counts nowhere.
    ///
    /// With the switch off nobody is affected and `diseases_on` is false.
    #[test]
    fn the_census_counts_the_affected_by_the_cards_rule() {
        let data = crate::data::testing::data();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let key = |text| crate::data_id::Code::new(text).expect("a code").key();
        let mut domain = DomainState::new();
        domain.sims.insert(
            sim(1),
            afflicted(
                1,
                at(-20, 0),
                &[
                    (key("core:disease_ar"), 0.0),
                    (key("core:disease_liability"), 25.0),
                ],
            ),
        );
        domain.sims.insert(
            sim(2),
            afflicted(
                2,
                at(-30, 0),
                &[
                    (key("core:disease_ad_late"), 30.0),
                    (key("core:disease_mt"), 25.0),
                ],
            ),
        );
        domain.archive.insert(
            sim(3),
            ArchivedSim {
                afflictions: afflicted(3, at(-20, 0), &[(key("core:disease_ar"), 0.0)]).afflictions,
                ..archived(3, at(-20, 0), at(3, 0))
            },
        );
        let end = at(4, -1);
        switch(&mut domain, true);
        let on = inheritance(&domain, &data, end, &[]);
        assert!(on.diseases_on);
        assert_eq!(on.affected, alloc::vec![1, 1, 0, 0, 1, 0]);
        switch(&mut domain, false);
        let off = inheritance(&domain, &data, end, &[]);
        assert!(!off.diseases_on);
        assert_eq!(off.affected, alloc::vec![0, 0, 0, 0, 0, 0]);
    }

    /// The heights at the end of year 3 by hand, from the heights the bodies give (in cm):
    /// - women of at least 25: sim 5 (born 30 years before year 0, 160.0), sim 6 (40
    ///   years, 165.5) and sim 9 (born in the last second of year −22, so exactly 25
    ///   full years, 170.25): 1600 + 1655 + 1702.5 = 4957.5 tenths of a cm (exact), mean
    ///   1652.5, half away from zero 1653 mm.
    /// - men of at least 25: sim 7 (30 years, 175.0) and sim 8 (born in the first second
    ///   of year −22, 25 full years, 180.0): mean 1775 mm.
    /// - exactly 25: the woman sim 9, 1702.5 → 1703 mm; the man sim 8, 1800 mm.
    /// - sim 10, a child, counts nowhere.
    #[test]
    fn the_census_averages_the_heights_of_the_adults_and_of_the_25_year_olds() {
        let data = crate::data::testing::data();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let person = |n: u64, born: SimTime, sex: Sex| Sim {
            sex,
            ..living(n, born)
        };
        let mut domain = DomainState::new();
        let people = [
            (5, at(-30, 0), Sex::Female, 160.0),
            (6, at(-40, 0), Sex::Female, 165.5),
            (7, at(-30, 0), Sex::Male, 175.0),
            (8, at(-22, 0), Sex::Male, 180.0),
            (9, at(-21, -1), Sex::Female, 170.25),
            (10, at(0, 0), Sex::Female, 100.0),
        ];
        for (n, born, sex, _) in people {
            domain.sims.insert(sim(n), person(n, born, sex));
        }
        let heights: alloc::vec::Vec<(SimId, f64)> =
            (people.iter()).map(|&(n, _, _, cm)| (sim(n), cm)).collect();
        let row = inheritance(&domain, &data, at(4, -1), &heights);
        let mean = |count, mm| HeightMean { count, mm };
        assert_eq!(row.adults, [mean(3, 1653), mean(2, 1775)]);
        assert_eq!(row.age25, [mean(1, 1703), mean(1, 1800)]);
    }

    /// The newborns' inbreeding by hand: sims 1 and 2 are unrelated founders, sims 3 and 4
    /// their children, so φ(3, 4) = ½(φ(1, 4) + φ(2, 4)) = ½(¼ + ¼) = ¼.
    /// - year 3: sim 5 of 3 and 4, `F` = ¼, 2500.
    /// - sim 6 of 1 and 2 (`F` = 0) is born in the first second of year 4: in year 4 only
    ///   (the year is half-open), else year 3 would give 1250.
    #[test]
    fn the_newborns_of_a_year_are_those_born_in_it() {
        let data = crate::data::testing::data();
        let at = |year: i64, secs: i64| SimTime::from_secs(year_start(year).as_secs() + secs);
        let of = |mother: u64, father: u64| {
            Some(Parents {
                mother: sim(mother),
                father: sim(father),
                conception: 0,
            })
        };
        let mut domain = DomainState::new();
        let people = [
            (1, at(-40, 0), Sex::Female, None),
            (2, at(-40, 0), Sex::Male, None),
            (3, at(-20, 0), Sex::Female, of(1, 2)),
            (4, at(-19, 0), Sex::Male, of(1, 2)),
            (5, at(3, 100), Sex::Female, of(3, 4)),
            (6, at(4, 0), Sex::Male, of(1, 2)),
        ];
        for (n, born, sex, parents) in people {
            let person = Sim {
                sex,
                parents,
                ..living(n, born)
            };
            domain.keys.insert(person.lineage_key, sim(n));
            domain.sims.insert(sim(n), person);
        }
        assert_eq!(
            inheritance(&domain, &data, at(4, -1), &[]).inbreeding_e4,
            2500
        );
        assert_eq!(inheritance(&domain, &data, at(5, -1), &[]).inbreeding_e4, 0);
    }

    /// The mean of the addendum by hand: one division, then half away from zero.
    #[test]
    fn a_mean_rounds_half_away_from_zero() {
        assert_eq!(mean_e(625.0, 2), 313);
        assert_eq!(mean_e(624.999_999_999_9, 2), 312);
        assert_eq!(mean_e(625.0, 3), 208);
        assert_eq!(mean_e(3550.0, 2), 1775);
        assert_eq!(mean_e(0.0, 4), 0);
        assert_eq!(mean_e(0.0, 0), 0);
    }
}
