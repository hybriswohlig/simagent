//! Where a person's looks and traits come from, line by line (Grok's round 43, R3, and round
//! 45, H1 to H5): hair, eyes and skin always; height, curls and freckles when the parents'
//! part is clear; up to three face axes; the traits with a step; the copies of MC1R and HERC2.
//! `a` and `b` are the mother's and the father's gamete in the unit of the genetic part of
//! `z` (`√h² · G` with the adult `h²`), their sum `D`.

use alloc::vec::Vec;

use serde::Serialize;

use crate::body_contract::{herc2, mc1r};
use crate::data::Data;
use crate::data_id::DataId;
use crate::genome::Genome;
use crate::ids::SimId;
use crate::world::World;

use super::card::facts;
use super::looks::{Band, Curls, Eyes, Freckles, Hair, Skin};

/// Where a line's word comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Mother,
    Father,
    Both,
    /// "von beiden, bei den Eltern kaum zu sehen".
    BothFaint,
    /// "Eltern nicht in der Stadt".
    Away,
    /// The word stands alone: two copies of MC1R or HERC2 carry it, their line tells whence.
    Copies,
}

/// Where one copy of a large pigment locus came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyFrom {
    Mother,
    Father,
    New,
}

/// A line of the card's origin, in the card's order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OriginLine {
    Hair {
        word: Hair,
        source: Source,
    },
    /// The copies of MC1R, one or two, in the order mother's, father's copy.
    RedCopies {
        copies: Vec<CopyFrom>,
    },
    Eyes {
        word: Eyes,
        source: Source,
    },
    /// The copies of HERC2, one or two.
    BlueCopies {
        copies: Vec<CopyFrom>,
    },
    Skin {
        word: Skin,
        source: Source,
    },
    Height {
        word: Band,
        source: Source,
    },
    Curls {
        word: Curls,
        source: Source,
    },
    Freckles {
        word: Freckles,
        source: Source,
    },
    /// Freckles that come with MC1R ("mit dem roten Haar", "mit der Anlage für rotes Haar").
    FrecklesRed {
        word: Freckles,
        copies: u8,
    },
    Face {
        axis: u8,
        up: bool,
        source: Source,
    },
    Smart {
        step: i8,
        source: Source,
    },
    Strong {
        step: i8,
        source: Source,
    },
    Beauty {
        step: i8,
        source: Source,
    },
}

/// The bound of `|D|` below which the parents' part is not told apart.
const CLEAR: f64 = 0.40;
/// The least part a parent must have to be named.
const NAMED: f64 = 0.25;

/// The source of the gametes `a` and `b` (R3): under `|D|` 0.40 "von beiden"; else the
/// parent with at least twice the other's part and at least 0.25, otherwise both; "kaum zu
/// sehen" with both when the larger part is under 0.25.
pub fn source(a: f64, b: f64) -> Source {
    let d = a + b;
    let sign = if d < 0.0 { -1.0 } else { 1.0 };
    let (m, p) = (a * sign, b * sign);
    if d.abs() >= CLEAR {
        if m >= 2.0 * p && m >= NAMED {
            return Source::Mother;
        }
        if p >= 2.0 * m && p >= NAMED {
            return Source::Father;
        }
    }
    if m < NAMED && p < NAMED {
        Source::BothFaint
    } else {
        Source::Both
    }
}

/// Beauty's source down (H3): the sums of the squared parts over the face axes; a parent
/// when its sum is at least 1.5 and twice the other, otherwise both, faintly.
pub fn beauty_source(mother: f64, father: f64) -> Source {
    if mother >= 1.5 && mother >= 2.0 * father {
        Source::Mother
    } else if father >= 1.5 && father >= 2.0 * mother {
        Source::Father
    } else {
        Source::BothFaint
    }
}

/// The gametes of trait `t` in the unit of the genetic part of `z` (the adult `√h²`).
fn gametes(genome: &Genome, data: &Data, t: &DataId) -> Option<(f64, f64)> {
    let [_, _, h2_adult, _] = data.phenotype_weights(t)?;
    let root = sim_math::sqrt(h2_adult);
    genome
        .gamete_values(t, data)
        .map(|(a, b)| (root * a, root * b))
}

/// Two traits mixed with the slopes of a logit, on the scale of one `z`.
fn mixed((a1, b1): (f64, f64), (a2, b2): (f64, f64), s1: f64, s2: f64) -> (f64, f64) {
    let n = sim_math::sqrt(s1 * s1 + s2 * s2);
    ((s1 * a1 + s2 * a2) / n, (s1 * b1 + s2 * b2) / n)
}

/// Where each of a person's copies of allele 1 at a large pigment locus came from: the
/// mother's copy from her if she carries the allele, else new; likewise the father's.
fn copies_from(own: &[u8], mother: &[u8], father: &[u8]) -> Vec<CopyFrom> {
    let mut out = Vec::new();
    if own.first() == Some(&1) {
        out.push(if mother.contains(&1) {
            CopyFrom::Mother
        } else {
            CopyFrom::New
        });
    }
    if own.get(1) == Some(&1) {
        out.push(if father.contains(&1) {
            CopyFrom::Father
        } else {
            CopyFrom::New
        });
    }
    out
}

impl World {
    /// The origin lines of `sim`'s card at its second shown; `None` for an unknown sim.
    pub fn origin(&self, sim: SimId) -> Option<Vec<OriginLine>> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let now = self.closed_through();
        let (_, _, genome, _) = facts(domain, now, sim)?;
        let looks = self.looks(sim)?;
        let traits = self.traits(sim)?;
        let body = self.person_view(sim)?.shown.body;
        let parents: Option<[&Genome; 2]> = (domain.parent_keys(sim))
            .and_then(|keys| {
                let of = |key| domain.keys.get(&key).copied();
                Some([of(keys.mother)?, of(keys.father)?])
            })
            .and_then(|[m, f]| Some([facts(domain, now, m)?.2, facts(domain, now, f)?.2]));
        let away = parents.is_none();
        let by = |pair: Option<(f64, f64)>| match (away, pair) {
            (true, _) | (false, None) => Source::Away,
            (false, Some((a, b))) => source(a, b),
        };
        let clear = |pair: Option<(f64, f64)>| pair.is_some_and(|(a, b)| (a + b).abs() >= CLEAR);
        let g = |t| {
            let t = DataId::new(t).unwrap_or_else(|_| panic!("invariant origin.trait_id"));
            gametes(genome, data, &t)
        };
        let face_names = [
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
        let m = &data.genetics().body;
        let pigment = g("core:pigment");
        let hair = pigment
            .zip(g("core:hair_darkness"))
            .map(|(p, h)| mixed(p, h, m.pigment_hair_slope, m.pigment_hair_residual));
        let locus = |name: DataId| data.genome_map().loci.iter().find(|l| l.id == name);
        let copies = |name: DataId| locus(name).map_or_else(Vec::new, |l| genome.copies(l));
        let (red, blue) = (copies(mc1r()), copies(herc2()));
        let count = |c: &[u8]| u8::try_from(c.iter().filter(|&&x| x == 1).count()).unwrap_or(0);
        let lines_of = |name: DataId, own: &[u8]| -> Vec<CopyFrom> {
            match (parents, locus(name)) {
                (Some([mother, father]), Some(l)) => {
                    copies_from(own, &mother.copies(l), &father.copies(l))
                }
                _ => Vec::new(),
            }
        };
        let mut lines = Vec::new();
        let two = |c: &[u8]| count(c) == 2 && !away;
        lines.push(OriginLine::Hair {
            word: looks.hair,
            source: if two(&red) { Source::Copies } else { by(hair) },
        });
        if !away && count(&red) > 0 {
            lines.push(OriginLine::RedCopies {
                copies: lines_of(mc1r(), &red),
            });
        }
        lines.push(OriginLine::Eyes {
            word: looks.eyes,
            source: if two(&blue) {
                Source::Copies
            } else {
                by(pigment)
            },
        });
        if !away && count(&blue) > 0 {
            lines.push(OriginLine::BlueCopies {
                copies: lines_of(herc2(), &blue),
            });
        }
        lines.push(OriginLine::Skin {
            word: looks.skin,
            source: by(pigment),
        });
        let growth = g("core:growth");
        if clear(growth) {
            lines.push(OriginLine::Height {
                word: looks.height,
                source: by(growth),
            });
        }
        let curl = g("core:curl");
        if clear(curl) {
            lines.push(OriginLine::Curls {
                word: looks.curls,
                source: by(curl),
            });
        }
        let freckle = g("core:freckle")
            .zip(pigment)
            .map(|(f, p)| mixed(f, p, m.freckle_slope, m.freckle_pigment));
        if clear(freckle) {
            lines.push(OriginLine::Freckles {
                word: looks.freckles,
                source: by(freckle),
            });
        } else if looks.freckles != Freckles::None && count(&red) > 0 {
            lines.push(OriginLine::FrecklesRed {
                word: looks.freckles,
                copies: count(&red),
            });
        }
        // Up to three face axes with |f| at least 1.5, the largest first, the lower number on
        // a tie.
        let axes = [
            body.face_axis_01(),
            body.face_axis_02(),
            body.face_axis_03(),
            body.face_axis_04(),
            body.face_axis_05(),
            body.face_axis_06(),
            body.face_axis_07(),
            body.face_axis_08(),
            body.face_axis_09(),
            body.face_axis_10(),
            body.face_axis_11(),
            body.face_axis_12(),
        ];
        let faces: Vec<(u8, f64)> = (1..=12_u8).zip(axes).collect();
        let mut shown: Vec<(u8, f64)> = faces
            .iter()
            .copied()
            .filter(|(_, f)| f.abs() >= 1.5)
            .collect();
        shown.sort_by(|(n1, f1), (n2, f2)| f2.abs().total_cmp(&f1.abs()).then(n1.cmp(n2)));
        for (n, f) in shown.into_iter().take(3) {
            let face = g(face_names[usize::from(n - 1)]);
            lines.push(OriginLine::Face {
                axis: n,
                up: f > 0.0,
                source: by(face),
            });
        }
        if traits.cognition != 0 {
            lines.push(OriginLine::Smart {
                step: traits.cognition,
                source: by(g("core:cognition")),
            });
        }
        if traits.strength != 0 {
            lines.push(OriginLine::Strong {
                step: traits.strength,
                source: by(g("core:muscle")),
            });
        }
        if traits.beauty != 0 {
            let source = if away {
                Source::Away
            } else if traits.beauty > 0 {
                Source::Both
            } else {
                let sums = face_names
                    .iter()
                    .map(|&t| g(t))
                    .try_fold((0.0, 0.0), |(qm, qp), pair| {
                        pair.map(|(a, b)| (qm + a * a, qp + b * b))
                    });
                sums.map_or(Source::Away, |(qm, qp)| beauty_source(qm, qp))
            };
            lines.push(OriginLine::Beauty {
                step: traits.beauty,
                source,
            });
        }
        Some(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_follows_the_rule_of_round_43() {
        // |D| under 0.40: both; faintly when the larger part is under 0.25.
        assert_eq!(source(0.3, 0.05), Source::Both);
        assert_eq!(source(0.2, 0.1), Source::BothFaint);
        // A clear D: the parent with twice the other's part and at least 0.25.
        assert_eq!(source(0.5, 0.1), Source::Mother);
        assert_eq!(source(0.1, 0.5), Source::Father);
        assert_eq!(source(0.5, 0.25), Source::Mother);
        assert_eq!(source(0.4, 0.3), Source::Both);
        // Down works the same with the signs turned.
        assert_eq!(source(-0.5, -0.1), Source::Mother);
        assert_eq!(source(-0.1, -0.6), Source::Father);
        // A parent against the direction counts as nothing: 0.6 and −0.1 give D 0.5.
        assert_eq!(source(0.6, -0.1), Source::Mother);
    }

    #[test]
    fn beauty_down_names_a_parent_from_1_5_and_twice_the_other() {
        assert_eq!(beauty_source(1.5, 0.75), Source::Mother);
        assert_eq!(beauty_source(1.49, 0.1), Source::BothFaint);
        assert_eq!(beauty_source(2.0, 1.1), Source::BothFaint);
        assert_eq!(beauty_source(0.2, 3.0), Source::Father);
    }

    #[test]
    fn a_copy_comes_from_a_carrying_parent_or_is_new() {
        assert_eq!(
            copies_from(&[1, 1], &[0, 1], &[1, 0]),
            [CopyFrom::Mother, CopyFrom::Father]
        );
        assert_eq!(
            copies_from(&[1, 1], &[0, 0], &[1, 1]),
            [CopyFrom::New, CopyFrom::Father]
        );
        assert_eq!(copies_from(&[0, 1], &[1, 1], &[0, 0]), [CopyFrom::New]);
        assert_eq!(copies_from(&[1, 0], &[1, 0], &[0, 0]), [CopyFrom::Mother]);
        assert_eq!(
            copies_from(&[0, 0], &[1, 1], &[1, 1]),
            Vec::<CopyFrom>::new()
        );
    }
}
