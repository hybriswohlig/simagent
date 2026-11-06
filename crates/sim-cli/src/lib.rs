//! `simagent`: the command line runner. It loads scenarios, drives the core through
//! its command schema and writes the run directory. It never decides gameplay
//! (ADR-0000); every number in its files comes from the core.

pub mod data;
pub mod error;
pub mod files;
pub mod house;
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
    world_from_save_with(bytes, &data::core_data()?)
}

/// [`world_from_save`] on `data`.
pub fn world_from_save_with(bytes: &[u8], data: &Data) -> Result<World, CliError> {
    let (world, _) = World::load(bytes, data.clone())
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

/// The households with a house (ADR-0021, addendum of 2026-11-03), in ascending id, each with
/// its house and whether its first morning has come.
pub fn houses_answer(world: &World) -> serde_json::Value {
    let households: Vec<serde_json::Value> = (world.state().house().households.iter())
        .map(|(id, moved)| {
            serde_json::json!({"household": id.get(), "house": moved.house, "morning": moved.morning})
        })
        .collect();
    serde_json::json!({"format": "simagent.view.houses", "version": 1, "households": households})
}

/// A person's reserve at the clock `at` (ADR-0026, addendum of 2026-11-06 on hunger at 0): its
/// value, its capacity and its band, `draws`, `weak`, `wasted` or else `none`, as the core gives
/// them (`Reserve::band`); what an input at the clock wrote down, in the next second, with that
/// value.
fn reserve_json(person: &sim_core::house::state::InHouse, at: SimTime) -> serde_json::Value {
    use sim_core::house::reserve::Band;
    let reserve = person.reserve;
    let hunger = person.needs[0];
    let second = at.max(hunger.at()).max(reserve.at());
    let band = match reserve.band(hunger, second) {
        Some(Band::Draws) => "draws",
        Some(Band::Weak) => "weak",
        Some(Band::Wasted) => "wasted",
        None => "none",
    };
    serde_json::json!({
        "value": reserve.value_at(second),
        "capacity": reserve.capacity(),
        "band": band,
    })
}

/// A household's house as `simagent view` sends it (ADR-0021, addendum of 2026-11-03): the
/// drawing with each object's places and the names of its offers, and each person with needs
/// in ascending SimId with where it is, away at school or work until its return (version 2,
/// ADR-0024 section 4), what it does and until when, and its needs at the clock (a need an
/// input at the clock wrote down in the next second, as a command does, with that value); and
/// each child without needs in ascending SimId where the clock puts it (version 3). Whether it
/// lies on the floor, and whether it smells, hygiene at most 50 (version 4, ADR-0026, addendum
/// of 2026-11-04 on a need at 0): read from the state, the smell from the need it exports.
/// And each person's offers a command may name, by object and then offer (version 5): those
/// the core's check of a command opens in the second it acts, the one after the clock (ADR-0024
/// section 5): at the person's age then, an offer of sleep only in its night, no object whose
/// every place a small child holds then; none at the end of time. Each walk says whether it
/// leads out of the house (version 6, ADR-0026, addendum of 2026-11-04 after the playtest): a
/// walk without an action leads out or to the room one stays in. Each need's comfort, and per
/// person how much each need weighs in a choice now, `max(0, comfort − value)`; an action says
/// whether it is a time-filler, whose end is the one the queue holds (version 7, ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler). An action at its object that today's
/// departure ends (`departure_ends`) ends at that departure and says so; each person names the
/// object whose waiting list it stands on; and each offer a command may name says whether its
/// object has no free place for the person, as the core's check of a command finds it (version
/// 8, ADR-0021, addendum of 2026-11-05 on the house page). Each person's reserve at hunger 0 with
/// its band (version 9, `reserve_json`, ADR-0026, addendum of 2026-11-06 on hunger at 0). `None`
/// for a household without a house.
pub fn house_answer(world: &World, household: u64) -> Result<Option<serde_json::Value>, CliError> {
    use sim_core::house::day::{YoungPlace, departure, night, work_draw, young_place};
    use sim_core::house::need::EMERGENCY;
    use sim_core::house::state::{Place, action_end, dwell_walk};
    let state = world.state();
    let Ok(household) = sim_core::ids::HouseholdId::try_from(household) else {
        return Ok(None);
    };
    let Some(moved) = state.house().households.get(&household) else {
        return Ok(None);
    };
    let broken = |what: &str| CliError::new("save_rejected", format!("the house: {what}"));
    let houses = state
        .data()
        .houses()
        .ok_or_else(|| broken("no house data"))?;
    let house = houses
        .house(&moved.house)
        .ok_or_else(|| broken("an unknown house"))?;
    let layouts = data::layouts()?;
    let layout = (layouts.get(&moved.house))
        .ok_or_else(|| CliError::new("data_rejected", "a house without a drawing"))?;
    let names = data::offer_names()?;
    let objects: Vec<serde_json::Value> = (layout.objects.iter())
        .map(|object| {
            let slots = houses.kind(&object.kind).map_or(0, |kind| kind.slots);
            serde_json::json!({
                "kind": object.kind,
                "room": object.room,
                "x": object.x,
                "y": object.y,
                "w": object.w,
                "h": object.h,
                "slots": slots,
                "offers": names.get(&object.kind).cloned().unwrap_or_default(),
            })
        })
        .collect();
    let at = world.closed_through();
    // The second a command given at the clock acts in (ADR-0004 section 4).
    let acts = at.checked_add(SimDuration::from_secs(1));
    let seed = (state.world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let mut persons = Vec::new();
    for (&sim, person) in &state.house().sims {
        if state.household_of(sim) != Some(household) {
            continue;
        }
        let born = state.born(sim).ok_or_else(|| broken("an unknown person"))?;
        let mut commands = Vec::new();
        if let Some(acts) = acts {
            let age = sim_core::house::choice::age(born, acts)
                .ok_or_else(|| broken("a person before its birth"))?;
            let tonight = night(houses.day(), born, acts).is_some();
            // A bed whose every place a small child holds then is refused as taken by the
            // clock alone (ADR-0026, addendum of 2026-11-04 after the playtest); the beds'
            // tiers order a choice, not a command.
            let held = state.held_by_children(household, acts);
            // The places taken for a command of the person, as the core's check of a command
            // finds them (ADR-0024, addendum of 2026-11-04 on slice 12): the place of what it
            // does frees as the command ends it, and goes to the first who waits for that
            // object (version 8).
            let mut taken = places_taken(state, household, sim, &held);
            if let Some(own) = person.action
                && !state.house().waits.contains_key(&(household, own.object))
            {
                taken.remove(&(own.object, own.slot));
            }
            for (object, def) in (0_u16..).zip(&house.def().objects) {
                let kind = houses
                    .kind(&def.kind)
                    .ok_or_else(|| broken("an unknown kind"))?;
                let all_held = (0..kind.slots).all(|slot| held.contains(&(object, slot)));
                let full = (0..kind.slots).all(|slot| taken.contains(&(object, slot)));
                for (offer, def) in (0_u16..).zip(&kind.offers) {
                    let open =
                        (def.min_age..=def.max_age).contains(&age) && (!def.sleep || tonight);
                    if open && !all_held {
                        commands.push(
                            serde_json::json!({"object": object, "offer": offer, "taken": full}),
                        );
                    }
                }
            }
        }
        // The object whose waiting list the person stands on (version 8).
        let lists = (household, 0)..=(household, u16::MAX);
        let waits = (state.house().waits.range(lists))
            .find(|(_, waiters)| waiters.iter().any(|&(_, waiter)| waiter == sim))
            .map(|(&(_, object), _)| object);
        let place = match person.place {
            Place::Room(room) => serde_json::json!({"room": room}),
            Place::Walking {
                from,
                to,
                depart,
                arrive,
            } => serde_json::json!({"walking": {
                "from": from,
                "to": to,
                "depart": dated(depart),
                "arrive": dated(arrive),
                // Without an action out to the entrance, or to the room one stays in (ADR-0026,
                // addendum of 2026-11-04 after the playtest).
                "out": person.action.is_none() && !dwell_walk(person, house),
            }}),
            Place::Away => {
                let (place, back) = world
                    .away(sim)
                    .ok_or_else(|| broken("away without a return"))?;
                serde_json::json!({"away": {"place": place, "until": dated(back)}})
            }
        };
        let action = match person.action {
            None => serde_json::Value::Null,
            Some(action) => {
                let offer = (house.def().objects.get(usize::from(action.object)))
                    .and_then(|object| houses.kind(&object.kind))
                    .and_then(|kind| kind.offers.get(usize::from(action.offer)))
                    .ok_or_else(|| broken("an unknown offer"))?;
                let (until, departs) = match (person.place, action.filler) {
                    (Place::Room(_), filler) => {
                        let planned = match filler {
                            true => world.action_until(sim),
                            false => action_end(person, offer, houses.day(), born),
                        };
                        // Today's departure ends it first, unless it waits for it (version 8).
                        let key =
                            (state.lineage_key(sim)).ok_or_else(|| broken("an unknown person"))?;
                        let leaves = departure(houses.day(), born, || work_draw(seed, key), at);
                        let departs =
                            departure_ends(houses, offer, (&person.needs, born), (planned, leaves));
                        (departs.or(planned), departs.is_some())
                    }
                    _ => (None, false),
                };
                serde_json::json!({
                    "object": action.object,
                    "offer": action.offer,
                    "slot": action.slot,
                    "until": until.map(dated),
                    "filler": action.filler,
                    "departs": departs,
                })
            }
        };
        // An input at the clock acts in the next second (ADR-0004 section 4): what it wrote
        // down has no value before.
        let needs = person.needs.map(|need| need.value_at(at.max(need.at())));
        let comfort = houses.rates().comfort;
        let weights: Vec<u16> = (comfort.iter().zip(needs))
            .map(|(comfort, value)| comfort.saturating_sub(value))
            .collect();
        persons.push(serde_json::json!({
            "sim": sim.get(),
            "place": place,
            "action": action,
            "waits": waits,
            "needs": needs,
            "weights": weights,
            "collapsed": person.collapsed,
            // Hygiene, the fourth need, at the emergency level or below.
            "smells": needs[3] <= EMERGENCY,
            "commands": commands,
            "reserve": reserve_json(person, at),
        }));
    }
    // The members without needs (ADR-0024 sections 2 and 4): none before the first morning;
    // after it exactly the children whose plan at 5:00 has not found them 6 yet, so a child
    // who turned 6 at noon stands here until its next plan.
    let def = house.def();
    let place = match young_place(houses.day(), at) {
        YoungPlace::Kita { until } => {
            serde_json::json!({"away": {"place": "kita", "until": dated(until)}})
        }
        YoungPlace::Bed => serde_json::json!({"room": def.child_night_room, "asleep": true}),
        YoungPlace::DayRoom => serde_json::json!({"room": def.child_day_room, "asleep": false}),
    };
    let children: Vec<serde_json::Value> = (state.members(household))
        .filter(|sim| moved.morning && !state.house().sims.contains_key(sim))
        .map(|sim| serde_json::json!({"sim": sim.get(), "place": place}))
        .collect();
    Ok(Some(serde_json::json!({
        "format": "simagent.view.house",
        "version": 9,
        "at": dated(at),
        "household": household.get(),
        "house": moved.house,
        "morning": moved.morning,
        "comfort": houses.rates().comfort,
        "layout": {"rooms": layout.rooms, "doors": layout.doors, "objects": objects},
        "persons": persons,
        "children": children,
    })))
}

/// The departure that ends what a person does at its object, offer `offer` with the needs
/// `needs`, born at `born` (version 8 of the house answer, ADR-0021, addendum of 2026-11-05 on the
/// house page): today's departure still to come, `leaves`, if the action would still run then,
/// its end `until` at or after it or beyond the end of time, and the departure does not wait for
/// it (ADR-0024 section 4 and its addendum on slice 8b): the offer raises no need that is
/// critical in that second, as `house::choice::crosses_departure` closes it for a choice made
/// there. `None` otherwise, and for an offer of sleep, which the wake before a departure ends.
fn departure_ends(
    houses: &sim_core::house::data::HouseData,
    offer: &sim_core::house::data::OfferDef,
    (needs, born): (&[sim_core::house::need::NeedSample; 5], SimTime),
    (until, leaves): (Option<SimTime>, Option<SimTime>),
) -> Option<SimTime> {
    use sim_core::house::choice::{age, crosses_departure};
    let leaves = leaves?;
    if until.is_some_and(|until| until < leaves) {
        return None;
    }
    let age = age(born, leaves)?;
    let values = needs.map(|need| need.value_at(leaves.max(need.at())));
    crosses_departure(offer, houses.rates(), (age, values), 0, Some(0)).then_some(leaves)
}

/// The places of the house of `household` taken for `sim` (ADR-0026 section 5): each that an
/// action of a member holds, each held for another than `sim`, and `children`, those small
/// children hold in their beds then.
fn places_taken(
    state: &sim_core::world::WorldState,
    household: sim_core::ids::HouseholdId,
    sim: SimId,
    children: &[(u16, u8)],
) -> std::collections::BTreeSet<(u16, u8)> {
    let mut taken = std::collections::BTreeSet::new();
    for member in state.members(household) {
        if let Some(action) = state.house().sims.get(&member).and_then(|p| p.action) {
            taken.insert((action.object, action.slot));
        }
    }
    for (&(of, object, slot), &holder) in &state.house().holds {
        if of == household && holder != sim {
            taken.insert((object, slot));
        }
    }
    taken.extend(children.iter().copied());
    taken
}

/// "Warum?" for a choice (ADR-0021, addendum of 2026-11-03; ADR-0026 section 6): from an entry
/// `core:action_started` or `core:idle`, the points of every offer recomputed with
/// `house::choice::score` from the house, the room and the needs of the entry and the
/// person's age and night in its second, the most first (a tie to the smaller object, then
/// offer), each with whether its object had no free place; after them each offer of sleep
/// closed then, outside the person's night, without points (version 2, ADR-0026, addendum of
/// 2026-11-04 on the night). With the night the choice lay in, or else the next, and the
/// object an idle person waited for. Each offer with points and the chosen one carry the
/// person's mood for it in that second, in percent, and the points with it; a command is no
/// choice and has no mood, its points are without one and its moods `null`, as for a closed
/// offer (version 3, the addendum of 2026-11-04 on the mood). The needs of the entry, and each
/// offer and the chosen one with the needs critical then that it raises, the urgent part of its
/// points; and for an idle entry that is the person's `latest` choice in the log while the person
/// is still idle in a room of the same house, the same comparison at the clock, `now` (version
/// 4). Each offer says whether a small child held a place of its object, whether it was a bed
/// outside the person's tier in the night and whom its kind is for; an idle choice waiting for
/// nothing in a room one does not stay in names the room the person went to (version 5, ADR-0026,
/// addendum of 2026-11-04 after the playtest). The comfort of each need, and per offer the
/// needs it raises that stand at or above their comfort and so weigh nothing; an offer that would
/// still run at today's departure is closed with that departure; a time-filler, taken when
/// nothing brought enough, with its walk, its end and, when it ended as planned, the needs the
/// next choice raised (version 6, ADR-0026, addendum of 2026-11-04 on comfort and the
/// time-filler): its end from `ended`, the log's entry that ended it with the choice after it in
/// that second, or, while it still runs, the end the queue holds. Why a time-filler ends where it
/// does, its mood and those of the other time-fillers free then; for an idle choice that took no
/// time-filler as it would have ended under `fallback_min_seconds` away, that end and why it lies
/// there; and for an offer the person was commanded to that today's departure closes, that
/// departure instead of its points (version 7, ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length). A time-filler that ends at its next choice as planned names what that
/// choice raised also when the choice waits for a taken object: what the offer it waits for
/// raises (version 8, ADR-0021, addendum of 2026-11-05 on the house page). For the person's latest
/// choice in the log, `latest`, its reserve at the clock as the house answer gives it (version 9,
/// ADR-0026, addendum of 2026-11-06 on hunger at 0), else `null`. `None` for any other entry.
pub fn why_answer(
    world: &World,
    entry: &sim_core::log::LogEntry,
    id: u64,
    latest: bool,
    ended: Option<Ended<'_>>,
) -> Result<Option<serde_json::Value>, CliError> {
    use sim_core::house::choice::ENOUGH;
    use sim_core::house::day::{departure, next_night, night, work_draw};
    use sim_core::log::{LogPayload, Subject};
    let (house, room, needs, busy, chosen, commanded, (waits, too_short), (held, tier)) =
        match &entry.payload {
            LogPayload::ActionStarted {
                house,
                object,
                offer,
                room,
                needs,
                busy,
                commanded,
                held,
                tier,
                ..
            } => (
                house,
                *room,
                *needs,
                busy,
                Some((*object, *offer)),
                *commanded,
                (None, None),
                (held, tier),
            ),
            LogPayload::Idle {
                house,
                room,
                needs,
                busy,
                waits,
                held,
                tier,
                too_short,
            } => (
                house,
                *room,
                *needs,
                busy,
                None,
                false,
                (*waits, *too_short),
                (held, tier),
            ),
            _ => return Ok(None),
        };
    let filled = match &entry.payload {
        LogPayload::ActionStarted {
            filler: true,
            walk_secs,
            ..
        } => Some(*walk_secs),
        _ => None,
    };
    let chosen_offer = chosen;
    let [Subject::Sim(sim)] = entry.subjects.as_slice() else {
        return Ok(None);
    };
    let broken = |what: &str| CliError::new("save_rejected", format!("the choice: {what}"));
    let born = world
        .state()
        .born(*sim)
        .ok_or_else(|| broken("an unknown person"))?;
    let houses = (world.state().data().houses()).ok_or_else(|| broken("no house data"))?;
    let house = houses
        .house(house)
        .ok_or_else(|| broken("an unknown house"))?;
    let age = sim_core::house::choice::age(born, entry.t)
        .ok_or_else(|| broken("a choice before the birth"))?;
    let tonight = night(houses.day(), born, entry.t);
    let seed = (world.state().world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let key = (world.state().lineage_key(*sim)).ok_or_else(|| broken("an unknown person"))?;
    let leaves = departure(houses.day(), born, || work_draw(seed, key), entry.t);
    let choice = ChoiceAt {
        sim: *sim,
        at: entry.t,
        room,
        needs,
        busy,
        commanded,
        held,
        tier,
        leaves,
    };
    let scored = scored_offers(world, (house, houses), &choice)?;
    let chosen = match chosen {
        None => None,
        Some((object, offer)) => {
            let found =
                (scored.iter()).find(|scored| (scored.object, scored.offer) == (object, offer));
            let def = offer_def(house, houses, object, offer)?;
            // An offer today's departure closes has no points: its departure instead (version 7,
            // ADR-0026, addendum of 2026-11-05 on the time-filler's length).
            let walk = house.walk_to(room, object);
            let depart_in = leaves.map(|leaves| (leaves - entry.t).as_secs());
            let closes = !def.sleep
                && sim_core::house::choice::crosses_departure(
                    def,
                    houses.rates(),
                    (age, needs),
                    walk,
                    depart_in,
                );
            Some(serde_json::json!({
                "object": object,
                "offer": offer,
                "points": found.map(|scored| scored.points),
                "mood": found.and_then(|scored| (!commanded).then_some(scored.mood)),
                "departs": leaves.filter(|_| closes).map(dated),
                "urgent": urgent(houses, def, age, needs)?,
            }))
        }
    };
    let mut offers = offer_values(house, houses, &choice, &scored, age)?;
    offers.extend(departure_closed(house, houses, &choice, age)?);
    // Outside the night every offer of sleep open at the age is closed.
    if tonight.is_none() {
        for (object, def) in (0_u16..).zip(&house.def().objects) {
            let kind = houses
                .kind(&def.kind)
                .ok_or_else(|| broken("an unknown kind"))?;
            for (offer, def) in (0_u16..).zip(&kind.offers) {
                if def.sleep && (def.min_age..=def.max_age).contains(&age) {
                    offers.push(serde_json::json!({
                        "object": object,
                        "offer": offer,
                        "points": null,
                        "mood": null,
                        "busy": busy.contains(&object),
                        "closed": true,
                        "departs": null,
                        "urgent": urgent(houses, def, age, needs)?,
                        "sated": sated(houses, def, age, needs)?,
                        "held": held.contains(&object),
                        "outside": false,
                        "bed_for": kind.bed_for,
                    }));
                }
            }
        }
    }
    let shown = tonight
        .map(|night| (night, true))
        .or_else(|| next_night(houses.day(), born, entry.t).map(|night| (night, false)));
    let night = shown.map(|(night, now)| {
        let until = night.until.map(dated);
        serde_json::json!({"from": dated(night.from), "until": until, "now": now})
    });
    let idle = matches!(entry.payload, LogPayload::Idle { .. });
    let now = if idle && latest {
        compared_now(world, *sim, (house, houses))?
    } else {
        None
    };
    // The person's latest choice tells its reserve at the clock (version 9).
    let reserve = (world.state().house().sims.get(sim))
        .filter(|_| latest)
        .map(|person| reserve_json(person, world.closed_through()));
    // Idle and waiting for nothing in a room one does not stay in, the person went to the room
    // one does (ADR-0026, addendum of 2026-11-04 after the playtest).
    let goes_to =
        (idle && waits.is_none() && !house.dwells(room)).then_some(house.def().dwell_room);
    let fill = FillerAt {
        sim: *sim,
        at: entry.t,
        age,
        room,
        busy,
    };
    let filler = match (filled, chosen_offer) {
        (Some(walk_secs), Some(offered)) => Some(filler_values(
            world,
            (house, houses),
            &fill,
            (offered, walk_secs),
            ended,
        )?),
        _ => None,
    };
    let too_short = match too_short {
        Some(until) => Some(short_values(world, (house, houses), &fill, until)?),
        None => None,
    };
    Ok(Some(serde_json::json!({
        "format": "simagent.view.why",
        "version": 9,
        "entry": id,
        "sim": sim.get(),
        "at": dated(entry.t),
        "enough": ENOUGH,
        "commanded": commanded,
        "needs": needs,
        "comfort": houses.rates().comfort,
        "night": night,
        "chosen": chosen,
        "waits": waits,
        "goes_to": goes_to,
        "filler": filler,
        "too_short": too_short,
        "offers": offers,
        "now": now,
        "reserve": reserve,
    })))
}

/// How a time-filler ended in the domain log (version 6 of "Warum?"): the entry
/// `core:action_ended` that ended it, and the choice its person made in that second after it.
#[derive(Clone, Copy, Debug)]
pub struct Ended<'a> {
    pub ended: &'a sim_core::log::LogEntry,
    pub next: Option<&'a sim_core::log::LogEntry>,
}

/// The choice a time-filler of "Warum?" comes from: who, in which second, at which age, from
/// which room, with which objects without a free place.
struct FillerAt<'a> {
    sim: SimId,
    at: SimTime,
    age: u8,
    room: u8,
    busy: &'a [u16],
}

/// The time-fillers free in `fill` with the person's moods in its second, the highest first
/// (`house::choice::fillers`, ADR-0026, addendum of 2026-11-05 on the time-filler's length).
fn free_fillers(
    world: &World,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
    fill: &FillerAt<'_>,
) -> Result<Vec<sim_core::house::choice::Filler>, CliError> {
    use sim_core::house::choice::{fillers, mood};
    let broken = |what: &str| CliError::new("save_rejected", format!("the choice: {what}"));
    let state = world.state();
    let seed = (state.world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let key = (state.lineage_key(fill.sim)).ok_or_else(|| broken("an unknown person"))?;
    let percent = houses.day().choice_mood_percent;
    let moods = |object, offer| mood(seed, key, fill.at, (object, offer), percent);
    Ok(fillers(
        (house, houses),
        (fill.age, fill.room),
        fill.busy,
        moods,
    ))
}

/// Why a time-filler of `fill`, `offered` and `walk_secs` away, ends at `end` (version 7 of
/// "Warum?", ADR-0026, addendum of 2026-11-05 on the time-filler's length), by the bounds its end
/// has (`house::state::filler_end_of`): "departure" at today's departure still to come at the
/// choice, "bedtime" at the bedtime after its arrival (at its arrival if that lies in a night),
/// "time" at its length or `fallback_max_seconds` after its arrival, else "choice", the next
/// worthwhile choice. A time-filler's length is its offer's `seconds`, which the data require of
/// it (`house.fallback_length`), so it never ends as full.
fn filler_ends(
    world: &World,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
    fill: &FillerAt<'_>,
    ((object, offer), walk_secs): ((u16, u16), u32),
    end: SimTime,
) -> Result<&'static str, CliError> {
    use sim_core::house::day::{departure, next_night, night, work_draw};
    let broken = |what: &str| CliError::new("save_rejected", format!("the choice: {what}"));
    let state = world.state();
    let born = state
        .born(fill.sim)
        .ok_or_else(|| broken("an unknown person"))?;
    let seed = (state.world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let key = (state.lineage_key(fill.sim)).ok_or_else(|| broken("an unknown person"))?;
    let rules = houses.day();
    if departure(rules, born, || work_draw(seed, key), fill.at) == Some(end) {
        return Ok("departure");
    }
    let after = |secs: i64| fill.at.checked_add(SimDuration::from_secs(secs));
    let arrive = after(i64::from(walk_secs)).ok_or_else(|| broken("beyond the end of time"))?;
    let bedtime = match night(rules, born, arrive) {
        Some(_) => Some(arrive),
        None => next_night(rules, born, arrive).map(|night| night.from),
    };
    if bedtime == Some(end) {
        return Ok("bedtime");
    }
    let def = offer_def(house, houses, object, offer)?;
    let length =
        (def.seconds).and_then(|secs| arrive.checked_add(SimDuration::from_secs(secs.into())));
    let longest = SimDuration::from_secs(rules.fallback_max_seconds.into());
    if length == Some(end) || arrive.checked_add(longest) == Some(end) {
        return Ok("time");
    }
    Ok("choice")
}

/// A time-filler as "Warum?" tells it (version 6, ADR-0026, addendum of 2026-11-04 on comfort
/// and the time-filler; version 7, its addendum of 2026-11-05 on the time-filler's length): its
/// walk; its end, from the log's entry that ended it, else the end the queue holds while the
/// person does it at its object (none on its way there); why it ends there (`filler_ends`; a
/// departure that ended it, none for an end of another reason); if it ended at the next choice
/// as planned (reason `done`) and that choice took an offer, or waited for one (version 8,
/// `waited_offer`), the needs that offer raises at the person's age, by their codes; its mood,
/// and the other time-fillers free then with theirs, in the order of the choice.
fn filler_values(
    world: &World,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
    fill: &FillerAt<'_>,
    (offered, walk_secs): ((u16, u16), u32),
    ended: Option<Ended<'_>>,
) -> Result<serde_json::Value, CliError> {
    use sim_core::house::data::NEEDS;
    use sim_core::log::{EndReason, LogPayload};
    let until = match ended {
        Some(ended) => Some(ended.ended.t),
        None => world.action_until(fill.sim),
    };
    let reason = ended.and_then(|ended| match ended.ended.payload {
        LogPayload::ActionEnded { reason, .. } => Some(reason),
        _ => None,
    });
    let ends = match (until, reason) {
        (_, Some(EndReason::Obligation)) => Some("departure"),
        (Some(until), None | Some(EndReason::Done)) => Some(filler_ends(
            world,
            (house, houses),
            fill,
            (offered, walk_secs),
            until,
        )?),
        _ => None,
    };
    let planned = ended.filter(|_| reason == Some(EndReason::Done) && ends == Some("choice"));
    // The offer the next choice took, or the one it waits for (version 8, ADR-0021, addendum of
    // 2026-11-05 on the house page).
    let raised = match planned.and_then(|ended| ended.next) {
        Some(next) => match &next.payload {
            LogPayload::ActionStarted {
                house,
                object,
                offer,
                commanded: false,
                filler: false,
                ..
            } => Some((house, (*object, *offer))),
            LogPayload::Idle { house, .. } => {
                waited_offer(world, houses, next)?.map(|waited| (house, waited))
            }
            _ => None,
        },
        None => None,
    };
    let mut then = Vec::new();
    if let Some((next_house, (object, offer))) = raised {
        let broken = || CliError::new("save_rejected", "the choice: an unknown house");
        let next_house = houses.house(next_house).ok_or_else(broken)?;
        let def = offer_def(next_house, houses, object, offer)?;
        for need in NEEDS {
            if def.raises(houses.rates(), need, fill.age) {
                then.push(
                    serde_json::to_value(need).map_err(|e| CliError::new("io", e.to_string()))?,
                );
            }
        }
    }
    let free = free_fillers(world, (house, houses), fill)?;
    let mood = (free.iter())
        .find(|each| (each.object, each.offer) == offered)
        .map(|each| each.mood);
    let others: Vec<serde_json::Value> = (free.iter())
        .filter(|each| (each.object, each.offer) != offered)
        .map(|each| serde_json::json!({"object": each.object, "offer": each.offer, "mood": each.mood}))
        .collect();
    Ok(serde_json::json!({
        "walk_secs": walk_secs,
        "until": until.map(dated),
        "ends": ends,
        "then": then,
        "mood": mood,
        "others": others,
    }))
}

/// The offer an idle choice `entry` that waits for a taken object waits for (version 8 of
/// "Warum?", ADR-0021, addendum of 2026-11-05 on the house page): of that object the one with the
/// most points, recomputed as "Warum?" recomputes the entry (`scored_offers`), as
/// `house::choice::choose` waits for the best of the objects without a free place
/// (`house::choice::best`). `None` for an entry that waits for nothing.
fn waited_offer(
    world: &World,
    houses: &sim_core::house::data::HouseData,
    entry: &sim_core::log::LogEntry,
) -> Result<Option<(u16, u16)>, CliError> {
    use sim_core::house::choice::best;
    use sim_core::house::day::{departure, work_draw};
    use sim_core::log::{LogPayload, Subject};
    let LogPayload::Idle {
        house,
        room,
        needs,
        busy,
        waits: Some(waited),
        held,
        tier,
        ..
    } = &entry.payload
    else {
        return Ok(None);
    };
    let [Subject::Sim(sim)] = entry.subjects.as_slice() else {
        return Ok(None);
    };
    let broken = |what: &str| CliError::new("save_rejected", format!("the choice: {what}"));
    let state = world.state();
    let house = houses
        .house(house)
        .ok_or_else(|| broken("an unknown house"))?;
    let born = state
        .born(*sim)
        .ok_or_else(|| broken("an unknown person"))?;
    let seed = (state.world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let key = (state.lineage_key(*sim)).ok_or_else(|| broken("an unknown person"))?;
    let choice = ChoiceAt {
        sim: *sim,
        at: entry.t,
        room: *room,
        needs: *needs,
        busy,
        commanded: false,
        held,
        tier,
        leaves: departure(houses.day(), born, || work_draw(seed, key), entry.t),
    };
    let scored = scored_offers(world, (house, houses), &choice)?;
    let of_it: Vec<sim_core::house::choice::Scored> = (scored.into_iter())
        .filter(|scored| scored.object == *waited)
        .collect();
    Ok(best(&of_it, &[]).map(|waited| (waited.object, waited.offer)))
}

/// A time-filler not taken because it would have ended at `until`, under `fallback_min_seconds`
/// after the choice of `fill` (version 7 of "Warum?", ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length): that end and why it lies there (`filler_ends`), for the time-filler the
/// choice would have taken, the first of those free then.
fn short_values(
    world: &World,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
    fill: &FillerAt<'_>,
    until: SimTime,
) -> Result<serde_json::Value, CliError> {
    let free = free_fillers(world, (house, houses), fill)?;
    let first = (free.first())
        .ok_or_else(|| CliError::new("save_rejected", "the choice: no time-filler was free"))?;
    let walked = ((first.object, first.offer), first.walk_secs);
    let ends = filler_ends(world, (house, houses), fill, walked, until)?;
    Ok(serde_json::json!({"until": dated(until), "ends": ends}))
}

/// The offers of `choice` open at `age` that would still run at today's departure, closed
/// without points or mood, each with that departure (version 6 of "Warum?", ADR-0026, addendum
/// of 2026-11-04 on comfort and the time-filler); none without a departure still to come.
fn departure_closed(
    house: &sim_core::house::data::House,
    houses: &sim_core::house::data::HouseData,
    choice: &ChoiceAt<'_>,
    age: u8,
) -> Result<Vec<serde_json::Value>, CliError> {
    use sim_core::house::choice::crosses_departure;
    let Some(leaves) = choice.leaves else {
        return Ok(Vec::new());
    };
    let depart_in = Some((leaves - choice.at).as_secs());
    let mut closed = Vec::new();
    for (object, def) in (0_u16..).zip(&house.def().objects) {
        let kind = houses
            .kind(&def.kind)
            .ok_or_else(|| CliError::new("save_rejected", "the choice: an unknown kind"))?;
        let walk = house.walk_to(choice.room, object);
        for (offer, def) in (0_u16..).zip(&kind.offers) {
            let open = (def.min_age..=def.max_age).contains(&age) && !def.sleep;
            if !open
                || !crosses_departure(def, houses.rates(), (age, choice.needs), walk, depart_in)
            {
                continue;
            }
            closed.push(serde_json::json!({
                "object": object,
                "offer": offer,
                "points": null,
                "mood": null,
                "busy": choice.busy.contains(&object),
                "closed": true,
                "departs": dated(leaves),
                "urgent": urgent(houses, def, age, choice.needs)?,
                "sated": sated(houses, def, age, choice.needs)?,
                "held": choice.held.contains(&object),
                "outside": false,
                "bed_for": kind.bed_for,
            }));
        }
    }
    Ok(closed)
}

/// The needs among `values` that `offer` raises at `age` and that stand at or above their
/// comfort, by their codes in the order of the needs: they weigh nothing in its points (ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler).
fn sated(
    houses: &sim_core::house::data::HouseData,
    offer: &sim_core::house::data::OfferDef,
    age: u8,
    values: [u16; 5],
) -> Result<Vec<serde_json::Value>, CliError> {
    use sim_core::house::data::NEEDS;
    let rates = houses.rates();
    (NEEDS.into_iter().zip(values))
        .filter(|&(need, value)| value >= rates.comfort(need) && offer.raises(rates, need, age))
        .map(|(need, _)| serde_json::to_value(need).map_err(|e| CliError::new("io", e.to_string())))
        .collect()
}

/// A choice to compute again: who, in which second, from which room, with which needs and
/// which objects without a free place, whether a command made it, in the night the objects
/// with a place a small child held and the beds of the person's tier, and today's departure
/// still to come.
struct ChoiceAt<'a> {
    sim: SimId,
    at: SimTime,
    room: u8,
    needs: [u16; 5],
    busy: &'a [u16],
    commanded: bool,
    held: &'a [u16],
    tier: &'a [u16],
    leaves: Option<SimTime>,
}

/// The points of every offer open in `choice` recomputed with `house::choice::score` from its
/// room and needs and the person's age and night in its second, each with the person's mood
/// for it then (none for a command), the most first (a tie to the smaller object, then offer).
fn scored_offers(
    world: &World,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
    choice: &ChoiceAt<'_>,
) -> Result<Vec<sim_core::house::choice::Scored>, CliError> {
    use sim_core::house::choice::{age, mood, score};
    use sim_core::house::day::night;
    let broken = |what: &str| CliError::new("save_rejected", format!("the choice: {what}"));
    let state = world.state();
    let born = state
        .born(choice.sim)
        .ok_or_else(|| broken("an unknown person"))?;
    let age = age(born, choice.at).ok_or_else(|| broken("a choice before the birth"))?;
    let left = night(houses.day(), born, choice.at).map(|night| night.left(choice.at));
    let seed = (state.world_seed()).ok_or_else(|| broken("the world has not started"))?;
    let key = (state.lineage_key(choice.sim)).ok_or_else(|| broken("an unknown person"))?;
    let percent = houses.day().choice_mood_percent;
    let moods = |object, offer| {
        if choice.commanded {
            0
        } else {
            mood(seed, key, choice.at, (object, offer), percent)
        }
    };
    let depart_in = choice.leaves.map(|leaves| (leaves - choice.at).as_secs());
    let values = (age, choice.needs, choice.room);
    let mut scored = score(house, houses, values, (left, depart_in), moods);
    scored.sort_by_key(|scored| {
        (
            core::cmp::Reverse(scored.points),
            scored.object,
            scored.offer,
        )
    });
    Ok(scored)
}

/// The offers of `scored` as "Warum?" sends them: object, offer, points, the mood (none for a
/// command), whether the object was in `choice.busy`, open, and the needs critical in `choice`
/// that the offer raises at `age`; whether a small child held a place of the object, whether the
/// offer is a bed's sleep outside the person's tier, and whom the object's kind is for (version
/// 5, ADR-0026, addendum of 2026-11-04 after the playtest).
fn offer_values(
    house: &sim_core::house::data::House,
    houses: &sim_core::house::data::HouseData,
    choice: &ChoiceAt<'_>,
    scored: &[sim_core::house::choice::Scored],
    age: u8,
) -> Result<Vec<serde_json::Value>, CliError> {
    (scored.iter())
        .map(|scored| {
            let def = offer_def(house, houses, scored.object, scored.offer)?;
            let kind = (house.def().objects.get(usize::from(scored.object)))
                .and_then(|object| houses.kind(&object.kind))
                .ok_or_else(|| CliError::new("save_rejected", "the choice: an unknown kind"))?;
            let outside = def.sleep
                && kind.is_bed()
                && !choice.tier.is_empty()
                && !choice.tier.contains(&scored.object);
            Ok(serde_json::json!({
                "object": scored.object,
                "offer": scored.offer,
                "points": scored.points,
                "mood": (!choice.commanded).then_some(scored.mood),
                "busy": choice.busy.contains(&scored.object),
                "closed": false,
                "departs": null,
                "urgent": urgent(houses, def, age, choice.needs)?,
                "sated": sated(houses, def, age, choice.needs)?,
                "held": choice.held.contains(&scored.object),
                "outside": outside,
                "bed_for": kind.bed_for,
            }))
        })
        .collect()
}

/// Offer `offer` of object `object` of `house`.
fn offer_def<'a>(
    house: &sim_core::house::data::House,
    houses: &'a sim_core::house::data::HouseData,
    object: u16,
    offer: u16,
) -> Result<&'a sim_core::house::data::OfferDef, CliError> {
    (house.def().objects.get(usize::from(object)))
        .and_then(|def| houses.kind(&def.kind))
        .and_then(|kind| kind.offers.get(usize::from(offer)))
        .ok_or_else(|| CliError::new("save_rejected", "the choice: an unknown offer"))
}

/// The needs among `values` critical (at most `house::choice::CRITICAL`) that `offer` raises at
/// `age`, by their codes in the order of the needs: what puts the urgent part into its points
/// (ADR-0026 section 3).
fn urgent(
    houses: &sim_core::house::data::HouseData,
    offer: &sim_core::house::data::OfferDef,
    age: u8,
    values: [u16; 5],
) -> Result<Vec<serde_json::Value>, CliError> {
    use sim_core::house::choice::CRITICAL;
    use sim_core::house::data::NEEDS;
    (NEEDS.into_iter().zip(values))
        .filter(|&(need, value)| value <= CRITICAL && offer.raises(houses.rates(), need, age))
        .map(|(need, _)| serde_json::to_value(need).map_err(|e| CliError::new("io", e.to_string())))
        .collect()
}

/// The comparison of a choice of `sim` at the clock (version 4 of "Warum?"): its needs then
/// (as the house answer gives them), and the offers recomputed as for an entry, from the room
/// it stands in, with its mood in the clock's second and the objects without a free place for
/// it then: each place taken by an action of a member of its household or held for another, as
/// the core's choice counts them (ADR-0026 section 5). `None` unless the person is idle in a
/// room of `house` now, neither walking nor lying on the floor.
fn compared_now(
    world: &World,
    sim: SimId,
    (house, houses): (
        &sim_core::house::data::House,
        &sim_core::house::data::HouseData,
    ),
) -> Result<Option<serde_json::Value>, CliError> {
    use sim_core::house::state::Place;
    let state = world.state();
    let Some(person) = state.house().sims.get(&sim) else {
        return Ok(None);
    };
    let (Place::Room(room), None, false) = (person.place, person.action, person.collapsed) else {
        return Ok(None);
    };
    let Some(household) = state.household_of(sim) else {
        return Ok(None);
    };
    let home = state.house().households.get(&household);
    if home.is_none_or(|moved| &moved.house != house.id()) {
        return Ok(None);
    }
    let at = world.closed_through();
    let needs = person.needs.map(|need| need.value_at(at.max(need.at())));
    // With those a small child holds in its bed (ADR-0026, addendum of 2026-11-04 after the
    // playtest), and the beds of the person's tier in its night.
    let children = state.held_by_children(household, at);
    let taken = places_taken(state, household, sim, &children);
    let mut held: Vec<u16> = children.iter().map(|&(object, _)| object).collect();
    held.dedup();
    let busy: Vec<u16> = (0_u16..)
        .zip(&house.def().objects)
        .filter(|(object, def)| {
            let slots = houses.kind(&def.kind).map_or(0, |kind| kind.slots);
            (0..slots).all(|slot| taken.contains(&(*object, slot)))
        })
        .map(|(object, _)| object)
        .collect();
    let born = state
        .born(sim)
        .ok_or_else(|| CliError::new("save_rejected", "the choice: an unknown person"))?;
    let age = sim_core::house::choice::age(born, at)
        .ok_or_else(|| CliError::new("save_rejected", "the choice: before the birth"))?;
    let tonight = sim_core::house::day::night(houses.day(), born, at).is_some();
    let tier = match tonight {
        true => state.night_beds(sim, at).unwrap_or_default(),
        false => Vec::new(),
    };
    let held = if tonight { held } else { Vec::new() };
    let seed = (state.world_seed())
        .ok_or_else(|| CliError::new("save_rejected", "the choice: the world has not started"))?;
    let key = (state.lineage_key(sim))
        .ok_or_else(|| CliError::new("save_rejected", "the choice: an unknown person"))?;
    let draw = || sim_core::house::day::work_draw(seed, key);
    let leaves = sim_core::house::day::departure(houses.day(), born, draw, at);
    let choice = ChoiceAt {
        sim,
        at,
        room,
        needs,
        busy: &busy,
        commanded: false,
        held: &held,
        tier: &tier,
        leaves,
    };
    let scored = scored_offers(world, (house, houses), &choice)?;
    let mut offers = offer_values(house, houses, &choice, &scored, age)?;
    offers.extend(departure_closed(house, houses, &choice, age)?);
    Ok(Some(
        serde_json::json!({"at": dated(at), "needs": needs, "offers": offers}),
    ))
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
                    Input::ArrangePartnership(_)
                    | Input::TriggerShock(_)
                    | Input::SeparatePartnership(_)
                    | Input::TimeTogether(_)
                    | Input::MoveIn(_)
                    | Input::MoveOut(_)
                    | Input::Command(_) => {}
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
/// round 33, P3 and P4), the end of a window of "Zeit zu zweit" still ahead and its
/// genetics card (section 5), and with `to` what
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
        "version": 4,
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
    // The last month start of the window of "Zeit zu zweit" of the person's partnership,
    // with its date, while it is ahead (ADR-0023, addendum of 2026-11-01, W5).
    answer["time_together"] = match world.time_together(sim) {
        Some((_, last)) if last > world.closed_through() => dated(last),
        _ => serde_json::Value::Null,
    };
    answer["traits"] = json(serde_json::to_value(world.traits(sim)))?;
    answer["origin"] = json(serde_json::to_value(world.origin(sim)))?;
    answer["ages"] = json(serde_json::to_value(world.ages()))?;
    answer["wish"] = json(serde_json::to_value(world.wish(sim)))?;
    with_grown_at(
        &mut answer["wish"],
        world.data().demography().fertility.extra,
    );
    if let Some(card) = world.genetics_card(sim) {
        answer["card"] = json(serde_json::to_value(card))?;
    }
    if let Some(other) = to {
        let Some(kinship) = kinship(world, sim, other)? else {
            return Ok(None);
        };
        answer["kinship"] = kinship;
    }
    Ok(Some(answer))
}

/// The couple of a person's wish with the age a child stops counting at, the town's own,
/// while a grown child of hers opens it, else null (version 4 of the person answer, ADR-0017
/// and ADR-0021, addenda of 2026-11-05): the page's sentence names it. Only a woman under the
/// rule of the extra child is opened so; a wish without a couple stays as it is.
fn with_grown_at(wish: &mut serde_json::Value, rule: Option<sim_core::demography::ExtraChild>) {
    if let Some(couple) = wish
        .get_mut("couple")
        .and_then(serde_json::Value::as_object_mut)
    {
        let grown = couple.get("state").and_then(serde_json::Value::as_str) == Some("grown");
        let years = rule.filter(|_| grown).map(|rule| rule.count_until_years);
        couple.insert("grown_at".into(), years.into());
    }
}

/// What `other` is to `sim`, by the core, with `to` naming `other`; `None` if either is
/// unknown.
fn kinship(world: &World, sim: SimId, other: SimId) -> Result<Option<serde_json::Value>, CliError> {
    let Some(kin) = world.kin_view(sim, other) else {
        return Ok(None);
    };
    let mut kinship = serde_json::to_value(kin).map_err(|e| CliError::new("io", e.to_string()))?;
    if let Some(kinship) = kinship.as_object_mut() {
        kinship.insert("to".into(), other.get().into());
    }
    Ok(Some(kinship))
}

/// The answer of the kin route (Grok's round 46, P3): the kinship of each of `others` to
/// `sim`, in their order; `None` if any of them is unknown.
pub fn kin_answer(
    world: &World,
    sim: SimId,
    others: &[SimId],
) -> Result<Option<serde_json::Value>, CliError> {
    let mut kin = Vec::new();
    for other in others {
        let Some(kinship) = kinship(world, sim, *other)? else {
            return Ok(None);
        };
        kin.push(kinship);
    }
    Ok(Some(serde_json::json!({
        "format": "simagent.view.kin",
        "version": 1,
        "kin": kin,
    })))
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
            "traits": json(serde_json::to_value(world.traits(age.sim)))?,
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
    let data = data::core_data()?;
    compared_worlds_with((a, &data), (b, &data))
}

/// [`compared_worlds`] with the data each run opens with (ADR-0021, addendum of
/// 2026-11-05).
pub fn compared_worlds_with(
    (a, data_a): (&Path, &Data),
    (b, data_b): (&Path, &Data),
) -> Result<Compared, CliError> {
    let load =
        |dir: &Path, data: &Data| world_from_save_with(&files::read(&dir.join(files::SAVE))?, data);
    let (mut one, mut other) = (load(a, data_a)?, load(b, data_b)?);
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
/// its load checked (ADR-0001), on its own data: the start, each input stamped at or before
/// `t` at its stamp, then on to `t`. Nothing is written.
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
    let mut rebuilt = World::new(world.data().clone());
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
    begin_with(out, scenario, limit, &data::core_data()?)
}

/// [`begin`] on `data`.
pub fn begin_with(
    out: &Path,
    scenario: &Scenario,
    limit: SimTime,
    data: &Data,
) -> Result<Session, CliError> {
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
    Session::start_with(out, stamp, first, Compare::Nothing, rejected, data)
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

/// The records and the end line of an input log of this core version and the data hash
/// `data_hash`. The whole log is read and checked before anything runs.
fn read_input_log(
    log: &Path,
    source: &Source,
    data_hash: u64,
) -> Result<(Vec<InputRecord>, LogEnd), CliError> {
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
    let (records, end) = read_input_log(log, &REPLAYED_LOG, data::core_data()?.hash())?;
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
    fork_into_with(parent, from_year, changes, out, &data::core_data()?, None)
}

/// [`fork_into`] on `data`, the data the parent opens with. The fork keeps `settings`, if
/// given, as its own `settings.json` before its first stop (ADR-0021, addendum of
/// 2026-11-05).
pub fn fork_into_with(
    parent: &Path,
    from_year: i64,
    changes: Vec<ParamChange>,
    out: &Path,
    data: &Data,
    settings: Option<&str>,
) -> Result<(), CliError> {
    // The fork reads its parent: it may not be in use.
    let _parent = lock::RunLock::acquire(parent)?;
    // What the core would reject at the fork point is refused before anything is written.
    let block = ForkOverride { changes };
    if let Some(code) = block.rejection().filter(|_| !block.changes.is_empty()) {
        return Err(CliError::new("override_rejected", files::code(&code)));
    }
    let changes = block.changes;
    let (records, end) = read_input_log(&parent.join(files::INPUT_LOG), &PARENT_RUN, data.hash())?;
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
        && manifest.data_hash == data.hash()
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
    let mut session = Session::start_with(out, t, first.input.clone(), compare, rejected, data)?;
    if let Some(text) = settings {
        files::write(&out.join(crate::data::SETTINGS_FILE), text.as_bytes())?;
    }
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sim_core::data_id::DataId;
    use sim_core::demography::ExtraChild;
    use sim_core::house::need::NeedSample;
    use sim_core::time::{DAY, SimTime, YEAR};

    /// The age a grown child names on the person page (version 4 of the person answer): the
    /// town's own, 16 here, only while a grown child opens the couple's wish; null in every
    /// other state; nothing added to a wish without a couple.
    #[test]
    fn a_grown_child_names_the_towns_age() {
        let rule = ExtraChild {
            count_until_years: 16,
            extra: 1,
            grown_from: 3,
            grown_chance: 0.2,
        };
        let couple =
            |state: &str| json!({"wish": 2, "couple": {"woman": 1, "man": 2, "state": state}});
        let mut grown = couple("grown");
        super::with_grown_at(&mut grown, Some(rule));
        assert_eq!(grown["couple"]["grown_at"], json!(16));
        for state in [
            "fertility_over",
            "pregnant",
            "pause",
            "hers",
            "his",
            "fulfilled",
        ] {
            let mut wish = couple(state);
            super::with_grown_at(&mut wish, Some(rule));
            assert_eq!(
                wish["couple"]["grown_at"],
                serde_json::Value::Null,
                "{state}"
            );
        }
        let mut without = json!({"wish": null, "couple": null});
        super::with_grown_at(&mut without, Some(rule));
        assert_eq!(without, json!({"wish": null, "couple": null}));
    }

    /// The departure that ends what a person does at its object (version 8 of the house answer,
    /// ADR-0021, addendum of 2026-11-05 on the house page; ADR-0024 section 4): someone of
    /// 30 watches television (fun +400 an hour, `kinds.json`) at 8:30 of day 1, the departure at
    /// 8:40. With fun at 85 then it is 85 + 400 · 600 / 3 600 = 151 at 8:40, above 150, and no
    /// other need comes near 150 (the bladder 500 − 120 / 6 = 480): the departure ends the show if
    /// it would still run then, its end at or after 8:40 or beyond the end of time, not if it ends
    /// at 8:39:59. With fun at 84 it is 150 at 8:40, critical, and the departure waits for the
    /// show. A sleep the wake before the departure ends; without a departure nothing ends it.
    #[test]
    fn the_departure_ends_what_raises_no_need_critical_then() {
        let data = crate::data::core_data().expect("the data");
        let houses = data.houses().expect("the houses");
        let offer = |kind: &str| {
            let kind = houses.kind(&DataId::new(kind).expect("valid"));
            kind.and_then(|kind| kind.offers.first()).expect("an offer")
        };
        let at = |secs: i64| SimTime::from_secs(DAY.as_secs() + secs);
        let (now, leaves) = (at(30_600), at(31_200));
        let born = SimTime::from_secs(DAY.as_secs() - 30 * YEAR.as_secs());
        let needs = |fun: u16| {
            [
                NeedSample::new(700, now, -100),
                NeedSample::new(800, now, -40),
                NeedSample::new(500, now, -120),
                NeedSample::new(700, now, -30),
                NeedSample::new(fun, now, 400),
            ]
        };
        let ends = |kind: &str, fun: u16, until: Option<SimTime>, leaves: Option<SimTime>| {
            super::departure_ends(houses, offer(kind), (&needs(fun), born), (until, leaves))
        };
        let tv = "core:tv";
        assert_eq!(ends(tv, 85, Some(at(32_700)), Some(leaves)), Some(leaves));
        assert_eq!(ends(tv, 85, Some(leaves), Some(leaves)), Some(leaves));
        assert_eq!(ends(tv, 85, None, Some(leaves)), Some(leaves));
        assert_eq!(ends(tv, 85, Some(at(31_199)), Some(leaves)), None);
        assert_eq!(ends(tv, 84, Some(at(32_700)), Some(leaves)), None);
        assert_eq!(ends(tv, 85, Some(at(32_700)), None), None);
        assert_eq!(ends("core:bed", 85, None, Some(leaves)), None);
    }
}
