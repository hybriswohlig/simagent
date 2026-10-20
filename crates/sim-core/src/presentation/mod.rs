//! The presentation state (ADR-0021 section 3): what the viewer shows of a person, read from
//! the world and never written to it. Its body at the second shown (now for the living, the
//! second of death or emigration for the others) and at reached ages, with the age in whole
//! years and the calendar year of every second shown. It is no state: not in the hash, not
//! in the save, and taking it changes nothing.

pub mod kin;
pub mod names;
pub mod partners;

use alloc::vec::Vec;

use serde::Serialize;

use crate::body_contract::BodyParams;
use crate::ids::SimId;
use crate::state::LeftBy;
use crate::time::{SimTime, YEAR, calendar};
use crate::world::World;

/// The reached ages whose bodies a person page shows (the ages of Grok's G10).
pub const SHOWN_AGES: [i64; 7] = [2, 8, 12, 16, 25, 40, 60];

/// A second of a person's life: the second, its age then in whole years, its calendar year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Moment {
    pub t: SimTime,
    pub age_years: i64,
    pub calendar_year: i64,
}

/// A body at a second of a person's life.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shown {
    pub at: Moment,
    pub body: BodyParams,
}

/// How a life in the town ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum End {
    Death,
    Emigration,
}

/// What the viewer shows of one person (ADR-0021 section 3); the reader adds the contract
/// version of the body values.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PersonView {
    pub sim: SimId,
    pub born: Moment,
    /// `None` while the person lives in the town.
    pub ended: Option<End>,
    /// The body now for the living, at the second of death or emigration for the others.
    pub shown: Shown,
    /// The bodies at each age of [`SHOWN_AGES`] the person reached before `shown`.
    pub at_ages: Vec<Shown>,
}

/// The second `t` of a life that began at `born`: its age in whole years and its calendar
/// year; `None` before the birth.
fn moment(born: SimTime, t: SimTime) -> Option<Moment> {
    let age = t.checked_since(born).filter(|age| age.as_secs() >= 0)?;
    Some(Moment {
        t,
        age_years: age.as_secs().div_euclid(YEAR.as_secs()),
        calendar_year: calendar(t).year,
    })
}

impl World {
    /// The presentation state of `sim`, living or archived; `None` for an unknown sim or
    /// data without the body. "Now" is the last closed second of the world.
    pub fn person_view(&self, sim: SimId) -> Option<PersonView> {
        let domain = self.state().domain();
        let (born, end) = match domain.sims.get(&sim) {
            Some(person) => (person.born, None),
            None => {
                let gone = domain.archive.get(&sim)?;
                let end = match gone.left_by {
                    LeftBy::Death { .. } => End::Death,
                    LeftBy::Emigration => End::Emigration,
                };
                (gone.born, Some((gone.left, end)))
            }
        };
        let last = end.map_or_else(|| self.closed_through(), |(left, _)| left);
        let shown_at = |t: SimTime| -> Option<Shown> {
            let body = self.state().body_at(sim, t)?.params;
            Some(Shown {
                at: moment(born, t)?,
                body,
            })
        };
        let shown = shown_at(last)?;
        let at_ages = SHOWN_AGES
            .iter()
            .filter_map(|age| {
                let secs = age
                    .checked_mul(YEAR.as_secs())?
                    .checked_add(born.as_secs())?;
                let t = SimTime::from_secs(secs);
                (t <= last).then(|| shown_at(t)).flatten()
            })
            .collect();
        Some(PersonView {
            sim,
            born: moment(born, born)?,
            ended: end.map(|(_, end)| end),
            shown,
            at_ages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A year is 52 weeks of 7 days (ADR-0002, `YEAR`). Ages count whole years lived, the
    /// calendar year is the year of the second.
    #[test]
    fn a_moment_has_its_whole_years_and_its_calendar_year() {
        let y = YEAR.as_secs();
        let at = |born: i64, t: i64| moment(SimTime::from_secs(born), SimTime::from_secs(t));
        let pair = |m: Option<Moment>| m.map(|m| (m.age_years, m.calendar_year));
        assert_eq!(pair(at(-1, 0)), Some((0, 0)));
        assert_eq!(pair(at(-1, y - 2)), Some((0, 0)));
        assert_eq!(pair(at(-1, y - 1)), Some((1, 0)));
        assert_eq!(pair(at(-1, y)), Some((1, 1)));
        assert_eq!(pair(at(-30 * y, 0)), Some((30, 0)));
        assert_eq!(pair(at(-30 * y, -1)), Some((29, -1)));
        assert_eq!(pair(at(0, -1)), None);
    }
}
