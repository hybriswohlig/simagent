//! Class P known-answer tests (ADR-0003 section 3.3): the expected bits of a class P row
//! are those of the pinned libm, written by `cargo xtask kat-record`; its reference is
//! the correctly rounded value from `tools/kat_reference.py`. This module holds the
//! bound between the two and checks the files in `tests/golden/kat/`.

/// How far a recorded value may lie from its reference.
#[derive(Clone, Copy)]
pub enum Bound {
    /// In ulp: the libm functions (ADR-0003 section 3.3).
    Ulp(u128),
    /// 2^-48 times the reference: `exponential` (ADR-0001 section 3).
    Relative,
    /// 2^-45, and only at mean 0 and sd 1: `normal` (ADR-0001 section 3). Rounded
    /// intermediate values near a zero of the cosine may cost many ulp.
    AbsoluteStandardNormal,
}

/// The functions with class P rows and the bound of each against its reference. A
/// function without an entry cannot have a class P row.
pub const CLASS_P: [(&str, Bound); 5] = [
    ("exp", Bound::Ulp(1)),
    ("ln", Bound::Ulp(1)),
    ("cos", Bound::Ulp(1)),
    ("exponential", Bound::Relative),
    ("normal", Bound::AbsoluteStandardNormal),
];

const TWO_POW_MINUS_48: f64 = f64::from_bits(0x3cf0_0000_0000_0000);
const TWO_POW_MINUS_45: f64 = f64::from_bits(0x3d20_0000_0000_0000);
/// The inputs of `normal` end with mean 0 and sd 1 in the rows that have a bound.
const STANDARD_NORMAL: &str = ";0000000000000000;3ff0000000000000";

/// The expected field of a class P row before its first recording.
pub const UNRECORDED: &str = "unrecorded";

const HEADER: &str = "input_bits,expected_bits,class,reference_bits";

/// The place of an f64 on a monotone integer scale: negative values mirrored, both
/// zeros at 0, so neighbouring values are 1 apart, also across zero.
pub fn ordinal(bits: u64) -> i128 {
    let magnitude = i128::from(bits & (u64::MAX >> 1));
    if bits >> 63 == 1 {
        -magnitude
    } else {
        magnitude
    }
}

/// Distance of two f64 bit patterns in ulp (ADR-0003 section 3.3).
pub fn ulp_distance(a: u64, b: u64) -> u128 {
    ordinal(a).abs_diff(ordinal(b))
}

/// 16 lowercase hex digits as bits; NaN patterns are refused, a NaN result is written
/// as `nan` and never compared by bits.
fn finite_bits(field: &str) -> Option<u64> {
    if field.len() != 16
        || !field
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let bits = u64::from_str_radix(field, 16).ok()?;
    (!f64::from_bits(bits).is_nan()).then_some(bits)
}

/// The data rows of a file in the unary form, with their line numbers from 1. Comment
/// lines and the header are skipped; `None` if the header is another one.
fn unary_rows(csv: &str) -> Option<Vec<(usize, Vec<&str>)>> {
    let mut lines = csv
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#') && !line.is_empty());
    let (_, header) = lines.next()?;
    if header != HEADER {
        return None;
    }
    Some(
        lines
            .map(|(index, line)| (index + 1, line.split(',').collect()))
            .collect(),
    )
}

/// The findings of one KAT file `<name>.csv` as the gate sees it. A file in another
/// form has no reference column and so no class P row: it must not name the class.
pub fn violations(name: &str, csv: &str) -> Vec<String> {
    let bound = CLASS_P
        .iter()
        .find(|(function, _)| *function == name)
        .map(|(_, bound)| *bound);
    let Some(rows) = unary_rows(csv) else {
        let mut findings = Vec::new();
        if bound.is_some() {
            findings.push(format!(
                "{name}.csv: a class P file needs the header {HEADER}"
            ));
        }
        for (index, line) in csv.lines().enumerate() {
            if !line.starts_with('#') && line.split(',').any(|field| field == "P") {
                findings.push(format!(
                    "{name}.csv:{}: a class P row needs a reference column",
                    index + 1
                ));
            }
        }
        return findings;
    };
    let mut findings = Vec::new();
    for (line, fields) in rows {
        let [inputs, expected, class, reference] = fields[..] else {
            findings.push(format!("{name}.csv:{line}: expected 4 fields"));
            continue;
        };
        match class {
            "E" => {
                if expected != reference {
                    findings.push(format!(
                        "{name}.csv:{line}: a class E row expects its reference"
                    ));
                }
            }
            "P" => {
                let Some(bound) = bound else {
                    findings.push(format!(
                        "{name}.csv:{line}: class P row in a function without a bound in xtask/src/kat.rs"
                    ));
                    continue;
                };
                if expected == UNRECORDED {
                    findings.push(format!(
                        "{name}.csv:{line}: not recorded, run cargo xtask kat-record"
                    ));
                    continue;
                }
                let (Some(recorded), Some(correct)) =
                    (finite_bits(expected), finite_bits(reference))
                else {
                    findings.push(format!(
                        "{name}.csv:{line}: a class P row needs two non-NaN bit patterns"
                    ));
                    continue;
                };
                let difference = (f64::from_bits(recorded) - f64::from_bits(correct)).abs();
                match bound {
                    Bound::Ulp(most) => {
                        let distance = ulp_distance(recorded, correct);
                        if distance > most {
                            findings.push(format!(
                                "{name}.csv:{line}: {expected} is {distance} ulp from the reference {reference}, at most {most} allowed"
                            ));
                        }
                    }
                    Bound::Relative => {
                        // A difference of NaN (two infinities) is not within the bound.
                        let within = difference <= TWO_POW_MINUS_48 * f64::from_bits(correct).abs();
                        if !within {
                            findings.push(format!(
                                "{name}.csv:{line}: {expected} is further than 2^-48 relative from the reference {reference}"
                            ));
                        }
                    }
                    Bound::AbsoluteStandardNormal => {
                        let within = difference <= TWO_POW_MINUS_45;
                        if !inputs.ends_with(STANDARD_NORMAL) {
                            findings.push(format!(
                                "{name}.csv:{line}: a class P row of normal needs mean 0 and sd 1, other parameters have no bound"
                            ));
                        } else if !within {
                            findings.push(format!(
                                "{name}.csv:{line}: {expected} is further than 2^-45 absolute from the reference {reference}"
                            ));
                        }
                    }
                }
            }
            other => findings.push(format!("{name}.csv:{line}: unknown class {other}")),
        }
    }
    findings
}

/// True for the fields of a class P row.
fn is_class_p(fields: &[&str]) -> bool {
    fields.len() == 4 && fields[2] == "P"
}

/// The input fields of the class P rows of a file, in file order.
pub fn class_p_inputs(csv: &str) -> Vec<String> {
    unary_rows(csv)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, fields)| is_class_p(fields))
        .map(|(_, fields)| fields[0].to_string())
        .collect()
}

/// The file with the expected bits of its class P rows replaced by `actual` (one value
/// per row, in file order), every other byte unchanged. Findings instead of a file if
/// the count does not fit or a row breaks its bound: then nothing may be written.
pub fn record(name: &str, csv: &str, actual: &[String]) -> Result<String, Vec<String>> {
    let wanted = class_p_inputs(csv).len();
    if actual.len() != wanted {
        return Err(vec![format!(
            "{name}.csv: {wanted} class P rows but {} values",
            actual.len()
        )]);
    }
    let mut values = actual.iter();
    let mut text = String::new();
    for line in csv.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        let recorded = if !line.starts_with('#') && is_class_p(&fields) {
            values.next()
        } else {
            None
        };
        match recorded {
            Some(value) => {
                text.push_str(&format!(
                    "{},{value},{},{}",
                    fields[0], fields[2], fields[3]
                ));
            }
            None => text.push_str(line),
        }
        text.push('\n');
    }
    let findings = violations(name, &text);
    if findings.is_empty() {
        Ok(text)
    } else {
        Err(findings)
    }
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
                "3ff0000000000000,4005bf0a8b14576,P,4005bf0a8b145769",
                "exp.csv:3: a class P row needs two non-NaN bit patterns",
            ),
            (
                "3ff0000000000000,4005bf0a8b145769,P,04005bf0a8b145769",
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

    /// ADR-0001 section 3: `exponential` may differ from its reference by 2^-48 relative.
    #[test]
    fn the_relative_bound_of_exponential() {
        let row = |expected: &str, reference: &str| {
            file(&[&format!(
                "0000000000000000;3ff0000000000000,{expected},P,{reference}"
            )])
        };
        // Around 1.0 an ulp is 2^-52 above and 2^-53 below: 2^-48 is 16 and 32 ulp.
        for within in ["3ff0000000000000", "3ff0000000000010", "3fefffffffffffe0"] {
            let csv = row(within, "3ff0000000000000");
            assert_eq!(
                violations("exponential", &csv),
                Vec::<String>::new(),
                "{within}"
            );
        }
        for beyond in ["3ff0000000000011", "3fefffffffffffdf", "bff0000000000000"] {
            assert_eq!(
                violations("exponential", &row(beyond, "3ff0000000000000")),
                vec![format!(
                    "exponential.csv:3: {beyond} is further than 2^-48 relative from the reference 3ff0000000000000"
                )],
                "{beyond}"
            );
        }
        // A reference of zero allows only zero.
        assert_eq!(
            violations("exponential", &row("0000000000000001", "0000000000000000")),
            vec![
                "exponential.csv:3: 0000000000000001 is further than 2^-48 relative from the reference 0000000000000000"
                    .to_string()
            ]
        );
        assert_eq!(
            violations("exponential", &row("7ff0000000000000", "7ff0000000000000")),
            vec![
                "exponential.csv:3: 7ff0000000000000 is further than 2^-48 relative from the reference 7ff0000000000000"
                    .to_string()
            ],
            "infinity is no known answer of a draw"
        );
    }

    /// ADR-0001 section 3: `normal` with mean 0 and sd 1 may differ by 2^-45 absolute;
    /// other parameters have no bound and so no class P row.
    #[test]
    fn the_absolute_bound_of_normal() {
        const STANDARD: &str =
            "0000000000000000;0000000000000000;0000000000000000;3ff0000000000000";
        let row = |inputs: &str, expected: &str, reference: &str| {
            file(&[&format!("{inputs},{expected},P,{reference}")])
        };
        // 2^-45 is 3d20000000000000.
        for within in ["0000000000000000", "3d20000000000000", "bd20000000000000"] {
            let csv = row(STANDARD, within, "0000000000000000");
            assert_eq!(violations("normal", &csv), Vec::<String>::new(), "{within}");
        }
        for beyond in ["3d20000000000001", "bd20000000000001"] {
            assert_eq!(
                violations("normal", &row(STANDARD, beyond, "0000000000000000")),
                vec![format!(
                    "normal.csv:3: {beyond} is further than 2^-45 absolute from the reference 0000000000000000"
                )],
                "{beyond}"
            );
        }
        // Far from zero the absolute bound is below one ulp: only the reference itself.
        assert_eq!(
            violations(
                "normal",
                &row(STANDARD, "402124b2800eda48", "402124b2800eda48")
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            violations("normal", &row(STANDARD, "4340000000000001", "4340000000000000")),
            vec![
                "normal.csv:3: 4340000000000001 is further than 2^-45 absolute from the reference 4340000000000000"
                    .to_string()
            ]
        );
        let scaled = "0000000000000000;0000000000000000;3ff0000000000000;4008000000000000";
        assert_eq!(
            violations("normal", &row(scaled, "3ff0000000000000", "3ff0000000000000")),
            vec![
                "normal.csv:3: a class P row of normal needs mean 0 and sd 1, other parameters have no bound"
                    .to_string()
            ]
        );
        // An exact row may have any parameters.
        let exact = file(&[&format!("{scaled},3ff0000000000000,E,3ff0000000000000")]);
        assert_eq!(violations("normal", &exact), Vec::<String>::new());
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
