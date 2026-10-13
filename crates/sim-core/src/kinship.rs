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

use crate::ids::SimId;
use crate::time::SimTime;

/// How far up a query looks (genetics specification 2.7, ADR-0020 section 4, Grok's H2):
/// the ancestors up to the great-grandparents of each of the two persons. Whoever stands
/// above counts as a founder in that query.
pub const DEPTH: u32 = 3;

/// What the recursion needs to know about one person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kin {
    /// The second of birth: the later one is the younger person.
    pub born: SimTime,
    /// Mother and father, in this order; `None` for a founder.
    pub parents: Option<(SimId, SimId)>,
}

/// A person as one query sees it: its parents only if they belong to the query.
#[derive(Clone, Copy)]
struct Seen {
    born: SimTime,
    mother: Option<SimId>,
    father: Option<SimId>,
}

/// A pair of persons, the smaller ID first: φ is symmetric.
type Pair = (SimId, SimId);

fn pair(a: SimId, b: SimId) -> Pair {
    if a <= b { (a, b) } else { (b, a) }
}

/// The working memory of one query: the values of the pairs it needed. Each query starts
/// it empty: what a pair is worth depends on the query's ancestors (Grok's H2), so no
/// value carries over, and a long run holds no more than one query's pairs. A caller may
/// keep one to reuse its allocation.
#[derive(Clone, Debug, Default)]
pub struct KinshipCache {
    values: BTreeMap<Pair, f64>,
}

/// How φ of a pair follows from other pairs.
#[derive(Clone, Copy)]
enum Rule {
    Known(f64),
    /// `(1 + φ(parents)) / 2`: a person with itself whose two parents are known.
    Inbred(Pair),
    /// `(φ(first) + φ(second)) / 2`: the younger person's mother and father with the
    /// older one; a parent the query does not know adds 0.
    Mean(Option<Pair>, Option<Pair>),
}

/// The persons of the query of `x` and `y`: the two and every ancestor of either up to
/// `DEPTH` generations.
fn query_set(x: SimId, y: SimId, lookup: &impl Fn(SimId) -> Kin) -> BTreeSet<SimId> {
    let mut set = BTreeSet::new();
    for start in [x, y] {
        set.insert(start);
        let mut generation = BTreeSet::from([start]);
        for _ in 0..DEPTH {
            let mut next = BTreeSet::new();
            for sim in generation {
                if let Some((mother, father)) = lookup(sim).parents {
                    next.insert(mother);
                    next.insert(father);
                }
            }
            set.extend(next.iter().copied());
            generation = next;
        }
    }
    set
}

/// `sim` as the query sees it.
///
/// # Panics
///
/// Invariant `kinship.parent_not_older`: a parent the query knows is not born before its
/// child.
fn seen(sim: SimId, set: &BTreeSet<SimId>, lookup: &impl Fn(SimId) -> Kin) -> Seen {
    let kin = lookup(sim);
    let known = |parent: SimId| {
        set.contains(&parent).then(|| {
            assert!(
                lookup(parent).born < kin.born,
                "invariant kinship.parent_not_older: sim {} parent {}",
                sim.get(),
                parent.get()
            );
            parent
        })
    };
    Seen {
        born: kin.born,
        mother: kin.parents.and_then(|(mother, _)| known(mother)),
        father: kin.parents.and_then(|(_, father)| known(father)),
    }
}

fn rule((a, b): Pair, see: &impl Fn(SimId) -> Seen) -> Rule {
    let seen_a = see(a);
    if a == b {
        return match (seen_a.mother, seen_a.father) {
            (Some(mother), Some(father)) => Rule::Inbred(pair(mother, father)),
            _ => Rule::Known(0.5),
        };
    }
    let seen_b = see(b);
    let (younger, older) = if (seen_a.born, a) > (seen_b.born, b) {
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

impl KinshipCache {
    pub const fn new() -> KinshipCache {
        KinshipCache {
            values: BTreeMap::new(),
        }
    }

    /// φ of `x` and `y` over the ancestors up to the great-grandparents of the two
    /// (`DEPTH`); `lookup` gives birth and parents of every person of their ancestry.
    /// Bit for bit the same as [`phi`].
    ///
    /// # Panics
    ///
    /// Invariant `kinship.parent_not_older` if `lookup` gives a parent in the query that
    /// is not born before its child; whatever `lookup` itself panics with for an unknown
    /// ID.
    pub fn phi(&mut self, x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
        self.values.clear();
        let set = query_set(x, y, &lookup);
        let see = |sim: SimId| seen(sim, &set, &lookup);
        let first = pair(x, y);
        let mut current = (first, rule(first, &see));
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
                    current = (missing, rule(missing, &see));
                }
            }
        }
    }

    /// The value of `rule` if the memory holds its terms, else the first term missing.
    fn apply(&self, rule: Rule) -> Result<f64, Pair> {
        let term = |of: Pair| self.values.get(&of).copied().ok_or(of);
        let optional = |of: Option<Pair>| of.map_or(Ok(0.0), term);
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
pub fn phi(x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
    KinshipCache::new().phi(x, y, lookup)
}
