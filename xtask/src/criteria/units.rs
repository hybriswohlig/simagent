//! The units of the criteria (design note of the criteria kit, section 4): families and
//! their first-borns, and per check the unit list in canonical order with the values its
//! estimator reads and the anchor keys a report hashes (section 2: count and
//! `stable_hash64` over the anchors, pairs with both keys).
//!
//! Derived once (section 1): *town-born* iff `pool` ≠ 1; *left before t* iff `died` or
//! `emig` is set and below `t`; *reaches age a in the town* iff `born + a · YEAR ≤ end`,
//! the entry (`arrived` for an arrival, else `born`) is at most that second and the
//! person has not left before it.

use std::collections::{BTreeMap, BTreeSet};

use sim_math::i64_to_f64_exact;

use super::export::{Export, Row};
use super::stats::{MONTHS, mc1r_expectation, real};

/// Seconds of a year and of a month (spec 11.1: 13 months a year).
pub const YEAR: i64 = 31_449_600;
pub const MONTH: i64 = 2_419_200;
/// Entries of the life table: the ages 0 to 99 and the open interval from 100.
pub const AGES: usize = 101;
/// The decades of P4 in the town (G5): `k` = 0 to 17.
pub const DECADES: i64 = 18;

/// `stable_hash64` over the keys as 8-byte little-endian words in list order (section 2).
pub fn unit_hash(keys: &[u64]) -> u64 {
    let bytes: Vec<u8> = keys.iter().flat_map(|key| key.to_le_bytes()).collect();
    sim_core::hash::stable_hash64(&bytes)
}

/// The three traits of `z`, found in the header by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trait {
    Growth,
    Pigment,
    Cognition,
}

/// A unit list in canonical order: the values per unit and the anchor keys (one per
/// unit, two for a pair of persons).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Units<T> {
    pub values: Vec<T>,
    pub anchors: Vec<u64>,
}

impl<T> Units<T> {
    fn push(&mut self, value: T, anchors: &[u64]) {
        self.values.push(value);
        self.anchors.extend_from_slice(anchors);
    }
}

/// A family (K1): two rows as parents with their town-born children in birth order
/// (`born`, `mult`); the first is the first-born.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family {
    pub mother: u64,
    pub father: u64,
    pub children: Vec<u64>,
}

/// P9 on `core:mc1r`: the copies over the distinct parents of all families and `q`; per
/// family with an affected child `(y_f, E_f)` anchored at its earliest affected child,
/// that family's canonical index, and per family whether both parents are unaffected (the
/// twin takes the parents of other families). The sample is listed whatever `q`: the
/// report counts and hashes it also when `q` makes the check red.
#[derive(Clone, Debug, PartialEq)]
pub struct Mc1r {
    pub copies: u64,
    pub parents: u64,
    /// `None` without a family parent (no frequency to estimate).
    pub q: Option<f64>,
    pub units: Units<(bool, f64)>,
    pub families: Vec<usize>,
    pub unaffected: Vec<bool>,
}

impl Mc1r {
    /// Whether `q` lies in `[0.13, 0.17]` (K5); outside, the check is red with `q` and
    /// reads no expectation.
    pub fn in_band(&self) -> bool {
        self.q.is_some_and(|q| (0.13..=0.17).contains(&q))
    }
}

/// The village (G4, G5): the measured block `g*`, its units (`z_h`), `(g, N_m, N_f)` of the
/// blocks 0 to `g*` with births, ascending, and the persons alive at the end (no row
/// without `died` and `emig` makes the check red, as fewer than two units do).
#[derive(Clone, Debug, PartialEq)]
pub struct Village {
    pub block: i64,
    pub units: Units<f64>,
    pub blocks: Vec<(i64, u64, u64)>,
    pub living: u64,
}

/// The first partnerships of one sex: the histogram by month of age, the count without one
/// before 50, and the persons in ascending key.
#[derive(Clone, Debug, PartialEq)]
pub struct Partnerships {
    pub hist: [u64; MONTHS],
    pub never: u64,
    pub anchors: Vec<u64>,
}

/// The period life table's input of one sex: seconds lived and deaths by age, and as
/// anchors every row of the sex, ascending (section 4: "every row, per sex").
#[derive(Clone, Debug, PartialEq)]
pub struct LifeInput {
    pub exposure: [u64; AGES],
    pub deaths: [u64; AGES],
    pub anchors: Vec<u64>,
}

/// The `h²_T` sample of one sex (GK-2): `(T, gl)` per life ended in the town, and the
/// emigrated and the still living of the same births.
#[derive(Clone, Debug, PartialEq)]
pub struct Longevity {
    pub units: Units<(f64, f64)>,
    pub emigrated: u64,
    pub living: u64,
}

fn town_born(row: &Row) -> bool {
    row.pool != 1
}

/// The second a person enters the town.
fn entry(row: &Row) -> i64 {
    match (row.pool, row.arrived) {
        (1, Some(arrived)) => arrived,
        _ => row.born,
    }
}

/// Also decides whether a death is one in the town: a death after emigrating is not (the
/// export never sets both; the rule keeps the reading explicit).
fn left_before(row: &Row, t: i64) -> bool {
    row.died.is_some_and(|died| died < t) || row.emig.is_some_and(|emig| emig < t)
}

/// A span of seconds in years, the seconds converted exactly.
fn years(seconds: i64) -> f64 {
    let exact = i64_to_f64_exact(seconds)
        .unwrap_or_else(|| panic!("invariant criteria.seconds: {seconds}"));
    exact / 31_449_600.0
}

fn tally(n: usize) -> u64 {
    u64::try_from(n).unwrap_or_else(|_| panic!("invariant criteria.count"))
}

/// An export with its rows by key, its families and the places of its traits in `z`.
pub struct Town<'a> {
    export: &'a Export,
    by_key: BTreeMap<u64, usize>,
    families: Vec<Family>,
    traits: [usize; 3],
}

impl<'a> Town<'a> {
    /// Reads the families of an export as `export::read` returns it. Fails if a trait is
    /// missing, an arrival has no arrival second, a birth in the town (`born ≥ 0`) has no
    /// mother among the rows or a partner of a couple is not a row.
    pub fn new(export: &'a Export) -> Result<Town<'a>, String> {
        let by_key: BTreeMap<u64, usize> = (export.rows.iter().enumerate())
            .map(|(at, row)| (row.key, at))
            .collect();
        let place = |name: &str| {
            (export.header.traits.iter())
                .position(|(trait_name, _)| trait_name == name)
                .ok_or_else(|| format!("the export has no trait {name}"))
        };
        let traits = [
            place("core:growth")?,
            place("core:pigment")?,
            place("core:cognition")?,
        ];
        let is_row = |key: Option<u64>| key.is_some_and(|key| by_key.contains_key(&key));
        for row in &export.rows {
            if row.pool == 1 && row.arrived.is_none() {
                return Err(format!("arrival {:016x} has no arrival second", row.key));
            }
            if town_born(row) && row.born >= 0 && !is_row(row.mother) {
                return Err(format!(
                    "{:016x} is born in the town without a mother among the rows",
                    row.key
                ));
            }
        }
        if let Some(c) =
            (export.couples.iter()).find(|c| !is_row(Some(c.woman)) || !is_row(Some(c.man)))
        {
            return Err(format!(
                "couple {:016x} and {:016x}: a partner is not a row",
                c.woman, c.man
            ));
        }
        let mut groups: BTreeMap<(u64, u64), Vec<&Row>> = BTreeMap::new();
        for row in export.rows.iter().filter(|row| town_born(row)) {
            if let (Some(mother), Some(father)) = (row.mother, row.father)
                && is_row(row.mother)
                && is_row(row.father)
            {
                groups.entry((mother, father)).or_default().push(row);
            }
        }
        let mut families: Vec<Family> = (groups.into_iter())
            .map(|((mother, father), mut children)| {
                children.sort_by_key(|child| (child.born, child.mult));
                let children = children.iter().map(|child| child.key).collect();
                Family {
                    mother,
                    father,
                    children,
                }
            })
            .collect();
        families.sort_by_key(|family| family.children[0]);
        Ok(Town {
            export,
            by_key,
            families,
            traits,
        })
    }

    /// The families in canonical order: ascending first-born key.
    pub fn families(&self) -> &[Family] {
        &self.families
    }

    fn rows(&self) -> std::slice::Iter<'a, Row> {
        self.export.rows.iter()
    }

    fn row(&self, key: u64) -> &'a Row {
        let at =
            (self.by_key.get(&key)).unwrap_or_else(|| panic!("invariant criteria.row: {key:016x}"));
        &self.export.rows[*at]
    }

    fn z(&self, key: u64, of: Trait) -> f64 {
        let place = match of {
            Trait::Growth => self.traits[0],
            Trait::Pigment => self.traits[1],
            Trait::Cognition => self.traits[2],
        };
        self.row(key).z[place]
    }

    fn reaches(&self, row: &Row, age: i64) -> bool {
        let at = row.born + age * YEAR;
        at <= self.export.header.end && entry(row) <= at && !left_before(row, at)
    }

    /// Pairs of persons with their values of `of` and both keys as anchors.
    fn pairs(&self, pairs: &[(u64, u64)], of: Trait) -> Units<(f64, f64)> {
        let mut units = Units::default();
        for &(a, b) in pairs {
            units.push((self.z(a, of), self.z(b, of)), &[a, b]);
        }
        units
    }

    /// P3: per family the midparent `(z_h(mother) + z_h(father)) / 2` and `z_h` of the
    /// first-born, anchored at the first-born (it names the family).
    pub fn p3(&self) -> Units<(f64, f64)> {
        let mut units = Units::default();
        for family in &self.families {
            let first = family.children[0];
            let mid =
                (self.z(family.mother, Trait::Growth) + self.z(family.father, Trait::Growth)) / 2.0;
            units.push((mid, self.z(first, Trait::Growth)), &[first]);
        }
        units
    }

    /// P5 siblings: the first-born and the first later child of another genome key (an MZ
    /// co-twin is skipped); families of one genome key give no pair.
    pub fn p5_siblings(&self) -> Units<(f64, f64)> {
        let pairs: Vec<(u64, u64)> = (self.families.iter())
            .filter_map(|family| {
                let first = family.children[0];
                let zygote = self.row(first).gkey;
                (family.children[1..].iter())
                    .find(|child| self.row(**child).gkey != zygote)
                    .map(|second| (first, *second))
            })
            .collect();
        self.pairs(&pairs, Trait::Growth)
    }

    /// `A(c)`: the non-null keys of `c`'s parents and of the parents on their rows.
    fn ancestors(&self, key: u64) -> Vec<u64> {
        let row = self.row(key);
        let mut keys = Vec::new();
        for parent in [row.mother, row.father].into_iter().flatten() {
            keys.push(parent);
            if let Some(up) = self.by_key.get(&parent).map(|at| &self.export.rows[*at]) {
                keys.extend([up.mother, up.father].into_iter().flatten());
            }
        }
        keys
    }

    /// P5 strangers: one pending first-born walks the families in canonical order; a
    /// first-born sharing no key of `A` with it is emitted with it, else it is dropped.
    pub fn p5_strangers(&self) -> Units<(f64, f64)> {
        let mut pairs = Vec::new();
        let mut pending: Option<(u64, Vec<u64>)> = None;
        for family in &self.families {
            let first = family.children[0];
            let ancestors = self.ancestors(first);
            pending = match pending {
                None => Some((first, ancestors)),
                Some((kept, shared)) if shared.iter().all(|key| !ancestors.contains(key)) => {
                    pairs.push((kept, first));
                    None
                }
                kept => kept,
            };
        }
        self.pairs(&pairs, Trait::Growth)
    }

    /// P9 polygenic: family `i` pairs its mother (even `i`) or father (odd `i`) with the
    /// first-born, on `z_pig`.
    pub fn p9_polygenic(&self) -> Units<(f64, f64)> {
        let pairs: Vec<(u64, u64)> = (self.families.iter().enumerate())
            .map(|(i, family)| ([family.mother, family.father][i % 2], family.children[0]))
            .collect();
        self.pairs(&pairs, Trait::Pigment)
    }

    /// P9 on `core:mc1r` (K5, GK-3, GK-4): `q` over the distinct parents of all families;
    /// per family with an affected child `y_f` (both parents unaffected) and `E_f` at its
    /// distinct genome keys.
    pub fn p9_mc1r(&self) -> Mc1r {
        let parents: BTreeSet<u64> = (self.families.iter())
            .flat_map(|family| [family.mother, family.father])
            .collect();
        let copies: u64 = parents
            .iter()
            .map(|key| u64::from(self.row(*key).mc1r))
            .sum();
        let parents = tally(parents.len());
        let q = (parents > 0).then(|| real(copies) / real(2 * parents));
        let unaffected: Vec<bool> = (self.families.iter())
            .map(|family| self.row(family.mother).mc1r < 2 && self.row(family.father).mc1r < 2)
            .collect();
        let (mut units, mut families) = (Units::default(), Vec::new());
        for (i, family) in self.families.iter().enumerate() {
            let affected = family
                .children
                .iter()
                .find(|child| self.row(**child).mc1r == 2);
            if let Some(&child) = affected {
                let zygotes: BTreeSet<u64> = (family.children.iter())
                    .map(|child| self.row(*child).gkey)
                    .collect();
                let n = u32::try_from(zygotes.len())
                    .unwrap_or_else(|_| panic!("invariant criteria.zygotes"));
                units.push((unaffected[i], mc1r_expectation(n)), &[child]);
                families.push(i);
            }
        }
        Mc1r {
            copies,
            parents,
            q,
            units,
            families,
            unaffected,
        }
    }

    /// G5: per mother her first town-born child born in `[from, to)` (and `born ≥ 0`) by
    /// (`born`, `mult`); a unit iff it reaches 18 in the town (no move-up), ascending key,
    /// with `z_h`.
    fn firsts(&self, from: i64, to: i64) -> Units<f64> {
        let mut first: BTreeMap<u64, &Row> = BTreeMap::new();
        let born_in =
            |row: &&Row| town_born(row) && 0 <= row.born && from <= row.born && row.born < to;
        for row in self.rows().filter(born_in) {
            if let Some(mother) = row.mother {
                let kept = first.entry(mother).or_insert(row);
                if (row.born, row.mult) < (kept.born, kept.mult) {
                    *kept = row;
                }
            }
        }
        let mut keys: Vec<u64> = (first.values())
            .filter(|row| self.reaches(row, 18))
            .map(|row| row.key)
            .collect();
        keys.sort_unstable();
        let mut units = Units::default();
        for key in keys {
            units.push(self.z(key, Trait::Growth), &[key]);
        }
        units
    }

    /// P4 in the town: the units of decade `k`, `[10k · YEAR, (10k + 10) · YEAR)`.
    pub fn p4_city(&self, decade: i64) -> Units<f64> {
        self.firsts(10 * decade * YEAR, (10 * decade + 10) * YEAR)
    }

    /// The measured block of a village of `E` = `(end + 1) / YEAR` years, `g* = (E − 18)
    /// div 30 − 1`; `None` if the run does not end at a year end (`E` not whole, the
    /// reference's assertion) or `E − 18 < 30` (where a truncating and a flooring division
    /// would disagree, the reference's rule).
    pub fn village_block(&self) -> Option<i64> {
        let after = self.export.header.end + 1;
        let years = after.div_euclid(YEAR);
        (after.rem_euclid(YEAR) == 0 && years - 18 >= 30).then(|| (years - 18) / 30 - 1)
    }

    /// `(g, N_m, N_f)` of the blocks `g` = 0 to `block` with births, ascending: the
    /// distinct father and mother keys of the town-born rows born in each (the reference's
    /// reading: an arrival is not a birth of the village).
    pub fn parent_blocks(&self, block: i64) -> Vec<(i64, u64, u64)> {
        let span = 30 * YEAR;
        let mut parents: BTreeMap<i64, (BTreeSet<u64>, BTreeSet<u64>)> = BTreeMap::new();
        for row in self.rows().filter(|row| row.pool != 1) {
            if 0 <= row.born && row.born < (block + 1) * span {
                let (fathers, mothers) = parents.entry(row.born.div_euclid(span)).or_default();
                fathers.extend(row.father);
                mothers.extend(row.mother);
            }
        }
        (parents.iter())
            .map(|(g, (fathers, mothers))| (*g, tally(fathers.len()), tally(mothers.len())))
            .collect()
    }

    /// The village: block `g*`, its units as the town's, the blocks' parents and the
    /// living at the end; `None` without a measured block.
    pub fn village(&self) -> Option<Village> {
        let span = 30 * YEAR;
        let block = self.village_block()?;
        let living = (self.rows())
            .filter(|row| row.died.is_none() && row.emig.is_none())
            .count();
        Some(Village {
            block,
            units: self.firsts(block * span, (block + 1) * span),
            blocks: self.parent_blocks(block),
            living: tally(living),
        })
    }

    /// The earliest couple (its index) of each woman and of each man.
    fn earliest(&self) -> [BTreeMap<u64, usize>; 2] {
        let (mut women, mut men) = (BTreeMap::new(), BTreeMap::new());
        for (i, couple) in self.export.couples.iter().enumerate() {
            women.entry(couple.woman).or_insert(i);
            men.entry(couple.man).or_insert(i);
        }
        [women, men]
    }

    /// F11 pair correlations (GK-1): couples not carried of two town-born partners, each
    /// partner's earliest couple, as (woman, man) in couple order.
    pub fn couple_pairs(&self, of: Trait) -> Units<(f64, f64)> {
        let [women, men] = self.earliest();
        let pairs: Vec<(u64, u64)> = (self.export.couples.iter().enumerate())
            .filter(|(i, c)| {
                !c.carried
                    && town_born(self.row(c.woman))
                    && town_born(self.row(c.man))
                    && women.get(&c.woman) == Some(i)
                    && men.get(&c.man) == Some(i)
            })
            .map(|(_, c)| (c.woman, c.man))
            .collect();
        self.pairs(&pairs, of)
    }

    /// F11 median and never share: town-born persons of one sex reaching 50 in the town,
    /// the month of age of their earliest couple; from month 650 on, or without a couple,
    /// never.
    pub fn first_partnerships(&self, female: bool) -> Partnerships {
        let [women, men] = self.earliest();
        let earliest = if female { women } else { men };
        let (mut hist, mut never, mut anchors) = ([0; MONTHS], 0, Vec::new());
        let sample = (self.rows())
            .filter(|row| row.female == female && town_born(row) && self.reaches(row, 50));
        for row in sample {
            anchors.push(row.key);
            let month = (earliest.get(&row.key))
                .map(|i| (self.export.couples[*i].formed - row.born).div_euclid(MONTH))
                .and_then(|month| usize::try_from(month).ok())
                .filter(|month| *month < MONTHS);
            match month {
                Some(month) => hist[month] += 1,
                None => never += 1,
            }
        }
        Partnerships {
            hist,
            never,
            anchors,
        }
    }

    /// F11 births: every woman reaching 50 in the town, `brought` plus her town-born
    /// children born before her 50th birthday, ascending key.
    pub fn births(&self) -> Units<u64> {
        let mut children: BTreeMap<u64, Vec<i64>> = BTreeMap::new();
        for row in self.rows().filter(|row| town_born(row)) {
            if let Some(mother) = row.mother {
                children.entry(mother).or_default().push(row.born);
            }
        }
        let mut units = Units::default();
        for row in self
            .rows()
            .filter(|row| row.female && self.reaches(row, 50))
        {
            let before = row.born + 50 * YEAR;
            let town = (children.get(&row.key))
                .map_or(0, |born| born.iter().filter(|born| **born < before).count());
            units.push(row.brought + tally(town), &[row.key]);
        }
        units
    }

    /// F11 early births: every town-born person with `born ≥ 0`, whether its mother was
    /// under 18 at the birth, ascending key.
    pub fn early_births(&self) -> Units<bool> {
        let mut units = Units::default();
        for row in self.rows().filter(|row| town_born(row) && row.born >= 0) {
            if let Some(mother) = row.mother {
                units.push(row.born - self.row(mother).born < 18 * YEAR, &[row.key]);
            }
        }
        units
    }

    /// F11 age gap: per couple not carried `(born(woman) − born(man)) / YEAR`, in couple
    /// order.
    pub fn age_gap(&self) -> Units<f64> {
        let mut units = Units::default();
        for c in self.export.couples.iter().filter(|c| !c.carried) {
            let gap = years(self.row(c.woman).born - self.row(c.man).born);
            units.push(gap, &[c.woman, c.man]);
        }
        units
    }

    /// The period life table's input of one sex (section 3): the seconds in the town within
    /// `[100 · YEAR, 200 · YEAR)` by age (from 100 into the open interval), from the entry
    /// to `died`, `emig` or `end + 1`; the deaths in the period by age at death.
    pub fn life_table(&self, female: bool) -> LifeInput {
        let (start, stop) = (100 * YEAR, 200 * YEAR);
        let open = |age: i64| {
            usize::try_from(age.min(100)).unwrap_or_else(|_| panic!("invariant criteria.age"))
        };
        let mut table = LifeInput {
            exposure: [0; AGES],
            deaths: [0; AGES],
            anchors: Vec::new(),
        };
        for row in self.rows().filter(|row| row.female == female) {
            table.anchors.push(row.key);
            let exit = [row.died, row.emig]
                .into_iter()
                .flatten()
                .fold(self.export.header.end + 1, i64::min);
            let (mut t, to) = (entry(row).max(start), exit.min(stop));
            while t < to {
                let age = (t - row.born).div_euclid(YEAR);
                let next = (row.born + (age + 1) * YEAR).min(to);
                table.exposure[open(age)] +=
                    u64::try_from(next - t).unwrap_or_else(|_| panic!("invariant criteria.span"));
                t = next;
            }
            let died = row
                .died
                .filter(|died| (start..stop).contains(died) && !left_before(row, *died));
            if let Some(died) = died {
                table.deaths[open((died - row.born).div_euclid(YEAR))] += 1;
            }
        }
        table
    }

    /// The `h²_T` sample of one sex (GK-2): town-born persons born in `[0, 90 · YEAR)` who
    /// died in the town, `T = (died − born) / YEAR` with `gl`; the emigrated and the still
    /// living counted.
    pub fn longevity(&self, female: bool) -> Longevity {
        let mut sample = Longevity {
            units: Units::default(),
            emigrated: 0,
            living: 0,
        };
        let births = self.rows().filter(|row| {
            row.female == female && town_born(row) && (0..90 * YEAR).contains(&row.born)
        });
        for row in births {
            match (row.died, row.emig) {
                (Some(died), _) if !left_before(row, died) => {
                    sample
                        .units
                        .push((years(died - row.born), row.gl), &[row.key]);
                }
                (_, Some(_)) => sample.emigrated += 1,
                _ => sample.living += 1,
            }
        }
        sample
    }
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "units_reference_tests.rs"]
mod reference_tests;
