//! Known answers of the stable hash (ADR-0003 5.1): expected values come from the C
//! reference libxxhash via tools/hash_reference.py, never from a run of this crate.

use sim_core::hash::{StableHasher, stable_hash64};

const KAT: &str = include_str!("../../../tests/golden/kat/xxh3_64.csv");

fn decode_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex byte"))
        .collect()
}

fn cases() -> Vec<(Vec<u8>, u64)> {
    let mut lines = KAT.lines().filter(|line| !line.starts_with('#'));
    assert_eq!(lines.next(), Some("input_hex,xxh3_64"), "the header line");
    lines
        .map(|line| {
            let (input, expected) = line.split_once(',').expect("two columns");
            (
                decode_hex(input),
                u64::from_str_radix(expected, 16).expect("hex hash"),
            )
        })
        .collect()
}

#[test]
fn stable_hash_known_answers() {
    let cases = cases();
    assert_eq!(cases.len(), 23, "xxh3_64.csv: number of rows");
    for (input, expected) in cases {
        assert_eq!(
            stable_hash64(&input),
            expected,
            "xxh3_64 of {} bytes",
            input.len()
        );
    }
}

#[test]
fn streaming_equals_one_shot() {
    for (input, expected) in cases() {
        for chunk in [1, 7, 64, 1000] {
            let mut hasher = StableHasher::new();
            for part in input.chunks(chunk) {
                hasher.update(part);
            }
            assert_eq!(
                hasher.digest(),
                expected,
                "{} bytes in chunks of {chunk}",
                input.len()
            );
        }
    }
}
