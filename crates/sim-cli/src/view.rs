//! `simagent view` (ADR-0021 sections 1 and 2): a local server for the browser. It listens
//! on 127.0.0.1 only, answers one connection at a time, serves the files `web/dist` held
//! when it started, reads the run directories below its directory, and moves their clocks
//! and takes their inputs through the `Session` of the command line. It computes nothing:
//! every number it sends comes from the files of the command line or from the core.

mod http;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sim_core::input::Input;
use sim_core::params::ParamChange;
use sim_core::time::{DAY, SimTime, calendar};

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
    port: u16,
    /// The files of `web/dist` at start: relative path to type and bytes.
    files: BTreeMap<String, (&'static str, Vec<u8>)>,
    /// The runs a write opened, by name; each keeps its lock until the server ends.
    open: BTreeMap<String, Open>,
    /// The runs whose write failed: their files may stand between two writes, so the
    /// server reads none of them until a write opens the run whole again.
    failed: BTreeSet<String>,
}

/// A run the server writes (ADR-0021 section 1): opened as `run --resume` opens it, its
/// lock held while it is open.
struct Open {
    session: Session,
    scenario: Scenario,
    _lock: RunLock,
}

/// The one step of `POST …/advance` (ADR-0021 section 2).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Days28,
    Year,
    Decade,
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
/// of the year that begins or continues after `closed`; ten such year ends. A decade that
/// would pass the horizon stops at its first year end beyond, which `check_limit` refuses.
pub fn step_target(closed: SimTime, step: Step) -> SimTime {
    match step {
        Step::Days28 => closed + DAY * 28,
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
            ["api", "runs", _, "events"] | ["api", "runs", _, "persons", _] => {}
            ["api", ..] if request.query.is_some() => return Response::error(400, "bad_query"),
            _ => {}
        }
        match segments.as_slice() {
            ["api", "runs"] if request.method == "POST" => self.create(&request.body),
            ["api", "runs"] => self.runs(),
            ["api", "runs", run] => self.manifest(run),
            ["api", "runs", run, "pedigree"] => self.read(run, |dir| self.pedigree(dir)),
            ["api", "runs", run, "persons", sim] => match person_query(request.query.as_deref()) {
                Some(to) => self.read(run, |dir| person(dir, sim, to)),
                None => Response::error(400, "bad_query"),
            },
            ["api", "runs", run, "census"] => self.read(run, census),
            ["api", "runs", run, "world"] => self.read(run, world),
            ["api", "runs", a, "diff", b] => {
                self.read(a, |one| self.read(b, |other| diff((a, b), (one, other))))
            }
            ["api", "runs", run, "fork"] => self.fork(run, &request.body),
            ["api", "runs", run, "events"] => {
                let query = request.query.as_deref().unwrap_or("");
                self.read(run, |dir| events(dir, query))
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
            match open(&self.runs.join(run)) {
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
    /// must be free in the runs directory; a seed may replace the template's.
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
            crate::resolve(file, crate::data::core_data)
        });
        let scenario = match scenario {
            Ok(scenario) => scenario,
            Err(error) => return failure(&error),
        };
        // From here on the directory is this server's: a failure marks it failed.
        let made = (|| {
            // The run stands at the template's end, as `run --scenario` leaves it.
            let mut session = crate::begin(&dir, &scenario, scenario.until)?;
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

    /// `GET /api/runs`: the runs in name order, each with its manifest and the core's
    /// calendar date of its last closed second, or why it has none.
    fn runs(&self) -> Response {
        let Some(names) = self.run_names() else {
            return Response::error(500, "runs_unreadable");
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
                match read {
                    Some((manifest, closed)) => {
                        json!({"run": name, "manifest": manifest, "date": date(closed)})
                    }
                    None => json!({"run": name, "error": "manifest_invalid"}),
                }
            })
            .collect();
        let body = json!({"format": "simagent.view.runs", "version": 1, "runs": runs});
        Response::new(200, JSON, format!("{body}\n").into_bytes())
    }

    /// `POST /api/runs/{run}/fork` (the addendum of ADR-0021 section 2, G2): the fork of
    /// `run` made like `simagent fork` into a new run of the list, its name free as for
    /// `POST /api/runs`. A run the server holds open is closed first, which lets its lock
    /// go; its files are whole since its last write. The fork's directory is the request's
    /// only under its lock and only if it is still absent there; the lock is held through
    /// the fork and, if it fails, through taking away what it made.
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
        let made = crate::fork_into(&parent_dir, fork.from_year, fork.changes, &dir);
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

    /// The answer of `read` on the directory of `run`, if it is one of the list exactly.
    fn read(&self, run: &str, read: impl FnOnce(&Path) -> Response) -> Response {
        match self.listed(run) {
            Ok(dir) => read(&dir),
            Err(refused) => refused,
        }
    }

    /// `GET /api/runs/{run}/pedigree`: byte for byte what `simagent export` writes.
    fn pedigree(&self, dir: &Path) -> Response {
        match crate::pedigree_text(dir) {
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
fn date(t: SimTime) -> Value {
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
fn open(dir: &Path) -> Result<Open, CliError> {
    let lock = RunLock::acquire(dir)?;
    let scenario = crate::resumed_scenario(dir)?;
    let session = Session::resume(dir)?;
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

/// A sim of a path or a query: decimal, without a leading zero or sign; `None` otherwise.
fn sim_of(text: &str) -> Option<sim_core::ids::SimId> {
    (text.parse::<u64>().ok())
        .filter(|_| !text.starts_with('0') && !text.starts_with('+'))
        .and_then(|n| sim_core::ids::SimId::try_from(n).ok())
}

/// `GET /api/runs/{run}/persons/{sim}`: the presentation state of the core for one person,
/// with `?to=` also what the second sim is to it.
fn person(dir: &Path, sim: &str, to: Option<&str>) -> Response {
    let unknown = || Response::error(404, "unknown_sim");
    let Some(id) = sim_of(sim) else {
        return unknown();
    };
    let other = match to.map(sim_of) {
        None => None,
        Some(Some(other)) => Some(other),
        Some(None) => return unknown(),
    };
    match crate::person_answer(dir, id, other) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => unknown(),
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
/// with its date.
fn diff((a, b): (&str, &str), (one, other): (&Path, &Path)) -> Response {
    let compared = (|| -> Result<Value, CliError> {
        let (one, other) = (
            crate::load_world(one)?.pedigree(),
            crate::load_world(other)?.pedigree(),
        );
        serde_json::to_value(sim_core::diff::diff(&one, &other))
            .map_err(|error| CliError::new("io", error.to_string()))
    })();
    let compared = match compared {
        Ok(compared) => compared,
        Err(error) => return Response::error(500, error.code),
    };
    let body = json!({
        "format": "simagent.view.diff",
        "version": 1,
        "a": a,
        "b": b,
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

/// `GET /api/runs/{run}/world`: where the switch of the hereditary diseases stands.
fn world(dir: &Path) -> Response {
    match crate::world_answer(dir) {
        Ok(answer) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Err(error) => Response::error(500, error.code),
    }
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
/// without being parsed. Only complete lines count (see [`complete_lines`]).
fn events(dir: &Path, query: &str) -> Response {
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
    let next = (page.last())
        .filter(|_| more)
        .map(|last| last["entry"]["id"].clone())
        .unwrap_or(Value::Null);
    let body =
        json!({"format": "simagent.view.events", "version": 2, "events": page, "next": next});
    Response::new(200, JSON, format!("{body}\n").into_bytes())
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
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|e| CliError::new("port_unavailable", format!("127.0.0.1:{port}: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CliError::new("port_unavailable", e.to_string()))?
        .port();
    let mut server = Server {
        runs: runs.to_path_buf(),
        port,
        files: found,
        open: BTreeMap::new(),
        failed: BTreeSet::new(),
    };
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
