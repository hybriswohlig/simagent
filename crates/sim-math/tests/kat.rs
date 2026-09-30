//! Known-answer tests (ADR-0003, section 3). Expected values come from
//! tools/kat_reference.py (class E: exact by IEEE 754 / C99 Annex F), never from a run of
//! this crate. NaN is checked with `is_nan`, never by bit pattern.

const CEIL: &str = include_str!("../../../tests/golden/kat/ceil.csv");
const TO_I64_EXACT: &str = include_str!("../../../tests/golden/kat/to_i64_exact.csv");
const I64_TO_F64_EXACT: &str = include_str!("../../../tests/golden/kat/i64_to_f64_exact.csv");

/// Data rows of a KAT file (without the comment line and the header).
fn rows(csv: &str) -> impl Iterator<Item = Vec<&str>> {
    csv.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .skip(1)
        .map(|line| line.split(',').collect())
}

fn from_bits(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).expect("hex bits"))
}

fn assert_f64(function: &str, input: &str, expected: &str, actual: f64) {
    if expected == "nan" {
        assert!(
            actual.is_nan(),
            "{function}({input}): expected NaN, got {actual:e}"
        );
    } else {
        assert_eq!(
            actual.to_bits(),
            from_bits(expected).to_bits(),
            "{function}({input}): expected bits {expected}, got {:016x}",
            actual.to_bits()
        );
    }
}

#[test]
fn ceil_known_answers() {
    let mut count = 0;
    for row in rows(CEIL) {
        let input = from_bits(row[0]);
        assert_f64("ceil", row[0], row[1], sim_math::ceil(input));
        count += 1;
    }
    assert!(count >= 20, "ceil.csv has too few rows: {count}");
}

#[test]
fn to_i64_exact_known_answers() {
    let mut count = 0;
    for row in rows(TO_I64_EXACT) {
        let expected = match row[1] {
            "none" => None,
            value => Some(value.parse::<i64>().expect("i64")),
        };
        assert_eq!(
            sim_math::to_i64_exact(from_bits(row[0])),
            expected,
            "to_i64_exact({})",
            row[0]
        );
        count += 1;
    }
    assert!(count >= 15, "to_i64_exact.csv has too few rows: {count}");
}

#[test]
fn i64_to_f64_exact_known_answers() {
    let mut count = 0;
    for row in rows(I64_TO_F64_EXACT) {
        let input = row[0].parse::<i64>().expect("i64");
        let actual = sim_math::i64_to_f64_exact(input);
        match row[1] {
            "none" => assert_eq!(actual, None, "i64_to_f64_exact({input})"),
            bits => assert_f64(
                "i64_to_f64_exact",
                row[0],
                bits,
                actual.unwrap_or_else(|| panic!("i64_to_f64_exact({input}) is None")),
            ),
        }
        count += 1;
    }
    assert!(
        count >= 10,
        "i64_to_f64_exact.csv has too few rows: {count}"
    );
}
