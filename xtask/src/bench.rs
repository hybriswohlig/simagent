//! `cargo xtask bench`: P1 of the genetics specification (8.2). Ten founder families over
//! 200 years with the release binary, five runs; the median at most 5 seconds. Outside the
//! gate; the result goes into the weekly report. Every run must give the same manifest.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::{Result, cargo, read, relative, release_run};

/// The scenario of P1: ten couples of `Start`, women of 20 to 29 with men two years older,
/// reference alleles, the hereditary diseases on, until the end of year 199.
pub const SCENARIO: &str = "tests/scenarios/bench_p1.json";
/// How often the scenario runs; the median of the times counts (spec 8.2).
pub const RUNS: usize = 5;
/// The limit of P1 for the median.
pub const LIMIT: Duration = Duration::from_secs(5);

/// The median of the times of the runs: the middle one of an odd count.
pub fn median(times: &[Duration]) -> Option<Duration> {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied()
}

/// What a run leaves: its time, the entries of its log and the living at its end.
struct Measured {
    time: Duration,
    entries: usize,
    living: String,
    manifest: String,
}

/// Builds the release binary of `sim-cli`, runs the scenario `RUNS` times and reports.
pub fn bench(root: &Path) -> Result {
    cargo(
        root,
        &[
            "build",
            "--quiet",
            "--locked",
            "--release",
            "--package",
            "sim-cli",
        ],
    )?;
    let binary = root.join("target").join("release").join("simagent");
    let base = root.join("target").join("xtask-bench");
    let mut runs = Vec::new();
    for number in 1..=RUNS {
        let out = base.join(format!("run-{number}"));
        if out.exists() {
            std::fs::remove_dir_all(&out)
                .map_err(|error| format!("cannot remove {}: {error}", out.display()))?;
        }
        let started = Instant::now();
        let status = Command::new(&binary)
            .args(["run", "--scenario", SCENARIO, "--out"])
            .arg(&out)
            .current_dir(root)
            .status()
            .map_err(|error| format!("cannot run {}: {error}", binary.display()))?;
        let time = started.elapsed();
        if !status.success() {
            return Err(format!("run {number} of {SCENARIO} failed"));
        }
        let census = read(&out.join("census.csv"))?;
        let living = census
            .lines()
            .last()
            .and_then(|row| row.split(',').nth(1))
            .unwrap_or("?")
            .to_string();
        runs.push(Measured {
            time,
            entries: read(&out.join("events.ndjson"))?.lines().count(),
            living,
            manifest: read(&out.join("manifest.json"))?,
        });
        println!(
            "run {number}: {:.3} s",
            runs.last().map_or(0.0, |run| run.time.as_secs_f64())
        );
    }
    let first = runs.first().ok_or("no run")?;
    for (number, run) in (1..).zip(&runs) {
        let found = release_run::differences(&run.manifest, &first.manifest)?;
        if !found.is_empty() {
            return Err(format!(
                "run {number} differs from run 1:\n  {}",
                found.join("\n  ")
            ));
        }
    }
    let times: Vec<Duration> = runs.iter().map(|run| run.time).collect();
    let middle = median(&times).ok_or("no run")?;
    println!(
        "P1: {} with the release binary, {} entries, {} living at the end; median of {RUNS} runs {:.3} s (limit {} s); all runs give the same manifest; outputs in {}",
        SCENARIO,
        first.entries,
        first.living,
        middle.as_secs_f64(),
        LIMIT.as_secs(),
        relative(root, &base),
    );
    if middle <= LIMIT {
        Ok(())
    } else {
        Err(format!(
            "P1 red: the median {:.3} s is above {} s",
            middle.as_secs_f64(),
            LIMIT.as_secs()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_is_the_middle_run() {
        let s = Duration::from_millis;
        assert_eq!(median(&[s(5), s(1), s(9), s(3), s(7)]), Some(s(5)));
        assert_eq!(median(&[s(2)]), Some(s(2)));
        assert_eq!(median(&[]), None);
    }
}
