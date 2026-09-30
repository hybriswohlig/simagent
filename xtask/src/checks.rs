//! Pure check functions of the gate, kept free of I/O so they can be tested.

/// Markdown files allowed in the repository root (ADR-0000: planning lives in `docs/`).
pub const ROOT_MARKDOWN_ALLOWED: [&str; 2] = ["CLAUDE.md", "HANDOFF.md"];
/// Line limit for CLAUDE.md (ADR-0000).
pub const CLAUDE_MD_MAX_LINES: usize = 200;
/// Line limit for docs/STAND.md (ADR-0000).
pub const STAND_MD_MAX_LINES: usize = 50;
/// Compiler flags that may change floating point results (ADR-0003, AT4).
pub const FORBIDDEN_FLAGS: [&str; 3] = ["target-cpu", "target-feature", "llvm-args"];

/// Path fragments of folders synced by iCloud Drive.
const ICLOUD_MARKERS: [&str; 3] = [
    "/Library/Mobile Documents/",
    "/iCloud Drive/",
    "com~apple~CloudDocs",
];

/// True if the path lies in a folder synced by iCloud Drive (ADR-0000: iCloud once
/// emptied source files and created duplicates in CapLab).
pub fn is_icloud_path(path: &str) -> bool {
    ICLOUD_MARKERS.iter().any(|marker| path.contains(marker))
}

/// The forbidden flags that occur in `text` (environment value or config file), in the
/// order of `FORBIDDEN_FLAGS`.
pub fn forbidden_flags_in(text: &str) -> Vec<&'static str> {
    FORBIDDEN_FLAGS
        .into_iter()
        .filter(|flag| text.contains(flag))
        .collect()
}

/// Root file names that are markdown but not on the allow list.
pub fn unexpected_root_markdown(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".md"))
        .filter(|name| !ROOT_MARKDOWN_ALLOWED.contains(name))
        .map(|name| (*name).to_string())
        .collect()
}

/// Error message if `text` has more than `max` lines.
pub fn line_limit_violation(name: &str, text: &str, max: usize) -> Option<String> {
    let lines = text.lines().count();
    (lines > max).then(|| format!("{name} has {lines} lines, the limit is {max} (ADR-0000)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icloud_paths_are_detected() {
        assert!(is_icloud_path(
            "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/simagent"
        ));
        assert!(is_icloud_path("/Users/x/iCloud Drive/simagent"));
        assert!(!is_icloud_path("/Users/x/Projects/simagent/simagent"));
    }

    #[test]
    fn forbidden_flags_are_found() {
        assert_eq!(
            forbidden_flags_in("-C target-cpu=native"),
            vec!["target-cpu"]
        );
        assert_eq!(
            forbidden_flags_in(
                "rustflags = [\"-C\", \"target-feature=+fma\", \"-C\", \"llvm-args=-x\"]"
            ),
            vec!["target-feature", "llvm-args"]
        );
        assert!(forbidden_flags_in("-D warnings").is_empty());
    }

    #[test]
    fn only_allowed_markdown_in_root() {
        assert_eq!(
            unexpected_root_markdown(&["CLAUDE.md", "HANDOFF.md", "LICENSE", "PLAN-X.md"]),
            vec!["PLAN-X.md".to_string()]
        );
        assert!(unexpected_root_markdown(&["CLAUDE.md", "Cargo.toml"]).is_empty());
    }

    #[test]
    fn line_limits_are_enforced() {
        let three = "a\nb\nc\n";
        assert!(line_limit_violation("X.md", three, 3).is_none());
        let msg = line_limit_violation("X.md", three, 2);
        assert!(msg.is_some_and(|m| m.contains("X.md") && m.contains('3') && m.contains('2')));
    }
}
