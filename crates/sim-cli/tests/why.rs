//! "Warum?" recomputes the choice (ADR-0026 section 6, the test `why_recomputes_the_choice`):
//! from every `core:action_started` of a week the recomputed points give the same choice, the
//! offer with the most points among the objects with a free place and at least 40 000; from
//! every `core:idle` no free offer brings that much. A couple of 30 and 32 in the flat of the
//! repository, with the data `simagent` uses.

use sim_cli::data::core_data;
use sim_cli::why_answer;
use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
use sim_core::log::{LogEntry, LogPayload};
use sim_core::params::WorldParams;
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::world::{World, YearOut};

#[test]
fn why_recomputes_the_choice() {
    let founder = |years: i64, sex: Sex| Founder {
        born: SimTime::from_secs(-years * YEAR.as_secs()),
        sex,
        alleles: Vec::<LocusAlleles>::new(),
    };
    let mut world = World::new(core_data().expect("the data"));
    let start = Start {
        world_seed: 7,
        founder_tag: DataId::new("test:why").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![founder(30, Sex::Female), founder(32, Sex::Male)],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    };
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    let mut log: Vec<LogEntry> = Vec::new();
    world.advance_until(&mut out, SimTime::from_secs(DAY.as_secs() - 2), &mut |_| {
        false
    });
    log.extend(world.take_log());
    let household = (log.iter())
        .find_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .expect("their household");
    world
        .submit(Input::MoveIn(MoveIn { household }))
        .expect("moves in");
    let week = SimTime::from_secs(8 * DAY.as_secs());
    world.advance_until(&mut out, week, &mut |_| false);
    log.extend(world.take_log());
    let (mut starts, mut idles) = (0, 0);
    for entry in &log {
        let Some(answer) = why_answer(&world, entry, 0).expect("recomputes") else {
            continue;
        };
        let enough = answer["enough"].as_i64().expect("enough");
        let offers = answer["offers"].as_array().expect("offers");
        let best = offers.iter().find(|offer| offer["busy"] == false);
        let points = |offer: &serde_json::Value| offer["points"].as_i64().expect("points");
        match &entry.payload {
            LogPayload::ActionStarted { object, offer, .. } => {
                starts += 1;
                let best = best.expect("a free offer");
                assert_eq!(
                    (&best["object"], &best["offer"]),
                    (&(*object).into(), &(*offer).into())
                );
                assert!(points(best) >= enough, "{answer}");
                assert_eq!(answer["chosen"]["points"], best["points"]);
            }
            _ => {
                idles += 1;
                assert!(best.is_none_or(|best| points(best) < enough), "{answer}");
                assert!(answer["chosen"].is_null());
            }
        }
    }
    // About 60 entries a person a day: hundreds of choices in a week.
    assert!(starts > 200, "{starts} starts");
    assert!(idles > 0, "{idles} idles");
}
