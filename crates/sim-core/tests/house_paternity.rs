//! ADR-0031 section 6 and acceptance test 5: at a birth in a moved-in household the mother,
//! the biological father, her partner and everyone else from 6 hold one `VaterVon`. The
//! expected sentences are `tools/social_reference.py` `paternity_beliefs`, by hand. A birth
//! before move-in writes none. Beliefs do not change kinship or the pedigree, and a save
//! loads back to the same bytes.

mod common;

use common::day::{self, Run};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{
    ArrangePartnership, Couple, Founder, Input, MoveIn, SeparatePartnership, Start,
};
use sim_core::log::{Cause, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::social::belief::{Basis, Belief, Statement};
use sim_core::time::{MONTH, SimTime, YEAR};
use sim_core::world::{World, YearOut};

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
/// data without talk furniture.
fn life_data(identical: f64) -> Data {
    let (mut calibration, genome_map) = common::death_files(1e-9, None, json!([]));
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(1.0);
    demography["values"]["core:twins_identical"]["value"] = json!(identical);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(|_| {}))
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
