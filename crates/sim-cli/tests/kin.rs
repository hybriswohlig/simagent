//! Kinship words of a run (ADR-0021 section 3, Grok's K1 to K6) on the dummy run, whose
//! pedigree (`tests/golden/dummy_pedigree.json`, from the Python reference) has parents,
//! siblings, grandparents, an uncle and a foster child: sim 3, a founder taken in by sims 5
//! and 6. Sim 1 died and sim 6 emigrated, so both read from the archive. The words are read
//! off that pedigree by hand, `r` from φ of each relation (1/4 for a parent or a sibling,
//! 1/8 for a grandparent or an uncle, 1/2 for a founder with itself).

use std::path::Path;
use std::process::Command;

use sim_core::ids::SimId;
use sim_core::presentation::kin::{Foster, KinWord};
use sim_core::world::World;

#[test]
fn the_dummy_runs_words_by_hand() {
    let run = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("kin")
        .join("run");
    if run.exists() {
        std::fs::remove_dir_all(&run).expect("removes the old run");
    }
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), run.as_os_str()])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let bytes = std::fs::read(run.join("state.save")).expect("the save");
    let world = World::load(&bytes, sim_cli::data::core_data().expect("data"))
        .expect("loads")
        .0;
    let sim = |n: u64| SimId::try_from(n).expect("an id");
    use KinWord::*;
    let cases = [
        (7, 6, Father, None, 50.0),
        (6, 7, Son, None, 50.0),
        (7, 9, Brother, None, 50.0),
        (8, 10, Brother, None, 50.0),
        (13, 4, Father, None, 50.0),
        (13, 2, Grandfather, None, 25.0),
        (13, 1, Grandmother, None, 25.0),
        (13, 10, Uncle, None, 25.0),
        (10, 13, Nephew, None, 25.0),
        (7, 8, Unrelated, None, 0.0),
        (3, 5, Unrelated, Some(Foster::FosterMother), 0.0),
        (3, 6, Unrelated, Some(Foster::FosterFather), 0.0),
        (5, 3, Unrelated, Some(Foster::FosterSon), 0.0),
        (3, 3, Myself, None, 100.0),
    ];
    for (a, b, word, foster, r) in cases {
        let view = world.kin_view(sim(a), sim(b)).expect("both known");
        assert_eq!((view.word, view.foster), (word, foster), "{a} to {b}");
        assert!(
            view.r_percent.to_bits() == f64::to_bits(r),
            "{a} to {b}: {}",
            view.r_percent
        );
    }
    assert!(world.kin_view(sim(1), sim(999)).is_none());
    assert!(world.kin_view(sim(999), sim(1)).is_none());
}
