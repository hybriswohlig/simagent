//! The percentile of the genetics card (ADR-0021 section 5, Grok G2 and L1): `p = 100 Φ(z)`;
//! from the middle up "größer als `min(⌊p⌋, 99)`", below "kleiner als `min(⌊100 − p⌋, 99)`".
//! The expected values from tables of Φ: Φ(0.5) = 0.69146, Φ(1) = 0.84134, Φ(0.1) = 0.53983,
//! Φ(2.32) = 0.98983, Φ(2.33) = 0.99010, Φ(6) = 1 − 1e-9.

use sim_core::presentation::card::{Percentile, percentile};
use sim_math::probit;

#[test]
fn the_percentile_by_hand() {
    let up = |n| Percentile { up: true, n };
    let down = |n| Percentile { up: false, n };
    let cases = [
        (0.0, up(50)),
        (-0.0, up(50)),
        (0.5, up(69)),
        (1.0, up(84)),
        (-1.0, down(84)),
        (-0.1, down(53)),
        (2.32, up(98)),
        (2.33, up(99)),
        (-2.33, down(99)),
        (6.0, up(99)),
        (-6.0, down(99)),
    ];
    for (z, expected) in cases {
        assert_eq!(percentile(z), expected, "{z}");
    }
}

/// A `z` that is a threshold itself counts: the core's rule is "at or above the threshold"
/// (`probit(j / 100) ≤ z` up, `≥ z` down), which may differ from `⌊100 Φ(z)⌋` within the
/// tolerance of `probit`; the reference keeps every test value away from such a `z`.
#[test]
fn a_threshold_itself_counts() {
    assert_eq!(percentile(probit(0.6)), Percentile { up: true, n: 60 });
    assert_eq!(percentile(probit(0.4)), Percentile { up: false, n: 60 });
    assert_eq!(percentile(probit(0.99)), Percentile { up: true, n: 99 });
}

/// The status of a disease (Grok G4, L7, M1, M4) by hand: what the genotype found, the switch
/// at the second shown and the age then. A person born at the start of year −10 with the
/// onset 45.9 reaches it in the second `−10 · YEAR + ⌈45.9 · YEAR⌉`, in year 35, at 45 whole
/// years; with the onset 0 at its birth, in year −10, at 0 (the web's "seit der Geburt").
#[test]
fn the_status_of_a_disease_by_hand() {
    use sim_core::presentation::card::{DiseaseStatus, Found, disease_status};
    use sim_core::time::{SimTime, YEAR};
    let y = YEAR.as_secs();
    let born = SimTime::from_secs(-10 * y);
    let at = |years: i64| SimTime::from_secs(born.as_secs() + years * y);
    let cases = [
        (Found::Not, true, 50, DiseaseStatus::NotAffected),
        (Found::Carrier, true, 50, DiseaseStatus::Carrier),
        (
            Found::PenetranceFallen,
            true,
            50,
            DiseaseStatus::PenetranceFallen,
        ),
        (
            Found::PenetranceFallen,
            false,
            50,
            DiseaseStatus::PenetranceFallen,
        ),
        (Found::Onset(45.9), false, 50, DiseaseStatus::Resting),
        (Found::Onset(45.9), false, 40, DiseaseStatus::Resting),
        (Found::Onset(45.9), true, 40, DiseaseStatus::NotYet),
        (
            Found::Onset(45.9),
            true,
            50,
            DiseaseStatus::Affected {
                year: 35,
                age_years: 45,
            },
        ),
        (
            Found::Onset(0.0),
            true,
            0,
            DiseaseStatus::Affected {
                year: -10,
                age_years: 0,
            },
        ),
    ];
    for (found, on, years, expected) in cases {
        assert_eq!(
            disease_status(found, on, born, at(years)),
            expected,
            "{found:?} {on} {years}"
        );
    }
}

/// At the clock's last second a disease whose onset second, rounded up, lies beyond the
/// clock (31 s as years from 31 s before the end, Astra's case of the comparison) has not
/// come and never comes (ADR-0002): "not yet", where the card failed before.
#[test]
fn an_onset_beyond_the_clock_is_not_yet() {
    use sim_core::presentation::card::{DiseaseStatus, Found, disease_status};
    use sim_core::time::{MAX_ABS_SECS, SimTime};
    let born = SimTime::from_secs(MAX_ABS_SECS - 31);
    let found = Found::Onset(31.0 / 31_449_600.0);
    assert_eq!(
        disease_status(found, true, born, SimTime::from_secs(MAX_ABS_SECS)),
        DiseaseStatus::NotYet
    );
}

/// Where a copy from a parent came from (Grok G7, L8), by hand: the parent's one copy on its
/// mother's strand came from the person's grandmother, on its father's from the grandfather,
/// from the pool if that grandparent is not in the run; a parent without a copy made it new.
#[test]
fn the_source_of_a_copy_by_hand() {
    use sim_core::ids::SimId;
    use sim_core::presentation::card::{CopySource, copy_source};
    let sim = |n: u64| SimId::try_from(n).expect("an id");
    let both = [Some(sim(4)), Some(sim(5))];
    assert_eq!(
        copy_source(&[1, 0], both),
        CopySource::Grandmother { sim: sim(4) }
    );
    assert_eq!(
        copy_source(&[0, 1], both),
        CopySource::Grandfather { sim: sim(5) }
    );
    assert_eq!(copy_source(&[1, 0], [None, Some(sim(5))]), CopySource::Pool);
    assert_eq!(copy_source(&[0, 1], [Some(sim(4)), None]), CopySource::Pool);
    assert_eq!(copy_source(&[0, 0], both), CopySource::New);
    assert_eq!(copy_source(&[0, 0], [None, None]), CopySource::New);
}

/// The line "übersprungen" (Grok G7, L8) by hand: only for two copies with no parent
/// carrying two; each copy's source from that parent's copies and its parents in the run.
#[test]
fn the_skipped_line_by_hand() {
    use sim_core::data_id::DataId;
    use sim_core::ids::SimId;
    use sim_core::presentation::card::{CopySource, SkippedLine, skipped_line};
    let sim = |n: u64| SimId::try_from(n).expect("an id");
    let mc1r = DataId::new("core:mc1r").expect("an id");
    let grandparents = [[Some(sim(4)), Some(sim(5))], [Some(sim(6)), Some(sim(7))]];
    let line = |own: &[u8], mother: &[u8], father: &[u8]| {
        skipped_line(mc1r.clone(), own, (mother, father), grandparents)
    };
    // The mother's copy on her mother's strand, the father's on his father's.
    assert_eq!(
        line(&[1, 1], &[1, 0], &[0, 1]),
        Some(SkippedLine {
            locus: mc1r.clone(),
            mother_copy: CopySource::Grandmother { sim: sim(4) },
            father_copy: CopySource::Grandfather { sim: sim(7) },
        })
    );
    // Neither parent carries a copy: both new.
    assert_eq!(
        line(&[1, 1], &[0, 0], &[0, 0]),
        Some(SkippedLine {
            locus: mc1r.clone(),
            mother_copy: CopySource::New,
            father_copy: CopySource::New,
        })
    );
    // A parent with two copies, or a person with fewer than two: no line.
    assert_eq!(line(&[1, 1], &[1, 1], &[0, 1]), None);
    assert_eq!(line(&[1, 1], &[1, 0], &[1, 1]), None);
    assert_eq!(line(&[1, 0], &[1, 0], &[0, 1]), None);
    assert_eq!(line(&[0, 1], &[1, 0], &[0, 1]), None);
}
