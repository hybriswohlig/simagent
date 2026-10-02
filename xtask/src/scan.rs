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

/// The names of the functions that `source` declares (strings and comments ignored).
pub fn function_names(source: &str) -> Vec<String> {
    let code = mask_non_code(source);
    let mut names = Vec::new();
    for (start, _) in code.match_indices("fn") {
        let before = code[..start].chars().next_back();
        let rest = &code[start + 2..];
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| is_identifier_char(*c))
            .collect();
        // The word `fn`, space, a name: `fn(f64)` is a type and `fn_count` a name.
        if !before.is_some_and(is_identifier_char)
            && rest.starts_with(char::is_whitespace)
            && !name.is_empty()
        {
            names.push(name);
        }
    }
    names
}

/// Text types that the core must not name: log entries carry only numbers, IDs, codes
/// and data IDs (ADR-0004 section 7, B16). Scanning every file of `sim-core/src` also
/// covers a payload field whose type (newtype or alias) is defined outside `log.rs`.
/// `char` is text in JSON. The scan errs on the safe side: the module path `core::str`
/// is reported as well; a file that needs it joins `TEXT_FILES` with a reason.
pub const TEXT_TYPES: [&str; 4] = ["String", "str", "Cow", "char"];

/// The only files of `sim-core/src` that may name a text type: the file, the types it
/// may name and the reason.
pub const TEXT_FILES: [(&str, &[&str], &str); 4] = [
    (
        "crates/sim-core/src/body_contract/generated.rs",
        &["str"],
        "static keys and version of the body contract, generated (ADR-0005 section 2)",
    ),
    (
        "crates/sim-core/src/data_id.rs",
        &TEXT_TYPES,
        "DataId and Code, the checked newtypes",
    ),
    (
        "crates/sim-core/src/hex_u64.rs",
        &TEXT_TYPES,
        "hex form of u64 in human-readable formats",
    ),
    (
        TAGS_FILE,
        &["str"],
        "static names of the purpose tags for the pinned list (ADR-0001 section 5)",
    ),
];

/// The only file of `sim-core/src` that may name the hash crate (ADR-0003 S5).
pub const HASH_FILE: &str = "crates/sim-core/src/hash.rs";

/// The files of the random module, the only place that may name the generator crate
/// (ADR-0001 section 2): no generator, and so no generator state, anywhere else.
pub const RNG_DIR: &str = "crates/sim-core/src/rng/";

/// The files of the time module, the only place that may build a scheduler context.
pub const TIME_DIR: &str = "crates/sim-core/src/time/";

/// The only file outside `TIME_DIR` that calls `input_ctx(`: `World::submit` lives here.
pub const SUBMIT_FILE: &str = "crates/sim-core/src/world/mod.rs";

/// The only file of `sim-core/src` that may spell the seconds of a year as a number.
pub const YEAR_LITERAL_FILE: &str = "crates/sim-core/src/time/clock.rs";

/// How often `ident` is used in the code: every occurrence as an identifier that is not
/// its definition `fn ident`. A method call, a path such as `Self::ident(..)`, a function
/// reference and a call with a comment or a line break before the parenthesis all count.
pub fn count_uses(source: &str, ident: &str) -> usize {
    let code = mask_non_code(source);
    code.match_indices(ident)
        .filter(|(start, _)| {
            let before = &code[..*start];
            let after = code[start + ident.len()..].chars().next();
            let is_word = !before.chars().next_back().is_some_and(is_identifier_char)
                && !after.is_some_and(is_identifier_char);
            // The definition: the keyword `fn`, then only white space.
            let head = before.trim_end();
            let defined = head.len() < before.len()
                && head
                    .strip_suffix("fn")
                    .is_some_and(|rest| !rest.chars().next_back().is_some_and(is_identifier_char));
            is_word && !defined
        })
        .count()
}

/// True if the code spells the seconds of a year, `31449600`, as a number: with or
/// without underscores, with a type suffix or as a float.
pub fn names_year_literal(source: &str) -> bool {
    const YEAR_DIGITS: &str = "31449600";
    let code = mask_non_code(source);
    let mut previous: Option<char> = None;
    let mut token = String::new();
    // A number token: digits and underscores that follow neither an identifier
    // character nor a decimal point.
    for c in code.chars().chain(core::iter::once(' ')) {
        let in_token = !token.is_empty();
        if c.is_ascii_digit() || (in_token && c == '_') {
            let starts = !in_token && !previous.is_some_and(|p| is_identifier_char(p) || p == '.');
            if in_token || starts {
                token.push(c);
            }
        } else {
            let digits: String = token.chars().filter(|d| *d != '_').collect();
            if digits == YEAR_DIGITS {
                return true;
            }
            token.clear();
        }
        previous = Some(c);
    }
    false
}

/// The time rules of the core over all files of `sim-core/src` (ADR-0002, "Eine
/// Zeitfunktion, keine zweite Schleife" and "Nur zwei Umrechnungen"): exactly one use
/// of `pop_next`, exactly one use of `input_ctx` outside the time module and that in
/// `SUBMIT_FILE`, the year literal only in `YEAR_LITERAL_FILE`.
pub fn core_time_findings(core_sources: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    let files_using = |ident: &str, outside: Option<&str>| -> (usize, Vec<&str>) {
        let mut total = 0;
        let mut files = Vec::new();
        for (path, source) in core_sources {
            if outside.is_some_and(|dir| path.starts_with(dir)) {
                continue;
            }
            let count = count_uses(source, ident);
            if count > 0 {
                total += count;
                files.push(path.as_str());
            }
        }
        files.sort_unstable();
        (total, files)
    };
    let (pops, pop_files) = files_using("pop_next", None);
    if pops != 1 {
        findings.push(format!(
            "pop_next is used {pops} times in sim-core/src, exactly one use is allowed (ADR-0002): {}",
            pop_files.join(", ")
        ));
    }
    let (inputs, input_files) = files_using("input_ctx", Some(TIME_DIR));
    if inputs != 1 || input_files != [SUBMIT_FILE] {
        findings.push(format!(
            "input_ctx is used {inputs} times outside the time module, exactly one use in {SUBMIT_FILE} is allowed (ADR-0002): {}",
            input_files.join(", ")
        ));
    }
    for (path, source) in core_sources {
        if path != YEAR_LITERAL_FILE && names_year_literal(source) {
            findings.push(format!(
                "{path}: spells the seconds of a year as a number, only {YEAR_LITERAL_FILE} may (ADR-0002)"
            ));
        }
    }
    findings
}

/// The identifier rules of the core over all files of `sim-core/src` (path relative to
/// the workspace root, source): the hash crate only in `HASH_FILE` (ADR-0003 S5), text
/// types only in `TEXT_FILES` (ADR-0004 section 7).
pub fn core_identifier_findings(core_sources: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for finding in identifiers_outside(core_sources, &["xxhash_rust"], &[HASH_FILE]) {
        findings.push(format!(
            "{finding} (only sim-core/src/hash.rs names the hash crate, ADR-0003 S5)"
        ));
    }
    const TEXT_RULE: &str =
        "(no text in the core outside the checked newtypes, ADR-0004 section 7)";
    if core_sources.is_empty() {
        findings.push(format!("no file scanned {TEXT_RULE}"));
    }
    for (file, _, _) in TEXT_FILES {
        if !core_sources.is_empty() && !core_sources.iter().any(|(path, _)| path == file) {
            findings.push(format!(
                "{file}: allowed file is not among the scanned files {TEXT_RULE}"
            ));
        }
    }
    for (path, source) in core_sources {
        let allowed: &[&str] = TEXT_FILES
            .iter()
            .find(|(file, _, _)| file == path)
            .map_or(&[], |(_, types, _)| *types);
        for ty in TEXT_TYPES {
            if !allowed.contains(&ty) && mentions_identifier(source, ty) {
                findings.push(format!("{path}: names {ty} {TEXT_RULE}"));
            }
        }
    }
    findings
}

/// The only file that names the generator type, and how often: its import and the one
/// generator that `key_words` builds and drops.
pub const GENERATOR_FILE: &str = "crates/sim-core/src/rng/mod.rs";

/// The only statements of the random module that name the generator crate, with single
/// spaces. Everything else (a re-export, an alias of the crate, a path in an expression)
/// is a finding, whatever its spelling.
pub const GENERATOR_IMPORTS: [&str; 2] = [
    "use rand_chacha::ChaCha8Rng;",
    "use rand_chacha::rand_core::{Rng, SeedableRng};",
];

/// The file of the purpose tags and the golden that pins them (ADR-0001 section 5).
pub const TAGS_FILE: &str = "crates/sim-core/src/rng/tags.rs";

/// The file of the tag type. Its field is private to this file, so the rest of the random
/// module builds a tag only through the function `from_pinned_list`.
pub const TAG_TYPE_FILE: &str = "crates/sim-core/src/rng/tag.rs";

/// The generator rules over the Rust files of all workspace members (ADR-0001 section
/// 2). A scan that cannot be trusted is reported too: no file under `RNG_DIR` (the
/// module moved or was renamed, or nothing was scanned at all).
pub fn generator_findings(workspace_sources: &[(String, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    if !workspace_sources
        .iter()
        .any(|(path, _)| path.starts_with(RNG_DIR))
    {
        findings.push(format!(
            "{RNG_DIR}: no file of the allowed directory is among the scanned files"
        ));
    }
    // Outside the random module nothing names the crate.
    for (path, source) in workspace_sources {
        if !path.starts_with(RNG_DIR) && mentions_identifier(source, "rand_chacha") {
            findings.push(format!(
                "{path}: names rand_chacha (only sim-core/src/rng/ names the generator crate, ADR-0001 section 2)"
            ));
        }
    }
    // Inside, the crate is named in its two imports only and the type exactly twice: no
    // re-export, no alias, no field, no second generator.
    let mut uses = 0;
    let mut using_files = Vec::new();
    for (path, source) in workspace_sources {
        if !path.starts_with(RNG_DIR) {
            continue;
        }
        for (line, statement) in statements_naming(source, "rand_chacha") {
            if path != GENERATOR_FILE || !GENERATOR_IMPORTS.contains(&statement.as_str()) {
                findings.push(format!(
                    "{path}:{line}: names rand_chacha outside the two imports of {GENERATOR_FILE} (ADR-0001 section 2): {statement}"
                ));
            }
        }
        if mentions_identifier(source, "include") {
            findings.push(format!(
                "{path}: include! hides code of the random module from the scans (ADR-0001)"
            ));
        }
        let count = count_uses(source, "ChaCha8Rng");
        if count > 0 {
            uses += count;
            using_files.push(path.as_str());
        }
    }
    if uses != 2 || using_files != [GENERATOR_FILE] {
        let files = if using_files.is_empty() {
            "no file".to_string()
        } else {
            using_files.join(", ")
        };
        findings.push(format!(
            "ChaCha8Rng is named {uses} times under {RNG_DIR}, exactly 2 uses in {GENERATOR_FILE} are allowed: the import and the one generator of key_words (ADR-0001 section 2): {files}"
        ));
    }
    findings
}

/// The statements of the code that name `ident`: the line of the name, counted from
/// one, and the text of the statement with single spaces. A statement runs from the end
/// of the previous statement or block (`;`, `{` or `}`) to the next `;`.
fn statements_naming(source: &str, ident: &str) -> Vec<(usize, String)> {
    let code = mask_non_code(source);
    let mut found = Vec::new();
    for (start, _) in code.match_indices(ident) {
        let before = &code[..start];
        let after = &code[start + ident.len()..];
        if before.chars().next_back().is_some_and(is_identifier_char)
            || after.chars().next().is_some_and(is_identifier_char)
        {
            continue;
        }
        let from = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
        let to = after
            .find(';')
            .map_or(code.len(), |index| start + ident.len() + index + 1);
        let statement: Vec<&str> = code[from..to].split_whitespace().collect();
        let line = before.matches('\n').count() + 1;
        found.push((line, statement.join(" ")));
    }
    found
}

/// The tag rules over all files of `sim-core/src` (ADR-0001 section 5); `golden` is the
/// text of `tests/golden/rng_tags.txt`. The compiler does the first half: the field of
/// the tag type is private to `TAG_TYPE_FILE`, so the rest of the core gets a pinned tag
/// only from the function `from_pinned_list`. The gate does the second: that name
/// stands only in `TAGS_FILE`, only in constants of one form, and their names and values
/// are the tag lines of the golden.
pub fn tag_findings(core_sources: &[(String, String)], golden: &str) -> Vec<String> {
    const BUILD: &str = "from_pinned_list";
    const GOLDEN: &str = "tests/golden/rng_tags.txt";
    let mut findings = Vec::new();
    for file in [TAGS_FILE, TAG_TYPE_FILE] {
        if !core_sources.iter().any(|(path, _)| path == file) {
            findings.push(format!(
                "{file}: file of the tag rules is not among the scanned files"
            ));
        }
    }
    let pinned: Vec<&str> = golden
        .lines()
        .filter(|line| line.starts_with("tag "))
        .collect();
    for (path, source) in core_sources {
        if path == TAGS_FILE {
            // The name stands only in tag constants of one form, and these are the tag
            // lines of the golden: no wrapper, no alias, no tag beside the list.
            let mut constants = Vec::new();
            for (line, statement) in statements_naming(source, BUILD) {
                match tag_constant(&statement) {
                    Some((name, value)) => constants.push(format!("tag {value} {name}")),
                    None => findings.push(format!(
                        "{path}:{line}: {BUILD} outside a constant of the form pub const NAME: PurposeTag = PurposeTag::{BUILD}(N); (ADR-0001 section 5): {statement}"
                    )),
                }
            }
            for constant in &constants {
                if !pinned.contains(&constant.as_str()) {
                    findings.push(format!(
                        "{path}: {constant} is not pinned in {GOLDEN} (ADR-0001 section 5)"
                    ));
                }
            }
            for line in &pinned {
                if !constants.iter().any(|constant| constant == line) {
                    findings.push(format!(
                        "{GOLDEN}: {line} has no constant in {path} (ADR-0001 section 5)"
                    ));
                }
            }
        } else if count_uses(source, BUILD) > 0 {
            findings.push(format!(
                "{path}: names {BUILD}; only {TAGS_FILE} builds pinned tags (ADR-0001 section 5)"
            ));
        }
    }
    findings
}

/// Name and value of a statement of exactly the form
/// `pub const NAME: PurposeTag = PurposeTag::from_pinned_list(N);` with a plain number.
fn tag_constant(statement: &str) -> Option<(&str, &str)> {
    let rest = statement.strip_prefix("pub const ")?;
    let (name, rest) = rest.split_once(": PurposeTag = PurposeTag::from_pinned_list(")?;
    let value = rest.strip_suffix(");")?;
    let plain_name = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    let plain_value = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    (plain_name && plain_value).then_some((name, value))
}

/// One message per file (path, source) that names one of `idents` as an identifier
/// without being in `allowed`. A scan that cannot be trusted is reported too: no file
/// at all, nothing to look for, or an allowed path that is not among the files (renamed
/// or moved).
pub fn identifiers_outside(
    files: &[(String, String)],
    idents: &[&str],
    allowed: &[&str],
) -> Vec<String> {
    if files.is_empty() {
        return vec!["no file scanned".to_string()];
    }
    if idents.is_empty() {
        return vec!["no identifier to look for".to_string()];
    }
    let mut findings = Vec::new();
    for path in allowed {
        if !files.iter().any(|(file, _)| file == path) {
            findings.push(format!(
                "{path}: allowed file is not among the scanned files"
            ));
        }
    }
    for (path, source) in files {
        if allowed.contains(&path.as_str()) {
            continue;
        }
        for ident in idents {
            if mentions_identifier(source, ident) {
                findings.push(format!("{path}: names {ident}"));
            }
        }
    }
    findings
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
    fn function_names_are_the_declared_functions() {
        let source = r#"
impl BodyParams {
    /// Not this: fn in_a_comment() {}
    pub fn with(mut self, key: BodyParamKey) -> Self {
        let text = "fn in_a_string() {}";
        self
    }
    pub(super) const fn clamp_to_contract (raw: f64) -> f64 { raw }
    fn  spaced<T>(value: T) {}
    fn
    on_the_next_line() {}
}
type Callback = fn(f64) -> f64;
fn free() { let often = 1; let fn_like = often; }
struct Often { fn_count: u8 }
"#;
        assert_eq!(
            function_names(source),
            [
                "with",
                "clamp_to_contract",
                "spaced",
                "on_the_next_line",
                "free"
            ]
        );
        assert_eq!(function_names("const X: u8 = 1;"), Vec::<String>::new());
    }

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
        let cases: [(&str, &[&str]); 7] = [
            ("pub struct P { initial: char }", &["char"]),
            ("pub struct P { letters: Vec<char> }", &["char"]),
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
            "fn f(text: &[u8]) -> usize { text.iter().filter(|b| b.is_ascii()).count() }\n",
            "fn g(s: Chars) -> usize { s.chars().count() }\n",
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
        assert_eq!(
            identifiers_outside(&files(&[("src/log.rs", "pub struct P;")]), &[], &[]),
            vec!["no identifier to look for".to_string()]
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
    fn uses_are_counted_in_every_form() {
        let cases = [
            (
                concat!(
                    "fn pop_next(&mut self) {}\n",
                    "fn run(&mut self) { while let Some(e) = self.pop_next(now) {} }\n",
                    "// self.pop_next(now) in a comment\n",
                    "let s = \".pop_next(\";\n",
                ),
                1,
            ),
            ("a.pop_next(1); b.pop_next(2);", 2),
            ("Self::pop_next(self, second)", 1),
            ("let f = Scheduler::pop_next;", 1),
            ("self.pop_next /* why */ (now)", 1),
            ("self.pop_next\n    (now)", 1),
            ("fn pop_next(&mut self) {}", 0),
            ("fn  pop_next (&mut self) {}", 0),
            ("fn\npop_next() {}", 0),
            ("self.my_pop_next(now); self.pop_next_event(now);", 0),
            ("defn pop_next", 1),
            ("", 0),
        ];
        for (source, expected) in cases {
            assert_eq!(count_uses(source, "pop_next"), expected, "{source}");
        }
        assert_eq!(count_uses("let c = s.input_ctx();", "input_ctx"), 1);
        assert_eq!(count_uses("pub fn input_ctx(&mut self) {}", "input_ctx"), 0);
    }

    #[test]
    fn the_year_literal_is_found_in_every_spelling() {
        for source in [
            "const Y: i64 = 31449600;",
            "const Y: i64 = 31_449_600;",
            "const Y: f64 = 31_449_600.0;",
            "let y = 31_449_600_i64;",
            "let y = 31449600i64;",
            "let y = -(99_999 * 31_449_600_i64);",
            "let y = 3_1449_600;",
        ] {
            assert!(names_year_literal(source), "{source}");
        }
        for source in [
            "let end = 31_449_599;",
            "let ten = 314_496_000;",
            "let x = 131_449_600;",
            "let x = 0.31449600;",
            "// 31_449_600 in a comment",
            "let s = \"31449600\";",
            "let x = a31449600;",
            "",
        ] {
            assert!(!names_year_literal(source), "{source}");
        }
    }

    /// A core as the time rules expect it, plus `extra`.
    fn timed_core(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut all = vec![
            (
                "crates/sim-core/src/time/scheduler.rs",
                "fn pop_next(&mut self) {}\nfn run(&mut self) { self.pop_next(now); }\npub fn input_ctx(&mut self) {}\n",
            ),
            (YEAR_LITERAL_FILE, "const YEAR_SECONDS: f64 = 31_449_600.0;"),
            (
                SUBMIT_FILE,
                "fn submit(&mut self) { let c = self.sched.input_ctx(); }",
            ),
        ];
        all.extend_from_slice(extra);
        files(&all)
    }

    #[test]
    fn a_clean_core_has_no_time_findings() {
        assert_eq!(core_time_findings(&timed_core(&[])), Vec::<String>::new());
    }

    /// Canary for the gate wiring: each time rule fires with the message the gate prints.
    #[test]
    fn the_time_rules_fire_on_a_broken_core() {
        let second_loop = timed_core(&[(
            "crates/sim-core/src/world/tick.rs",
            "fn tick(s: &mut S) { while let Some(e) = s.pop_next(now) {} }",
        )]);
        assert_eq!(
            core_time_findings(&second_loop),
            vec![
                "pop_next is used 2 times in sim-core/src, exactly one use is allowed (ADR-0002): crates/sim-core/src/time/scheduler.rs, crates/sim-core/src/world/tick.rs"
                    .to_string()
            ]
        );
        let path_call = timed_core(&[(
            "crates/sim-core/src/time/extra.rs",
            "fn drain(s: &mut S) { while Self::pop_next(s, now).is_some() {} }",
        )]);
        assert_eq!(
            core_time_findings(&path_call),
            vec![
                "pop_next is used 2 times in sim-core/src, exactly one use is allowed (ADR-0002): crates/sim-core/src/time/extra.rs, crates/sim-core/src/time/scheduler.rs"
                    .to_string()
            ]
        );
        let second_entry = timed_core(&[(
            "crates/sim-core/src/state/cheat.rs",
            "fn cheat(s: &mut S) { let c = s.input_ctx(); }",
        )]);
        assert_eq!(
            core_time_findings(&second_entry),
            vec![
                "input_ctx is used 2 times outside the time module, exactly one use in crates/sim-core/src/world/mod.rs is allowed (ADR-0002): crates/sim-core/src/state/cheat.rs, crates/sim-core/src/world/mod.rs"
                    .to_string()
            ]
        );
        let literal = timed_core(&[(
            "crates/sim-core/src/state/census.rs",
            "fn years(secs: i64) -> i64 { secs / 31_449_600 }",
        )]);
        assert_eq!(
            core_time_findings(&literal),
            vec![
                "crates/sim-core/src/state/census.rs: spells the seconds of a year as a number, only crates/sim-core/src/time/clock.rs may (ADR-0002)"
                    .to_string()
            ]
        );
    }

    #[test]
    fn missing_calls_are_time_findings_too() {
        // No call at all: the scan would otherwise pass on a renamed function.
        let found = core_time_findings(&files(&[("crates/sim-core/src/lib.rs", "pub mod time;")]));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("pop_next is used 0 times"),
            "{found:?}"
        );
        assert!(
            found[1].starts_with("input_ctx is used 0 times"),
            "{found:?}"
        );
        // The one call outside the time module must be in the submit file.
        let elsewhere = files(&[
            (
                "crates/sim-core/src/time/scheduler.rs",
                "fn run(&mut self) { self.pop_next(now); }",
            ),
            (
                "crates/sim-core/src/state/start.rs",
                "fn f(s: &mut S) { s.input_ctx(); }",
            ),
        ]);
        let found = core_time_findings(&elsewhere);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("input_ctx is used 1 times outside the time module")
                && found[0].ends_with(": crates/sim-core/src/state/start.rs"),
            "{found:?}"
        );
    }

    /// A core as the gate sees it: the allowed files plus `extra`.
    fn core(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut all = vec![
            (HASH_FILE, "use xxhash_rust::xxh3::xxh3_64;"),
            (
                "crates/sim-core/src/data_id.rs",
                "pub struct DataId(String);",
            ),
            (
                "crates/sim-core/src/hex_u64.rs",
                "pub fn parse(text: &str) {}",
            ),
            ("crates/sim-core/src/lib.rs", "pub mod log;"),
            (
                "crates/sim-core/src/rng/mod.rs",
                "use rand_chacha::ChaCha8Rng;",
            ),
            (
                "crates/sim-core/src/rng/tags.rs",
                "pub const ALL: &[(PurposeTag, &str)] = &[];",
            ),
            (
                "crates/sim-core/src/body_contract/generated.rs",
                "pub const fn as_str(self) -> &'static str { \"\" }",
            ),
        ];
        all.extend_from_slice(extra);
        files(&all)
    }

    #[test]
    fn a_clean_core_has_no_identifier_findings() {
        assert_eq!(core_identifier_findings(&core(&[])), Vec::<String>::new());
    }

    /// Canary for the gate wiring: each rule fires on a core that breaks it, with the
    /// message the gate prints.
    #[test]
    fn the_core_rules_fire_on_a_broken_core() {
        let text = core(&[(
            "crates/sim-core/src/log.rs",
            "pub enum LogPayload { Renamed { note: String } }",
        )]);
        assert_eq!(
            core_identifier_findings(&text),
            vec![
                "crates/sim-core/src/log.rs: names String (no text in the core outside the checked newtypes, ADR-0004 section 7)"
                    .to_string()
            ]
        );
        let hash = core(&[(
            "crates/sim-core/src/world.rs",
            "fn h(b: &[u8]) -> u64 { xxhash_rust::xxh3::xxh3_64(b) }",
        )]);
        assert_eq!(
            core_identifier_findings(&hash),
            vec![
                "crates/sim-core/src/world.rs: names xxhash_rust (only sim-core/src/hash.rs names the hash crate, ADR-0003 S5)"
                    .to_string()
            ]
        );
        // The tag names are allowed in their file only, not in the whole random module.
        let named = core(&[(
            "crates/sim-core/src/rng/select.rs",
            "pub fn name(tag: PurposeTag) -> &'static str { \"\" }",
        )]);
        assert_eq!(
            core_identifier_findings(&named),
            vec![
                "crates/sim-core/src/rng/select.rs: names str (no text in the core outside the checked newtypes, ADR-0004 section 7)"
                    .to_string()
            ]
        );
        // Of the text types the tag file and the generated contract types may name only
        // `str`: static names, never owned text.
        for file in [
            "crates/sim-core/src/rng/tags.rs",
            "crates/sim-core/src/body_contract/generated.rs",
        ] {
            let mut owned = core(&[]);
            for (path, source) in &mut owned {
                if path == file {
                    source.push_str("\npub type Label = alloc::string::String;");
                }
            }
            assert_eq!(
                core_identifier_findings(&owned),
                vec![format!(
                    "{file}: names String (no text in the core outside the checked newtypes, ADR-0004 section 7)"
                )]
            );
        }
        // The hand-written part of the contract module stays under the rule.
        let hand_written = core(&[(
            "crates/sim-core/src/body_contract/mod.rs",
            "pub fn name(key: BodyParamKey) -> &'static str { key.as_str() }",
        )]);
        assert_eq!(
            core_identifier_findings(&hand_written),
            vec![
                "crates/sim-core/src/body_contract/mod.rs: names str (no text in the core outside the checked newtypes, ADR-0004 section 7)"
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_core_without_its_allowed_files_is_a_finding() {
        let found =
            core_identifier_findings(&files(&[("crates/sim-core/src/lib.rs", "pub mod log;")]));
        assert_eq!(found.len(), 5, "{found:?}");
        for path in [
            HASH_FILE,
            "crates/sim-core/src/data_id.rs",
            "crates/sim-core/src/hex_u64.rs",
            "crates/sim-core/src/rng/tags.rs",
            "crates/sim-core/src/body_contract/generated.rs",
        ] {
            assert!(
                found.iter().any(|f| f.starts_with(&format!(
                    "{path}: allowed file is not among the scanned files"
                ))),
                "{path}: {found:?}"
            );
        }
        assert_eq!(core_identifier_findings(&[]).len(), 2, "one per rule");
    }

    const TAG_TYPE_SOURCE: &str = "pub struct PurposeTag(u64);\nimpl PurposeTag {\n    pub(super) const fn from_pinned_list(value: u64) -> PurposeTag {\n        PurposeTag(value)\n    }\n}\n";
    const RNG_MOD: &str = "mod tag;\npub mod tags;\npub use tag::PurposeTag;\nuse rand_chacha::ChaCha8Rng;\nuse rand_chacha::rand_core::{Rng, SeedableRng};\nfn key_words(key: [u8; 32]) -> u64 {\n    ChaCha8Rng::from_seed(key).next_u64()\n}\n";
    const TAGS: &str = "pub const A: PurposeTag = PurposeTag::from_pinned_list(1);\npub const B: PurposeTag = PurposeTag::from_pinned_list(2);\npub const ALL: &[(PurposeTag, &str)] = &[(A, \"A\"), (B, \"B\")];\n";
    const TAGS_GOLDEN: &str = "# comment\ntag 1 A\ntag 2 B\nretired 9\n";

    /// The workspace as the generator and tag rules see it: the random module with
    /// `rng_mod` as its `mod.rs`, plus `extra`.
    fn workspace(rng_mod: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut all = vec![
            (GENERATOR_FILE, rng_mod),
            (TAG_TYPE_FILE, TAG_TYPE_SOURCE),
            (TAGS_FILE, TAGS),
            ("crates/sim-core/src/lib.rs", "pub mod rng;"),
            ("crates/sim-cli/src/main.rs", "fn main() {}"),
        ];
        all.extend_from_slice(extra);
        files(&all)
    }

    const ONLY_RNG: &str = "(only sim-core/src/rng/ names the generator crate, ADR-0001 section 2)";
    const SELECT: &str = "crates/sim-core/src/rng/select.rs";

    #[test]
    fn a_clean_workspace_has_no_generator_findings() {
        assert_eq!(
            generator_findings(&workspace(RNG_MOD, &[])),
            Vec::<String>::new()
        );
    }

    /// ADR-0001 canary: an import of the generator crate outside the random module, in
    /// any Rust file of the workspace.
    #[test]
    fn the_generator_crate_outside_the_random_module_is_a_finding() {
        let outside = workspace(
            RNG_MOD,
            &[
                (
                    "crates/sim-core/src/world/mod.rs",
                    "use rand_chacha::ChaCha8Rng;\npub struct World { generator: ChaCha8Rng }",
                ),
                (
                    "crates/sim-core/src/rng_helper.rs",
                    "fn g() { let _ = rand_chacha::ChaCha8Rng::from_seed([0; 32]); }",
                ),
                (
                    "crates/sim-core/src/world/ctx.rs",
                    "// draws come from rng, never from rand_chacha\nfn f() { let _ = \"rand_chacha\"; }",
                ),
                (
                    "crates/sim-core/tests/draws.rs",
                    "use rand_chacha::ChaCha8Rng;",
                ),
                (
                    "crates/sim-cli/src/session.rs",
                    "use rand_chacha::rand_core::Rng;",
                ),
            ],
        );
        assert_eq!(
            generator_findings(&outside),
            vec![
                format!("crates/sim-core/src/world/mod.rs: names rand_chacha {ONLY_RNG}"),
                format!("crates/sim-core/src/rng_helper.rs: names rand_chacha {ONLY_RNG}"),
                format!("crates/sim-core/tests/draws.rs: names rand_chacha {ONLY_RNG}"),
                format!("crates/sim-cli/src/session.rs: names rand_chacha {ONLY_RNG}"),
            ]
        );
    }

    fn other_statement(path: &str, line: usize, statement: &str) -> String {
        format!(
            "{path}:{line}: names rand_chacha outside the two imports of {GENERATOR_FILE} (ADR-0001 section 2): {statement}"
        )
    }

    fn generator_count(n: usize, files: &str) -> String {
        format!(
            "ChaCha8Rng is named {n} times under {RNG_DIR}, exactly 2 uses in {GENERATOR_FILE} are allowed: the import and the one generator of key_words (ADR-0001 section 2): {files}"
        )
    }

    /// Review finding, both rounds: inside the random module the crate is named in two
    /// fixed import statements and nowhere else, whatever the spelling. A re-export or an
    /// alias of the crate would hand a generator to every other module.
    #[test]
    fn the_generator_crate_is_named_in_its_two_imports_only() {
        let reexport = workspace(
            RNG_MOD,
            &[(SELECT, "pub use rand_chacha::ChaCha8Rng as Generator;")],
        );
        assert_eq!(
            generator_findings(&reexport),
            vec![
                other_statement(SELECT, 1, "pub use rand_chacha::ChaCha8Rng as Generator;"),
                generator_count(3, &format!("{GENERATOR_FILE}, {SELECT}")),
            ]
        );
        // Visibility and crate name on different lines.
        let split = format!("{RNG_MOD}pub use\n    rand_chacha as engine;\n");
        assert_eq!(
            generator_findings(&workspace(&split, &[])),
            vec![other_statement(
                GENERATOR_FILE,
                10,
                "pub use rand_chacha as engine;"
            )]
        );
        // A private alias of the crate, re-exported under the alias.
        let alias = format!("{RNG_MOD}use rand_chacha as engine;\npub use engine::*;\n");
        assert_eq!(
            generator_findings(&workspace(&alias, &[])),
            vec![other_statement(
                GENERATOR_FILE,
                9,
                "use rand_chacha as engine;"
            )]
        );
        // An allowed import in another file of the module.
        let moved_import = workspace(
            RNG_MOD,
            &[(SELECT, "use rand_chacha::rand_core::{Rng, SeedableRng};")],
        );
        assert_eq!(
            generator_findings(&moved_import),
            vec![other_statement(
                SELECT,
                1,
                "use rand_chacha::rand_core::{Rng, SeedableRng};"
            )]
        );
        // A comment or a string inside the module names nothing.
        let quiet = format!(
            "{RNG_MOD}// rand_chacha keeps no state here\nconst NOTE: &[u8] = b\"\";\nfn note() {{ let _ = \"rand_chacha\"; }}\n"
        );
        assert_eq!(
            generator_findings(&workspace(&quiet, &[])),
            Vec::<String>::new()
        );
        // An attribute makes it another statement: the rule errs on the safe side.
        let attribute = RNG_MOD.replace(
            "use rand_chacha::ChaCha8Rng;",
            "#[cfg(test)]\nuse rand_chacha::ChaCha8Rng;",
        );
        assert_eq!(
            generator_findings(&workspace(&attribute, &[])),
            vec![other_statement(
                GENERATOR_FILE,
                5,
                "#[cfg(test)] use rand_chacha::ChaCha8Rng;"
            )]
        );
        // A path in an expression, inside a block.
        let path = format!(
            "{RNG_MOD}fn second(key: [u8; 32]) {{\n    let _ = 1;\n    let g = rand_chacha::ChaCha20Rng::from_seed(key);\n}}\n"
        );
        assert_eq!(
            generator_findings(&workspace(&path, &[])),
            vec![other_statement(
                GENERATOR_FILE,
                11,
                "let g = rand_chacha::ChaCha20Rng::from_seed(key);"
            )]
        );
    }

    /// The generator type itself: its import and the one generator, nothing else.
    #[test]
    fn the_random_module_does_not_hand_out_the_generator() {
        let alias = workspace(
            RNG_MOD,
            &[(SELECT, "pub type Generator = super::ChaCha8Rng;")],
        );
        assert_eq!(
            generator_findings(&alias),
            vec![generator_count(3, &format!("{GENERATOR_FILE}, {SELECT}"))]
        );
        let field = format!("{RNG_MOD}pub struct Stream {{ generator: ChaCha8Rng }}\n");
        assert_eq!(
            generator_findings(&workspace(&field, &[])),
            vec![generator_count(3, GENERATOR_FILE)]
        );
        // A glob import hides the type name: one use or none is a finding as well.
        let glob = RNG_MOD.replace("use rand_chacha::ChaCha8Rng;", "use rand_chacha::*;");
        assert_eq!(
            generator_findings(&workspace(&glob, &[])),
            vec![
                other_statement(GENERATOR_FILE, 4, "use rand_chacha::*;"),
                generator_count(1, GENERATOR_FILE),
            ]
        );
        let none = "mod tag;\npub mod tags;\npub use tag::PurposeTag;\n";
        assert_eq!(
            generator_findings(&workspace(none, &[])),
            vec![generator_count(0, "no file")]
        );
        // `include!` would hide code from every scan of the module.
        let included = workspace(RNG_MOD, &[(SELECT, "include!(\"generator.in\");")]);
        assert_eq!(
            generator_findings(&included),
            vec![format!(
                "{SELECT}: include! hides code of the random module from the scans (ADR-0001)"
            )]
        );
        let text = workspace(
            RNG_MOD,
            &[(SELECT, "const T: &[u8] = include_bytes!(\"t.bin\");")],
        );
        assert_eq!(generator_findings(&text), Vec::<String>::new());
    }

    /// A random module that moved or was renamed leaves the rules blind.
    #[test]
    fn a_workspace_without_the_random_module_is_a_finding() {
        let missing =
            format!("{RNG_DIR}: no file of the allowed directory is among the scanned files");
        let found = generator_findings(&files(&[("crates/sim-core/src/lib.rs", "pub mod log;")]));
        assert!(found.contains(&missing), "{found:?}");
        assert!(generator_findings(&[]).contains(&missing));
    }

    fn core_of(workspace: Vec<(String, String)>) -> Vec<(String, String)> {
        workspace
            .into_iter()
            .filter(|(path, _)| path.starts_with("crates/sim-core/src/"))
            .collect()
    }

    /// The core with `tags` as the source of the tag file.
    fn core_with_tags(tags: &str) -> Vec<(String, String)> {
        let mut core = core_of(workspace(RNG_MOD, &[]));
        for (path, source) in &mut core {
            if path == TAGS_FILE {
                *source = tags.to_string();
            }
        }
        core
    }

    #[test]
    fn a_clean_core_has_no_tag_findings() {
        let core = core_of(workspace(RNG_MOD, &[]));
        assert_eq!(tag_findings(&core, TAGS_GOLDEN), Vec::<String>::new());
        // The name in a comment or a string, and a longer name, are no calls.
        let named = core_of(workspace(
            RNG_MOD,
            &[(
                SELECT,
                "// from_pinned_list(7)\nfn not_from_pinned_list() { let _ = \"from_pinned_list\"; }",
            )],
        ));
        assert_eq!(tag_findings(&named, TAGS_GOLDEN), Vec::<String>::new());
    }

    /// Review finding, three rounds: a pinned tag exists only through `from_pinned_list`
    /// (the compiler sees to that: the field is private to `tag.rs`), and that name
    /// stands only in constants of one fixed form whose names and values are the tag
    /// lines of the golden. A tag beside the list, in any spelling, breaks one of the two.
    #[test]
    fn a_tag_outside_the_pinned_list_is_a_finding() {
        let unpinned = |line: &str| {
            format!(
                "{TAGS_FILE}: {line} is not pinned in tests/golden/rng_tags.txt (ADR-0001 section 5)"
            )
        };
        let other_form = |line: usize, statement: &str| {
            format!(
                "{TAGS_FILE}:{line}: from_pinned_list outside a constant of the form pub const NAME: PurposeTag = PurposeTag::from_pinned_list(N); (ADR-0001 section 5): {statement}"
            )
        };
        let extra = core_with_tags(&format!(
            "{TAGS}pub const C: PurposeTag = PurposeTag::from_pinned_list(3);\n"
        ));
        assert_eq!(tag_findings(&extra, TAGS_GOLDEN), vec![unpinned("tag 3 C")]);
        // Other spellings of one more tag.
        let braces = core_with_tags(&format!(
            "{TAGS}pub(crate) const C: PurposeTag = {{\n    PurposeTag::from_pinned_list(3)\n}};\n"
        ));
        assert_eq!(
            tag_findings(&braces, TAGS_GOLDEN),
            vec![other_form(5, "PurposeTag::from_pinned_list(3) };")]
        );
        let alias = core_with_tags(&format!(
            "{TAGS}use super::PurposeTag as T;\npub const C: T = T::from_pinned_list(3);\n"
        ));
        assert_eq!(
            tag_findings(&alias, TAGS_GOLDEN),
            vec![other_form(5, "pub const C: T = T::from_pinned_list(3);")]
        );
        let pointer = core_with_tags(&format!(
            "{TAGS}const BUILD: fn(u64) -> PurposeTag = PurposeTag::from_pinned_list;\n"
        ));
        assert_eq!(
            tag_findings(&pointer, TAGS_GOLDEN),
            vec![other_form(
                4,
                "const BUILD: fn(u64) -> PurposeTag = PurposeTag::from_pinned_list;"
            )]
        );
        // A wrapper keeps the number of uses but can build any tag.
        let wrapper = core_with_tags(
            "pub const fn make(value: u64) -> PurposeTag {\n    PurposeTag::from_pinned_list(value)\n}\npub const A: PurposeTag = make(1);\npub const B: PurposeTag = PurposeTag::from_pinned_list(2);\n",
        );
        assert_eq!(
            tag_findings(&wrapper, TAGS_GOLDEN),
            vec![
                // A statement runs to the next semicolon.
                other_form(
                    2,
                    "PurposeTag::from_pinned_list(value) } pub const A: PurposeTag = make(1);"
                ),
                format!(
                    "tests/golden/rng_tags.txt: tag 1 A has no constant in {TAGS_FILE} (ADR-0001 section 5)"
                ),
            ]
        );
        // A value that is not a plain number.
        let computed = core_with_tags(&format!(
            "{TAGS}pub const C: PurposeTag = PurposeTag::from_pinned_list(1 + 2);\n"
        ));
        assert_eq!(
            tag_findings(&computed, TAGS_GOLDEN),
            vec![other_form(
                4,
                "pub const C: PurposeTag = PurposeTag::from_pinned_list(1 + 2);"
            )]
        );
        // Something after the call.
        let chained = core_with_tags(&format!(
            "{TAGS}pub const C: PurposeTag = PurposeTag::from_pinned_list(2).next();\n"
        ));
        assert_eq!(
            tag_findings(&chained, TAGS_GOLDEN),
            vec![other_form(
                4,
                "pub const C: PurposeTag = PurposeTag::from_pinned_list(2).next();"
            )]
        );
        // A renumbered or renamed constant, and one that left the file but not the golden.
        let renumbered =
            core_with_tags(&TAGS.replace("from_pinned_list(2)", "from_pinned_list(7)"));
        assert_eq!(
            tag_findings(&renumbered, TAGS_GOLDEN),
            vec![
                unpinned("tag 7 B"),
                format!(
                    "tests/golden/rng_tags.txt: tag 2 B has no constant in {TAGS_FILE} (ADR-0001 section 5)"
                ),
            ]
        );
        let fewer = core_with_tags("pub const A: PurposeTag = PurposeTag::from_pinned_list(1);\n");
        assert_eq!(
            tag_findings(&fewer, TAGS_GOLDEN),
            vec![format!(
                "tests/golden/rng_tags.txt: tag 2 B has no constant in {TAGS_FILE} (ADR-0001 section 5)"
            )]
        );
        // The same tag twice.
        let twice = core_with_tags(&format!(
            "{TAGS}pub const B2: PurposeTag = PurposeTag::from_pinned_list(2);\n"
        ));
        assert_eq!(
            tag_findings(&twice, TAGS_GOLDEN),
            vec![unpinned("tag 2 B2")]
        );

        let elsewhere = |path: &str| {
            format!(
                "{path}: names from_pinned_list; only {TAGS_FILE} builds pinned tags (ADR-0001 section 5)"
            )
        };
        let select = core_of(workspace(
            RNG_MOD,
            &[(
                SELECT,
                "pub const C: PurposeTag = PurposeTag::from_pinned_list(3);",
            )],
        ));
        assert_eq!(tag_findings(&select, TAGS_GOLDEN), vec![elsewhere(SELECT)]);
        let in_mod =
            format!("{RNG_MOD}pub const C: PurposeTag = PurposeTag::from_pinned_list(3);\n");
        assert_eq!(
            tag_findings(&core_of(workspace(&in_mod, &[])), TAGS_GOLDEN),
            vec![elsewhere(GENERATOR_FILE)]
        );
        // The file of the type defines the function once and does not use it.
        let mut in_type = core_of(workspace(RNG_MOD, &[]));
        for (path, source) in &mut in_type {
            if path == TAG_TYPE_FILE {
                source.push_str("pub const C: PurposeTag = PurposeTag::from_pinned_list(3);\n");
            }
        }
        assert_eq!(
            tag_findings(&in_type, TAGS_GOLDEN),
            vec![elsewhere(TAG_TYPE_FILE)]
        );
    }

    #[test]
    fn a_core_without_the_tag_files_is_a_finding() {
        let found = tag_findings(
            &files(&[("crates/sim-core/src/lib.rs", "pub mod log;")]),
            "",
        );
        assert_eq!(
            found,
            vec![
                format!("{TAGS_FILE}: file of the tag rules is not among the scanned files"),
                format!("{TAG_TYPE_FILE}: file of the tag rules is not among the scanned files"),
            ]
        );
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
