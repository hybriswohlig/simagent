//! The wish for children as a person page shows it (ADR-0017, addendum of 2026-11-05): the
//! drawn wish of the person, and for its open partnership where the couple's wish stands,
//! read from the gate the conception rule uses (`conception::decide`). Read only, like the
//! rest of the presentation state.

use serde::Serialize;

use crate::ids::SimId;
use crate::state::conception::{self, Held, Lot, Opening};
use crate::state::partnership;
use crate::world::World;

use super::whole_years;

/// Where the wish of a couple stands at the last closed second, in the order the page reads
/// it: she is pregnant (then the page shows no sentence of the couple, whatever else holds),
/// her fertility over, the pause after a birth of the partnership, they try (by her wish, by
/// his, by a grown child of hers), her wish is fulfilled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WishState {
    FertilityOver,
    Pregnant,
    Pause,
    Hers,
    His,
    Grown,
    Fulfilled,
}

/// The couple of a person's open partnership and where its wish stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CoupleWish {
    pub woman: SimId,
    pub man: SimId,
    pub state: WishState,
}

/// The wish of a person: none below the market age (whole years), else the drawn wish (not
/// the extra child, not the tilt), and the couple of its open partnership if it lives in one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct WishView {
    pub wish: Option<u32>,
    pub couple: Option<CoupleWish>,
}

impl World {
    /// The wish of `sim`, living or archived, at the last closed second (its end for the
    /// archived); `None` for an unknown sim or before the start.
    pub fn wish(&self, sim: SimId) -> Option<WishView> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        domain.world_seed?;
        let (born, key, ended) = match domain.sims.get(&sim) {
            Some(person) => (person.born, person.lineage_key, None),
            None => {
                let gone = domain.archive.get(&sim)?;
                (gone.born, gone.lineage_key, Some(gone.left))
            }
        };
        let now = ended.unwrap_or_else(|| self.closed_through());
        let market = i64::from(data.demography().market.age_years);
        if whole_years(born, now).is_none_or(|years| years < market) {
            return Some(WishView {
                wish: None,
                couple: None,
            });
        }
        let wish = conception::wish(data, Lot::of(domain, sim), key);
        let couple = if ended.is_some() {
            None
        } else {
            partnership::open_partnership(domain, sim).and_then(|id| {
                let pair = domain.partnerships.get(&id)?;
                let (woman, man) = (pair.woman, pair.man);
                let facts = conception::pair_facts(domain, data, (woman, man, id));
                let life = domain.sims.get(&woman)?.births;
                // The pause after her last birth in it, in the town or brought from the pool.
                let paused =
                    conception::pause_end(domain, data, woman, id).is_some_and(|end| end > now);
                let pregnant = domain.pregnancies.contains_key(&woman);
                let couple = facts.couple(data, now, life, (pregnant, paused));
                // While she is pregnant the page shows no sentence of the couple, also past the
                // end of her fertility (which `decide` reads first, for the plan).
                let state = if pregnant {
                    WishState::Pregnant
                } else {
                    match conception::decide(&couple) {
                        Ok(Opening::Hers) => WishState::Hers,
                        Ok(Opening::His) => WishState::His,
                        Ok(Opening::Grown) => WishState::Grown,
                        Err(Held::FertilityOver) => WishState::FertilityOver,
                        Err(Held::Pregnant) => WishState::Pregnant,
                        Err(Held::Pause) => WishState::Pause,
                        Err(Held::Fulfilled) => WishState::Fulfilled,
                    }
                };
                Some(CoupleWish { woman, man, state })
            })
        };
        Some(WishView {
            wish: Some(wish),
            couple,
        })
    }
}
