//! The man's answer and the target ages (genetics specification 11.2a, ADR-0018 addendum of
//! 2026-11-06): the chosen man is asked, with the weights of the group of six values; after
//! a refusal the round ends; `ArrangePartnership` does not ask.
//!
//! The expected months and partners come from the independent draws of
//! `tools/rng_reference.py` and the rule of 11.2 and 11.2a applied by hand (a script that
//! uses `tools/world_reference.py` for the draws, not this code): per round the search draw
//! (`MARKET_SEARCH`, 19), her utility `−w_a · (Δa − μ_a − ν_f)² + G` with `G` from
//! `PAIRING_NOISE` (21) and his `U_m = −w_am · (a_f − t_m(a_m))² + G_m` with `G_m` from
//! `PAIRING_ACCEPT` (33) and the taste `ν` from `PAIRING_TASTE` (34), `normal(0, σ_ν)`. The
//! repository's data with the search height `c` = 1 and almost no deaths; the weights of the
//! similarity terms do not appear because the test map names no cognition or height trait.
//!
//! Scenario "answer": seed 0xff, founder tag `test:answer`, a woman of 20 (sim 1) and a man
//! of 22 (sim 2), single. `ν` of the man is +5.4009, so he aims at 27.4 and she is 7.4 years
//! under his target. His Gumbel `G_m` at the rounds she searches: month 1 −0.2653, month 3
//! 2.8330, month 29 4.6751 (rounds 2 and 4 are rounds she does not search in).
//!
//! Scenario "choice": a woman of 20 (sim 1), a man of 26 (sim 2) and a man of 21 (sim 3),
//! founder tag `test:answer2`.

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::Sex;
use sim_core::ids::SimId;
use sim_core::input::{ArrangePartnership, Founder, Input, Start};
use sim_core::log::LogEntry;
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("an id")
}

/// The data of the tests: the repository's life course data with the search height 1 and
/// almost no deaths, and with the group of the man's answer if `answer` names his weight and
/// the taste's spread (the other four values are the specification's).
fn data(answer: Option<(f64, f64)>) -> Data {
    let (calibration, genome_map) = common::death_files(1e-9, None, common::repository_diseases());
    let mut demography = common::demography();
    demography["values"]["core:market_search_height_female"]["value"] = json!(1.0);
    if let Some((weight, spread)) = answer {
        for (key, unit, value) in [
            ("core:pairing_taste_sd", "year", spread),
            ("core:pairing_male_target_slope", "ratio", 0.4),
            ("core:pairing_male_target_from", "year", 22.0),
            ("core:pairing_male_target_cap", "year", 15.0),
            ("core:pairing_male_target_min", "year", 21.0),
            ("core:pairing_weight_age_male", "logit", weight),
        ] {
            demography["values"][key] = json!({"value": value, "unit": unit});
        }
    }
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

impl Run {
    /// Single founders born `years` before the start, in this order: sim 1, 2, ...
    fn new(data: Data, seed: u64, tag: &str, founders: &[(i64, Sex)]) -> Run {
        let mut world = World::new(data);
        world
            .submit(Input::Start(Start {
                world_seed: seed,
                founder_tag: DataId::new(tag).expect("valid"),
                world_params: WorldParams {
                    hereditary_diseases: false,
                    nutrition: Vec::new(),
                },
                founders: (founders.iter())
                    .map(|&(years, sex)| Founder {
                        born: SimTime::from_secs(-years * Y),
                        sex,
                        alleles: Vec::new(),
                    })
                    .collect(),
                couples: Vec::new(),
                capacity: None,
                pool: None,
            }))
            .expect("accepted");
        let log = world.take_log();
        Run {
            world,
            out: YearOut::default(),
            log,
        }
    }

    fn advance(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    /// The partnerships formed so far as (woman, man, formed).
    fn pairs(&self) -> Vec<(u64, u64, i64)> {
        self.world
            .pedigree()
            .partnerships
            .iter()
            .map(|p| (p.woman.get(), p.man.get(), p.formed.as_secs()))
            .collect()
    }

    /// The causes of the entry of the first partnership, as JSON.
    fn causes(&self) -> Vec<Value> {
        let entry = (self.log.iter())
            .map(|entry| serde_json::to_value(entry).expect("json"))
            .find(|entry| entry["payload"].get("core:partnered").is_some())
            .expect("a partnership entry");
        entry["causes"].as_array().expect("causes").clone()
    }
}

fn answer(data: Data) -> Run {
    Run::new(
        data,
        0xff,
        "test:answer",
        &[(20, Sex::Female), (22, Sex::Male)],
    )
}

fn choice(data: Data, seed: u64) -> Run {
    Run::new(
        data,
        seed,
        "test:answer2",
        &[(20, Sex::Female), (26, Sex::Male), (21, Sex::Male)],
    )
}

/// The first month of a partnership of `run` within `months` months, if any.
fn first_month(mut run: Run, months: i64) -> Option<i64> {
    run.advance(months * MONTH);
    run.pairs().first().map(|&(_, _, formed)| formed / MONTH)
}

/// His weight and the taste set when he accepts: `U_m = G_m − w_am · (a_f − t_m)² > 0` in the
/// rounds she searches (months 1, 3, ... of the script). With `w_am` = 1e-6 only the sign of
/// `G_m` counts: refused in month 1 (−0.2653), accepted in month 3 (2.8330). With 0.06 the
/// age term is 0.06 · (about 7.4)² = 3.3 at the start, so only month 29 (4.6751 − 2.54 = 2.13)
/// clears it; the margins of every refused round before are at least 0.37. With 0.5 and the
/// taste (his target 27.4) every round is refused for 140 months (least margin 1.54); without
/// the taste (σ_ν = 0, his target 22, so `d` = −1.9 at month 3) month 3 clears it: 2.8330 −
/// 0.5 · 1.9² = 1.01. A weight of 1e6 refuses everything.
#[test]
fn the_man_accepts_by_his_weight_and_his_taste() {
    for (weight, spread, month) in [
        (1e-6, 3.0, Some(3)),
        (0.06, 3.0, Some(29)),
        (0.5, 3.0, None),
        (0.5, 0.0, Some(3)),
        (1e6, 3.0, None),
    ] {
        let run = answer(data(Some((weight, spread))));
        assert_eq!(first_month(run, 140), month, "{weight} {spread}");
    }
}

/// A refusal is no state: she searches again in month 3 and he accepts. The partnership is
/// formed at that round, names the rule and both draws: hers `PAIRING_NOISE` (tag 21, her
/// key, month 3, his key) and his `PAIRING_ACCEPT` (tag 33, his key, month 3, her key).
#[test]
fn a_refusal_is_no_state_and_the_partnership_names_both_draws() {
    let mut run = answer(data(Some((1e-6, 3.0))));
    run.advance(3 * MONTH - 1);
    assert!(run.pairs().is_empty());
    assert!(run.world.pedigree().partnerships.is_empty());
    run.advance(3 * MONTH);
    assert_eq!(run.pairs(), [(1, 2, 3 * MONTH)]);
    let causes = run.causes();
    assert_eq!(causes.len(), 4, "{causes:?}");
    assert_eq!(causes[1], json!({"rule": "core:partner_market"}));
    assert_eq!(
        causes[2],
        json!({"random": {
            "tag": "0000000000000015",
            "subject": "09e2636c710c3b7a",
            "occasion": "0000000000000003",
            "detail": "480f66c0286df3b5",
        }})
    );
    assert_eq!(
        causes[3],
        json!({"random": {
            "tag": "0000000000000021",
            "subject": "480f66c0286df3b5",
            "occasion": "0000000000000003",
            "detail": "09e2636c710c3b7a",
        }})
    );
}

/// "Choice", seed 4, the weight 1e-6 (so the sign of `G_m` decides). Round 1: she proposes to
/// the man of 26 (her utility 2.53 against 0.02); his `U_m` = −1.17, he refuses; the man of 21
/// would accept (+1.06) but is not asked, so no partnership. Round 2: she proposes to the man
/// of 21 (2.00 against 1.24), who refuses (−1.21) while the man of 26 would accept (+0.93);
/// again nobody. Round 3: she proposes to the man of 26, who accepts (+0.66): the
/// partnership of sims 1 and 2 forms in month 3.
#[test]
fn after_a_refusal_there_is_no_second_proposal_in_the_round() {
    let mut run = choice(data(Some((1e-6, 3.0))), 4);
    run.advance(MONTH);
    assert!(run.pairs().is_empty());
    run.advance(2 * MONTH);
    assert!(run.pairs().is_empty());
    run.advance(3 * MONTH);
    assert_eq!(run.pairs(), [(1, 2, 3 * MONTH)]);
    let causes = run.causes();
    assert_eq!(causes.len(), 4, "{causes:?}");
    assert_eq!(causes[1], json!({"rule": "core:partner_market"}));
    assert_eq!(
        causes[2],
        json!({"random": {
            "tag": "0000000000000015",
            "subject": "5972c50bc8af3f94",
            "occasion": "0000000000000003",
            "detail": "700b34fbb9d7b242",
        }})
    );
    assert_eq!(
        causes[3],
        json!({"random": {
            "tag": "0000000000000021",
            "subject": "700b34fbb9d7b242",
            "occasion": "0000000000000003",
            "detail": "5972c50bc8af3f94",
        }})
    );
}

/// Without the group the chosen man just accepts: the same founders, seed 4 and data, no
/// six values. In round 1 she chooses the man of 26 (2.51 against 0.12 without any taste) and
/// the partnership forms in month 1, naming the rule and her draw only, as before.
#[test]
fn without_the_group_the_chosen_man_does_not_answer() {
    let mut run = choice(data(None), 4);
    run.advance(MONTH);
    assert_eq!(run.pairs(), [(1, 2, MONTH)]);
    let causes = run.causes();
    assert_eq!(causes.len(), 3, "{causes:?}");
    assert_eq!(causes[1], json!({"rule": "core:partner_market"}));
    assert_eq!(causes[2]["random"]["tag"], json!("0000000000000015"));
}

/// Her taste moves her target age: `t_f = a_f + μ_a + ν_f`. Seed 78, round 1, the weight
/// 1e-6 (both men accept): with `σ_ν` = 0 she prefers the man of 26 (5.9 older, near
/// `μ_a` = 4.69; her utility margin 0.49), and the man accepts (+4.35); with `σ_ν` = 3 her
/// taste is −4.18, so she aims 0.5 years from her own age and takes the man of 21 (margin
/// 0.35), who accepts (+2.06).
#[test]
fn her_taste_moves_her_choice() {
    let mut without = choice(data(Some((1e-6, 0.0))), 78);
    without.advance(MONTH);
    assert_eq!(without.pairs(), [(1, 2, MONTH)]);
    let mut with = choice(data(Some((1e-6, 3.0))), 78);
    with.advance(MONTH);
    assert_eq!(with.pairs(), [(1, 3, MONTH)]);
}

/// `ArrangePartnership` does not ask the man: with a weight of 1e6 the market would never
/// pair these two (the first test), yet the input forms the partnership at the next second
/// and the entry names no `PAIRING_ACCEPT` draw (tag 33); the market then pairs nobody else.
#[test]
fn an_arrangement_does_not_ask_the_man() {
    let mut run = answer(data(Some((1e6, 3.0))));
    let stamp = 100 * 86_400;
    run.advance(stamp);
    assert!(run.pairs().is_empty());
    run.world
        .submit(Input::ArrangePartnership(ArrangePartnership {
            woman: sim(1),
            man: sim(2),
        }))
        .expect("accepted");
    run.log.extend(run.world.take_log());
    assert_eq!(run.pairs(), [(1, 2, stamp + 1)]);
    let causes = run.causes();
    let draws: Vec<&Value> = causes
        .iter()
        .filter(|c| c.get("random").is_some())
        .collect();
    assert!(draws.is_empty(), "{causes:?}");
    run.advance(20 * MONTH);
    assert_eq!(run.pairs(), [(1, 2, stamp + 1)]);
}
