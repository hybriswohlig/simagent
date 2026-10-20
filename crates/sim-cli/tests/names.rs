//! Names (ADR-0021 section 3): the roots of the paternal chains of the dummy run by hand from
//! its golden pedigree; the deep pool chain of the repository's pool; and the same names in a
//! parent and its fork.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::Command;

use sim_cli::files::ScenarioFile;
use sim_core::ids::SimId;
use sim_core::input::Input;
use sim_core::lineage::LineageKey;
use sim_core::presentation::names::{first_name_index, surname_index};
use sim_core::rng::WorldSeed;
use sim_core::world::{World, YearOut};

const SCENARIO: &str = include_str!("../../../tests/scenarios/export_coverage.json");

#[test]
fn a_pool_fathers_fathers_give_the_root() {
    let file: ScenarioFile = serde_json::from_str(SCENARIO).expect("a scenario");
    let until = file.until;
    let scenario = sim_cli::resolve(file, sim_cli::data::core_data).expect("resolved");
    let mut world = World::new(sim_cli::data::core_data().expect("data"));
    world
        .submit(Input::Start(scenario.start))
        .expect("accepted");
    // The whole scenario: its sixty years show each case (one child of a pool pregnancy is
    // born in the town).
    world.advance_until(&mut YearOut::default(), until, &mut |_| false);
    let records = world.data().pool().expect("the pool");
    let pool_root = |mut key: LineageKey| {
        let mut steps = 0;
        while let Some(father) = records.record(key).and_then(|record| record.father) {
            key = father;
            steps += 1;
        }
        (key, steps)
    };
    let pedigree = world.pedigree();
    let from_pool = |sim| {
        (pedigree.persons.iter())
            .find(|p| p.sim == sim)
            .and_then(|p| p.pool.as_ref())
            .is_some()
    };
    // Each case of the definition, counted: a child of the town whose father came from the
    // pool (mixed parentage), a child of a pool pregnancy born in the town, a pool person whose
    // pool father has fathers among the records.
    let (mut mixed, mut imported, mut deep) = (0, 0, 0);
    for person in &pedigree.persons {
        let root = world.name_root(person.sim).expect("a root");
        let pool_father = person.pool.as_ref().and_then(|pool| pool.father_key);
        match (person.father, pool_father) {
            (Some(father), _) => {
                assert_eq!(Some(root), world.name_root(father), "{:?}", person.sim);
                mixed += usize::from(from_pool(father));
            }
            (None, Some(key)) => {
                let (expected, steps) = pool_root(key);
                assert_eq!(root, expected, "{:?}", person.sim);
                let born_here = person
                    .pool
                    .as_ref()
                    .is_some_and(|pool| pool.born_pool.is_none());
                imported += usize::from(born_here);
                deep += usize::from(steps > 0);
            }
            (None, None) => assert_eq!(root, person.lineage_key, "{:?}", person.sim),
        }
    }
    assert!(
        mixed > 0 && imported > 0 && deep > 0,
        "{mixed} {imported} {deep}"
    );
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("names")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("removes the old scratch directory");
    }
    std::fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

fn simagent(args: &[&std::ffi::OsStr]) {
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success(), "{args:?}");
}

fn loaded(dir: &Path) -> World {
    let bytes = std::fs::read(dir.join("state.save")).expect("the save");
    World::load(&bytes, sim_cli::data::core_data().expect("data"))
        .expect("loads")
        .0
}

fn key(hex: &str) -> LineageKey {
    serde_json::from_value(serde_json::Value::from(hex)).expect("a key")
}

/// The dummy run (`tests/golden/dummy_pedigree.json`, from the Python reference): the root of
/// each person's chain, read off that pedigree by hand. Founders are their own roots; the
/// children of sim 6 (who emigrated) and of sim 2 and sim 4 (who live) have their fathers'.
#[test]
fn the_dummy_runs_roots_by_hand() {
    let base = scratch("dummy");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let run = base.join("run");
    simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        run.as_os_str(),
    ]);
    let world = loaded(&run);
    let roots = [
        (1, "67a2a1a58cf8f47a"),
        (2, "d14e686636ce6fd8"),
        (3, "431e7431023db46a"),
        (4, "5ecfb3ffee518eab"),
        (5, "7223b2c13652bc0d"),
        (6, "8768613bccdc39a5"),
        (7, "8768613bccdc39a5"),
        (8, "d14e686636ce6fd8"),
        (9, "8768613bccdc39a5"),
        (10, "d14e686636ce6fd8"),
        (11, "8768613bccdc39a5"),
        (12, "d14e686636ce6fd8"),
        (13, "5ecfb3ffee518eab"),
    ];
    for (sim, root) in roots {
        let sim = SimId::try_from(sim).expect("an id");
        assert_eq!(world.name_root(sim), Some(key(root)), "{sim:?}");
    }
}

/// A fork names everybody it shares with its parent as the parent does: the same roots and
/// the same indices, by lineage key.
#[test]
fn a_fork_names_as_its_parent() {
    let base = scratch("fork");
    let scenario =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/export_coverage.json");
    let (parent, fork) = (base.join("parent"), base.join("fork"));
    let until = (20 * 31_449_600 - 1).to_string();
    simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        parent.as_os_str(),
        "--until".as_ref(),
        until.as_ref(),
    ]);
    simagent(&[
        "fork".as_ref(),
        parent.as_os_str(),
        "--from-year".as_ref(),
        "10".as_ref(),
        "--out".as_ref(),
        fork.as_os_str(),
    ]);
    let len = |n| NonZeroU64::new(n).expect("not zero");
    let names = |world: &World| -> BTreeMap<LineageKey, (LineageKey, u64, u64)> {
        let seed = WorldSeed::new(world.state().world_seed().expect("started"));
        (world.pedigree().persons.iter())
            .map(|person| {
                let root = world.name_root(person.sim).expect("a root");
                let first = first_name_index(seed, person.lineage_key, person.sex, len(200));
                (
                    person.lineage_key,
                    (root, first, surname_index(seed, root, len(300))),
                )
            })
            .collect()
    };
    let (ours, theirs) = (names(&loaded(&parent)), names(&loaded(&fork)));
    let shared: Vec<_> = ours.keys().filter(|k| theirs.contains_key(k)).collect();
    assert!(shared.len() > 100, "{}", shared.len());
    for k in shared {
        assert_eq!(ours[k], theirs[k], "{k:?}");
    }
}
