//! Genetics specification 2.7: the kinship coefficient φ by its recursion, on a small
//! pedigree built by hand. It is the pedigree of `tools/spec_reference.py` (function
//! `kinships`) plus an uncle, double first cousins and a child of two first cousins.
//! The expected values are the table W2.7 or derived by hand in the comments, never
//! taken from a run.

use std::collections::BTreeMap;

use sim_core::ids::SimId;
use sim_core::kinship::{Kin, KinshipCache, phi};
use sim_core::time::SimTime;

/// Name, second of birth, mother and father.
type Person = (&'static str, i64, Option<(&'static str, &'static str)>);

/// Parents come before their children. The six founders share their second of birth,
/// and so do the twins `child` and `sib`: there the tie rule decides who counts as
/// younger.
const FAMILY: [Person; 17] = [
    ("gm", 0, None),
    ("gf", 0, None),
    ("gm2", 0, None),
    ("gf2", 0, None),
    ("x", 0, None),
    ("y", 0, None),
    ("mother", 100, Some(("gm", "gf"))),
    ("aunt", 110, Some(("gm", "gf"))),
    ("father", 105, Some(("gm2", "gf2"))),
    ("uncle", 120, Some(("gm2", "gf2"))),
    ("uncle_in_law", 101, Some(("x", "y"))),
    ("child", 200, Some(("mother", "father"))),
    ("sib", 200, Some(("mother", "father"))),
    ("half_sib", 210, Some(("mother", "y"))),
    ("cousin", 205, Some(("aunt", "uncle_in_law"))),
    ("double_cousin", 220, Some(("aunt", "uncle"))),
    ("inbred", 300, Some(("cousin", "child"))),
];

struct Tree {
    kin: BTreeMap<SimId, Kin>,
    ids: BTreeMap<&'static str, SimId>,
}

impl Tree {
    /// `FAMILY` with the ID `number(n)` for its n-th person (from 0).
    fn family(number: impl Fn(u64) -> u64) -> Tree {
        let mut tree = Tree {
            kin: BTreeMap::new(),
            ids: BTreeMap::new(),
        };
        for (n, (name, born, parents)) in (0_u64..).zip(FAMILY) {
            let id = SimId::try_from(number(n)).expect("id");
            let parents = parents.map(|(mother, father)| (tree.id(mother), tree.id(father)));
            let kin = Kin {
                born: SimTime::from_secs(born),
                parents,
            };
            assert!(tree.kin.insert(id, kin).is_none(), "{name}: id twice");
            tree.ids.insert(name, id);
        }
        tree
    }

    fn id(&self, name: &str) -> SimId {
        self.ids[name]
    }

    fn lookup(&self) -> impl Fn(SimId) -> Kin + '_ {
        |sim| self.kin[&sim]
    }

    fn phi(&self, a: &str, b: &str) -> f64 {
        phi(self.id(a), self.id(b), self.lookup())
    }

    /// All ordered pairs of persons, a person with itself included.
    fn pairs(&self) -> Vec<(SimId, SimId)> {
        let ids: Vec<SimId> = self.kin.keys().copied().collect();
        ids.iter()
            .flat_map(|&a| ids.iter().map(move |&b| (a, b)))
            .collect()
    }
}

fn ascending(n: u64) -> u64 {
    n + 1
}

/// φ of two named persons, in both orders, is `expected` bit for bit.
fn assert_phi(tree: &Tree, a: &str, b: &str, expected: f64) {
    for (x, y) in [(a, b), (b, a)] {
        let value = tree.phi(x, y);
        assert!(
            value.to_bits() == expected.to_bits(),
            "phi({x}, {y}) = {value}, expected {expected}"
        );
    }
}

#[test]
fn w2_7a_parent_and_child() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "mother", "child", 0.25);
    assert_phi(&tree, "father", "child", 0.25);
}

#[test]
fn w2_7b_full_siblings() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "child", "sib", 0.25);
}

#[test]
fn w2_7c_half_siblings() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "child", "half_sib", 0.125);
}

#[test]
fn w2_7d_grandparent_and_grandchild() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "gm", "child", 0.125);
    assert_phi(&tree, "gf2", "child", 0.125);
}

#[test]
fn w2_7e_aunt_or_uncle_and_niece_or_nephew() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "aunt", "child", 0.125);
    assert_phi(&tree, "uncle", "sib", 0.125);
}

#[test]
fn w2_7f_first_cousins() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "child", "cousin", 0.0625);
}

/// 2.7 below the table: a child of two first cousins has `F = φ(cousin, child) = 1/16`,
/// so `φ_xx = (1 + 1/16) / 2 = 17/32`. With that `F` a recessive allele of frequency
/// 0.02 affects `q² + F q (1 − q)` = 0.001625 [W2.7g] of such children, 4.06 [W2.7h]
/// times a child without inbreeding.
#[test]
fn w2_7g_child_of_first_cousins_has_f_one_sixteenth() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "inbred", "inbred", 17.0 / 32.0);
    let f = 2.0 * tree.phi("inbred", "inbred") - 1.0;
    let q = 0.02;
    let affected = q * q + f * q * (1.0 - q);
    assert!((affected - 0.001_625).abs() < 5e-7, "{affected}");
    assert!((affected / (q * q) - 4.06).abs() < 0.005, "{affected}");
}

/// 2.7: `φ_xx = (1 + F_x) / 2`, and a founder has no parents, so `F = 0`.
#[test]
fn w2_7_founder_with_itself_one_half() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "gm", "gm", 0.5);
    assert_phi(&tree, "y", "y", 0.5);
}

/// 2.7: founders are unrelated, and so are two persons without a common ancestor.
#[test]
fn w2_7_founders_unrelated() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "gm", "gf", 0.0);
    assert_phi(&tree, "x", "gm2", 0.0);
    assert_phi(&tree, "mother", "father", 0.0);
    assert_phi(&tree, "child", "uncle_in_law", 0.0);
}

/// Double first cousins (two sisters with two brothers, not in the table): the younger
/// one is `double_cousin`, and its parents are the aunt (the mother's sister) and the
/// uncle (the father's brother) of `child`, each related to `child` by 1/8 [W2.7e]. So
/// `φ = (1/8 + 1/8) / 2 = 1/8`, twice the 1/16 of single first cousins: two pairs of
/// common grandparents instead of one.
#[test]
fn w2_7_double_first_cousins_one_eighth() {
    let tree = Tree::family(ascending);
    assert_phi(&tree, "child", "double_cousin", 0.125);
    assert_phi(&tree, "sib", "double_cousin", 0.125);
}

#[test]
fn w2_7_symmetric_for_all_pairs() {
    let tree = Tree::family(ascending);
    for (a, b) in tree.pairs() {
        let ab = phi(a, b, tree.lookup());
        let ba = phi(b, a, tree.lookup());
        assert!(ab.to_bits() == ba.to_bits(), "{a:?} {b:?}: {ab} {ba}");
    }
}

/// A cache kept over many queries gives every value bit for bit as a fresh computation,
/// whatever order filled it.
#[test]
fn w2_7_cache_equals_no_cache() {
    let tree = Tree::family(ascending);
    let pairs = tree.pairs();
    let fresh: Vec<u64> = pairs
        .iter()
        .map(|&(a, b)| phi(a, b, tree.lookup()).to_bits())
        .collect();
    let mut forward = KinshipCache::new();
    let forward: Vec<u64> = pairs
        .iter()
        .map(|&(a, b)| forward.phi(a, b, tree.lookup()).to_bits())
        .collect();
    let mut backward = KinshipCache::default();
    let mut backward: Vec<u64> = pairs
        .iter()
        .rev()
        .map(|&(a, b)| backward.phi(a, b, tree.lookup()).to_bits())
        .collect();
    backward.reverse();
    // The deepest pair first, then everything again in order.
    let mut deepest_first = KinshipCache::new();
    let inbred = tree.id("inbred");
    deepest_first.phi(inbred, inbred, tree.lookup());
    let deepest_first: Vec<u64> = pairs
        .iter()
        .map(|&(a, b)| deepest_first.phi(a, b, tree.lookup()).to_bits())
        .collect();
    assert_eq!(forward, fresh);
    assert_eq!(backward, fresh);
    assert_eq!(deepest_first, fresh);
    assert!(fresh.iter().any(|&bits| bits != 0), "all zero");
}

/// A pair the cache holds, in either order, is answered from the cache alone: the lookup
/// is never called again.
#[test]
fn w2_7_cached_pair_needs_no_lookup() {
    let tree = Tree::family(ascending);
    let (child, cousin) = (tree.id("child"), tree.id("cousin"));
    let mut cache = KinshipCache::new();
    assert!(cache.phi(child, cousin, tree.lookup()).to_bits() == 0.0625_f64.to_bits());
    let again = cache.phi(cousin, child, |sim: SimId| -> Kin {
        panic!("lookup of {sim:?}")
    });
    assert!(again.to_bits() == 0.0625_f64.to_bits());
}

/// The IDs only break ties: with the IDs in the opposite order (children now below their
/// parents, the founders and the twins ordered the other way round) every value stays
/// the same bit for bit.
#[test]
fn w2_7_ids_do_not_change_phi() {
    let ascending = Tree::family(ascending);
    let descending = Tree::family(|n| 100 - n);
    for (a, _, _) in FAMILY {
        for (b, _, _) in FAMILY {
            let up = ascending.phi(a, b);
            let down = descending.phi(a, b);
            assert!(up.to_bits() == down.to_bits(), "{a} {b}: {up} {down}");
        }
    }
    assert_phi(&descending, "mother", "child", 0.25);
}

/// A line of 20 000 generations: each member is the child of the one before and of a
/// fresh founder. All founders are born in the first second, so they are older than
/// every member of the line, and φ of the last parent and child walks the whole line down
/// to its first founder (an implementation with one stack frame per generation overflows
/// the stack of a test thread). The partner of the last member is unrelated to the line
/// and its own parents are unrelated, so `φ = ((1 + 0) / 2 + 0) / 2 = 1/4`.
#[test]
fn w2_7a_deep_line_keeps_the_stack() {
    const GENERATIONS: u64 = 20_000;
    let id = |n: u64| SimId::try_from(n).expect("id");
    // The line: member g has ID 2g + 1 and is born in second g; founder partner g has ID
    // 2g and is born in second 0, like the first member.
    let line = |g: u64| id(2 * g + 1);
    let mut kin = BTreeMap::new();
    kin.insert(
        line(0),
        Kin {
            born: SimTime::from_secs(0),
            parents: None,
        },
    );
    for g in 1..=GENERATIONS {
        let partner = id(2 * g);
        kin.insert(
            partner,
            Kin {
                born: SimTime::from_secs(0),
                parents: None,
            },
        );
        kin.insert(
            line(g),
            Kin {
                born: SimTime::from_secs(i64::try_from(g).expect("small")),
                parents: Some((line(g - 1), partner)),
            },
        );
    }
    let lookup = |sim: SimId| kin[&sim];
    let parent = line(GENERATIONS - 1);
    let child = line(GENERATIONS);
    assert!(phi(parent, child, lookup).to_bits() == 0.25_f64.to_bits());
    assert!(phi(child, parent, lookup).to_bits() == 0.25_f64.to_bits());
    let mut cache = KinshipCache::new();
    assert!(cache.phi(child, child, lookup).to_bits() == 0.5_f64.to_bits());
    assert!(cache.phi(child, parent, lookup).to_bits() == 0.25_f64.to_bits());
}

/// φ of a child born in second 10 and its father born in second 0, whose mother is born
/// in second `mother_born`. The child is the younger one, so the recursion expands it.
fn phi_with_mother_born(mother_born: i64) -> f64 {
    let id = |n: u64| SimId::try_from(n).expect("id");
    let (mother, father, child) = (id(1), id(2), id(3));
    let founder = |born: i64| Kin {
        born: SimTime::from_secs(born),
        parents: None,
    };
    let kin = BTreeMap::from([
        (mother, founder(mother_born)),
        (father, founder(0)),
        (
            child,
            Kin {
                born: SimTime::from_secs(10),
                parents: Some((mother, father)),
            },
        ),
    ]);
    phi(child, father, |sim| kin[&sim])
}

/// The recursion needs every parent older than its child (2.7: `x` is the younger one).
/// A lookup that breaks this, here a mother born after her child, is an invariant break
/// and not a wrong value: with such data the recursion could run in a circle.
#[test]
#[should_panic(expected = "invariant kinship.parent_not_older")]
fn w2_7_parent_younger_than_child_is_an_invariant_break() {
    let _ = phi_with_mother_born(50);
}

/// The boundary: a mother born in the same second as her child is not older either.
#[test]
#[should_panic(expected = "invariant kinship.parent_not_older")]
fn w2_7_parent_born_with_its_child_is_an_invariant_break() {
    let _ = phi_with_mother_born(10);
}

/// The twin of the two above: a mother born one second before her child is fine, and
/// the father of the child is related to it as a parent.
#[test]
fn w2_7a_parent_born_just_before_its_child() {
    assert!(phi_with_mother_born(9).to_bits() == 0.25_f64.to_bits());
}
