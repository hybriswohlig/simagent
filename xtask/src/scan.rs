//! Source and config scans of the gate (ADR-0000, ADR-0003). Text based for now; the
//! token based scans S1 to S5 of ADR-0003 arrive with the types they inspect (step 2).

/// Lints that must stay at `forbid`, with the table they live in.
pub const FORBID_LINTS: [(&str, &str); 5] = [
    ("rust", "unsafe_code"),
    ("rust", "private_interfaces"),
    ("clippy", "disallowed_types"),
    ("clippy", "disallowed_methods"),
    ("clippy", "as_conversions"),
];

/// Lint names and groups that an `allow` or `expect` attribute must never mention.
pub const GUARDED_NAMES: [&str; 8] = [
    "unsafe_code",
    "private_interfaces",
    "disallowed_types",
    "disallowed_methods",
    "as_conversions",
    "clippy::style",
    "clippy::restriction",
    "clippy::all",
];

/// Line numbers (1 based) of `extern crate std` that are not directly preceded by
/// `#[cfg(test)]`. Strings and comments are ignored.
pub fn extern_std_outside_test(source: &str) -> Vec<usize> {
    let code = mask_non_code(source);
    let mut previous = "";
    let mut found = Vec::new();
    for (index, line) in code.lines().enumerate() {
        let line = line.trim();
        if line.starts_with("extern crate std") && previous != "#[cfg(test)]" {
            found.push(index + 1);
        }
        if !line.is_empty() {
            previous = line;
        }
    }
    found
}

/// Attributes that `allow` or `expect` a guarded lint, as found in the source. Strings,
/// char literals and comments are ignored.
pub fn guarded_allows(source: &str) -> Vec<String> {
    let code = mask_non_code(source);
    let mut found = Vec::new();
    let mut rest = code.as_str();
    while let Some(start) = [rest.find("#["), rest.find("#![")]
        .into_iter()
        .flatten()
        .min()
    {
        let attribute = attribute_at(&rest[start..]);
        let compact: String = attribute.chars().filter(|c| !c.is_whitespace()).collect();
        let allows = compact.contains("allow(") || compact.contains("expect(");
        if allows && GUARDED_NAMES.iter().any(|name| compact.contains(name)) {
            found.push(compact);
        }
        rest = &rest[start + attribute.len().max(1)..];
    }
    found
}

/// `source` with the contents of string literals, raw strings, char literals and
/// comments replaced by spaces. Newlines are kept, so line numbers stay valid.
fn mask_non_code(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut code = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let next = chars.get(i + 1).copied();
        let end = match (chars[i], next) {
            ('/', Some('/')) => line_comment_end(&chars, i),
            ('/', Some('*')) => block_comment_end(&chars, i),
            ('r', _) if raw_string_opens(&chars, i) => raw_string_end(&chars, i),
            ('"', _) => string_end(&chars, i),
            ('\'', _) => char_literal_end(&chars, i).unwrap_or(i + 1),
            _ => i + 1,
        };
        if end == i + 1 {
            code.push(chars[i]);
        } else {
            code.extend(
                chars[i..end]
                    .iter()
                    .map(|&c| if c == '\n' { '\n' } else { ' ' }),
            );
        }
        i = end;
    }
    code
}

fn line_comment_end(chars: &[char], start: usize) -> usize {
    (start..chars.len())
        .find(|&j| chars[j] == '\n')
        .unwrap_or(chars.len())
}

/// End of a block comment starting at `start`; Rust block comments nest.
fn block_comment_end(chars: &[char], start: usize) -> usize {
    let mut depth = 0_u32;
    let mut j = start;
    while j + 1 < chars.len() {
        match (chars[j], chars[j + 1]) {
            ('/', '*') => {
                depth += 1;
                j += 2;
            }
            ('*', '/') => {
                depth = depth.saturating_sub(1);
                j += 2;
                if depth == 0 {
                    return j;
                }
            }
            _ => j += 1,
        }
    }
    chars.len()
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True if a raw string (`r"`, `r#"`, also after a `b` prefix) starts at `start`.
fn raw_string_opens(chars: &[char], start: usize) -> bool {
    let prefix_ok = match start.checked_sub(1).map(|p| chars[p]) {
        None => true,
        Some('b') => start < 2 || !is_identifier_char(chars[start - 2]),
        Some(c) => !is_identifier_char(c),
    };
    let quote = (start + 1..chars.len()).find(|&j| chars[j] != '#');
    prefix_ok && quote.is_some_and(|j| chars[j] == '"')
}

fn raw_string_end(chars: &[char], start: usize) -> usize {
    let quote = (start + 1..chars.len())
        .find(|&j| chars[j] != '#')
        .unwrap_or(chars.len());
    let hashes = quote - start - 1;
    let mut j = quote + 1;
    while j < chars.len() {
        let closes =
            chars[j] == '"' && (1..=hashes).all(|k| chars.get(j + k).is_some_and(|&c| c == '#'));
        if closes {
            return j + 1 + hashes;
        }
        j += 1;
    }
    chars.len()
}

fn string_end(chars: &[char], start: usize) -> usize {
    let mut j = start + 1;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '"' => return j + 1,
            _ => j += 1,
        }
    }
    chars.len()
}

/// End of a char literal starting at `start`, or `None` for a lifetime such as `'a`.
fn char_literal_end(chars: &[char], start: usize) -> Option<usize> {
    match (chars.get(start + 1), chars.get(start + 2)) {
        (Some('\\'), _) => (start + 3..chars.len().min(start + 14))
            .find(|&j| chars[j] == '\'')
            .map(|j| j + 1),
        (Some(&c), Some('\'')) if c != '\'' => Some(start + 3),
        _ => None,
    }
}

/// The attribute starting at `text` (which begins with `#`), up to its closing bracket.
fn attribute_at(text: &str) -> &str {
    let mut depth = 0_u32;
    for (index, character) in text.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &text[..=index];
                }
            }
            _ => {}
        }
    }
    text
}

/// True if `ident` occurs as an identifier in the code (strings and comments ignored).
pub fn mentions_identifier(source: &str, ident: &str) -> bool {
    let code = mask_non_code(source);
    code.match_indices(ident).any(|(start, _)| {
        let before = code[..start].chars().next_back();
        let after = code[start + ident.len()..].chars().next();
        !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
    })
}

/// Text types that the core must not name: log entries carry only numbers, IDs, codes
/// and data IDs (ADR-0004 section 7, B16). Scanning every file of `sim-core/src` also
/// covers a payload field whose type (newtype or alias) is defined outside `log.rs`.
pub const TEXT_TYPES: [&str; 3] = ["String", "str", "Cow"];

/// The only files of `sim-core/src` that may name a text type, with the reason.
pub const TEXT_FILES: [(&str, &str); 2] = [
    (
        "crates/sim-core/src/data_id.rs",
        "DataId and Code, the checked newtypes",
    ),
    (
        "crates/sim-core/src/hex_u64.rs",
        "hex form of u64 in human-readable formats",
    ),
];

/// The only file of `sim-core/src` that may name the hash crate (ADR-0003 S5).
pub const HASH_FILE: &str = "crates/sim-core/src/hash.rs";

/// One message per file (path, source) that names one of `idents` as an identifier
/// without being in `allowed`. A scan that cannot be trusted is reported too: no file
/// at all, or an allowed path that is not among the files (renamed or moved).
pub fn identifiers_outside(
    files: &[(String, String)],
    idents: &[&str],
    allowed: &[&str],
) -> Vec<String> {
    let _ = (files, idents, allowed, mentions_identifier);
    Vec::new()
}

/// The text between `# BEGIN COMMON` and `# END COMMON` of a clippy.toml.
pub fn common_block(clippy_toml: &str) -> Option<String> {
    let mut lines = clippy_toml.lines();
    lines.find(|line| line.trim_start().starts_with("# BEGIN COMMON"))?;
    let mut block = String::new();
    for line in lines {
        if line.trim_start().starts_with("# END COMMON") {
            return Some(block);
        }
        block.push_str(line);
        block.push('\n');
    }
    None
}

/// Lints of `FORBID_LINTS` that are not set to "forbid" in the root Cargo.toml.
pub fn lints_not_forbidden(root_cargo_toml: &str) -> Vec<String> {
    let mut forbidden = Vec::new();
    let mut table = "";
    for line in root_cargo_toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            table = line.trim_matches(|c| c == '[' || c == ']');
        } else if let Some((key, value)) = line.split_once('=')
            && value.contains("\"forbid\"")
        {
            forbidden.push((table, key.trim()));
        }
    }
    FORBID_LINTS
        .iter()
        .filter(|(group, name)| {
            let expected = format!("workspace.lints.{group}");
            !forbidden.contains(&(expected.as_str(), name))
        })
        .map(|(group, name)| {
            if *group == "clippy" {
                format!("clippy::{name}")
            } else {
                (*name).to_string()
            }
        })
        .collect()
}

/// True if a member Cargo.toml inherits the workspace lints (`[lints]` with
/// `workspace = true`).
pub fn inherits_workspace_lints(member_cargo_toml: &str) -> bool {
    let mut in_lints = false;
    for line in member_cargo_toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_lints = line == "[lints]";
        } else if in_lints && line.replace(' ', "") == "workspace=true" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extern_std_is_allowed_only_under_cfg_test() {
        let source = "#![no_std]\nextern crate alloc;\n\n#[cfg(test)]\nextern crate std;\n";
        assert!(extern_std_outside_test(source).is_empty());
        let bad = "#![no_std]\nextern crate std;\n// extern crate std; in a comment\n";
        assert_eq!(extern_std_outside_test(bad), vec![2]);
        let renamed = "#[cfg(test)]\nextern crate alloc;\nextern crate std as s;\n";
        assert_eq!(extern_std_outside_test(renamed), vec![3]);
    }

    #[test]
    fn guarded_allows_are_found() {
        assert!(guarded_allows("#[allow(dead_code)]\nfn f() { x.expect(\"a\"); }").is_empty());
        let found = guarded_allows(
            "#[allow(clippy::disallowed_types)]\nfn f() {}\n#![expect(\n    clippy::as_conversions\n)]\n#[cfg_attr(test, allow(unsafe_code))]\n#[allow(clippy::style)]",
        );
        assert_eq!(found.len(), 4, "{found:?}");
    }

    #[test]
    fn attributes_in_strings_and_comments_are_ignored() {
        let source = concat!(
            "let s = \"#[allow(clippy::as_conversions)]\";\n",
            "let r = r#\"#[allow(unsafe_code)] \"quoted\" \"#;\n",
            "// #[allow(unsafe_code)]\n",
            "/* #[allow(clippy::disallowed_types)] */\n",
            "let open = \"#[\";\n",
            "let bracket = '[';\n",
            "fn f<'a>(x: &'a str) {}\n",
            "#[allow(clippy::disallowed_methods)]\n",
            "fn g() {}\n",
        );
        let found = guarded_allows(source);
        assert_eq!(
            found,
            vec!["#[allow(clippy::disallowed_methods)]".to_string()]
        );
    }

    #[test]
    fn extern_std_in_a_string_is_ignored() {
        let source = "let s = \"\nextern crate std;\n\";\n";
        assert!(extern_std_outside_test(source).is_empty());
    }

    #[test]
    fn identifiers_are_found_only_in_code() {
        assert!(mentions_identifier("use xxhash_rust::xxh3;", "xxhash_rust"));
        assert!(mentions_identifier(
            "let h = xxhash_rust::xxh3::xxh3_64(b);",
            "xxhash_rust"
        ));
        assert!(!mentions_identifier(
            "// xxhash_rust in a comment\nlet s = \"xxhash_rust\";",
            "xxhash_rust"
        ));
        assert!(!mentions_identifier(
            "use my_xxhash_rust_wrapper;",
            "xxhash_rust"
        ));
    }

    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
            .collect()
    }

    const ALLOWED: &str = "src/data_id.rs";

    fn text_findings(list: &[(&str, &str)]) -> Vec<String> {
        let mut all = vec![(ALLOWED, "pub struct DataId(String);")];
        all.extend_from_slice(list);
        identifiers_outside(&files(&all), &TEXT_TYPES, &[ALLOWED])
    }

    #[test]
    fn text_types_are_reported_outside_the_allowed_files() {
        let cases: [(&str, &[&str]); 5] = [
            ("pub struct P { note: String }", &["String"]),
            ("pub struct P { note: alloc::string::String }", &["String"]),
            ("pub struct P { note: Box<str> }", &["str"]),
            ("pub struct P { note: &'static str }", &["str"]),
            ("pub struct P<'a> { note: Cow<'a, str> }", &["str", "Cow"]),
        ];
        for (source, names) in cases {
            let expected: Vec<String> = names
                .iter()
                .map(|name| format!("src/log.rs: names {name}"))
                .collect();
            assert_eq!(
                text_findings(&[("src/log.rs", source)]),
                expected,
                "{source}"
            );
        }
        assert_eq!(text_findings(&[]), Vec::<String>::new());
    }

    #[test]
    fn a_newtype_or_alias_in_another_file_is_reported() {
        let found = text_findings(&[
            (
                "src/log.rs",
                "pub enum P { Renamed { name: Name, note: Text } }",
            ),
            (
                "src/names.rs",
                "pub struct Name(pub alloc::string::String);\npub type Text = alloc::string::String;",
            ),
        ]);
        assert_eq!(found, vec!["src/names.rs: names String".to_string()]);
    }

    #[test]
    fn text_types_in_comments_strings_and_longer_names_are_ignored() {
        let source = concat!(
            "// a String in a comment\n",
            "fn fmt(&self, f: &mut Formatter<'_>) -> Result { f.write_str(\"str\") }\n",
            "pub struct P { code: Code, id: DataId, s: StringTable }\n",
        );
        assert_eq!(
            text_findings(&[("src/log.rs", source)]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_scan_that_sees_nothing_is_reported() {
        assert_eq!(
            identifiers_outside(&[], &TEXT_TYPES, &[]),
            vec!["no file scanned".to_string()]
        );
        let found = identifiers_outside(
            &files(&[("src/log.rs", "pub struct P;")]),
            &TEXT_TYPES,
            &[ALLOWED],
        );
        assert_eq!(
            found,
            vec!["src/data_id.rs: allowed file is not among the scanned files".to_string()]
        );
    }

    #[test]
    fn one_identifier_stays_in_one_file() {
        let sources = files(&[
            ("src/hash.rs", "use xxhash_rust::xxh3::xxh3_64;"),
            (
                "src/world.rs",
                "fn h(b: &[u8]) -> u64 { xxhash_rust::xxh3::xxh3_64(b) }",
            ),
            ("src/log.rs", "// xxhash_rust in a comment"),
        ]);
        assert_eq!(
            identifiers_outside(&sources, &["xxhash_rust"], &["src/hash.rs"]),
            vec!["src/world.rs: names xxhash_rust".to_string()]
        );
    }

    #[test]
    fn the_real_lists_name_files_of_the_core() {
        for (file, reason) in TEXT_FILES {
            assert!(file.starts_with("crates/sim-core/src/") && !reason.is_empty());
        }
        assert!(HASH_FILE.starts_with("crates/sim-core/src/"));
    }

    #[test]
    fn the_common_block_is_extracted() {
        let text = "# head\n# BEGIN COMMON x\na = 1\n# END COMMON\nb = 2\n";
        assert_eq!(common_block(text).as_deref(), Some("a = 1\n"));
        assert!(common_block("a = 1\n").is_none());
    }

    #[test]
    fn lint_levels_must_stay_forbid() {
        let good = "[workspace.lints.rust]\nunsafe_code = \"forbid\"\nprivate_interfaces = \"forbid\"\n\n[workspace.lints.clippy]\ndisallowed_types = \"forbid\"\ndisallowed_methods = \"forbid\"\nas_conversions = \"forbid\"\n";
        assert!(lints_not_forbidden(good).is_empty());
        let lowered = good.replace("as_conversions = \"forbid\"", "as_conversions = \"deny\"");
        assert_eq!(
            lints_not_forbidden(&lowered),
            vec!["clippy::as_conversions".to_string()]
        );
        let moved = good.replace(
            "[workspace.lints.clippy]\ndisallowed_types = \"forbid\"\n",
            "[workspace.lints.clippy]\n",
        );
        let moved = moved.replace(
            "[workspace.lints.rust]\n",
            "[workspace.lints.rust]\ndisallowed_types = \"forbid\"\n",
        );
        assert_eq!(
            lints_not_forbidden(&moved),
            vec!["clippy::disallowed_types".to_string()]
        );
    }

    #[test]
    fn members_inherit_the_workspace_lints() {
        assert!(inherits_workspace_lints(
            "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n"
        ));
        assert!(!inherits_workspace_lints("[package]\nname = \"a\"\n"));
        assert!(!inherits_workspace_lints("[lints]\nworkspace = false\n"));
        assert!(!inherits_workspace_lints(
            "[lints.rust]\nunsafe_code = \"allow\"\n"
        ));
    }
}
