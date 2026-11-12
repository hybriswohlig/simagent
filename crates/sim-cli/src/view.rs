//! `simagent view` (ADR-0021 sections 1 and 2): a local server for the browser. It listens
//! on 127.0.0.1 only, answers one connection at a time, serves the files `web/dist` held
//! when it started, reads the run directories below its directory, and moves their clocks
//! and takes their inputs through the `Session` of the command line. It computes nothing:
//! every number it sends comes from the files of the command line or from the core.

mod http;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::input::Input;
use sim_core::params::ParamChange;
use sim_core::time::{DAY, HOUR, MINUTE, SimDuration, SimTime, calendar};
use sim_core::world::World;

use crate::error::CliError;
use crate::files::{self, Scenario};
use crate::lock::RunLock;
use crate::session::{self, Session};

pub use http::{BODY_LIMIT, HEAD_LIMIT, READ_TIME};
use http::{JSON, Request, Response};

/// The port without `--port`.
pub const PORT: u16 = 8742;

/// What the server serves and reads.
struct Server {
    runs: PathBuf,
    /// The data of a run without settings of its own (ADR-0021, addendum of 2026-11-05): a
    /// `settings.json` in `runs` replaces calibration values; otherwise these are the
    /// built-in data.
    data: Data,
    /// The values of that `settings.json` as the server read it at its start, if there is
    /// one: a new town that brings no values of its own keeps them as its own.
    folder: Option<BTreeMap<String, f64>>,
    /// The data of the runs with settings of their own, by the text of their file.
    own: RefCell<BTreeMap<String, Data>>,
    port: u16,
    /// The files of `web/dist` at start: relative path to type and bytes.
    files: BTreeMap<String, (&'static str, Vec<u8>)>,
    /// The runs a write opened, by name; each keeps its lock until the server ends.
    open: BTreeMap<String, Open>,
    /// The runs whose write failed: their files may stand between two writes, so the
    /// server reads none of them until a write opens the run whole again.
    failed: BTreeSet<String>,
    /// The world loaded last: the reads of a step (world, pedigree, a person) load it once
    /// (the third playtest, K12).
    loaded: RefCell<Option<Loaded>>,
}

/// What a run opens with (ADR-0021, addendum of 2026-11-05): its data and the values
/// beside the base game's they are built from, its own or else the folder's; `None` for the
/// base game without any settings.
struct Settled {
    data: Data,
    values: Option<BTreeMap<String, f64>>,
}

/// The values a new town may set where it is founded (ADR-0021, addendum of 2026-11-05), in
/// the order of the page, each with what the base game's towns show of it where the value
/// was fitted to that: the median age of women at their first partnership, 23.5 years, for
/// the centre of their search, and the mean age gap, 3.0 years, for the preferred gap
/// (`tools/life_reference.py` and the notes in `data/calibration/demography.json`). Then the
/// children's wish (ADR-0017, addendum of 2026-11-05): the tilt with the live births per
/// woman reaching 50 it was fitted to, 2.2, and the four values of the extra child, none
/// fitted to an outcome (the note of each in `data/calibration/demography_game.json`).
const FOUNDING: [(&str, Option<f64>); 8] = [
    ("core:market_age", None),
    ("core:market_search_center_female", Some(23.5)),
    ("core:pairing_age_gap_preferred", Some(3.0)),
    ("core:fertility_wish_tilt", Some(2.2)),
    ("core:fertility_count_until", None),
    ("core:fertility_extra", None),
    ("core:fertility_grown_from", None),
    ("core:fertility_grown_chance", None),
];

/// [`FOUNDING`] as the run list names it, each with the base game's value from the
/// built-in data; `None` if the data lack one.
fn founding() -> Option<Value> {
    let values = (FOUNDING.iter())
        .map(|(key, outcome)| {
            let base = crate::data::base_value(key)?;
            Some(json!({"key": key, "base": base, "outcome": outcome}))
        })
        .collect::<Option<Vec<Value>>>()?;
    Some(Value::from(values))
}

/// Values by name as the run list names them: `[{"key", "value"}]`.
fn listed_values(values: &BTreeMap<String, f64>) -> Value {
    (values.iter())
        .map(|(key, value)| json!({"key": key, "value": value}))
        .collect()
}

/// A run's world with the directory and the bytes of the save it was loaded from.
struct Loaded {
    dir: PathBuf,
    save: Vec<u8>,
    world: Rc<World>,
}

/// A run the server writes (ADR-0021 section 1): opened as `run --resume` opens it, its
/// lock held while it is open.
struct Open {
    session: Session,
    scenario: Scenario,
    _lock: RunLock,
}

/// The one step of `POST …/advance` (ADR-0021 section 2 and its addendum of 2026-11-03).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Days28,
    Year,
    Decade,
    Minutes10,
    Hour,
    Morning,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Advance {
    step: Step,
}

/// The templates a new run is made from (ADR-0021 section 2): a closed list, the fun-test
/// town first (section 6).
const TEMPLATES: [(&str, &str); 1] =
    [("stadt", include_str!("../../../data/scenarios/stadt.json"))];

/// The body of `POST /api/runs/{run}/fork` (the addendum of ADR-0021 section 2, G2): the
/// name of the fork, its year and its override, as `simagent fork` takes them.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fork {
    run: String,
    from_year: i64,
    changes: Vec<ParamChange>,
}

/// The body of `POST /api/runs`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Found {
    template: String,
    run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    world_seed: Option<String>,
    /// The town's own values by calibration name (ADR-0021, addendum of 2026-11-05), the
    /// numbers as they came, so that the body reads back as it was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<BTreeMap<String, serde_json::Number>>,
}

/// A seed as the files write it: 16 lower-case hex digits.
fn hex_seed(text: &str) -> Result<u64, ()> {
    let digits = text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if text.len() != 16 || !digits {
        return Err(());
    }
    u64::from_str_radix(text, 16).map_err(|_| ())
}

/// A write, read from its body before the run is opened.
enum WriteRequest {
    Advance(Step),
    Input(Input),
}

/// The write the body of `POST …/{route}` asks for, if it is one.
fn parse_write(route: &str, body: &[u8]) -> Option<WriteRequest> {
    let body = std::str::from_utf8(body).ok()?;
    match route {
        "advance" => files::parse_strict(body).map(|Advance { step }| WriteRequest::Advance(step)),
        _ => files::parse_strict(body).map(WriteRequest::Input),
    }
}

/// The target of `step` from `closed`, by the core's calendar: 28 days on; the last second
/// of the year that begins or continues after `closed`; ten such year ends; ten minutes, an
/// hour on; the first 6:00 after `closed`. A decade that would pass the horizon stops at its
/// first year end beyond, which `check_limit` refuses.
pub fn step_target(closed: SimTime, step: Step) -> SimTime {
    match step {
        Step::Days28 => closed + DAY * 28,
        Step::Minutes10 => closed + MINUTE * 10,
        Step::Hour => closed + HOUR,
        Step::Morning => {
            let of_day = i64::from(calendar(closed).second_of_day);
            let six = HOUR.as_secs() * 6;
            let wait = if of_day < six {
                six - of_day
            } else {
                DAY.as_secs() - of_day + six
            };
            closed + SimDuration::from_secs(wait)
        }
        Step::Year => session::end_of_the_open_year(closed),
        Step::Decade => {
            let mut target = closed;
            for _ in 0..10 {
                target = session::end_of_the_open_year(target);
                if session::check_limit(closed, target).is_err() {
                    break;
                }
            }
            target
        }
    }
}

/// The type of a served file by its extension.
fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => JSON,
        _ => "application/octet-stream",
    }
}

/// The regular files below `dir`, by their path relative to `web`; symbolic links and what
/// they point at are left out, and so is a path with a segment that is no plain name (a
/// browser would send it percent-encoded, which the server refuses).
fn web_files(
    web: &Path,
    dir: &Path,
    found: &mut BTreeMap<String, (&'static str, Vec<u8>)>,
) -> Result<(), CliError> {
    let entries = std::fs::read_dir(dir).map_err(|e| CliError::io(dir, &e))?;
    for entry in entries {
        let entry = entry.map_err(|e| CliError::io(dir, &e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| CliError::io(&path, &e))?;
        if kind.is_dir() {
            web_files(web, &path, found)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(web).unwrap_or(&path);
            let segments: Option<Vec<&str>> = relative
                .components()
                .map(|c| c.as_os_str().to_str())
                .collect();
            if let Some(segments) = segments.filter(|s| s.iter().all(|s| run_name(s))) {
                let name = segments.join("/");
                let bytes = files::read(&path)?;
                found.insert(name.clone(), (content_type(&name), bytes));
            }
        }
    }
    Ok(())
}

/// A run's name, as the router of `web/` takes it: letters, digits, `.`, `_` and `-`, not
/// starting with `.`.
pub fn run_name(name: &str) -> bool {
    let first = name.bytes().next();
    first.is_some_and(|b| b != b'.')
        && (name.bytes()).all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// A path the server may look up: no `%`, `\`, `..` or empty segment.
fn plain(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').skip(1).collect();
    !path.contains('%')
        && !path.contains('\\')
        && (segments.iter()).all(|s| !s.is_empty() && *s != "." && *s != "..")
}

impl Server {
    /// The answer to a request: origin, method, then the route.
    fn answer(&mut self, request: &Request) -> Response {
        let host = request.header("host");
        let own = [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ];
        let host = match host {
            None => return Response::error(400, "host_missing"),
            Some(host) if !own.iter().any(|o| o == host) => {
                return Response::error(421, "host_foreign");
            }
            Some(host) => host,
        };
        let segments: Vec<&str> = request.path.split('/').skip(1).collect();
        let allowed = match segments.as_slice() {
            ["api", "runs"] => "GET, POST",
            ["api", "runs", _, "advance" | "input" | "fork"] => "POST",
            _ => "GET",
        };
        if !allowed.split(", ").any(|method| method == request.method) {
            let mut refused = Response::error(405, "method_not_allowed");
            refused.allow = Some(allowed);
            return refused;
        }
        if request.method == "POST" {
            if let Some(refused) = foreign_write(request, host) {
                return refused;
            }
        } else if !request.body.is_empty() {
            return Response::error(400, "bad_request");
        }
        if request.path == "/" {
            return self.file("index.html", request);
        }
        if !plain(&request.path) {
            return Response::error(404, "not_found");
        }
        match segments.as_slice() {
            // A query belongs to the events and the persons alone.
            ["api", "runs", _, "events"]
            | ["api", "runs", _, "persons", _]
            | ["api", "runs", _, "persons", _, "kin"] => {}
            ["api", ..] if request.query.is_some() => return Response::error(400, "bad_query"),
            _ => {}
        }
        match segments.as_slice() {
            ["api", "runs"] if request.method == "POST" => self.create(&request.body),
            ["api", "runs"] => self.runs(),
            ["api", "runs", run] => self.manifest(run),
            ["api", "runs", run, "pedigree"] => self.read(run, |dir| self.pedigree(dir)),
            ["api", "runs", run, "persons", sim] => match person_query(request.query.as_deref()) {
                Some(to) => self.read(run, |dir| person(|| self.world_of(dir), sim, to)),
                None => Response::error(400, "bad_query"),
            },
            ["api", "runs", run, "persons", sim, "kin"] => {
                match kin_query(request.query.as_deref()) {
                    Some(to) => self.read(run, |dir| kin(|| self.world_of(dir), sim, &to)),
                    None => Response::error(400, "bad_query"),
                }
            }
            ["api", "runs", run, "census"] => self.read(run, census),
            ["api", "runs", run, "people"] => self.read(run, |dir| people(|| self.world_of(dir))),
            ["api", "runs", run, "children", mother, father] => {
                self.read(run, |dir| children(|| self.world_of(dir), mother, father))
            }
            ["api", "runs", run, "world"] => self.read(run, |dir| world(self.world_of(dir))),
            ["api", "runs", run, "houses"] => self.read(run, |dir| houses(self.world_of(dir))),
            ["api", "runs", run, "house", household] => {
                self.read(run, |dir| house(self.world_of(dir), household))
            }
            ["api", "runs", run, "why", entry] => {
                self.read(run, |dir| why(dir, || self.world_of(dir), entry))
            }
            ["api", "runs", a, "diff", b] => self.read(a, |one| {
                self.read(b, |other| match (self.data_of(one), self.data_of(other)) {
                    (Ok(x), Ok(y)) => diff((a, b), (one, other), (&x, &y)),
                    (Err(error), _) | (_, Err(error)) => Response::error(500, error.code),
                })
            }),
            ["api", "runs", run, "fork"] => self.fork(run, &request.body),
            ["api", "sheet"] => sheet(),
            ["api", "runs", run, "events"] => {
                let query = request.query.as_deref().unwrap_or("");
                self.read(run, |dir| {
                    let feelings = self
                        .data_of(dir)
                        .ok()
                        .and_then(|data| data.social().map(|social| social.feelings.clone()));
                    events(dir, query, feelings.as_ref())
                })
            }
            ["api", "runs", run, route @ ("advance" | "input")] => {
                match parse_write(route, &request.body) {
                    Some(write) => self.write(run, write),
                    None => Response::error(400, "body_invalid"),
                }
            }
            ["api", ..] => Response::error(404, "not_found"),
            _ => self.file(&request.path[1..], request),
        }
    }

    /// Does `write` on `run`, opening it on its first write: listed, its lock taken, its
    /// scenario and session read as `run --resume` reads them (which checks the files are
    /// one run). A run that does not open is not kept and, unless another process holds
    /// it, marked failed; so is one whose write failed, closed with its lock let go. The
    /// mark goes when a write opens the run whole again.
    fn write(&mut self, run: &str, write: WriteRequest) -> Response {
        if !self.open.contains_key(run) {
            match self.run_names() {
                None => return Response::error(500, "runs_unreadable"),
                Some(names) if !names.iter().any(|n| n == run) => {
                    return Response::error(404, "unknown_run");
                }
                Some(_) => {}
            }
            let dir = self.runs.join(run);
            // A run whose own values give no data is not opened, and not marked failed:
            // nothing of it was written.
            let data = match self.data_of(&dir) {
                Ok(data) => data,
                Err(error) => return failure(&error),
            };
            match open(&dir, &data) {
                Ok(opened) => {
                    self.failed.remove(run);
                    self.open.insert(run.to_owned(), opened);
                }
                Err(error) => {
                    if error.code != "run_locked" {
                        self.failed.insert(run.to_owned());
                    }
                    return failure(&error);
                }
            }
        }
        let Some(opened) = self.open.get_mut(run) else {
            return Response::error(500, "run_not_open");
        };
        let done = match write {
            WriteRequest::Advance(step) => advance(opened, step),
            WriteRequest::Input(input) => take_input(opened, input),
        };
        done.unwrap_or_else(|error| {
            self.open.remove(run);
            self.failed.insert(run.to_owned());
            failure(&error)
        })
    }

    /// `POST /api/runs`: a new run from a template, made like `run --scenario <template>`
    /// (the template's end: the town stands in its first second) and kept open by the server. The name
    /// must be free in the runs directory; a seed may replace the template's. The town keeps
    /// its own values, those it brings or else the folder's, as its own `settings.json`,
    /// written before its first stop (ADR-0021, addendum of 2026-11-05); values the data
    /// refuse make nothing.
    fn create(&mut self, body: &[u8]) -> Response {
        let found = std::str::from_utf8(body)
            .ok()
            .and_then(files::parse_strict::<Found>);
        let template = found.as_ref().and_then(|found| {
            let (_, text) = TEMPLATES.iter().find(|(name, _)| *name == found.template)?;
            Some(*text)
        });
        let seed = found
            .as_ref()
            .map(|found| found.world_seed.as_deref().map(hex_seed));
        let (Some(found), Some(template), Some(seed)) = (found, template, seed) else {
            return Response::error(400, "body_invalid");
        };
        let Some(seed) = seed.transpose().ok() else {
            return Response::error(400, "body_invalid");
        };
        if !run_name(&found.run) {
            return Response::error(400, "body_invalid");
        }
        let dir = self.runs.join(&found.run);
        if dir.symlink_metadata().is_ok() {
            return Response::error(409, "run_exists");
        }
        let (own, data) = match &found.settings {
            None => (
                self.folder.as_ref().map(crate::data::settings_text),
                self.data.clone(),
            ),
            Some(given) => {
                let values = (given.iter())
                    .map(|(key, number)| Some((key.clone(), number.as_f64()?)))
                    .collect::<Option<BTreeMap<String, f64>>>();
                let Some(values) = values else {
                    return Response::error(400, "body_invalid");
                };
                let text = crate::data::settings_text(&values);
                match crate::data::data_with_settings(&text) {
                    Ok(data) => (Some(text), data),
                    Err(error) => return Response::error(409, error.code),
                }
            }
        };
        let lock = match RunLock::acquire(&dir) {
            Ok(lock) => lock,
            Err(error) => return failure(&error),
        };
        // Again under the lock: another process may have made it since.
        if dir.symlink_metadata().is_ok() {
            return Response::error(409, "run_exists");
        }
        let scenario = crate::scenario_file(template).and_then(|mut file| {
            if let Some(seed) = seed {
                file.start.world_seed = seed;
            }
            crate::resolve(file, || Ok(data.clone()))
        });
        let scenario = match scenario {
            Ok(scenario) => scenario,
            Err(error) => return failure(&error),
        };
        // From here on the directory is this server's: a failure marks it failed.
        let made = (|| {
            // The run stands at the template's end, as `run --scenario` leaves it.
            let mut session = crate::begin_with(&dir, &scenario, scenario.until, &data)?;
            // Its own values before its first stop: a town that is listed has them.
            if let Some(text) = &own {
                files::write(&dir.join(crate::data::SETTINGS_FILE), text.as_bytes())?;
            }
            session.advance(scenario.until)?;
            session.checkpoint(&scenario)?;
            Ok::<_, CliError>(session)
        })();
        match made {
            Ok(session) => {
                self.failed.remove(&found.run);
                let opened = Open {
                    session,
                    scenario,
                    _lock: lock,
                };
                self.open.insert(found.run.clone(), opened);
                if let Some(text) = own {
                    self.own.borrow_mut().insert(text, data);
                }
                let body =
                    json!({"format": "simagent.view.created", "version": 1, "run": found.run});
                Response::new(200, JSON, format!("{body}\n").into_bytes())
            }
            Err(error) => {
                self.failed.insert(found.run);
                failure(&error)
            }
        }
    }

    fn file(&self, path: &str, request: &Request) -> Response {
        match (self.files.get(path), &request.query) {
            (Some((content_type, bytes)), None) => Response::new(200, content_type, bytes.clone()),
            _ => Response::error(404, "not_found"),
        }
    }

    /// The names of the runs in name order: directories (no link) of the runs directory,
    /// under a plain name, with a regular file `manifest.json` (no link).
    fn run_names(&self) -> Option<Vec<String>> {
        let entries = std::fs::read_dir(&self.runs).ok()?;
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| {
                let dir = self.runs.join(name);
                let is = |path: &Path, dir: bool| {
                    (path.symlink_metadata())
                        .is_ok_and(|m| if dir { m.is_dir() } else { m.is_file() })
                };
                run_name(name) && is(&dir, true) && is(&dir.join(files::MANIFEST), false)
            })
            .collect();
        names.sort();
        Some(names)
    }

    /// The manifest of a run as the command line wrote it, and what it says, if it reads
    /// as one.
    fn read_manifest(&self, run: &str) -> Result<(Vec<u8>, files::Manifest), &'static str> {
        let bytes = std::fs::read(self.runs.join(run).join(files::MANIFEST))
            .map_err(|_| "manifest_invalid")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "manifest_invalid")?;
        let manifest = files::read_manifest(text).map_err(|e| e.code)?;
        Ok((bytes, manifest))
    }

    /// `GET /api/runs`, version 2 (ADR-0021, addendum of 2026-11-05): the values a new town
    /// may set, and the runs in name order, each with its manifest, the core's calendar date
    /// of its last closed second and the values it opens with beside the base game's, or why
    /// it has none.
    fn runs(&self) -> Response {
        let Some(names) = self.run_names() else {
            return Response::error(500, "runs_unreadable");
        };
        let Some(founding) = founding() else {
            return Response::error(500, "data_rejected");
        };
        let runs: Vec<Value> = (names.iter())
            .map(|name| {
                if self.failed.contains(name) {
                    return json!({"run": name, "error": "run_failed"});
                }
                let read = self.read_manifest(name).ok().and_then(|(bytes, manifest)| {
                    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
                    Some((value, manifest.closed_through))
                });
                let Some((manifest, closed)) = read else {
                    return json!({"run": name, "error": "manifest_invalid"});
                };
                match self.settled(&self.runs.join(name)) {
                    Ok(settled) => json!({
                        "run": name,
                        "manifest": manifest,
                        "date": date(closed),
                        "settings": listed_values(&settled.values.unwrap_or_default()),
                    }),
                    Err(_) => json!({"run": name, "error": "settings_invalid"}),
                }
            })
            .collect();
        let body = json!({
            "format": "simagent.view.runs",
            "version": 2,
            "founding": founding,
            "runs": runs,
        });
        Response::new(200, JSON, format!("{body}\n").into_bytes())
    }

    /// `POST /api/runs/{run}/fork` (the addendum of ADR-0021 section 2, G2): the fork of
    /// `run` made like `simagent fork` into a new run of the list, its name free as for
    /// `POST /api/runs`. A run the server holds open is closed first, which lets its lock
    /// go; its files are whole since its last write. The fork's directory is the request's
    /// only under its lock and only if it is still absent there; the lock is held through
    /// the fork and, if it fails, through taking away what it made. The fork opens with the
    /// parent's data and keeps the parent's values as its own (ADR-0021, addendum of
    /// 2026-11-05).
    fn fork(&mut self, parent: &str, body: &[u8]) -> Response {
        let Some(fork) = std::str::from_utf8(body)
            .ok()
            .and_then(files::parse_strict::<Fork>)
            .filter(|fork| run_name(&fork.run))
        else {
            return Response::error(400, "body_invalid");
        };
        let parent_dir = match self.listed(parent) {
            Ok(dir) => dir,
            Err(refused) => return refused,
        };
        let settled = match self.settled(&parent_dir) {
            Ok(settled) => settled,
            Err(error) => return failure(&error),
        };
        let own = settled.values.as_ref().map(crate::data::settings_text);
        let dir = self.runs.join(&fork.run);
        if dir.symlink_metadata().is_ok() {
            return Response::error(409, "run_exists");
        }
        let lock = match RunLock::acquire(&dir) {
            Ok(lock) => lock,
            Err(error) => return failure(&error),
        };
        if dir.symlink_metadata().is_ok() {
            return Response::error(409, "run_exists");
        }
        self.open.remove(parent);
        let made = crate::fork_into_with(
            &parent_dir,
            fork.from_year,
            fork.changes,
            &dir,
            &settled.data,
            own.as_deref(),
        );
        if made.is_err() {
            discard(&dir, &self.runs.join(format!("{}.repro.json", fork.run)));
        }
        drop(lock);
        match made {
            Ok(()) => {
                self.failed.remove(&fork.run);
                let body = json!({"format": "simagent.view.forked", "version": 1, "run": fork.run});
                Response::new(200, JSON, format!("{body}\n").into_bytes())
            }
            Err(error) => {
                let mut body =
                    json!({"format": "simagent.view.error", "version": 1, "code": error.code});
                if error.code == "override_rejected" {
                    body["reason"] = json!(error.detail);
                }
                // A conflict with the request; anything else is a failure of the parent or
                // of the server.
                let conflict = matches!(
                    error.code,
                    "fork_point_out_of_range" | "override_rejected" | "run_locked"
                );
                let status = if conflict { 409 } else { 500 };
                Response::new(status, JSON, format!("{body}\n").into_bytes())
            }
        }
    }

    /// The directory of `run` if it is one of the list exactly, else the refusal.
    fn listed(&self, run: &str) -> Result<PathBuf, Response> {
        match self.run_names() {
            None => Err(Response::error(500, "runs_unreadable")),
            Some(names) if !names.iter().any(|n| n == run) => {
                Err(Response::error(404, "unknown_run"))
            }
            Some(_) if self.failed.contains(run) => Err(Response::error(500, "run_failed")),
            Some(_) => Ok(self.runs.join(run)),
        }
    }

    /// What the run in `dir` opens with (ADR-0021, addendum of 2026-11-05): the data of its
    /// own `settings.json` if it holds one, else the server's. A file there that is no
    /// regular file, does not read or gives no data is an error: the run does not open on
    /// other data instead.
    fn settled(&self, dir: &Path) -> Result<Settled, CliError> {
        let path = dir.join(crate::data::SETTINGS_FILE);
        match path.symlink_metadata() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settled {
                    data: self.data.clone(),
                    values: self.folder.clone(),
                });
            }
            Err(error) => return Err(CliError::io(&path, &error)),
            Ok(kind) if !kind.is_file() => {
                let detail = format!("{} is no regular file", path.display());
                return Err(CliError::new("data_rejected", detail));
            }
            Ok(_) => {}
        }
        let text = files::read_text(&path)?;
        let values = Some(crate::data::settings_values(&text)?);
        if let Some(data) = self.own.borrow().get(&text) {
            let data = data.clone();
            return Ok(Settled { data, values });
        }
        let data = crate::data::data_with_settings(&text)?;
        self.own.borrow_mut().insert(text, data.clone());
        Ok(Settled { data, values })
    }

    /// The data the run in `dir` opens with.
    fn data_of(&self, dir: &Path) -> Result<Data, CliError> {
        self.settled(dir).map(|settled| settled.data)
    }

    /// The answer of `read` on the directory of `run`, if it is one of the list exactly.
    fn read(&self, run: &str, read: impl FnOnce(&Path) -> Response) -> Response {
        match self.listed(run) {
            Ok(dir) => read(&dir),
            Err(refused) => refused,
        }
    }

    /// The world of the run in `dir`: the one loaded last while it is this run's, its save
    /// has the same bytes and it opens with the same data, else loaded from them. The world
    /// is the save's alone (the data the run opens with), so both give the same answers.
    fn world_of(&self, dir: &Path) -> Result<Rc<World>, CliError> {
        let data = self.data_of(dir)?;
        let save = files::read(&dir.join(files::SAVE))?;
        if let Some(loaded) = self.loaded.borrow().as_ref()
            && loaded.dir == dir
            && loaded.save == save
            && loaded.world.data().hash() == data.hash()
        {
            return Ok(Rc::clone(&loaded.world));
        }
        let world = Rc::new(crate::world_from_save_with(&save, &data)?);
        let world_kept = Rc::clone(&world);
        *self.loaded.borrow_mut() = Some(Loaded {
            dir: dir.to_path_buf(),
            save,
            world: world_kept,
        });
        Ok(world)
    }

    /// `GET /api/runs/{run}/pedigree`: byte for byte what `simagent export` writes.
    fn pedigree(&self, dir: &Path) -> Response {
        match self
            .world_of(dir)
            .and_then(|world| crate::pedigree_text(&world))
        {
            Ok(text) => Response::new(200, JSON, text.into_bytes()),
            Err(error) => Response::error(500, error.code),
        }
    }

    /// `GET /api/runs/{run}`: the manifest of the command line as it lies in the directory.
    /// The name must be one of the list exactly (a file system may ignore case).
    fn manifest(&self, run: &str) -> Response {
        self.read(run, |_| match self.read_manifest(run) {
            Ok((bytes, _)) => Response::new(200, JSON, bytes),
            Err(code) => Response::error(500, code),
        })
    }

    fn serve(&mut self, mut stream: TcpStream) {
        let response = match http::read_request(&mut stream) {
            Ok(request) => self.answer(&request),
            Err(refusal) => refusal,
        };
        let _ = http::write_response(&mut stream, &response);
        let _ = stream.shutdown(Shutdown::Both);
    }
}

/// The core's calendar date of `t` (ADR-0002: every unit counted from 0; their names are
/// texts of `web/`).
pub(crate) fn date(t: SimTime) -> Value {
    let date = calendar(t);
    json!({
        "year": date.year,
        "day_of_year": date.day_of_year,
        "month": date.month,
        "day_of_month": date.day_of_month,
        "week": date.week,
        "weekday": date.weekday,
        "season": date.season,
        "second_of_day": date.second_of_day,
    })
}

/// A write that does not come from the server's own page as JSON (ADR-0021 section 1):
/// `Origin` exactly the origin of the checked `Host`, the media type `application/json`.
fn foreign_write(request: &Request, host: &str) -> Option<Response> {
    if request.header("origin") != Some(format!("http://{host}").as_str()) {
        return Some(Response::error(403, "origin_foreign"));
    }
    let media = (request.header("content-type"))
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !media.is_some_and(|media| media.eq_ignore_ascii_case(JSON)) {
        return Some(Response::error(415, "content_type_not_json"));
    }
    None
}

/// A run opened for writing: its lock first, then what `run --resume` reads.
fn open(dir: &Path, data: &Data) -> Result<Open, CliError> {
    let lock = RunLock::acquire(dir)?;
    let scenario = crate::resumed_scenario(dir)?;
    let session = Session::resume_with(dir, data)?;
    Ok(Open {
        session,
        scenario,
        _lock: lock,
    })
}

/// The answer to an error of the command line: a run another process holds is a conflict,
/// anything else a failure of the run.
fn failure(error: &CliError) -> Response {
    let status = if error.code == "run_locked" { 409 } else { 500 };
    Response::error(status, error.code)
}

/// `POST /api/runs/{run}/advance`: the clock moved by one step, written like `run --resume
/// --until` the step's target; the closed second and its calendar date.
fn advance(open: &mut Open, step: Step) -> Result<Response, CliError> {
    let closed = open.session.closed_through();
    let target = step_target(closed, step);
    if let Err(refused) = session::check_limit(closed, target) {
        return Ok(Response::error(409, refused.code));
    }
    open.session.advance(target)?;
    open.session.checkpoint(&open.scenario)?;
    let t = open.session.closed_through();
    let body = json!({
        "format": "simagent.view.advanced",
        "version": 1,
        "closed_through": t.as_secs(),
        "date": date(t),
    });
    Ok(Response::new(200, JSON, format!("{body}\n").into_bytes()))
}

/// `POST /api/runs/{run}/input`: one input in the command schema at the stop, written like
/// `run --resume --input` before the answer; its stamp, or the core's code of a rejection,
/// which writes nothing.
fn take_input(open: &mut Open, input: Input) -> Result<Response, CliError> {
    let stamp = match open
        .session
        .submit(input, |code| CliError::new("input_rejected", code))
    {
        Ok(stamp) => stamp,
        Err(rejected) if rejected.code == "input_rejected" => {
            let body = json!({
                "format": "simagent.view.error",
                "version": 1,
                "code": rejected.code,
                "reason": rejected.detail,
            });
            return Ok(Response::new(409, JSON, format!("{body}\n").into_bytes()));
        }
        Err(error) => return Err(error),
    };
    open.session.checkpoint(&open.scenario)?;
    let body = json!({
        "format": "simagent.view.input",
        "version": 1,
        "t": stamp.t.as_secs(),
        "seq": stamp.seq.get(),
    });
    Ok(Response::new(200, JSON, format!("{body}\n").into_bytes()))
}

/// The query of the person route: none, or exactly `to=` and digits (the second sim, still
/// as text); anything else is `None`.
fn person_query(query: Option<&str>) -> Option<Option<&str>> {
    match query {
        None => Some(None),
        Some(query) => (query.strip_prefix("to="))
            .filter(|to| !to.is_empty() && to.bytes().all(|b| b.is_ascii_digit()))
            .map(Some),
    }
}

/// The query of the kin route: exactly `to=` and one or more runs of digits, separated by
/// commas (the other sims, still as text); anything else is `None`.
fn kin_query(query: Option<&str>) -> Option<Vec<&str>> {
    let to = query?.strip_prefix("to=")?;
    let others: Vec<&str> = to.split(',').collect();
    (others.iter())
        .all(|other| !other.is_empty() && other.bytes().all(|b| b.is_ascii_digit()))
        .then_some(others)
}

/// A sim of a path or a query: decimal, without a leading zero or sign; `None` otherwise.
fn sim_of(text: &str) -> Option<sim_core::ids::SimId> {
    (text.parse::<u64>().ok())
        .filter(|_| !text.starts_with('0') && !text.starts_with('+'))
        .and_then(|n| sim_core::ids::SimId::try_from(n).ok())
}

/// `GET /api/runs/{run}/persons/{sim}`: the presentation state of the core for one person,
/// with `?to=` also what the second sim is to it.
fn person(
    world: impl FnOnce() -> Result<Rc<World>, CliError>,
    sim: &str,
    to: Option<&str>,
) -> Response {
    let unknown = || Response::error(404, "unknown_sim");
    let Some(id) = sim_of(sim) else {
        return unknown();
    };
    let other = match to.map(sim_of) {
        None => None,
        Some(Some(other)) => Some(other),
        Some(None) => return unknown(),
    };
    match world().and_then(|world| crate::person_answer(&world, id, other)) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => unknown(),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/persons/{sim}/kin?to={other},…` (Grok's round 46, P3): what each
/// other sim is to the person, in the order asked, as the person route's `kinship`.
fn kin(world: impl FnOnce() -> Result<Rc<World>, CliError>, sim: &str, to: &[&str]) -> Response {
    let unknown = || Response::error(404, "unknown_sim");
    let (Some(id), Some(others)) = (
        sim_of(sim),
        to.iter()
            .map(|other| sim_of(other))
            .collect::<Option<Vec<_>>>(),
    ) else {
        return unknown();
    };
    match world().and_then(|world| crate::kin_answer(&world, id, &others)) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => unknown(),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/children/{mother}/{father}`: the four possible children of the couple
/// (Grok's round 44), from the core; `no_children` for a pair the core shows none of.
fn children(
    world: impl FnOnce() -> Result<Rc<World>, CliError>,
    mother: &str,
    father: &str,
) -> Response {
    let none = || Response::error(404, "no_children");
    let (Some(mother), Some(father)) = (sim_of(mother), sim_of(father)) else {
        return none();
    };
    let world = match world() {
        Ok(world) => world,
        Err(error) => return Response::error(500, error.code),
    };
    let Some(children) = world.preview_children(mother, father) else {
        return none();
    };
    let answer = serde_json::json!({
        "format": "simagent.view.children",
        "version": 1,
        "contract_version": sim_core::body_contract::CONTRACT_VERSION,
        "children": children,
    });
    Response::new(200, JSON, format!("{answer}\n").into_bytes())
}

/// `GET /api/runs/{run}/people`: the living persons with their bodies now and their looks
/// in words, from the core.
fn people(world: impl FnOnce() -> Result<Rc<World>, CliError>) -> Response {
    match world().and_then(|world| crate::people_answer(&world)) {
        Ok(answer) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Err(error) => Response::error(500, error.code),
    }
}

/// Takes away the directory a failed fork made, under its lock: all of it but a repro
/// package of the core (ADR-0016), which is kept as `kept` beside the runs. A hard link
/// never replaces what is there: if it cannot be made, the directory stays with it.
fn discard(dir: &Path, kept: &Path) {
    let repro = dir.join(crate::repro::REPRO);
    if repro.symlink_metadata().is_ok() && std::fs::hard_link(&repro, kept).is_err() {
        return;
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// `GET /api/runs/{a}/diff/{b}` (the addendum of ADR-0021 section 2, G2): the comparison
/// of the core, as `simagent diff` prints it, with the names of the runs and each second
/// with its date; the second compared at and the run recomputed to it (the addendum of
/// 2026-10-29), each run on the data it opens with.
fn diff((a, b): (&str, &str), (one, other): (&Path, &Path), data: (&Data, &Data)) -> Response {
    let compared = (|| -> Result<(Value, SimTime, Option<crate::Side>), CliError> {
        let worlds = crate::compared_worlds_with((one, data.0), (other, data.1))?;
        let compared = serde_json::to_value(crate::diff_of_worlds(&worlds.a, &worlds.b))
            .map_err(|error| CliError::new("io", error.to_string()))?;
        Ok((compared, worlds.at, worlds.recomputed))
    })();
    let (compared, at, recomputed) = match compared {
        Ok(compared) => compared,
        Err(error) => return Response::error(500, error.code),
    };
    let recomputed = recomputed.map(|side| match side {
        crate::Side::A => a,
        crate::Side::B => b,
    });
    let body = json!({
        "format": "simagent.view.diff",
        "version": 4,
        "a": a,
        "b": b,
        "at": {"t": at.as_secs(), "date": date(at)},
        "recomputed": recomputed,
        "summary": compared["summary"],
        "a_only": compared["a_only"],
        "b_only": compared["b_only"],
        "changed": dated(compared["changed"].clone()),
    });
    Response::new(200, JSON, format!("{body}\n").into_bytes())
}

/// The changed lives of a comparison with each second as `{"t", "date"}`.
fn dated(mut changed: Value) -> Value {
    let moment = |value: &mut Value| {
        if let Some(t) = value.as_i64() {
            *value = json!({"t": t, "date": date(SimTime::from_secs(t))});
        }
    };
    for entry in changed.as_array_mut().into_iter().flatten() {
        for side in ["a", "b"] {
            let life = &mut entry[side];
            for field in ["born", "died", "emigrated"] {
                moment(&mut life[field]);
            }
            for bond in life["partnerships"].as_array_mut().into_iter().flatten() {
                moment(&mut bond["formed"]);
                moment(&mut bond["ended"]);
            }
        }
    }
    changed
}

/// `GET /api/sheet`: the sheet of portraits (ADR-0021 G10).
fn sheet() -> Response {
    match crate::sheet_answer() {
        Ok(answer) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/world`: where the switch of the hereditary diseases stands.
fn world(world: Result<Rc<World>, CliError>) -> Response {
    match world.and_then(|world| crate::world_answer(&world)) {
        Ok(answer) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/houses` (ADR-0021, addendum of 2026-11-03).
fn houses(world: Result<Rc<World>, CliError>) -> Response {
    match world.map(|world| crate::houses_answer(&world)) {
        Ok(answer) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/house/{household}` (ADR-0021, addendum of 2026-11-03).
fn house(world: Result<Rc<World>, CliError>, household: &str) -> Response {
    let Ok(id) = household.parse::<u64>() else {
        return Response::error(404, "not_in_house");
    };
    match world.and_then(|world| crate::house_answer(&world, id)) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => Response::error(404, "not_in_house"),
        Err(error) => Response::error(500, error.code),
    }
}

/// `GET /api/runs/{run}/why/{entry}` (ADR-0021, addendum of 2026-11-03): the entry from the
/// run's domain log, if it is a choice, recomputed; an idle entry with whether a later choice of
/// its person follows it in the log (version 4: if none does, "Warum?" adds the comparison at
/// the clock), and any choice so with its person's reserve at the clock (version 9); a time-filler with the entry that ended it and the choice after that in its
/// second, if the log has them (version 6, ADR-0026, addendum of 2026-11-04 on comfort and the
/// time-filler).
fn why(dir: &Path, world: impl FnOnce() -> Result<Rc<World>, CliError>, entry: &str) -> Response {
    use sim_core::log::{LogPayload, Subject};
    let no_choice = || Response::error(404, "not_a_choice");
    let Ok(id) = entry.parse::<u64>() else {
        return no_choice();
    };
    let log = dir.join(files::EVENTS);
    let (line, end) = match find_entry(&log, id) {
        Err(()) => return Response::error(500, "events_invalid"),
        Ok(None) => return no_choice(),
        Ok(Some(found)) => found,
    };
    let Ok(found) = serde_json::from_str::<sim_core::log::LogEntry>(&line) else {
        return Response::error(500, "events_invalid");
    };
    let latest = match (&found.payload, found.subjects.as_slice()) {
        (LogPayload::Idle { .. } | LogPayload::ActionStarted { .. }, [Subject::Sim(sim)]) => {
            match later_choice(&log, end, *sim) {
                Err(()) => return Response::error(500, "events_invalid"),
                Ok(later) => !later,
            }
        }
        _ => false,
    };
    let ended = match (&found.payload, found.subjects.as_slice()) {
        (LogPayload::ActionStarted { filler: true, .. }, [Subject::Sim(sim)]) => {
            match filler_ended(&log, end, *sim) {
                Err(()) => return Response::error(500, "events_invalid"),
                Ok(ended) => ended,
            }
        }
        _ => None,
    };
    let ended = (ended.as_ref()).map(|(ended, next)| crate::Ended {
        ended,
        next: next.as_ref(),
    });
    let (rows, history) = match &found.payload {
        LogPayload::Idle { .. } | LogPayload::ActionStarted { .. } => {
            let choice_at = end.saturating_sub(u64::try_from(line.len()).unwrap_or(0));
            let rows = match company_before(&log, choice_at, &found) {
                Ok(rows) => rows,
                Err(()) => return Response::error(500, "events_invalid"),
            };
            // Bond lines can stand long before the choice. The company row itself stays in the
            // short window above.
            let history = if rows.is_empty() {
                Vec::new()
            } else {
                match social_prefix(&log, end) {
                    Ok(history) => history,
                    Err(()) => return Response::error(500, "events_invalid"),
                }
            };
            (rows, history)
        }
        _ => (Vec::new(), Vec::new()),
    };
    match world().and_then(|world| {
        crate::why_answer(
            &world,
            &found,
            id,
            latest,
            ended,
            |object| {
                (rows.iter())
                    .find(|row| row.object == object)
                    .map(|row| row.company - row.avoid)
                    .unwrap_or(0)
            },
            crate::social_view::CompanyAt {
                rows: &rows,
                history: &history,
            },
        )
    }) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => no_choice(),
        Err(error) => Response::error(500, error.code),
    }
}

/// The end of a time-filler of `sim` begun before the byte `from` of the domain log at `path`:
/// the first entry `core:action_ended` about it alone in the complete lines from there, and the
/// choice about it alone that follows in that second, if one does. `None` if no such end stands
/// in the log yet; `Err` for a log it cannot read or a line it cannot parse.
fn filler_ended(
    path: &Path,
    from: u64,
    sim: sim_core::ids::SimId,
) -> Result<Option<(sim_core::log::LogEntry, Option<sim_core::log::LogEntry>)>, ()> {
    use sim_core::log::{LogPayload, Subject};
    use std::io::{BufRead, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(drop)?;
    file.seek(SeekFrom::Start(from)).map_err(drop)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut ended: Option<sim_core::log::LogEntry> = None;
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(drop)?;
        if !line.ends_with('\n') {
            return Ok(ended.map(|ended| (ended, None)));
        }
        let wanted = match ended {
            None => line.contains("\"core:action_ended\""),
            Some(_) => true,
        };
        if !wanted {
            continue;
        }
        let entry: sim_core::log::LogEntry = serde_json::from_str(&line).map_err(drop)?;
        if let Some(end) = ended.as_ref()
            && entry.t != end.t
        {
            return Ok(ended.map(|ended| (ended, None)));
        }
        if entry.subjects.as_slice() != [Subject::Sim(sim)] {
            continue;
        }
        match ended.take() {
            None if matches!(entry.payload, LogPayload::ActionEnded { .. }) => {
                ended = Some(entry);
            }
            None => {}
            Some(end) => match entry.payload {
                LogPayload::ActionStarted { .. } | LogPayload::Idle { .. } => {
                    return Ok(Some((end, Some(entry))));
                }
                _ => ended = Some(end),
            },
        }
    }
}

/// Whether a choice of `sim`, an entry `core:action_started` or `core:idle` about it alone,
/// stands in the complete lines of the domain log at `path` from the byte `from` on. Only lines
/// that name one of the two codes are parsed; `Err` for a log it cannot read or a line it
/// cannot parse.
fn later_choice(path: &Path, from: u64, sim: sim_core::ids::SimId) -> Result<bool, ()> {
    use sim_core::log::{LogPayload, Subject};
    use std::io::{BufRead, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(drop)?;
    file.seek(SeekFrom::Start(from)).map_err(drop)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(drop)?;
        if !line.ends_with('\n') {
            return Ok(false);
        }
        if !(line.contains("\"core:idle\"") || line.contains("\"core:action_started\"")) {
            continue;
        }
        let entry: sim_core::log::LogEntry = serde_json::from_str(&line).map_err(drop)?;
        let choice = matches!(
            entry.payload,
            LogPayload::Idle { .. } | LogPayload::ActionStarted { .. }
        );
        if choice && entry.subjects.as_slice() == [Subject::Sim(sim)] {
            return Ok(true);
        }
    }
}

/// How many bytes of the domain log the search for an entry reads one line after the other.
const SEARCH_WINDOW: u64 = 1 << 16;

/// The `core:company` rows of the choice that starts at byte `choice_start`, from the last such
/// entry about the same person in the same second within `SEARCH_WINDOW` before it. Empty when
/// the choice weighed no company. `Err` for a log it cannot read or a line it cannot parse.
fn company_before(
    path: &Path,
    choice_start: u64,
    choice: &sim_core::log::LogEntry,
) -> Result<Vec<sim_core::log::Company>, ()> {
    use sim_core::log::LogPayload;
    use std::io::{Read, Seek, SeekFrom};
    if choice_start == 0 {
        return Ok(Vec::new());
    }
    let from = choice_start.saturating_sub(SEARCH_WINDOW);
    let mut file = std::fs::File::open(path).map_err(drop)?;
    file.seek(SeekFrom::Start(from)).map_err(drop)?;
    let mut buf = vec![0_u8; usize::try_from(choice_start - from).map_err(drop)?];
    file.read_exact(&mut buf).map_err(drop)?;
    let start = if from == 0 {
        0
    } else {
        match buf.iter().position(|byte| *byte == b'\n') {
            Some(at) => at + 1,
            None => return Ok(Vec::new()),
        }
    };
    let text = std::str::from_utf8(&buf[start..]).map_err(drop)?;
    let mut rows = Vec::new();
    for line in text.split_inclusive('\n') {
        if !line.ends_with('\n') || !line.contains("\"core:company\"") {
            continue;
        }
        let entry: sim_core::log::LogEntry = serde_json::from_str(line).map_err(drop)?;
        if entry.t == choice.t
            && entry.subjects == choice.subjects
            && let LogPayload::Company { objects } = entry.payload
        {
            rows = objects;
        }
    }
    Ok(rows)
}

/// Bond, belief and feeling lines in the domain log at `path` from the start through byte
/// `until` (the byte after the choice). `core:unbelieved` is not a belief. Feeling lines let
/// "Warum?" replay the temper at the choice (ADR-0032 section 7). `Err` for a log it cannot
/// read or a line it cannot parse.
fn social_prefix(path: &Path, until: u64) -> Result<Vec<sim_core::log::LogEntry>, ()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(drop)?;
    let len = until.min(file.metadata().map_err(drop)?.len());
    let mut buf = vec![0_u8; usize::try_from(len).map_err(drop)?];
    if len > 0 {
        file.read_exact(&mut buf).map_err(drop)?;
    }
    let text = std::str::from_utf8(&buf).map_err(drop)?;
    let mut entries = Vec::new();
    for line in text.split_inclusive('\n') {
        if !line.ends_with('\n') {
            break;
        }
        let bond = line.contains("\"core:bond_");
        let believed = line.contains("\"core:believed\"");
        let felt = line.contains("\"core:felt\"");
        let moved = line.contains("\"core:moved_out\"") || line.contains("\"core:moved_in\"");
        if !bond && !believed && !felt && !moved {
            continue;
        }
        entries.push(serde_json::from_str(line).map_err(drop)?);
    }
    Ok(entries)
}

/// The line of the domain log at `path` with the entry `id` and the byte after it: only a
/// complete line (the command line appends whole lines, a last one without its line end it is
/// still writing), found by halving, since the ids grow along the log (review: no scan from the
/// start per request). `Err` for a log it cannot read, or whose ids do not grow around the
/// entry.
fn find_entry(path: &Path, id: u64) -> Result<Option<(String, u64)>, ()> {
    use std::io::{BufRead, Seek, SeekFrom};
    let file = std::fs::File::open(path).map_err(drop)?;
    let len = file.metadata().map_err(drop)?.len();
    let mut reader = std::io::BufReader::new(file);
    // The first complete line that starts at or after `at`, with its start.
    let mut line_from = |at: u64| -> Result<Option<(u64, String)>, ()> {
        let mut skipped = Vec::new();
        let start = if at == 0 {
            reader.seek(SeekFrom::Start(0)).map_err(drop)?;
            0
        } else {
            reader.seek(SeekFrom::Start(at - 1)).map_err(drop)?;
            let rest = reader.read_until(b'\n', &mut skipped).map_err(drop)?;
            at - 1 + u64::try_from(rest).map_err(drop)?
        };
        let mut line = String::new();
        reader.read_line(&mut line).map_err(drop)?;
        Ok(line.ends_with('\n').then_some((start, line)))
    };
    let (mut low, mut high) = (0, len);
    while high - low > SEARCH_WINDOW {
        let middle = low + (high - low) / 2;
        match line_from(middle)? {
            Some((start, line)) if leading_id(&line).ok_or(())? < id => {
                low = start + u64::try_from(line.len()).map_err(drop)?;
            }
            _ => high = middle,
        }
    }
    let mut at = low;
    let mut before = None;
    while let Some((start, line)) = line_from(at)? {
        let here = leading_id(&line).ok_or(())?;
        if before.is_some_and(|before| before >= here) {
            return Err(());
        }
        if here > id {
            return Ok(None);
        }
        at = start + u64::try_from(line.len()).map_err(drop)?;
        if here == id {
            // The next entry, if written, has a larger id.
            let next = line_from(at)?.map(|(_, next)| leading_id(&next));
            return match next {
                Some(Some(next)) if next <= id => Err(()),
                Some(None) => Err(()),
                _ => Ok(Some((line, at))),
            };
        }
        before = Some(here);
    }
    Ok(None)
}

/// Events per page (ADR-0021 section 2).
pub const EVENTS_PER_PAGE: usize = 500;

/// `GET /api/runs/{run}/census`: the rows of `census.csv` as JSON, each cell an integer.
/// The complete lines of `text`: the command line appends whole lines, so a last line
/// without its line end is one it is still writing, and is left out.
fn complete_lines(text: &str) -> impl Iterator<Item = &str> {
    let written = text.rfind('\n').map_or("", |end| &text[..end]);
    written.split('\n').filter(|line| !line.is_empty())
}

/// A cell of the census: a signed or an unsigned integer.
fn cell(text: &str) -> Option<Value> {
    (text.parse::<i64>().map(Value::from).ok())
        .or_else(|| text.parse::<u64>().map(Value::from).ok())
}

fn census(dir: &Path) -> Response {
    let invalid = || Response::error(500, "census_invalid");
    let Ok(text) = std::fs::read_to_string(dir.join(files::CENSUS)) else {
        return invalid();
    };
    let mut lines = complete_lines(&text);
    let Some(head) = lines.next() else {
        return invalid();
    };
    let columns: Vec<&str> = head.split(',').collect();
    let mut rows = Vec::new();
    for line in lines {
        let cells: Option<Vec<Value>> = line.split(',').map(cell).collect();
        match cells {
            Some(cells) if cells.len() == columns.len() => rows.push(cells),
            _ => return invalid(),
        }
    }
    let body =
        json!({"format": "simagent.view.census", "version": 1, "columns": columns, "rows": rows});
    Response::new(200, JSON, format!("{body}\n").into_bytes())
}

/// The filters of `GET /api/runs/{run}/events`: after an id, a window of seconds, a sim.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EventQuery {
    pub after: Option<u64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub subject: Option<u64>,
}

/// The query of the events route: `after`, `from`, `to` and `subject`, each at most once,
/// each a decimal integer; anything else is `None`.
pub fn event_query(query: &str) -> Option<EventQuery> {
    let mut found = EventQuery::default();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=')?;
        let digits = !value.is_empty()
            && (value.strip_prefix('-').unwrap_or(value).bytes()).all(|b| b.is_ascii_digit());
        if !digits {
            return None;
        }
        let fresh = match key {
            "after" => found.after.replace(value.parse().ok()?).is_none(),
            "from" => found.from.replace(value.parse().ok()?).is_none(),
            "to" => found.to.replace(value.parse().ok()?).is_none(),
            "subject" => found.subject.replace(value.parse().ok()?).is_none(),
            _ => false,
        };
        if !fresh {
            return None;
        }
    }
    Some(found)
}

/// Whether an entry of the domain log passes the filters.
fn passes(entry: &Value, query: &EventQuery) -> Option<bool> {
    let id = entry["id"].as_u64()?;
    let t = entry["t"].as_i64()?;
    let subjects = entry["subjects"].as_array()?;
    let about = |sim: u64| subjects.iter().any(|s| s["sim"].as_u64() == Some(sim));
    Some(
        query.after.is_none_or(|after| id > after)
            && query.from.is_none_or(|from| t >= from)
            && query.to.is_none_or(|to| t <= to)
            && query.subject.is_none_or(about),
    )
}

/// The id of an entry from the start of its line, `{"id":<n>,`, as the command line writes
/// it; `None` if the line does not start so.
fn leading_id(line: &str) -> Option<u64> {
    let rest = line.strip_prefix("{\"id\":")?;
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..digits]
        .parse()
        .ok()
        .filter(|_| rest[digits..].starts_with(','))
}

/// `GET /api/runs/{run}/events`: the entries of `events.ndjson` that pass the filters, in
/// the order of the file, each with the core's calendar date of its second, at most
/// [`EVENTS_PER_PAGE`]; `next` is the id of the last one if
/// more follow (the next page asks `after` it). The log is read as a stream and only up to
/// the first entry beyond the page; an entry at or before `after` is passed by its id
/// without being parsed. Only complete lines count (see [`complete_lines`]). A `core:felt`
/// also carries `before`, the four channels just before that entry (ADR-0032 section 7).
fn events(
    dir: &Path,
    query: &str,
    feelings: Option<&sim_core::social::data::FeelingRules>,
) -> Response {
    use std::io::BufRead;
    let Some(query) = event_query(query) else {
        return Response::error(400, "bad_query");
    };
    let invalid = || Response::error(500, "events_invalid");
    let Ok(file) = std::fs::File::open(dir.join(files::EVENTS)) else {
        return invalid();
    };
    let mut reader = std::io::BufReader::new(file);
    let mut page = Vec::new();
    let mut more = false;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Err(_) => return invalid(),
            Ok(_) if !line.ends_with('\n') => break,
            Ok(_) => {}
        }
        let before = (query.after.zip(leading_id(&line))).is_some_and(|(after, id)| id <= after);
        if before {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            return invalid();
        };
        match passes(&entry, &query) {
            None => return invalid(),
            Some(false) => {}
            Some(true) if page.len() == EVENTS_PER_PAGE => {
                more = true;
                break;
            }
            Some(true) => {
                let t = entry["t"].as_i64().and_then(|t| SimTime::try_from(t).ok());
                let Some(t) = t else {
                    return invalid();
                };
                page.push(json!({"entry": entry, "date": date(t)}));
            }
        }
    }
    if let Some(rules) = feelings
        && let Err(()) = attach_before(dir, &mut page, rules)
    {
        return invalid();
    }
    let next = (page.last())
        .filter(|_| more)
        .map(|last| last["entry"]["id"].clone())
        .unwrap_or(Value::Null);
    let body =
        json!({"format": "simagent.view.events", "version": 2, "events": page, "next": next});
    Response::new(200, JSON, format!("{body}\n").into_bytes())
}

/// The channels just before each `core:felt` on `page`, from the feeling lines up to the
/// latest such entry. Pages without one stay as they are. `Err` for a log it cannot read or
/// a feeling line it cannot parse.
fn attach_before(
    dir: &Path,
    page: &mut [Value],
    rules: &sim_core::social::data::FeelingRules,
) -> Result<(), ()> {
    use std::io::BufRead;
    let mut latest = None;
    for item in page.iter() {
        if item["entry"]["payload"].get("core:felt").is_none() {
            continue;
        }
        let id = item["entry"]["id"].as_u64().ok_or(())?;
        latest = Some(latest.map_or(id, |so_far: u64| so_far.max(id)));
    }
    let Some(latest) = latest else {
        return Ok(());
    };
    let file = std::fs::File::open(dir.join(files::EVENTS)).map_err(drop)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut history = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(drop)?;
        if !line.ends_with('\n') {
            break;
        }
        if leading_id(&line).is_some_and(|id| id > latest) {
            break;
        }
        let felt = line.contains("\"core:felt\"");
        let moved = line.contains("\"core:moved_out\"") || line.contains("\"core:moved_in\"");
        if !felt && !moved {
            continue;
        }
        history.push(serde_json::from_str(&line).map_err(drop)?);
    }
    let before = crate::social_view::channels_before(rules, &history);
    for item in page.iter_mut() {
        let Some(id) = item["entry"]["id"].as_u64() else {
            continue;
        };
        let Some(channels) = before.get(&id) else {
            continue;
        };
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        object.insert(
            "before".to_owned(),
            json!({
                "joy": channels[0],
                "anger": channels[1],
                "sadness": channels[2],
                "fear": channels[3],
            }),
        );
    }
    Ok(())
}

/// `simagent view <runs> [--port N] [--web DIR]`: prints its address, then serves until the
/// process ends.
pub fn view(runs: &Path, web: &Path, port: u16) -> Result<(), CliError> {
    let kind = runs.metadata().map_err(|e| CliError::io(runs, &e))?;
    if !kind.is_dir() {
        return Err(CliError::new(
            "usage",
            format!("{} is no directory", runs.display()),
        ));
    }
    let mut found = BTreeMap::new();
    web_files(web, web, &mut found).map_err(|e| {
        CliError::new(
            "web_missing",
            format!("{} (build it with cargo xtask web)", e.detail),
        )
    })?;
    if !found.contains_key("index.html") {
        return Err(CliError::new(
            "web_missing",
            format!(
                "{} holds no index.html (build it with cargo xtask web)",
                web.display()
            ),
        ));
    }
    let settings = runs.join(crate::data::SETTINGS_FILE);
    let (data, folder) = if settings.is_file() {
        eprintln!("view: settings {}", settings.display());
        let text = files::read_text(&settings)?;
        let values = crate::data::settings_values(&text)?;
        (crate::data::data_with_settings(&text)?, Some(values))
    } else {
        (crate::data::core_data()?, None)
    };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|e| CliError::new("port_unavailable", format!("127.0.0.1:{port}: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CliError::new("port_unavailable", e.to_string()))?
        .port();
    let mut server = Server {
        runs: runs.to_path_buf(),
        data,
        folder,
        own: RefCell::new(BTreeMap::new()),
        port,
        files: found,
        open: BTreeMap::new(),
        failed: BTreeSet::new(),
        loaded: RefCell::new(None),
    };
    // A run a process left between two stops goes back to its last stop before the first
    // request (ADR-0021 section 2, addendum 2026-10-27); a run another process holds stays
    // its, and a run that does not go back is reported by the open as before. Each goes back
    // on the data it opens with (ADR-0021, addendum of 2026-11-05).
    for name in server.run_names().unwrap_or_default() {
        let dir = runs.join(&name);
        let Ok(_lock) = RunLock::acquire(&dir) else {
            continue;
        };
        let Ok(data) = server.data_of(&dir) else {
            continue;
        };
        if let Ok(Some(cut)) = session::recover_with(&dir, &data) {
            let t = cut.closed_through;
            eprintln!(
                "view: run {name} went back to its last stop at {} (year {}): cut {} events, {} census rows, {} inputs",
                t.as_secs(),
                calendar(t).year,
                cut.events,
                cut.census,
                cut.inputs
            );
        }
    }
    let mut out = std::io::stdout();
    writeln!(out, "http://127.0.0.1:{port}/")
        .and_then(|()| out.flush())
        .map_err(|e| CliError::new("io", e.to_string()))?;
    let mut failures = 0;
    for stream in listener.incoming() {
        let Some(next) = after_accept(failures, stream.is_ok()) else {
            let error = stream.err().map(|e| e.to_string()).unwrap_or_default();
            return Err(CliError::new("accept_failed", error));
        };
        match stream {
            Ok(stream) => server.serve(stream),
            Err(error) => eprintln!("view: cannot accept a connection: {error}"),
        }
        failures = next;
    }
    Ok(())
}

/// Failures to accept a connection in a row after which the server stops.
const ACCEPT_FAILURES: u32 = 10;

/// The failures in a row after one more attempt to accept, or `None` when the server
/// stops: a failure may pass (the client went away), many in a row end the server.
fn after_accept(failures: u32, accepted: bool) -> Option<u32> {
    if accepted {
        Some(0)
    } else if failures + 1 < ACCEPT_FAILURES {
        Some(failures + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The search for an entry by halving (review): a log of 20 000 entries with the odd ids
    /// 1 to 39 999, far beyond the window; each id found with the byte after its line, or not,
    /// a last line not yet ended left out, a doubled id and ids that do not grow around the
    /// entry refused.
    #[test]
    fn an_entry_is_found_by_halving() {
        let base = std::env::temp_dir().join(format!("simagent-find-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("a directory");
        let log = base.join("events.ndjson");
        let line = |id: u64| {
            format!(
                "{{\"id\":{id},\"t\":0,\"payload\":\"{}\"}}\n",
                "x".repeat(40)
            )
        };
        // The line of id k ends after the lines of the ids 1, 3, …, k.
        let found = |id: u64| {
            let lines: String = (0..=(id - 1) / 2).map(|n| line(2 * n + 1)).collect();
            Ok(Some((line(id), u64::try_from(lines.len()).expect("fits"))))
        };
        let lines: String = (0..20_000).map(|n| line(2 * n + 1)).collect();
        assert!(u64::try_from(lines.len()).expect("fits") > 10 * SEARCH_WINDOW);
        std::fs::write(&log, &lines).expect("written");
        for id in [1, 3, 2_001, 19_999, 20_001, 39_997, 39_999] {
            assert_eq!(find_entry(&log, id), found(id), "{id}");
        }
        for id in [0, 2, 20_000, 39_998, 40_000, 1 << 40] {
            assert_eq!(find_entry(&log, id), Ok(None), "{id}");
        }
        let unended = line(40_001);
        std::fs::write(&log, format!("{lines}{}", unended.trim_end())).expect("written");
        assert_eq!(find_entry(&log, 40_001), Ok(None));
        assert_eq!(find_entry(&log, 39_999), found(39_999));
        std::fs::write(&log, format!("{lines}{}", line(39_999))).expect("written");
        assert_eq!(find_entry(&log, 39_999), Err(()));
        let shuffled = lines.replacen(&line(20_001), &line(20_005), 1);
        std::fs::write(&log, shuffled).expect("written");
        // 19 999, 20 005, 20 003: the entry 20 005 is followed by a smaller id.
        assert_eq!(find_entry(&log, 20_005), Err(()));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Joy 800 at the joy rate of 300 a day is 500 a day later, and that is what `before` says.
    #[test]
    fn a_felt_entry_carries_the_channels_from_before_it() {
        use sim_core::social::data::SocialData;
        let raw: SocialData =
            serde_json::from_str(include_str!("../../../data/social/social.json"))
                .expect("the file");
        let rules = raw.checked().expect("checked").feelings;
        let base = std::env::temp_dir().join(format!("simagent-felt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("a directory");
        let felt = |id: u64, t: i64, joy: u16| {
            format!(
                "{{\"id\":{id},\"t\":{t},\"subjects\":[{{\"sim\":1}}],\"causes\":[{{\"rule\":\"core:test\"}}],\"payload\":{{\"core:felt\":{{\"joy\":{joy},\"anger\":0,\"sadness\":0,\"fear\":0,\"temper\":0}}}}}}\n"
            )
        };
        let log = format!("{}{}", felt(1, 0, 800), felt(2, 86_400, 500));
        std::fs::write(base.join("events.ndjson"), log).expect("written");
        let response = events(&base, "", Some(&rules));
        assert_eq!(response.status, 200);
        let page: Value = serde_json::from_slice(&response.body).expect("json");
        let events = page["events"].as_array().expect("events");
        assert_eq!(
            events[0]["before"],
            json!({"joy": 0, "anger": 0, "sadness": 0, "fear": 0})
        );
        assert_eq!(
            events[1]["before"],
            json!({"joy": 500, "anger": 0, "sadness": 0, "fear": 0})
        );
        assert_eq!(page["version"], json!(2));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A founder in a world without a pool, for one or two years.
    const TINY: &str = r#"{"format_version":5,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}],"couples":[],"capacity":null,"pool":null},"until":62899199}"#;

    /// K12 of the third playtest: the server keeps the world it loaded last, the same while
    /// the run's save stays the same, and loads again once the save changed or another
    /// run was loaded since.
    #[test]
    fn a_world_is_loaded_again_only_when_its_save_changes() {
        let base = std::env::temp_dir().join(format!("simagent-loaded-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("makes");
        let scenario = base.join("tiny.json");
        std::fs::write(&scenario, TINY).expect("writes");
        for (name, until) in [("one", "31449599"), ("two", "62899199")] {
            let args = ["simagent", "run", "--scenario"].map(std::ffi::OsString::from);
            let out = base.join(name).into_os_string();
            let rest = [
                scenario.clone().into_os_string(),
                "--out".into(),
                out,
                "--until".into(),
                until.into(),
            ];
            crate::run_cli(args.into_iter().chain(rest)).expect("runs");
        }
        let server = Server {
            runs: base.clone(),
            data: crate::data::core_data().expect("data"),
            folder: None,
            own: RefCell::new(BTreeMap::new()),
            port: 0,
            files: BTreeMap::new(),
            open: BTreeMap::new(),
            failed: BTreeSet::new(),
            loaded: RefCell::new(None),
        };
        let (one, two) = (base.join("one"), base.join("two"));
        let at = |secs: i64| SimTime::from_secs(secs);
        let first = server.world_of(&one).expect("loads");
        assert_eq!(first.closed_through(), at(31_449_599));
        assert!(Rc::ptr_eq(&first, &server.world_of(&one).expect("loads")));
        assert_eq!(
            server.world_of(&two).expect("loads").closed_through(),
            at(62_899_199)
        );
        let again = server.world_of(&one).expect("loads");
        assert!(!Rc::ptr_eq(&first, &again));
        assert_eq!(again.closed_through(), at(31_449_599));
        std::fs::copy(two.join(files::SAVE), one.join(files::SAVE)).expect("copies");
        assert_eq!(
            server.world_of(&one).expect("loads").closed_through(),
            at(62_899_199)
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Astra's review of G2: a failed fork's directory goes, its repro package is kept
    /// beside the runs without replacing anything there; where it cannot be, the directory
    /// stays with it.
    #[test]
    fn a_failed_fork_goes_but_its_repro_package() {
        let base = std::env::temp_dir().join(format!("simagent-discard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let made = |name: &str, repro: bool| {
            let dir = base.join(name);
            std::fs::create_dir_all(&dir).expect("makes");
            std::fs::write(dir.join("events.ndjson"), b"{}\n").expect("writes");
            if repro {
                std::fs::write(dir.join(crate::repro::REPRO), name.as_bytes()).expect("writes");
            }
            dir
        };
        let plain = made("plain", false);
        discard(&plain, &base.join("plain.repro.json"));
        assert!(!plain.exists() && !base.join("plain.repro.json").exists());
        let failed = made("failed", true);
        discard(&failed, &base.join("failed.repro.json"));
        assert_eq!(
            std::fs::read(base.join("failed.repro.json")).expect("kept"),
            b"failed"
        );
        assert!(!failed.exists());
        let taken = made("taken", true);
        std::fs::write(base.join("taken.repro.json"), b"theirs").expect("writes");
        discard(&taken, &base.join("taken.repro.json"));
        assert_eq!(
            std::fs::read(base.join("taken.repro.json")).expect("theirs"),
            b"theirs"
        );
        assert_eq!(
            std::fs::read(taken.join(crate::repro::REPRO)).expect("kept"),
            b"taken"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ten_failures_to_accept_in_a_row_end_the_server() {
        assert_eq!(after_accept(7, true), Some(0));
        assert_eq!(after_accept(0, false), Some(1));
        assert_eq!(after_accept(8, false), Some(9));
        // The tenth failure in a row.
        assert_eq!(after_accept(9, false), None);
        assert_eq!(after_accept(9, true), Some(0));
    }

    /// Near the end of the time range every step stays in range and the limit refuses it;
    /// a decade stops at its first year end beyond the horizon (one year before the end).
    #[test]
    fn a_step_past_the_horizon_stays_in_range_and_is_refused() {
        use sim_core::time::{MAX_ABS_SECS, YEAR};
        let year = YEAR.as_secs();
        let horizon = SimTime::from_secs(MAX_ABS_SECS - year);
        for step in [Step::Days28, Step::Year, Step::Decade] {
            let target = step_target(horizon, step);
            let refused = session::check_limit(horizon, target).err();
            assert_eq!(refused.map(|e| e.code), Some("until_beyond_horizon"));
        }
        let end = SimTime::from_secs(MAX_ABS_SECS - 1);
        assert_eq!(step_target(horizon, Step::Decade), end);
        // From the end of the year three before the last: two year ends within, the third
        // beyond.
        let before = SimTime::from_secs(MAX_ABS_SECS - 3 * year - 1);
        assert_eq!(step_target(before, Step::Decade), end);
        assert_eq!(
            step_target(before, Step::Year),
            SimTime::from_secs(MAX_ABS_SECS - 2 * year - 1)
        );
    }
}
