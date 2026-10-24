//! Relatives in direct line (ADR-0018 section 8, Grok's round 33, P1): parents,
//! grandparents and every generation above, and the other way round, never a couple,
//! whatever φ, at any depth. The pedigree is built by hand; every expected value follows
//! from the definition (one is an ancestor of the other by mother and father edges), and
//! the φ values from genetics specification 2.7 with the depth of a query (`kinship`
//! module, Grok's H2: a great-great-grandparent 1/32, one generation more 0).

use std::collections::BTreeMap;

use sim_core::ids::SimId;
use sim_core::kinship::{Kin, in_direct_line, phi};
use sim_core::time::SimTime;

/// Name, class of the order (0 the pool run, 1 the town), second of birth, mother, father
/// (`None` for one not known).
type Person = (
    &'static str,
    u8,
    i64,
    Option<&'static str>,
    Option<&'static str>,
);

/// Parents come before their children. `a0` to `a6` are one line of seven generations,
/// `m1` to `m5` the partners who marry into it; `u1` is a brother of `a1`, `h1` a half
/// brother, `c2` a first cousin of `a2`. The pool persons are born later in seconds than
/// the town's founders but come first in the order (class 0), as in the world. `d5` is a
/// second child of `a4`, so `k` descends from `a4` by two routes.
const FAMILY: [Person; 27] = [
    ("a0", 1, 0, None, None),
    ("b0", 1, 0, None, None),
    ("x", 1, 0, None, None),
    ("z", 1, 0, None, None),
    ("m1", 1, 90, None, None),
    ("m2", 1, 190, None, None),
    ("m3", 1, 390, None, None),
    ("m4", 1, 490, None, None),
    ("m5", 1, 590, None, None),
    ("p_gm", 0, 5_000, None, None),
    ("p_gf", 0, 5_000, None, None),
    ("p_mother", 0, 20_000, Some("p_gm"), Some("p_gf")),
    ("a1", 1, 100, Some("a0"), Some("b0")),
    ("u1", 1, 110, Some("a0"), Some("b0")),
    ("h1", 1, 120, Some("a0"), Some("z")),
    ("t1", 1, 150, Some("p_mother"), Some("b0")),
    ("a2", 1, 200, Some("a1"), Some("m1")),
    ("c2", 1, 205, Some("x"), Some("u1")),
    ("t2", 1, 250, Some("t1"), Some("x")),
    ("a3", 1, 300, Some("a2"), Some("m2")),
    ("a4", 1, 400, Some("a3"), Some("m3")),
    ("a5", 1, 500, Some("a4"), Some("m4")),
    ("d5", 1, 510, Some("a4"), Some("z")),
    ("a6", 1, 600, Some("a5"), Some("m5")),
    ("k", 1, 700, Some("a6"), Some("d5")),
    ("only_mother", 1, 650, Some("a6"), None),
    ("stranger", 1, 640, None, None),
];

struct Tree {
    kin: BTreeMap<SimId, Kin<SimId>>,
    ids: BTreeMap<&'static str, SimId>,
}

impl Tree {
    fn new() -> Tree {
        let mut tree = Tree {
            kin: BTreeMap::new(),
            ids: BTreeMap::new(),
        };
        for (n, (name, class, born, mother, father)) in (1_u64..).zip(FAMILY) {
            let id = SimId::try_from(n).expect("id");
            let kin = Kin {
                order: (class, SimTime::from_secs(born)),
                mother: mother.map(|parent| tree.id(parent)),
                father: father.map(|parent| tree.id(parent)),
            };
            tree.kin.insert(id, kin);
            tree.ids.insert(name, id);
        }
        tree
    }

    fn id(&self, name: &str) -> SimId {
        self.ids[name]
    }

    /// Whether `a` and `b` are in direct line, asked both ways round: the answer is the
    /// same.
    fn line(&self, a: &str, b: &str) -> bool {
        let lookup = |key: SimId| self.kin[&key];
        let forth = in_direct_line(self.id(a), self.id(b), lookup);
        let back = in_direct_line(self.id(b), self.id(a), lookup);
        assert_eq!(forth, back, "{a} and {b}: the same both ways");
        forth
    }

    fn phi(&self, a: &str, b: &str) -> f64 {
        phi(self.id(a), self.id(b), |key| self.kin[&key])
    }
}

#[test]
fn every_ancestor_of_seven_generations_is_in_direct_line() {
    let tree = Tree::new();
    for ancestor in [
        "a0", "b0", "a1", "m1", "a2", "m2", "a3", "m3", "a4", "m4", "a5", "m5",
    ] {
        assert!(tree.line(ancestor, "a6"), "{ancestor} and a6");
    }
    // And every pair within the line.
    let line = ["a0", "a1", "a2", "a3", "a4", "a5", "a6"];
    for (i, older) in line.iter().enumerate() {
        for younger in &line[i + 1..] {
            assert!(tree.line(older, younger), "{older} and {younger}");
        }
    }
}

/// The depth of a query ends φ at the great-great-grandparent: `a2` is one of `a6` (four
/// generations up, φ = 1/32), `a1` one generation more (φ = 0). The direct line has no
/// depth: both are in it.
#[test]
fn the_line_holds_where_phi_is_zero() {
    let tree = Tree::new();
    assert_eq!(tree.phi("a2", "a6"), 1.0 / 32.0);
    assert_eq!(tree.phi("a1", "a6"), 0.0);
    assert!(tree.line("a2", "a6"));
    assert!(tree.line("a1", "a6"));
    assert!(tree.line("a0", "a6"));
}

#[test]
fn collateral_relatives_partners_and_strangers_are_not() {
    let tree = Tree::new();
    for (a, b) in [
        ("a1", "u1"),       // brothers and sisters
        ("a1", "h1"),       // half siblings
        ("u1", "a2"),       // an uncle
        ("u1", "a6"),       // a brother of an ancestor
        ("c2", "a2"),       // first cousins
        ("h1", "a3"),       // a half brother of a grandparent
        ("m1", "a1"),       // partners
        ("m1", "m2"),       // two who married into the line
        ("a0", "m1"),       // a founder and one who married in
        ("stranger", "a6"), // no shared ancestor
        ("d5", "a5"),       // half siblings through a4
    ] {
        assert!(!tree.line(a, b), "{a} and {b}");
    }
}

/// The pool's persons count by the same edges: the pool grandmother is in direct line with
/// her town grandchild's child, though she is born later in seconds than the town's
/// founders (her class comes first).
#[test]
fn the_pool_counts_like_the_town() {
    let tree = Tree::new();
    assert!(tree.line("p_mother", "t1"));
    assert!(tree.line("p_gm", "t1"));
    assert!(tree.line("p_gf", "t2"));
    assert!(tree.line("b0", "t2"));
    assert!(!tree.line("p_gm", "a6"));
    assert!(!tree.line("p_mother", "a1"));
}

/// A pedigree with a loop: `k` descends from `a4` through `a5` and through `d5`; every
/// ancestor on either route is in its line, a brother of a route is not.
#[test]
fn a_pedigree_with_a_loop() {
    let tree = Tree::new();
    for ancestor in ["a4", "a5", "d5", "z", "m4", "m5", "a0", "a6"] {
        assert!(tree.line(ancestor, "k"), "{ancestor} and k");
    }
    assert!(!tree.line("u1", "k"));
    assert!(!tree.line("stranger", "k"));
}

/// A parent that is not known is no edge: a person with only a known mother is in direct
/// line with her line and with nobody through a father.
#[test]
fn an_unknown_parent_adds_no_edge() {
    let tree = Tree::new();
    assert!(tree.line("a6", "only_mother"));
    assert!(tree.line("a0", "only_mother"));
    assert!(tree.line("m5", "only_mother"));
    assert!(!tree.line("stranger", "only_mother"));
    assert!(!tree.line("k", "only_mother"));
}
