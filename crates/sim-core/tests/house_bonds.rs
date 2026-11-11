//! ADR-0030 section 2 in a moved-in household: a directed bond per other person, the start
//! values of the first matching role, analytic fade of familiarity and grudge, eviction of
//! the weakest unprotected bond, what stays on moving out and what a death drops, the save
//! and the load. The expected values are the ADR's and `tools/social_reference.py`
//! `check_bonds`, by hand. Nothing here changes a choice.

mod common;

use serde_json::{Value, json};
use sim_core::canonical::PROJECTION_VERSION;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{
    ArrangePartnership, Couple, Founder, Input, MoveIn, MoveOut, SeparatePartnership, Start,
};
use sim_core::log::{Cause, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::social::bond::{ATTRACTION, Bond, FAMILIARITY};
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::version::CORE_VERSION;
use sim_core::world::{FORMAT_VERSION, LoadError, World, YearOut};

/// Start values by role, in the bond's order (ADR-0030 section 2, `check_bonds`).
const PARTNER: [u16; 6] = [700, 700, 500, 500, 600, 0];
const PARENT: [u16; 6] = [650, 650, 600, 0, 600, 0];
const CHILD: [u16; 6] = [700, 600, 300, 0, 600, 0];
const SIBLING: [u16; 6] = [550, 550, 400, 0, 500, 0];
const KIN: [u16; 6] = [500, 500, 450, 0, 300, 0];
const HOUSEMATE: [u16; 6] = [400, 450, 400, 0, 200, 0];

/// Familiarity and grudge, per day (`check_bonds`).
const FAMILIARITY_FADE: u16 = 5;
const GRUDGE_FADE: u16 = 10;

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

/// Test data: accident `accident` per year, a pregnancy of `months`, and the social data
/// after `change` when one is given. The topics name no furniture, so no talk is planned.
/// Talks have their own tests in `house_talk.rs`.
fn data_with(accident: f64, months: f64, social: Option<&dyn Fn(&mut Value)>) -> Data {
    let (mut calibration, genome_map) = common::death_files(accident, None, json!([]));
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(months);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    let data = Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses());
    match social {
        Some(change) => data.with_social(common::repository_social_without_talks(change)),
        None => data,
    }
}

/// The repository's social data, a pregnancy of one month and no accident worth the name.
fn quiet() -> Data {
    data_with(1e-9, 1.0, Some(&|_| {}))
}

/// One bond per person: a birth into a couple cannot drop the partner edge.
fn one_bond() -> Data {
    data_with(
        1e-9,
        1.0,
        Some(&|file| {
            file["limits"]["bonds"] = json!(1);
        }),
    )
}

/// An accident rate of 3 a year and the repository's social data.
fn accidents() -> Data {
    data_with(3.0, 1.0, Some(&|_| {}))
}

/// The child's birth: both parents moved in before it, 28 days after the certain conception.
const BIRTH: i64 = 56 * DAY.as_secs();

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, t(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    fn submit(&mut self, input: Input) {
        self.world.submit(input).expect("accepted");
        self.log.extend(self.world.take_log());
    }

    /// The bonds of `who` now, in the state's order.
    fn bonds(&self, who: u64) -> Vec<Bond> {
        self.bonds_of(sim(who))
    }

    fn bonds_of(&self, who: SimId) -> Vec<Bond> {
        (self.world.state().house().social.bonds.get(&who))
            .cloned()
            .unwrap_or_default()
    }
}

fn founder(years: i64, sex: Sex) -> Founder {
    Founder {
        born: t(-years * YEAR.as_secs()),
        sex,
        alleles: common::dummy(0, 0),
    }
}

fn started(data: Data, seed: u64, founders: Vec<Founder>, couples: Vec<Couple>) -> Run {
    started_tag(data, seed, "test:bonds", founders, couples, None)
}

/// `tag` is part of each founder's lineage key, and the accident draws are keyed by it.
fn started_tag(
    data: Data,
    seed: u64,
    tag: &str,
    founders: Vec<Founder>,
    couples: Vec<Couple>,
    capacity: Option<u64>,
) -> Run {
    let start = Start {
        world_seed: seed,
        founder_tag: DataId::new(tag).expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders,
        couples,
        capacity,
        pool: None,
    };
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    Run {
        world,
        out: YearOut::default(),
        log: Vec::new(),
    }
}

/// The households formed so far, in the order of their entries.
fn households(run: &Run) -> Vec<HouseholdId> {
    (run.log.iter())
        .filter_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .collect()
}

/// A woman of 30 (sim 1) and a man of 32 (sim 2) as a couple, moved into their house at
/// 23:59:59 of day 0.
fn couple(data: Data, seed: u64) -> (Run, HouseholdId) {
    couple_tagged(data, seed, "test:bonds")
}

fn couple_tagged(data: Data, seed: u64, tag: &str) -> (Run, HouseholdId) {
    let founders = vec![founder(30, Sex::Female), founder(32, Sex::Male)];
    let mut run = started_tag(
        data,
        seed,
        tag,
        founders,
        vec![Couple { woman: 0, man: 1 }],
        None,
    );
    run.advance(DAY.as_secs() - 2);
    let home = households(&run)[0];
    run.submit(Input::MoveIn(MoveIn { household: home }));
    (run, home)
}

/// The second the couple's `MoveIn` acts.
const MOVED_IN: i64 = DAY.as_secs() - 1;

fn caused_by(entry: &LogEntry) -> Vec<u64> {
    (entry.causes.as_slice().iter())
        .filter_map(|cause| match cause {
            Cause::Event(id) => Some(common::day::number(id)),
            Cause::Plan(plan) => Some(common::day::number(&plan.by)),
            _ => None,
        })
        .collect()
}

fn is_bond(payload: &LogPayload) -> bool {
    matches!(
        payload,
        LogPayload::BondFormed { .. }
            | LogPayload::BondChanged { .. }
            | LogPayload::BondDropped { .. }
    )
}

fn bond_log(run: &Run) -> Vec<&LogEntry> {
    run.log
        .iter()
        .filter(|entry| is_bond(&entry.payload))
        .collect()
}

fn formed(run: &Run, who: u64, other: u64) -> &LogEntry {
    (run.log.iter())
        .find(|entry| {
            entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::BondFormed { other: o, .. } if o == sim(other))
        })
        .expect("formed")
}

/// A birth's child, mother, father and entry number.
struct Born {
    at: i64,
    child: SimId,
    mother: SimId,
    father: SimId,
    entry: u64,
}

fn births(run: &Run) -> Vec<Born> {
    (run.log.iter())
        .filter_map(|entry| match entry.payload {
            LogPayload::Birth { .. } => {
                let ids: Vec<SimId> = entry
                    .subjects
                    .iter()
                    .map(|subject| match subject {
                        Subject::Sim(id) => *id,
                    })
                    .collect();
                Some(Born {
                    at: entry.t.as_secs(),
                    child: ids[0],
                    mother: ids[1],
                    father: ids[2],
                    entry: common::day::number(&entry.id),
                })
            }
            _ => None,
        })
        .collect()
}

fn edge(run: &Run, owner: SimId, other: SimId) -> Bond {
    (run.bonds_of(owner).into_iter())
        .find(|bond| bond.other == other)
        .expect("the bond")
}

/// Lists are non-empty, sorted, within the cap, and never to oneself.
fn sound(run: &Run) {
    let cap = usize::from(run.world.data().social().expect("social").limits.bonds);
    for (&owner, list) in &run.world.state().house().social.bonds {
        assert!(!list.is_empty() && list.len() <= cap, "{owner:?}");
        assert!(list.windows(2).all(|pair| pair[0].other < pair[1].other));
        assert!(
            list.iter().all(|bond| {
                bond.other != owner && bond.values.iter().all(|value| *value <= 1000)
            })
        );
    }
}

/// The six rows, the two fades and the cap, as `check_bonds` and the data file state them.
#[test]
fn the_start_table_is_the_one_of_the_reference() {
    let data = quiet();
    let social = data.social().expect("social");
    assert_eq!(social.bonds.start.partner, PARTNER);
    assert_eq!(social.bonds.start.parent, PARENT);
    assert_eq!(social.bonds.start.child, CHILD);
    assert_eq!(social.bonds.start.sibling, SIBLING);
    assert_eq!(social.bonds.start.kin, KIN);
    assert_eq!(social.bonds.start.housemate, HOUSEMATE);
    assert_eq!(social.bonds.familiarity_fade, FAMILIARITY_FADE);
    assert_eq!(social.bonds.grudge_fade, GRUDGE_FADE);
    assert_eq!(social.limits.bonds, 32);
    assert_eq!(
        PARTNER[ATTRACTION], 500,
        "only the partner starts with attraction"
    );
    for row in [PARENT, CHILD, SIBLING, KIN, HOUSEMATE] {
        assert_eq!(row[ATTRACTION], 0);
    }
}

/// Both directed partner edges, attraction 500, caused by the household's `core:moved_in`.
/// No `core:bond_changed`: this slice only forms.
#[test]
fn a_move_in_forms_both_partner_edges() {
    let (run, _) = couple(quiet(), 7);
    for (owner, other) in [(1, 2), (2, 1)] {
        let bond = edge(&run, sim(owner), sim(other));
        assert_eq!(bond.values, PARTNER, "attraction 500");
        assert_eq!(bond.since, t(MOVED_IN));
        let wrote = &formed(&run, owner, other);
        assert_eq!(wrote.t, t(MOVED_IN));
        assert_eq!(
            wrote.payload,
            LogPayload::BondFormed {
                other: sim(other),
                values: PARTNER,
            }
        );
    }
    let moved = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::MovedIn { .. }))
        .expect("moved in");
    let by = common::day::number(&moved.id);
    assert_eq!(caused_by(formed(&run, 1, 2)), vec![by]);
    assert_eq!(caused_by(formed(&run, 2, 1)), vec![by]);
    let entries = bond_log(&run);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].subjects, [Subject::Sim(sim(1))]);
    assert_eq!(entries[1].subjects, [Subject::Sim(sim(2))]);
    assert!(
        entries
            .iter()
            .all(|entry| !matches!(entry.payload, LogPayload::BondChanged { .. }))
    );
    sound(&run);
}

/// A child born in the house: each parent to the child, the child to each parent. The
/// child is under 6 and holds the bonds. The partner edges stay. The cause is the birth.
#[test]
fn a_birth_forms_the_parent_and_child_edges() {
    let (mut run, _) = couple(quiet(), 7);
    run.advance(BIRTH);
    let born = &births(&run)[0];
    assert_eq!(born.at, BIRTH);
    assert_eq!(
        (born.mother, born.father, born.child),
        (sim(1), sim(2), sim(3))
    );
    for parent in [sim(1), sim(2)] {
        let bond = edge(&run, parent, sim(3));
        assert_eq!(bond.values, CHILD, "the parent's edge, attraction 0");
        assert_eq!(bond.since, t(BIRTH));
        let to_parent = edge(&run, sim(3), parent);
        assert_eq!(to_parent.values, PARENT, "the child's edge, attraction 0");
        assert_eq!(to_parent.since, t(BIRTH));
    }
    assert_eq!(edge(&run, sim(1), sim(2)).values, PARTNER);
    let at_birth: Vec<&LogEntry> = bond_log(&run)
        .into_iter()
        .filter(|entry| entry.t.as_secs() == BIRTH)
        .collect();
    let expect = [(1, 3, CHILD), (2, 3, CHILD), (3, 1, PARENT), (3, 2, PARENT)];
    assert_eq!(at_birth.len(), expect.len());
    for (entry, (owner, other, values)) in at_birth.iter().zip(expect) {
        assert_eq!(entry.subjects, [Subject::Sim(sim(owner))]);
        assert_eq!(
            entry.payload,
            LogPayload::BondFormed {
                other: sim(other),
                values,
            }
        );
        assert_eq!(caused_by(entry), vec![born.entry]);
    }
    sound(&run);
}

/// Full siblings are also kin and housemates. The first matching role is the sibling, so
/// both edges take that row and not the kin row.
#[test]
fn full_siblings_take_the_sibling_row_ahead_of_kin() {
    let (mut run, _) = couple(quiet(), 7);
    run.advance(140 * DAY.as_secs());
    let born = births(&run);
    assert!(born.len() >= 2, "a second child");
    let (first, second) = (&born[0], &born[1]);
    assert_eq!(first.at, BIRTH);
    assert_eq!(first.father, sim(2));
    assert_eq!(second.father, sim(2));
    for (owner, other) in [(first.child, second.child), (second.child, first.child)] {
        let bond = edge(&run, owner, other);
        assert_eq!(bond.values, SIBLING, "sibling ahead of kin");
        assert_eq!(bond.since, t(second.at));
        assert_ne!(bond.values, KIN);
    }
    assert_eq!(edge(&run, sim(1), second.child).values, CHILD);
    assert_eq!(edge(&run, second.child, sim(1)).values, PARENT);
    let wrote = (bond_log(&run).into_iter())
        .find(|entry| {
            entry.t.as_secs() == second.at
                && entry.subjects == [Subject::Sim(first.child)]
                && matches!(entry.payload, LogPayload::BondFormed { other, .. } if other == second.child)
        })
        .expect("the sibling edge");
    assert_eq!(caused_by(wrote), vec![second.entry]);
    sound(&run);
}

/// A founder of 15 is fostered into the only couple. She is not kin, so each edge with an
/// adult is the housemate row, attraction 0. The couple stays on the partner row.
#[test]
fn a_fostered_housemate_takes_the_housemate_row() {
    let founders = vec![
        founder(30, Sex::Female),
        founder(32, Sex::Male),
        founder(15, Sex::Female),
    ];
    let mut run = started(quiet(), 7, founders, vec![Couple { woman: 0, man: 1 }]);
    run.advance(DAY.as_secs() - 2);
    let home = households(&run)[0];
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let moved = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::MovedIn { .. }))
        .expect("moved in");
    assert_eq!(moved.subjects.len(), 3, "the girl lives with the couple");
    for adult in [1, 2] {
        for (owner, other) in [(adult, 3), (3, adult)] {
            let bond = edge(&run, sim(owner), sim(other));
            assert_eq!(bond.values, HOUSEMATE, "attraction 0");
            assert_eq!(bond.values[ATTRACTION], 0);
            assert_eq!(bond.since, t(MOVED_IN));
        }
    }
    assert_eq!(edge(&run, sim(1), sim(2)).values, PARTNER);
    assert_eq!(edge(&run, sim(1), sim(2)).values[ATTRACTION], 500);
    sound(&run);
}

/// Half siblings share the mother, live together, and are kin from φ = 1/8. Kin comes
/// before housemate, and they are not full siblings, so both edges take the kin row.
#[test]
fn half_siblings_take_the_kin_row_ahead_of_housemate() {
    let founders = vec![
        founder(30, Sex::Female),
        founder(32, Sex::Male),
        founder(28, Sex::Male),
    ];
    let mut run = started(quiet(), 7, founders, vec![Couple { woman: 0, man: 1 }]);
    run.advance(DAY.as_secs() - 2);
    run.submit(Input::MoveIn(MoveIn {
        household: households(&run)[0],
    }));
    run.advance(BIRTH);
    let first = births(&run).pop().expect("the first child");
    assert_eq!(first.father, sim(2));
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(3),
    }));
    let home = *households(&run).last().expect("the new household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(140 * DAY.as_secs());
    let born = births(&run);
    assert!(born.len() >= 2, "the second child");
    let second = &born[1];
    assert_eq!(second.father, sim(3));
    for (owner, other) in [(first.child, second.child), (second.child, first.child)] {
        let bond = edge(&run, owner, other);
        assert_eq!(bond.values, KIN, "kin ahead of housemate");
        assert_eq!(bond.values[ATTRACTION], 0);
        assert_ne!(bond.values, SIBLING);
        assert_eq!(bond.since, t(second.at));
    }
    let wrote = (bond_log(&run).into_iter())
        .find(|entry| {
            entry.subjects == [Subject::Sim(first.child)]
                && matches!(entry.payload, LogPayload::BondFormed { other, .. } if other == second.child)
        })
        .expect("the kin edge");
    assert_eq!(caused_by(wrote), vec![second.entry]);
    sound(&run);
}

/// Familiarity falls by 5 a day from the move-in, truncating, and the stored values stay
/// until a move-out writes them down. Outside they do not fall. Coming back sets `since`
/// to the re-entry and keeps the settled values, so the gap adds nothing.
#[test]
fn familiarity_falls_and_a_move_out_settles_it_until_reentry() {
    let (mut run, home) = couple(quiet(), 7);
    let ten = MOVED_IN + 10 * DAY.as_secs();
    run.advance(MOVED_IN + 17_280);
    let bond = edge(&run, sim(1), sim(2));
    assert_eq!(bond.values, PARTNER, "no event writes the fade");
    assert_eq!(
        bond.values_at(t(MOVED_IN + 17_280), FAMILIARITY_FADE, GRUDGE_FADE),
        [700, 700, 500, 500, 599, 0]
    );
    run.advance(ten - 1);
    let bond = edge(&run, sim(1), sim(2));
    assert_eq!(bond.values[FAMILIARITY], 600);
    assert_eq!(
        bond.values_at(t(ten), FAMILIARITY_FADE, GRUDGE_FADE)[FAMILIARITY],
        550
    );
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let settled = edge(&run, sim(1), sim(2));
    assert_eq!(settled.values, [700, 700, 500, 500, 550, 0]);
    assert_eq!(settled.since, t(ten));
    let gap = ten + 10 * DAY.as_secs();
    run.advance(gap);
    let frozen = edge(&run, sim(1), sim(2));
    assert_eq!(frozen.values[FAMILIARITY], 550);
    assert_eq!(frozen.since, t(ten));
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let back = gap + 1;
    let resumed = edge(&run, sim(1), sim(2));
    assert_eq!(resumed.values[FAMILIARITY], 550);
    assert_eq!(resumed.since, t(back));
    assert_eq!(
        resumed.values_at(t(back), FAMILIARITY_FADE, GRUDGE_FADE)[FAMILIARITY],
        550
    );
    let later = back + 10 * DAY.as_secs();
    run.advance(later);
    let bond = edge(&run, sim(1), sim(2));
    assert_eq!(bond.values[FAMILIARITY], 550, "still the settled value");
    assert_eq!(
        bond.values_at(t(later), FAMILIARITY_FADE, GRUDGE_FADE)[FAMILIARITY],
        500
    );
    assert_eq!(
        bond_log(&run).len(),
        2,
        "moving does not write a bond entry"
    );
    sound(&run);
}

/// At a separation the man leaves and his bonds are written down. The woman stays, so her
/// bond keeps the move-in second and only the read value falls. No bond entry.
#[test]
fn a_separation_settles_the_leaver_and_leaves_the_stayer() {
    let (mut run, _) = couple(quiet(), 7);
    let ten = MOVED_IN + 10 * DAY.as_secs();
    run.advance(ten - 1);
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let hers = edge(&run, sim(1), sim(2));
    assert_eq!(hers.since, t(MOVED_IN));
    assert_eq!(hers.values[FAMILIARITY], 600);
    assert_eq!(
        hers.values_at(t(ten), FAMILIARITY_FADE, GRUDGE_FADE)[FAMILIARITY],
        550
    );
    let his = edge(&run, sim(2), sim(1));
    assert_eq!(his.since, t(ten));
    assert_eq!(his.values, [700, 700, 500, 500, 550, 0]);
    assert!(bond_log(&run).iter().all(|entry| entry.t.as_secs() != ten));
    sound(&run);
}

/// Cap 1, and the partner is still a housemate: the birth cannot drop that bond, so the
/// parents form nothing. The child has room for the mother and then cannot drop her.
#[test]
fn a_protected_bond_is_not_dropped_and_the_new_one_is_not_formed() {
    let (mut run, _) = couple(one_bond(), 7);
    run.advance(BIRTH);
    assert!(
        bond_log(&run)
            .iter()
            .all(|entry| !matches!(entry.payload, LogPayload::BondDropped { .. }))
    );
    assert_eq!(run.bonds(1), vec![edge(&run, sim(1), sim(2))]);
    assert_eq!(run.bonds(2), vec![edge(&run, sim(2), sim(1))]);
    assert!(run.bonds(1).iter().all(|bond| bond.other != sim(3)));
    assert!(run.bonds(2).iter().all(|bond| bond.other != sim(3)));
    assert_eq!(run.bonds(3).len(), 1);
    assert_eq!(edge(&run, sim(3), sim(1)).values, PARENT);
    assert!(run.bonds(3).iter().all(|bond| bond.other != sim(2)));
    let formed_at_birth: Vec<_> = bond_log(&run)
        .into_iter()
        .filter(|entry| entry.t.as_secs() == BIRTH)
        .collect();
    assert_eq!(formed_at_birth.len(), 1);
    assert_eq!(formed_at_birth[0].subjects, [Subject::Sim(sim(3))]);
    sound(&run);
}

/// Cap 1, and the man has left before the birth: he is not a housemate and not kin, so her
/// bond to him falls and the bond to the child forms. The drop follows the new bond and is
/// caused by it. The tie on a larger id is the pure `insert` test.
#[test]
fn an_unprotected_bond_falls_so_the_child_can_form() {
    let (mut run, _) = couple(one_bond(), 7);
    let left = 40 * DAY.as_secs();
    run.advance(left - 1);
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    run.advance(BIRTH);
    let born = &births(&run)[0];
    assert_eq!(born.at, BIRTH);
    assert_eq!(born.father, sim(2));
    let at_birth: Vec<&LogEntry> = bond_log(&run)
        .into_iter()
        .filter(|entry| entry.t.as_secs() == BIRTH)
        .collect();
    assert_eq!(at_birth.len(), 3);
    assert_eq!(at_birth[0].subjects, [Subject::Sim(sim(1))]);
    assert_eq!(
        at_birth[0].payload,
        LogPayload::BondFormed {
            other: sim(3),
            values: CHILD,
        }
    );
    assert_eq!(caused_by(at_birth[0]), vec![born.entry]);
    assert_eq!(at_birth[1].subjects, [Subject::Sim(sim(1))]);
    assert_eq!(
        at_birth[1].payload,
        LogPayload::BondDropped { other: sim(2) }
    );
    assert_eq!(
        caused_by(at_birth[1]),
        vec![common::day::number(&at_birth[0].id)]
    );
    assert_eq!(
        at_birth[2].payload,
        LogPayload::BondFormed {
            other: sim(1),
            values: PARENT,
        }
    );
    assert_eq!(run.bonds(1).len(), 1);
    assert_eq!(edge(&run, sim(1), sim(3)).values, CHILD);
    assert!(run.bonds(1).iter().all(|bond| bond.other != sim(2)));
    assert_eq!(edge(&run, sim(3), sim(1)).values, PARENT);
    assert!(run.bonds(3).iter().all(|bond| bond.other != sim(2)));
    assert!(run.bonds(2).iter().any(|bond| bond.other == sim(1)));
    assert!(run.bonds(2).iter().all(|bond| bond.other != sim(3)));
    sound(&run);
}

/// The seconds of the woman's death and the man's. Seed 10 with the founder tag
/// `test:memory` is the world whose death order the memory tests already pin: she dies
/// after the move-in and before him. The tag is part of the lineage key, so another tag
/// draws other accidents.
fn deaths_of_seed_10() -> (i64, i64) {
    static SECONDS: std::sync::OnceLock<(i64, i64)> = std::sync::OnceLock::new();
    *SECONDS.get_or_init(|| {
        let (mut run, _) = couple_tagged(accidents(), 10, "test:memory");
        run.advance(150 * DAY.as_secs());
        let at = |who: u64| {
            (run.log.iter())
                .find(|entry| {
                    entry.subjects == [Subject::Sim(sim(who))]
                        && matches!(entry.payload, LogPayload::Death { .. })
                })
                .map(|entry| entry.t.as_secs())
                .expect("a death")
        };
        (at(1), at(2))
    })
}

/// She dies: her list goes with her and nothing is written. He keeps the bond to her, still
/// dated at the move-in, so it was not settled by the death.
#[test]
fn a_death_drops_the_dead_persons_bonds_only() {
    let (mother_dies, father_dies) = deaths_of_seed_10();
    assert!(MOVED_IN < mother_dies && mother_dies < father_dies);
    let (mut run, _) = couple_tagged(accidents(), 10, "test:memory");
    run.advance(mother_dies - 1);
    assert_eq!(edge(&run, sim(1), sim(2)).values, PARTNER);
    run.advance(mother_dies);
    assert!(run.bonds(1).is_empty());
    let his = edge(&run, sim(2), sim(1));
    assert_eq!(his.values, PARTNER);
    assert_eq!(his.since, t(MOVED_IN));
    assert!(
        bond_log(&run)
            .iter()
            .all(|entry| entry.t.as_secs() != mother_dies)
    );
    assert!(
        bond_log(&run)
            .iter()
            .all(|entry| !matches!(entry.payload, LogPayload::BondDropped { .. }))
    );
    sound(&run);
}

/// ADR-0030 section 2 keeps a leaver's own bonds "frozen like the memories". Memories are
/// lost at death and at emigration, so the own bonds are too. Two days after the birth the
/// household moves out, the same world as `a_death_outside_drops_the_frozen_memories`: each
/// person keeps its bonds frozen. The woman dies later, outside any house. Her list goes and
/// nothing is written per bond. The man keeps his bond to her. A save taken after her death
/// loads: the owner of a bond list is a living sim.
#[test]
fn a_death_outside_drops_the_dead_persons_own_bonds() {
    let (mother_dies, father_dies) = deaths_of_seed_10();
    let out = BIRTH + 2 * DAY.as_secs();
    assert!(out < mother_dies && mother_dies < father_dies);
    let (mut run, home) = couple_tagged(accidents(), 10, "test:memory");
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let his = edge(&run, sim(2), sim(1));
    assert!(run.bonds(1).iter().any(|bond| bond.other == sim(2)));
    run.advance(mother_dies - 1);
    assert_eq!(edge(&run, sim(2), sim(1)), his);
    run.advance(mother_dies);
    assert!(run.bonds(1).is_empty());
    assert_eq!(edge(&run, sim(2), sim(1)), his);
    assert!(
        bond_log(&run)
            .iter()
            .all(|entry| entry.t.as_secs() != mother_dies)
    );
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, accidents()).expect("loads");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    sound(&run);
}

/// The same reading for an emigration, on the world of
/// `an_emigration_outside_drops_the_frozen_memories`: seed 10 and capacity 1, moved out two
/// days after the birth. At the last second of the year the household leaves. Each leaver's
/// own list goes and nothing is written per bond. A save taken after that loads.
#[test]
fn an_emigration_outside_drops_the_leavers_own_bonds() {
    let out = BIRTH + 2 * DAY.as_secs();
    let leaves = YEAR.as_secs() - 1;
    assert!(out < leaves);
    let founders = vec![founder(30, Sex::Female), founder(32, Sex::Male)];
    let mut run = started_tag(
        quiet(),
        10,
        "test:memory",
        founders,
        vec![Couple { woman: 0, man: 1 }],
        Some(1),
    );
    run.advance(DAY.as_secs() - 2);
    let home = households(&run)[0];
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let his = edge(&run, sim(2), sim(1));
    assert!(run.bonds(1).iter().any(|bond| bond.other == sim(2)));
    run.advance(leaves - 1);
    assert_eq!(edge(&run, sim(2), sim(1)), his);
    run.advance(leaves);
    let gone = |who: u64| {
        run.log.iter().any(|entry| {
            entry.t.as_secs() == leaves
                && entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::Emigrated { .. })
        })
    };
    assert!(gone(1) && gone(2) && gone(3));
    assert!(run.bonds(1).is_empty() && run.bonds(2).is_empty() && run.bonds(3).is_empty());
    assert!(
        bond_log(&run)
            .iter()
            .all(|entry| entry.t.as_secs() != leaves)
    );
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, quiet()).expect("loads");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

/// A birth outside a house writes no bond. The household has not moved in.
#[test]
fn no_bond_forms_before_a_household_moves_in() {
    let founders = vec![founder(30, Sex::Female), founder(32, Sex::Male)];
    let mut run = started(quiet(), 7, founders, vec![Couple { woman: 0, man: 1 }]);
    run.advance(BIRTH);
    assert!(births(&run).len() == 1);
    assert!(run.world.state().house().social.bonds.is_empty());
    assert!(bond_log(&run).is_empty());
}

/// Without social data a move-in and a birth write no bond, and the social part stays empty.
#[test]
fn without_social_data_nobody_forms_a_bond() {
    let (mut run, _) = couple(data_with(1e-9, 1.0, None), 7);
    run.advance(BIRTH);
    assert!(births(&run).len() == 1);
    assert!(bond_log(&run).is_empty());
    assert!(run.world.state().house().social.is_empty());
}

fn encode_bond(bond: &Bond) -> Vec<u8> {
    postcard::to_allocvec(bond).expect("bytes")
}

/// Replaces the one occurrence of `from` with `to`. Both encodings have the same width.
fn spliced(bytes: &[u8], from: &Bond, to: &Bond) -> Vec<u8> {
    let (old, new) = (encode_bond(from), encode_bond(to));
    assert_eq!(old.len(), new.len(), "the forgery keeps the width");
    let at = bytes
        .windows(old.len())
        .position(|window| window == old.as_slice())
        .expect("found");
    let mut forged = bytes.to_vec();
    forged[at..at + new.len()].copy_from_slice(&new);
    forged
}

/// The core and projection version bytes of a save this core just wrote.
fn version_bytes(bytes: &[u8]) -> (usize, usize) {
    let rest = bytes.strip_prefix(b"SIMAGENT").expect("magic");
    let (format, rest) = postcard::take_from_bytes::<u32>(rest).expect("format");
    assert_eq!(format, FORMAT_VERSION);
    let core_at = bytes.len() - rest.len();
    let (core, rest) = postcard::take_from_bytes::<u32>(rest).expect("core");
    assert_eq!(core, CORE_VERSION);
    assert_eq!(bytes[core_at], 0x44);
    let (_, rest) = postcard::take_from_bytes::<u64>(rest).expect("hash");
    let proj_at = bytes.len() - rest.len();
    let (projection, _) = postcard::take_from_bytes::<u32>(rest).expect("projection");
    assert_eq!(projection, PROJECTION_VERSION);
    assert_eq!(bytes[proj_at], 0x17);
    (core_at, proj_at)
}

/// A same-width second after `clock`: the largest time whose postcard width matches `since`.
fn later_same_width(since: i64, clock: i64) -> i64 {
    let width = postcard::to_allocvec(&t(since)).expect("bytes").len();
    let max_zig = (1u128 << (7 * width)) - 1;
    let later = i64::try_from(max_zig / 2).expect("fits");
    assert!(later > clock + 1);
    assert_eq!(
        postcard::to_allocvec(&t(later)).expect("bytes").len(),
        width
    );
    later
}

/// The second of the couple's second birth, found by running to day 140: two births, and
/// the later one early enough that twenty days after a mid-week save still precede day 140.
fn second_birth_at() -> i64 {
    static AT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *AT.get_or_init(|| {
        let (mut run, _) = couple(quiet(), 7);
        run.advance(140 * DAY.as_secs());
        let born = births(&run);
        assert_eq!(born.len(), 2);
        assert!(BIRTH < born[1].at && born[1].at + 24 * DAY.as_secs() < 140 * DAY.as_secs());
        born[1].at
    })
}

/// A save a few seconds after the second birth loads and runs on. A value above 1000, a
/// bond to oneself, a bond to nobody, a `since` after the open second, an unsorted list and
/// a repeated other are refused. A save of the previous core or projection is refused.
/// The format stays 2: the bonds are a field of the house part.
#[test]
fn a_save_in_the_middle_of_the_week_loads_and_an_old_one_does_not() {
    assert_eq!(FORMAT_VERSION, 2);
    assert_eq!(CORE_VERSION, 68);
    assert_eq!(PROJECTION_VERSION, 23);
    let second = second_birth_at();
    let (mut run, _) = couple(quiet(), 7);
    let at = second + 3 * DAY.as_secs() + 5;
    assert!(at + 20 * DAY.as_secs() < 140 * DAY.as_secs());
    run.advance(at);
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let data = quiet();
    let (mut loaded, hashes) = World::load(&bytes, data.clone()).expect("loaded");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    let horizon = at + 20 * DAY.as_secs();
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, t(horizon), &mut |_| false);
    run.advance(horizon);
    let tail: Vec<&LogEntry> = run.log.iter().filter(|entry| entry.t > t(at)).collect();
    assert_eq!(
        serde_json::to_string(&loaded.take_log()).expect("json"),
        serde_json::to_string(&tail).expect("json")
    );

    let load = |bytes: &[u8]| World::load(bytes, data.clone()).err();
    let hers = run.bonds(1);
    let partner = hers
        .iter()
        .find(|bond| bond.other == sim(2))
        .expect("the partner");
    let mut strong = *partner;
    strong.values[0] = 1001;
    assert_eq!(
        load(&spliced(&bytes, partner, &strong)),
        Some(LoadError::Inconsistent)
    );
    let mut self_bond = *partner;
    self_bond.other = sim(1);
    assert_eq!(
        load(&spliced(&bytes, partner, &self_bond)),
        Some(LoadError::Inconsistent)
    );
    let mut unknown = *partner;
    unknown.other = sim(50);
    assert_eq!(
        load(&spliced(&bytes, partner, &unknown)),
        Some(LoadError::Inconsistent)
    );
    let child_bond = hers
        .iter()
        .find(|bond| bond.values == CHILD)
        .expect("a child");
    let mut late = *child_bond;
    late.since = t(later_same_width(child_bond.since.as_secs(), at));
    assert_eq!(
        load(&spliced(&bytes, child_bond, &late)),
        Some(LoadError::Inconsistent)
    );

    let (first, second) = (hers[hers.len() - 2], hers[hers.len() - 1]);
    assert!(first.other < second.other);
    let (a, b) = (encode_bond(&first), encode_bond(&second));
    let mut pattern = a.clone();
    pattern.extend_from_slice(&b);
    let place = bytes
        .windows(pattern.len())
        .position(|window| window == pattern.as_slice())
        .expect("in order");
    let mut unsorted = bytes.clone();
    unsorted[place..place + b.len()].copy_from_slice(&b);
    unsorted[place + b.len()..place + a.len() + b.len()].copy_from_slice(&a);
    assert_eq!(load(&unsorted), Some(LoadError::Inconsistent));
    let mut repeated = second;
    repeated.other = first.other;
    assert_eq!(
        load(&spliced(&bytes, &second, &repeated)),
        Some(LoadError::Inconsistent)
    );

    let (core_at, proj_at) = version_bytes(&bytes);
    let mut old_core = bytes.clone();
    old_core[core_at] = 0x42;
    assert_eq!(load(&old_core), Some(LoadError::ForeignCoreVersion));
    let mut old_projection = bytes.clone();
    old_projection[proj_at] = 0x16;
    assert_eq!(
        load(&old_projection),
        Some(LoadError::ForeignProjectionVersion)
    );
    assert!(load(&bytes).is_none());
}
