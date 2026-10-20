//! The nearest common ancestor of two persons (ADR-0021 section 3), the ground of the
//! kinship words: generations up from A and down to B, the smallest sum first and then the
//! fewest up, and how many ancestors stand at that place (one: a half relation). On the
//! family of `tests/kinship.rs`; the expected places are read off the family by hand.

use std::collections::BTreeMap;

use sim_core::ids::SimId;
use sim_core::kinship::{Kin, Relation, phi, relation};
use sim_core::time::SimTime;

type Person = (&'static str, i64, Option<(&'static str, &'static str)>);

/// The family of `tests/kinship.rs`.
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

fn family() -> (BTreeMap<SimId, Kin<SimId>>, BTreeMap<&'static str, SimId>) {
    tree(&FAMILY)
}

fn place(up: u32, down: u32, shared: u32) -> Option<Relation> {
    Some(Relation { up, down, shared })
}

#[test]
fn the_nearest_common_ancestor_by_hand() {
    let (kin, ids) = family();
    let of = |a: &str, b: &str| relation(ids[a], ids[b], |sim| kin[&sim]);
    // Oneself; the direct line up and down.
    assert_eq!(of("child", "child"), place(0, 0, 1));
    assert_eq!(of("child", "mother"), place(1, 0, 1));
    assert_eq!(of("mother", "child"), place(0, 1, 1));
    assert_eq!(of("child", "gm"), place(2, 0, 1));
    assert_eq!(of("gf2", "child"), place(0, 2, 1));
    // Siblings share both parents; the half sibling shares the mother only.
    assert_eq!(of("child", "sib"), place(1, 1, 2));
    assert_eq!(of("child", "half_sib"), place(1, 1, 1));
    // The aunt and the uncle are children of a grandparent pair; the other way round the
    // child is their niece or nephew.
    assert_eq!(of("child", "aunt"), place(2, 1, 2));
    assert_eq!(of("child", "uncle"), place(2, 1, 2));
    assert_eq!(of("uncle", "child"), place(1, 2, 2));
    // First cousins share a grandparent pair; double first cousins two.
    assert_eq!(of("child", "cousin"), place(2, 2, 2));
    assert_eq!(of("child", "double_cousin"), place(2, 2, 4));
    // The child of two cousins: its father is the nearest, before every shared grandparent.
    assert_eq!(of("inbred", "child"), place(1, 0, 1));
    assert_eq!(of("inbred", "cousin"), place(1, 0, 1));
    assert_eq!(of("inbred", "sib"), place(2, 1, 2));
    // No common ancestor.
    assert_eq!(of("child", "uncle_in_law"), None);
    assert_eq!(of("x", "y"), None);
}

/// Two lines below a founder couple ("a", "b"): their children "c1" and "c2", and below
/// each a child, grandchild and great-grandchild ("d", "e", "f") with a founder partner,
/// and a "g1" below "f1". "f" stands four generations below the couple, "g1" five.
const LINES: [Person; 18] = [
    ("a", 0, None),
    ("b", 0, None),
    ("p1", 0, None),
    ("p2", 0, None),
    ("p3", 0, None),
    ("p4", 0, None),
    ("p5", 0, None),
    ("p6", 0, None),
    ("p7", 0, None),
    ("c1", 100, Some(("a", "b"))),
    ("c2", 101, Some(("a", "b"))),
    ("d1", 200, Some(("c1", "p1"))),
    ("d2", 201, Some(("p2", "c2"))),
    ("e1", 300, Some(("d1", "p3"))),
    ("e2", 301, Some(("p4", "d2"))),
    ("f1", 400, Some(("e1", "p5"))),
    ("f2", 401, Some(("p6", "e2"))),
    ("g1", 500, Some(("p7", "f1"))),
];

fn tree(people: &[Person]) -> (BTreeMap<SimId, Kin<SimId>>, BTreeMap<&'static str, SimId>) {
    let mut kin = BTreeMap::new();
    let mut ids = BTreeMap::new();
    for (n, &(name, born, parents)) in (1_u64..).zip(people) {
        let id = SimId::try_from(n).expect("id");
        ids.insert(name, id);
        let parents = parents.map(|(m, f)| (ids[m], ids[f]));
        kin.insert(
            id,
            Kin {
                order: (1, SimTime::from_secs(born)),
                mother: parents.map(|(m, _)| m),
                father: parents.map(|(_, f)| f),
            },
        );
    }
    (kin, ids)
}

/// The search walks what a query of φ reads, along the same paths (Grok's H2b: a path up
/// to a common ancestor at `dx` and down from `dy` counts iff `max ≤ 4` and `min ≤ 3`): a
/// great-great-grandparent, the sibling of a great-grandparent, and cousins once and twice
/// removed four generations up are relations; third cousins and a great-great-great-
/// grandparent are none.
#[test]
fn the_search_reaches_as_far_as_kinship() {
    let (kin, ids) = tree(&LINES);
    let of = |a: &str, b: &str| relation(ids[a], ids[b], |sim| kin[&sim]);
    assert_eq!(of("e1", "a"), place(3, 0, 1));
    assert_eq!(of("f1", "a"), place(4, 0, 1));
    assert_eq!(of("a", "f1"), place(0, 4, 1));
    assert_eq!(of("f1", "c2"), place(4, 1, 2));
    assert_eq!(of("c2", "f1"), place(1, 4, 2));
    assert_eq!(of("f1", "d2"), place(4, 2, 2));
    assert_eq!(of("d2", "f1"), place(2, 4, 2));
    assert_eq!(of("f1", "e2"), place(4, 3, 2));
    assert_eq!(of("e2", "f1"), place(3, 4, 2));
    assert_eq!(of("e1", "e2"), place(3, 3, 2));
    assert_eq!(of("f1", "f2"), None);
    assert_eq!(of("g1", "a"), None);
    assert_eq!(of("g1", "c2"), None);
    assert_eq!(of("g1", "d1"), place(3, 0, 1));
}

/// A loop: "m" fathers "c" with "w", and "y" with "c"'s granddaughter "g"; "c"'s other line
/// ends in "x", a great-great-grandchild. "x" reaches "c" in four steps, and "m" in five,
/// one step from "y".
const LOOP: [Person; 16] = [
    ("m", 0, None),
    ("w", 0, None),
    ("q1", 0, None),
    ("q2", 0, None),
    ("q3", 0, None),
    ("q4", 0, None),
    ("c", 100, Some(("w", "m"))),
    ("d", 200, Some(("c", "q1"))),
    ("d_other", 201, Some(("q2", "c"))),
    ("g", 300, Some(("d", "q3"))),
    ("e_other", 301, Some(("q4", "d_other"))),
    ("q5", 0, None),
    ("q6", 0, None),
    ("y", 400, Some(("g", "m"))),
    ("f_other", 401, Some(("q5", "e_other"))),
    ("x", 501, Some(("f_other", "q6"))),
];

/// The two places a loop adds: `y` is the (half) sister of `x`'s great-great-grandparent,
/// nearer by "m" (5 up, 1 down) than by "c" (4 up, 3 down).
#[test]
fn a_loop_reaches_five_generations_up() {
    let (kin, ids) = tree(&LOOP);
    let of = |a: &str, b: &str| relation(ids[a], ids[b], |sim| kin[&sim]);
    assert_eq!(of("x", "c"), place(4, 0, 1));
    assert_eq!(of("x", "y"), place(5, 1, 1));
    assert_eq!(of("y", "x"), place(1, 5, 1));
    assert_eq!(of("y", "m"), place(1, 0, 1));
    // "c" is "y"'s half brother before her great-grandfather: the smaller sum first.
    assert_eq!(of("y", "c"), place(1, 1, 1));
}

/// A tie of sums broken by fewer up: "y" reaches "c" in three steps and "m" in two (by
/// "p"); "x" reaches "c" in four, "m" in five. Both sum to 7, so "m" at `(2, 5)`. Two paths
/// of seven steps, `φ = 2 · 2^-8`.
const TIE: [Person; 18] = [
    ("m", 0, None),
    ("fc", 0, None),
    ("fe", 0, None),
    ("fb", 0, None),
    ("fa", 0, None),
    ("fx", 0, None),
    ("fd", 0, None),
    ("fg", 0, None),
    ("fp", 0, None),
    ("c", 100, Some(("fc", "m"))),
    ("p", 100, Some(("fp", "m"))),
    ("e", 200, Some(("fe", "c"))),
    ("d", 200, Some(("fd", "c"))),
    ("b", 300, Some(("fb", "e"))),
    ("g", 300, Some(("fg", "d"))),
    ("a", 400, Some(("fa", "b"))),
    ("y", 400, Some(("g", "p"))),
    ("x", 500, Some(("fx", "a"))),
];

/// Three ancestors tied at 7: "c" at `(3, 4)`, "h" at `(2, 5)`, "m" at `(1, 6)` from "y";
/// "m" wins. Three paths of seven steps, `φ = 3 · 2^-8`.
const TIES: [Person; 17] = [
    ("m", 0, None),
    ("fh", 0, None),
    ("fc", 0, None),
    ("fe", 0, None),
    ("fb", 0, None),
    ("fa", 0, None),
    ("fx", 0, None),
    ("fd", 0, None),
    ("h", 100, Some(("fh", "m"))),
    ("c", 200, Some(("fc", "h"))),
    ("e", 300, Some(("fe", "c"))),
    ("d", 300, Some(("fd", "c"))),
    ("b", 400, Some(("fb", "e"))),
    ("g", 400, Some(("d", "h"))),
    ("a", 500, Some(("fa", "b"))),
    ("y", 500, Some(("g", "m"))),
    ("x", 600, Some(("fx", "a"))),
];

/// The places a tie of sums adds: fewer up wins, so B may stand five or six down. From
/// the other side the same pedigrees give `(4, 3)`.
#[test]
fn a_tie_of_sums_goes_to_fewer_up() {
    let (kin, ids) = tree(&TIE);
    let lookup = |sim: SimId| kin[&sim];
    assert_eq!(relation(ids["y"], ids["x"], lookup), place(2, 5, 1));
    assert_eq!(relation(ids["x"], ids["y"], lookup), place(4, 3, 1));
    assert!(phi(ids["y"], ids["x"], lookup).to_bits() == f64::to_bits(2.0 / 256.0));
    let (kin, ids) = tree(&TIES);
    let lookup = |sim: SimId| kin[&sim];
    assert_eq!(relation(ids["y"], ids["x"], lookup), place(1, 6, 1));
    assert_eq!(relation(ids["x"], ids["y"], lookup), place(4, 3, 1));
    assert!(phi(ids["y"], ids["x"], lookup).to_bits() == f64::to_bits(3.0 / 256.0));
}

/// A relation is there exactly when φ is above 0, for every pair of the five families.
#[test]
fn a_relation_exactly_when_kinship_is_above_zero() {
    for people in [&FAMILY[..], &LINES[..], &LOOP[..], &TIE[..], &TIES[..]] {
        let (kin, ids) = tree(people);
        for &x in ids.values() {
            for &y in ids.values() {
                let lookup = |sim: SimId| kin[&sim];
                assert_eq!(
                    relation(x, y, lookup).is_some(),
                    phi(x, y, lookup) > 0.0,
                    "{x:?} {y:?}"
                );
            }
        }
    }
}

use sim_core::genome::Sex;
use sim_core::presentation::kin::{Foster, KinWord, foster, r_percent, word};

/// The words of Grok's list (K1, K2) for every place a search reaches (`up, down ≤ 4`, not
/// both 4, and `(5, 1)`, `(1, 5)`, `(1, 6)`, `(2, 5)` in a loop), by the sex of the other person, read off the list by hand: "Ur" `k − 2` times
/// from `k = 3` in the direct line, "Groß" and then "Urgroß" for the siblings of
/// grandparents and of their parents, cousins of the grade `min − 1` with the generations
/// between them, and half only for siblings.
#[test]
fn the_words_of_every_place() {
    use KinWord::*;
    let at = |up, down, shared| Some(Relation { up, down, shared });
    let cases = [
        (at(0, 0, 1), Myself, Myself),
        (at(1, 0, 1), Mother, Father),
        (at(2, 0, 1), Grandmother, Grandfather),
        (at(3, 0, 1), GreatGrandmother, GreatGrandfather),
        (at(4, 0, 1), GreatGreatGrandmother, GreatGreatGrandfather),
        (at(0, 1, 1), Daughter, Son),
        (at(0, 2, 1), Granddaughter, Grandson),
        (at(0, 3, 1), GreatGranddaughter, GreatGrandson),
        (at(0, 4, 1), GreatGreatGranddaughter, GreatGreatGrandson),
        (at(1, 1, 2), Sister, Brother),
        (at(1, 1, 1), HalfSister, HalfBrother),
        (at(2, 1, 2), Aunt, Uncle),
        (at(2, 1, 1), Aunt, Uncle),
        (at(3, 1, 2), GreatAunt, GreatUncle),
        (at(4, 1, 1), GreatGreatAunt, GreatGreatUncle),
        (at(5, 1, 1), GreatGreatGreatAunt, GreatGreatGreatUncle),
        (at(1, 2, 2), Niece, Nephew),
        (at(1, 3, 2), GreatNiece, GreatNephew),
        (at(1, 4, 1), GreatGreatNiece, GreatGreatNephew),
        (at(1, 5, 1), GreatGreatGreatNiece, GreatGreatGreatNephew),
        (
            at(1, 6, 1),
            GreatGreatGreatGreatNiece,
            GreatGreatGreatGreatNephew,
        ),
        (at(2, 2, 2), FirstCousinFemale, FirstCousinMale),
        (at(2, 2, 1), FirstCousinFemale, FirstCousinMale),
        (at(3, 2, 2), FirstCousinFemaleOlder1, FirstCousinMaleOlder1),
        (at(4, 2, 2), FirstCousinFemaleOlder2, FirstCousinMaleOlder2),
        (
            at(2, 3, 2),
            FirstCousinFemaleYounger1,
            FirstCousinMaleYounger1,
        ),
        (
            at(2, 4, 2),
            FirstCousinFemaleYounger2,
            FirstCousinMaleYounger2,
        ),
        (at(3, 3, 2), SecondCousinFemale, SecondCousinMale),
        (
            at(4, 3, 2),
            SecondCousinFemaleOlder1,
            SecondCousinMaleOlder1,
        ),
        (
            at(3, 4, 2),
            SecondCousinFemaleYounger1,
            SecondCousinMaleYounger1,
        ),
        (None, Unrelated, Unrelated),
    ];
    for (place, female, male) in cases {
        assert_eq!(word(place, Sex::Female), female, "{place:?}");
        assert_eq!(word(place, Sex::Male), male, "{place:?}");
    }
}

/// A place no search reaches has no word of its own: distant (Grok's K4).
#[test]
fn a_place_beyond_the_search_is_distant() {
    for (up, down) in [(4, 4), (5, 0), (0, 5), (6, 1), (5, 2), (3, 5), (8, 0)] {
        for sex in [Sex::Female, Sex::Male] {
            let place = Some(Relation {
                up,
                down,
                shared: 1,
            });
            assert_eq!(word(place, sex), KinWord::Distant, "{up} {down}");
        }
    }
}

/// `r = 2φ` in percent to a tenth, half up (Grok G6, K5), by hand: a parent or a sibling
/// 1/4, a grandparent 1/8, a first cousin 1/16, once removed 1/32, a second cousin 1/64,
/// half second cousins 1/128; oneself 1/2 and, with `F = 1/16`, 17/32; nobody 0.
#[test]
fn r_in_percent_to_a_tenth() {
    let cases = [
        (0.25, 50.0),
        (0.125, 25.0),
        (0.0625, 12.5),
        (0.03125, 6.3),
        (0.015625, 3.1),
        (0.0078125, 1.6),
        (0.5, 100.0),
        (17.0 / 32.0, 106.3),
        (0.0, 0.0),
    ];
    for (phi, percent) in cases {
        assert!(
            r_percent(phi).to_bits() == f64::to_bits(percent),
            "{phi}: {}",
            r_percent(phi)
        );
    }
}

/// The word of a direct social edge (K3): beside a blood word other than parent or child,
/// alone without one, never beside mother, father, daughter or son.
#[test]
fn a_foster_word_beside_the_blood_word() {
    use KinWord::*;
    // B took A in: B is A's foster mother or father.
    assert_eq!(
        foster(Unrelated, true, false, Sex::Female),
        Some(Foster::FosterMother)
    );
    assert_eq!(
        foster(Grandmother, true, false, Sex::Female),
        Some(Foster::FosterMother)
    );
    assert_eq!(
        foster(Aunt, true, false, Sex::Female),
        Some(Foster::FosterMother)
    );
    assert_eq!(
        foster(Grandfather, true, false, Sex::Male),
        Some(Foster::FosterFather)
    );
    // A took B in: B is A's foster daughter or son.
    assert_eq!(
        foster(Niece, false, true, Sex::Female),
        Some(Foster::FosterDaughter)
    );
    assert_eq!(
        foster(Unrelated, false, true, Sex::Male),
        Some(Foster::FosterSon)
    );
    // No edge, or a parent or child already.
    assert_eq!(foster(Unrelated, false, false, Sex::Male), None);
    for blood in [Mother, Father, Daughter, Son] {
        assert_eq!(foster(blood, true, false, Sex::Female), None, "{blood:?}");
        assert_eq!(foster(blood, false, true, Sex::Male), None, "{blood:?}");
    }
}
