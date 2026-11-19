//! The upbringing table (ADR-0033 sections 4 and 5): the saturating step of `D`, the age factor,
//! the table and its bounds, the steps of an occasion, and a family in a house whose children
//! come home from school. The expected numbers are `tools/development_reference.py` (saturation,
//! age factor) and `tools/upbringing_reference.py` (the table and the worked occasions).

mod common;

use common::day::Run;
use serde_json::json;
use sim_core::data::Data;
use sim_core::development::{
    Axis, Stepped, Upbringing, UpbringingRow, UpbringingTable, age_factor, saturate,
};
use sim_core::house::state::Duty;
use sim_core::ids::SimId;
use sim_core::log::{Cause, LogPayload, Subject};
use sim_core::time::{DAY, SimDuration, YEAR};

fn years(n: i64) -> SimDuration {
    SimDuration::from_secs(n * YEAR.as_secs())
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

/// The saturating step of the reference (`saturate`, its asserted values).
#[test]
fn the_step_saturates_toward_the_bound() {
    assert_eq!(saturate(0, 40), 40);
    assert_eq!(saturate(40, 40), 79);
    assert_eq!((saturate(0, -40), saturate(-40, -40)), (-40, -79));
    assert_eq!(saturate(1000, 30), 1020);
    assert_eq!(saturate(1000, -30), 960);
    assert_eq!(saturate(-2999, 1), -2998);
    assert_eq!(saturate(2999, -1), 2998);
    assert_eq!((saturate(3000, 40), saturate(-3000, -40)), (3000, -3000));
    assert_eq!(saturate(1234, 0), 1234);
    assert_eq!(
        (saturate(0, i32::MAX), saturate(0, i32::MIN)),
        (3000, -3000)
    );
}

/// The age factor of the reference (`age_factor_step`, its asserted values), to the second.
#[test]
fn the_age_factor_halves_at_11_and_quarters_at_15() {
    for (age, want) in [(10, 40), (11, 20), (14, 20), (15, 10), (17, 10), (18, 0)] {
        assert_eq!(age_factor(40, years(age)), want, "age {age}");
    }
    let before = |n: i64| SimDuration::from_secs(n * YEAR.as_secs() - 1);
    assert_eq!(
        (age_factor(40, before(11)), age_factor(40, years(11))),
        (40, 20)
    );
    assert_eq!(
        (age_factor(40, before(15)), age_factor(40, years(15))),
        (20, 10)
    );
    assert_eq!(
        (age_factor(40, before(18)), age_factor(40, years(18))),
        (10, 0)
    );
    assert_eq!(
        (age_factor(5, years(12)), age_factor(-5, years(12))),
        (2, -2)
    );
    assert_eq!(
        (age_factor(-5, years(16)), age_factor(3, years(16))),
        (-1, 0)
    );
}

fn row(occasion: Upbringing, steps: [i16; 5]) -> UpbringingRow {
    let [
        openness,
        conscientiousness,
        extraversion,
        agreeableness,
        neuroticism,
    ] = steps;
    UpbringingRow {
        occasion,
        openness,
        conscientiousness,
        extraversion,
        agreeableness,
        neuroticism,
    }
}

/// The start values of ADR-0033 section 5 for praise, tease, apology, school and the talk.
fn adr_table() -> UpbringingTable {
    UpbringingTable {
        rows: vec![
            row(Upbringing::Praise, [0, 0, 0, 20, -40]),
            row(Upbringing::Tease, [0, 0, 0, -20, 40]),
            row(Upbringing::Apology, [0, 0, 0, 20, -20]),
            row(Upbringing::School, [0, 10, 0, 0, 0]),
            row(Upbringing::Talk, [0, 0, 10, 0, 0]),
        ],
    }
}

/// The repository's table is the ADR's.
#[test]
fn the_repository_table_is_the_adrs() {
    assert_eq!(common::repository_upbringing(), adr_table());
}

/// The bounds: a step of ±1000 and 32 rows are kept; 1001, −1001 and 33 rows are refused, and
/// an unknown occasion or field does not parse.
#[test]
fn the_table_keeps_its_bounds() {
    let table = |rows: Vec<UpbringingRow>| UpbringingTable { rows }.checked();
    let one = |step: i16| row(Upbringing::Talk, [0, 0, step, 0, 0]);
    assert!(table(vec![one(1000), one(-1000)]).is_ok());
    assert!(table(vec![one(1001)]).is_err());
    assert!(table(vec![one(-1001)]).is_err());
    assert!(table(vec![one(1); 32]).is_ok());
    assert!(table(vec![one(1); 33]).is_err());
    assert_eq!(
        table(vec![one(1001)]).expect_err("refused").to_string(),
        "development.rows"
    );
    for wrong in [
        json!({"rows": [{"occasion": "hug", "openness": 1}]}),
        json!({"rows": [{"occasion": "talk", "humour": 1}]}),
        json!({"rows": [], "note": "x"}),
    ] {
        assert!(
            serde_json::from_value::<UpbringingTable>(wrong.clone()).is_err(),
            "{wrong}"
        );
    }
}

fn stepped(axis: Axis, before: i32, after: i32, step: i32, by: usize) -> Stepped {
    Stepped {
        axis,
        before,
        after,
        step,
        by,
    }
}

/// The worked talks of `upbringing_reference.py` for the daughter of 8 and the son of 14:
/// praise by the mother, then a tease by the son, then a talk only; per axis in order, and `by`
/// naming the occasion of `reached`.
#[test]
fn the_steps_of_a_talk_are_the_references() {
    use Axis::{Agreeableness, Extraversion, Neuroticism};
    use Upbringing::{Praise, Talk, Tease};
    let table = adr_table();
    let mut daughter = [0; 5];
    let got = table.steps(&[Talk, Praise], years(8), &mut daughter);
    assert_eq!(
        got,
        vec![
            stepped(Extraversion, 0, 10, 10, 0),
            stepped(Agreeableness, 0, 20, 20, 1),
            stepped(Neuroticism, 0, -40, -40, 1),
        ]
    );
    let mut son = [0; 5];
    let got = table.steps(&[Talk], years(14), &mut son);
    assert_eq!(got, vec![stepped(Extraversion, 0, 5, 5, 0)]);
    let got = table.steps(&[Tease, Talk], years(8), &mut daughter);
    assert_eq!(
        got,
        vec![
            stepped(Extraversion, 10, 19, 10, 1),
            stepped(Agreeableness, 20, 0, -20, 0),
            stepped(Neuroticism, -40, 0, 40, 0),
        ]
    );
    assert_eq!(daughter, [0, 0, 19, 0, 0]);
    let got = table.steps(&[Talk], years(8), &mut daughter);
    assert_eq!(got, vec![stepped(Extraversion, 19, 28, 10, 0)]);
    let got = table.steps(&[Talk], years(14), &mut son);
    assert_eq!(got, vec![stepped(Extraversion, 5, 9, 5, 0)]);
}

/// The girl of 16 and an apology of her mother: a quarter, toward 0 (E 2, A 5, N −5); a step
/// that changes nothing at the bound is left out; school at 14 is C 5, a second before 18 C 2,
/// at 18 nothing; an empty table nothing.
#[test]
fn the_age_factor_and_the_bound_shape_the_steps() {
    use Axis::{Agreeableness, Conscientiousness, Extraversion, Neuroticism};
    use Upbringing::{Apology, School, Talk, Tease};
    let table = adr_table();
    let mut girl = [0; 5];
    let got = table.steps(&[Apology, Talk], years(16), &mut girl);
    assert_eq!(
        got,
        vec![
            stepped(Extraversion, 0, 2, 2, 1),
            stepped(Agreeableness, 0, 5, 5, 0),
            stepped(Neuroticism, 0, -5, -5, 0),
        ]
    );
    let mut full = [0, 0, 0, 0, 2990];
    let got = table.steps(&[Tease], years(8), &mut full);
    assert_eq!(got, vec![stepped(Agreeableness, 0, -20, -20, 0)]);
    assert_eq!(full[4], 2990);
    let mut d = [0; 5];
    let got = table.steps(&[School], years(14), &mut d);
    assert_eq!(got, vec![stepped(Conscientiousness, 0, 5, 5, 0)]);
    let last = SimDuration::from_secs(18 * YEAR.as_secs() - 1);
    assert_eq!(table.steps(&[School], last, &mut [0; 5])[0].after, 2);
    assert!(table.steps(&[School], years(18), &mut [0; 5]).is_empty());
    let empty = UpbringingTable { rows: vec![] };
    assert!(empty.steps(&[Talk], years(8), &mut [0; 5]).is_empty());
}

/// The life data of `raised.rs` (no deaths, the first birth after two months, the repository's houses,
/// its social data without talk furniture) with the repository's table, or without it.
fn life_data(table: bool) -> Data {
    let (mut calibration, genome_map) = common::repository_files();
    let values = &mut calibration["values"];
    for sex in ["female", "male"] {
        values[format!("core:accident_{sex}")]["value"] = json!(1e-9);
        values[format!("core:senescence_a_{sex}")]["value"] = json!(1e-15);
        values[format!("core:senescence_b_{sex}")]["value"] = json!(0.001);
    }
    calibration["diseases"] = json!([]);
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(1.0);
    demography["values"]["core:twins_identical"]["value"] = json!(0.0);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    let data = Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(|_| {}));
    if table {
        data.with_upbringing(common::repository_upbringing())
    } else {
        data
    }
}

/// A woman of 30 and a man of 32 who take in a girl of 12 and a girl of 17.
const FAMILY: [i64; 4] = [
    -30 * YEAR.as_secs(),
    -32 * YEAR.as_secs(),
    -12 * YEAR.as_secs(),
    -17 * YEAR.as_secs(),
];

/// The girls of 12 and 17 come home from school on each weekday of two weeks: each return
/// writes one `core:developed` of conscientiousness for the girl alone, by the return, with
/// the step of her age (10 · 2 / 4 = 5 at 12, 10 · 1 / 4 = 2 at 17) saturating from 0. Nothing
/// else develops (no talk furniture), and `D` in the state is the last entry's.
#[test]
fn a_child_coming_home_from_school_develops_conscientiousness() {
    let (mut run, _) = Run::family(life_data(true), 7, "test:upbringing", &FAMILY, 0);
    run.advance(15 * DAY.as_secs());
    for (child, step) in [(3, 5), (4, 2)] {
        let returns: Vec<_> = (run.log.iter())
            .filter(|entry| {
                entry.subjects == [Subject::Sim(sim(child))]
                    && entry.payload
                        == LogPayload::CameHome {
                            place: Duty::School,
                        }
            })
            .map(|entry| entry.id)
            .collect();
        assert!(
            returns.len() >= 8,
            "child {child}: {} returns",
            returns.len()
        );
        let developed: Vec<_> = (run.log.iter())
            .filter(|entry| matches!(entry.payload, LogPayload::Developed { .. }))
            .filter(|entry| entry.subjects.contains(&Subject::Sim(sim(child))))
            .collect();
        assert_eq!(developed.len(), returns.len(), "child {child}");
        let mut d = 0;
        for (entry, came) in developed.iter().zip(&returns) {
            let after = saturate(d, step);
            let want = LogPayload::Developed {
                axis: Axis::Conscientiousness,
                before: d,
                after,
                step,
                cause: Upbringing::School,
            };
            assert_eq!(entry.payload, want, "child {child}");
            assert_eq!(entry.subjects, [Subject::Sim(sim(child))]);
            let by_return = matches!(entry.causes.as_slice(), [Cause::Event(id)] if common::day::number(id) == common::day::number(came));
            assert!(by_return, "child {child}");
            d = after;
        }
        let raised = &run.world.state().house().raised[&sim(child)];
        assert_eq!(raised.experience, [0, d, 0, 0, 0], "child {child}");
    }
    let all =
        (run.log.iter()).filter(|entry| matches!(entry.payload, LogPayload::Developed { .. }));
    let girls = [Subject::Sim(sim(3)), Subject::Sim(sim(4))];
    assert!(all.clone().all(|entry| girls.contains(&entry.subjects[0])));
}

/// Without a move-in, or without the table, the girls go to school and nothing develops.
#[test]
fn without_a_house_or_a_table_nothing_develops() {
    let mut founded = Run::founded(life_data(true), 7, "test:upbringing", &FAMILY);
    founded.advance(15 * DAY.as_secs());
    let (mut untabled, _) = Run::family(life_data(false), 7, "test:upbringing", &FAMILY, 0);
    untabled.advance(15 * DAY.as_secs());
    for run in [&founded, &untabled] {
        let developed = (run.log.iter()).any(|e| matches!(e.payload, LogPayload::Developed { .. }));
        assert!(!developed);
    }
    let school = (untabled.log.iter()).any(|e| {
        e.payload
            == LogPayload::CameHome {
                place: Duty::School,
            }
    });
    assert!(school);
}

/// The same family with the repository's talk furniture: every talk a girl takes part in writes
/// one `core:developed` of extraversion for her, by the talk, step 10 · 2 / 4 = 5 at 12 and
/// 10 · 1 / 4 = 2 at 17. The couple took the girls in, so they are their social parents, and
/// the girl of 17 is the older sibling of the girl of 12 (ADR-0033 section 5): a praise, tease or
/// apology row names the talk's actor as its source, one of those, with the topic's row.
#[test]
fn a_talk_develops_the_extraversion_of_each_child_in_it() {
    let data = life_data(true).with_social(common::repository_social());
    let (mut run, _) = Run::family(data, 7, "test:upbringing", &FAMILY, 0);
    run.advance(15 * DAY.as_secs());
    let mut talks = 0;
    for (child, step) in [(3, 5), (4, 2)] {
        let talked: Vec<_> = (run.log.iter())
            .filter(|e| matches!(e.payload, LogPayload::Talked { .. }))
            .filter(|e| e.subjects.contains(&Subject::Sim(sim(child))))
            .map(|e| common::day::number(&e.id))
            .collect();
        let developed: Vec<_> = (run.log.iter())
            .filter(|e| e.subjects.first() == Some(&Subject::Sim(sim(child))))
            .filter_map(|e| match (&e.payload, e.causes.as_slice()) {
                (
                    LogPayload::Developed {
                        axis: Axis::Extraversion,
                        before,
                        after,
                        step,
                        cause,
                    },
                    [Cause::Event(by)],
                ) => Some((*before, *after, *step, *cause, common::day::number(by))),
                _ => None,
            })
            .collect();
        let mut d = 0;
        let want: Vec<_> = (talked.iter())
            .map(|by| {
                let after = saturate(d, step);
                let entry = (d, after, step, Upbringing::Talk, *by);
                d = after;
                entry
            })
            .collect();
        assert_eq!(developed, want, "child {child}");
        talks += talked.len();
    }
    assert!(talks >= 4, "{talks} talks of the girls");
    let topic_of = |occasion| match occasion {
        Upbringing::Praise => 1,
        Upbringing::Tease => 2,
        Upbringing::Apology => 5,
        _ => panic!("no topic"),
    };
    let mut by_actor = 0;
    for entry in &run.log {
        let LogPayload::Developed { cause, .. } = entry.payload else {
            continue;
        };
        if matches!(cause, Upbringing::Talk | Upbringing::School) {
            continue;
        }
        let [Subject::Sim(child), Subject::Sim(source)] = entry.subjects[..] else {
            panic!("child and source: {:?}", entry.subjects);
        };
        let allowed = [sim(1), sim(2)].contains(&source)
            || (cause != Upbringing::Praise && (child, source) == (sim(3), sim(4)));
        assert!(allowed, "{child:?} {source:?} {cause:?}");
        let [Cause::Event(by)] = entry.causes.as_slice() else {
            panic!("one cause");
        };
        let talk = run.entry(by);
        let LogPayload::Talked { topic, actor, .. } = talk.payload else {
            panic!("by a talk");
        };
        assert_eq!((topic, actor), (topic_of(cause), Some(source)));
        assert!(talk.subjects.contains(&Subject::Sim(child)));
        by_actor += 1;
    }
    // And each such talk reached the girl: agreeableness and neuroticism, both far from the bound.
    let reaching = (run.log.iter()).filter(|e| match e.payload {
        LogPayload::Talked {
            topic,
            actor: Some(actor),
            ..
        } if [1, 2, 5].contains(&topic) => {
            let other = (e.subjects.iter())
                .find_map(|s| match s {
                    Subject::Sim(other) if *other != actor => Some(*other),
                    _ => None,
                })
                .expect("the other");
            let girl = [sim(3), sim(4)].contains(&other);
            let parent = [sim(1), sim(2)].contains(&actor);
            let elder = topic != 1 && (other, actor) == (sim(3), sim(4));
            girl && (parent || elder)
        }
        _ => false,
    });
    assert!(by_actor > 0);
    assert_eq!(by_actor, 2 * reaching.count());
}
