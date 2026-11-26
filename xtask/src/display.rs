//! Which diffs take the short gate (ADR-0000): one that changes display files only. The
//! tests of the workspace, the canaries, the no-std build, the gate run with its goldens,
//! the export types and the story read none of these files, so the short gate leaves them
//! out. Every push to main takes the full gate.

/// Whether a diff against the merge base with origin/main takes the short gate. `changed`
/// holds every path the diff names, a rename as its old and its new path; `None` stands
/// for a merge base that could not be found.
pub fn short_gate(changed: Option<&[&str]>) -> bool {
    changed.is_some_and(|paths| !paths.is_empty() && paths.iter().all(|path| display_file(path)))
}

/// The suffixes of the sources directly under `web/src/`.
const SOURCE_SUFFIXES: [&str; 3] = [".ts", ".tsx", ".css"];

/// A display file: below `web/src/render/`, `web/test/` or `web/figure/`, or a source
/// directly under `web/src/`. The generated types, the smoke test and the build files of
/// `web/` are none.
fn display_file(path: &str) -> bool {
    let below = |dir: &str| path.strip_prefix(dir).is_some_and(|rest| !rest.is_empty());
    if below("web/src/render/") || below("web/test/") || below("web/figure/") {
        return true;
    }
    path.strip_prefix("web/src/").is_some_and(|name| {
        !name.contains('/') && SOURCE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
    })
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
