//! "Warum?" recomputes the choice (ADR-0026 section 6, the test `why_recomputes_the_choice`):
//! from every `core:action_started` of a week the recomputed points give the same choice by
//! the rule of the second, `house::choice::choose`; by day the offer with the most points
//! among the objects with a free place and at least 40 000, in the night first what raises a
//! critical need, then a bed (ADR-0026, addendum of 2026-11-04); from every `core:idle` the
//! same rule gives idling, or waiting for the object the entry names. The offers of sleep are
//! closed by day, without points or mood, and open in the night. Each open offer carries the
//! person's mood for it, the addressed draw of the choice's second (ADR-0026, addendum of
//! 2026-11-04 on the mood), and its points with it; the chosen one the same. A couple of 30 and
//! 32 in the flat of the repository, with the data `simagent` uses.

use sim_cli::data::core_data;
use sim_cli::why_answer;
use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::house::choice::{Choice, Scored, age, choose, mood};
use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json`.
const MOOD: u8 = 10;

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
    let houses = world.state().data().houses().expect("houses");
    let (mut starts, mut idles, mut nights) = (0, 0, 0);
    for entry in &log {
        let Some(answer) = why_answer(&world, entry, 0).expect("recomputes") else {
            continue;
        };
        let number = |value: &serde_json::Value| value.as_i64().expect("a number");
        let small = |value: &serde_json::Value| u16::try_from(number(value)).expect("small");
        let percent = |value: &serde_json::Value| i8::try_from(number(value)).expect("a mood");
        let offers = answer["offers"].as_array().expect("offers");
        let (open, closed): (Vec<_>, Vec<_>) =
            offers.iter().partition(|offer| offer["closed"] == false);
        assert!(
            (closed.iter()).all(|offer| offer["points"].is_null() && offer["mood"].is_null()),
            "{answer}"
        );
        let scored: Vec<Scored> = (open.iter())
            .map(|offer| Scored {
                object: small(&offer["object"]),
                offer: small(&offer["offer"]),
                mood: percent(&offer["mood"]),
                points: number(&offer["points"]),
            })
            .collect();
        let night = answer["night"]["now"] == true;
        assert_eq!(night, closed.is_empty(), "{answer}");
        let (house, needs, busy, tier) = match &entry.payload {
            LogPayload::ActionStarted {
                house,
                needs,
                busy,
                tier,
                ..
            }
            | LogPayload::Idle {
                house,
                needs,
                busy,
                tier,
                ..
            } => (house, *needs, busy, tier),
            other => panic!("no choice: {other:?}"),
        };
        let house = houses.house(house).expect("its house");
        let [Subject::Sim(sim)] = entry.subjects.as_slice() else {
            panic!("one person");
        };
        // Sims 1 and 2 are the founders 0 and 1 of `test:why`.
        let founder = LineageKey::founder(&DataId::new("test:why").expect("valid"), sim.get() - 1);
        for each in &scored {
            let drawn = mood(7, founder, entry.t, (each.object, each.offer), MOOD);
            assert_eq!(each.mood, drawn, "{answer}");
        }
        let born = world.state().born(*sim).expect("born");
        let years = age(born, entry.t).expect("an age");
        // In the night the beds of the person's tier, as its entry names them.
        let night = night.then_some(tier.as_slice());
        let choice = choose((house, houses), (years, needs), &scored, busy, night);
        match &entry.payload {
            LogPayload::ActionStarted { object, offer, .. } => {
                starts += 1;
                nights += usize::from(night.is_some());
                let chosen = &answer["chosen"];
                let expected = Scored {
                    object: *object,
                    offer: *offer,
                    mood: percent(&chosen["mood"]),
                    points: number(&chosen["points"]),
                };
                assert_eq!(choice, Choice::Take(expected), "{answer}");
            }
            LogPayload::Idle { waits, .. } => {
                idles += 1;
                assert_eq!(choice, waits.map_or(Choice::Idle, Choice::Wait), "{answer}");
                assert!(answer["chosen"].is_null());
                assert_eq!(answer["waits"], serde_json::json!(waits));
            }
            other => panic!("no choice: {other:?}"),
        }
    }
    // About 60 entries a person a day: hundreds of choices in a week, each night at least the
    // two bedtimes.
    assert!(starts > 200, "{starts} starts");
    assert!(idles > 0, "{idles} idles");
    assert!(nights >= 14, "{nights} choices in the night");
}
