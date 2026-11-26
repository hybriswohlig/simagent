//! The classifier of the short gate, against the path list of the rule: `web/src/render/`,
//! the sources directly under `web/src/`, `web/test/` and `web/figure/` are display files;
//! everything else, an empty diff and a missing merge base take the full gate.

use super::*;

#[test]
fn display_files_take_the_short_gate() {
    let changed = [
        "web/src/render/figure.ts",
        "web/src/render/shaders/hair.ts",
        "web/src/person.tsx",
        "web/src/api.ts",
        "web/src/style.css",
        "web/test/figure-map.test.mjs",
        "web/test/fixtures/melanin.json",
        "web/figure/body.glb",
    ];
    assert!(short_gate(Some(&changed)));
}

#[test]
fn a_generated_file_takes_the_full_gate() {
    assert!(!short_gate(Some(&[
        "web/src/render/figure.ts",
        "web/src/generated/body.ts"
    ])));
}

#[test]
fn a_smoke_file_takes_the_full_gate() {
    assert!(!short_gate(Some(&["web/smoke/smoke.mjs"])));
}

#[test]
fn the_build_files_of_web_take_the_full_gate() {
    for path in [
        "web/package.json",
        "web/package-lock.json",
        "web/tsconfig.json",
        "web/allowlist.json",
        "web/.nvmrc",
    ] {
        assert!(!short_gate(Some(&[path])), "{path}");
    }
}

#[test]
fn a_file_outside_web_takes_the_full_gate() {
    for path in [
        "crates/sim-core/src/lib.rs",
        "xtask/src/main.rs",
        ".github/workflows/gate.yml",
        "contracts/body.json",
        "README.md",
    ] {
        assert!(
            !short_gate(Some(&["web/src/render/figure.ts", path])),
            "{path}"
        );
    }
}

/// Without rename detection a rename is its old path and its new one.
#[test]
fn a_rename_out_of_web_takes_the_full_gate() {
    assert!(!short_gate(Some(&[
        "web/src/render/color.ts",
        "crates/sim-cli/src/color.ts"
    ])));
    assert!(!short_gate(Some(&[
        "web/smoke/helper.mjs",
        "web/test/helper.mjs"
    ])));
}

#[test]
fn other_kinds_of_file_under_web_src_take_the_full_gate() {
    for path in [
        "web/src/data.json",
        "web/src/other/deep.ts",
        "web/src/render",
        "web/src/",
        "web/testing/x.mjs",
        "web/figures/body.glb",
        "\"web/src/render/f\\303\\244.ts\"",
    ] {
        assert!(!short_gate(Some(&[path])), "{path}");
    }
}

#[test]
fn an_empty_diff_takes_the_full_gate() {
    assert!(!short_gate(Some(&[])));
}

#[test]
fn a_missing_merge_base_takes_the_full_gate() {
    assert!(!short_gate(None));
}
