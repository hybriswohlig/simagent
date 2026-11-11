//! "Warum?" recomputes the choice (ADR-0026 section 6, the test `why_recomputes_the_choice`):
//! from every `core:action_started` of a week the recomputed points give the same choice by
//! the rule of the second, `house::choice::choose`; by day the offer with the most points
//! among the objects with a free place and at least 40 000, in the night first what raises a
//! critical need, then a bed (ADR-0026, addendum of 2026-11-04); from every `core:idle` the
//! same rule gives idling, or waiting for the object the entry names. The offers of sleep are
//! closed by day, without points or mood, and open in the night. Each open offer carries the
//! person's mood for it, the addressed draw of the choice's second (ADR-0026, addendum of
//! 2026-11-04 on the mood), and its points with it; the chosen one the same. A time-filler's start
//! is a choice that comes to idling by day, and its offer is the one `house::choice::filler`
//! gives (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler), the one with the
//! highest mood, its mood and the others' in the answer, why it ends there by its code; an idle
//! choice by day with a time-filler free took none as it would have ended under 600 s on, and the
//! answer names that end (its addendum of 2026-11-05 on the time-filler's length). A time-filler
//! that ends at a next choice that waits names what the offer waited for raises (version 8,
//! ADR-0021, addendum of 2026-11-05 on the house page). A couple of 30 and 32 in the flat
//! of the repository, with the data `simagent` uses.

use sim_cli::data::core_data;
use sim_cli::why_answer;
use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::house::choice::{Choice, Scored, age, choose, filler, fillers, mood};
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
    let (mut starts, mut idles, mut nights, mut filled_in, mut shorts) = (0, 0, 0, 0, 0);
    let mut waited_after = 0;
    let mut reasons: std::collections::BTreeMap<String, usize> = Default::default();
    for (at, entry) in log.iter().enumerate() {
        // A time-filler's end as the server finds it: the person's next `core:action_ended`, and
        // its choice in that second after it.
        let mine = |other: &&LogEntry| other.subjects == entry.subjects;
        let end = (log[at + 1..].iter())
            .filter(mine)
            .position(|other| matches!(other.payload, LogPayload::ActionEnded { .. }))
            .map(|offset| {
                let later = &log[at + 1..];
                let ends = (later.iter()).filter(mine).nth(offset).expect("the end");
                let next = (later.iter())
                    .filter(mine)
                    .skip(offset + 1)
                    .take_while(|other| other.t == ends.t)
                    .find(|other| {
                        matches!(
                            other.payload,
                            LogPayload::ActionStarted { .. } | LogPayload::Idle { .. }
                        )
                    });
                sim_cli::Ended { ended: ends, next }
            });
        let filled = matches!(
            entry.payload,
            LogPayload::ActionStarted { filler: true, .. }
        );
        let ended = end.filter(|_| filled);
        let bonus = |object: u16| -> i64 {
            let Some(LogPayload::Company { objects }) = (log[..at].iter())
                .rev()
                .find(|other| {
                    other.t == entry.t
                        && other.subjects == entry.subjects
                        && matches!(other.payload, LogPayload::Company { .. })
                })
                .map(|other| &other.payload)
            else {
                return 0;
            };
            (objects.iter())
                .find(|row| row.object == object)
                .map(|row| row.company - row.avoid)
                .unwrap_or(0)
        };
        let company_rows = match (log[..at].iter())
            .rev()
            .find(|other| {
                other.t == entry.t
                    && other.subjects == entry.subjects
                    && matches!(other.payload, LogPayload::Company { .. })
            })
            .map(|other| &other.payload)
        {
            Some(LogPayload::Company { objects }) => objects.as_slice(),
            _ => &[],
        };
        let company = sim_cli::social_view::CompanyAt {
            rows: company_rows,
            history: &log[..at],
        };
        let Some(answer) =
            why_answer(&world, entry, 0, false, ended, bonus, company).expect("recomputes")
        else {
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
        let (house, room, needs, busy, tier) = match &entry.payload {
            LogPayload::ActionStarted {
                house,
                room,
                needs,
                busy,
                tier,
                ..
            }
            | LogPayload::Idle {
                house,
                room,
                needs,
                busy,
                tier,
                ..
            } => (house, *room, *needs, busy, tier),
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
            LogPayload::ActionStarted {
                object,
                offer,
                filler: true,
                ..
            } => {
                (starts, filled_in) = (starts + 1, filled_in + 1);
                assert_eq!(choice, Choice::Idle, "{answer}");
                assert!(night.is_none(), "{answer}");
                let moods = |object, offer| mood(7, founder, entry.t, (object, offer), MOOD);
                let filled = filler((house, houses), (years, room), busy, moods);
                assert_eq!(filled, Some((*object, *offer)), "{answer}");
                assert!(answer["filler"].is_object(), "{answer}");
                // Its mood and those of the others free then, the highest first (version 7).
                let free = fillers((house, houses), (years, room), busy, moods);
                let told = &answer["filler"];
                assert_eq!(told["mood"], serde_json::json!(free[0].mood), "{answer}");
                let others: Vec<serde_json::Value> = (free[1..].iter())
                    .map(|each| {
                        serde_json::json!({"object": each.object, "offer": each.offer,
                                           "mood": each.mood})
                    })
                    .collect();
                assert_eq!(told["others"], serde_json::json!(others), "{answer}");
                let ends = told["ends"].as_str();
                assert!(ends.is_none_or(|ends| ENDS.contains(&ends)), "{answer}");
                if let Some(ends) = ends {
                    *reasons.entry(ends.to_owned()).or_default() += 1;
                }
                // Ended at its next choice, which waits for a taken object: what the offer it
                // waits for raises, which brings enough, so at least one need (version 8).
                let waits = (ended.and_then(|ended| ended.next)).is_some_and(|next| {
                    matches!(next.payload, LogPayload::Idle { waits: Some(_), .. })
                });
                if waits && ends == Some("choice") {
                    waited_after += 1;
                    let then = told["then"].as_array();
                    assert!(then.is_some_and(|then| !then.is_empty()), "{answer}");
                }
            }
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
            LogPayload::Idle {
                waits, too_short, ..
            } => {
                idles += 1;
                assert_eq!(choice, waits.map_or(Choice::Idle, Choice::Wait), "{answer}");
                assert!(answer["chosen"].is_null());
                assert_eq!(answer["waits"], serde_json::json!(waits));
                // By day one idle and waiting for nothing with a time-filler free took none only
                // as it would have ended too soon (version 7).
                let moods = |object, offer| mood(7, founder, entry.t, (object, offer), MOOD);
                let free = fillers((house, houses), (years, room), busy, moods);
                let by_day = night.is_none() && waits.is_none() && !free.is_empty();
                assert_eq!(too_short.is_some(), by_day, "{answer}");
                match too_short {
                    Some(until) => {
                        shorts += 1;
                        let told = &answer["too_short"];
                        assert_eq!(told["until"]["t"], serde_json::json!(until.as_secs()));
                        assert!(until.as_secs() - entry.t.as_secs() < 600, "{answer}");
                        let ends = told["ends"].as_str().expect("why it ends there");
                        assert!(ENDS.contains(&ends), "{answer}");
                    }
                    None => assert!(answer["too_short"].is_null(), "{answer}"),
                }
            }
            other => panic!("no choice: {other:?}"),
        }
    }
    // About 60 entries a person a day: hundreds of choices in a week, each night at least the
    // two bedtimes; the time-fillers' starts among the starts.
    assert!(starts > 200, "{starts} starts");
    assert!(idles > 0, "{idles} idles");
    assert!(nights >= 14, "{nights} choices in the night");
    assert!(filled_in > 14, "{filled_in} time-fillers");
    // Their ends by their reasons, each but the next choice by its length, the bedtime or the
    // departure; the idle ones that took none, reported.
    println!(
        "time-fillers {filled_in}, ends {reasons:?}, {waited_after} of them before a wait; too short {shorts}"
    );
    assert!(reasons.contains_key("time"), "{reasons:?}");
}

/// Why a time-filler ends where it does, by its codes (version 7 of "Warum?").
const ENDS: [&str; 4] = ["departure", "bedtime", "time", "choice"];
