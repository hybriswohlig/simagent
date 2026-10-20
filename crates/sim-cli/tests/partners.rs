//! The partners a pairing input would be accepted with (ADR-0018 section 1, ADR-0021
//! section 6): at the end of the dummy run (its last second 943487999), read off its golden
//! pedigree (`tests/golden/dummy_pedigree.json`) by hand.
//! Living then: sims 2, 4, 10, 12 and 13 (men) and 8 (the one woman); every partnership
//! has ended. Sim 8's father is 2, her brothers 10 and 12 (φ 1/4), her son 13 is eight;
//! sim 4, her former partner, is unrelated to her. Sim 1 died, sims 3, 5, 6, 7, 9 and 11
//! emigrated.

use std::path::Path;
use std::process::Command;

use sim_core::ids::SimId;
use sim_core::world::World;

#[test]
fn the_dummy_runs_partners_by_hand() {
    let run = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("partners")
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
    assert_eq!(world.closed_through().as_secs(), 943_487_999);
    let sim = |n: u64| SimId::try_from(n).expect("an id");
    let cases: [(u64, &[u64]); 13] = [
        (8, &[4]),
        (4, &[8]),
        (2, &[]),
        (10, &[]),
        (12, &[]),
        (13, &[]),
        (1, &[]),
        (3, &[]),
        (5, &[]),
        (6, &[]),
        (7, &[]),
        (9, &[]),
        (11, &[]),
    ];
    for (person, partners) in cases {
        let expected: Vec<SimId> = partners.iter().map(|&n| sim(n)).collect();
        assert_eq!(world.partner_candidates(sim(person)), expected, "{person}");
    }
    assert!(world.partner_candidates(sim(999)).is_empty());
}
