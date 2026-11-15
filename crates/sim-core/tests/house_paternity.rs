//! ADR-0031 section 6 and acceptance test 5: at a birth in a moved-in household the mother,
//! the biological father, her partner and everyone else from 6 hold one `VaterVon`. The
//! expected sentences are `tools/social_reference.py` `paternity_beliefs`, by hand. A birth
//! before move-in writes none. Beliefs do not change kinship or the pedigree, and a save
//! loads back to the same bytes.

mod common;

use common::day::{self, Run};
use serde_json::{Value, json};
use sim_core::data::{Data, Effect};
use sim_core::data_id::{Code, DataId};
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{
    ArrangePartnership, Command, Couple, Founder, Input, MoveIn, MoveOut, PoolStart,
    SeparatePartnership, Start,
};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, LogEntry, LogPayload, Subject, log_hash};
use sim_core::mortality::{StepHazard, disease_age};
use sim_core::params::WorldParams;
use sim_core::pool::Pool;
use sim_core::rng::{DrawKey, Draws, WorldSeed, tags};
use sim_core::social::belief::{Basis, Belief, Statement};
use sim_core::time::{MONTH, SimDuration, SimTime, YEAR};
use sim_core::world::{LoadError, World, YearOut};

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

/// Conception at the first month start, birth one month later (`house_feelings.rs`).
const BIRTH: i64 = 2 * MONTH.as_secs();

/// Fertility certain at the first month start, a pregnancy of one month, no twins unless
/// `identical` says so, no separation and no market, the repository's houses and its social
/// data without talk furniture. `social` changes the social file first. `tune` changes the
/// calibration and the demography after the window, so a test can name a separation rate or
/// a disease without disturbing the conception.
fn life_data(identical: f64) -> Data {
    life_data_with(identical, |_| {}, |_, _| {})
}

fn life_data_with(
    identical: f64,
    social: impl FnOnce(&mut Value),
    tune: impl FnOnce(&mut Value, &mut Value),
) -> Data {
    let (mut calibration, genome_map) = common::death_files(1e-9, None, json!([]));
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(1.0);
    demography["values"]["core:twins_identical"]["value"] = json!(identical);
    tune(&mut calibration, &mut demography);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(social))
}

fn founder(years: i64, sex: Sex) -> Founder {
    Founder {
        born: t(-years * YEAR.as_secs()),
        sex,
        alleles: common::dummy(0, 0),
    }
}

fn started(founders: Vec<Founder>, couples: Vec<Couple>, identical: f64) -> Run {
    let start = Start {
        world_seed: 7,
        founder_tag: DataId::new("test:paternity").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders,
        couples,
        capacity: None,
        pool: None,
    };
    let mut run = Run {
        world: World::new(life_data(identical)),
        out: YearOut::default(),
        log: Vec::new(),
    };
    run.submit(Input::Start(start));
    run
}

/// Mother 30, F 32, P 34, a child of 7 and a child of 4. The couple is the mother and F.
/// The two children are taken in by her.
fn covered() -> Run {
    started(
        vec![
            founder(30, Sex::Female),
            founder(32, Sex::Male),
            founder(34, Sex::Male),
            founder(7, Sex::Female),
            founder(4, Sex::Male),
        ],
        vec![Couple { woman: 0, man: 1 }],
        0.0,
    )
}

fn caused_by(entry: &LogEntry) -> Vec<u64> {
    (entry.causes.as_slice().iter())
        .filter_map(|cause| match cause {
            Cause::Event(id) => Some(day::number(id)),
            Cause::Plan(plan) => Some(day::number(&plan.by)),
            _ => None,
        })
        .collect()
}

fn belief(child: u64, man: u64, certainty: u16, basis: Basis, secret: bool) -> Belief {
    Belief {
        statement: Statement::VaterVon {
            child: sim(child),
            man: sim(man),
        },
        certainty,
        since: t(BIRTH),
        source: None,
        origin: None,
        basis,
        secret,
    }
}

fn held(run: &Run, who: u64) -> Vec<Belief> {
    (run.world.state().house().social.beliefs.get(&sim(who)))
        .cloned()
        .unwrap_or_default()
}

/// Acceptance test 5: conception with F, separation, a new partnership with P, then the
/// birth in the moved-in household. Four sentences. The 4-year-old and the newborn hold
/// none. Kinship and the pedigree stay the biological truth. The save loads back bit for bit.
#[test]
fn a_birth_after_a_new_partnership_hides_the_father() {
    let mut run = covered();
    run.advance(MONTH.as_secs());
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    }));
    run.advance(BIRTH - 2);
    let home = run
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    assert_ne!(run.world.state().household_of(sim(2)), Some(home));
    let members: Vec<SimId> = run.world.state().members(home).collect();
    assert_eq!(members, vec![sim(1), sim(3), sim(4), sim(5)]);
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(BIRTH);

    let birth = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::Birth { .. }))
        .expect("the birth");
    assert_eq!(birth.t, t(BIRTH));
    assert_eq!(
        birth.subjects,
        [
            Subject::Sim(sim(6)),
            Subject::Sim(sim(1)),
            Subject::Sim(sim(2))
        ]
    );
    assert_eq!(run.world.state().biological_father(sim(6)), Some(sim(2)));
    // φ of a parent and a child is 0.25 (kinship.rs); P is not a parent.
    assert_eq!(run.world.state().kinship(sim(6), sim(1)), Some(0.25));
    assert_eq!(run.world.state().kinship(sim(6), sim(2)), Some(0.25));
    assert_eq!(run.world.state().kinship(sim(6), sim(3)), Some(0.0));
    let person = (run.world.pedigree().persons.into_iter())
        .find(|person| person.sim == sim(6))
        .expect("the child");
    assert_eq!(person.mother, Some(sim(1)));
    assert_eq!(person.father, Some(sim(2)));
    assert!(person.social_parents.is_empty());
    assert_eq!(person.carer, Some(sim(1)));

    assert_eq!(
        held(&run, 1),
        vec![belief(6, 2, 1000, Basis::Beteiligt, true)]
    );
    assert_eq!(
        held(&run, 2),
        vec![belief(6, 2, 1000, Basis::Beteiligt, false)]
    );
    assert_eq!(
        held(&run, 3),
        vec![belief(6, 3, 900, Basis::Geschlossen, false)]
    );
    assert_eq!(
        held(&run, 4),
        vec![belief(6, 3, 900, Basis::Geschlossen, false)]
    );
    assert!(held(&run, 5).is_empty(), "the 4-year-old");
    assert!(held(&run, 6).is_empty(), "the newborn");

    let believed: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Believed { .. }))
        .collect();
    let birth_no = day::number(&birth.id);
    let expect = [
        (1, belief(6, 2, 1000, Basis::Beteiligt, true)),
        (2, belief(6, 2, 1000, Basis::Beteiligt, false)),
        (3, belief(6, 3, 900, Basis::Geschlossen, false)),
        (4, belief(6, 3, 900, Basis::Geschlossen, false)),
    ];
    assert_eq!(believed.len(), expect.len());
    for (entry, (who, held_belief)) in believed.iter().zip(expect) {
        assert_eq!(entry.t, t(BIRTH));
        assert_eq!(entry.subjects, [Subject::Sim(sim(who))]);
        assert_eq!(entry.causes.as_slice().len(), 1);
        assert_eq!(caused_by(entry), vec![birth_no]);
        let LogPayload::Believed {
            statement,
            certainty,
            basis,
            source,
            origin,
            replaced,
        } = entry.payload
        else {
            panic!("believed");
        };
        assert_eq!(statement, held_belief.statement);
        assert_eq!(certainty, held_belief.certainty);
        assert_eq!(basis, held_belief.basis);
        assert_eq!(source, None);
        assert_eq!(origin, None);
        assert!(!replaced);
    }
    assert!((run.log.iter()).all(|entry| !matches!(entry.payload, LogPayload::Unbelieved { .. })));
    // The day counter opens at the first morning. Move-in is one second before the birth,
    // so that morning is still ahead and nobody has a counter. The twins test checks a
    // counter that already exists stays at 0.
    assert!(
        run.world.state().house().sims.is_empty(),
        "no day counter at the birth"
    );

    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, life_data(0.0)).expect("loads");
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    assert_eq!(loaded.save(&hashes).expect("saved again"), bytes);
    assert_eq!(
        loaded.state().house().social.beliefs,
        run.world.state().house().social.beliefs
    );
}

/// A household that has not moved in writes no belief at the birth, and no `core:believed`.
#[test]
fn a_birth_before_move_in_writes_nothing() {
    let born = [-30 * YEAR.as_secs(), -32 * YEAR.as_secs()];
    let mut run = Run::founded(life_data(0.0), 7, "test:paternity", &born);
    run.advance(BIRTH);
    assert!(
        (run.log.iter()).any(|entry| matches!(entry.payload, LogPayload::Birth { .. })),
        "the birth happened"
    );
    assert!(run.world.state().house().social.beliefs.is_empty());
    assert!((run.log.iter()).all(|entry| {
        !matches!(
            entry.payload,
            LogPayload::Believed { .. } | LogPayload::Unbelieved { .. }
        )
    }));
}

/// Twins, P = F, the household moved in: one sentence each, in index order, the same father,
/// nothing secret. The father is not given a second sentence.
#[test]
fn twins_each_get_the_fathers_sentence_in_index_order() {
    let born = [-30 * YEAR.as_secs(), -32 * YEAR.as_secs()];
    let (mut run, _) = Run::family(life_data(1.0), 7, "test:paternity", &born, 0);
    run.advance(BIRTH);
    let births: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Birth { .. }))
        .collect();
    assert_eq!(births.len(), 2);
    assert_eq!(
        births[0].subjects,
        [
            Subject::Sim(sim(3)),
            Subject::Sim(sim(1)),
            Subject::Sim(sim(2))
        ]
    );
    assert_eq!(
        births[1].subjects,
        [
            Subject::Sim(sim(4)),
            Subject::Sim(sim(1)),
            Subject::Sim(sim(2))
        ]
    );
    let one = |child: u64| belief(child, 2, 1000, Basis::Beteiligt, false);
    assert_eq!(held(&run, 1), vec![one(3), one(4)]);
    assert_eq!(held(&run, 2), vec![one(3), one(4)]);
    assert!(held(&run, 3).is_empty() && held(&run, 4).is_empty());

    let believed: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Believed { .. }))
        .collect();
    let order = [(1, 3), (2, 3), (1, 4), (2, 4)];
    assert_eq!(believed.len(), order.len());
    for (entry, ((who, child), birth)) in believed.iter().zip(order.into_iter().zip([0, 0, 1, 1])) {
        assert_eq!(entry.subjects, [Subject::Sim(sim(who))]);
        assert_eq!(entry.causes.as_slice().len(), 1);
        assert_eq!(caused_by(entry), vec![day::number(&births[birth].id)]);
        assert_eq!(
            entry.payload,
            LogPayload::Believed {
                statement: Statement::VaterVon {
                    child: sim(child),
                    man: sim(2),
                },
                certainty: 1000,
                basis: Basis::Beteiligt,
                source: None,
                origin: None,
                replaced: false,
            }
        );
    }
    for who in [1, 2] {
        assert_eq!(
            run.world
                .state()
                .house()
                .sims
                .get(&sim(who))
                .map(|person| person.entries),
            Some(0),
            "sim {who}"
        );
    }
}

/// Moving in after the birth does not go back and write the sentences.
#[test]
fn a_move_in_after_the_birth_does_not_backfill() {
    let born = [-30 * YEAR.as_secs(), -32 * YEAR.as_secs()];
    let mut run = Run::founded(life_data(0.0), 7, "test:paternity", &born);
    run.advance(BIRTH);
    assert!(
        (run.log.iter()).any(|entry| matches!(entry.payload, LogPayload::Birth { .. })),
        "the birth happened"
    );
    let home = run
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    assert!(
        (run.log.iter()).any(|entry| matches!(entry.payload, LogPayload::MovedIn { .. })),
        "they moved in"
    );
    assert!(run.world.state().house().social.beliefs.is_empty());
    assert!((run.log.iter()).all(|entry| {
        !matches!(
            entry.payload,
            LogPayload::Believed { .. } | LogPayload::Unbelieved { .. }
        )
    }));
}

/// `social.limits.min_age` is 8. The seven-year-old is in the household and holds nothing.
/// The adults still do. The save loads: a sentence for the child would fail the age gate.
#[test]
fn a_seven_year_old_is_silent_when_min_age_is_8() {
    let data = life_data_with(0.0, |file| file["limits"]["min_age"] = json!(8), |_, _| {});
    let mut run = started_on(
        data.clone(),
        vec![
            founder(30, Sex::Female),
            founder(32, Sex::Male),
            founder(34, Sex::Male),
            founder(7, Sex::Female),
            founder(4, Sex::Male),
        ],
        vec![Couple { woman: 0, man: 1 }],
        false,
    );
    run.advance(MONTH.as_secs());
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    }));
    run.advance(BIRTH - 2);
    let home = run
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(BIRTH);
    assert_eq!(
        held(&run, 1),
        vec![belief(6, 2, 1000, Basis::Beteiligt, true)]
    );
    assert_eq!(
        held(&run, 2),
        vec![belief(6, 2, 1000, Basis::Beteiligt, false)]
    );
    assert_eq!(
        held(&run, 3),
        vec![belief(6, 3, 900, Basis::Geschlossen, false)]
    );
    assert!(held(&run, 4).is_empty(), "seven, under min_age 8");
    assert!(held(&run, 5).is_empty(), "the 4-year-old");
    assert!(held(&run, 6).is_empty(), "the newborn");
    reload(&run, &data);
}

/// `social.limits.min_age` is 40. The adults are 30, 32 and 34, so none of the four
/// rules writes. The save loads: a sentence for one of them would fail the age gate.
#[test]
fn a_min_age_of_40_writes_nothing_for_the_adults_under_it() {
    let data = life_data_with(0.0, |file| file["limits"]["min_age"] = json!(40), |_, _| {});
    let mut run = started_on(
        data.clone(),
        vec![
            founder(30, Sex::Female),
            founder(32, Sex::Male),
            founder(34, Sex::Male),
            founder(7, Sex::Female),
            founder(4, Sex::Male),
        ],
        vec![Couple { woman: 0, man: 1 }],
        false,
    );
    run.advance(MONTH.as_secs());
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    }));
    run.advance(BIRTH - 2);
    let home = run
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(BIRTH);
    assert!(
        (run.log.iter()).any(|entry| matches!(entry.payload, LogPayload::Birth { .. })),
        "the birth happened"
    );
    for who in 1..=6 {
        assert!(held(&run, who).is_empty(), "sim {who} is under 40");
    }
    assert!((run.log.iter()).all(|entry| {
        !matches!(
            entry.payload,
            LogPayload::Believed { .. } | LogPayload::Unbelieved { .. }
        )
    }));
    reload(&run, &data);
}

/// Cap 1, twins, P = F. The parents' own-child sentences stay (the second is refused, so
/// nothing is unbelieved). The seven-year-old's first sentence falls to the second.
#[test]
fn a_full_list_at_the_birth_protects_an_own_child() {
    let data = life_data_with(1.0, |file| file["limits"]["beliefs"] = json!(1), |_, _| {});
    let born = [
        -30 * YEAR.as_secs(),
        -32 * YEAR.as_secs(),
        -7 * YEAR.as_secs(),
        -4 * YEAR.as_secs(),
    ];
    let (mut run, _) = Run::family(data, 7, "test:paternity", &born, 0);
    run.advance(BIRTH);
    let one = |child: u64, basis: Basis| {
        belief(
            child,
            2,
            if basis == Basis::Beteiligt { 1000 } else { 900 },
            basis,
            false,
        )
    };
    assert_eq!(held(&run, 1), vec![one(5, Basis::Beteiligt)]);
    assert_eq!(held(&run, 2), vec![one(5, Basis::Beteiligt)]);
    assert_eq!(held(&run, 3), vec![one(6, Basis::Geschlossen)]);
    assert!(held(&run, 4).is_empty() && held(&run, 5).is_empty() && held(&run, 6).is_empty());
    let births: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Birth { .. }))
        .collect();
    assert_eq!(births.len(), 2);
    let paternity: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::Believed {
                    statement: Statement::VaterVon { .. },
                    ..
                } | LogPayload::Unbelieved {
                    statement: Statement::VaterVon { .. },
                }
            )
        })
        .collect();
    let child_of = |entry: &LogEntry| match entry.payload {
        LogPayload::Believed {
            statement: Statement::VaterVon { child, .. },
            ..
        }
        | LogPayload::Unbelieved {
            statement: Statement::VaterVon { child, .. },
        } => child,
        _ => panic!("paternity"),
    };
    assert_eq!(paternity.len(), 5);
    assert_eq!(paternity[0].subjects, [Subject::Sim(sim(1))]);
    assert_eq!(child_of(paternity[0]), sim(5));
    assert_eq!(paternity[1].subjects, [Subject::Sim(sim(2))]);
    assert_eq!(child_of(paternity[1]), sim(5));
    assert_eq!(paternity[2].subjects, [Subject::Sim(sim(3))]);
    assert_eq!(child_of(paternity[2]), sim(5));
    assert!(matches!(paternity[3].payload, LogPayload::Believed { .. }));
    assert_eq!(paternity[3].subjects, [Subject::Sim(sim(3))]);
    assert_eq!(child_of(paternity[3]), sim(6));
    assert_eq!(caused_by(paternity[3]), vec![day::number(&births[1].id)]);
    assert!(matches!(
        paternity[4].payload,
        LogPayload::Unbelieved { .. }
    ));
    assert_eq!(paternity[4].subjects, [Subject::Sim(sim(3))]);
    assert_eq!(child_of(paternity[4]), sim(5));
    assert_eq!(caused_by(paternity[4]), vec![day::number(&paternity[3].id)]);
}

/// Saved one second before the birth, with the move-in already closed, and run on through
/// the birth: the same log hash, the same entries and the same state as the uninterrupted run.
#[test]
fn a_save_one_second_before_the_birth_continues_bit_identically() {
    let data = life_data(0.0);
    let born = [-30 * YEAR.as_secs(), -32 * YEAR.as_secs()];
    let at_save = || {
        let (mut run, _) = Run::family(data.clone(), 7, "test:paternity", &born, 0);
        run.advance(BIRTH - 1);
        run
    };
    let mut whole = at_save();
    let before = whole.log.len();
    whole.advance(BIRTH);
    let run = at_save();
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (mut loaded, _) = World::load(&bytes, data).expect("loads");
    assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
    loaded.advance_until(&mut YearOut::default(), t(BIRTH), &mut |_| false);
    let continued = loaded.take_log();
    let tail = &whole.log[before..];
    assert_eq!(log_hash(&continued), log_hash(tail));
    assert_eq!(
        serde_json::to_value(&continued).expect("json"),
        serde_json::to_value(tail).expect("json")
    );
    assert_eq!(loaded.state_hash(), whole.world.state_hash());
}

/// The separation of the new partnership and the seven-year-old's death are both due in the
/// birth's second. The birth reads P and the household first: P's sentence names P, and the
/// child's sentence is written before the death drops it.
#[test]
fn a_separation_and_a_death_in_the_birth_second_come_after_the_beliefs() {
    let (separation, hazard) = rates_for_the_birth_second();
    let data = life_data_with(
        0.0,
        |_| {},
        |calibration, demography| {
            demography["values"]["core:separation_rate"]["value"] = json!(separation);
            calibration["diseases"] = disease_killing_at(hazard);
        },
    );
    assert_eq!(data.demography().separation_rate, separation);
    let Effect::Death { per_year } = &data.diseases()[0].effect else {
        panic!("the dummy disease kills");
    };
    assert_eq!(per_year.value.get(), hazard);
    let mut founders = vec![
        founder(30, Sex::Female),
        founder(32, Sex::Male),
        founder(34, Sex::Male),
        founder(7, Sex::Female),
        founder(4, Sex::Male),
    ];
    founders[3].alleles = common::dummy(1, 1);
    let mut run = started_on(data, founders, vec![Couple { woman: 0, man: 1 }], true);
    run.advance(MONTH.as_secs());
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    }));
    run.advance(BIRTH - 2);
    let home = run
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(BIRTH);

    let at_birth: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| entry.t == t(BIRTH))
        .collect();
    let index = |kind: &str| {
        (at_birth.iter().position(|entry| match kind {
            "birth" => matches!(entry.payload, LogPayload::Birth { .. }),
            "child" => {
                entry.subjects == [Subject::Sim(sim(4))]
                    && matches!(entry.payload, LogPayload::Believed { .. })
            }
            "partner" => match entry.payload {
                LogPayload::Believed {
                    statement: Statement::VaterVon { man, .. },
                    ..
                } => entry.subjects == [Subject::Sim(sim(3))] && man == sim(3),
                _ => false,
            },
            "separation" => matches!(entry.payload, LogPayload::Separated { .. }),
            "death" => matches!(entry.payload, LogPayload::Death { .. }),
            _ => false,
        }))
        .unwrap_or_else(|| panic!("no {kind} at the birth"))
    };
    let birth_at = index("birth");
    let child_at = index("child");
    let partner_at = index("partner");
    let separation_at = index("separation");
    let death_at = index("death");
    assert!(birth_at < child_at && child_at < separation_at);
    assert!(birth_at < partner_at && partner_at < separation_at);
    assert!(separation_at < death_at);
    assert!(at_birth.iter().enumerate().all(|(at, entry)| {
        !matches!(entry.payload, LogPayload::Believed { .. }) || at < separation_at
    }));
    let separation = at_birth[separation_at];
    assert_eq!(
        separation.subjects,
        [Subject::Sim(sim(1)), Subject::Sim(sim(3))]
    );
    assert!(matches!(
        at_birth[death_at].payload,
        LogPayload::Death { ref cause } if cause.as_str() == "test:dummy_disease"
    ));
    assert_eq!(at_birth[death_at].subjects, [Subject::Sim(sim(4))]);
    assert_eq!(
        held(&run, 3),
        vec![belief(6, 3, 900, Basis::Geschlossen, false)]
    );
    assert_eq!(
        held(&run, 1),
        vec![belief(6, 2, 1000, Basis::Beteiligt, true)]
    );
    assert!(
        held(&run, 4).is_empty(),
        "the death dropped the child's list"
    );
    assert!(run.world.state().household_of(sim(4)).is_none());
    let earlier: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| {
            entry.t.as_secs() < BIRTH && matches!(entry.payload, LogPayload::Separated { .. })
        })
        .collect();
    assert_eq!(earlier.len(), 1, "only the manual separation from F");
    assert_eq!(earlier[0].t, t(MONTH.as_secs() + 1));
    assert_eq!(
        earlier[0].subjects,
        [Subject::Sim(sim(1)), Subject::Sim(sim(2))]
    );
}

/// `pool_c` household 28: the father is an ancestor and never a person of the town. The
/// mother's household has moved in. The birth writes nothing.
#[test]
fn a_pool_father_with_no_town_person_writes_nothing() {
    let data = pool_c_data(POOL_C, |_| {});
    let (mut run, drawn) = pool_world(data.clone(), 6, 1);
    assert_eq!(drawn.households.len(), 1);
    assert_eq!(
        pregnancy_father(&drawn, POOL_MOTHER_WITHOUT_A_FATHER),
        POOL_FATHER_NOT_IN_TOWN
    );
    assert!(
        drawn.households[0]
            .members
            .iter()
            .all(|member| member.lineage_key.get() != POOL_FATHER_NOT_IN_TOWN)
    );
    let mother = sim_of(&run, POOL_MOTHER_WITHOUT_A_FATHER);
    moved_in_through(&mut run, mother, POOL_BIRTH_WITHOUT_A_FATHER);
    let children = children_of(&run, mother);
    assert_eq!(children.len(), 1);
    assert!(
        (run.log.iter()).any(|entry| matches!(entry.payload, LogPayload::Birth { .. })),
        "the birth happened"
    );
    assert!(run.world.state().house().social.beliefs.is_empty());
    assert!((run.log.iter()).all(|entry| {
        !matches!(
            entry.payload,
            LogPayload::Believed { .. } | LogPayload::Unbelieved { .. }
        )
    }));
    reload(&run, &data);
}

/// The same birth, with a non-secret `VaterVon(child, P)` written into the save. P is a
/// town person. The child's pool father is not, so the load is refused.
#[test]
fn a_vater_von_for_a_child_whose_pool_father_is_nobody_does_not_load() {
    let data = pool_c_data(POOL_C, |_| {});
    let (mut run, drawn) = pool_world(data.clone(), 6, 1);
    assert_eq!(
        pregnancy_father(&drawn, POOL_MOTHER_WITHOUT_A_FATHER),
        POOL_FATHER_NOT_IN_TOWN
    );
    let mother = sim_of(&run, POOL_MOTHER_WITHOUT_A_FATHER);
    moved_in_through(&mut run, mother, POOL_BIRTH_WITHOUT_A_FATHER);
    let children = children_of(&run, mother);
    assert_eq!(children.len(), 1);
    let child = children[0];
    pool_child_without_recorded_parents(&run, child);
    let allocated = run.world.state().sim_ids_allocated();
    assert!((1..=allocated).all(|id| {
        let sim = SimId::try_from(id).expect("id");
        run.world.state().lineage_key(sim).map(|key| key.get()) != Some(POOL_FATHER_NOT_IN_TOWN)
    }));
    let named = (run.log.iter())
        .find_map(|entry| match entry.payload {
            LogPayload::Partnered { .. } => match entry.subjects.as_slice() {
                [Subject::Sim(woman), Subject::Sim(man)] if *woman == mother => Some(*man),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or(mother);
    assert_ne!(named, child);
    assert!(
        run.world.state().born(named).is_some(),
        "P is a town person"
    );
    assert!(run.world.state().house().social.beliefs.is_empty());
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    assert!(World::load(&bytes, data.clone()).is_ok());
    let social = run.world.state().house().social.clone();
    let mut forged_social = social.clone();
    forged_social.beliefs.insert(
        mother,
        vec![sentence(
            child,
            named,
            POOL_BIRTH_WITHOUT_A_FATHER,
            900,
            Basis::Geschlossen,
            false,
        )],
    );
    let forged = replaced(&bytes, &encode(&social), &encode(&forged_social));
    assert_eq!(
        World::load(&forged, data).err(),
        Some(LoadError::Inconsistent)
    );
}

/// `pool_c` household 28 with its pregnancy's father retargeted to the solo man of household
/// 12. Seed 100 draws the mother before him, so at the insert he has no sim id yet. By the
/// birth he does, and both sentences name him. The birth's subject list still has no father.
#[test]
fn a_pool_father_who_arrives_later_is_still_named() {
    let bytes = pool_with_a_later_father();
    let data = without_a_market(pool_c_data(&bytes, |_| {}));
    let (mut run, drawn) = pool_world(data.clone(), 100, 3);
    let male = Pool::from_bytes(POOL_C).expect("pool_c").households[12].members[0]
        .lineage_key
        .get();
    let mother_at = draw_index(&drawn, POOL_MOTHER_WITHOUT_A_FATHER);
    let male_at = draw_index(&drawn, male);
    assert!(
        mother_at < male_at,
        "the father arrives in a later household"
    );
    assert_eq!(pregnancy_father(&drawn, POOL_MOTHER_WITHOUT_A_FATHER), male);
    let mother = sim_of(&run, POOL_MOTHER_WITHOUT_A_FATHER);
    let father = sim_of(&run, male);
    moved_in_through(&mut run, mother, POOL_BIRTH_WITHOUT_A_FATHER);
    let children = children_of(&run, mother);
    assert_eq!(children.len(), 1);
    let child = children[0];
    let birth = (run.log.iter())
        .find(|entry| {
            matches!(entry.payload, LogPayload::Birth { .. })
                && entry.subjects.contains(&Subject::Sim(child))
        })
        .expect("the birth");
    assert_eq!(birth.subjects, [Subject::Sim(child), Subject::Sim(mother)]);
    pool_child_without_recorded_parents(&run, child);
    let named = sentence(
        child,
        father,
        POOL_BIRTH_WITHOUT_A_FATHER,
        1000,
        Basis::Beteiligt,
        false,
    );
    assert_eq!(holds(&run, mother), vec![named]);
    assert_eq!(holds(&run, father), vec![named]);
    assert_ne!(
        run.world.state().household_of(father),
        run.world.state().household_of(mother)
    );
    reload(&run, &data);
}

/// `pool_c` household 31, twins, the partner is the father, cap 1. Each parent keeps the
/// first child's sentence and refuses the second, so nothing is unbelieved. That is the
/// own-child protection of a pool child: the sentence is not secret.
#[test]
fn a_pool_fathers_own_child_sentence_is_kept() {
    let data = pool_c_data(POOL_C, |file| file["limits"]["beliefs"] = json!(1));
    let (mut run, drawn) = pool_world(data.clone(), 31, 2);
    assert_eq!(drawn.households.len(), 1);
    assert_eq!(pregnancy_father(&drawn, POOL_TWIN_MOTHER), POOL_TWIN_FATHER);
    assert_eq!(drawn.households[0].pregnancies[0].children.len(), 2);
    let mother = sim_of(&run, POOL_TWIN_MOTHER);
    let father = sim_of(&run, POOL_TWIN_FATHER);
    moved_in_through(&mut run, mother, POOL_TWIN_BIRTH);
    let children = children_of(&run, mother);
    assert_eq!(children.len(), 2);
    let first = sentence(
        children[0],
        father,
        POOL_TWIN_BIRTH,
        1000,
        Basis::Beteiligt,
        false,
    );
    assert_eq!(holds(&run, mother), vec![first]);
    assert_eq!(holds(&run, father), vec![first]);
    assert!((run.log.iter()).all(|entry| !matches!(entry.payload, LogPayload::Unbelieved { .. })));
    pool_child_without_recorded_parents(&run, children[0]);
    reload(&run, &data);
}

/// The repository pool, seed 59, household 15348: one child, the partner is the father, a
/// witness of 6. Re-paired with a solo man before the birth, the mother hides F. The save
/// loads. After F is back in the house, his talk with the witness refutes her sentence.
#[test]
fn a_pool_birth_hides_the_father_and_loads() {
    let data = quiet_town();
    let (mut run, drawn) = pool_world(data.clone(), 59, 37);
    assert_eq!(pregnancy_father(&drawn, TOWN_MOTHER), TOWN_FATHER);
    assert!(draw_index(&drawn, TOWN_WITNESS) == draw_index(&drawn, TOWN_MOTHER));
    let mother = sim_of(&run, TOWN_MOTHER);
    let father = sim_of(&run, TOWN_FATHER);
    let witness = sim_of(&run, TOWN_WITNESS);
    let partner = sim_of(&run, solo_man(&drawn, TOWN_FATHER));
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: mother,
        man: father,
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: mother,
        man: partner,
    }));
    assert_eq!(
        run.world.state().household_of(witness),
        run.world.state().household_of(mother)
    );
    assert_ne!(
        run.world.state().household_of(father),
        run.world.state().household_of(mother)
    );
    moved_in_through(&mut run, mother, TOWN_BIRTH);
    assert!(
        run.world.state().household_of(mother).is_some(),
        "the mother lives"
    );
    assert!(
        run.world.state().household_of(witness).is_some(),
        "the witness lives"
    );
    assert!(run.world.state().household_of(father).is_some(), "F lives");
    let children = children_of(&run, mother);
    assert_eq!(children.len(), 1);
    let child = children[0];
    pool_child_without_recorded_parents(&run, child);
    assert_eq!(
        holds(&run, mother),
        vec![sentence(
            child,
            father,
            TOWN_BIRTH,
            1000,
            Basis::Beteiligt,
            true
        )]
    );
    assert_eq!(
        holds(&run, father),
        vec![sentence(
            child,
            father,
            TOWN_BIRTH,
            1000,
            Basis::Beteiligt,
            false
        )]
    );
    assert_eq!(
        holds(&run, partner),
        vec![sentence(
            child,
            partner,
            TOWN_BIRTH,
            900,
            Basis::Geschlossen,
            false
        )]
    );
    assert_eq!(
        holds(&run, witness),
        vec![sentence(
            child,
            partner,
            TOWN_BIRTH,
            900,
            Basis::Geschlossen,
            false
        )]
    );
    let home = run
        .world
        .state()
        .household_of(mother)
        .expect("her household");
    for member in run.world.state().members(home) {
        if member == mother || member == partner || member == witness || member == child {
            continue;
        }
        assert!(
            holds(&run, member).is_empty(),
            "sim {} is under 6",
            member.get()
        );
    }
    let loaded = reload(&run, &data);
    let mut run = Run {
        world: loaded,
        out: YearOut::default(),
        log: run.log,
    };
    let home = run
        .world
        .state()
        .household_of(mother)
        .expect("her household");
    run.submit(Input::MoveOut(MoveOut { household: home }));
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: mother,
        man: partner,
    }));
    assert_eq!(
        run.world.state().household_of(witness),
        run.world.state().household_of(mother),
        "the witness stays with her mother"
    );
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: mother,
        man: father,
    }));
    assert_eq!(
        run.world.state().household_of(witness),
        run.world.state().household_of(father)
    );
    let home = run
        .world
        .state()
        .household_of(mother)
        .expect("the new household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let morning = TOWN_BIRTH + 6 * 3_600;
    run.advance(morning);
    for who in [father, witness] {
        run.submit(Input::Command(Command {
            sim: who,
            object: SOFA,
            offer: 1,
        }));
    }
    let delay = i64::from(
        run.world
            .data()
            .social()
            .expect("social")
            .limits
            .talk_delay_seconds,
    );
    run.advance(morning + 1 + 3_600);
    let reached = |who: SimId| {
        (run.log.iter())
            .filter(|entry| {
                entry.subjects == [Subject::Sim(who)]
                    && matches!(entry.payload, LogPayload::Reached { object: SOFA })
            })
            .map(|entry| entry.t.as_secs())
            .max()
    };
    let later = reached(father)
        .and_then(|hers| reached(witness).map(|his| hers.max(his)))
        .expect("F and the witness reach the sofa");
    assert!(
        later + delay <= morning + 1 + 3_600,
        "the talk is inside the hour"
    );
    let refuted = (run.log.iter())
        .find(|entry| {
            matches!(
                entry.payload,
                LogPayload::Refuted {
                    by,
                    statement: Statement::VaterVon { child: born, man },
                    source: None,
                } if by == father && born == child && man == father
            )
        })
        .expect("F refutes the witness");
    let held_belief = (holds(&run, witness).into_iter())
        .find(|belief| {
            matches!(
                belief.statement,
                Statement::VaterVon { child: born, .. } if born == child
            )
        })
        .expect("the witness still holds the child");
    assert_eq!(
        held_belief,
        Belief {
            statement: Statement::VaterVon { child, man: father },
            certainty: 1000,
            since: refuted.t,
            source: Some(father),
            origin: Some(father),
            basis: Basis::Widerlegt,
            secret: false,
        }
    );
}

fn started_on(data: Data, founders: Vec<Founder>, couples: Vec<Couple>, diseases: bool) -> Run {
    let start = Start {
        world_seed: 7,
        founder_tag: DataId::new("test:paternity").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: diseases,
            nutrition: Vec::new(),
        },
        founders,
        couples,
        capacity: None,
        pool: None,
    };
    let mut run = Run {
        world: World::new(data),
        out: YearOut::default(),
        log: Vec::new(),
    };
    run.submit(Input::Start(start));
    run
}

fn reload(run: &Run, data: &Data) -> World {
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, data.clone()).expect("loads");
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    assert_eq!(loaded.save(&hashes).expect("saved again"), bytes);
    assert_eq!(
        loaded.state().house().social.beliefs,
        run.world.state().house().social.beliefs
    );
    loaded
}

/// The second partnership, formed one second after the first month, separates at the birth.
/// The first, formed at the start, is still open at the end of that month. The seven-year-old's
/// disease is due in the birth's second. The rates are the JSON numbers the data stores.
fn rates_for_the_birth_second() -> (f64, f64) {
    let tag = DataId::new("test:paternity").expect("valid");
    let draws = Draws::new(WorldSeed::new(7));
    let separation_budget = |occasion: u64, his: u64| {
        draws.exponential(
            DrawKey {
                tag: tags::SEPARATION_BUDGET,
                subject: LineageKey::founder(&tag, 0).into(),
                occasion,
                detail: LineageKey::founder(&tag, his).get(),
            },
            1.0,
        )
    };
    let first = separation_budget(0, 1);
    let second = separation_budget(1, 2);
    let formed = MONTH.as_secs() + 1;
    let half = SimDuration::from_secs(1).as_years() / 2.0;
    let separation = stored(second / (SimDuration::from_secs(BIRTH - formed).as_years() - half));
    let due = |budget: f64, rate: f64, at: i64| {
        let after = SimDuration::from_years_ceil(budget / rate)
            .expect("a due")
            .as_secs();
        at + after.max(1)
    };
    assert!(
        (1e-9..=1000.0).contains(&separation),
        "separation {separation}"
    );
    assert_eq!(
        due(second, separation, formed),
        BIRTH,
        "the second partnership"
    );
    assert!(
        due(first, separation, 0) > MONTH.as_secs(),
        "the first partnership ends in the first month: budgets {first} and {second}"
    );
    let disease = draws.exponential(
        DrawKey {
            tag: tags::DEATH_BUDGET,
            subject: LineageKey::founder(&tag, 3).into(),
            occasion: Code::new("test:dummy_disease").expect("code").key(),
            detail: 0,
        },
        1.0,
    );
    let hazard = stored(disease / (SimDuration::from_secs(BIRTH).as_years() - half));
    assert!((1e-9..=1000.0).contains(&hazard), "hazard {hazard}");
    let age = disease_age(
        7.0,
        disease,
        StepHazard {
            before: hazard,
            change_age: 0.0,
            after: hazard,
        },
    )
    .expect("an age");
    assert_eq!(
        SimDuration::from_years_ceil(age - 7.0)
            .expect("due")
            .as_secs(),
        BIRTH,
        "the disease"
    );
    (separation, hazard)
}

fn stored(rate: f64) -> f64 {
    serde_json::json!(rate).as_f64().expect("a finite rate")
}

fn disease_killing_at(per_year: f64) -> Value {
    let mut diseases = common::dummy_disease(0.0);
    diseases[0]["effect"]["death"]["per_year"]["value"] = json!(per_year);
    diseases
}

const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_MOTHER_WITHOUT_A_FATHER: u64 = 0x6a9c_7afc_a615_8ecd;
const POOL_FATHER_NOT_IN_TOWN: u64 = 0xb9b8_567f_f6ec_1066;
const POOL_BIRTH_WITHOUT_A_FATHER: i64 = 14_515_200;
const POOL_TWIN_MOTHER: u64 = 0x403a_f8b2_3398_414c;
const POOL_TWIN_FATHER: u64 = 0xd41c_26b6_8d0f_94c3;
const POOL_TWIN_BIRTH: i64 = 7_257_600;
const TOWN_MOTHER: u64 = 0xeffe_a08d_6677_b12e;
const TOWN_FATHER: u64 = 0xfcc9_94af_5747_54cd;
const TOWN_WITNESS: u64 = 0x3bc3_6c1c_9262_588b;
const TOWN_BIRTH: i64 = 12_096_000;
/// The third object of `core:house`: dining table, fridge, sofa. Three places, and the sit
/// offer is the second one.
const SOFA: u16 = 2;

fn pool_c_data(pool: &[u8], social: impl FnOnce(&mut Value)) -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/father_dies/calibration/genetics.json"),
        include_str!("../../../tests/golden/father_dies/genetics/genome_map.json"),
    );
    let demography = common::demography_from(include_str!(
        "../../../tests/golden/father_dies/calibration/demography.json"
    ));
    let definitions =
        common::definitions_with(demography, calibration, genome_map).expect("definitions");
    Data::with_pool(definitions, pool, common::pool_frequencies("pool_c"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(social))
}

/// The market would pair the mother with the man before the birth. Height 0 keeps him in
/// his own household. The pool was checked against the fixture's documents, so the change
/// goes on afterwards.
fn without_a_market(data: Data) -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/father_dies/calibration/genetics.json"),
        include_str!("../../../tests/golden/father_dies/genetics/genome_map.json"),
    );
    let mut demography = common::demography_from(include_str!(
        "../../../tests/golden/father_dies/calibration/demography.json"
    ));
    demography["values"]["core:market_search_height_female"]["value"] = json!(0.0);
    let definitions =
        common::definitions_with(demography, calibration, genome_map).expect("definitions");
    data.with_calibration(definitions.demography, definitions.calibration)
        .expect("market off")
}

fn pool_with_a_later_father() -> Vec<u8> {
    let mut pool = Pool::from_bytes(POOL_C).expect("pool_c");
    let male = pool.households[12].members[0].lineage_key;
    assert_eq!(pool.households[12].members.len(), 1);
    assert_eq!(pool.households[12].members[0].sex, Sex::Male);
    let pregnancy = &mut pool.households[28].pregnancies[0];
    assert_ne!(pregnancy.father, male);
    pregnancy.father = male;
    pool.to_bytes()
}

fn pool_world(data: Data, seed: u64, n_start: u64) -> (Run, PoolStart) {
    let pool = PoolStart::drawn(&data, seed, n_start, false).expect("a draw");
    let start = Start {
        world_seed: seed,
        founder_tag: DataId::new("test:paternity").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: None,
        pool: Some(pool.clone()),
    };
    let mut run = Run {
        world: World::new(data),
        out: YearOut::default(),
        log: Vec::new(),
    };
    run.submit(Input::Start(start));
    (run, pool)
}

fn quiet_town() -> Data {
    let data = common::repository_data_with(|demography| {
        let values = demography["values"].as_object_mut().expect("values");
        for key in ["core:separation_rate", "core:market_search_height_female"] {
            values.get_mut(key).expect(key)["value"] = json!(0.0);
        }
    });
    data.with_houses(common::repository_houses_with_day(|day| {
        day["school"]["from_age"] = json!(7);
        day["school"]["until_age"] = json!(8);
        day["work"]["from_age"] = json!(66);
        day["work"]["until_age"] = json!(67);
    }))
    .with_social(common::repository_social_with(|file| {
        file["beliefs"]["refute_ppm"] = json!(1_000_000);
    }))
}

fn draw_index(drawn: &PoolStart, key: u64) -> usize {
    (drawn.households.iter())
        .position(|household| {
            household
                .members
                .iter()
                .any(|member| member.lineage_key.get() == key)
        })
        .unwrap_or_else(|| panic!("lineage {key:x} is not in the draw"))
}

fn pregnancy_father(drawn: &PoolStart, mother: u64) -> u64 {
    (drawn.households.iter())
        .find_map(|household| {
            household
                .pregnancies
                .iter()
                .find(|pregnancy| pregnancy.mother.get() == mother)
                .map(|pregnancy| pregnancy.father.get())
        })
        .unwrap_or_else(|| panic!("no pregnancy of {mother:x}"))
}

fn sim_of(run: &Run, key: u64) -> SimId {
    (run.log.iter())
        .find_map(|entry| match entry.payload {
            LogPayload::Arrived { lineage_key } if lineage_key.get() == key => {
                match entry.subjects.as_slice() {
                    [Subject::Sim(id)] => Some(*id),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("nobody arrived with {key:x}"))
}

fn children_of(run: &Run, mother: SimId) -> Vec<SimId> {
    (run.log.iter())
        .filter_map(|entry| {
            if !matches!(entry.payload, LogPayload::Birth { .. }) {
                return None;
            }
            match entry.subjects.as_slice() {
                [Subject::Sim(child), Subject::Sim(hers), ..] if *hers == mother => Some(*child),
                _ => None,
            }
        })
        .collect()
}

fn moved_in_through(run: &mut Run, mother: SimId, birth: i64) {
    run.advance(birth - 2);
    let home = run
        .world
        .state()
        .household_of(mother)
        .expect("her household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(birth);
}

fn sentence(
    child: SimId,
    man: SimId,
    at: i64,
    certainty: u16,
    basis: Basis,
    secret: bool,
) -> Belief {
    Belief {
        statement: Statement::VaterVon { child, man },
        certainty,
        since: t(at),
        source: None,
        origin: None,
        basis,
        secret,
    }
}

fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("bytes")
}

/// Replaces the one occurrence of `from` with `to`. The widths may differ: a postcard
/// struct is its fields in order, so the social field can grow by one belief.
fn replaced(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let hits = bytes
        .windows(from.len())
        .filter(|window| *window == from)
        .count();
    assert_eq!(hits, 1, "one occurrence");
    let at = bytes
        .windows(from.len())
        .position(|window| window == from)
        .expect("found");
    let mut forged = Vec::with_capacity(bytes.len() - from.len() + to.len());
    forged.extend_from_slice(&bytes[..at]);
    forged.extend_from_slice(to);
    forged.extend_from_slice(&bytes[at + from.len()..]);
    forged
}

fn holds(run: &Run, who: SimId) -> Vec<Belief> {
    (run.world.state().house().social.beliefs.get(&who))
        .cloned()
        .unwrap_or_default()
}

fn pool_child_without_recorded_parents(run: &Run, child: SimId) {
    assert_eq!(run.world.state().biological_father(child), None);
    let person = (run.world.pedigree().persons.into_iter())
        .find(|person| person.sim == child)
        .expect("the child");
    assert_eq!(person.mother, None);
    assert_eq!(person.father, None);
    assert!(person.pool.is_some());
}

/// A man of 18 or older who arrives alone, not F.
fn solo_man(drawn: &PoolStart, father: u64) -> u64 {
    let frozen = Pool::from_bytes(common::REPOSITORY_POOL)
        .expect("the repository pool")
        .frozen_at;
    (drawn.households.iter())
        .find_map(|household| {
            if household.members.len() != 1 || household.partnership.is_some() {
                return None;
            }
            let man = &household.members[0];
            if man.sex != Sex::Male || man.lineage_key.get() == father {
                return None;
            }
            let age = SimDuration::from_secs(frozen.as_secs() - man.born.as_secs()).as_years();
            (18.0..60.0).contains(&age).then_some(man.lineage_key.get())
        })
        .expect("a solo man in the draw")
}
