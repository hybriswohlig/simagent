//! The surname's root on the repository's pool (ADR-0021 section 3): in the town of the
//! export coverage scenario, a pool father has fathers of his own among the records, and the
//! root follows them; every other clause as in `sim-core/tests/census.rs`.

use sim_cli::files::ScenarioFile;
use sim_core::input::Input;
use sim_core::lineage::LineageKey;
use sim_core::time::SimTime;
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
    // Twenty years: enough for arrivals, births in the town and their children.
    let twenty = SimTime::from_secs(20 * 31_449_600 - 1).min(until);
    world.advance_until(&mut YearOut::default(), twenty, &mut |_| false);
    let records = world.data().pool().expect("the pool");
    let pool_root = |mut key: LineageKey| {
        let mut steps = 0;
        while let Some(father) = records.record(key).and_then(|record| record.father) {
            key = father;
            steps += 1;
        }
        (key, steps)
    };
    let (mut by_father, mut deep) = (0, 0);
    for person in world.pedigree().persons {
        let root = world.name_root(person.sim).expect("a root");
        let pool_father = person.pool.as_ref().and_then(|pool| pool.father_key);
        match (person.father, pool_father) {
            (Some(father), _) => {
                assert_eq!(Some(root), world.name_root(father), "{:?}", person.sim);
                by_father += 1;
            }
            (None, Some(key)) => {
                let (expected, steps) = pool_root(key);
                assert_eq!(root, expected, "{:?}", person.sim);
                deep += usize::from(steps > 0);
            }
            (None, None) => assert_eq!(root, person.lineage_key, "{:?}", person.sim),
        }
    }
    assert!(by_father > 0 && deep > 0, "{by_father} {deep}");
}
