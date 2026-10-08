//! The kinship coefficient φ (genetics specification 2.7; Falconer and Mackay 1996,
//! chapter 5): the probability that an allele drawn at random from `x` and one drawn at
//! the same locus from `y` are identical by descent. The partner market excludes
//! `φ ≥ 1/8` and lowers first cousins (`φ = 1/16`); the orphan rule reads it later.
//!
//! # Recursion
//!
//! `φ_xx = (1 + F_x) / 2` with `F_x = φ(mother_x, father_x)`; for `x ≠ y`
//! `φ_xy = (φ(mother_x, y) + φ(father_x, y)) / 2`, where `x` is the younger of the two.
//! A founder has no known parents: expanded, it gives 0, so founders are unrelated to
//! each other and have `F = 0`.
//!
//! "Younger" is a total order: the later second of birth, and in the same second (twins,
//! founders created together) the larger `SimId`. The ID only breaks ties, it does not
//! decide the result: φ is symmetric, and the recursion is valid in every order that puts
//! parents before their children, because then the expanded person is never an ancestor
//! of the other one. Two persons of the same second are not ancestors of each other, so
//! either may be expanded. Every parent must be born before its child (in an earlier
//! second); a lookup that breaks this is the invariant `kinship.parent_not_older`. That
//! check also bounds the work: the younger person of a pair gets strictly older with
//! every step, so the recursion can never return to a pair it is working on.
//!
//! # Numbers
//!
//! Every value is a dyadic rational, built from 0, 1/2 and 1 by sums and halvings. With
//! `g` the generation of a person (0 for a founder, else one more than the larger of its
//! parents' generations), `φ_xy` is by induction a multiple of `2^-(g_x + g_y + 1)` in
//! `[0, 1]`, and the sum before a halving is a multiple of `2^-(g_x + g_y)` in `[0, 2]`.
//! Both fit into the 53 bits of an f64 while `g_x + g_y ≤ 52`: every value of a pedigree
//! up to 26 generations deep is exact. Deeper pedigrees stay exact unless one value mixes
//! a close relationship with a loop more than 52 halvings smaller; then that remote
//! contribution is rounded to nearest, far below the thresholds 1/16 and 1/8. The
//! exponent range is no limit before about 500 generations (`2^-1022`, the smallest
//! normal f64).
//!
//! Exact or rounded, a value is the same bit for bit with and without a cache: every pair
//! has exactly one expression (the younger one is expanded, the sum of the mother's and
//! the father's term is commutative in IEEE 754), and a cached value is the value of that
//! same expression. So a cache is never part of the state, its hash or a save.
//!
//! # Evaluation
//!
//! No recursion on the call stack: an explicit stack holds the pairs that wait for a
//! term, one per step along the current chain. Every step lowers `g` of a pair by at
//! least one, so the stack never holds more than `g_x + g_y + 1` pairs; a line of
//! thousands of generations needs heap, not call stack (a recursive version overflows
//! the stack of a test thread at 5 000 generations in debug, 20 000 in release). Each
//! pair is looked up and computed once per cache and kept, so a query costs at most one
//! evaluation per pair of an ancestor of `x` (or `x`) and an ancestor of `y` (or `y`),
//! and far less when the cache already holds the pairs of the parents. All maps are
//! `BTreeMap` (ADR-0003).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::ids::SimId;
use crate::time::SimTime;

/// What the recursion needs to know about one person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kin {
    /// The second of birth: the later one is the younger person.
    pub born: SimTime,
    /// Mother and father, in this order; `None` for a founder.
    pub parents: Option<(SimId, SimId)>,
}

/// A pair of persons, the smaller ID first: φ is symmetric.
type Pair = (SimId, SimId);

fn pair(a: SimId, b: SimId) -> Pair {
    if a <= b { (a, b) } else { (b, a) }
}

/// The values φ computed so far, by pair. A caller may keep one for a world as long as it
/// likes: parents and births never change, so a value never goes stale. IDs are counters
/// of one world, so a cache serves one world only.
#[derive(Clone, Debug, Default)]
pub struct KinshipCache {
    values: BTreeMap<Pair, f64>,
}

/// How φ of a pair follows from other pairs.
#[derive(Clone, Copy)]
enum Rule {
    Known(f64),
    /// `(1 + φ(parents)) / 2`: a person with itself.
    Inbred(Pair),
    /// `(φ(first) + φ(second)) / 2`: the younger person's mother and father with the
    /// older one.
    Mean(Pair, Pair),
}

/// The parents of `sim`; `None` for a founder.
///
/// # Panics
///
/// Invariant `kinship.parent_not_older`: a parent is not born before its child.
fn parents_of(sim: SimId, kin: Kin, lookup: &impl Fn(SimId) -> Kin) -> Option<(SimId, SimId)> {
    let (mother, father) = kin.parents?;
    for parent in [mother, father] {
        assert!(
            lookup(parent).born < kin.born,
            "invariant kinship.parent_not_older: sim {} parent {}",
            sim.get(),
            parent.get()
        );
    }
    Some((mother, father))
}

fn rule((a, b): Pair, lookup: &impl Fn(SimId) -> Kin) -> Rule {
    let kin_a = lookup(a);
    if a == b {
        return match parents_of(a, kin_a, lookup) {
            None => Rule::Known(0.5),
            Some((mother, father)) => Rule::Inbred(pair(mother, father)),
        };
    }
    let kin_b = lookup(b);
    let (younger, kin, older) = if (kin_a.born, a) > (kin_b.born, b) {
        (a, kin_a, b)
    } else {
        (b, kin_b, a)
    };
    match parents_of(younger, kin, lookup) {
        None => Rule::Known(0.0),
        Some((mother, father)) => Rule::Mean(pair(mother, older), pair(father, older)),
    }
}

impl KinshipCache {
    pub const fn new() -> KinshipCache {
        KinshipCache {
            values: BTreeMap::new(),
        }
    }

    /// φ of `x` and `y`; `lookup` gives birth and parents of every person of their
    /// ancestry. Bit for bit the same as [`phi`] without a cache.
    ///
    /// # Panics
    ///
    /// Invariant `kinship.parent_not_older` if `lookup` gives a parent that is not born
    /// before its child; whatever `lookup` itself panics with for an unknown ID.
    pub fn phi(&mut self, x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
        let first = pair(x, y);
        if let Some(&value) = self.values.get(&first) {
            return value;
        }
        let mut current = (first, rule(first, &lookup));
        // The pairs whose rule waits for a term, with that rule, one per step along the
        // chain that leads to `current`.
        let mut waiting: Vec<(Pair, Rule)> = Vec::new();
        loop {
            let (at, rule_at) = current;
            match self.apply(rule_at) {
                Ok(value) => {
                    self.values.insert(at, value);
                    match waiting.pop() {
                        Some(next) => current = next,
                        None => return value,
                    }
                }
                Err(missing) => {
                    waiting.push(current);
                    current = (missing, rule(missing, &lookup));
                }
            }
        }
    }

    /// The value of `rule` if the cache holds its terms, else the first term missing.
    fn apply(&self, rule: Rule) -> Result<f64, Pair> {
        let term = |of: Pair| self.values.get(&of).copied().ok_or(of);
        match rule {
            Rule::Known(value) => Ok(value),
            Rule::Inbred(parents) => Ok((1.0 + term(parents)?) / 2.0),
            Rule::Mean(first, second) => Ok((term(first)? + term(second)?) / 2.0),
        }
    }
}

/// φ of `x` and `y` with a cache of its own for this one query.
///
/// # Panics
///
/// As [`KinshipCache::phi`].
pub fn phi(x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
    KinshipCache::new().phi(x, y, lookup)
}
