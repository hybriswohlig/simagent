//! Class P known-answer tests (ADR-0003 section 3.3): the expected bits of a class P row
//! are those of the pinned libm, written by `cargo xtask kat-record`; its reference is
//! the correctly rounded value from `tools/kat_reference.py`. This module holds the
//! bound between the two and checks the files in `tests/golden/kat/`.

/// The functions with class P rows and the bound of each against its reference, in ulp.
/// A function without an entry cannot have a class P row.
pub const CLASS_P: [(&str, u128); 3] = [("exp", 1), ("ln", 1), ("cos", 1)];

/// The expected field of a class P row before its first recording.
pub const UNRECORDED: &str = "unrecorded";

const HEADER: &str = "input_bits,expected_bits,class,reference_bits";

/// The place of an f64 on a monotone integer scale: negative values mirrored, both
/// zeros at 0, so neighbouring values are 1 apart, also across zero.
pub fn ordinal(bits: u64) -> i128 {
    i128::from(bits)
}

/// Distance of two f64 bit patterns in ulp (ADR-0003 section 3.3).
pub fn ulp_distance(a: u64, b: u64) -> u128 {
    let _ = (ordinal(a), ordinal(b));
    0
}

/// The findings of one KAT file `<name>.csv` as the gate sees it.
pub fn violations(name: &str, csv: &str) -> Vec<String> {
    let _ = (name, csv, CLASS_P, UNRECORDED, HEADER, ulp_distance(0, 0));
    Vec::new()
}

/// The input fields of the class P rows of a file, in file order.
pub fn class_p_inputs(csv: &str) -> Vec<String> {
    let _ = csv;
    Vec::new()
}

/// The file with the expected bits of its class P rows replaced by `actual`.
pub fn record(name: &str, csv: &str, actual: &[String]) -> Result<String, Vec<String>> {
    let _ = (name, actual);
    Ok(csv.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: u64 = 0x3ff0_0000_0000_0000;
    const NEGATIVE: u64 = 1 << 63;
    const INFINITY: u64 = 0x7ff0_0000_0000_0000;

    fn file(rows: &[&str]) -> String {
        let mut text = format!("# comment\n{HEADER}\n");
        for row in rows {
            text.push_str(row);
            text.push('\n');
        }
        text
    }

    /// ADR-0003 section 3.3: distance of the bit patterns after mapping onto a monotone
    /// integer, negative values mirrored, both zeros 0.
    #[test]
    fn ulp_distance_follows_the_adr() {
        assert_eq!(ordinal(0), 0);
        assert_eq!(ordinal(NEGATIVE), 0, "-0.0");
        assert_eq!(ulp_distance(0, NEGATIVE), 0);
        assert_eq!(ulp_distance(ONE, ONE), 0);
        assert_eq!(ulp_distance(ONE, ONE + 1), 1);
        assert_eq!(ulp_distance(ONE + 1, ONE), 1);
        assert_eq!(ulp_distance(ONE - 1, ONE + 2), 3, "across a power of two");
        // Negative values: a larger pattern is a smaller value.
        assert!(ordinal(NEGATIVE | (ONE + 1)) < ordinal(NEGATIVE | ONE));
        assert_eq!(ulp_distance(NEGATIVE | ONE, NEGATIVE | (ONE + 1)), 1);
        // The smallest subnormals on both sides of zero are 2 apart, each 1 from zero.
        assert_eq!(ulp_distance(1, NEGATIVE | 1), 2);
        assert_eq!(ulp_distance(1, NEGATIVE), 1);
        // The largest finite value and infinity are neighbours.
        assert_eq!(ulp_distance(INFINITY - 1, INFINITY), 1);
        assert_eq!(
            ulp_distance(INFINITY, NEGATIVE | INFINITY),
            2 * u128::from(INFINITY)
        );
    }

    #[test]
    fn a_recorded_file_within_its_bound_is_clean() {
        let csv = file(&[
            "0000000000000000,3ff0000000000000,E,3ff0000000000000",
            "7ff8000000000000,nan,E,nan",
            "3ff0000000000000,4005bf0a8b145769,P,4005bf0a8b145769",
            "4000000000000000,401d8e64b8d4ddad,P,401d8e64b8d4ddae",
            "c000000000000000,3fc152aaa3bf81cc,P,3fc152aaa3bf81cb",
        ]);
        assert_eq!(violations("exp", &csv), Vec::<String>::new());
    }

    #[test]
    fn each_broken_row_is_a_finding() {
        let cases = [
            (
                "3ff0000000000000,unrecorded,P,4005bf0a8b145769",
                "exp.csv:3: not recorded, run cargo xtask kat-record",
            ),
            (
                "3ff0000000000000,4005bf0a8b14576b,P,4005bf0a8b145769",
                "exp.csv:3: 4005bf0a8b14576b is 2 ulp from the reference 4005bf0a8b145769, at most 1 allowed",
            ),
            (
                "3ff0000000000000,4005bf0a8b145767,P,4005bf0a8b145769",
                "exp.csv:3: 4005bf0a8b145767 is 2 ulp from the reference 4005bf0a8b145769, at most 1 allowed",
            ),
            (
                "0000000000000000,3ff0000000000001,E,3ff0000000000000",
                "exp.csv:3: a class E row expects its reference",
            ),
            (
                "3ff0000000000000,nan,P,4005bf0a8b145769",
                "exp.csv:3: a class P row needs two non-NaN bit patterns",
            ),
            (
                "3ff0000000000000,4005bf0a8b145769,P,nan",
                "exp.csv:3: a class P row needs two non-NaN bit patterns",
            ),
            (
                "3ff0000000000000,7ff8000000000000,P,7ff8000000000000",
                "exp.csv:3: a class P row needs two non-NaN bit patterns",
            ),
            (
                "3ff0000000000000,4005BF0A8B145769,P,4005bf0a8b145769",
                "exp.csv:3: a class P row needs two non-NaN bit patterns",
            ),
            (
                "3ff0000000000000,4005bf0a8b145769,Q,4005bf0a8b145769",
                "exp.csv:3: unknown class Q",
            ),
            (
                "3ff0000000000000,4005bf0a8b145769,P",
                "exp.csv:3: expected 4 fields",
            ),
        ];
        for (row, finding) in cases {
            assert_eq!(
                violations("exp", &file(&[row])),
                vec![finding.to_string()],
                "{row}"
            );
        }
    }

    /// ADR-0003 section 3.3: without a fixed bound a function has no class P row.
    #[test]
    fn class_p_needs_a_bound_and_a_reference_column() {
        let row = "3ff0000000000000,3ff0000000000000,P,3ff0000000000000";
        assert_eq!(
            violations("sqrt", &file(&[row])),
            vec![
                "sqrt.csv:3: class P row in a function without a bound in xtask/src/kat.rs"
                    .to_string()
            ]
        );
        // A class E file of a function without a bound is fine.
        let exact = "3ff0000000000000,3ff0000000000000,E,3ff0000000000000";
        assert_eq!(violations("sqrt", &file(&[exact])), Vec::<String>::new());
        // Another form has no reference column: fine without class P, a finding with.
        let other = "# comment\ninput,expected,class\n1,none,E\n";
        assert_eq!(violations("to_i64_exact", other), Vec::<String>::new());
        let hidden = "# comment\ninput,expected,class\n1,none,P\n";
        assert_eq!(
            violations("to_i64_exact", hidden),
            vec!["to_i64_exact.csv:3: a class P row needs a reference column".to_string()]
        );
        assert_eq!(
            violations("exp", other),
            vec![format!("exp.csv: a class P file needs the header {HEADER}")]
        );
        assert_eq!(
            violations("xxh3_64", "# c\ninput_hex,xxh3_64\n,2d06\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn recording_fills_the_class_p_rows_and_nothing_else() {
        let csv = file(&[
            "0000000000000000,3ff0000000000000,E,3ff0000000000000",
            "3ff0000000000000,unrecorded,P,4005bf0a8b145769",
            "7ff8000000000000,nan,E,nan",
            "4000000000000000,401d8e64b8d4ddae,P,401d8e64b8d4ddae",
        ]);
        assert_eq!(
            class_p_inputs(&csv),
            vec![
                "3ff0000000000000".to_string(),
                "4000000000000000".to_string()
            ]
        );
        let actual = [
            "4005bf0a8b14576a".to_string(),
            "401d8e64b8d4ddad".to_string(),
        ];
        assert_eq!(
            record("exp", &csv, &actual),
            Ok(file(&[
                "0000000000000000,3ff0000000000000,E,3ff0000000000000",
                "3ff0000000000000,4005bf0a8b14576a,P,4005bf0a8b145769",
                "7ff8000000000000,nan,E,nan",
                "4000000000000000,401d8e64b8d4ddad,P,401d8e64b8d4ddae",
            ]))
        );
    }

    /// ADR-0003 section 3.3: `kat-record` writes nothing if a row breaks its bound.
    #[test]
    fn recording_refuses_a_value_beyond_the_bound() {
        let csv = file(&[
            "3ff0000000000000,unrecorded,P,4005bf0a8b145769",
            "4000000000000000,unrecorded,P,401d8e64b8d4ddae",
        ]);
        let beyond = [
            "4005bf0a8b145769".to_string(),
            "401d8e64b8d4ddb0".to_string(),
        ];
        assert_eq!(
            record("exp", &csv, &beyond),
            Err(vec![
                "exp.csv:4: 401d8e64b8d4ddb0 is 2 ulp from the reference 401d8e64b8d4ddae, at most 1 allowed"
                    .to_string()
            ])
        );
        let nan = ["4005bf0a8b145769".to_string(), "nan".to_string()];
        assert_eq!(
            record("exp", &csv, &nan),
            Err(vec![
                "exp.csv:4: a class P row needs two non-NaN bit patterns".to_string()
            ])
        );
        let short = ["4005bf0a8b145769".to_string()];
        assert_eq!(
            record("exp", &csv, &short),
            Err(vec!["exp.csv: 2 class P rows but 1 values".to_string()])
        );
        let long = vec!["4005bf0a8b145769".to_string(); 3];
        assert_eq!(
            record("exp", &csv, &long),
            Err(vec!["exp.csv: 2 class P rows but 3 values".to_string()])
        );
    }
}
