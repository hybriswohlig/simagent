//! The acceptance scenario of ADR-0001: `tests/scenarios/fork_no_density.json`, 16 fixed
//! founder couples of 20 to 25 years. In 8 couples one partner has the dummy allele twice
//! (4 mothers, 4 fathers), all other founders have it once. Only these couples have
//! children; nobody comes or goes, nobody pairs again. Death per cause from hazard
//! budgets on test data (ADR-0017 section 5): accident 0.02 per year and senescence with
//! Gompertz `a` 1e-12, `b` 0.1 per year for everyone, `test:dummy_disease` 0.3 per year
//! from 18 for the homozygous while the switch is on. Conception monthly with 0.05 while both live,
//! the mother is 18 to 45 years old and not pregnant; ten months of pregnancy. The parent
//! run lasts 60 years with the switch off; the fork at year 5 switches it on.
//!
//! Expected are the properties of ADR-0001, not values of a run. The scenario values are
//! test values (protected list); a change of seed to make (6) pass is forbidden
//! (ADR-0000). Over random seeds (6) holds with a probability above 0.999 (ADR-0001: a
//! Monte Carlo estimate on a simplified model gives a failure rate of 2.5e-4).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use sim_core::genome::meiosis::{Mutation, SegmentMeiosis};
use sim_core::genome::{LocusAlleles, Sex, SexChromosome};
use sim_core::ids::{EventId, SimId};
use sim_core::input::{ForkOverride, Input};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, LogEntry, LogPayload, Subject, log_hash};
use sim_core::params::ParamChange;
use sim_core::pedigree::Pedigree;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
/// The fork point T_5.
const T5: i64 = 5 * Y;

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }
}

/// The test data of the scenario (ADR-0017 section 5): the repository's map and calibration
/// with accident 0.02 and senescence `a` 1e-12, `b` 0.1 per year for both sexes, the
/// disease of the walking skeleton only, from 18 years on, the fertility of the old model
/// (the window 18 to 45, 0.05 per month, no pause, wish 40, no twins), the market off and no
/// separation.
fn fork_data() -> sim_core::data::Data {
    let (mut calibration, genome_map) = common::files();
    let values = &mut calibration["values"];
    for sex in ["female", "male"] {
        values[format!("core:accident_{sex}")]["value"] = serde_json::json!(0.02);
        values[format!("core:senescence_a_{sex}")]["value"] = serde_json::json!(1e-12);
        values[format!("core:senescence_b_{sex}")]["value"] = serde_json::json!(0.1);
    }
    calibration["diseases"] = common::dummy_disease(18.0);
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 0.05);
    // The market off and no separation (ADR-0017 section 5).
    demography["values"]["core:market_search_height_female"]["value"] = serde_json::json!(0.0);
    demography["values"]["core:separation_rate"]["value"] = serde_json::json!(0.0);
    let definitions =
        common::definitions_with(demography, calibration, genome_map).expect("definitions");
    sim_core::data::Data::new(definitions).expect("checked")
}

/// The data hash of the test data (`tools/state_reference.py`).
#[test]
fn the_test_data_are_those_of_the_reference() {
    assert_eq!(fork_data().hash(), 0x48c8_a138_26dd_546c);
}

/// The scenario: its `Start` and its end.
fn scenario() -> (Input, i64) {
    let text = include_str!("../../../tests/scenarios/fork_no_density.json");
    let value: serde_json::Value = serde_json::from_str(text).expect("json");
    let start =
        serde_json::from_value(serde_json::json!({ "start": value["start"] })).expect("a start");
    (start, value["until"].as_i64().expect("an end"))
}

/// The parent run, or with `fork` the fork at year 5 with the switch on.
fn run(fork: bool) -> Run {
    let (start, until) = scenario();
    let mut world = World::new(fork_data());
    world.submit(start).expect("accepted");
    let mut run = Run {
        log: world.take_log(),
        world,
        out: YearOut::default(),
    };
    if fork {
        run.advance(T5 - 1);
        let switch_on = Input::ForkOverride(ForkOverride {
            changes: vec![ParamChange::HereditaryDiseases(true)],
        });
        run.world.submit(switch_on).expect("accepted");
        run.log.extend(run.world.take_log());
    }
    run.advance(until);
    run
}

/// The dummy allele twice: the only alternative alleles of this scenario.
fn homozygous(alleles: &[LocusAlleles]) -> bool {
    alleles.len() == 1 && alleles[0].copies == [1, 1]
}

/// What one meiosis recorded: its segments, the sex chromosome passed on and the new
/// mutations.
type MeiosisRecord = (Vec<SegmentMeiosis>, SexChromosome, Vec<Mutation>);

/// A conception as both runs can compare it: by lineage keys, never by SimId.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Conception {
    mother: LineageKey,
    father: LineageKey,
    child: LineageKey,
    at: SimTime,
    /// The meioses of mother and father: segments with start strand and crossovers,
    /// the sex chromosome passed on and the new mutations.
    genome: Vec<MeiosisRecord>,
}

/// What a run says about one born lineage key (ADR-0001): parent keys, genome, the
/// times of birth and death. No SimId and no EventId; the conception is in
/// `Projection::conceptions`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Member {
    /// The keys of mother and father, `None` for a founder.
    parents: Option<(LineageKey, LineageKey)>,
    sex: Sex,
    alleles: Vec<LocusAlleles>,
    born: SimTime,
    died: Option<SimTime>,
}

/// Everything of a member but its death.
type Origin = (
    Option<(LineageKey, LineageKey)>,
    Sex,
    Vec<LocusAlleles>,
    SimTime,
);

impl Member {
    fn origin(&self) -> Origin {
        (self.parents, self.sex, self.alleles.clone(), self.born)
    }
}

/// What a run says by lineage key: the members of its pedigree and its conceptions,
/// those lost with the death of the mother included.
struct Projection {
    members: BTreeMap<LineageKey, Member>,
    conceptions: BTreeMap<LineageKey, Conception>,
    /// The founder couples, by the keys of woman and man.
    couples: Vec<(LineageKey, LineageKey)>,
}

/// Ten months of 28 days (the scenario's `pregnancy_months`).
const PREGNANCY: i64 = 10 * 28 * 86_400;

fn project(run: &Run) -> Projection {
    let pedigree: Pedigree = run.world.state().pedigree();
    let key_of: BTreeMap<SimId, LineageKey> = pedigree
        .persons
        .iter()
        .map(|person| (person.sim, person.lineage_key))
        .collect();
    let members: BTreeMap<LineageKey, Member> = pedigree
        .persons
        .iter()
        .map(|person| {
            assert_eq!(person.mother.is_some(), person.father.is_some());
            let parents = person
                .mother
                .zip(person.father)
                .map(|(mother, father)| (key_of[&mother], key_of[&father]));
            let member = Member {
                parents,
                sex: person.sex,
                alleles: person.alleles.clone(),
                born: person.born,
                died: person.died,
            };
            (person.lineage_key, member)
        })
        .collect();
    let couples = pedigree
        .partnerships
        .iter()
        .map(|couple| (key_of[&couple.woman], key_of[&couple.man]))
        .collect();
    let sim = |subject: &Subject| match subject {
        Subject::Sim(id) => key_of[id],
    };
    let mut conceptions = BTreeMap::new();
    let mut strands = BTreeMap::new();
    for entry in &run.log {
        match &entry.payload {
            LogPayload::Conception { lineage_key } => {
                let conception = Conception {
                    mother: sim(&entry.subjects[0]),
                    father: sim(&entry.subjects[1]),
                    child: *lineage_key,
                    at: entry.t,
                    genome: Vec::new(),
                };
                conceptions.insert(*lineage_key, conception);
            }
            LogPayload::Meiosis {
                genome_key,
                segments,
                sex_chromosome,
                mutations,
            } => strands
                .entry(genome_key.get())
                .or_insert_with(Vec::new)
                .push((segments.clone(), *sex_chromosome, mutations.clone())),
            _ => {}
        }
    }
    for conception in conceptions.values_mut() {
        let meioses = &strands[&conception.child.get()];
        assert_eq!(meioses.len(), 2, "two meioses per conception (ADR-0004)");
        conception.genome.clone_from(meioses);
    }
    // The pedigree and the log agree: every born child has the parents of its
    // conception and is born ten months after it; a founder was never conceived.
    for (key, member) in &members {
        match (member.parents, conceptions.get(key)) {
            (None, None) => {}
            (Some(parents), Some(conception)) => {
                assert_eq!(parents, (conception.mother, conception.father), "{key:?}");
                let born = conception.at.as_secs() + PREGNANCY;
                assert_eq!(member.born.as_secs(), born, "{key:?}");
            }
            _ => panic!("{key:?}: parents without conception or the other way round"),
        }
    }
    Projection {
        members,
        conceptions,
        couples,
    }
}

/// `fork_point_hash_equals_parent`: the year hashes of years 0 to 4 are those of the
/// parent run, that of year 5 differs.
#[test]
fn fork_point_hash_equals_parent() {
    let (parent, fork) = (run(false), run(true));
    assert_eq!(parent.out.year_hashes.len(), 60);
    assert_eq!(fork.out.year_hashes.len(), 60);
    assert_eq!(parent.out.year_hashes[..5], fork.out.year_hashes[..5]);
    assert_ne!(parent.out.year_hashes[5], fork.out.year_hashes[5]);
}

/// `fork_without_override_equals_original`: a fork without override, run to the fork
/// point and on, is the original run: all year hashes and the log hash are equal.
#[test]
fn fork_without_override_equals_original() {
    let parent = run(false);
    let (start, until) = scenario();
    let mut world = World::new(fork_data());
    world.submit(start).expect("accepted");
    let mut fork = Run {
        log: world.take_log(),
        world,
        out: YearOut::default(),
    };
    fork.advance(T5 - 1);
    fork.advance(until);
    assert_eq!(fork.out.year_hashes, parent.out.year_hashes);
    assert_eq!(log_hash(&fork.log), log_hash(&parent.log));
    assert_eq!(fork.world.state_hash(), parent.world.state_hash());
}

/// `fork_projection_by_lineage_key`, compared by lineage key, never by SimId.
#[test]
fn fork_projection_by_lineage_key() {
    let parent = project(&run(false));
    let fork = project(&run(true));
    let at = SimTime::from_secs(T5);

    // (1) Every key conceived before T_5 exists in both runs with the same parents and
    // genome; so do the founders.
    let before = |projection: &Projection| -> Vec<Conception> {
        projection
            .conceptions
            .values()
            .filter(|conception| conception.at < at)
            .cloned()
            .collect()
    };
    assert_eq!(before(&parent), before(&fork), "(1) conceptions before T_5");
    let founders = |projection: &Projection| -> Vec<(LineageKey, Origin)> {
        projection
            .members
            .iter()
            .filter(|(_, member)| member.parents.is_none())
            .map(|(key, member)| (*key, member.origin()))
            .collect()
    };
    assert_eq!(founders(&parent), founders(&fork), "(1) founders");
    assert_eq!(founders(&parent).len(), 32);
    // A key born in both runs has the same parent keys, sex, genome and birth there.
    let mut both = 0;
    for (key, ours) in &fork.members {
        if let Some(theirs) = parent.members.get(key) {
            assert_eq!(ours.origin(), theirs.origin(), "(1) {key:?}");
            both += 1;
        }
    }
    assert!(both > 32, "(1) no child is born in both runs");

    // (2) Persons who are not homozygous and live in both runs die at the same time.
    // (3) The homozygous die in the fork not later.
    let mut earlier = 0;
    for (key, ours) in &fork.members {
        let Some(theirs) = parent.members.get(key) else {
            continue;
        };
        if homozygous(&ours.alleles) {
            let not_later = match (ours.died, theirs.died) {
                (_, None) => true,
                (Some(ours), Some(theirs)) => ours <= theirs,
                (None, Some(_)) => false,
            };
            assert!(not_later, "(3) {key:?} dies later in the fork");
            if ours.died != theirs.died {
                earlier += 1;
            }
        } else {
            assert_eq!(ours.died, theirs.died, "(2) {key:?}");
        }
    }

    // (4) Couples of two heterozygotes have the same conceptions (key, time, genome).
    // (5) For mixed couples those of the fork are a subset of the parent's, with the
    // same time and genome.
    let of = |projection: &Projection, couple: (LineageKey, LineageKey)| -> Vec<Conception> {
        projection
            .conceptions
            .values()
            .filter(|conception| (conception.mother, conception.father) == couple)
            .cloned()
            .collect()
    };
    assert_eq!(parent.couples, fork.couples);
    assert_eq!(parent.couples.len(), 16);
    let homozygous = |key: &LineageKey| homozygous(&parent.members[key].alleles);
    let (mut mixed, mut fewer, mut after) = (0, 0, 0);
    for &couple in &parent.couples {
        let (theirs, ours) = (of(&parent, couple), of(&fork, couple));
        if homozygous(&couple.0) || homozygous(&couple.1) {
            mixed += 1;
            for conception in &ours {
                assert!(theirs.contains(conception), "(5) {conception:?}");
            }
            if ours.len() < theirs.len() {
                fewer += 1;
            }
        } else {
            assert_eq!(ours, theirs, "(4) {couple:?}");
            after += ours.iter().filter(|conception| conception.at >= at).count();
        }
    }
    assert_eq!(mixed, 8);

    // (6) Not empty: at least one homozygous person dies earlier in the fork, at least
    // one mixed couple has fewer conceptions there, and the couples of two heterozygotes
    // have at least 3 conceptions after T_5 (together: one couple alone can lose a
    // partner early, and the failure rate of ADR-0001 holds only for the eight together).
    assert!(earlier >= 1, "(6) nobody homozygous dies earlier");
    assert!(fewer >= 1, "(6) no mixed couple has fewer conceptions");
    assert!(
        after >= 3,
        "(6) {after} conceptions of heterozygous couples after T_5"
    );
}

/// `draw_keys_unique_in_log`: every draw key is the cause of at most one log entry, in
/// the parent run and in the fork. Every death and conception names its draw, so the
/// check covers every entry that cites one.
#[test]
fn draw_keys_unique_in_log() {
    for fork in [false, true] {
        let run = run(fork);
        let mut seen: BTreeMap<(u64, u64, u64, u64), EventId> = BTreeMap::new();
        for entry in &run.log {
            let draws: BTreeSet<(u64, u64, u64, u64)> = entry
                .causes
                .as_slice()
                .iter()
                .filter_map(|cause| match cause {
                    Cause::Random(draw) => {
                        Some((draw.tag, draw.subject, draw.occasion, draw.detail))
                    }
                    _ => None,
                })
                .collect();
            // A meiosis names no draw: its draws follow from the genome key and the
            // side (ADR-0004 section 7), and the entry holds what they gave.
            if matches!(
                entry.payload,
                LogPayload::Death { .. } | LogPayload::Conception { .. }
            ) {
                assert!(
                    !draws.is_empty(),
                    "fork {fork}: {:?} names no draw",
                    entry.id
                );
            }
            for draw in draws {
                if let Some(first) = seen.insert(draw, entry.id) {
                    panic!("fork {fork}: {draw:x?} in {first:?} and {:?}", entry.id);
                }
            }
        }
    }
}

/// ADR-0004 test 5 on the parent run, year by year: the creation entries so far are as
/// many as the SimIds handed out, the death entries as many as the archive holds, and
/// every SimId of the log is in the pedigree.
#[test]
fn yearly_balance_log_matches_state() {
    let (start, until) = scenario();
    let mut world = World::new(fork_data());
    world.submit(start).expect("accepted");
    let mut run = Run {
        log: world.take_log(),
        world,
        out: YearOut::default(),
    };
    let mut year = 0;
    while (year + 1) * Y - 1 <= until {
        run.advance((year + 1) * Y - 1);
        let created = run
            .log
            .iter()
            .filter(|entry| {
                matches!(
                    entry.payload,
                    LogPayload::FounderAdded { .. } | LogPayload::Birth { .. }
                )
            })
            .count();
        let died = run
            .log
            .iter()
            .filter(|entry| matches!(entry.payload, LogPayload::Death { .. }))
            .count();
        let state = run.world.state();
        assert_eq!(
            u64::try_from(created).expect("small"),
            state.sim_ids_allocated(),
            "year {year}"
        );
        assert_eq!(
            u64::try_from(died).expect("small"),
            state.archive_len(),
            "year {year}"
        );
        let known: BTreeSet<SimId> = state
            .pedigree()
            .persons
            .iter()
            .map(|person| person.sim)
            .collect();
        for entry in &run.log {
            for Subject::Sim(id) in &entry.subjects {
                assert!(known.contains(id), "year {year}: sim {}", id.get());
            }
        }
        year += 1;
    }
    assert_eq!(year, 60);
}
