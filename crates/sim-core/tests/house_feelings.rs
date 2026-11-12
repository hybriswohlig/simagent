//! ADR-0032 in a moved-in household: the four channels and the temper of a person from 6, written
//! by the occasions of the table (a birth, a death, a partnership that began or ended, the
//! outcome of a talk), read analytically at the second they are needed, frozen on moving out,
//! dropped on leaving the world, saved and loaded without fading. The expected values are the
//! ADR's and `tools/social_reference.py --check`'s (`apply_occasion`, `channel_at`, `temper_at`,
//! `effect_*`), by hand; the seconds of the deaths are the world of seed 10 (found by running it
//! once, as `house_memory.rs` does). A need at 0 and the contagion on entering a room are in
//! `house_zero.rs`, next to the test house they need.

mod common;

use common::day::{self, Run};
use serde_json::json;
use sim_core::canonical::PROJECTION_VERSION;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::ids::{HouseholdId, SimId};
use sim_core::input::{
    ArrangePartnership, Command, Input, MoveIn, MoveOut, SeparatePartnership, Start,
};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, DrawRef, LogEntry, LogPayload, Subject};
use sim_core::params::WorldParams;
use sim_core::social::belief::{LAESTERT, Statement};
use sim_core::social::feeling::Feel;
use sim_core::social::talk::{house_day, lie_draw_key, refute_draw_key};
use sim_core::time::{DAY, SimTime, YEAR};
use sim_core::version::CORE_VERSION;
use sim_core::world::{LoadError, World, YearOut};

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn year() -> i64 {
    YEAR.as_secs()
}

fn days(n: i64) -> i64 {
    n * DAY.as_secs()
}

// ---------------------------------------------------------------------------------------------
// Life events: births, deaths, partnerships, moves. The world of `house_memory.rs`: a woman of 30
// (sim 1) and a man of 32 (sim 2), founders of `test:memory`, moved in at 23:59:59 of day 0, her
// pregnancy of one month certain at the first month start.

/// Accident `accident` per year, the woman's fertility window with a certain conception at the
/// first month start and a pregnancy of one month, the repository's houses and the repository's
/// social data without furniture for a talk (no talk is planned: talks have their own tests).
fn life_data(accident: f64) -> Data {
    let (mut calibration, genome_map) = common::death_files(accident, None, json!([]));
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 1.0);
    demography["values"]["core:pregnancy_length"]["value"] = json!(1.0);
    let definitions = common::definitions_with(demography, calibration, genome_map);
    Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(|_| {}))
}

/// The child's birth, 28 days after the certain conception (`house_memory.rs`).
const BIRTH: i64 = 56 * 86_400;

fn life(accident: f64, seed: u64) -> (Run, HouseholdId) {
    let born = [-30 * year(), -32 * year()];
    Run::family(life_data(accident), seed, "test:memory", &born, 0)
}

/// An accident rate of 3 a year, so that deaths come within the run (seed 10).
fn accidents() -> Data {
    life_data(3.0)
}

/// The seconds of the deaths of the child, the woman and the man in the world of seed 10 with an
/// accident rate of 3 a year: the child's comes 17 days after its birth, the woman's and the
/// man's after (`house_memory.rs`).
fn deaths_of_seed_10() -> (i64, i64, i64) {
    static SECONDS: std::sync::OnceLock<(i64, i64, i64)> = std::sync::OnceLock::new();
    *SECONDS.get_or_init(|| {
        let (mut run, _) = life(3.0, 10);
        run.advance(150 * 86_400);
        let second = |who: u64| {
            (run.log.iter())
                .find(|entry| {
                    entry.subjects == [Subject::Sim(sim(who))]
                        && matches!(entry.payload, LogPayload::Death { .. })
                })
                .map(|entry| entry.t.as_secs())
                .expect("a death")
        };
        (second(3), second(1), second(2))
    })
}

/// The stored feelings of `who` now.
fn stored(run: &Run, who: u64) -> Option<Feel> {
    (run.world.state().house().social.feelings.get(&sim(who))).copied()
}

/// The `core:felt` entries about `who` alone, in the log's order.
fn felt(run: &Run, who: u64) -> Vec<&LogEntry> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(who))])
        .filter(|entry| matches!(entry.payload, LogPayload::Felt { .. }))
        .collect()
}

fn felt_payload(channels: [u16; 4], temper: i16) -> LogPayload {
    let [joy, anger, sadness, fear] = channels;
    LogPayload::Felt {
        joy,
        anger,
        sadness,
        fear,
        temper,
    }
}

/// The numbers of the entries an entry was caused by.
fn caused_by(entry: &LogEntry) -> Vec<u64> {
    (entry.causes.as_slice().iter())
        .filter_map(|cause| match cause {
            Cause::Event(id) => Some(day::number(id)),
            Cause::Plan(plan) => Some(day::number(&plan.by)),
            _ => None,
        })
        .collect()
}

fn entry_where(run: &Run, pick: impl Fn(&LogEntry) -> bool) -> &LogEntry {
    run.log.iter().find(|entry| pick(entry)).expect("the entry")
}

/// Acceptance 1 and 2 of ADR-0032: a child is born in a moved-in household and both parents feel
/// joy +800 and a temper of +200, no band, caused by `core:birth`, the mother first. The child of
/// 0 has none. Joy 800 at 300 a day is 0 at 800 · 86 400 / 300 = 230 400 s and 1 a second before;
/// the temper 200 at 100 a day is 1 a second before two days and 0 at two days.
#[test]
fn a_birth_is_a_joy_that_fades_at_the_pinned_second() {
    let (mut run, _) = life(1e-9, 7);
    run.advance(BIRTH);
    let birth = entry_where(&run, |e| matches!(e.payload, LogPayload::Birth { .. }));
    assert_eq!(birth.t, t(BIRTH));
    let rules = common::repository_social().feelings;
    for who in [1, 2] {
        let entries = felt(&run, who);
        assert_eq!(entries.len(), 1, "sim {who}");
        assert_eq!(entries[0].t, t(BIRTH));
        assert_eq!(entries[0].payload, felt_payload([800, 0, 0, 0], 200));
        assert_eq!(caused_by(entries[0]), vec![day::number(&birth.id)]);
        let feel = stored(&run, who).expect("feelings");
        assert_eq!(
            feel,
            Feel {
                channels: [800, 0, 0, 0],
                since: t(BIRTH),
                temper: 200,
                temper_since: t(BIRTH)
            }
        );
        // Joy 800 at 300 a day: 0 at 230 400 s, 1 a second before. Nothing ticks: it is read.
        assert_eq!(800 * 86_400 / 300, 230_400);
        assert_eq!(feel.at(t(BIRTH + 230_400), &rules).0, [0; 4]);
        assert_eq!(feel.at(t(BIRTH + 230_399), &rules).0, [1, 0, 0, 0]);
        // The temper 200 at 100 a day: two days.
        assert_eq!(feel.at(t(BIRTH + days(2) - 1), &rules).1, 1);
        assert_eq!(feel.at(t(BIRTH + days(2)), &rules).1, 0);
        assert_eq!(feel.at(t(BIRTH + days(2) + 5000), &rules).1, 0);
    }
    // The mother's entry comes before the father's, and the child has none.
    let (mother, father) = (felt(&run, 1)[0], felt(&run, 2)[0]);
    assert!(day::number(&mother.id) < day::number(&father.id));
    assert!(felt(&run, 3).is_empty() && stored(&run, 3).is_none());
    // Nothing is written by the passing of time: a run on to the fade writes no entry.
    run.advance(BIRTH + 230_400 + 10);
    assert_eq!(felt(&run, 1).len(), 1);
}

/// Acceptance 6 and the death of a relative: the child dies 17 days after its birth, so each parent
/// (a relative, φ 1/4, with an affection of 700 to it, in the band) has sadness 1 200 clamped to
/// 1 000, fear 150 and a temper of (-1 000 - 150) / 4 = -287; the joy of the birth is long gone.
/// Later the woman dies: her partner (affection 700, in the band) has sadness 1 000 (clamped from
/// 1 500) and fear +300 in one entry, a temper of -325, caused by her `core:death`; she loses
/// everything she had, and when he dies so does he. No entry is written for the loss.
#[test]
fn the_death_of_a_child_and_of_a_partner() {
    let (child_dies, mother_dies, father_dies) = deaths_of_seed_10();
    assert!(BIRTH + days(3) < child_dies && child_dies + days(7) < mother_dies);
    assert!(mother_dies < father_dies);
    let (mut run, _) = life(3.0, 10);
    run.advance(child_dies - 1);
    let before = felt(&run, 1).len();
    assert_eq!(before, 1, "the birth");
    run.advance(child_dies);
    let death = entry_where(&run, |e| {
        e.subjects == [Subject::Sim(sim(3))] && matches!(e.payload, LogPayload::Death { .. })
    });
    for who in [1, 2] {
        let entries = felt(&run, who);
        assert_eq!(entries.len(), 2, "sim {who}");
        assert_eq!(entries[1].t, t(child_dies));
        assert_eq!(entries[1].payload, felt_payload([0, 0, 1000, 150], -287));
        assert_eq!(caused_by(entries[1]), vec![day::number(&death.id)]);
        assert_eq!(
            stored(&run, who),
            Some(Feel {
                channels: [0, 0, 1000, 150],
                since: t(child_dies),
                temper: -287,
                temper_since: t(child_dies)
            })
        );
    }
    assert!(felt(&run, 3).is_empty(), "the dead child had none");
    // The mother dies: one entry for her partner, none for her, her feelings gone.
    run.advance(mother_dies);
    let death = entry_where(&run, |e| {
        e.subjects == [Subject::Sim(sim(1))] && matches!(e.payload, LogPayload::Death { .. })
    });
    let at_death: Vec<&LogEntry> = (run.log.iter())
        .filter(|entry| entry.t == t(mother_dies))
        .filter(|entry| matches!(entry.payload, LogPayload::Felt { .. }))
        .collect();
    assert_eq!(
        at_death.len(),
        1,
        "one row for the partner, none for the dead"
    );
    assert_eq!(at_death[0].subjects, [Subject::Sim(sim(2))]);
    // Sadness and fear and the temper of the child's death have faded away by then.
    assert_eq!(at_death[0].payload, felt_payload([0, 0, 1000, 300], -325));
    assert_eq!(caused_by(at_death[0]), vec![day::number(&death.id)]);
    assert_eq!(felt(&run, 1).len(), 2, "nothing for the loss");
    assert_eq!(stored(&run, 1), None);
    assert!(stored(&run, 2).is_some());
    run.advance(father_dies);
    assert_eq!(stored(&run, 2), None);
    assert_eq!(felt(&run, 2).len(), 3, "nothing for the loss");
}

/// The partnership that began (both, joy +600, no band, the woman first) and the separation of one
/// (both, anger +300 and sadness +600): two days after the birth the joy of the birth (800 at 300
/// a day) is 200 and its temper (200 at 100 a day) is 0, so each has [200, 300, 600, 0] and a
/// temper of (200 - 300 - 600) / 4 clamped as the changes are: (0 - 300 - 600) / 4 = -225. The
/// separation entry is the cause. A person who then moves out has the values written down.
#[test]
fn a_partnership_began_and_a_separation_are_felt() {
    let (mut run, _) = life(1e-9, 7);
    let apart = BIRTH + days(2);
    run.advance(apart - 1);
    run.submit(Input::SeparatePartnership(SeparatePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let separated = entry_where(&run, |e| matches!(e.payload, LogPayload::Separated { .. }));
    assert_eq!(separated.t, t(apart));
    for who in [1, 2] {
        let entries = felt(&run, who);
        assert_eq!(entries.len(), 2, "the birth and the separation");
        assert_eq!(entries[1].payload, felt_payload([200, 300, 600, 0], -225));
        assert_eq!(caused_by(entries[1]), vec![day::number(&separated.id)]);
    }
    let (her, his) = (felt(&run, 1)[1], felt(&run, 2)[1]);
    assert!(
        day::number(&her.id) < day::number(&his.id),
        "the woman first"
    );
    // He moved out with the separation: written down at that second, `since` then, no entry.
    assert_eq!(
        stored(&run, 2),
        Some(Feel {
            channels: [200, 300, 600, 0],
            since: t(apart),
            temper: -225,
            temper_since: t(apart)
        })
    );

    // The two single founders pair by an input, each in a house of their own: joy +600, a temper
    // of 150, the woman first, caused by `core:partnered`.
    let mut run = single_founders();
    run.advance(days(1) - 2);
    let homes: Vec<HouseholdId> = (run.log.iter())
        .filter_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .collect();
    assert_eq!(homes.len(), 2);
    for home in homes {
        run.submit(Input::MoveIn(MoveIn { household: home }));
    }
    let pairing = days(3);
    run.advance(pairing - 1);
    run.submit(Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(2),
    }));
    let partnered = entry_where(&run, |e| matches!(e.payload, LogPayload::Partnered { .. }));
    assert_eq!(partnered.t, t(pairing));
    for who in [1, 2] {
        let entries = felt(&run, who);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].payload, felt_payload([600, 0, 0, 0], 150));
        assert_eq!(caused_by(entries[0]), vec![day::number(&partnered.id)]);
    }
    assert!(day::number(&felt(&run, 1)[0].id) < day::number(&felt(&run, 2)[0].id));
}

/// A woman of 30 and a man of 32 who are not a couple: each founds a household of their own.
fn single_founders() -> Run {
    use sim_core::genome::Sex;
    use sim_core::input::Founder;
    let founder = |years: i64, sex: Sex| Founder {
        born: t(-years * year()),
        sex,
        alleles: common::dummy(0, 0),
    };
    let start = Start {
        world_seed: 7,
        founder_tag: DataId::new("test:memory").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![founder(30, Sex::Female), founder(32, Sex::Male)],
        couples: Vec::new(),
        capacity: None,
        pool: None,
    };
    let mut world = World::new(life_data(1e-9));
    world.submit(Input::Start(start)).expect("accepted");
    Run {
        world,
        out: YearOut::default(),
        log: Vec::new(),
    }
}

/// ADR-0032 section 1, "Auszug", and acceptance 8: a household that moves out a day after the
/// birth has the values written down at that second (joy 800 - 300 = 500, the temper 200 - 100 =
/// 100), `since` that second, no entry. Outside they do not fade, not even on a load; moving in again sets `since` to
/// that second without fading the gap: a day later joy is 200.
#[test]
fn moving_out_writes_the_feelings_down_and_moving_in_resumes_them() {
    let out = BIRTH + days(1);
    let (mut run, home) = life(1e-9, 7);
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let frozen = Feel {
        channels: [500, 0, 0, 0],
        since: t(out),
        temper: 100,
        temper_since: t(out),
    };
    for who in [1, 2] {
        assert_eq!(stored(&run, who), Some(frozen));
        assert_eq!(
            felt(&run, who).len(),
            1,
            "nothing is written for the freezing"
        );
    }
    // Outside, nothing fades: stored as they were after five days, and after a save and a load.
    run.advance(out + days(5));
    for who in [1, 2] {
        assert_eq!(stored(&run, who), Some(frozen));
    }
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (loaded, _) = World::load(&bytes, life_data(1e-9)).expect("loads");
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    assert_eq!(
        loaded.state().house().social.feelings.get(&sim(1)),
        Some(&frozen)
    );
    // Moving in again: `since` is that second, the values stand, the gap is not faded.
    let back = out + days(8);
    run.advance(back - 1);
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let rules = common::repository_social().feelings;
    for who in [1, 2] {
        let feel = stored(&run, who).expect("feelings");
        assert_eq!(
            feel,
            Feel {
                since: t(back),
                temper_since: t(back),
                ..frozen
            }
        );
        assert_eq!(feel.at(t(back), &rules), ([500, 0, 0, 0], 100));
        assert_eq!(feel.at(t(back + days(1)), &rules), ([200, 0, 0, 0], 0));
        assert_eq!(felt(&run, who).len(), 1, "nothing is written on the way in");
    }
}

/// A person who dies outside any moved-in household loses its frozen feelings; nothing is written,
/// and the other who still lives outside keeps them.
#[test]
fn a_death_outside_drops_the_frozen_feelings() {
    let (child_dies, mother_dies, _) = deaths_of_seed_10();
    let out = BIRTH + days(2);
    assert!(out < child_dies && child_dies < mother_dies);
    let (mut run, home) = life(3.0, 10);
    run.advance(out - 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    for who in [1, 2] {
        // Two days after the birth: joy 200, the temper 0.
        assert_eq!(
            stored(&run, who),
            Some(Feel {
                channels: [200, 0, 0, 0],
                since: t(out),
                temper: 0,
                temper_since: t(out)
            })
        );
    }
    run.advance(mother_dies);
    assert_eq!(stored(&run, 1), None);
    assert!(stored(&run, 2).is_some(), "he lives on, outside");
    let entries = felt(&run, 1).len() + felt(&run, 2).len();
    assert_eq!(
        entries, 2,
        "the births only: nothing outside, nothing for the loss"
    );
}

/// Acceptance 8: a save in the middle of the week loads to the same state and runs on to the same
/// log; the feelings do not fade on the load. The loader refuses a channel over 1 000, a temper
/// out of its range, a second after the clock, an entry with every value 0, and an owner under 6,
/// and a save of the previous projection (ADR-0032 section 6).
#[test]
fn a_save_in_the_middle_of_the_week_runs_on_and_the_loader_checks_it() {
    let (child_dies, _, _) = deaths_of_seed_10();
    let (mut run, _) = life(3.0, 10);
    let mid = child_dies + days(3) + 5;
    run.advance(mid);
    let data = accidents();
    let feel = stored(&run, 1).expect("feelings of the mother");
    // Sadness 1000 at 150 a day was written at the child's death; it is read, not rewritten.
    assert_eq!(feel.channels, [0, 0, 1000, 150]);
    assert_eq!(feel.since, t(child_dies));
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let (mut loaded, hashes) = World::load(&bytes, data.clone()).expect("loaded");
    assert_eq!(hashes, run.out.year_hashes);
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
    assert_eq!(
        loaded.state().house().social.feelings,
        run.world.state().house().social.feelings
    );
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, t(150 * 86_400), &mut |_| false);
    run.advance(150 * 86_400);
    let tail: Vec<&LogEntry> = run.log.iter().filter(|e| e.t > t(mid)).collect();
    assert_eq!(
        serde_json::to_string(&loaded.take_log()).expect("json"),
        serde_json::to_string(&tail).expect("json")
    );
    assert_eq!(loaded.state_hash(), run.world.state_hash());

    // Tampered saves, a day after the birth: the child of a day lives and has no feelings, each
    // parent has those of the birth as written down (joy 800, temper 200).
    let tamper_at = BIRTH + days(1);
    let (mut run, _) = life(3.0, 10);
    run.advance(tamper_at);
    let feel = stored(&run, 1).expect("feelings of the mother");
    assert_eq!(feel.channels, [800, 0, 0, 0]);
    let bytes = run.world.save(&run.out.year_hashes).expect("saved");
    let load = |bytes: &[u8]| World::load(bytes, data.clone()).err();
    let encode = |value: &Feel| postcard::to_allocvec(value).expect("bytes");
    // The mother's and the father's entries are equal; one pair of key and value is unique.
    let key = |who: u64| postcard::to_allocvec(&sim(who)).expect("key");
    let pair = |who: u64, value: &Feel| [key(who), encode(value)].concat();
    let forged = |who: u64, value: &Feel| {
        let from = pair(who, &feel);
        let hits = (bytes.windows(from.len())).filter(|w| *w == from).count();
        assert_eq!(hits, 1, "one occurrence");
        let at = bytes
            .windows(from.len())
            .position(|w| w == from)
            .expect("found");
        let to = pair(who, value);
        let mut forged = bytes[..at].to_vec();
        forged.extend(&to);
        forged.extend(&bytes[at + from.len()..]);
        forged
    };
    let inconsistent = Some(LoadError::Inconsistent);
    let mut strong = feel;
    strong.channels[1] = 1001;
    assert_eq!(load(&forged(1, &strong)), inconsistent);
    let mut hot = feel;
    hot.temper = 1001;
    assert_eq!(load(&forged(1, &hot)), inconsistent);
    let mut cold = feel;
    cold.temper = -1001;
    assert_eq!(load(&forged(1, &cold)), inconsistent);
    let mut late = feel;
    late.since = t(tamper_at + 2);
    assert_eq!(load(&forged(1, &late)), inconsistent);
    let mut late_temper = feel;
    late_temper.temper_since = t(tamper_at + 2);
    assert_eq!(load(&forged(1, &late_temper)), inconsistent);
    let mut blank = feel;
    blank.channels = [0; 4];
    blank.temper = 0;
    assert_eq!(load(&forged(1, &blank)), inconsistent);
    // The values at their limits still load: 1000 and -1000 are reachable.
    let mut at_limits = feel;
    at_limits.channels = [1000; 4];
    at_limits.temper = -1000;
    assert!(load(&forged(1, &at_limits)).is_none());
    // An owner under 6: the child (sim 3) in the father's place (the map stays sorted, or the
    // save would not be the canonical encoding of what it holds).
    let from = pair(2, &feel);
    let to = pair(3, &feel);
    let at = bytes
        .windows(from.len())
        .position(|w| w == from)
        .expect("found");
    let mut child = bytes[..at].to_vec();
    child.extend(&to);
    child.extend(&bytes[at + from.len()..]);
    assert_eq!(load(&child), inconsistent);
    // A save of the previous projection, and of the previous core, is refused.
    assert_eq!(PROJECTION_VERSION, 24);
    assert_eq!(CORE_VERSION, 69);
    let rest = bytes.strip_prefix(b"SIMAGENT").expect("magic");
    let (_, rest) = postcard::take_from_bytes::<u32>(rest).expect("format");
    let (_, rest) =
        postcard::take_from_bytes::<u64>(postcard::take_from_bytes::<u32>(rest).expect("core").1)
            .expect("hash");
    let proj_at = bytes.len() - rest.len();
    assert_eq!(bytes[proj_at], 24);
    let mut old = bytes.clone();
    old[proj_at] = 23;
    assert_eq!(load(&old), Some(LoadError::ForeignProjectionVersion));
    assert!(load(&bytes).is_none());
}

// ---------------------------------------------------------------------------------------------
// Talks: the rows a talk brings are one `core:felt` per person after `core:talked`.

const WITHIN_ROOM: i64 = 5;
const TALK_DELAY: i64 = 600;
const DINING: u16 = 0;
const FRIDGE: u16 = 1;
const TOILET: u16 = 2;
const SHOWER: u16 = 3;

/// A meal of `meal` seconds at a two-place dining table and the other essential kinds, as in
/// `house_talk.rs`; everything stands in the hall. The repository's social data.
fn talk_data(
    meal: u32,
    people: u8,
    dwell: u32,
    day_file: &str,
    social: sim_core::social::data::SocialData,
) -> Data {
    let kinds = format!(
        r#"[
      {{"id": "core:dining_table", "slots": 3, "offers": [
        {{"id": "eat", "rates": {{"hunger": 1}}, "seconds": {meal}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:fridge", "slots": 4, "offers": [
        {{"id": "snack", "rates": {{"hunger": 1}}, "seconds": 60, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:toilet", "slots": 4, "offers": [
        {{"id": "use", "rates": {{"bladder": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:shower", "slots": 4, "offers": [
        {{"id": "shower", "rates": {{"hygiene": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:sofa", "slots": 1, "offers": [
        {{"id": "sit", "rates": {{"fun": 1}}, "seconds": {dwell}, "min_age": 0, "max_age": 130}}]}},
      {{"id": "core:bed", "slots": {people}, "offers": [
        {{"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}}]}}
    ]"#
    );
    let house = format!(
        r#"{{
      "id": "test:talk", "capacity": {people},
      "rooms": [{{"id": "hall", "entrance": true}}, {{"id": "den"}}],
      "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
      "doors": [{{"a": "hall", "b": "den", "seconds": 1}}],
      "objects": [
        {{"kind": "core:dining_table", "room": "hall"}},
        {{"kind": "core:fridge", "room": "hall"}},
        {{"kind": "core:toilet", "room": "hall"}},
        {{"kind": "core:shower", "room": "hall"}},
        {{"kind": "core:sofa", "room": "hall"}},
        {{"kind": "core:bed", "room": "hall"}}]
    }}"#
    );
    let houses = common::houses_from(&day::rates(1, 0, 0), day_file, &kinds, &[&house]);
    let (calibration, genome_map) = common::death_files(1e-9, None, json!([]));
    let definitions = common::definitions_with(common::demography(), calibration, genome_map)
        .expect("definitions");
    Data::new(definitions)
        .expect("checked")
        .with_houses(houses)
        .with_social(social)
}

fn command(id: u64, object: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer: 0,
    })
}

/// Commands `seats` one second after `morning` and runs to the arrival. Returns that second.
fn arrive(run: &mut Run, morning: i64, seats: &[(u64, u16)]) -> i64 {
    run.advance(morning);
    for &(id, object) in seats {
        run.submit(command(id, object));
    }
    let arrival = morning + 1 + WITHIN_ROOM;
    run.advance(arrival);
    arrival
}

fn talked(run: &Run) -> Vec<&LogEntry> {
    (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::Talked { .. }))
        .collect()
}

/// Founders 0 and 1 of `test:social` tease (founder 0 is the actor, as in `house_talk.rs`): the
/// teased one, sim 2, has anger +100 at the band of its affection of 700 to the teaser, so +150, and
/// a temper of -150 / 4 = -37; the teaser has nothing. One `core:felt`, after `core:talked`,
/// caused by it, and not charged against the talk's limits (the counters stay as without it).
#[test]
fn a_tease_in_the_band_angers_the_teased_one() {
    let born = [-30 * year(), -32 * year()];
    let day = include_str!("../../../data/house/day.json");
    let data = talk_data(7_200, 2, 60, day, common::repository_social());
    let (mut run, _) = Run::family(data, 0x5EED, "test:social", &born, 0);
    let arrival = arrive(&mut run, day::MORNING, &[(1, DINING), (2, DINING)]);
    let first = arrival + TALK_DELAY;
    run.advance(first);
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 1);
    assert_eq!(
        spoken[0].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 2,
            actor: Some(sim(1)),
        }
    );
    assert!(felt(&run, 1).is_empty() && stored(&run, 1).is_none());
    let entries = felt(&run, 2);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].t, t(first));
    assert_eq!(entries[0].payload, felt_payload([0, 150, 0, 0], -37));
    assert_eq!(caused_by(entries[0]), vec![day::number(&spoken[0].id)]);
    assert!(day::number(&entries[0].id) > day::number(&spoken[0].id));
    assert_eq!(
        stored(&run, 2),
        Some(Feel {
            channels: [0, 150, 0, 0],
            since: t(first),
            temper: -37,
            temper_since: t(first)
        })
    );
    // Not counted against the limits of the talk (`house_talk.rs`: (1, 3) and (1, 4)).
    let person = |id: u64| {
        let person = &run.world.state().house().sims[&sim(id)];
        (person.talks, person.entries)
    };
    assert_eq!((person(1), person(2)), ((1, 3), (1, 4)));
}

/// Founders 0 and 1 and the girl of 10 (W) of `house_talk.rs`' lie: the man is 60, so Necken is closed
/// and Klatsch is the topic. The woman lies to him about W: he hears `lästert` against himself,
/// at his affection of 400 to W (below the band): anger +200, sadness +100, a temper of
/// (0 - 200 - 100) / 4 = -75, one `core:felt` caused by `core:talked`; Klatsch has no row of its
/// own, so the speaker has none.
fn lie_data() -> Data {
    let repository = include_str!("../../../data/house/day.json");
    let day = repository
        .replace(
            r#""school": {"from_age": 6, "until_age": 18"#,
            r#""school": {"from_age": 6, "until_age": 7"#,
        )
        .replace(
            "\"from_age\": 18,\n    \"until_age\": 67,",
            "\"from_age\": 7,\n    \"until_age\": 8,",
        );
    assert_ne!(day, repository, "the ages in the day file still bind");
    // Partner trust 600, the figure the ADR uses for a parent heard by a child; housemate grudge
    // starts at 800 so it is still at least 700 after the fade (`house_talk.rs`).
    let social = common::repository_social_with(|social| {
        social["bonds"]["start"]["partner"][1] = json!(600);
        social["bonds"]["start"]["housemate"][1] = json!(600);
        social["bonds"]["start"]["housemate"][5] = json!(800);
    });
    talk_data(20_000, 3, 43_200, &day, social)
}

fn lie_early() -> Run {
    let born = [-30 * year(), -60 * year(), -10 * year()];
    let mut run = Run::founded(lie_data(), 0x5EED, "test:social", &born);
    run.advance(999);
    let home = run.together(3).expect("one household");
    run.submit(Input::MoveIn(MoveIn { household: home }));
    assert_eq!(run.world.state().house().households[&home].since, t(1_000));
    run
}

fn founder(index: u64) -> LineageKey {
    LineageKey::founder(&DataId::new("test:social").expect("tag"), index)
}

/// The couple at the table in time for a talk at `at`.
fn stage(run: &mut Run, at: i64) {
    let arrival = arrive(
        run,
        at - TALK_DELAY - WITHIN_ROOM - 1,
        &[(1, DINING), (2, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, at);
    run.advance(at);
}

fn laestert(doer: u64, other: u64, day: i64) -> Statement {
    Statement::Tat {
        doer: sim(doer),
        kind: LAESTERT,
        other: sim(other),
        day,
    }
}

fn caused_by_draw(entry: &LogEntry, draw: DrawRef) -> bool {
    matches!(entry.causes.as_slice(), [Cause::Random(got)] if *got == draw)
}

/// The reference's first hit for founder 0 toward founder 1 is second 23 400 (`house_talk.rs`).
#[test]
fn a_lie_against_the_hearer_angers_and_saddens_it_and_a_refutation_is_one_row_with_the_topic() {
    let mut run = lie_early();
    arrive(
        &mut run,
        6 * 3_600,
        &[(1, TOILET), (2, TOILET), (3, TOILET)],
    );
    let at = 23_400;
    stage(&mut run, at);
    let home = run.together(3).expect("household");
    let since = run.world.state().house().households[&home].since;
    let said = laestert(3, 2, house_day(t(at), since));
    let lie = (run.log.iter())
        .find(|e| matches!(e.payload, LogPayload::Told { lie: true, statement, .. } if statement == said))
        .expect("the lie");
    assert!(caused_by_draw(
        lie,
        DrawRef::from(lie_draw_key(founder(0), t(at), founder(1)))
    ));
    let spoken = talked(&run);
    assert_eq!(spoken.len(), 1);
    assert_eq!(
        spoken[0].payload,
        LogPayload::Talked {
            other: sim(2),
            object: DINING,
            topic: 3,
            actor: None,
        }
    );
    // The hearer: anger 200 and sadness 100, a temper of -75; the speaker and W have nothing.
    let entries = felt(&run, 2);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].t, t(at));
    assert_eq!(entries[0].payload, felt_payload([0, 200, 100, 0], -75));
    assert_eq!(caused_by(entries[0]), vec![day::number(&spoken[0].id)]);
    assert!(felt(&run, 1).is_empty() && felt(&run, 3).is_empty());

    // An hour later he and W talk: Entschuldigung (W apologises, he is the one grudged) with the
    // refutation of the lie (the draw of founder 2 toward founder 1 hits). His rows: joy +50 and
    // anger -200 at his affection of 400 to W (below the band), and anger +150 refuted toward the
    // source, the liar, at his affection of 700 to her: in the band, +225. Summed first: anger
    // +25, joy +50; his anger 200 fell to 188 and sadness 100 to 94 by then, the temper -75 to
    // -71, so he has [50, 213, 94, 0]; the temper moves by (50 - 25) / 4 = 6, once, to -65.
    for who in [1, 2] {
        run.submit(command(who, TOILET));
    }
    run.advance(at + 1 + WITHIN_ROOM);
    let later = 27_000;
    let arrival = arrive(
        &mut run,
        later - TALK_DELAY - WITHIN_ROOM - 1,
        &[(2, DINING), (3, DINING)],
    );
    assert_eq!(arrival + TALK_DELAY, later);
    run.advance(later);
    let refuted = (run.log.iter())
        .find(|e| matches!(e.payload, LogPayload::Refuted { .. }))
        .expect("the refutation");
    assert_eq!(refuted.t, t(later));
    assert!(caused_by_draw(
        refuted,
        DrawRef::from(refute_draw_key(founder(2), t(later), founder(1)))
    ));
    let talk = *talked(&run).last().expect("the talk");
    assert_eq!(
        talk.payload,
        LogPayload::Talked {
            other: sim(3),
            object: DINING,
            topic: 5,
            actor: Some(sim(3)),
        }
    );
    let at_talk: Vec<&LogEntry> = (run.log.iter())
        .filter(|e| e.t == t(later) && matches!(e.payload, LogPayload::Felt { .. }))
        .collect();
    assert_eq!(at_talk.len(), 1, "one entry for all the rows of the talk");
    assert_eq!(at_talk[0].subjects, [Subject::Sim(sim(2))]);
    assert_eq!(at_talk[0].payload, felt_payload([50, 213, 94, 0], -65));
    assert_eq!(caused_by(at_talk[0]), vec![day::number(&talk.id)]);
    assert_eq!(felt(&run, 2).len(), 2);
    assert!(felt(&run, 1).is_empty() && felt(&run, 3).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Contagion: on entering a room with an anger of 700 or more.

fn four_data() -> Data {
    let repository = include_str!("../../../data/house/day.json");
    let day = repository
        .replace(
            r#""school": {"from_age": 6, "until_age": 18"#,
            r#""school": {"from_age": 6, "until_age": 7"#,
        )
        .replace(
            "\"from_age\": 18,\n    \"until_age\": 67,",
            "\"from_age\": 7,\n    \"until_age\": 8,",
        );
    assert_ne!(day, repository, "the ages in the day file still bind");
    talk_data(20_000, 4, 43_200, &day, common::repository_social())
}

fn at(run: &Run, who: u64) -> (sim_core::house::state::Place, Option<u16>) {
    let person = &run.world.state().house().sims[&sim(who)];
    (person.place, person.action.map(|action| action.object))
}

/// The second of the `core:reached` of `who` at `object` after `from`, and its entry.
fn reached(run: &Run, who: u64, object: u16, from: i64) -> &LogEntry {
    (run.log.iter())
        .find(|e| {
            e.t >= t(from)
                && e.subjects == [Subject::Sim(sim(who))]
                && matches!(e.payload, LogPayload::Reached { object: got } if got == object)
        })
        .expect("the arrival")
}

/// The `core:felt` entries of `who` from `from` on, as (second, payload, the entries that caused it).
fn pushed(run: &Run, who: u64, from: i64) -> Vec<(i64, LogPayload, Vec<u64>)> {
    (run.log.iter())
        .filter(|e| e.t >= t(from) && e.subjects == [Subject::Sim(sim(who))])
        .filter(|e| matches!(e.payload, LogPayload::Felt { .. }))
        .map(|e| (e.t.as_secs(), e.payload.clone(), caused_by(e)))
        .collect()
}

/// Acceptance 4 of ADR-0032: a woman (sim 1) arrives in the hall with an anger of 720; there sit
/// the man (sim 2, anger 0, A), the girl (sim 3, 650, B) and the boy (sim 4, 800, C), all at the
/// toilet in the hall, and nobody else in the house. A has 150, B has 699, C stays 800 and has no
/// entry and no new `since`; she herself is not touched. Each entry is caused by her `core:reached`
/// and its temper follows by the rule of an occasion (150: -37, 49: -12). Then B comes to the
/// fridge with 699: below 700 she pushes nobody. Then C comes to the fridge with 800: A, with 150,
/// gets 150 more (300, temper -37 more), B at 699 stays (the cap, no change, no entry), the woman at
/// 720 stays.
#[test]
fn an_angry_arrival_gives_everyone_in_the_room_a_capped_push_and_no_one_passes_it_on() {
    let born = [-30 * year(), -32 * year(), -10 * year(), -12 * year()];
    let (mut run, _) = Run::family(four_data(), 0x5EED, "test:social", &born, 0);
    run.advance(day::MORNING + 1);
    for who in [2, 3, 4] {
        run.submit(command(who, TOILET));
    }
    let origin = day::MORNING + 200;
    run.advance(origin);
    let hall = sim_core::house::state::Place::Room(0);
    for who in [2, 3, 4] {
        assert_eq!(
            at(&run, who),
            (hall, Some(TOILET)),
            "sim {who} sits in the hall"
        );
    }
    // The feelings from a save, as a person would have them after a long day: anger, nothing else.
    let anger = |value: u16| Feel {
        channels: [0, value, 0, 0],
        since: t(origin),
        temper: 0,
        temper_since: t(origin),
    };
    let feelings = [(1, anger(720)), (3, anger(650)), (4, anger(800))];
    run.world = common::with_feelings(&run.world, &run.out.year_hashes, &four_data(), &feelings);
    // She comes to the shower in the hall.
    run.submit(command(1, SHOWER));
    run.advance(origin + 60);
    let arrival = reached(&run, 1, SHOWER, origin);
    let second = arrival.t.as_secs();
    let id = day::number(&arrival.id);
    assert_eq!(
        pushed(&run, 2, origin),
        [(second, felt_payload([0, 150, 0, 0], -37), vec![id])]
    );
    assert_eq!(
        pushed(&run, 3, origin),
        [(second, felt_payload([0, 699, 0, 0], -12), vec![id])]
    );
    assert!(pushed(&run, 4, origin).is_empty(), "800 stays: no entry");
    assert!(
        pushed(&run, 1, origin).is_empty(),
        "the arriver is not touched"
    );
    assert_eq!(
        stored(&run, 4),
        Some(anger(800)),
        "no change, no write, `since` stays"
    );
    assert_eq!(stored(&run, 1), Some(anger(720)));
    assert_eq!(
        stored(&run, 3),
        Some(Feel {
            channels: [0, 699, 0, 0],
            since: t(second),
            temper: -12,
            temper_since: t(second)
        })
    );
    // B arrives with 699: below 700, no push at all.
    run.submit(command(3, FRIDGE));
    run.advance(second + 60);
    let b = reached(&run, 3, FRIDGE, second);
    let later = b.t.as_secs();
    assert!(later > second);
    for who in [1, 2, 3, 4] {
        assert!(
            pushed(&run, who, second + 1).is_empty(),
            "sim {who} after B's arrival"
        );
    }
    // C arrives with 800: A gets 150 more, B and the woman stay.
    run.submit(command(4, FRIDGE));
    run.advance(later + 60);
    let c = reached(&run, 4, FRIDGE, later);
    let last = c.t.as_secs();
    assert_eq!(
        pushed(&run, 2, second + 1),
        [(
            last,
            felt_payload([0, 300, 0, 0], -74),
            vec![day::number(&c.id)]
        )]
    );
    for who in [1, 3, 4] {
        assert!(
            pushed(&run, who, second + 1).is_empty(),
            "sim {who} after C's arrival"
        );
    }
}
