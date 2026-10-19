//! The web build: its page, its Node check and its scans, on the repository's `web/` and on
//! broken trees (canaries: each rule fires with the message the gate prints).

use super::*;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
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

/// The page maps exactly the vendored modules, each to its file in `web/dist`, and loads
/// only `./main.js`.
#[test]
fn the_page_maps_exactly_the_vendored_modules() {
    let page = page();
    let (_, after) = page
        .split_once("<script type=\"importmap\">")
        .expect("a map");
    let (map, rest) = after.split_once("</script>").expect("its end");
    let map: serde_json::Value = serde_json::from_str(map).expect("JSON");
    assert_eq!(
        map,
        serde_json::json!({"imports": {
            "preact": "./vendor/preact.mjs",
            "preact/hooks": "./vendor/hooks.mjs",
            "preact/jsx-runtime": "./vendor/jsx-runtime.mjs"
        }})
    );
    assert_eq!(page.matches("importmap").count(), 1);
    assert_eq!(page.matches("src=").count(), 1);
    assert!(rest.contains("<script type=\"module\" src=\"./main.js\"></script>"));
    assert!(!page.contains("href=") && !page.contains("://"));
}

#[test]
fn the_repository_web_scans_clean() {
    assert_eq!(scan(&root()), Ok(Vec::new()));
}

#[test]
fn web_names_no_host_and_computes_only_in_render() {
    let math = |path: &str| {
        format!("{path}: uses Math outside web/src/render/ (web/ computes nothing, ADR-0021)")
    };
    for clean in [
        "export const a = 1;\n",
        "const myMath = 1; const MathML = 2; const $Math_ = 3;",
    ] {
        assert_eq!(source_findings("web/src/a.ts", clean), Vec::<String>::new());
    }
    assert_eq!(
        source_findings("web/src/render/a.ts", "const r = Math.sqrt(2);"),
        Vec::<String>::new()
    );
    for computing in [
        "const r = Math.round(2.5);",
        "const { round } = Math;",
        "const M = Math\n",
        "globalThis[\"Math\"]",
        "Math",
    ] {
        assert_eq!(
            source_findings("web/src/a.ts", computing),
            [math("web/src/a.ts")],
            "{computing}"
        );
    }
    assert_eq!(
        source_findings("web/src/rendering.ts", "Math.max(1, 2)"),
        [math("web/src/rendering.ts")]
    );
    assert_eq!(
        source_findings("web/src/api.ts", "fetch(\"https://example.org/x\")"),
        ["web/src/api.ts: names another host (web/ talks only to its own server)"]
    );
}

/// A tree of `web/` below a fresh directory, with one source file, one test and old output.
fn tree(name: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("simagent-web-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        ("web/src/deep/a.ts", source),
        ("web/test/a.test.mjs", ""),
        ("web/dist/stale.js", ""),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, text).expect("written");
    }
    root
}

/// The scan walks `web/src` below the root; the build removes old output first and stops on
/// the findings before it runs Node.
#[test]
fn the_scan_walks_the_tree_and_stops_the_build() {
    let root = tree("scan", "export const r = Math.PI;\n");
    let finding =
        "web/src/deep/a.ts: uses Math outside web/src/render/ (web/ computes nothing, ADR-0021)";
    assert_eq!(scan(&root), Ok(vec![finding.to_owned()]));
    assert_eq!(build(&root), Err(format!("\n  {finding}")));
    assert!(!root.join("web/dist").exists());
    std::fs::remove_dir_all(&root).expect("removed");
}

/// A symbolic link in `web/src` or `web/test` stops the scan: what it points at is no file
/// of `web/`.
#[cfg(unix)]
#[test]
fn a_symbolic_link_stops_the_scan() {
    for link in ["web/src/deep/link.ts", "web/test/link.test.mjs"] {
        let root = tree("link", "");
        std::os::unix::fs::symlink("/etc/hosts", root.join(link)).expect("a link");
        assert_eq!(scan(&root), Err(format!("{link}: a symbolic link in web/")));
        std::fs::remove_dir_all(&root).expect("removed");
    }
}

#[test]
fn add_refuses_what_tsc_wrote_and_run_reports_failure() {
    let root = tree("add", "");
    let to = root.join("web/dist/vendor/a.mjs");
    assert_eq!(add(&to, b"x"), Ok(()));
    assert_eq!(std::fs::read(&to).ok().as_deref(), Some(&b"x"[..]));
    assert_eq!(
        add(&to, b"y"),
        Err(format!("{}: tsc wrote it already", to.display()))
    );
    assert_eq!(run(&root, "true", &[]), Ok(()));
    assert_eq!(
        run(&root, "false", &["x"]),
        Err("`false x` failed".to_owned())
    );
    std::fs::remove_dir_all(&root).expect("removed");
}
