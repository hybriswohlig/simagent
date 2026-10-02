//! Known-answer tests (ADR-0003, section 3). Class E: the expected value comes from
//! tools/kat_reference.py (exact by IEEE 754 / C99 Annex F), never from a run of this
//! crate. Class P: the expected bits are those of the pinned libm, written by `cargo xtask
//! kat-record`; they prove equal results on every target, the gate checks them against
//! the reference column. NaN is checked with `is_nan`, never by bit pattern.

const CEIL: &str = include_str!("../../../tests/golden/kat/ceil.csv");
const EXP: &str = include_str!("../../../tests/golden/kat/exp.csv");
const LN: &str = include_str!("../../../tests/golden/kat/ln.csv");
const COS: &str = include_str!("../../../tests/golden/kat/cos.csv");
const SQRT: &str = include_str!("../../../tests/golden/kat/sqrt.csv");
const TO_I64_EXACT: &str = include_str!("../../../tests/golden/kat/to_i64_exact.csv");
const I64_TO_F64_EXACT: &str = include_str!("../../../tests/golden/kat/i64_to_f64_exact.csv");

const UNARY: &str = "input_bits,expected_bits,class,reference_bits";

/// Data rows of a KAT file: after the comment lines comes the header, then the rows.
fn rows<'a>(csv: &'a str, header: &str) -> impl Iterator<Item = Vec<&'a str>> {
    let mut lines = csv
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty());
    assert_eq!(lines.next(), Some(header), "the header line");
    lines.map(|line| line.split(',').collect())
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

/// The inputs every function is tested at (ADR-0003 section 3.1): both zeros, 1.0, both
/// infinities and NaN.
const REQUIRED: [&str; 6] = [
    "0000000000000000",
    "8000000000000000",
    "3ff0000000000000",
    "7ff0000000000000",
    "fff0000000000000",
    "7ff8000000000000",
];

/// A file in the unary form with exactly `count` rows.
fn unary_known_answers(function: &str, csv: &str, f: fn(f64) -> f64, count: usize) {
    let rows: Vec<Vec<&str>> = rows(csv, UNARY).collect();
    assert_eq!(rows.len(), count, "{function}.csv: number of rows");
    for required in REQUIRED {
        assert!(
            rows.iter().any(|row| row[0] == required),
            "{function}.csv has no row for the input {required}"
        );
    }
    for row in rows {
        assert_ne!(
            row[1], "unrecorded",
            "{function}({}): run cargo xtask kat-record",
            row[0]
        );
        assert_f64(function, row[0], row[1], f(from_bits(row[0])));
    }
}

#[test]
fn ceil_known_answers() {
    unary_known_answers("ceil", CEIL, sim_math::ceil, 28);
}

#[test]
fn exp_known_answers() {
    unary_known_answers("exp", EXP, sim_math::exp, 32);
}

#[test]
fn ln_known_answers() {
    unary_known_answers("ln", LN, sim_math::ln, 21);
}

#[test]
fn cos_known_answers() {
    unary_known_answers("cos", COS, sim_math::cos, 22);
}

#[test]
fn sqrt_known_answers() {
    unary_known_answers("sqrt", SQRT, sim_math::sqrt, 22);
}

#[test]
fn to_i64_exact_known_answers() {
    let mut count = 0;
    for row in rows(TO_I64_EXACT, "input_bits,expected,class") {
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
    assert_eq!(count, 21, "to_i64_exact.csv: number of rows");
}

#[test]
fn i64_to_f64_exact_known_answers() {
    let mut count = 0;
    for row in rows(I64_TO_F64_EXACT, "input,expected_bits,class") {
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
    assert_eq!(count, 14, "i64_to_f64_exact.csv: number of rows");
}
