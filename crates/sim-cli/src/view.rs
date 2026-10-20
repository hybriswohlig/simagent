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
        let writes = matches!(segments.as_slice(), ["api", "runs", _, "advance" | "input"]);
        let method = if writes { "POST" } else { "GET" };
        if request.method != method {
            let mut refused = Response::error(405, "method_not_allowed");
            refused.allow = Some(method);
            return refused;
        }
        if writes {
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
            // A query belongs to the events alone.
            ["api", "runs", _, "events"] => {}
            ["api", ..] if request.query.is_some() => return Response::error(400, "bad_query"),
            _ => {}
        }
        match segments.as_slice() {
            ["api", "runs"] => self.runs(),
            ["api", "runs", run] => self.manifest(run),
            ["api", "runs", run, "pedigree"] => self.read(run, |dir| self.pedigree(dir)),
            ["api", "runs", run, "persons", sim] => self.read(run, |dir| person(dir, sim)),
            ["api", "runs", run, "census"] => self.read(run, census),
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

    /// The answer of `read` on the directory of `run`, if it is one of the list exactly.
    fn read(&self, run: &str, read: impl FnOnce(&Path) -> Response) -> Response {
        match self.run_names() {
            None => Response::error(500, "runs_unreadable"),
            Some(names) if !names.iter().any(|n| n == run) => Response::error(404, "unknown_run"),
            Some(_) if self.failed.contains(run) => Response::error(500, "run_failed"),
            Some(_) => read(&self.runs.join(run)),
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

/// `GET /api/runs/{run}/persons/{sim}`: the presentation state of the core for one person.
fn person(dir: &Path, sim: &str) -> Response {
    let id = sim
        .parse::<u64>()
        .ok()
        .filter(|_| !sim.starts_with('0') && !sim.starts_with('+'));
    let Some(id) = id.and_then(|n| sim_core::ids::SimId::try_from(n).ok()) else {
        return Response::error(404, "unknown_sim");
    };
    match crate::person_answer(dir, id) {
        Ok(Some(answer)) => Response::new(200, JSON, format!("{answer}\n").into_bytes()),
        Ok(None) => Response::error(404, "unknown_sim"),
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
