//! `cargo xtask story`: the unscripted chain on the household of four (ADR-0031,
//! acceptance test 6, addendum of 2026-11-15).
//!
//! Seeds 1 to 20 of `tests/scenarios/social_chain.json`; seed `n` takes the ages of the
//! house-week test family `n - 1` (`family_ages`, the grid of `crates/sim-core/tests/
//! house_week.rs`), the file holds seed 1. Each run is the CLI's inputs:
//! start, advance to 86398, move household 1 in, advance to the scenario's end. The domain
//! log is the text the CLI writes as `events.ndjson`. The bar is one full chain over the
//! seeds and 16 seeds with a two-hop passing. Three canaries each change one datum
//! (`beliefs.heard_cap` with `beliefs.confirm_step`, `choice.avoid_below_trust`,
//! `beliefs.refute_ppm`) to 0 through
//! [`Data::with_social`] and must fail that bar: the passing canary with no two-hop seed,
//! avoidance and refutation with no full chain. A debug build runs the release build, so
//! the time is the release time. Not part of `quick`.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::Value;
use sim_core::data::Data;
use sim_core::ids::HouseholdId;
use sim_core::input::{Input, MoveIn, Start};
use sim_core::social::data::SocialData;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

use crate::{Result, cargo_program};

/// The second the run stops for the move-in: 23:59:58 of day 0, the CLI's `--until` before
/// household 1 moves in.
const MOVE_IN_AT: i64 = 86_398;
/// World seeds of the measurement (ADR-0031, addendum of 2026-11-15).
const FIRST_SEED: u64 = 1;
const LAST_SEED: u64 = 20;
/// At least this many seeds pass a sentence on over two hops (ADR-0031, acceptance test 6).
const TWO_HOP_BAR: usize = 16;
/// At least one full chain over the seeds.
const CHAIN_BAR: usize = 1;
const SCENARIO: &str = "tests/scenarios/social_chain.json";
/// Seconds of a year of 364 days.
const YEAR_SECS: i64 = 364 * 86_400;

/// The ages in whole years of the house-week test family `i` (`crates/sim-core/tests/
/// house_week.rs`): the woman, the man and the two girls, in fixed steps through 30 to 45
/// and 6 to 15.
pub(crate) fn family_ages(i: u64) -> [i64; 4] {
    let i = i64::try_from(i).unwrap_or(0);
    [
        30 + i % 16,
        30 + (7 * i + 3) % 16,
        6 + i % 10,
        6 + (3 * i + 5) % 10,
    ]
}

/// One seed's checker counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SeedRow {
    pub seed: u64,
    pub passings: usize,
    pub chains: usize,
    /// A furthest partial story misses only `avoided`: every other link is there.
    pub only_avoided: bool,
    /// A furthest partial story misses only `refuted` and `trust_loss` (the loss is caused by
    /// the refutation): every other link is there.
    pub only_refuted: bool,
}

/// The counts the bar reads, and the totals the table prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub two_hop_seeds: usize,
    pub chain_seeds: usize,
    pub passings: usize,
    pub chains: usize,
    /// Seeds whose furthest partial misses only `avoided`.
    pub only_avoided_seeds: usize,
    /// Seeds whose furthest partial misses only `refuted` and `trust_loss`.
    pub only_refuted_seeds: usize,
}

/// Which datum a canary sets to 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Canary {
    /// `beliefs.heard_cap`: a hearer takes over nothing, so nothing is passed on.
    Heard,
    /// `choice.avoid_below_trust`: trust is never under the bar, so nobody is avoided.
    Avoid,
    /// `beliefs.refute_ppm`: a refutation is never drawn.
    Refute,
}

impl Canary {
    fn label(self) -> &'static str {
        match self {
            Canary::Heard => "heard_cap=0 confirm_step=0",
            Canary::Avoid => "avoid_below_trust=0",
            Canary::Refute => "refute_ppm=0",
        }
    }
}

/// Seeds with a two-hop passing, seeds with a full chain, and the sums.
pub(crate) fn summarize(rows: &[SeedRow]) -> Summary {
    Summary {
        two_hop_seeds: rows.iter().filter(|row| row.passings > 0).count(),
        chain_seeds: rows.iter().filter(|row| row.chains > 0).count(),
        passings: rows.iter().map(|row| row.passings).sum(),
        chains: rows.iter().map(|row| row.chains).sum(),
        only_avoided_seeds: rows.iter().filter(|row| row.only_avoided).count(),
        only_refuted_seeds: rows.iter().filter(|row| row.only_refuted).count(),
    }
}

/// The bar: at least one full chain, and at least [`TWO_HOP_BAR`] seeds with a two-hop
/// passing. The totals are not the bar.
pub(crate) fn batch_passes(summary: &Summary) -> bool {
    summary.chain_seeds >= CHAIN_BAR && summary.two_hop_seeds >= TWO_HOP_BAR
}

/// The canary fails the same bar for the reason its datum is there to remove.
/// Passing on must leave no two-hop seed. Avoidance and refutation must leave no full chain,
/// and some seed must still have every other link (a positive control: the batch is not
/// silent, only that link is gone). The passing canary also keeps a heard sentence from
/// being refuted, since a refutation needs certainty above 0 (ADR-0031, addendum of
/// 2026-11-15); its own test is the two-hop count.
pub(crate) fn canary_is_red(canary: Canary, summary: &Summary) -> bool {
    if batch_passes(summary) {
        return false;
    }
    match canary {
        Canary::Heard => summary.two_hop_seeds == 0,
        Canary::Avoid => summary.chain_seeds == 0 && summary.only_avoided_seeds > 0,
        Canary::Refute => summary.chain_seeds == 0 && summary.only_refuted_seeds > 0,
    }
}

struct Scenario {
    start: Start,
    until: SimTime,
}

struct Batch {
    rows: Vec<SeedRow>,
}

/// `cargo xtask story`.
pub fn command(root: &Path) -> Result {
    if cfg!(debug_assertions) {
        let status = Command::new(cargo_program())
            .args([
                "run",
                "--release",
                "--locked",
                "--quiet",
                "--package",
                "xtask",
            ])
            .args(["--", "story"])
            .current_dir(root)
            .status()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err("the release build of story failed".to_string())
        };
    }
    run(root)
}

fn run(root: &Path) -> Result {
    let started = Instant::now();
    let scenario = scenario(root)?;
    let data = crate::criteria::village::repository_data(root)?;
    println!("| batch | two-hop seeds | chain seeds | passings | chains |");
    let baseline = batch(&data, &scenario)?;
    let baseline_summary = summarize(&baseline.rows);
    println!("{}", line("baseline", &baseline));
    if !batch_passes(&baseline_summary) {
        print_time(started);
        return Err(format!(
            "baseline misses the bar: {}",
            miss(&baseline_summary)
        ));
    }
    let mut problems = Vec::new();
    for canary in [Canary::Heard, Canary::Avoid, Canary::Refute] {
        let shifted = with_canary(&data, canary)?;
        let ran = batch(&shifted, &scenario)?;
        println!("{}", line(canary.label(), &ran));
        let summary = summarize(&ran.rows);
        if batch_passes(&summary) {
            problems.push(format!("{} passes the check", canary.label()));
        } else if !canary_is_red(canary, &summary) {
            problems.push(format!(
                "{} fails the check, but not as specified ({} two-hop seeds, {} chain seeds)",
                canary.label(),
                summary.two_hop_seeds,
                summary.chain_seeds
            ));
        }
    }
    print_time(started);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

fn print_time(started: Instant) {
    let secs = started.elapsed().as_secs_f64();
    println!("story: {secs:.3} s in release");
}

fn line(name: &str, batch: &Batch) -> String {
    let summary = summarize(&batch.rows);
    let chains = seed_list(&batch.rows, |row| row.chains > 0);
    format!(
        "| {name} | {}/{} | {} ({chains}) | {} | {} |",
        summary.two_hop_seeds,
        batch.rows.len(),
        summary.chain_seeds,
        summary.passings,
        summary.chains,
    )
}

fn seed_list(rows: &[SeedRow], keep: impl Fn(&SeedRow) -> bool) -> String {
    let ids: Vec<String> = rows
        .iter()
        .filter(|row| keep(row))
        .map(|row| row.seed.to_string())
        .collect();
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(" ")
    }
}

fn miss(summary: &Summary) -> String {
    let mut parts = Vec::new();
    if summary.chain_seeds < CHAIN_BAR {
        parts.push(format!(
            "{} chain seeds, below {CHAIN_BAR}",
            summary.chain_seeds
        ));
    }
    if summary.two_hop_seeds < TWO_HOP_BAR {
        parts.push(format!(
            "{} two-hop seeds, below {TWO_HOP_BAR}",
            summary.two_hop_seeds
        ));
    }
    if parts.is_empty() {
        "the bar holds".to_string()
    } else {
        parts.join("; ")
    }
}

fn scenario(root: &Path) -> Result<Scenario> {
    let path = root.join(SCENARIO);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let start: Start = serde_json::from_value(file["start"].clone())
        .map_err(|e| format!("{}: start: {e}", path.display()))?;
    let until = file["until"]
        .as_i64()
        .ok_or_else(|| format!("{}: until is not a time", path.display()))?;
    Ok(Scenario {
        start,
        until: SimTime::from_secs(until),
    })
}

/// The repository data with one canary datum set to 0.
fn with_canary(data: &Data, canary: Canary) -> Result<Data> {
    let mut social: SocialData = data
        .social()
        .ok_or("the repository data hold no social data")?
        .clone();
    match canary {
        // A heard sentence is believed at 0 and a repeat does not raise it, so nothing heard
        // becomes material (ADR-0031, addendum of 2026-11-15).
        Canary::Heard => {
            social.beliefs.heard_cap = 0;
            social.beliefs.confirm_step = 0;
        }
        Canary::Avoid => social.choice.avoid_below_trust = 0,
        Canary::Refute => social.beliefs.refute_ppm = 0,
    }
    Ok(data.clone().with_social(social))
}

fn batch(data: &Data, scenario: &Scenario) -> Result<Batch> {
    let mut rows = Vec::new();
    for seed in FIRST_SEED..=LAST_SEED {
        let mut start = scenario.start.clone();
        start.world_seed = seed;
        let ages = family_ages(seed - FIRST_SEED);
        if start.founders.len() != ages.len() {
            return Err(format!("{SCENARIO}: four founders expected"));
        }
        for (founder, years) in start.founders.iter_mut().zip(ages) {
            founder.born = SimTime::from_secs(-years * YEAR_SECS);
        }
        let text = events(data, start, scenario.until)?;
        let report = crate::story::check(&text)?;
        if !report.warnings.is_empty() {
            return Err(format!("seed {seed}: {}", report.warnings.join("; ")));
        }
        rows.push(SeedRow {
            seed,
            passings: report.hops,
            chains: report.chains.len(),
            only_avoided: missing_only(&report, &["avoided"]),
            only_refuted: missing_only(&report, &["refuted", "trust_loss"]),
        });
    }
    Ok(Batch { rows })
}

/// Whether a furthest partial of the run misses exactly `links`.
fn missing_only(report: &crate::story::Report, links: &[&str]) -> bool {
    (report.groups.iter()).any(|group| group.missing.as_slice() == links)
}

/// The domain log of one run, one JSON entry per line, as the CLI writes `events.ndjson`.
fn events(data: &Data, start: Start, until: SimTime) -> Result<String> {
    let mut world = World::new(data.clone());
    world
        .submit(Input::Start(start))
        .map_err(|e| format!("start: {e:?}"))?;
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(MOVE_IN_AT), &mut |_| false);
    let household = HouseholdId::try_from(1).map_err(|e| format!("household 1: {e}"))?;
    world
        .submit(Input::MoveIn(MoveIn { household }))
        .map_err(|e| format!("move in: {e:?}"))?;
    world.advance_until(&mut out, until, &mut |_| false);
    let mut text = String::new();
    for entry in world.take_log() {
        let line = serde_json::to_string(&entry).map_err(|e| format!("log entry: {e}"))?;
        text.push_str(&line);
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        two_hop_seeds: usize,
        chain_seeds: usize,
        passings: usize,
        chains: usize,
    ) -> Summary {
        Summary {
            two_hop_seeds,
            chain_seeds,
            passings,
            chains,
            only_avoided_seeds: 0,
            only_refuted_seeds: 0,
        }
    }

    /// The bar is the spec's 16 seeds and one chain. A louder total under the seed counts
    /// stays red, and one chain with 16 seeds stays green.
    #[test]
    fn the_family_ages_follow_the_house_week_grid() {
        // Seeds 1, 7, 8 and 20 of the measurement (by hand from the grid).
        assert_eq!(family_ages(0), [30, 33, 6, 11]);
        assert_eq!(family_ages(6), [36, 43, 12, 9]);
        assert_eq!(family_ages(7), [37, 34, 13, 12]);
        assert_eq!(family_ages(19), [33, 38, 15, 8]);
    }

    #[test]
    fn the_bar_is_one_chain_and_sixteen_two_hop_seeds() {
        assert!(batch_passes(&summary(16, 1, 16, 1)));
        assert!(batch_passes(&summary(20, 2, 40, 3)));
        assert!(!batch_passes(&summary(15, 1, 1_000, 50)));
        assert!(!batch_passes(&summary(16, 0, 16, 0)));
        assert!(!batch_passes(&summary(0, 0, 0, 0)));
    }

    /// A seed counts once, when it has any passing or any chain. The sums are the totals.
    #[test]
    fn a_seed_counts_once_toward_the_bar() {
        let rows = [
            SeedRow {
                seed: 1,
                passings: 4,
                chains: 0,
                only_avoided: false,
                only_refuted: false,
            },
            SeedRow {
                seed: 7,
                passings: 5,
                chains: 1,
                only_avoided: false,
                only_refuted: false,
            },
            SeedRow {
                seed: 8,
                passings: 4,
                chains: 2,
                only_avoided: true,
                only_refuted: false,
            },
            SeedRow {
                seed: 11,
                passings: 0,
                chains: 0,
                only_avoided: false,
                only_refuted: false,
            },
        ];
        let summed = summarize(&rows);
        assert_eq!((summed.two_hop_seeds, summed.chain_seeds), (3, 2));
        assert_eq!((summed.passings, summed.chains), (13, 3));
        assert_eq!(
            (summed.only_avoided_seeds, summed.only_refuted_seeds),
            (1, 0)
        );
    }

    /// A green batch is not a red canary. Passing on is red only at zero two-hop seeds.
    /// Avoidance and refutation are red when no seed has a chain, even if every seed
    /// passes a sentence on.
    #[test]
    fn a_canary_is_red_only_in_its_specified_way() {
        let green = summary(16, 1, 16, 1);
        assert!(batch_passes(&green));
        for canary in [Canary::Heard, Canary::Avoid, Canary::Refute] {
            assert!(!canary_is_red(canary, &green));
        }
        let silent = summary(0, 0, 0, 0);
        assert!(!batch_passes(&silent));
        assert!(canary_is_red(Canary::Heard, &silent));
        assert!(!canary_is_red(Canary::Heard, &summary(1, 0, 1, 0)));
        let no_chain = summary(20, 0, 48, 0);
        assert!(!batch_passes(&no_chain));
        // No chain but no seed with the other links: silent, not red for this reason.
        assert!(!canary_is_red(Canary::Avoid, &no_chain));
        assert!(!canary_is_red(Canary::Refute, &no_chain));
        assert!(!canary_is_red(Canary::Avoid, &silent));
        let avoid_gone = Summary {
            only_avoided_seeds: 2,
            ..no_chain
        };
        assert!(canary_is_red(Canary::Avoid, &avoid_gone));
        assert!(!canary_is_red(Canary::Refute, &avoid_gone));
        let refute_gone = Summary {
            only_refuted_seeds: 1,
            ..no_chain
        };
        assert!(canary_is_red(Canary::Refute, &refute_gone));
        assert!(!canary_is_red(Canary::Avoid, &refute_gone));
        assert!(!canary_is_red(Canary::Avoid, &summary(10, 1, 10, 1)));
        assert!(!canary_is_red(Canary::Refute, &green));
    }
}
