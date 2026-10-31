//! `simagent`: the command line runner. It loads scenarios, drives the core through
//! its command schema and writes the run directory. It never decides gameplay
//! (ADR-0000); every number in its files comes from the core.

pub mod data;
pub mod error;
pub mod files;
pub mod lock;
pub mod names;
pub mod repro;
pub mod session;
pub mod view;

use std::ffi::OsString;
use std::path::Path;

use clap::{Arg, ArgAction, ArgGroup, Command};
use serde::Serialize;
use sim_core::data::Data;
use sim_core::ids::SimId;
use sim_core::input::{ForkOverride, Input, InputRecord, PoolStart, Start};
use sim_core::measure::{LocusDose, TraitSpread};
use sim_core::params::ParamChange;
use sim_core::time::{SimDuration, SimTime, calendar, year_start};
use sim_core::version::CORE_VERSION;
use sim_core::world::World;

use crate::error::CliError;
use crate::files::{ForkInfo, LogEnd, LogHeader, Manifest, Scenario};
use crate::session::{Compare, Session};

/// Arguments through clap's builder API: its derive macros set their own lint levels
/// and collide with the workspace `forbid` lints (ADR-0000).
fn command() -> Command {
    let run = Command::new("run")
        .about("Runs a scenario, replays an input log or continues a run")
        .arg(Arg::new("scenario").long("scenario").value_name("FILE"))
        .arg(Arg::new("replay").long("replay").value_name("INPUT_NDJSON"))
        .arg(Arg::new("resume").long("resume").action(ArgAction::SetTrue))
        .group(
            ArgGroup::new("source")
                .args(["scenario", "replay", "resume"])
                .required(true),
        )
        .arg(Arg::new("out").long("out").value_name("DIR").required(true))
        .arg(
            Arg::new("until")
                .long("until")
                .value_name("SECONDS")
                .allow_negative_numbers(true)
                .conflicts_with("replay"),
        )
        .arg(
            Arg::new("manifest")
                .long("manifest")
                .value_name("MANIFEST_JSON")
                .requires("replay"),
        )
        .arg(
            Arg::new("input")
                .long("input")
                .value_name("INPUTS_JSON")
                .requires("resume"),
        );
    let fork = Command::new("fork")
        .about("Runs a parent run again from t0 and checks the years before the fork point")
        .arg(Arg::new("parent").value_name("PARENT_DIR").required(true))
        .arg(
            Arg::new("from-year")
                .long("from-year")
                .value_name("YEAR")
                .allow_negative_numbers(true)
                .required(true),
        )
        .arg(
            Arg::new("override")
                .long("override")
                .value_name("OVERRIDE_JSON"),
        )
        .arg(Arg::new("out").long("out").value_name("DIR").required(true));
    let export = Command::new("export")
        .about("Writes the pedigree of a run into its directory (pedigree.json)")
        .arg(Arg::new("run").value_name("RUN_DIR").required(true));
    let kinship = Command::new("kinship")
        .about("Prints the kinship coefficient of two persons of a run (genetics spec 11.2)")
        .arg(Arg::new("run").value_name("RUN_DIR").required(true))
        // Any text reaches the check of the IDs, also one that starts with a hyphen.
        .arg(
            Arg::new("a")
                .value_name("SIM")
                .required(true)
                .allow_hyphen_values(true),
        )
        .arg(
            Arg::new("b")
                .value_name("SIM")
                .required(true)
                .allow_hyphen_values(true),
        );
    let diff = Command::new("diff")
        .about("Prints the comparison of two runs by lineage key (ADR-0021 section 3)")
        .arg(Arg::new("a").value_name("RUN_DIR").required(true))
        .arg(Arg::new("b").value_name("RUN_DIR").required(true));
    let measure = Command::new("measure")
        .about("Prints the genic spread of each trait and the mean dose of each additive locus over the living of a run (ADR-0020 section 3)")
        .arg(Arg::new("run").value_name("RUN_DIR").required(true));
    let view = Command::new("view")
        .about("Serves the runs of a directory and web/dist to the browser, on 127.0.0.1 only (ADR-0021)")
        .arg(Arg::new("runs").value_name("RUNS_DIR").required(true))
        .arg(Arg::new("port").long("port").value_name("PORT"))
        .arg(Arg::new("web").long("web").value_name("WEB_DIST_DIR"));
    Command::new("simagent")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(run)
        .subcommand(fork)
        .subcommand(export)
        .subcommand(kinship)
        .subcommand(diff)
        .subcommand(measure)
        .subcommand(view)
        .subcommand(
            Command::new("repro")
                .about("Replays a repro package; succeeds once the failure is gone (ADR-0016)")
                .arg(Arg::new("file").value_name("REPRO_JSON").required(true)),
        )
}

/// Runs the program with these arguments. A usage error ends the process through clap.
pub fn run_cli(args: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    let matches = command().get_matches_from(args);
    let run = match matches.subcommand() {
        Some(("run", run)) => run,
        Some(("fork", fork)) => {
            let text = |name: &str| fork.get_one::<String>(name).map_or("", String::as_str);
            let year = text("from-year");
            let from_year: i64 = year
                .parse()
                .map_err(|_| CliError::new("usage", format!("--from-year {year} is not a year")))?;
            let override_file = fork.get_one::<String>("override").map(Path::new);
            return fork_run(
                Path::new(text("parent")),
                from_year,
                override_file,
                Path::new(text("out")),
            );
        }
        Some(("repro", repro)) => {
            let file = repro.get_one::<String>("file").map_or("", String::as_str);
            return repro::check(Path::new(file));
        }
        Some(("export", export)) => {
            let dir = export.get_one::<String>("run").map_or("", String::as_str);
            return export_run(Path::new(dir));
        }
        Some(("kinship", kinship)) => {
            let text = |name: &str| kinship.get_one::<String>(name).map_or("", String::as_str);
            return kinship_of(Path::new(text("run")), text("a"), text("b"));
        }
        Some(("diff", diff)) => {
            let dir =
                |name: &str| Path::new(diff.get_one::<String>(name).map_or("", String::as_str));
            return diff_of(dir("a"), dir("b"));
        }
        Some(("view", view)) => {
            let text = |name: &str| view.get_one::<String>(name).map(String::as_str);
            let port = match text("port") {
                Some(port) => port
                    .parse::<u16>()
                    .map_err(|_| CliError::new("usage", format!("--port {port} is not a port")))?,
                None => view::PORT,
            };
            let runs = Path::new(text("runs").unwrap_or_default());
            return view::view(runs, Path::new(text("web").unwrap_or("web/dist")), port);
        }
        Some(("measure", measure)) => {
            let dir = measure.get_one::<String>("run").map_or("", String::as_str);
            return measure_of(Path::new(dir));
        }
        _ => return Err(CliError::new("usage", "unknown subcommand")),
    };
    let text = |name: &str| run.get_one::<String>(name).map(String::as_str);
    let out = Path::new(text("out").unwrap_or_default());
    let until = match text("until") {
        Some(secs) => Some(parse_time(secs).ok_or_else(|| {
            CliError::new("usage", format!("--until {secs} is not a time in seconds"))
        })?),
        None => None,
    };
    if let Some(scenario) = text("scenario") {
        run_scenario(Path::new(scenario), out, until)
    } else if let Some(log) = text("replay") {
        replay(Path::new(log), text("manifest").map(Path::new), out)
    } else {
        resume(out, until, text("input").map(Path::new))
    }
}

/// Writes `pedigree.json` into the run directory, from its save. The pedigree is the
/// core's view of the state; the CLI only writes it.
fn export_run(dir: &Path) -> Result<(), CliError> {
    let _lock = lock::RunLock::acquire(dir)?;
    let text = pedigree_text(&load_world(dir)?)?;
    files::replace(&dir.join(files::PEDIGREE), text.as_bytes())
}

/// The pedigree of a run's world as `simagent export` writes it and `simagent view` sends
/// it: the core's view of the state in its save with each person's names (ADR-0021
/// section 3), one line of JSON.
pub fn pedigree_text(world: &World) -> Result<String, CliError> {
    let pedigree = world.pedigree();
    let lists = names::NameLists::load()?;
    Ok(files::json(&names::export(world, &pedigree, &lists)?) + "\n")
}

/// The world of the run in `dir`, from its save.
pub fn load_world(dir: &Path) -> Result<World, CliError> {
    world_from_save(&files::read(&dir.join(files::SAVE))?)
}

/// The world a save's bytes hold.
pub fn world_from_save(bytes: &[u8]) -> Result<World, CliError> {
    let (world, _) = World::load(bytes, data::core_data()?)
        .map_err(|error| CliError::new("save_rejected", files::code(&error)))?;
    Ok(world)
}

/// The scenario of the sheet of portraits (ADR-0021 G10 and its addendum of 2026-10-24),
/// written by `tools/sheet_reference.py`.
const SHEET: &str = include_str!("../../../data/scenarios/bogen.json");

/// The variant of each pair of the sheet's founders, a woman, then a man, in the scenario's
/// order (Grok's round 37, B8).
const SHEET_VARIANTS: [&str; 5] = ["early", "base", "late", "red", "light_iris"];

/// The sheet of portraits as `simagent view` sends it (ADR-0021 G10): its scenario run in
/// memory to its end, and per founder its variant, its sex and its bodies without
/// environment at the ages of the sheet (Grok's round 37, B7 b).
pub fn sheet_answer() -> Result<serde_json::Value, CliError> {
    let invalid = |detail: &str| CliError::new("scenario_invalid", detail);
    let scenario = resolve(scenario_file(SHEET)?, data::core_data)?;
    if before_the_start(scenario.until) {
        return Err(invalid("the sheet's scenario ends before its start"));
    }
    let founders = scenario.start.founders.clone();
    // Each variant names a woman and a man, in this order: a founder more or less would go
    // without its name.
    if founders.len() != 2 * SHEET_VARIANTS.len() {
        return Err(invalid(
            "the sheet's scenario has not two founders per variant",
        ));
    }
    let mut world = World::new(data::core_data()?);
    world
        .submit(Input::Start(scenario.start))
        .map_err(|rejected| CliError::new("input_rejected", format!("{:?}", rejected.code)))?;
    world.advance_until(
        &mut sim_core::world::YearOut::default(),
        scenario.until,
        &mut |_| false,
    );
    let json = |value: serde_json::Result<serde_json::Value>| {
        value.map_err(|e| CliError::new("io", e.to_string()))
    };
    let mut rows = Vec::new();
    for ((founder, number), variant) in founders
        .iter()
        .zip(1_u64..)
        .zip(SHEET_VARIANTS.iter().flat_map(|variant| [variant, variant]))
    {
        let sim = SimId::try_from(number).map_err(|_| CliError::new("io", "a sim id"))?;
        let view = world
            .neutral_person_view(sim)
            .ok_or_else(|| CliError::new("io", "a founder of the sheet without a view"))?;
        rows.push(serde_json::json!({
            "variant": variant,
            "sex": json(serde_json::to_value(founder.sex))?,
            "at_ages": json(serde_json::to_value(&view.at_ages))?,
        }));
    }
    Ok(serde_json::json!({
        "format": "simagent.view.sheet",
        "version": 1,
        "contract_version": sim_core::body_contract::CONTRACT_VERSION,
        "rows": rows,
    }))
}

/// A run's world as `simagent view` sends it: where the switch of the
/// hereditary diseases stands after its last input (ADR-0023 section 6 and its addendum
/// before S2), the years whose nutrition index is below 1, and the switch a fork from each
/// year keeps (ADR-0021, addendum 2026-10-28; Grok round 40).
pub fn world_answer(world: &World) -> Result<serde_json::Value, CliError> {
    let params = (world.state().world_params())
        .ok_or_else(|| CliError::new("save_rejected", "the world of the save has not started"))?;
    let hunger: Vec<i64> = (params.nutrition.iter())
        .filter(|entry| entry.index.get() < 1.0)
        .map(|entry| entry.year)
        .collect();
    Ok(serde_json::json!({
        "format": "simagent.view.world",
        "version": 2,
        "hereditary_diseases": params.hereditary_diseases,
        "hunger_years": hunger,
        "fork_keeps_diseases": fork_keeps_diseases(world),
    }))
}

/// For each year X from 1 to the year of the clock, the switch of the hereditary diseases a
/// fork from X keeps: the start's, changed by every input `fork_into` takes over (stamped
/// before `fork_stamp(X)`), in the order of the log.
fn fork_keeps_diseases(world: &World) -> Vec<bool> {
    let last_year = calendar(world.closed_through()).year;
    (1..=last_year)
        .map(|year| {
            let stamp = fork_stamp(year);
            let mut on = false;
            for record in world.input_log().iter().filter(|record| record.t < stamp) {
                match &record.input {
                    Input::Start(start) => on = start.world_params.hereditary_diseases,
                    Input::SetWorldParam(set) => {
                        if let ParamChange::HereditaryDiseases(value) = set.change {
                            on = value;
                        }
                    }
                    Input::ForkOverride(block) => {
                        for change in &block.changes {
                            if let ParamChange::HereditaryDiseases(value) = *change {
                                on = value;
                            }
                        }
                    }
                    Input::ArrangePartnership(_) | Input::TriggerShock(_) => {}
                }
            }
            on
        })
        .collect()
}

/// The stamp of a fork from year X: its override acts at `T_X - 1 s`, and it takes over the
/// inputs of its parent stamped before.
fn fork_stamp(from_year: i64) -> SimTime {
    year_start(from_year) - SimDuration::from_secs(1)
}

/// The presentation state of a person of a run's world as `simagent view` sends it
/// (ADR-0021 section 3), with the contract version of its body values, the partners a
/// pairing would be accepted with now (nearest in age first), the ages of the living (Grok's
/// round 33, P3 and P4) and its genetics card (section 5), and with `to` what
/// that sim is to the person (its
/// kinship word, its foster word and `r` in percent, all from the core); `None` for a sim
/// the run does not know.
pub fn person_answer(
    world: &World,
    sim: SimId,
    to: Option<SimId>,
) -> Result<Option<serde_json::Value>, CliError> {
    let Some(view) = world.person_view(sim) else {
        return Ok(None);
    };
    let mut answer = serde_json::json!({
        "format": "simagent.view.person",
        "version": 2,
        "contract_version": sim_core::body_contract::CONTRACT_VERSION,
    });
    let json = |value: serde_json::Result<serde_json::Value>| {
        value.map_err(|e| CliError::new("io", e.to_string()))
    };
    let fields = json(serde_json::to_value(&view))?;
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), fields.as_object()) {
        answer.extend(fields.clone());
    }
    let partners: Vec<u64> = (world.partner_candidates(sim).iter())
        .map(|partner| partner.get())
        .collect();
    answer["partner_candidates"] = partners.into();
    answer["pregnancy"] = match world.pregnancy(sim) {
        Some(pregnancy) => serde_json::json!({
            "conceived": dated(pregnancy.conceived),
            "due": dated(pregnancy.due),
        }),
        None => serde_json::Value::Null,
    };
    answer["ages"] = json(serde_json::to_value(world.ages()))?;
    if let Some(card) = world.genetics_card(sim) {
        answer["card"] = json(serde_json::to_value(card))?;
    }
    if let Some(other) = to {
        let Some(kin) = world.kin_view(sim, other) else {
            return Ok(None);
        };
        let mut kinship = json(serde_json::to_value(kin))?;
        if let Some(kinship) = kinship.as_object_mut() {
            kinship.insert("to".into(), other.get().into());
        }
        answer["kinship"] = kinship;
    }
    Ok(Some(answer))
}

/// A second with its date, as the server's answers carry dates.
fn dated(t: SimTime) -> serde_json::Value {
    serde_json::json!({"t": t.as_secs(), "date": view::date(t)})
}

/// The living persons of a run's world for finding them (the filter of the person list): in
/// the order of the ages the person answer lists, each with its body now and its looks in
/// words, all from the core.
pub fn people_answer(world: &World) -> Result<serde_json::Value, CliError> {
    let json = |value: serde_json::Result<serde_json::Value>| {
        value.map_err(|e| CliError::new("io", e.to_string()))
    };
    let mut people = Vec::new();
    for age in world.ages() {
        let (Some(view), Some(looks)) = (world.person_view(age.sim), world.looks(age.sim)) else {
            return Err(CliError::new(
                "io",
                format!("sim {} has no view", age.sim.get()),
            ));
        };
        people.push(serde_json::json!({
            "sim": age.sim.get(),
            "pregnant": world.pregnancy(age.sim).is_some(),
            "shown": json(serde_json::to_value(&view.shown))?,
            "looks": json(serde_json::to_value(looks))?,
        }));
    }
    Ok(serde_json::json!({
        "format": "simagent.view.people",
        "version": 1,
        "contract_version": sim_core::body_contract::CONTRACT_VERSION,
        "people": people,
    }))
}

/// Prints φ of the persons `a` and `b` of the run in `dir`, from its save, as one JSON
/// number. The core computes it; the CLI only prints it.
fn kinship_of(dir: &Path, a: &str, b: &str) -> Result<(), CliError> {
    let bytes = files::read(&dir.join(files::SAVE))?;
    let (world, _) = World::load(&bytes, data::core_data()?)
        .map_err(|error| CliError::new("save_rejected", files::code(&error)))?;
    let unknown = |text: &str| CliError::new("unknown_sim", format!("no person {text} in the run"));
    let sim = |text: &str| {
        text.parse::<u64>()
            .ok()
            .and_then(|n| SimId::try_from(n).ok())
            .ok_or_else(|| unknown(text))
    };
    let phi = world
        .state()
        .kinship(sim(a)?, sim(b)?)
        .ok_or_else(|| unknown(&format!("{a} or {b}")))?;
    println!("{}", files::json(&phi));
    Ok(())
}

/// Prints the comparison of the runs in `a` and `b` at the earlier of their seconds
/// (ADR-0021 section 3 and its addenda of 2026-10-23 and 2026-10-29): one line of JSON.
fn diff_of(a: &Path, b: &Path) -> Result<(), CliError> {
    let compared = compared_worlds(a, b)?;
    println!("{}", files::json(&diff_of_worlds(&compared.a, &compared.b)));
    Ok(())
}

/// Which of the two runs of a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// Two runs as the comparison takes them (ADR-0021, addendum of 2026-10-29): both at `at`,
/// the earlier of their seconds, and the one rebuilt to it, if any.
pub struct Compared {
    pub a: World,
    pub b: World,
    pub at: SimTime,
    pub recomputed: Option<Side>,
}

/// The runs in `a` and `b` from their saves; the one at the later second is rebuilt in
/// memory up to the earlier (Grok's round 42, D1). Runs at the same second stay as saved.
pub fn compared_worlds(a: &Path, b: &Path) -> Result<Compared, CliError> {
    let (mut one, mut other) = (load_world(a)?, load_world(b)?);
    let at = one.closed_through().min(other.closed_through());
    let recomputed = if one.closed_through() > at {
        one = world_at(&one, at)?;
        Some(Side::A)
    } else if other.closed_through() > at {
        other = world_at(&other, at)?;
        Some(Side::B)
    } else {
        None
    };
    Ok(Compared {
        a: one,
        b: other,
        at,
        recomputed,
    })
}

/// `world` rebuilt in memory up to `t` from its own input log, the one its save carries and
/// its load checked (ADR-0001): the start, each input stamped at or before `t` at its
/// stamp, then on to `t`. Nothing is written.
fn world_at(world: &World, t: SimTime) -> Result<World, CliError> {
    let inconsistent = |detail: String| CliError::new("run_dir_inconsistent", detail);
    let records = world.input_log();
    if !matches!(
        records.first(),
        Some(InputRecord {
            input: Input::Start(_),
            ..
        })
    ) {
        return Err(inconsistent(
            "the input log of the save has no start".to_owned(),
        ));
    }
    let mut rebuilt = World::new(data::core_data()?);
    let mut out = sim_core::world::YearOut::default();
    for record in records.iter().take_while(|record| record.t <= t) {
        if record.t > rebuilt.closed_through() {
            rebuilt.advance_until(&mut out, record.t, &mut |_| false);
        }
        let seq = record.seq;
        let stamp = rebuilt.submit(record.input.clone()).map_err(|rejected| {
            let code = files::code(&rejected.code);
            inconsistent(format!("the input {} is rejected: {code}", seq.get()))
        })?;
        if stamp.t != record.t || stamp.seq != seq {
            let detail = format!("the input {} is stamped otherwise", seq.get());
            return Err(inconsistent(detail));
        }
    }
    if t > rebuilt.closed_through() {
        rebuilt.advance_until(&mut out, t, &mut |_| false);
    }
    Ok(rebuilt)
}

/// The comparison of two runs (ADR-0021, addenda of 2026-10-28 and 2026-10-29): their
/// pedigrees at their clocks, which [`compared_worlds`] makes the same second, with the
/// diseases of the data; `simagent diff` and the server's route both take it.
pub fn diff_of_worlds(a: &World, b: &World) -> sim_core::diff::Diff {
    let diseases: Vec<sim_core::data_id::Code> = (a.data().diseases().iter())
        .map(|disease| disease.code.clone().into())
        .collect();
    sim_core::diff::diff(
        &a.pedigree(),
        &b.pedigree(),
        (a.closed_through(), b.closed_through()),
        &diseases,
    )
}

/// The line `simagent measure` prints: the form, the last closed second and the
/// measurement of the core.
#[derive(Serialize)]
struct MeasureLine<'a> {
    format_version: u32,
    clock: SimTime,
    living: u64,
    traits: &'a [TraitSpread],
    loci: &'a [LocusDose],
}

/// Prints the measurement of the living of the run in `dir` (ADR-0020 section 3), from its
/// save, as one line of JSON. The core measures; the CLI only prints.
fn measure_of(dir: &Path) -> Result<(), CliError> {
    let bytes = files::read(&dir.join(files::SAVE))?;
    let (world, _) = World::load(&bytes, data::core_data()?)
        .map_err(|error| CliError::new("save_rejected", files::code(&error)))?;
    let measurement = world
        .state()
        .measure()
        .ok_or_else(|| CliError::new("no_living", "nobody lives in the run"))?;
    let line = MeasureLine {
        format_version: 1,
        clock: world.closed_through(),
        living: measurement.living,
        traits: &measurement.traits,
        loci: &measurement.loci,
    };
    println!("{}", files::json(&line));
    Ok(())
}

fn parse_time(text: &str) -> Option<SimTime> {
    SimTime::try_from(text.parse::<i64>().ok()?).ok()
}

fn before_the_start(until: SimTime) -> bool {
    until.as_secs() < SimTime::START.as_secs() - 1
}

/// A scenario file read and resolved: its pool drawn from the data's (`PoolStart::drawn`),
/// the founders and couples as they stand. Format 4 is no longer read: under format 5 the
/// same founders mean something else.
fn read_scenario(file: &Path) -> Result<Scenario, CliError> {
    resolve(scenario_file(&files::read_text(file)?)?, data::core_data)
}

/// The scenario file in `text`, of format version 5, not yet resolved.
pub fn scenario_file(text: &str) -> Result<files::ScenarioFile, CliError> {
    let invalid = |detail: &str| CliError::new("scenario_invalid", detail);
    let version = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("format_version")
                .and_then(serde_json::Value::as_u64)
        });
    if version == Some(4) {
        return Err(invalid(
            "a scenario of format version 4 is no longer read (format 5 draws the alleles founders do not name)",
        ));
    }
    files::parse_strict(text)
        .filter(|scenario: &files::ScenarioFile| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| invalid("not a scenario of format version 5"))
}

/// A scenario file resolved on the data `data` gives, which it asks for only when the file
/// names a pool: the pool drawn by `PoolStart::drawn`, every other field as written.
pub fn resolve(
    file: files::ScenarioFile,
    data: impl FnOnce() -> Result<Data, CliError>,
) -> Result<Scenario, CliError> {
    let invalid = |detail: &str| CliError::new("scenario_invalid", detail);
    let start = file.start;
    let pool = match start.pool {
        None => None,
        Some(choice) => Some(
            PoolStart::drawn(
                &data()?,
                start.world_seed,
                choice.n_start,
                choice.immigration,
            )
            .ok_or_else(|| invalid("the data hold no pool"))?,
        ),
    };
    Ok(Scenario {
        format_version: files::FORMAT_VERSION,
        start: Start {
            world_seed: start.world_seed,
            founder_tag: start.founder_tag,
            world_params: start.world_params,
            founders: start.founders,
            couples: start.couples,
            capacity: start.capacity,
            pool,
        },
        until: file.until,
    })
}

fn run_scenario(file: &Path, out: &Path, until: Option<SimTime>) -> Result<(), CliError> {
    let _lock = lock::RunLock::acquire(out)?;
    let scenario = read_scenario(file)?;
    let mut session = begin(out, &scenario, until.unwrap_or(scenario.until))?;
    session.advance(until.unwrap_or(scenario.until))?;
    session.finish(&scenario)
}

/// A new run of `scenario` in `out`, standing at the second before the start: its limits
/// (the scenario's and `limit`) are checked before anything is written. The caller holds
/// the lock of `out`.
pub fn begin(out: &Path, scenario: &Scenario, limit: SimTime) -> Result<Session, CliError> {
    if before_the_start(scenario.until) {
        let detail = format!("until {} lies before the start", scenario.until.as_secs());
        return Err(CliError::new("scenario_invalid", detail));
    }
    let before_start = SimTime::START.as_secs() - 1;
    let stamp =
        SimTime::try_from(before_start).unwrap_or_else(|_| panic!("invariant cli.start_stamp"));
    session::check_limit(stamp, scenario.until)?;
    session::check_limit(stamp, limit)?;
    let first = Input::Start(scenario.start.clone());
    let rejected = |code: String| CliError::new("input_rejected", code);
    Session::start(out, stamp, first, Compare::Nothing, rejected)
}

/// Continues a run from its save. The inputs of `inputs` (a JSON list in the command
/// schema, ADR-0004 section 4) act at the stop, before the clock moves on.
fn resume(dir: &Path, until: Option<SimTime>, inputs: Option<&Path>) -> Result<(), CliError> {
    let _lock = lock::RunLock::acquire(dir)?;
    let scenario = resumed_scenario(dir)?;
    let inputs: Vec<Input> = match inputs {
        Some(file) => files::parse_strict(&files::read_text(file)?).ok_or_else(|| {
            CliError::new(
                "input_invalid",
                "not a list of inputs in the command schema",
            )
        })?,
        None => Vec::new(),
    };
    let session = Session::resume(dir)?;
    let limit = until.unwrap_or(scenario.until);
    session::check_limit(session.closed_through(), limit)?;
    // Nothing has been written up to here: a refused resume, or a rejected input, leaves
    // the run as it was.
    let mut session = session.submit_at_stop(inputs)?;
    session.advance(limit)?;
    session.finish(&scenario)
}

/// The resolved scenario a run directory goes on with.
pub fn resumed_scenario(dir: &Path) -> Result<Scenario, CliError> {
    files::parse_strict(&files::read_text(&dir.join(files::SCENARIO))?)
        .filter(|scenario: &Scenario| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| CliError::new("scenario_invalid", "not a scenario of format version 5"))
}

/// Whose log is read, for the messages about a foreign version.
struct Source {
    subject: &'static str,
    foreign_core_version: &'static str,
    foreign_data_hash: &'static str,
}

const REPLAYED_LOG: Source = Source {
    subject: "the input log",
    foreign_core_version: "replay_foreign_core_version",
    foreign_data_hash: "replay_foreign_data_hash",
};

const PARENT_RUN: Source = Source {
    subject: "the parent run",
    foreign_core_version: "fork_foreign_core_version",
    foreign_data_hash: "fork_foreign_data_hash",
};

/// The records and the end line of an input log of this core version and data hash.
/// The whole log is read and checked before anything runs.
fn read_input_log(log: &Path, source: &Source) -> Result<(Vec<InputRecord>, LogEnd), CliError> {
    let invalid = |detail: String| CliError::new("input_log_invalid", detail);
    let text = files::read_text(log)?;
    let mut lines: Vec<&str> = text.lines().collect();
    let header: LogHeader = lines
        .first()
        .and_then(|line| files::parse_strict(line))
        .ok_or_else(|| invalid("line 1 is not the header of an input log".to_owned()))?;
    if header.format_version != files::FORMAT_VERSION {
        return Err(invalid(
            "line 1 is not the header of an input log".to_owned(),
        ));
    }
    let subject = source.subject;
    if header.core_version != CORE_VERSION {
        let detail = format!(
            "{subject} is of core version {}, this is {CORE_VERSION}",
            header.core_version
        );
        return Err(CliError::new(source.foreign_core_version, detail));
    }
    let data_hash = data::core_data()?.hash();
    if header.data_hash != data_hash {
        let detail = format!(
            "{subject} has data hash {:016x}, this is {data_hash:016x}",
            header.data_hash
        );
        return Err(CliError::new(source.foreign_data_hash, detail));
    }
    let end: LogEnd = lines
        .pop()
        .filter(|_| !lines.is_empty())
        .and_then(files::parse_strict)
        .ok_or_else(|| invalid("the last line is not {\"end\":t}".to_owned()))?;
    let mut records = Vec::new();
    for (line, number) in lines[1..].iter().zip(2..) {
        let record: InputRecord = files::parse_strict(line)
            .ok_or_else(|| invalid(format!("line {number} is not a record of the input log")))?;
        records.push(record);
    }
    let last = records.last().map_or(SimTime::START, |record| record.t);
    if end.end < last {
        return Err(invalid(format!(
            "the end {} lies before the last input at {}",
            end.end.as_secs(),
            last.as_secs()
        )));
    }
    Ok((records, end))
}

fn read_manifest(path: &Path) -> Result<Manifest, CliError> {
    files::read_manifest(&files::read_text(path)?)
}

/// ADR-0004 section 4: a new world, per record `advance_until(t)` and `submit`, at the
/// end `advance_until(end)`.
fn replay(log: &Path, manifest: Option<&Path>, out: &Path) -> Result<(), CliError> {
    let _lock = lock::RunLock::acquire(out)?;
    let invalid = |detail: String| CliError::new("input_log_invalid", detail);
    let (records, end) = read_input_log(log, &REPLAYED_LOG)?;
    let compare = match manifest {
        Some(path) => Compare::Replay(read_manifest(path)?),
        None => Compare::Nothing,
    };
    let mut records = records.into_iter();
    let Some(first) = records.next() else {
        return Err(invalid("the input log has no input".to_owned()));
    };
    let Input::Start(start) = &first.input else {
        return Err(invalid("the first input is not a start".to_owned()));
    };
    let scenario = Scenario {
        format_version: files::FORMAT_VERSION,
        start: start.clone(),
        until: end.end,
    };
    session::check_limit(first.t, end.end)?;
    let (t, seq) = (first.t, first.seq.get());
    let rejected = |code: String| session::replay_rejected(t, seq, &code);
    let mut session = Session::start(out, t, first.input.clone(), compare, rejected)?;
    session.check_stamp(t, seq)?;
    for record in records {
        if record.t > session.closed_through() {
            session.advance(record.t)?;
        }
        session.replay_input(record)?;
    }
    session.advance(end.end)?;
    session.finish(&scenario)
}

/// `simagent fork`: the override block of the file, a list of world parameters with their
/// new values (an empty list is no override and gives no input), then [`fork_with`].
fn fork_run(
    parent: &Path,
    from_year: i64,
    override_file: Option<&Path>,
    out: &Path,
) -> Result<(), CliError> {
    let changes: Vec<ParamChange> = match override_file {
        Some(file) => files::parse_strict(files::read_text(file)?.trim_end()).ok_or_else(|| {
            CliError::new(
                "override_invalid",
                "not a list of world parameters with their new values",
            )
        })?,
        None => Vec::new(),
    };
    fork_with(parent, from_year, changes, out)
}

/// [`fork_into`] under the lock of `out`: the fork writes its own directory, which may not
/// be in use.
pub fn fork_with(
    parent: &Path,
    from_year: i64,
    changes: Vec<ParamChange>,
    out: &Path,
) -> Result<(), CliError> {
    let _lock = lock::RunLock::acquire(out)?;
    fork_into(parent, from_year, changes, out)
}

/// ADR-0001 section 6: a fork is a new run from t0 with the resolved scenario and the
/// seed of the parent. The inputs of the parent with a stamp before `T_X - 1 s` are
/// taken over; the year hashes 0 to X - 1 must equal those in the manifest of the
/// parent, and so must the events before the fork point; the run goes on to the end of
/// the parent. The override `changes` is submitted at `T_X - 1 s`. Without it and without
/// dropped inputs the fork is exactly the parent. The caller holds the lock of `out`.
pub fn fork_into(
    parent: &Path,
    from_year: i64,
    changes: Vec<ParamChange>,
    out: &Path,
) -> Result<(), CliError> {
    // The fork reads its parent: it may not be in use.
    let _parent = lock::RunLock::acquire(parent)?;
    // What the core would reject at the fork point is refused before anything is written.
    let block = ForkOverride { changes };
    if let Some(code) = block.rejection().filter(|_| !block.changes.is_empty()) {
        return Err(CliError::new("override_rejected", files::code(&code)));
    }
    let changes = block.changes;
    let (records, end) = read_input_log(&parent.join(files::INPUT_LOG), &PARENT_RUN)?;
    let manifest = read_manifest(&parent.join(files::MANIFEST))?;
    let scenario: Scenario = files::parse_strict(&files::read_text(&parent.join(files::SCENARIO))?)
        .filter(|scenario: &Scenario| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| CliError::new("scenario_invalid", "not a scenario of format version 5"))?;
    let invalid = |detail: &str| CliError::new("input_log_invalid", detail);
    let first = records
        .first()
        .ok_or_else(|| invalid("the input log has no input"))?;
    let Input::Start(start) = &first.input else {
        return Err(invalid("the first input is not a start"));
    };
    if *start != scenario.start {
        return Err(CliError::new(
            "run_dir_inconsistent",
            "the resolved scenario of the parent does not start as its input log",
        ));
    }
    // The fork names its parent by this manifest, so the manifest must describe the run
    // it lies in.
    if manifest.closed_through != end.end {
        let detail = format!(
            "the manifest of the parent ends at {}, its input log at {}",
            manifest.closed_through.as_secs(),
            end.end.as_secs()
        );
        return Err(CliError::new("manifest_invalid", detail));
    }
    let describes_the_run = manifest.core_version == CORE_VERSION
        && manifest.data_hash == data::core_data()?.hash()
        && manifest.world_seed == scenario.start.world_seed;
    if !describes_the_run {
        return Err(CliError::new(
            "manifest_invalid",
            "the manifest of the parent names another core version, data hash or world seed than its run",
        ));
    }
    let log_hash = session::log_hash_of_events(parent)?;
    if manifest.log_hash != log_hash {
        let detail = format!(
            "the manifest of the parent has log hash {:016x}, its events.ndjson gives {log_hash:016x}",
            manifest.log_hash
        );
        return Err(CliError::new("manifest_invalid", detail));
    }
    let out_of_range = |detail: String| {
        CliError::new(
            "fork_point_out_of_range",
            format!("from-year {from_year}: {detail}"),
        )
    };
    if from_year < 1 {
        return Err(out_of_range("a fork starts at year 1 or later".to_owned()));
    }
    // Year X starts at the end of the parent at the latest: `T_X` is a second the
    // parent has run.
    let last_year = calendar(end.end).year;
    let years = u32::try_from(from_year)
        .ok()
        .filter(|_| from_year <= last_year);
    let Some(years) = years else {
        return Err(out_of_range(format!(
            "the parent run ends at {}, before year {from_year} starts",
            end.end.as_secs()
        )));
    };
    let hashes = manifest.year_hashes.len();
    if u64::try_from(hashes).is_ok_and(|hashes| hashes < u64::from(years)) {
        let detail = format!(
            "the manifest of the parent has {hashes} year hashes, the fork point needs {years}"
        );
        return Err(CliError::new("manifest_invalid", detail));
    }
    let fork_stamp = fork_stamp(from_year);
    let (kept, dropped): (Vec<InputRecord>, Vec<InputRecord>) = records
        .into_iter()
        .partition(|record| record.t < fork_stamp);
    let mut kept = kept.into_iter();
    // A start is stamped before t0 and so before every fork point.
    let first = kept.next().ok_or_else(|| {
        CliError::new(
            "input_log_invalid",
            "the input log has no input before the fork point",
        )
    })?;
    session::check_limit(first.t, end.end)?;
    let info = ForkInfo {
        parent_log_hash: manifest.log_hash,
        from_year: years,
        dropped_inputs: u64::try_from(dropped.len())
            .unwrap_or_else(|_| panic!("invariant cli.count_range")),
    };
    // From the fork point on a fork may differ from its parent: by its override, or by
    // an input of the parent that it drops.
    let fork_point = year_start(from_year);
    let compare = Compare::Fork {
        parent: manifest,
        info,
        fork_point,
        events_before: session::log_hash_of_events_before(parent, fork_point)?,
        is_its_parent: changes.is_empty() && dropped.is_empty(),
    };
    let (t, seq) = (first.t, first.seq.get());
    let rejected = |code: String| session::replay_rejected(t, seq, &code);
    let mut session = Session::start(out, t, first.input.clone(), compare, rejected)?;
    session.check_stamp(t, seq)?;
    for record in kept {
        if record.t > session.closed_through() {
            session.advance(record.t)?;
        }
        session.replay_input(record)?;
    }
    // The session compares year by year up to the fork point. The override is stamped
    // `T_X - 1 s`: it acts before the first event of year X.
    if !changes.is_empty() {
        session.advance(fork_stamp)?;
        let rejected = |code: String| CliError::new("override_rejected", code);
        session.submit(Input::ForkOverride(ForkOverride { changes }), rejected)?;
    }
    session.advance(end.end)?;
    session.finish(&scenario)
}
