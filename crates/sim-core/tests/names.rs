//! Names (ADR-0021 section 3): the draws of the name indices against the Python reference,
//! and the chain of fathers by hand.

use std::num::NonZeroU64;

use sim_core::genome::Sex;
use sim_core::lineage::LineageKey;
use sim_core::presentation::names::{chain_root, first_name_index, surname_index};
use sim_core::rng::WorldSeed;

fn key(hex: &str) -> LineageKey {
    serde_json::from_value(serde_json::Value::from(hex)).expect("a key")
}

fn len(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("not zero")
}

/// The draws of `tools/rng_reference.py` (`uniform_below` of the first word at the
/// address seed, tag, key, occasion, 0) for two keys and two seeds.
#[test]
fn the_indices_are_the_references_draws() {
    let cases = [
        (1, "4ee9ea8961efbe47", 102, 2, 272),
        (1, "3d147d76b5f097ff", 77, 197, 181),
        (0xa001, "4ee9ea8961efbe47", 29, 0, 152),
        (0xa001, "3d147d76b5f097ff", 128, 92, 48),
    ];
    for (seed, hex, female, male, surname) in cases {
        let seed = WorldSeed::new(seed);
        let k = key(hex);
        assert_eq!(
            first_name_index(seed, k, Sex::Female, len(200)),
            female,
            "{hex}"
        );
        assert_eq!(
            first_name_index(seed, k, Sex::Male, len(200)),
            male,
            "{hex}"
        );
        assert_eq!(surname_index(seed, k, len(300)), surname, "{hex}");
    }
    // A list of one has one name.
    assert_eq!(
        surname_index(WorldSeed::new(1), key("3d147d76b5f097ff"), len(1)),
        0
    );
}

/// The chain of fathers by hand: a -> b -> c, c without a father; a key without a
/// record ends the chain at itself; cycles end where the bound stops them.
#[test]
fn a_chain_ends_at_the_first_without_a_father() {
    let (a, b, c) = (
        key("000000000000000a"),
        key("000000000000000b"),
        key("000000000000000c"),
    );
    let fathers = [(a, b), (b, c)];
    let father_of = |k: LineageKey| {
        fathers
            .iter()
            .find(|(child, _)| *child == k)
            .map(|(_, f)| *f)
    };
    assert_eq!(chain_root(a, father_of, 10), c);
    assert_eq!(chain_root(b, father_of, 10), c);
    assert_eq!(chain_root(c, father_of, 10), c);
    let unknown = key("00000000000000ff");
    assert_eq!(chain_root(unknown, father_of, 10), unknown);
    // A record that is its own father, and two that are each other's.
    assert_eq!(chain_root(a, |_| Some(a), 3), a);
    let swap = |k: LineageKey| Some(if k == a { b } else { a });
    assert_eq!(chain_root(a, swap, 3), b);
    assert_eq!(chain_root(a, swap, 4), a);
    // Without records nothing is followed.
    assert_eq!(chain_root(a, father_of, 0), a);
}
