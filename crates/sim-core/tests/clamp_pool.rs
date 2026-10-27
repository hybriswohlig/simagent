//! K6, the clamp rate of the repository's pool at its arrival (ADR-0020 section 7, ADR-0005
//! section 8): every member of `data/pool/pool.bin` evaluated on the repository's data at the
//! second it arrives, in a town founded with every household of the pool at `Start` (world
//! seed 0x6b6, tag `test:k6`, the hereditary diseases on, no immigration, no capacity, no
//! nutrition). Per clamped parameter of the contract at most 0.1 percent of the evaluations,
//! `1000 · clamped ≤ n`.
//!
//! The expected values come from the independent reference (`scripts/k6.py` of the
//! reference for slice S5 of the map-4 switch, on `tools/census_reference.py`): `n` 40 009
//! (the 409 unborn of the pool are no members yet), `body_fat_z` 2, `chest_depth_z` 1,
//! `face_axis_02` 1, `face_axis_08` 1, every other key 0, no held curve; and the lineage keys
//! of the clamped.

mod common;

use std::collections::BTreeMap;

use sim_core::body_contract::CLAMPED_KEYS;
use sim_core::data_id::DataId;
use sim_core::input::{Input, PoolStart, Start};
use sim_core::params::WorldParams;
use sim_core::pool::Pool;
use sim_core::time::SimTime;
use sim_core::world::World;

/// The clamp counts the reference gives per key of `CLAMPED_KEYS`.
fn expected(key: &str) -> u64 {
    match key {
        "body_fat_z" => 2,
        "chest_depth_z" | "face_axis_02" | "face_axis_08" => 1,
        _ => 0,
    }
}

#[test]
fn every_member_of_the_pool_meets_the_clamp_rate_at_its_arrival() {
    let data = common::repository_data();
    // The CLI's data hash of the repository (tools/data_reference.py, `data_hash`): these
    // are the data `simagent` runs on.
    assert_eq!(data.hash(), 0x8e6a_a8bd_adae_3765);
    let pool = Pool::from_bytes(common::REPOSITORY_POOL).expect("the pool");
    let members: usize = pool.households.iter().map(|h| h.members.len()).sum();
    assert_eq!(pool.households.len(), 20_859);
    assert_eq!(members, 40_009);
    let n_start = u64::try_from(members).expect("fits");
    let drawn = PoolStart::drawn(&data, 0x6b6, n_start, false).expect("the data hold a pool");
    assert_eq!(
        drawn.households.len(),
        20_859,
        "the draw takes every household"
    );
    let mut world = World::new(data);
    world
        .submit(Input::Start(Start {
            world_seed: 0x6b6,
            founder_tag: DataId::new("test:k6").expect("id"),
            world_params: WorldParams {
                hereditary_diseases: true,
                nutrition: Vec::new(),
            },
            founders: Vec::new(),
            couples: Vec::new(),
            capacity: None,
            pool: Some(drawn),
        }))
        .expect("accepted");
    assert_eq!(world.state().living(), 40_009);

    // Every living person's body at the arrival second, the start of year 0.
    let arrival = SimTime::from_secs(0);
    let mut n: u64 = 0;
    let mut counts = [0_u64; 21];
    let mut curve: u64 = 0;
    let mut clamped_keys: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for person in world.pedigree().persons {
        assert!(person.died.is_none() && person.emigrated.is_none());
        let body = world
            .state()
            .body_at(person.sim, arrival)
            .expect("a body at the arrival");
        n += 1;
        curve += u64::from(body.curve_clamped);
        for (place, key) in CLAMPED_KEYS.iter().enumerate() {
            if body.is_clamped(*key) {
                counts[place] += 1;
                clamped_keys
                    .entry(key.as_str())
                    .or_default()
                    .push(person.lineage_key.get());
            }
        }
    }
    assert_eq!(n, 40_009);
    for (place, key) in CLAMPED_KEYS.iter().enumerate() {
        assert_eq!(counts[place], expected(key.as_str()), "{}", key.as_str());
        assert!(
            1_000 * u128::from(counts[place]) <= u128::from(n),
            "the rate of {}",
            key.as_str()
        );
    }
    assert_eq!(curve, 0);
    for keys in clamped_keys.values_mut() {
        keys.sort_unstable();
    }
    let reference: BTreeMap<&str, Vec<u64>> = BTreeMap::from([
        (
            "body_fat_z",
            vec![0x1f9a_27df_38c6_26e5, 0xf89e_f31e_1677_2b59],
        ),
        ("chest_depth_z", vec![0x4aa8_5875_44d4_3efe]),
        ("face_axis_02", vec![0xdda0_3853_c577_a0dd]),
        ("face_axis_08", vec![0xccde_1d94_0cdc_452b]),
    ]);
    assert_eq!(clamped_keys, reference);
}
