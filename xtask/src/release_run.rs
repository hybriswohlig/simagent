//! The golden scenario through the release binary (ADR-0001 section 7). Tests build the
//! core with the features that the dev-dependencies add (`ppv-lite86/std` through
//! proptest), the binary of `sim-cli` does not. Results must not depend on that, so the
//! gate runs the scenario with the release binary and compares its manifest with the
//! golden of the scenario.

use serde_json::Value;

/// The fields a run and its golden share (ADR-0003 6.3).
const FIELDS: [&str; 4] = ["core_version", "data_hash", "year_hashes", "log_hash"];

/// What differs between the manifest of a run and the golden of its scenario. `Err` if
/// one of the two cannot be read: then nothing was compared.
pub fn differences(manifest_json: &str, golden_json: &str) -> Result<Vec<String>, String> {
    let _ = (manifest_json, golden_json, FIELDS, Value::Null);
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = r#"{
  "core_version": 1,
  "data_hash": "2d06800538d394c2",
  "year_hashes": [
    {"year": 0, "hash": "4975cb896896dd84"},
    {"year": 1, "hash": "85278e55ff32df8d"}
  ],
  "log_hash": "3233ead4da597110"
}"#;

    /// A manifest has more fields than its golden and is written on one line.
    fn manifest(
        core_version: u64,
        data_hash: &str,
        years: &[(u64, &str)],
        log_hash: &str,
    ) -> String {
        let hashes: Vec<String> = years
            .iter()
            .map(|(year, hash)| format!(r#"{{"year":{year},"hash":"{hash}"}}"#))
            .collect();
        format!(
            r#"{{"format_version":1,"core_version":{core_version},"data_hash":"{data_hash}","build_commit":"unknown","world_seed":"0000000000000001","closed_through":62899199,"year_hashes":[{}],"log_hash":"{log_hash}"}}"#,
            hashes.join(",")
        )
    }

    const YEARS: [(u64, &str); 2] = [(0, "4975cb896896dd84"), (1, "85278e55ff32df8d")];

    #[test]
    fn an_equal_run_has_no_differences() {
        let run = manifest(1, "2d06800538d394c2", &YEARS, "3233ead4da597110");
        assert_eq!(differences(&run, GOLDEN), Ok(Vec::new()));
    }

    #[test]
    fn each_field_is_compared() {
        let cases = [
            (
                manifest(2, "2d06800538d394c2", &YEARS, "3233ead4da597110"),
                "core_version: the release binary gives 2, the golden says 1",
            ),
            (
                manifest(1, "2d06800538d394c3", &YEARS, "3233ead4da597110"),
                r#"data_hash: the release binary gives "2d06800538d394c3", the golden says "2d06800538d394c2""#,
            ),
            (
                manifest(1, "2d06800538d394c2", &YEARS, "3233ead4da597111"),
                r#"log_hash: the release binary gives "3233ead4da597111", the golden says "3233ead4da597110""#,
            ),
            (
                manifest(
                    1,
                    "2d06800538d394c2",
                    &[(0, "4975cb896896dd84"), (1, "85278e55ff32df8e")],
                    "3233ead4da597110",
                ),
                r#"year_hashes[1]: the release binary gives {"hash":"85278e55ff32df8e","year":1}, the golden says {"hash":"85278e55ff32df8d","year":1}"#,
            ),
            (
                manifest(1, "2d06800538d394c2", &YEARS[..1], "3233ead4da597110"),
                "year_hashes: the release binary has 1 years, the golden 2",
            ),
            (
                manifest(
                    1,
                    "2d06800538d394c2",
                    &[YEARS[0], YEARS[1], (2, "3f5b80ad926d7600")],
                    "3233ead4da597110",
                ),
                "year_hashes: the release binary has 3 years, the golden 2",
            ),
        ];
        for (run, finding) in cases {
            assert_eq!(
                differences(&run, GOLDEN),
                Ok(vec![finding.to_string()]),
                "{finding}"
            );
        }
        // Two differences are both named, the first year that differs only once.
        let both = manifest(
            1,
            "2d06800538d394c2",
            &[(0, "0000000000000001"), (1, "0000000000000002")],
            "3233ead4da597111",
        );
        assert_eq!(
            differences(&both, GOLDEN),
            Ok(vec![
                r#"year_hashes[0]: the release binary gives {"hash":"0000000000000001","year":0}, the golden says {"hash":"4975cb896896dd84","year":0}"#.to_string(),
                r#"year_hashes[1]: the release binary gives {"hash":"0000000000000002","year":1}, the golden says {"hash":"85278e55ff32df8d","year":1}"#.to_string(),
                r#"log_hash: the release binary gives "3233ead4da597111", the golden says "3233ead4da597110""#.to_string(),
            ])
        );
    }

    /// A file that cannot be read compares nothing: that is an error, not an empty list.
    #[test]
    fn unreadable_files_are_errors() {
        let run = manifest(1, "2d06800538d394c2", &YEARS, "3233ead4da597110");
        assert_eq!(
            differences("not json", GOLDEN),
            Err("the manifest of the run is not a JSON object".to_string())
        );
        assert_eq!(
            differences("[]", GOLDEN),
            Err("the manifest of the run is not a JSON object".to_string())
        );
        assert_eq!(
            differences(&run, "{}"),
            Err("the golden has no field core_version".to_string())
        );
        assert_eq!(
            differences(&run, "tag 1 A"),
            Err("the golden is not a JSON object".to_string())
        );
        let without = run.replace(r#","log_hash":"3233ead4da597110""#, "");
        assert_eq!(
            differences(&without, GOLDEN),
            Err("the manifest of the run has no field log_hash".to_string())
        );
        let no_list = GOLDEN.replace(
            r#"[
    {"year": 0, "hash": "4975cb896896dd84"},
    {"year": 1, "hash": "85278e55ff32df8d"}
  ]"#,
            "7",
        );
        assert_eq!(
            differences(&run, &no_list),
            Err(
                "year_hashes is not a list in the manifest of the run and in the golden"
                    .to_string()
            )
        );
    }
}
