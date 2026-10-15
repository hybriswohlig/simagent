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
//! # Depth
//!
//! A query `φ(x, y)` knows only `x`, `y` and their ancestors up to the great-grandparents
//! (`DEPTH`, ADR-0020 section 4, Grok's H2): a parent outside that set counts as unknown
//! and adds 0, each parent on its own; whoever stands above counts as a founder in that
//! query. The depth belongs to the pair of the query, not to each step of the recursion.
//! A path of the recursion counts iff every person on it is in the set. Where every
//! ancestor is reached by one route only, a path from `x` up to a common ancestor at depth
//! `dx` and down to `y` from depth `dy` counts iff `max(dx, dy) ≤ 4` and `min(dx, dy) ≤ 3`
//! (Grok's H2b): exact are `φ ≥ 1/8`, first cousins (1/16), second cousins (1/64), second
//! cousins once removed (1/128), a great-great-grandparent (1/32) and `F` = 1/16 of a child
//! of first cousins; third cousins and a great-great-great-grandparent become 0, and so
//! does `F` of a child of second cousins. In an inbred pedigree a person can be in the set
//! by a short route and still close a long path, which then counts.
//!
//! The recursion works on any key `K` (a `SimId` in the tests, a lineage key in the world,
//! where persons of the pool and their records have no `SimId`, ADR-0020 section 4). Each
//! person has an order `(class, second)`: the world puts everyone with a birth in the pool
//! run (class 0, its second there) before everyone born in the town (class 1). "Younger"
//! is a total order: the greater `(order, key)`; the key only breaks ties (twins, founders
//! created together), it does not decide the result: φ is symmetric, and the recursion is
//! valid in every order that puts parents before their children, because then the
//! expanded person is never an ancestor of the other one. Two persons of the same order
//! are not ancestors of each other, so either may be expanded. Every parent must come
//! strictly before its child in `order`; a lookup that breaks this is the invariant
//! `kinship.parent_not_older`. That check also bounds the work: the younger person of a
//! pair gets strictly older with every step, so the recursion can never return to a pair
//! it is working on.
//!
//! # Numbers
//!
//! Every value is a dyadic rational, built from 0 and 1/2 by sums and halvings. Each step
//! of the recursion replaces the younger person of a pair by a parent, so the younger
//! person of the pairs along a chain gets strictly older; a query has at most 30 persons,
//! so a chain has at most 30 pairs and a value at most 30 halvings below its leaves. Every
//! value of a query is then a multiple of `2^-31` in `[0, 1]`, and the sum before a halving
//! a multiple of `2^-30` in `[0, 2]`: exact in an f64 (53 bits).
//!
//! A value is the same bit for bit however it is evaluated: every pair of a query has
//! exactly one expression (the younger one is expanded, the sum of the mother's and the
//! father's term is commutative in IEEE 754). The working memory of a query is never part
//! of the state, its hash or a save.
//!
//! # Evaluation
//!
//! No recursion on the call stack: an explicit stack holds the pairs that wait for a
//! term, one per step along the current chain, so it never holds more than 30 pairs (see
//! Numbers). Each pair is
//! computed once per query, so a query costs at most one evaluation per pair of its set
//! (at most 30 persons: two and 2 + 4 + 8 ancestors each), and it reads no one outside the
//! set, however deep the pedigree. Nothing carries over from one query to the next. All
//! maps are `BTreeMap` (ADR-0003).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::time::SimTime;

/// How far up a query looks (genetics specification 2.7, ADR-0020 section 4, Grok's H2):
/// the ancestors up to the great-grandparents of each of the two persons. Whoever stands
/// above counts as a founder in that query.
pub const DEPTH: u32 = 3;

/// What the recursion needs to know about one person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kin<K> {
    /// `(class, second of birth)`: the greater one is the younger person.
    pub order: (u8, SimTime),
    /// The mother and the father, each on its own; `None` for one that is not known.
    pub mother: Option<K>,
    pub father: Option<K>,
}

/// A person as one query sees it: its parents only if they belong to the query.
#[derive(Clone, Copy)]
struct Seen<K> {
    order: (u8, SimTime),
    mother: Option<K>,
    father: Option<K>,
}

/// A pair of persons, the smaller key first: φ is symmetric.
type Pair<K> = (K, K);

fn pair<K: Ord>(a: K, b: K) -> Pair<K> {
    if a <= b { (a, b) } else { (b, a) }
}

/// The working memory of one query: the values of the pairs it needed. Each query starts
/// it empty: what a pair is worth depends on the query's ancestors (Grok's H2), so no
/// value carries over, and a long run holds no more than one query's pairs. A caller may
/// keep one to reuse its allocation.
#[derive(Clone, Debug)]
pub struct KinshipCache<K> {
    values: BTreeMap<Pair<K>, f64>,
}

impl<K> Default for KinshipCache<K> {
    fn default() -> KinshipCache<K> {
        KinshipCache {
            values: BTreeMap::new(),
        }
    }
}

/// How φ of a pair follows from other pairs.
#[derive(Clone, Copy)]
enum Rule<K> {
    Known(f64),
    /// `(1 + φ(parents)) / 2`: a person with itself whose two parents are known.
    Inbred(Pair<K>),
    /// `(φ(first) + φ(second)) / 2`: the younger person's mother and father with the
    /// older one; a parent the query does not know adds 0.
    Mean(Option<Pair<K>>, Option<Pair<K>>),
}

/// The persons of the query of `x` and `y`: the two and every ancestor of either up to
/// `DEPTH` generations.
fn query_set<K: Ord + Copy>(x: K, y: K, lookup: &impl Fn(K) -> Kin<K>) -> BTreeSet<K> {
    let mut set = BTreeSet::new();
    for start in [x, y] {
        set.insert(start);
        let mut generation = BTreeSet::from([start]);
        for _ in 0..DEPTH {
            let mut next = BTreeSet::new();
            for person in generation {
                let kin = lookup(person);
                next.extend(kin.mother);
                next.extend(kin.father);
            }
            set.extend(next.iter().copied());
            generation = next;
        }
    }
    set
}

/// `person` as the query sees it.
///
/// # Panics
///
/// Invariant `kinship.parent_not_older`: a parent the query knows does not come before its
/// child in `order`.
fn seen<K: Ord + Copy + Debug>(
    person: K,
    set: &BTreeSet<K>,
    lookup: &impl Fn(K) -> Kin<K>,
) -> Seen<K> {
    let kin = lookup(person);
    let known = |parent: K| {
        set.contains(&parent).then(|| {
            assert!(
                lookup(parent).order < kin.order,
                "invariant kinship.parent_not_older: {person:?} parent {parent:?}"
            );
            parent
        })
    };
    Seen {
        order: kin.order,
        mother: kin.mother.and_then(known),
        father: kin.father.and_then(known),
    }
}

fn rule<K: Ord + Copy>((a, b): Pair<K>, see: &impl Fn(K) -> Seen<K>) -> Rule<K> {
    let seen_a = see(a);
    if a == b {
        return match (seen_a.mother, seen_a.father) {
            (Some(mother), Some(father)) => Rule::Inbred(pair(mother, father)),
            _ => Rule::Known(0.5),
        };
    }
    let seen_b = see(b);
    let (younger, older) = if (seen_a.order, a) > (seen_b.order, b) {
        (seen_a, b)
    } else {
        (seen_b, a)
    };
    match (younger.mother, younger.father) {
        (None, None) => Rule::Known(0.0),
        (mother, father) => Rule::Mean(
            mother.map(|mother| pair(mother, older)),
            father.map(|father| pair(father, older)),
        ),
    }
}

impl<K: Ord + Copy + Debug> KinshipCache<K> {
    pub const fn new() -> KinshipCache<K> {
        KinshipCache {
            values: BTreeMap::new(),
        }
    }

    /// φ of `x` and `y` over the ancestors up to the great-grandparents of the two
    /// (`DEPTH`); `lookup` gives order and parents of every person of their ancestry.
    /// Bit for bit the same as [`phi`].
    ///
    /// # Panics
    ///
    /// Invariant `kinship.parent_not_older` if `lookup` gives a parent in the query that
    /// does not come before its child; whatever `lookup` itself panics with for an unknown
    /// key.
    pub fn phi(&mut self, x: K, y: K, lookup: impl Fn(K) -> Kin<K>) -> f64 {
        self.values.clear();
        let set = query_set(x, y, &lookup);
        let see = |person: K| seen(person, &set, &lookup);
        let first = pair(x, y);
        let mut current = (first, rule(first, &see));
        // The pairs whose rule waits for a term, with that rule, one per step along the
        // chain that leads to `current`.
        let mut waiting: Vec<(Pair<K>, Rule<K>)> = Vec::new();
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
                    current = (missing, rule(missing, &see));
                }
            }
        }
    }

    /// The value of `rule` if the memory holds its terms, else the first term missing.
    fn apply(&self, rule: Rule<K>) -> Result<f64, Pair<K>> {
        let term = |of: Pair<K>| self.values.get(&of).copied().ok_or(of);
        let optional = |of: Option<Pair<K>>| of.map_or(Ok(0.0), term);
        match rule {
            Rule::Known(value) => Ok(value),
            Rule::Inbred(parents) => Ok((1.0 + term(parents)?) / 2.0),
            Rule::Mean(first, second) => Ok((optional(first)? + optional(second)?) / 2.0),
        }
    }
}

/// φ of `x` and `y` with a memory of its own for this one query.
///
/// # Panics
///
/// As [`KinshipCache::phi`].
pub fn phi<K: Ord + Copy + Debug>(x: K, y: K, lookup: impl Fn(K) -> Kin<K>) -> f64 {
    KinshipCache::new().phi(x, y, lookup)
}
