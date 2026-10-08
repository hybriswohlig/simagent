//! The repro package (ADR-0016): when the core breaks off or a replay diverges, the CLI
//! writes `repro.json`, from which `simagent repro` replays the failure and `cargo xtask
//! repro` makes a test.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sim_core::canonical::YearHash;
use sim_core::ids::InputSeq;
use sim_core::input::{Input, InputRecord};
use sim_core::time::{SimDuration, SimTime, calendar};
use sim_core::version::CORE_VERSION;
use sim_core::world::{World, YearOut};

use crate::error::CliError;
use crate::files;

/// The file name in the output directory of the command.
pub const REPRO: &str = "repro.json";

/// What failed (ADR-0016 section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The core broke off with a message `invariant <name>...`.
    Invariant,
    /// The core broke off with any other message.
    Panic,
    /// The year hashes of a replay differ from its manifest.
    ReplayDiverged,
}

/// The second of the failure and its calendar year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct At {
    pub t: SimTime,
    pub year: i64,
}

/// The content of `repro.json` (ADR-0016 section 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format_version: u32,
    pub kind: Kind,
    pub core_version: u32,
    #[serde(with = "sim_core::hex_u64")]
    pub data_hash: u64,
    #[serde(with = "sim_core::hex_u64")]
    pub world_seed: u64,
    pub input_log: Vec<InputRecord>,
    pub checkpoint: Option<YearHash>,
    pub at: At,
    pub invariant: Option<String>,
    pub detail: String,
    /// Only for a diverged replay: the year hashes of the manifest up to the first that
    /// differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Vec<YearHash>>,
}

/// The text of a panic payload.
fn message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_owned()
    }
}

/// The name of the invariant in a message `invariant <name>...`: up to the first `:` or
/// space.
fn invariant_name(message: &str) -> Option<String> {
    let rest = message.strip_prefix("invariant ")?;
    let name: String = rest
        .chars()
        .take_while(|c| *c != ':' && !c.is_whitespace())
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The second after the last closed one and its year.
fn open_second(world: &World) -> At {
    let t = world.closed_through() + SimDuration::from_secs(1);
    At {
        t,
        year: calendar(t).year,
    }
}

/// Writes the package into `dir` (created if need be: the start can break off before the
/// run directory exists) and gives its path.
pub fn write(dir: &Path, package: &Package) -> Result<PathBuf, CliError> {
    std::fs::create_dir_all(dir).map_err(|error| CliError::io(dir, &error))?;
    let path = dir.join(REPRO);
    files::write(&path, (files::json(package) + "\n").as_bytes())?;
    Ok(path)
}

/// Runs `step` on the world; a break-off of the core writes the package into `dir` and
/// gives the error `core_invariant` or `core_panic` that names its path (ADR-0016). The
/// world is not used after a break-off. `pending` is the input `step` submits, if any: the
/// world records an input only after it applied it, so a break-off while applying it
/// would leave it out of the package; it goes in with the stamp it would have had.
pub fn guarded<T>(
    world: &mut World,
    out: &mut YearOut,
    dir: &Path,
    pending: Option<&Input>,
    step: impl FnOnce(&mut World, &mut YearOut) -> T,
) -> Result<T, CliError> {
    match catch_unwind(AssertUnwindSafe(|| step(world, out))) {
        Ok(value) => Ok(value),
        Err(payload) => {
            let text = message(payload.as_ref());
            let invariant = invariant_name(&text);
            let kind = if invariant.is_some() {
                Kind::Invariant
            } else {
                Kind::Panic
            };
            let mut input_log = world.input_log().to_vec();
            if let Some(input) = pending {
                let next = u64::try_from(input_log.len())
                    .ok()
                    .and_then(|len| InputSeq::try_from(len + 1).ok())
                    .unwrap_or_else(|| panic!("invariant cli.input_count"));
                input_log.push(InputRecord {
                    t: world.closed_through(),
                    seq: next,
                    input: input.clone(),
                });
            }
            let package = Package {
                format_version: files::FORMAT_VERSION,
                kind,
                core_version: CORE_VERSION,
                data_hash: world.data().hash(),
                world_seed: world.state().world_seed().unwrap_or_default(),
                input_log,
                checkpoint: out.year_hashes.last().copied(),
                at: open_second(world),
                invariant,
                detail: text.clone(),
                expected: None,
            };
            let path = write(dir, &package)?;
            let code = match kind {
                Kind::Invariant => "core_invariant",
                Kind::Panic | Kind::ReplayDiverged => "core_panic",
            };
            Err(CliError::new(
                code,
                format!("{text} (repro package: {})", path.display()),
            ))
        }
    }
}

/// The package of a diverged replay: the world stands at the end of the year that
/// differs, `expected` are the hashes of the manifest up to it.
pub fn diverged(
    world: &World,
    out: &YearOut,
    expected: Vec<YearHash>,
    detail: &str,
    dir: &Path,
) -> Result<PathBuf, CliError> {
    let checkpoint = out.year_hashes.last().copied();
    let t = world.closed_through();
    let package = Package {
        format_version: files::FORMAT_VERSION,
        kind: Kind::ReplayDiverged,
        core_version: CORE_VERSION,
        data_hash: world.data().hash(),
        world_seed: world.state().world_seed().unwrap_or_default(),
        input_log: world.input_log().to_vec(),
        checkpoint,
        at: At {
            t,
            year: calendar(t).year,
        },
        invariant: None,
        detail: detail.to_owned(),
        expected: Some(expected),
    };
    write(dir, &package)
}

/// `simagent repro <file>` (ADR-0016 section 3): replays the package in a new world up
/// to its second; `Ok` once the failure is gone, else `repro_reproduced`.
pub fn check(file: &Path) -> Result<(), CliError> {
    let package: Package = files::parse_strict(files::read_text(file)?.trim_end())
        .filter(|package: &Package| package.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| CliError::new("repro_invalid", "not a repro package of format 1"))?;
    if package.core_version != CORE_VERSION {
        let detail = format!(
            "the repro package is of core version {}, this is {CORE_VERSION}",
            package.core_version
        );
        return Err(CliError::new("replay_foreign_core_version", detail));
    }
    let data = crate::data::core_data()?;
    if package.data_hash != data.hash() {
        let detail = format!(
            "the repro package has data hash {:016x}, this is {:016x}",
            package.data_hash,
            data.hash()
        );
        return Err(CliError::new("replay_foreign_data_hash", detail));
    }
    let reproduced = |detail: String| CliError::new("repro_reproduced", detail);
    let mut world = World::new(data);
    let mut out = YearOut::default();
    let run = catch_unwind(AssertUnwindSafe(|| -> Result<(), CliError> {
        for record in &package.input_log {
            if record.t > world.closed_through() {
                world.advance_until(&mut out, record.t, &mut |_| false);
            }
            let stamp = world
                .submit(record.input.clone())
                .map_err(|rejected| CliError::new("repro_invalid", files::code(&rejected.code)))?;
            if stamp.t != record.t || stamp.seq != record.seq {
                return Err(CliError::new(
                    "repro_invalid",
                    format!("the input {} is stamped otherwise", record.seq.get()),
                ));
            }
        }
        if package.at.t > world.closed_through() {
            world.advance_until(&mut out, package.at.t, &mut |_| false);
        }
        Ok(())
    }));
    match (run, package.kind, &package.expected) {
        (Err(payload), _, _) => Err(reproduced(message(payload.as_ref()))),
        (Ok(Err(error)), _, _) => Err(error),
        (Ok(Ok(())), Kind::Invariant | Kind::Panic, None) => Ok(()),
        (Ok(Ok(())), Kind::ReplayDiverged, Some(expected)) => {
            // Every year up to the one of the package, no fewer and no more.
            let produced: Vec<YearHash> = out
                .year_hashes
                .iter()
                .filter(|hash| hash.year <= package.at.year)
                .copied()
                .collect();
            for (got, want) in produced.iter().zip(expected) {
                if got != want {
                    return Err(reproduced(format!(
                        "year {} has hash {:016x}, the package says {:016x}",
                        got.year, got.hash, want.hash
                    )));
                }
            }
            if produced.len() != expected.len() {
                return Err(reproduced(format!(
                    "the replay has {} year hashes up to year {}, the package {}",
                    produced.len(),
                    package.at.year,
                    expected.len()
                )));
            }
            Ok(())
        }
        (Ok(Ok(())), _, _) => Err(CliError::new(
            "repro_invalid",
            "expected year hashes belong to a diverged replay, and only there",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0016 `a_core_panic_writes_a_repro_package`: a break-off of the core inside the
    /// guard writes the package with the name of the invariant and gives an error that
    /// names its path.
    #[test]
    fn a_core_panic_writes_a_repro_package() {
        let dir = std::env::temp_dir().join(format!("simagent-repro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut world = World::new(crate::data::core_data().expect("data"));
        let mut out = YearOut::default();
        let error = guarded(&mut world, &mut out, &dir, None, |_, _| {
            panic!("invariant test.repro: sim 1");
        })
        .expect_err("broke off");
        assert_eq!(error.code, "core_invariant");
        let path = dir.join(REPRO);
        assert_eq!(
            error.detail,
            format!(
                "invariant test.repro: sim 1 (repro package: {})",
                path.display()
            )
        );
        let package: Package =
            files::parse_strict(files::read_text(&path).expect("read").trim_end())
                .expect("package");
        assert_eq!(package.kind, Kind::Invariant);
        assert_eq!(package.invariant.as_deref(), Some("test.repro"));
        assert_eq!(package.detail, "invariant test.repro: sim 1");
        assert_eq!(package.core_version, CORE_VERSION);
        assert_eq!(
            package.at.t,
            world.closed_through() + SimDuration::from_secs(1)
        );
        // Any other message is a panic of the core.
        let error = guarded(&mut world, &mut out, &dir, None, |_, _| {
            panic!("index out of bounds")
        })
        .expect_err("broke off");
        assert_eq!(error.code, "core_panic");
        // An input that breaks off while it is applied goes into the package.
        let input = Input::ArrangePartnership(sim_core::input::ArrangePartnership {
            woman: sim_core::ids::SimId::try_from(1).expect("id"),
            man: sim_core::ids::SimId::try_from(2).expect("id"),
        });
        let _ = guarded(&mut world, &mut out, &dir, Some(&input), |_, _| {
            panic!("invariant test.apply")
        });
        let package: Package =
            files::parse_strict(files::read_text(&path).expect("read").trim_end())
                .expect("package");
        assert_eq!(package.input_log.len(), 1);
        assert_eq!(package.input_log[0].input, input);
        assert_eq!(package.input_log[0].seq.get(), 1);
        assert_eq!(package.input_log[0].t, world.closed_through());
        std::fs::remove_dir_all(&dir).expect("removed");
    }

    #[test]
    fn the_name_of_the_invariant_is_taken_from_the_message() {
        assert_eq!(
            invariant_name("invariant test.repro: sim 1").as_deref(),
            Some("test.repro")
        );
        assert_eq!(
            invariant_name("invariant body_input_age: -1").as_deref(),
            Some("body_input_age")
        );
        assert_eq!(
            invariant_name("invariant cli.start_stamp").as_deref(),
            Some("cli.start_stamp")
        );
        assert_eq!(invariant_name("index out of bounds"), None);
        assert_eq!(invariant_name("invariant "), None);
    }
}
