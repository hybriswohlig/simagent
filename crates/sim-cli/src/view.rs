//! `simagent view` (ADR-0021 sections 1 and 2): a local server for the browser. It listens
//! on 127.0.0.1 only, answers one connection at a time, serves the files `web/dist` held
//! when it started, and reads the run directories below its directory. It computes nothing:
//! every number it sends comes from the files of the command line or from the core.

mod http;

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::CliError;
use crate::files;

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
    fn answer(&self, request: &Request) -> Response {
        let host = request.header("host");
        let own = [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ];
        match host {
            None => return Response::error(400, "host_missing"),
            Some(host) if !own.iter().any(|o| o == host) => {
                return Response::error(421, "host_foreign");
            }
            Some(_) => {}
        }
        if request.method != "GET" {
            let mut refused = Response::error(405, "method_not_allowed");
            refused.allow = Some("GET");
            return refused;
        }
        if !request.body.is_empty() {
            return Response::error(400, "bad_request");
        }
        if request.path == "/" {
            return self.file("index.html", request);
        }
        if !plain(&request.path) {
            return Response::error(404, "not_found");
        }
        let segments: Vec<&str> = request.path.split('/').skip(1).collect();
        match segments.as_slice() {
            ["api", "runs"] => self.runs(),
            ["api", "runs", run] => self.manifest(run),
            ["api", ..] => Response::error(404, "not_found"),
            _ => self.file(&request.path[1..], request),
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

    /// The manifest of a run as the command line wrote it, if it reads as one.
    fn read_manifest(&self, run: &str) -> Result<Vec<u8>, &'static str> {
        let bytes = std::fs::read(self.runs.join(run).join(files::MANIFEST))
            .map_err(|_| "manifest_invalid")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "manifest_invalid")?;
        files::read_manifest(text).map_err(|e| e.code)?;
        Ok(bytes)
    }

    /// `GET /api/runs`: the runs in name order, each with its manifest or why it has none.
    fn runs(&self) -> Response {
        let Some(names) = self.run_names() else {
            return Response::error(500, "runs_unreadable");
        };
        let runs: Vec<Value> = (names.iter())
            .map(|name| {
                let manifest = self.read_manifest(name).ok();
                let manifest =
                    manifest.and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
                match manifest {
                    Some(manifest) => json!({"run": name, "manifest": manifest}),
                    None => json!({"run": name, "error": "manifest_invalid"}),
                }
            })
            .collect();
        let body = json!({"format": "simagent.view.runs", "version": 1, "runs": runs});
        Response::new(200, JSON, format!("{body}\n").into_bytes())
    }

    /// `GET /api/runs/{run}`: the manifest of the command line as it lies in the directory.
    /// The name must be one of the list exactly (a file system may ignore case).
    fn manifest(&self, run: &str) -> Response {
        let known = self
            .run_names()
            .is_some_and(|names| names.iter().any(|n| n == run));
        if !known {
            return Response::error(404, "unknown_run");
        }
        match self.read_manifest(run) {
            Ok(bytes) => Response::new(200, JSON, bytes),
            Err(code) => Response::error(500, code),
        }
    }

    fn serve(&self, mut stream: TcpStream) {
        let response = match http::read_request(&mut stream) {
            Ok(request) => self.answer(&request),
            Err(refusal) => refusal,
        };
        let _ = http::write_response(&mut stream, &response);
        let _ = stream.shutdown(Shutdown::Both);
    }
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
    let server = Server {
        runs: runs.to_path_buf(),
        port,
        files: found,
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
    } else if failures < ACCEPT_FAILURES {
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
        assert_eq!(after_accept(9, false), Some(10));
        assert_eq!(after_accept(10, false), None);
        assert_eq!(after_accept(10, true), Some(0));
    }
}
