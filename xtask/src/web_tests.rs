//! The scans of the web build on the repository's `web/` and on broken copies (canaries:
//! each rule fires with the message the gate prints).

use super::*;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

fn page() -> String {
    crate::read(&root().join("web/static/index.html")).expect("the page")
}

#[test]
fn the_running_node_has_the_pinned_major() {
    assert_eq!(node_major("v24.19.0\n", "24\n"), Ok(()));
    assert_eq!(node_major("v24.0.0", "24"), Ok(()));
    for printed in ["v22.11.0", "v240.1.0", "24.19.0", "", "v2.4.0"] {
        assert_eq!(
            node_major(printed, "24\n"),
            Err(format!(
                "node_version: node {printed}, web/.nvmrc wants major 24"
            )),
            "{printed:?}"
        );
    }
}

#[test]
fn the_repository_web_scans_clean() {
    assert_eq!(scan(&root()), Ok(Vec::new()));
}

#[test]
fn the_import_map_names_exactly_the_vendored_modules() {
    let page = page();
    assert_eq!(import_map_findings(&page), Vec::<String>::new());
    let expected = "web/static/index.html: the import map is not exactly {\"imports\":{\"preact\":\"./vendor/preact.mjs\",\"preact/hooks\":\"./vendor/hooks.mjs\",\"preact/jsx-runtime\":\"./vendor/jsx-runtime.mjs\"}}";
    let broken = [
        page.replace("./vendor/preact.mjs", "https://esm.sh/preact"),
        page.replace("./vendor/hooks.mjs", "./vendor/other.mjs"),
        page.replace(
            "\"preact/hooks\": \"./vendor/hooks.mjs\",",
            "\"preact/hooks\": \"./vendor/hooks.mjs\",\n\"left-pad\": \"./vendor/left-pad.mjs\",",
        ),
        page.replace("\"preact/hooks\": \"./vendor/hooks.mjs\",", ""),
        page.replace("\"imports\": {", "\"scopes\": {}, \"imports\": {"),
        page.replace(
            "<script type=\"module\"",
            "<script type=\"importmap\">{}</script><script type=\"module\"",
        ),
        page.replace("<script type=\"importmap\">", "<script>"),
        page.replace("\"imports\": {", "\"imports\": {,"),
    ];
    for page in broken {
        let found = import_map_findings(&page);
        assert_eq!(found.first().map(String::as_str), Some(expected), "{page}");
    }
}

#[test]
fn the_page_refers_only_to_its_own_files() {
    let page = page();
    let broken = [
        (
            page.replace("src=\"./main.js\"", "src=\"https://example.org/main.js\""),
            "src=\"https://example.org/main.js\"",
        ),
        (
            page.replace("src=\"./main.js\"", "src=\"//example.org/main.js\""),
            "src=\"//example.org/main.js\"",
        ),
        (
            page.replace("src=\"./main.js\"", "src=\"./../main.js\""),
            "src=\"./../main.js\"",
        ),
        (
            page.replace("<title>", "<link rel=\"icon\" href=\"/icon.png\"><title>"),
            "href=\"/icon.png\"",
        ),
    ];
    for (page, attribute) in broken {
        assert_eq!(
            import_map_findings(&page),
            [format!(
                "web/static/index.html: {attribute} is not a file of web/dist"
            )],
            "{attribute}"
        );
    }
}

#[test]
fn web_names_no_host_and_computes_only_in_render() {
    let clean = "export const a = 1;\n";
    assert_eq!(source_findings("web/src/a.ts", clean), Vec::<String>::new());
    assert_eq!(
        source_findings("web/src/render/a.ts", "const r = Math.sqrt(2);"),
        Vec::<String>::new()
    );
    assert_eq!(
        source_findings("web/src/a.ts", "const r = Math.round(2.5);"),
        ["web/src/a.ts: uses Math. outside web/src/render/ (web/ computes nothing, ADR-0021)"]
    );
    assert_eq!(
        source_findings("web/src/rendering.ts", "Math.max(1, 2)"),
        [
            "web/src/rendering.ts: uses Math. outside web/src/render/ (web/ computes nothing, ADR-0021)"
        ]
    );
    assert_eq!(
        source_findings("web/static/x.css", "Math.max"),
        Vec::<String>::new()
    );
    assert_eq!(
        source_findings("web/src/api.ts", "fetch(\"https://example.org/x\")"),
        ["web/src/api.ts: names another host (web/ talks only to its own server)"]
    );
    assert_eq!(
        source_findings("web/static/index.html", "ws://localhost:1"),
        ["web/static/index.html: names another host (web/ talks only to its own server)"]
    );
}

/// A tree of `web/` below a fresh directory: the repository's page and one source file.
fn tree(name: &str, page: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("simagent-web-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        ("web/static/index.html", page),
        ("web/src/deep/a.ts", source),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, text).expect("written");
    }
    root
}

/// The scan walks `web/src` and `web/static` below the root, and the build stops on its
/// findings before it runs Node.
#[test]
fn the_scan_walks_the_tree_and_stops_the_build() {
    let page = page().replace("src=\"./main.js\"", "src=\"/main.js\"");
    let root = tree("scan", &page, "export const r = Math.PI;\n");
    let findings = [
        "web/static/index.html: src=\"/main.js\" is not a file of web/dist",
        "web/src/deep/a.ts: uses Math. outside web/src/render/ (web/ computes nothing, ADR-0021)",
    ];
    assert_eq!(scan(&root), Ok(findings.map(str::to_owned).to_vec()));
    assert_eq!(build(&root), Err(format!("\n  {}", findings.join("\n  "))));
    std::fs::remove_dir_all(&root).expect("removed");
}

#[test]
fn copy_makes_the_directory_and_run_reports_failure() {
    let root = tree("copy", "", "");
    let to = root.join("web/dist/vendor/a.ts");
    assert_eq!(copy(&root.join("web/src/deep/a.ts"), &to), Ok(()));
    assert_eq!(std::fs::read_to_string(&to).ok().as_deref(), Some(""));
    assert!(copy(&root.join("web/src/missing.ts"), &to).is_err());
    assert_eq!(run(&root, "true", &[]), Ok(()));
    assert_eq!(
        run(&root, "false", &["x"]),
        Err("`false x` failed".to_owned())
    );
    std::fs::remove_dir_all(&root).expect("removed");
}
