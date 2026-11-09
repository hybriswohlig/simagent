//! ADR-0030 section 1 in a moved-in household: the memory of a person, written at the birth of
//! an own child, the death of a relative, the start and the end of the own partnership; its
//! strength and the one `Forget` at the earliest zero; displacement in a full list; what a person
//! keeps on moving out and loses by dying or by emigrating; the save and the load. The expected
//! values are the ADR's and `tools/social_reference.py --check`'s, by hand.

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{
    ArrangePartnership, Couple, Founder, Input, MoveIn, MoveOut, SeparatePartnership, Start,
};
use sim_core::log::{Cause, ForgetReason, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::social::memory::Memory;
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::world::{World, YearOut};

/// The kinds of memory by number, as `data/social/social.json` lists them.
const CHILD_BORN: u16 = 1;
const RELATIVE_DIED: u16 = 2;
const PARTNERSHIP_BEGAN: u16 = 3;
const PARTNERSHIP_ENDED: u16 = 4;

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

/// Test data: accident `accident` per year, the woman's fertility window with a certain
/// conception at the first month start and a pregnancy of `months`, the repository's houses and,
/// if given, the social data after `change`.
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
        Some(change) => data.with_social(common::repository_social_with(change)),
        None => data,
    }
}

/// The social data of the repository with the strength and the fade of the kind `kind`.
fn kind(file: &mut Value, kind: u16, (strength, fade): (u16, u16)) {
    let kind = &mut file["memory"]["kinds"][usize::from(kind)];
    kind["strength"] = json!(strength);
    kind["fade"] = json!(fade);
}

/// The repository's kinds of memory, a pregnancy of one month and no accident worth the name.
fn quiet() -> Data {
    data_with(1e-9, 1.0, Some(&|_| {}))
}

/// The child's birth: both parents moved in before it, 28 days after the certain conception.
const BIRTH: i64 = 56 * DAY.as_secs();

/// Child born, 800 at 100 a day: zero after exactly 8 days (ADR-0030 acceptance test 1).
fn eight_days() -> Data {
    data_with(1e-9, 1.0, Some(&|file| kind(file, 1, (800, 100))))
}

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

    /// The memories of `who` now.
    fn memories(&self, who: u64) -> Vec<Memory> {
        let house = self.world.state().house();
        (house.social.memories.get(&sim(who)))
            .cloned()
            .unwrap_or_default()
    }

    /// The second of the open `Forget` of `who`.
    fn forget_at(&self, who: u64) -> Option<i64> {
        self.world.forget_at(sim(who)).map(SimTime::as_secs)
    }

    /// The entries about memories of `who`, as (second, payload), in the log's order.
    fn about(&self, who: u64) -> Vec<(i64, LogPayload)> {
        (self.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(who))])
            .filter(|entry| {
                matches!(
                    entry.payload,
                    LogPayload::Remembered { .. } | LogPayload::Forgot { .. }
                )
            })
            .map(|entry| (entry.t.as_secs(), entry.payload.clone()))
            .collect()
    }

    /// The numbers of the entries of the kind `pick` takes.
    fn entry_of(&self, who: u64, kind: u16) -> &LogEntry {
        (self.log.iter())
            .find(|entry| {
                entry.subjects == [Subject::Sim(sim(who))]
                    && matches!(entry.payload, LogPayload::Remembered { kind: k, .. } if k == kind)
            })
            .expect("the entry")
    }
}

fn founders() -> Vec<Founder> {
    let founder = |years: i64, sex: Sex| Founder {
        born: t(-years * YEAR.as_secs()),
        sex,
        alleles: common::dummy(0, 0),
    };
    vec![founder(30, Sex::Female), founder(32, Sex::Male)]
}

fn started(data: Data, seed: u64, couples: Vec<Couple>, capacity: Option<u64>) -> Run {
    let start = Start {
        world_seed: seed,
        founder_tag: DataId::new("test:memory").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: founders(),
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

/// The households formed by the founders, in the order of their entries.
fn households(run: &Run) -> Vec<HouseholdId> {
    (run.log.iter())
        .filter_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .collect()
}

/// A woman of 30 (sim 1) and a man of 32 (sim 2) as a couple, moved into their house at
/// 23:59:59 of day 0, the first morning at 6:00 of day 1.
fn couple(data: Data, seed: u64) -> (Run, HouseholdId) {
    let mut run = started(data, seed, vec![Couple { woman: 0, man: 1 }], None);
    run.advance(DAY.as_secs() - 2);
    let home = households(&run)[0];
    run.submit(Input::MoveIn(MoveIn { household: home }));
    (run, home)
}

/// The number of an entry's id and the numbers of its causes: the entry it follows from, or the
/// entry that planned the event it ran in.
fn caused_by(entry: &LogEntry) -> Vec<u64> {
    (entry.causes.as_slice().iter())
        .filter_map(|cause| match cause {
            Cause::Event(id) => Some(common::day::number(id)),
            Cause::Plan(plan) => Some(common::day::number(&plan.by)),
            _ => None,
        })
        .collect()
}

/// A person who remembers is old enough and lives, in each state of a run.
fn remembers_only_who_may(world: &World) {
    let state = world.state();
    let now = world.closed_through();
    for (&owner, list) in &state.house().social.memories {
        let born = state.born(owner).expect("a person");
        let age = (now.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
        assert!(age >= 6, "{owner:?} is {age} and remembers");
        assert!(!list.is_empty() && list.windows(2).all(|pair| pair[0].key() < pair[1].key()));
    }
}

/// ADR-0030 section 1 and its acceptance test 1: a child is born in a moved-in household, and
/// each parent remembers it with the other parent and the child; 800 at 100 a day is 400 after
/// 4 days and 0 after exactly 691 200 s, one second before it 1; the one `Forget` stands in that
/// second, writes `core:forgot` with the reason `faded`, and no other is left.
#[test]
fn a_birth_is_remembered_until_the_pinned_second() {
    let (mut run, _) = couple(eight_days(), 7);
    run.advance(BIRTH);
    let birth = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::Birth { .. }))
        .expect("the birth");
    assert_eq!(birth.t.as_secs(), BIRTH);
    let born = common::day::number(&birth.id);
    for (parent, other) in [(1, 2), (2, 1)] {
        let memory = Memory {
            kind: CHILD_BORN,
            with: [Some(sim(other)), Some(sim(3))],
            at: t(BIRTH),
            valence: 900,
            strength: 800,
            since: t(BIRTH),
            fade: 100,
        };
        assert_eq!(run.memories(parent), vec![memory]);
        let wrote = LogPayload::Remembered {
            kind: CHILD_BORN,
            with: memory.with,
            at: t(BIRTH),
            strength: 800,
        };
        assert_eq!(run.about(parent), vec![(BIRTH, wrote)]);
        assert_eq!(caused_by(run.entry_of(parent, CHILD_BORN)), vec![born]);
        assert_eq!(run.forget_at(parent), Some(BIRTH + 691_200));
    }
    run.advance(BIRTH + 4 * DAY.as_secs());
    assert_eq!(
        run.memories(1)[0].strength_at(t(BIRTH + 4 * DAY.as_secs())),
        400
    );
    run.advance(BIRTH + 691_199);
    assert_eq!(run.memories(1).len(), 1, "one second before");
    assert_eq!(run.memories(1)[0].strength_at(t(BIRTH + 691_199)), 1);
    assert_eq!(run.about(1).len(), 1);
    run.advance(BIRTH + 691_200);
    for parent in [1, 2] {
        assert!(run.memories(parent).is_empty());
        assert_eq!(
            run.forget_at(parent),
            None,
            "the last memory was the only one"
        );
        let lost = LogPayload::Forgot {
            kind: CHILD_BORN,
            with: [Some(sim(if parent == 1 { 2 } else { 1 })), Some(sim(3))],
            at: t(BIRTH),
            reason: ForgetReason::Faded,
        };
        let entries = run.about(parent);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1], (BIRTH + 691_200, lost));
        // The reason of the entry is the plan of the forgetting: its `by` is the entry that wrote
        // the memory.
        let forgot = run.log.iter().find(|e| {
            e.subjects == [Subject::Sim(sim(parent))]
                && matches!(e.payload, LogPayload::Forgot { .. })
        });
        let remembered = common::day::number(&run.entry_of(parent, CHILD_BORN).id);
        assert_eq!(caused_by(forgot.expect("forgot")), vec![remembered]);
    }
    remembers_only_who_may(&run.world);
}

/// Every change of the list cancels the open `Forget` and plans it anew at the earliest zero
/// (ADR-0030 section 1): the end of the partnership, 400 at 100 a day, written two days after the
/// birth (800 at 100 a day) falls due before it; the woman forgets it first and the one
/// `Forget` moves on to the birth's zero. The man moved out with the separation: his list is
/// frozen and he has no event.
#[test]
fn a_new_memory_with_an_earlier_zero_moves_the_forget() {
    let data = data_with(
        1e-9,
        1.0,
        Some(&|file| {
            kind(file, CHILD_BORN, (800, 100));
            kind(file, PARTNERSHIP_ENDED, (400, 100));
        }),
    );
    let (mut run, _) = couple(data, 7);
    run.advance(BIRTH + 2 * DAY.as_secs() - 1);
    assert_eq!(run.forget_at(1), Some(BIRTH + 8 * DAY.as_secs()));
    let apart = BIRTH + 2 * DAY.as_secs();
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let ended = |other: u64| Memory {
        kind: PARTNERSHIP_ENDED,
        with: [Some(sim(other)), None],
        at: t(apart),
        valence: -700,
        strength: 400,
        since: t(apart),
        fade: 100,
    };
    let born = |other: u64, strength: u16, since: i64| Memory {
        kind: CHILD_BORN,
        with: [Some(sim(other)), Some(sim(3))],
        at: t(BIRTH),
        valence: 900,
        strength,
        since: t(since),
        fade: 100,
    };
    // She keeps both and plans the earlier zero: 400 at 100 a day, 4 days.
    assert_eq!(run.memories(1), vec![born(2, 800, BIRTH), ended(2)]);
    assert_eq!(run.forget_at(1), Some(apart + 4 * DAY.as_secs()));
    // He remembered the end too, then moved out: frozen at the strength of that second, no event.
    assert_eq!(run.memories(2), vec![born(1, 600, apart), ended(1)]);
    assert_eq!(run.forget_at(2), None);
    let entries = run.about(2);
    assert_eq!(entries.len(), 2, "no entry for the freezing: {entries:?}");
    let zero = apart + 4 * DAY.as_secs();
    run.advance(zero - 1);
    assert_eq!(run.memories(1).len(), 2);
    run.advance(zero);
    assert_eq!(run.memories(1), vec![born(2, 800, BIRTH)]);
    assert_eq!(run.forget_at(1), Some(BIRTH + 8 * DAY.as_secs()));
    let lost = run.about(1).pop().expect("an entry");
    let forgot = LogPayload::Forgot {
        kind: PARTNERSHIP_ENDED,
        with: [Some(sim(2)), None],
        at: t(apart),
        reason: ForgetReason::Faded,
    };
    assert_eq!(lost, (zero, forgot));
    run.advance(BIRTH + 8 * DAY.as_secs());
    assert!(run.memories(1).is_empty());
    assert_eq!(run.forget_at(1), None);
    // The frozen list of the man stands as it was: nothing ticks for him.
    assert_eq!(run.memories(2), vec![born(1, 600, apart), ended(1)]);
}

/// ADR-0030 section 1, "Neu": in a full list the weakest at `now` of the old ones and the new one
/// falls, a tie goes to the older `at`; if the newcomer is the one, nothing is written. A list
/// of one (`memories` 1), the birth at second 0 of the test and the end of the partnership two
/// days later, neither fading.
#[test]
fn a_full_list_displaces_the_weakest_and_the_newcomer_counts() {
    let apart = BIRTH + 2 * DAY.as_secs();
    for (born, ended, falls) in [
        (300, 800, CHILD_BORN),
        (800, 300, PARTNERSHIP_ENDED),
        (500, 500, CHILD_BORN),
    ] {
        let data = data_with(
            1e-9,
            1.0,
            Some(&|file| {
                file["limits"]["memories"] = json!(1);
                file["limits"]["keep_on_move_out"] = json!(1);
                kind(file, CHILD_BORN, (born, 0));
                kind(file, PARTNERSHIP_ENDED, (ended, 0));
            }),
        );
        let (mut run, _) = couple(data, 7);
        run.advance(apart - 1);
        run.submit(Input::SeparatePartnership(SeparatePartnership {
            woman: sim(1),
            man: sim(2),
        }));
        for (who, other) in [(1, 2), (2, 1)] {
            let list = run.memories(who);
            assert_eq!(list.len(), 1, "cap 1 for {born} and {ended}");
            let entries = run.about(who);
            let child = [Some(sim(other)), Some(sim(3))];
            if falls == CHILD_BORN {
                assert_eq!(list[0].kind, PARTNERSHIP_ENDED);
                assert_eq!(entries.len(), 3, "{entries:?}");
                let remembered = LogPayload::Remembered {
                    kind: PARTNERSHIP_ENDED,
                    with: [Some(sim(other)), None],
                    at: t(apart),
                    strength: ended,
                };
                let displaced = LogPayload::Forgot {
                    kind: CHILD_BORN,
                    with: child,
                    at: t(BIRTH),
                    reason: ForgetReason::Displaced,
                };
                assert_eq!(entries[1], (apart, remembered));
                assert_eq!(entries[2], (apart, displaced));
                // The displacement follows from the new memory's entry.
                let new = common::day::number(&run.entry_of(who, PARTNERSHIP_ENDED).id);
                let forgot = run.log.iter().rev().find(|e| {
                    e.subjects == [Subject::Sim(sim(who))]
                        && matches!(e.payload, LogPayload::Forgot { .. })
                });
                assert_eq!(caused_by(forgot.expect("forgot")), vec![new]);
            } else {
                assert_eq!(
                    list[0].kind, CHILD_BORN,
                    "the weaker newcomer is not taken in"
                );
                assert_eq!(entries.len(), 1, "no entry for a refusal: {entries:?}");
            }
            // Neither fades: no `Forget` anywhere.
            assert_eq!(run.forget_at(who), None);
        }
    }
}

/// ADR-0030 section 1, "Auszug und Tod": a person who moves out keeps its strongest memories,
/// frozen at their strength then with `since` that second and no event, and nothing is written
/// per memory. With `keep_on_move_out` 1 the man keeps the birth (600 after two days at 100 a
/// day) and drops the end of the partnership (400); with 8 he keeps both.
#[test]
fn moving_out_keeps_the_strongest_frozen() {
    let apart = BIRTH + 2 * DAY.as_secs();
    let strong = data_with(
        1e-9,
        1.0,
        Some(&|file| {
            file["limits"]["keep_on_move_out"] = json!(1);
            kind(file, CHILD_BORN, (800, 100));
            kind(file, PARTNERSHIP_ENDED, (400, 100));
        }),
    );
    let (mut run, _) = couple(strong, 7);
    run.advance(apart - 1);
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let kept = run.memories(2);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        (kept[0].kind, kept[0].strength, kept[0].since),
        (CHILD_BORN, 600, t(apart))
    );
    assert_eq!(run.forget_at(2), None);
    assert_eq!(
        run.about(2).len(),
        2,
        "remembered twice, nothing for the freezing"
    );
}

/// A household that moves out freezes everyone's list (ADR-0030 section 1, "Auszug und Tod"): the
/// birth, 800 at 100 a day, is 600 after two days; the strength stays 600 however long the
/// household is outside, no `Forget` stays, no entry is written. Moving in again sets `since` to
/// that second and plans the `Forget` from it: 6 days later, whether it moves in a day or eight
/// days after.
#[test]
fn a_household_moving_out_freezes_and_moving_in_resumes() {
    let out = BIRTH + 2 * DAY.as_secs();
    let (mut run, home) = couple(eight_days(), 7);
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    for (who, other) in [(1, 2), (2, 1)] {
        let list = run.memories(who);
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].strength, list[0].since), (600, t(out)));
        assert_eq!(list[0].with, [Some(sim(other)), Some(sim(3))]);
        assert_eq!(run.forget_at(who), None);
        assert_eq!(run.about(who).len(), 1);
    }
    // The frozen lists save and load.
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, _) = World::load(&bytes, eight_days()).expect("loads");
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    for back in [out + DAY.as_secs(), BIRTH + 10 * DAY.as_secs()] {
        let (mut run, home) = couple(eight_days(), 7);
        run.advance(out - 1);
        run.submit(Input::MoveOut(MoveOut { household: home }));
        run.advance(back - 1);
        assert_eq!(run.memories(1)[0].strength, 600, "frozen outside");
        run.submit(Input::MoveIn(MoveIn { household: home }));
        for who in [1, 2] {
            let list = run.memories(who);
            assert_eq!((list[0].strength, list[0].since), (600, t(back)));
            // 600 at 100 a day from the second of the move in: 6 days.
            assert_eq!(run.forget_at(who), Some(back + 6 * DAY.as_secs()));
            assert_eq!(
                run.about(who).len(),
                1,
                "nothing is forgotten on the way in"
            );
        }
        run.advance(back + 6 * DAY.as_secs());
        assert!(run.memories(1).is_empty() && run.forget_at(1).is_none());
        let faded = run.about(1).pop().expect("an entry");
        assert!(matches!(
            faded,
            (s, LogPayload::Forgot { reason: ForgetReason::Faded, .. }) if s == back + 6 * DAY.as_secs()
        ));
    }
}

/// The two single founders live in houses of their own and pair by an input: both remember the
/// start of the partnership with the other (caused by `core:partnered`), then move into the
/// couple's household, outside a house, which freezes the list (ADR-0030 section 1).
#[test]
fn the_start_of_a_partnership_is_remembered_by_both() {
    let mut run = started(quiet(), 7, Vec::new(), None);
    run.advance(DAY.as_secs() - 2);
    let homes = households(&run);
    assert_eq!(homes.len(), 2);
    for home in homes {
        run.submit(Input::MoveIn(MoveIn { household: home }));
    }
    let pairing = 3 * DAY.as_secs();
    run.advance(pairing - 1);
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let partnered = run
        .log
        .iter()
        .find(|entry| matches!(entry.payload, LogPayload::Partnered { .. }))
        .expect("partnered");
    assert_eq!(partnered.t.as_secs(), pairing);
    for (who, other) in [(1, 2), (2, 1)] {
        let memory = Memory {
            kind: PARTNERSHIP_BEGAN,
            with: [Some(sim(other)), None],
            at: t(pairing),
            valence: 700,
            strength: 800,
            since: t(pairing),
            fade: 20,
        };
        assert_eq!(run.memories(who), vec![memory], "frozen on moving out");
        assert_eq!(run.forget_at(who), None);
        assert_eq!(
            caused_by(run.entry_of(who, PARTNERSHIP_BEGAN)),
            vec![common::day::number(&partnered.id)]
        );
    }
}

/// The second and the entry number of the first death of `who`.
fn death_of(run: &Run, who: u64) -> Option<(i64, u64)> {
    (run.log.iter())
        .find(|entry| {
            entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::Death { .. })
        })
        .map(|entry| (entry.t.as_secs(), common::day::number(&entry.id)))
}

/// The seconds of the deaths of the child, the woman and the man in the world of seed 10 with an
/// accident rate of 3 a year (found by running it once): the child's comes 17 days after its
/// birth, the woman's and the man's after.
fn deaths_of_seed_10() -> (i64, i64, i64) {
    static SECONDS: std::sync::OnceLock<(i64, i64, i64)> = std::sync::OnceLock::new();
    *SECONDS.get_or_init(|| {
        let (mut run, _) = couple(accidents(), 10);
        run.advance(150 * DAY.as_secs());
        let second = |who: u64| death_of(&run, who).expect("a death").0;
        (second(3), second(1), second(2))
    })
}

/// Test data with an accident rate of 3 a year and the repository's kinds of memory.
fn accidents() -> Data {
    data_with(3.0, 1.0, Some(&|_| {}))
}

/// ADR-0030 section 1: the child dies (seed 10: its accident comes 17 days after its birth), and
/// each parent, living in a moved-in household, remembers the death of a relative (φ 1/4 is at
/// least 1/8); later the woman dies: she loses all her memories and her `Forget`, the man
/// remembers the end of the partnership; then he dies and loses his. Nothing is written for the
/// loss, nobody under 6 remembers, and the memories keep their order second by second.
#[test]
fn a_death_is_remembered_by_relatives_and_ends_a_persons_memories() {
    let (child_dies, mother_dies, father_dies) = deaths_of_seed_10();
    assert!(BIRTH < child_dies && child_dies < mother_dies && mother_dies < father_dies);
    let (mut run, _) = couple(accidents(), 10);
    run.advance(child_dies - 1);
    assert_eq!(run.memories(1).len(), 1, "the birth");
    run.advance(child_dies);
    let died = Memory {
        kind: RELATIVE_DIED,
        with: [Some(sim(3)), None],
        at: t(child_dies),
        valence: -900,
        strength: 900,
        since: t(child_dies),
        fade: 15,
    };
    let (_, child_death) = death_of(&run, 3).expect("the child's death");
    for who in [1, 2] {
        let list = run.memories(who);
        assert_eq!(list.len(), 2, "{who}");
        assert_eq!(list[1], died);
        assert_eq!(
            caused_by(run.entry_of(who, RELATIVE_DIED)),
            vec![child_death]
        );
    }
    // The birth's zero comes first: 900 at 15 a day is 60 days.
    let zero = BIRTH + 60 * DAY.as_secs();
    assert_eq!(run.forget_at(1), Some(zero));
    remembers_only_who_may(&run.world);
    // Her accident: all her memories go with her Forget; he remembers the end, in the same second.
    run.advance(mother_dies - 1);
    assert_eq!(run.memories(1).len(), 2);
    run.advance(mother_dies);
    assert!(run.memories(1).is_empty() && run.forget_at(1).is_none());
    assert!(
        run.about(1)
            .iter()
            .all(|(_, payload)| !matches!(payload, LogPayload::Forgot { .. }))
    );
    let ended = LogPayload::Remembered {
        kind: PARTNERSHIP_ENDED,
        with: [Some(sim(1)), None],
        at: t(mother_dies),
        strength: 800,
    };
    assert_eq!(run.about(2).last(), Some(&(mother_dies, ended)));
    assert_eq!(run.memories(2).len(), 3);
    remembers_only_who_may(&run.world);
    // His birth memory fades at its zero; his accident ends the rest.
    assert!(zero < father_dies);
    run.advance(zero);
    let faded = LogPayload::Forgot {
        kind: CHILD_BORN,
        with: [Some(sim(1)), Some(sim(3))],
        at: t(BIRTH),
        reason: ForgetReason::Faded,
    };
    assert_eq!(run.about(2).last(), Some(&(zero, faded)));
    assert_eq!(run.forget_at(2), Some(child_dies + 60 * DAY.as_secs()));
    run.advance(father_dies);
    assert!(run.memories(2).is_empty() && run.forget_at(2).is_none());
    assert_eq!(run.about(2).last().map(|(at, _)| *at), Some(zero));
    remembers_only_who_may(&run.world);
}

/// ADR-0030 section 1, "Auszug und Tod": two days after the birth the household moves out, so
/// each parent keeps that one memory frozen at its strength then (900 at 15 a day is 870) with
/// `since` that second and no `Forget`. The woman dies later, outside any moved-in household, and
/// loses the list; nothing is written for the loss. The man still lives outside and keeps his.
/// A save taken after her death loads: every memory owner is a living sim.
#[test]
fn a_death_outside_drops_the_frozen_memories() {
    let (child_dies, mother_dies, _) = deaths_of_seed_10();
    let out = BIRTH + 2 * DAY.as_secs();
    assert!(out < child_dies && child_dies < mother_dies);
    let (mut run, home) = couple(accidents(), 10);
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let frozen = |other: u64| Memory {
        kind: CHILD_BORN,
        with: [Some(sim(other)), Some(sim(3))],
        at: t(BIRTH),
        valence: 900,
        strength: 870,
        since: t(out),
        fade: 15,
    };
    for (who, other) in [(1, 2), (2, 1)] {
        assert_eq!(run.memories(who), vec![frozen(other)]);
        assert_eq!(run.forget_at(who), None);
    }
    // Outside, the list does not fade, and a death in the town is not remembered.
    run.advance(mother_dies - 1);
    assert_eq!(run.memories(1), vec![frozen(2)]);
    assert_eq!(run.forget_at(1), None);
    let birth = LogPayload::Remembered {
        kind: CHILD_BORN,
        with: [Some(sim(2)), Some(sim(3))],
        at: t(BIRTH),
        strength: 900,
    };
    assert_eq!(run.about(1), vec![(BIRTH, birth.clone())]);
    run.advance(mother_dies);
    assert_eq!(death_of(&run, 1).map(|(at, _)| at), Some(mother_dies));
    assert!(run.memories(1).is_empty());
    assert_eq!(run.forget_at(1), None);
    assert_eq!(run.about(1), vec![(BIRTH, birth)]);
    assert_eq!(run.memories(2), vec![frozen(1)]);
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, accidents()).expect("loads");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

/// ADR-0030 section 1: a person who leaves the world loses every memory. Two days after the birth
/// the household moves out, so each parent keeps that one memory frozen (900 at 15 a day is 870)
/// with `since` that second and no `Forget`. It emigrates later, still outside a house: seed 10
/// and capacity 1 give `e` = 1/2, and that household's draw for year 0 leaves, at the last second
/// of the year. Both lists go and nothing is written for the loss. A save taken after the
/// emigration loads: every memory owner is a living sim.
#[test]
fn an_emigration_outside_drops_the_frozen_memories() {
    let out = BIRTH + 2 * DAY.as_secs();
    let leaves = YEAR.as_secs() - 1;
    assert!(out < leaves);
    let mut run = started(quiet(), 10, vec![Couple { woman: 0, man: 1 }], Some(1));
    run.advance(DAY.as_secs() - 2);
    let home = households(&run)[0];
    run.submit(Input::MoveIn(MoveIn { household: home }));
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let frozen = |other: u64| Memory {
        kind: CHILD_BORN,
        with: [Some(sim(other)), Some(sim(3))],
        at: t(BIRTH),
        valence: 900,
        strength: 870,
        since: t(out),
        fade: 15,
    };
    for (who, other) in [(1, 2), (2, 1)] {
        assert_eq!(run.memories(who), vec![frozen(other)]);
        assert_eq!(run.forget_at(who), None);
    }
    // Outside, the list does not fade.
    run.advance(leaves - 1);
    assert_eq!(run.memories(1), vec![frozen(2)]);
    assert_eq!(run.forget_at(1), None);
    let birth = LogPayload::Remembered {
        kind: CHILD_BORN,
        with: [Some(sim(2)), Some(sim(3))],
        at: t(BIRTH),
        strength: 900,
    };
    assert_eq!(run.about(1), vec![(BIRTH, birth.clone())]);
    assert!(
        run.log
            .iter()
            .all(|entry| !matches!(entry.payload, LogPayload::Emigrated { .. }))
    );
    run.advance(leaves);
    let gone = |who: u64| {
        run.log.iter().any(|entry| {
            entry.t.as_secs() == leaves
                && entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::Emigrated { .. })
        })
    };
    assert!(gone(1) && gone(2) && gone(3));
    assert!(run.memories(1).is_empty() && run.memories(2).is_empty());
    assert!(run.world.state().house().social.is_empty());
    assert_eq!(run.forget_at(1), None);
    assert_eq!(run.forget_at(2), None);
    assert_eq!(run.about(1), vec![(BIRTH, birth)]);
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, hashes) = World::load(&bytes, quiet()).expect("loads");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

/// Without social data nothing remembers even in a moved-in household, and nothing is planned.
#[test]
fn without_social_data_nobody_remembers() {
    let (mut run, _) = couple(data_with(1e-9, 1.0, None), 7);
    run.advance(BIRTH + 2 * DAY.as_secs());
    assert!(
        run.log
            .iter()
            .any(|e| matches!(e.payload, LogPayload::Birth { .. }))
    );
    assert!(run.about(1).is_empty() && run.about(2).is_empty());
    assert!(run.world.state().house().social.is_empty());
    assert_eq!(run.forget_at(1), None);
}

/// ADR-0030, "Konsequenzen": a run without a move-in plans no `Forget`, writes no entry of the
/// memory and keeps its domain log and its events: they are the same with and without the social
/// data, and so is the projection, which ends with the empty maps of the house part and of the
/// memories (the data hash alone differs).
#[test]
fn a_run_without_move_in_does_not_change() {
    let run_with = |data: Data| {
        let mut run = started(data, 7, vec![Couple { woman: 0, man: 1 }], None);
        run.advance(BIRTH + 30 * DAY.as_secs());
        run
    };
    let (plain, social) = (run_with(data_with(1e-9, 1.0, None)), run_with(quiet()));
    assert_ne!(plain.world.data().hash(), social.world.data().hash());
    assert!(
        plain
            .log
            .iter()
            .any(|e| matches!(e.payload, LogPayload::Birth { .. }))
    );
    let text = |run: &Run| serde_json::to_string(&run.log).expect("json");
    assert_eq!(text(&plain), text(&social));
    assert_eq!(plain.world.pending_events(), social.world.pending_events());
    assert_eq!(
        plain.world.canonical_bytes(),
        social.world.canonical_bytes()
    );
    assert_eq!(plain.world.state_hash(), social.world.state_hash());
    assert!(social.world.canonical_bytes().ends_with(&[0, 0, 0, 0, 0]));
    assert!(social.log.iter().all(|e| !matches!(
        e.payload,
        LogPayload::Remembered { .. } | LogPayload::Forgot { .. }
    )));
}

/// A save in the middle of the week, with memories and their `Forget`, loads to the same state
/// and runs on to the same log; the loader refuses a memory out of range, one whose fade is not
/// its kind's, one whose strength moved the earliest zero away from the open `Forget`, and a
/// list out of order (ADR-0030 section 5).
#[test]
fn a_save_in_the_middle_of_the_week_loads_and_runs_on() {
    let (mut run, _) = couple(accidents(), 10);
    let (child_dies, _, _) = deaths_of_seed_10();
    run.advance(child_dies + 3 * DAY.as_secs() + 5);
    let [birth, died] = run.memories(1).try_into().expect("two memories");
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let data = accidents();
    let (mut loaded, hashes) = World::load(&bytes, data.clone()).expect("loaded");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    for who in [1, 2] {
        assert_eq!(loaded.forget_at(sim(who)), run.world.forget_at(sim(who)));
        assert!(loaded.forget_at(sim(who)).is_some());
    }
    let mut out = YearOut::default();
    let until = t(150 * DAY.as_secs());
    loaded.advance_until(&mut out, until, &mut |_| false);
    run.advance(150 * DAY.as_secs());
    let log = loaded.take_log();
    let tail: Vec<&LogEntry> = run
        .log
        .iter()
        .filter(|e| e.t > t(child_dies + 3 * DAY.as_secs() + 5))
        .collect();
    assert_eq!(
        serde_json::to_string(&log).expect("json"),
        serde_json::to_string(&tail).expect("json")
    );
    assert_eq!(loaded.state_hash(), run.world.state_hash());

    // Tampered saves. A memory as the state holds it: its postcard bytes are found in the save.
    let encode = |memory: &Memory| postcard::to_allocvec(memory).expect("bytes");
    let load = |bytes: &[u8]| World::load(bytes, data.clone()).err();
    let swapped = |from: &Memory, to: &Memory| {
        let (from, to) = (encode(from), encode(to));
        assert_eq!(from.len(), to.len());
        let at = bytes
            .windows(from.len())
            .position(|w| w == from)
            .expect("found");
        let mut forged = bytes.clone();
        forged[at..at + to.len()].copy_from_slice(&to);
        forged
    };
    let with = |memory: &Memory, change: fn(&mut Memory)| {
        let mut changed = *memory;
        change(&mut changed);
        swapped(memory, &changed)
    };
    // A strength of 1001.
    let strong = with(&birth, |m| m.strength = 1001);
    assert_eq!(
        load(&strong),
        Some(sim_core::world::LoadError::Inconsistent)
    );
    // A fade that is not the kind's.
    let slow = with(&birth, |m| m.fade = 16);
    assert_eq!(load(&slow), Some(sim_core::world::LoadError::Inconsistent));
    // A strength that moves the earliest zero away from the open `Forget`.
    let weak = with(&birth, |m| m.strength = 899);
    assert_eq!(load(&weak), Some(sim_core::world::LoadError::Inconsistent));
    // An unsorted list: the two memories exchanged.
    assert!(birth.key() < died.key());
    let (a, b) = (encode(&birth), encode(&died));
    let at = (bytes.windows(a.len() + b.len()))
        .position(|w| w[..a.len()] == a[..] && w[a.len()..] == b[..])
        .expect("in order");
    let mut unsorted = bytes.clone();
    unsorted[at..at + b.len()].copy_from_slice(&b);
    unsorted[at + b.len()..at + a.len() + b.len()].copy_from_slice(&a);
    assert_eq!(
        load(&unsorted),
        Some(sim_core::world::LoadError::Inconsistent)
    );
    assert!(load(&bytes).is_none());
}
