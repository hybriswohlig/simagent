//! Growing up in a house (ADR-0033 sections 1, 3 and 4): who is raised and from which second,
//! the age weight `w`, and lived experience `D` in the temperament phenotype. The expected
//! numbers are `tools/development_reference.py` (its printed and asserted values) and the
//! ADR's formulas by hand.

mod common;

use common::day::Run;
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::development::{Axis, Lived, Raised, weight};
use sim_core::genome::{Genome, Sex};
use sim_core::ids::SimId;
use sim_core::input::{Input, MoveIn, MoveOut};
use sim_core::lineage::LineageKey;
use sim_core::log::LogPayload;
use sim_core::phenotype::{Environment, PhenotypeParts, phenotype_parts};
use sim_core::rng::{Draws, WorldSeed};
use sim_core::time::{DAY, MONTH, SimDuration, SimTime, YEAR};
use sim_core::world::World;

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn years(n: i64) -> SimDuration {
    SimDuration::from_secs(n * YEAR.as_secs())
}

fn id(text: &str) -> DataId {
    DataId::new(text).expect("id")
}

/// Conception at the first month start, birth one month later (`house_paternity.rs`).
const BIRTH: i64 = 2 * MONTH.as_secs();

/// The repository's calibration and map alone: map 5 with the temperament traits.
fn genetics() -> Data {
    let (calibration, genome_map) = common::repository_files();
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// On the repository's map: fertility certain at the first month start, a pregnancy of one
/// month, no twins, almost no death, no hereditary disease, the repository's houses and its
/// social data without talk furniture.
fn life_data() -> Data {
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
    Data::new(definitions.expect("definitions"))
        .expect("checked")
        .with_houses(common::repository_houses())
        .with_social(common::repository_social_without_talks(|_| {}))
}

/// A woman of 30 and a man of 32, a couple who take in a girl of 12 and a girl of 17.
const FAMILY: [i64; 4] = [
    -30 * YEAR.as_secs(),
    -32 * YEAR.as_secs(),
    -12 * YEAR.as_secs(),
    -17 * YEAR.as_secs(),
];

/// The second of the household's `core:moved_in` with all its members.
fn moved_in_at(run: &Run) -> SimTime {
    (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::MovedIn { .. }))
        .expect("moved in")
        .t
}

/// Acceptance: the age weight of the reference. A move-in at 12 gives 0 at 8 and at 12, ⅓ at
/// 18 and later; raised from birth, ½ at 9 and 1 from 18; never raised, 0.
#[test]
fn the_age_weight_is_the_references() {
    let at_12 = Some(years(12));
    assert_eq!(weight(years(8), None), 0.0);
    assert_eq!(weight(years(8), at_12), 0.0);
    assert_eq!(weight(years(12), at_12), 0.0);
    assert_eq!(weight(years(18), at_12), 1.0 / 3.0);
    assert_eq!(weight(years(30), at_12), 1.0 / 3.0);
    assert_eq!(weight(years(9), Some(SimDuration::ZERO)), 0.5);
    assert_eq!(weight(years(18), Some(SimDuration::ZERO)), 1.0);
    // `w_age(1000 + 18 years + 5, 1000, 1000)`: raised in its first second, 5 s past 18.
    let past = SimDuration::from_secs(18 * YEAR.as_secs() + 5);
    assert_eq!(weight(past, Some(SimDuration::ZERO)), 1.0);
}

/// The axes in the order of ADR-0033 section 5 and their traits in the map.
#[test]
fn the_axes_name_the_temperament_traits_in_order() {
    let names: Vec<DataId> = Axis::ALL.iter().map(|axis| axis.trait_id()).collect();
    let names: Vec<&str> = names.iter().map(DataId::as_str).collect();
    assert_eq!(
        names,
        [
            "core:openness",
            "core:conscientiousness",
            "core:extraversion",
            "core:agreeableness",
            "core:neuroticism"
        ]
    );
    for (n, axis) in Axis::ALL.into_iter().enumerate() {
        assert_eq!(axis.index(), n);
        assert_eq!(Axis::of(&axis.trait_id()), Some(axis));
    }
    assert_eq!(Axis::of(&id("core:cognition")), None);
    // The repository's calibration holds `m_D = 0` and `s_D = 1` for every axis (ADR-0033
    // section 1, until the measurement replaces them).
    assert_eq!(genetics().genetics().development, Some([(0.0, 1.0); 5]));
}

/// The terms of `t` for a woman of the reference genome at `age` with `lived`.
fn parts(data: &Data, t: &str, age: SimDuration, lived: Lived) -> PhenotypeParts {
    let draws = Draws::new(WorldSeed::new(7));
    let environment = Environment {
        lived,
        ..Environment::one(&draws)
    };
    let key = LineageKey::founder(&id("test:raised"), 0);
    let genome = Genome::reference(Sex::Female);
    phenotype_parts(environment, data, &genome, key, None, &id(t), age).expect("a phenotype")
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= 1e-12 * want.abs()
}

/// Acceptance: `G = C = 0`, `D = 1000`, `w = 1` gives `z = √0.45` in an adult, so the
/// individual term alone is `√e² · D̂ = √0.45` (reference: `z_phenotype(0, 0, E, 1000, 1, 1000
/// years)` for any `E`). An axis with `D = 0` and `w = 1` has `U = 0`. A trait that is no axis
/// and a person never raised keep `E` bit for bit.
#[test]
fn lived_experience_takes_the_place_of_e_on_an_axis() {
    let data = genetics();
    let adult = years(1000);
    let raised = Lived {
        raised_at_age: Some(SimDuration::ZERO),
        experience: [1000, 0, 0, 0, 0],
    };
    let openness = parts(&data, "core:openness", adult, raised);
    assert!(
        close(openness.individual, sim_math::sqrt(0.45)),
        "{openness:?}"
    );
    let plain = parts(&data, "core:openness", adult, Lived::NONE);
    assert_eq!(openness.genetic.to_bits(), plain.genetic.to_bits());
    assert_eq!(openness.shared.to_bits(), plain.shared.to_bits());
    assert_ne!(openness.individual.to_bits(), plain.individual.to_bits());
    let conscientiousness = parts(&data, "core:conscientiousness", adult, raised);
    assert_eq!(conscientiousness.individual, 0.0);
    for t in ["core:cognition", "core:growth"] {
        assert_eq!(
            parts(&data, t, adult, raised),
            parts(&data, t, adult, Lived::NONE),
            "{t}"
        );
    }
}

/// At 12, raised from birth: `w = ⅔`, `U = √(1 − w²) · E + w · D̂` with `D̂ = 1`, and the weight of
/// `U` is `√e²(12) = √0.40` (`h² = 0.5`, `c² = 0.10` midway between child and adult).
#[test]
fn a_child_mixes_e_and_lived_experience_by_the_age_weight() {
    let data = genetics();
    let lived = Lived {
        raised_at_age: Some(SimDuration::ZERO),
        experience: [1000, 0, 0, 0, 0],
    };
    let plain = parts(&data, "core:openness", years(12), Lived::NONE);
    let e = plain.individual / sim_math::sqrt(0.40);
    let w: f64 = 2.0 / 3.0;
    let want = sim_math::sqrt(0.40) * (sim_math::sqrt(1.0 - w * w) * e + w * 1.0);
    let got = parts(&data, "core:openness", years(12), lived).individual;
    assert!(close(got, want), "{got} {want}");
}

/// Acceptance "Einzug ohne Sprung": at the age it is raised from, `w = 0` and `D = 0`, so the
/// phenotype is the one without anything lived, bit for bit.
#[test]
fn being_raised_from_now_changes_no_phenotype_now() {
    let data = genetics();
    for axis in Axis::ALL {
        let now = Lived {
            raised_at_age: Some(years(10)),
            experience: [0; 5],
        };
        assert_eq!(
            parts(&data, axis.trait_id().as_str(), years(10), now),
            parts(&data, axis.trait_id().as_str(), years(10), Lived::NONE),
            "{axis:?}"
        );
    }
}

/// The girls of 12 and 17 are raised from the second the household moves in, with nothing
/// lived; the adults are not. The younger girl's openness at that second is the same before and
/// after the move.
#[test]
fn a_child_moving_in_is_raised_from_that_second() {
    let mut run = Run::founded(life_data(), 7, "test:raised", &FAMILY);
    run.advance(DAY.as_secs() - 2);
    let home = run.together(FAMILY.len()).expect("one household");
    let openness = id("core:openness");
    let at = t(DAY.as_secs() - 1);
    let age = |born: i64| at.checked_since(t(born)).expect("born before");
    let before = run
        .world
        .state()
        .phenotype(sim(3), &openness, age(FAMILY[2]));
    run.submit(Input::MoveIn(MoveIn { household: home }));
    assert_eq!(moved_in_at(&run), at);
    let raised = &run.world.state().house().raised;
    assert_eq!(
        raised.iter().collect::<Vec<_>>(),
        vec![(&sim(3), &Raised::new(at)), (&sim(4), &Raised::new(at))]
    );
    let after = run
        .world
        .state()
        .phenotype(sim(3), &openness, age(FAMILY[2]));
    assert_eq!(before.map(f64::to_bits), after.map(f64::to_bits));
    assert!(before.is_some());
}

/// A later move out and in again changes nothing: the first second stays. (What the girls
/// practised in the three days in the house stays too, ADR-0033 section 6.)
#[test]
fn a_later_move_keeps_the_first_second() {
    let (mut run, home) = Run::family(life_data(), 7, "test:raised", &FAMILY, 0);
    let first = moved_in_at(&run);
    run.advance(3 * DAY.as_secs());
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let practised = run.world.state().house().raised[&sim(3)].practice;
    run.advance(5 * DAY.as_secs());
    run.submit(Input::MoveIn(MoveIn { household: home }));
    let raised = &run.world.state().house().raised;
    let want = Raised {
        practice: practised,
        ..Raised::new(first)
    };
    assert_eq!(raised.get(&sim(3)), Some(&want));
    assert_eq!(raised.len(), 2);
}

/// A child born into a household in a house is raised from its birth second; one born before
/// the move-in from the move-in.
#[test]
fn a_child_born_in_the_house_is_raised_from_its_birth() {
    let born = [FAMILY[0], FAMILY[1]];
    let (mut run, _) = Run::family(life_data(), 7, "test:raised", &born, 0);
    run.advance(BIRTH);
    let birth = (run.log.iter())
        .find(|entry| matches!(entry.payload, LogPayload::Birth { .. }))
        .expect("a birth");
    let child = sim(3);
    assert_eq!(
        run.world.state().house().raised.get(&child),
        Some(&Raised::new(birth.t))
    );

    let mut late = Run::founded(life_data(), 7, "test:raised", &born);
    late.advance(BIRTH);
    assert!(late.world.state().house().raised.is_empty());
    let home = late
        .world
        .state()
        .household_of(sim(1))
        .expect("her household");
    late.submit(Input::MoveIn(MoveIn { household: home }));
    let at = moved_in_at(&late);
    assert_eq!(
        late.world.state().house().raised.get(&child),
        Some(&Raised::new(at))
    );
}

/// A save after the move-in holds who is raised and loads back to the same bytes.
#[test]
fn a_save_keeps_who_is_raised() {
    let data = life_data();
    let (run, _) = Run::family(data.clone(), 7, "test:raised", &FAMILY, 0);
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (loaded, _) = World::load(&bytes, data).expect("loads");
    assert_eq!(
        loaded.state().house().raised,
        run.world.state().house().raised
    );
    assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
}

/// The loader's bounds: raised at an age of 0 to under 18, `D` within −3000 to 3000, no need
/// counted on a day before it was raised.
#[test]
fn a_raised_entry_fits_only_within_its_bounds() {
    let born = t(0);
    let at = |secs: i64| Raised::new(t(secs));
    assert!(at(0).fits(born));
    assert!(at(18 * YEAR.as_secs() - 1).fits(born));
    assert!(!at(18 * YEAR.as_secs()).fits(born));
    assert!(!at(-1).fits(born));
    let mut lived = at(0);
    lived.experience = [3000, -3000, 0, 0, 0];
    assert!(lived.fits(born));
    lived.experience[2] = 3001;
    assert!(!lived.fits(born));
    // A need counted on the day it was raised or later fits; on a day before, not.
    let mut counted = at(3 * DAY.as_secs() + 5);
    counted.counted = [None, Some(3), None, None, Some(9)];
    assert!(counted.fits(born));
    counted.counted[1] = Some(2);
    assert!(!counted.fits(born));
}

/// `m_D` and `s_D` differ on every axis, so a mixed-up axis or a calibration left unread
/// shows: with `D = 1000`, `w = 1` and `G = C = 0` an adult has `z = √0.45 · (1 − m_D) / s_D`,
/// the reference's `z_phenotype(0, 0, E, 1000, 1, 1000 years, m_D, s_D)` for any `E`.
#[test]
fn each_axis_reads_its_own_development_values() {
    let calibrated: [(f64, f64, f64); 5] = [
        (0.5, 2.0, 0.167_705_098_312_484_23),
        (-0.25, 0.5, 1.677_050_983_124_842_4),
        (1.0, 1.5, 0.0),
        (0.0, 0.25, 2.683_281_572_999_747_7),
        (-1.0, 4.0, 0.335_410_196_624_968_46),
    ];
    let (mut calibration, genome_map) = common::repository_files();
    for (axis, (m_d, s_d, _)) in Axis::ALL.into_iter().zip(calibrated) {
        let values = &mut calibration["values"];
        values[format!("{}_development_mean", axis.trait_id().as_str())]["value"] = json!(m_d);
        values[format!("{}_development_sd", axis.trait_id().as_str())]["value"] = json!(s_d);
    }
    let data = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
        .expect("checked");
    assert_eq!(
        data.genetics().development,
        Some(calibrated.map(|(m_d, s_d, _)| (m_d, s_d)))
    );
    let lived = Lived {
        raised_at_age: Some(SimDuration::ZERO),
        experience: [1000; 5],
    };
    for (axis, (_, _, want)) in Axis::ALL.into_iter().zip(calibrated) {
        let got = parts(&data, axis.trait_id().as_str(), years(1000), lived).individual;
        let fits = if want == 0.0 {
            got == 0.0
        } else {
            close(got, want)
        };
        assert!(fits, "{axis:?}: {got} {want}");
    }
}

/// The loader asks for the record of every child living in a house: the same save with the
/// girls' records dropped from the end of the projection does not load.
#[test]
fn a_save_without_a_housed_childs_record_does_not_load() {
    use sim_core::world::LoadError;

    let data = life_data();
    let (run, _) = Run::family(data.clone(), 7, "test:raised", &FAMILY, 0);
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let projection = run.world.canonical_bytes();
    let raised = postcard::to_allocvec(&run.world.state().house().raised).expect("bytes");
    assert!(projection.ends_with(&raised));
    let at = (bytes.windows(projection.len()))
        .position(|window| window == projection)
        .expect("the projection stands in the save");
    let end = at + projection.len();
    let mut forged = bytes[..end - raised.len()].to_vec();
    forged.push(0);
    forged.extend(&bytes[end..]);
    assert!(World::load(&bytes, data.clone()).is_ok());
    assert_eq!(
        World::load(&forged, data).err(),
        Some(LoadError::Inconsistent)
    );
}

/// The loader refuses a need counted on a day after that of the second the clock opens next: the
/// same save with the girl of
/// 12's day of hunger set to that day loads, set to the next day it does not.
#[test]
fn a_save_with_a_need_counted_after_the_clock_does_not_load() {
    use sim_core::world::LoadError;

    let data = life_data();
    let (run, _) = Run::family(data.clone(), 7, "test:raised", &FAMILY, 0);
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let projection = run.world.canonical_bytes();
    let raised = run.world.state().house().raised.clone();
    let old = postcard::to_allocvec(&raised).expect("bytes");
    assert!(projection.ends_with(&old));
    let at = (bytes.windows(projection.len()))
        .position(|window| window == projection)
        .expect("the projection stands in the save");
    let end = at + projection.len();
    let today = (run.world.closed_through().as_secs() + 1).div_euclid(DAY.as_secs());
    let forge = |day: i64| {
        let mut changed = raised.clone();
        changed.get_mut(&sim(3)).expect("raised").counted[0] = Some(day);
        let mut forged = bytes[..end - old.len()].to_vec();
        forged.extend(postcard::to_allocvec(&changed).expect("bytes"));
        forged.extend(&bytes[end..]);
        forged
    };
    assert!(World::load(&forge(today), data.clone()).is_ok());
    assert_eq!(
        World::load(&forge(today + 1), data).err(),
        Some(LoadError::Inconsistent)
    );
}
