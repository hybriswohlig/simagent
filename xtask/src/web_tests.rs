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
    assert_eq!(page.matches("<script").count(), 2);
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
    let host =
        |path: &str| format!("{path}: names another host (web/ talks only to its own server)");
    assert_eq!(
        source_findings("web/src/api.ts", "fetch(\"https://example.org/x\")"),
        [host("web/src/api.ts")]
    );
    for quoted in ["\"//example.org/x\"", "'//example.org'", "`//${h}`"] {
        assert_eq!(
            source_findings("web/src/a.ts", quoted),
            [host("web/src/a.ts")],
            "{quoted}"
        );
    }
    assert_eq!(
        source_findings("web/src/a.ts", "// a comment\nconst a = 1; // another\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        source_findings("web/src/a.ts", "const m = await import(\"./x.js\");"),
        ["web/src/a.ts: imports at run time (web/ loads only its own modules)"]
    );
}

/// The network is reached in `web/src/api.ts` alone.
#[test]
fn only_the_api_reaches_the_network() {
    let call = "await fetch(path);";
    assert_eq!(
        source_findings("web/src/api.ts", call),
        Vec::<String>::new()
    );
    for reach in [
        call,
        "new XMLHttpRequest()",
        "new WebSocket(u)",
        "new EventSource(u)",
        "navigator.sendBeacon(u)",
        "importScripts(u)",
        "const f = fetch;",
    ] {
        assert_eq!(
            source_findings("web/src/a.ts", reach),
            ["web/src/a.ts: reaches the network outside web/src/api.ts"],
            "{reach}"
        );
    }
    assert_eq!(
        source_findings("web/src/a.ts", "prefetch(); fetched"),
        Vec::<String>::new()
    );
}

/// Node's flags: the permission model, exactly the given reads, no code from strings; no
/// flag that would open writes, processes, workers, addons or the inspector.
#[test]
fn the_sandbox_reads_only_what_it_names() {
    let (a, b) = (Path::new("/w/dist"), Path::new("/w/test"));
    assert_eq!(
        sandbox(&[a, b]),
        [
            "--permission",
            "--allow-fs-read=/w/dist",
            "--allow-fs-read=/w/test",
            "--disallow-code-generation-from-strings"
        ]
    );
}

/// The tests run through `node --test`, in one process, in the directory of the first
/// read.
#[test]
fn the_tests_run_through_node_test() {
    let (test, dist) = (Path::new("/w/test"), Path::new("/w/dist"));
    assert_eq!(
        test_run(
            &[test, dist],
            &["a.test.mjs".to_owned(), "b.test.mjs".to_owned()]
        ),
        [
            "--permission",
            "--allow-fs-read=/w/test",
            "--allow-fs-read=/w/dist",
            "--disallow-code-generation-from-strings",
            "--test-isolation=none",
            "--test",
            "a.test.mjs",
            "b.test.mjs"
        ]
    );
}

#[test]
fn the_canary_names_the_files_it_may_not_touch() {
    let canary = canary(Path::new("/w/web"));
    for part in [
        "writeFileSync(\"/w/web/dist/canary\"",
        "readFileSync(\"/w/web/node_modules/typescript/package.json\")",
        "readFileSync(\"/w/web/dist/index.html\")",
        "execSync(\"true\")",
        "new Worker(",
        "new Function(",
        "process.env",
    ] {
        assert!(canary.contains(part), "{part}");
    }
    assert_eq!(canary.matches("test(").count(), 7);
}

/// A tree of `web/` below a fresh directory, with one source file, one test and old output.
fn tree(name: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("simagent-web-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        ("web/src/deep/a.ts", source),
        (
            "web/src/text.ts",
            "  \"payload core:birth\": \"wird geboren\",\n",
        ),
        ("tests/golden/log_codes.txt", "payload 8 core:birth\n"),
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

/// A symbolic link in `web/src` or `web/test`, or as one of them, stops the scan: what it
/// points at is no file of `web/`.
#[cfg(unix)]
#[test]
fn a_symbolic_link_stops_the_scan() {
    for link in ["web/src/deep/link.ts", "web/test/link.test.mjs"] {
        let root = tree("link", "");
        std::os::unix::fs::symlink("/etc/hosts", root.join(link)).expect("a link");
        assert_eq!(scan(&root), Err(format!("{link}: a symbolic link in web/")));
        std::fs::remove_dir_all(&root).expect("removed");
    }
    for dir in ["web", "web/src", "web/test"] {
        let root = tree("root-link", "");
        let moved = root.join("elsewhere");
        std::fs::rename(root.join(dir), &moved).expect("moved");
        std::os::unix::fs::symlink(&moved, root.join(dir)).expect("a link");
        assert_eq!(scan(&root), Err(format!("{dir}: a symbolic link in web/")));
        // The build stops before it removes anything: below a linked `web`, `dist` is
        // another directory's.
        assert_eq!(build(&root), Err(format!("{dir}: a symbolic link in web/")));
        let stale = if dir == "web" {
            "elsewhere/dist/stale.js"
        } else {
            "web/dist/stale.js"
        };
        assert!(root.join(stale).exists(), "{dir}");
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

/// Node runs with `PATH` alone and reports its failure (any Node: the runners of the CI
/// carry one even without the setup step).
#[test]
fn node_runs_with_an_empty_environment() {
    let root = tree("node", "");
    let script = |code: &str| ["-e".to_owned(), code.to_owned()];
    let alone = "const kept = Object.keys(process.env).filter((k) => k !== 'PATH' && k !== '__CF_USER_TEXT_ENCODING'); process.exit(kept.length === 0 ? 0 : 1)";
    assert_eq!(node(&root, &script(alone)), Ok(()));
    assert_eq!(
        node(&root, &script("process.exit(3)")),
        Err("`node -e process.exit(3)` failed".to_owned())
    );
    std::fs::remove_dir_all(&root).expect("removed");
}

/// Every code of the log codes needs a text under `<type> <name>` in `text.ts`; a retired
/// code too, comments and blank lines are none.
#[test]
fn every_log_code_needs_a_text() {
    let codes =
        "# a comment\n\npayload 0 core:birth\nreject 3 too_young\nretired applies_to 0 everyone\n";
    let texts = r#"export const CODES = {
  "payload core:birth": "wird geboren",
  "applies_to everyone": "alle",
};"#;
    assert_eq!(
        code_text_findings(codes, texts),
        ["web/src/text.ts: no text for the code `reject too_young` of tests/golden/log_codes.txt"]
    );
    let all = format!("{texts}\n  \"reject too_young\": \"zu jung\",");
    assert_eq!(code_text_findings(codes, &all), Vec::<String>::new());
    // A key only in a comment or as a value is none.
    let elsewhere = r#"// "reject too_young": no
const X = { a: "reject too_young" };"#;
    assert_eq!(
        code_text_findings("reject 3 too_young\n", elsewhere).len(),
        1
    );
    // The repository's own.
    assert_eq!(
        code_texts(&root()).map_err(|e| e.to_string()),
        Ok(Vec::new())
    );
}
