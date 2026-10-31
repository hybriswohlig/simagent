//! A person's looks in words, for finding persons (the filter of the person list): hair,
//! eyes, skin, curls and freckles from the values of the body contract at the second shown,
//! height and cognition in five bands of the percentile against the model reference (Grok's
//! round 43, R6). Greying never changes the word of the hair; the darkening of childhood is
//! in the melanin, so a blond child is called blond.

use serde::Serialize;

use crate::body_contract::{BodyParams, mc1r};
use crate::ids::SimId;
use crate::world::World;

use super::card::{card_traits, facts};

/// The colour of the hair: red with two MC1R copies, otherwise by hair eumelanin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hair {
    Blond,
    LightBrown,
    DarkBrown,
    Black,
    Red,
}

/// The colour of the eyes, by iris melanin after HERC2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Eyes {
    Blue,
    Green,
    Hazel,
    Brown,
}

/// The tone of the skin, by skin eumelanin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Skin {
    VeryLight,
    Light,
    Beige,
    Tanned,
}

/// The form of the hair, by `hair_curl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Curls {
    Straight,
    Wavy,
    Curly,
    Kinky,
}

/// How freckled, by `freckle_density`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Freckles {
    None,
    Few,
    Clear,
    Many,
}

/// A band of the percentile against the model reference: up to 10, up to 30, up to 70, up
/// to 90, above 90.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Low,
    RatherLow,
    Middle,
    RatherHigh,
    High,
}

/// A person's looks in words (see the module).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Looks {
    pub hair: Hair,
    pub eyes: Eyes,
    pub skin: Skin,
    pub curls: Curls,
    pub freckles: Freckles,
    pub height: Band,
    pub cognition: Band,
}

/// The first word of `words` whose bound `x` stays under, else the last word.
fn under<T: Copy>(x: f64, bounds: &[f64], words: &[T]) -> T {
    let at = bounds.iter().take_while(|&&bound| x >= bound).count();
    words[at]
}

/// The band of `z`: the percentile `Φ(z)` up to 10, 30, 70, 90 or above.
pub fn band(z: f64) -> Band {
    let bounds = [0.10, 0.30, 0.70, 0.90].map(sim_math::probit);
    under(
        z,
        &bounds,
        &[
            Band::Low,
            Band::RatherLow,
            Band::Middle,
            Band::RatherHigh,
            Band::High,
        ],
    )
}

/// The looks of a body with `mc1r` copies of MC1R, the target height `z_h` and cognition `z`.
pub fn looks_of(body: &BodyParams, mc1r: u8, height_z: f64, cognition_z: f64) -> Looks {
    let hair = if mc1r == 2 {
        Hair::Red
    } else {
        under(
            body.hair_eumelanin(),
            &[0.16, 0.36, 0.75],
            &[Hair::Blond, Hair::LightBrown, Hair::DarkBrown, Hair::Black],
        )
    };
    Looks {
        hair,
        eyes: under(
            body.iris_melanin(),
            &[0.20, 0.45, 0.60],
            &[Eyes::Blue, Eyes::Green, Eyes::Hazel, Eyes::Brown],
        ),
        skin: under(
            body.skin_eumelanin(),
            &[0.05, 0.10, 0.18],
            &[Skin::VeryLight, Skin::Light, Skin::Beige, Skin::Tanned],
        ),
        curls: under(
            body.hair_curl(),
            &[0.15, 0.40, 0.70],
            &[Curls::Straight, Curls::Wavy, Curls::Curly, Curls::Kinky],
        ),
        freckles: under(
            body.freckle_density(),
            &[0.10, 0.30, 0.60],
            &[
                Freckles::None,
                Freckles::Few,
                Freckles::Clear,
                Freckles::Many,
            ],
        ),
        height: band(height_z),
        cognition: band(cognition_z),
    }
}

impl World {
    /// The looks of `sim` at its second shown, living or archived; `None` for an unknown sim.
    pub fn looks(&self, sim: SimId) -> Option<Looks> {
        let state = self.state();
        let (_, _, genome, _) = facts(state.domain(), self.closed_through(), sim)?;
        let copies = (state.data().genome_map().loci.iter())
            .find(|locus| locus.id == mc1r())
            .map_or(0, |locus| genome.dose(locus));
        let body = self.person_view(sim)?.shown.body;
        let [growth, cognition, _] = card_traits();
        let z = |t| self.parts_shown(sim, t).map(|(parts, _)| parts.z());
        Some(looks_of(&body, copies, z(&growth)?, z(&cognition)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_at_a_bound_takes_the_word_above_it() {
        let words = ["a", "b", "c", "d"];
        let bounds = [0.16, 0.36, 0.75];
        assert_eq!(under(0.0, &bounds, &words), "a");
        assert_eq!(under(0.159_999, &bounds, &words), "a");
        assert_eq!(under(0.16, &bounds, &words), "b");
        assert_eq!(under(0.36, &bounds, &words), "c");
        assert_eq!(under(0.749_999, &bounds, &words), "c");
        assert_eq!(under(0.75, &bounds, &words), "d");
        assert_eq!(under(1.0, &bounds, &words), "d");
    }

    #[test]
    fn the_bands_split_the_percentile_at_10_30_70_and_90() {
        let at = |p: f64| band(sim_math::probit(p));
        assert_eq!(band(-4.0), Band::Low);
        assert_eq!(at(0.099), Band::Low);
        assert_eq!(at(0.10), Band::RatherLow);
        assert_eq!(at(0.299), Band::RatherLow);
        assert_eq!(at(0.30), Band::Middle);
        assert_eq!(band(0.0), Band::Middle);
        assert_eq!(at(0.699), Band::Middle);
        assert_eq!(at(0.70), Band::RatherHigh);
        assert_eq!(at(0.899), Band::RatherHigh);
        assert_eq!(at(0.90), Band::High);
        assert_eq!(band(4.0), Band::High);
    }
}
