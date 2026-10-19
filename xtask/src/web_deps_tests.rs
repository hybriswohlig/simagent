//! The npm guard on fixture lockfiles (ADR-0021, Abnahmetest): the clean one passes, and
//! each broken one is red with exactly its code. The fixtures follow the format of npm's
//! lockfile version 3 only; names, versions and hashes are arbitrary.

use serde_json::{Value, json};

use super::*;

const NVMRC: &str = "24\n";

fn package() -> Value {
    json!({
        "name": "simagent-web",
        "private": true,
        "engines": {"node": "24.x"},
        "devDependencies": {"typescript": "6.0.3"}
    })
}

fn entry(version: &str, license: &str) -> Value {
    json!({
        "version": version,
        "resolved": format!("https://registry.npmjs.org/x/-/x-{version}.tgz"),
        "integrity": "sha512-AAAA",
        "dev": true,
        "license": license
    })
}

fn lockfile() -> Value {
    json!({
        "name": "simagent-web",
        "lockfileVersion": 3,
        "requires": true,
        "packages": {
            "": {
                "name": "simagent-web",
                "engines": {"node": "24.x"},
                "devDependencies": {"typescript": "6.0.3"}
            },
            "node_modules/typescript": entry("6.0.3", "Apache-2.0"),
            "node_modules/typescript/node_modules/tiny": entry("1.0.0", "(MIT OR GPL-3.0-only)")
        }
    })
}

fn allowlist() -> Value {
    json!({
        "format": "simagent.web.allowlist",
        "version": 1,
        "packages": [
            {"name": "typescript", "license": "Apache-2.0"},
            {"name": "tiny", "license": "(MIT OR GPL-3.0-only)"}
        ]
    })
}

/// The codes of the problems of the files after `change`.
fn codes(
    change: impl FnOnce(&mut String, &mut Value, &mut Value, &mut Value),
) -> Vec<&'static str> {
    let (mut nvmrc, mut package, mut lockfile, mut allowlist) =
        (NVMRC.to_owned(), package(), lockfile(), allowlist());
    change(&mut nvmrc, &mut package, &mut lockfile, &mut allowlist);
    let (package, lockfile, allowlist) = (
        package.to_string(),
        lockfile.to_string(),
        allowlist.to_string(),
    );
    let files = Files {
        nvmrc: &nvmrc,
        package: &package,
        lockfile: &lockfile,
        allowlist: &allowlist,
    };
    let problems = check(&files).expect("readable files");
    problems.iter().map(|p| p.code).collect()
}

fn add(lockfile: &mut Value, key: &str, entry: Value) {
    lockfile["packages"][key] = entry;
}

fn list(allowlist: &mut Value, name: &str, license: &str) {
    let packages = allowlist["packages"].as_array_mut().expect("a list");
    packages.push(json!({"name": name, "license": license}));
}

#[test]
fn the_clean_fixture_passes() {
    assert_eq!(codes(|_, _, _, _| {}), Vec::<&str>::new());
}

#[test]
fn a_package_off_the_list_is_red() {
    let found = codes(|_, _, lockfile, _| {
        add(lockfile, "node_modules/left-pad", entry("1.3.0", "MIT"));
    });
    assert_eq!(found, ["unlisted_package"]);
    // An alias is checked under the name it installs.
    let found = codes(|_, _, lockfile, _| {
        let mut alias = entry("1.3.0", "MIT");
        alias["name"] = json!("left-pad");
        add(lockfile, "node_modules/typescript-alias", alias);
    });
    assert_eq!(found, ["unlisted_package"]);
}

#[test]
fn a_licence_off_the_list_is_red() {
    let found = codes(|_, _, lockfile, allowlist| {
        add(
            lockfile,
            "node_modules/copyleft",
            entry("2.0.0", "GPL-3.0-only"),
        );
        list(allowlist, "copyleft", "GPL-3.0-only");
    });
    assert_eq!(found, ["license_not_allowed"]);
    let found = codes(|_, _, lockfile, allowlist| {
        add(
            lockfile,
            "node_modules/both",
            entry("2.0.0", "MIT AND GPL-3.0-only"),
        );
        list(allowlist, "both", "MIT AND GPL-3.0-only");
    });
    assert_eq!(found, ["license_not_allowed"]);
    let found = codes(|_, _, lockfile, allowlist| {
        let mut bare = entry("2.0.0", "MIT");
        bare.as_object_mut().expect("an entry").remove("license");
        add(lockfile, "node_modules/bare", bare);
        list(allowlist, "bare", "MIT");
    });
    assert_eq!(found, ["license_changed", "license_not_allowed"]);
}

#[test]
fn a_licence_other_than_the_listed_one_is_red() {
    let found = codes(|_, _, lockfile, _| {
        lockfile["packages"]["node_modules/typescript"]["license"] = json!("MIT");
    });
    assert_eq!(found, ["license_changed"]);
}

#[test]
fn an_install_script_is_red() {
    let found = codes(|_, _, lockfile, _| {
        lockfile["packages"]["node_modules/typescript"]["hasInstallScript"] = json!(true);
    });
    assert_eq!(found, ["install_script"]);
}

#[test]
fn a_package_from_elsewhere_is_red() {
    let found = codes(|_, _, lockfile, _| {
        lockfile["packages"]["node_modules/typescript"]["resolved"] =
            json!("https://example.org/typescript-6.0.3.tgz");
    });
    assert_eq!(found, ["not_from_the_registry"]);
    let found = codes(|_, _, lockfile, _| {
        lockfile["packages"]["node_modules/typescript"]["integrity"] = json!("sha1-AAAA");
    });
    assert_eq!(found, ["not_from_the_registry"]);
}

#[test]
fn a_wrong_node_is_red() {
    let found = codes(|_, _, lockfile, _| {
        lockfile["packages"][""]["engines"]["node"] = json!("22.x");
    });
    assert_eq!(found, ["node_pin"]);
    let found = codes(|_, package, _, _| {
        package["engines"]["node"] = json!(">=24");
    });
    assert_eq!(found, ["node_pin"]);
    for nvmrc in ["v24", "24.19.0", "", "lts/*"] {
        let found = codes(|pin, _, _, _| *pin = nvmrc.to_owned());
        assert_eq!(found, ["node_pin"], "{nvmrc:?}");
    }
    let found = codes(|pin, _, _, _| *pin = "22".to_owned());
    assert_eq!(found, ["node_pin", "node_pin"]);
}

#[test]
fn another_lockfile_version_is_red() {
    let found = codes(|_, _, lockfile, _| lockfile["lockfileVersion"] = json!(2));
    assert_eq!(found, ["lockfile_version"]);
}

#[test]
fn every_problem_is_named() {
    let found = codes(|_, _, lockfile, _| {
        let mut bad = entry("0.1.0", "GPL-2.0");
        bad["hasInstallScript"] = json!(true);
        bad["resolved"] = json!("git+ssh://example.org/bad.git");
        add(lockfile, "node_modules/bad", bad);
    });
    assert_eq!(
        found,
        [
            "unlisted_package",
            "license_not_allowed",
            "install_script",
            "not_from_the_registry"
        ]
    );
}

#[test]
fn spdx_expressions_pass_as_the_adr_says() {
    for passes in [
        "MIT",
        "0BSD",
        "(MIT OR GPL-3.0-only)",
        "GPL-3.0-only OR ISC",
        "MIT AND ISC",
        "(MIT AND (ISC OR GPL-2.0))",
        "GPL-2.0 OR MIT AND BSD-3-Clause",
    ] {
        assert!(license_allowed(passes), "{passes}");
    }
    for fails in [
        "",
        "GPL-3.0-only",
        "MIT AND GPL-3.0-only",
        "MIT AND (GPL-2.0 OR LGPL-2.1)",
        "(MIT OR GPL-2.0) AND GPL-3.0-only",
        "Apache-2.0 WITH LLVM-exception",
        "MIT+",
        "mit",
        "LicenseRef-MIT",
        "(MIT",
        "MIT)",
        "MIT OR",
        "AND MIT",
        "MIT ISC",
        "()",
        "MIT OR )",
        "MIT OR AND",
        "MIT OR OR",
        "MIT OR WITH",
    ] {
        assert!(!license_allowed(fails), "{fails}");
    }
}

#[test]
fn an_unreadable_file_or_another_allowlist_is_an_error() {
    let (package, lockfile) = (package().to_string(), lockfile().to_string());
    let mut other = allowlist();
    other["version"] = json!(2);
    let other = other.to_string();
    let twice = json!({
        "format": "simagent.web.allowlist",
        "version": 1,
        "packages": [
            {"name": "typescript", "license": "Apache-2.0"},
            {"name": "typescript", "license": "MIT"}
        ]
    })
    .to_string();
    let nameless = json!({
        "format": "simagent.web.allowlist",
        "version": 1,
        "packages": [{"license": "MIT"}]
    })
    .to_string();
    for (allowlist, lockfile, error) in [
        (
            &other,
            &lockfile,
            "web/allowlist.json: not simagent.web.allowlist version 1",
        ),
        (&twice, &lockfile, "web/allowlist.json: typescript twice"),
        (
            &nameless,
            &lockfile,
            "web/allowlist.json: an entry without name or license",
        ),
        (&other, &"{".to_owned(), "web/package-lock.json: "),
        (&other, &"{}".to_owned(), "web/allowlist.json: not"),
    ] {
        let files = Files {
            nvmrc: NVMRC,
            package: &package,
            lockfile,
            allowlist,
        };
        let found = check(&files).expect_err("an error");
        assert!(found.starts_with(error), "{found}");
    }
    let empty = allowlist().to_string();
    let files = Files {
        nvmrc: NVMRC,
        package: &package,
        lockfile: "{}",
        allowlist: &empty,
    };
    assert_eq!(
        check(&files).expect_err("no packages"),
        "web/package-lock.json: no packages"
    );
}

#[test]
fn another_manifest_is_red() {
    let paths: Vec<String> = [
        "web/package.json",
        "web/package-lock.json",
        "web/src/main.ts",
        "web/npm-shrinkwrap.json",
        "package.json",
        "tools/yarn.lock",
        "web/tools/package.json",
        "web/package.json.orig",
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    let found: Vec<String> = stray_manifests(&paths)
        .iter()
        .map(Problem::to_string)
        .collect();
    assert_eq!(
        found,
        [
            "another_manifest: web/npm-shrinkwrap.json",
            "another_manifest: package.json",
            "another_manifest: tools/yarn.lock",
            "another_manifest: web/tools/package.json"
        ]
    );
}

/// The command reads `web/` below its root and walks the rest of the tree for another
/// manifest, skipping `.git`, `target`, `docs` and every `node_modules`.
#[test]
fn the_command_walks_the_tree() {
    let root = std::env::temp_dir().join(format!("simagent-web-deps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, text).expect("written");
    };
    write("web/.nvmrc", NVMRC);
    write("web/package.json", &package().to_string());
    write("web/package-lock.json", &lockfile().to_string());
    write("web/allowlist.json", &allowlist().to_string());
    assert_eq!(command(&root), Ok(()));
    for skipped in [
        ".git/package.json",
        "target/debug/package.json",
        "docs/package.json",
        "web/node_modules/typescript/package.json",
        "tools/node_modules/package.json",
    ] {
        write(skipped, "{}");
    }
    assert_eq!(command(&root), Ok(()));
    write("tools/deep/yarn.lock", "");
    write("web/npm-shrinkwrap.json", "{}");
    let found = command(&root).expect_err("two strays");
    assert_eq!(
        found,
        "\n  another_manifest: tools/deep/yarn.lock\n  another_manifest: web/npm-shrinkwrap.json"
    );
    std::fs::remove_dir_all(&root).expect("removed");
}

/// The repository's own `web/` passes: the gate step.
#[test]
fn the_repository_web_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace");
    assert_eq!(command(root), Ok(()));
}
