//! A person's traits in steps, as in Crusader Kings (Grok's round 43, R2): smart, strong and
//! beautiful, three steps up and three down, nothing in the middle. They are shown only and
//! act on nothing; they come from the phenotypes with the adult weights, so a person keeps
//! them from birth, and they pass on through the genes beneath them.

use serde::Serialize;

use crate::data_id::DataId;
use crate::genome::Sex;
use crate::ids::SimId;
use crate::time::YEAR;
use crate::world::World;

use super::card::facts;

/// The steps of a trait: `0` none, `1` to `3` up, `-1` to `-3` down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Traits {
    pub cognition: i8,
    pub strength: i8,
    pub beauty: i8,
}

/// The bounds of cognition and strength: `|s|` from 1.5, 2.0, 2.5.
const PLAIN: ([f64; 3], [f64; 3]) = ([1.5, 2.0, 2.5], [-1.5, -2.0, -2.5]);
/// The bounds of beauty (its handsome pole sits at the average face, its plain pole is long):
/// up from 1.3, 1.7, 2.0, down from -1.6, -2.4, -3.3.
const BEAUTY: ([f64; 3], [f64; 3]) = ([1.3, 1.7, 2.0], [-1.6, -2.4, -3.3]);
/// The start values of beauty until they are measured on the reference births: the mean and
/// spread of `Q = Σ f_k²` over the twelve face axes (correlated through growth, so 5.75 and
/// not √24) and the spread of `−0.8 · A + 0.5 · D`.
const Q_MEAN: f64 = 12.0;
const Q_SD: f64 = 5.75;
const RAW_SD: f64 = 0.94;

/// The step of `s` against `(up, down)`: the count of the bounds reached on its side.
pub fn step(s: f64, (up, down): ([f64; 3], [f64; 3])) -> i8 {
    let reached = |bounds: [f64; 3], above: bool| {
        let count = bounds
            .iter()
            .filter(|&&b| if above { s >= b } else { s <= b })
            .count();
        i8::try_from(count).unwrap_or_else(|_| panic!("invariant traits.step"))
    };
    reached(up, true) - reached(down, false)
}

/// `s` of beauty from the twelve face axes' `z` and the sex-typical `z` (estrogen for women,
/// androgen for men): `A = (Q − 12) / 5.75`, `s = (−0.8 · A + 0.5 · D) / 0.94`.
pub fn beauty(faces: &[f64; 12], typical: f64) -> f64 {
    let q = faces.iter().fold(0.0, |sum, f| sum + f * f);
    let a = (q - Q_MEAN) / Q_SD;
    (-0.8 * a + 0.5 * typical) / RAW_SD
}

/// The traits of `s` values of cognition, strength and beauty.
pub fn traits_of(cognition: f64, strength: f64, beauty: f64) -> Traits {
    Traits {
        cognition: step(cognition, PLAIN),
        strength: step(strength, PLAIN),
        beauty: step(beauty, BEAUTY),
    }
}

impl World {
    /// The traits of `sim`, living or archived, from its phenotypes with the adult weights;
    /// `None` for an unknown sim or a map without the traits read.
    pub fn traits(&self, sim: SimId) -> Option<Traits> {
        let state = self.state();
        let (_, _, genome, _) = facts(state.domain(), self.closed_through(), sim)?;
        // At a hundred years the weights are the adult ones exactly (the transition is 1 in
        // f64 from about 85 years on).
        let adult = YEAR
            .checked_mul(100)
            .unwrap_or_else(|| panic!("invariant traits.age"));
        let z = |t| {
            let t = DataId::new(t).unwrap_or_else(|_| panic!("invariant traits.trait_id"));
            self.parts_at(sim, &t, adult).map(|parts| parts.z())
        };
        let names = [
            "core:face_01",
            "core:face_02",
            "core:face_03",
            "core:face_04",
            "core:face_05",
            "core:face_06",
            "core:face_07",
            "core:face_08",
            "core:face_09",
            "core:face_10",
            "core:face_11",
            "core:face_12",
        ];
        let mut faces = [0.0; 12];
        for (face, t) in faces.iter_mut().zip(names) {
            *face = z(t)?;
        }
        let typical = z(match genome.sex() {
            Sex::Female => "core:estrogen",
            Sex::Male => "core:androgen",
        })?;
        Some(traits_of(
            z("core:cognition")?,
            z("core:muscle")?,
            beauty(&faces, typical),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_start_at_their_bounds() {
        assert_eq!(step(0.0, PLAIN), 0);
        assert_eq!(step(1.499, PLAIN), 0);
        assert_eq!(step(1.5, PLAIN), 1);
        assert_eq!(step(2.0, PLAIN), 2);
        assert_eq!(step(2.5, PLAIN), 3);
        assert_eq!(step(9.0, PLAIN), 3);
        assert_eq!(step(-1.499, PLAIN), 0);
        assert_eq!(step(-1.5, PLAIN), -1);
        assert_eq!(step(-2.0, PLAIN), -2);
        assert_eq!(step(-2.5, PLAIN), -3);
        assert_eq!(step(1.299, BEAUTY), 0);
        assert_eq!(step(1.3, BEAUTY), 1);
        assert_eq!(step(1.7, BEAUTY), 2);
        assert_eq!(step(2.0, BEAUTY), 3);
        assert_eq!(step(-1.599, BEAUTY), 0);
        assert_eq!(step(-1.6, BEAUTY), -1);
        assert_eq!(step(-2.4, BEAUTY), -2);
        assert_eq!(step(-3.3, BEAUTY), -3);
    }

    #[test]
    fn beauty_is_the_average_face_and_the_typical_form() {
        // Q = 12 (every axis 1) and D = 0: A = 0, s = 0.
        assert_eq!(beauty(&[1.0; 12], 0.0), 0.0);
        // Q = 0 (the average face): A = −12 / 5.75, s = (0.8 · 12 / 5.75) / 0.94.
        let average = beauty(&[0.0; 12], 0.0);
        assert!((average - (0.8 * 12.0 / 5.75) / 0.94).abs() < 1e-12);
        // A sex-typical form adds 0.5 / 0.94 per standard deviation.
        assert!((beauty(&[1.0; 12], 1.0) - 0.5 / 0.94).abs() < 1e-12);
        // A face far from the average: Q = 12 · 9 = 108, A = 96 / 5.75.
        assert!(beauty(&[3.0; 12], 0.0) < -3.3);
    }
}
