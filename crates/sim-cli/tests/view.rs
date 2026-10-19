//! `simagent view` as a process (ADR-0021 section 1 and its Abnahmetest): bound to
//! 127.0.0.1 only, the limits and checks of each request, the file list of `web/dist`, the
//! run list and the manifests. The expected answers are the files the command line wrote
//! and the rules of the ADR.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const TINY_SCENARIO: &str = r#"{"format_version":5,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}],"couples":[],"capacity":null,"pool":null},"until":31449599}"#;

/// A fresh directory for one test under cargo's temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("view")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("removes the old scratch directory");
    }
    fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    fs::write(path, bytes).expect("written");
}

/// The runs `town-1` and `fork.b` (scenario tiny), beside a directory without a manifest,
/// a hidden run, a run with a broken manifest and a plain file; and a `web/dist` with three
/// files and a symbolic link.
fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let runs = dir.join("runs");
    let scenario = dir.join("tiny.json");
    write(&scenario, TINY_SCENARIO.as_bytes());
    for run in ["town-1", "fork.b", ".hidden", "broken"] {
        let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
            .args(["run", "--scenario"])
            .arg(&scenario)
            .arg("--out")
            .arg(runs.join(run))
            .stdout(Stdio::null())
            .status()
            .expect("runs");
        assert!(status.success(), "{run}");
    }
    write(&runs.join("broken/manifest.json"), b"{");
    write(&runs.join("notes/readme.txt"), b"no run");
    write(&runs.join("plain.txt"), b"no run");
    let web = dir.join("web");
    write(&web.join("index.html"), b"<!doctype html><p>index</p>");
    write(&web.join("main.js"), b"export const a = 1;\n");
    write(&web.join("vendor/x.mjs"), b"export const x = 2;\n");
    write(&dir.join("secret.txt"), b"secret");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("secret.txt"), web.join("link.js")).expect("a link");
    (runs, web)
}

/// The server as a child process; killed when dropped.
struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(runs: &Path, web: &Path) -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .arg("view")
        .arg(runs)
        .args(["--port", "0", "--web"])
        .arg(web)
        .stdout(Stdio::piped())
        .spawn()
        .expect("starts");
    let mut line = String::new();
    let stdout = child.stdout.take().expect("its output");
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("its address");
    let port = line
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/\n"))
        .and_then(|port| port.parse().ok())
        .unwrap_or_else(|| panic!("the address line: {line:?}"));
    Server { child, port }
}

/// The status, the head and the body of the answer to `raw`.
fn send(port: u16, raw: &[u8]) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream.write_all(raw).expect("sends");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let end = (answer.windows(4).position(|w| w == b"\r\n\r\n")).expect("a head");
    let head = String::from_utf8(answer[..end].to_vec()).expect("text");
    let status = head[9..12].parse().expect("a status");
    (status, head, answer[end + 4..].to_vec())
}

fn get(port: u16, path: &str) -> (u16, String, Vec<u8>) {
    send(
        port,
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes(),
    )
}

fn code(body: &[u8]) -> String {
    let error: Value = serde_json::from_slice(body).expect("JSON");
    assert_eq!(
        (&error["format"], &error["version"]),
        (&"simagent.view.error".into(), &1.into())
    );
    error["code"].as_str().expect("a code").to_owned()
}

#[test]
fn the_files_of_web_dist_and_nothing_else() {
    let (runs, web) = fixture("files");
    let server = start(&runs, &web);
    let port = server.port;
    for (path, file, content_type) in [
        ("/", "index.html", "text/html; charset=utf-8"),
        ("/main.js", "main.js", "text/javascript; charset=utf-8"),
        (
            "/vendor/x.mjs",
            "vendor/x.mjs",
            "text/javascript; charset=utf-8",
        ),
    ] {
        let (status, head, body) = get(port, path);
        assert_eq!(status, 200, "{path}");
        assert!(
            head.contains(&format!("\r\nContent-Type: {content_type}\r\n")),
            "{head}"
        );
        for header in [
            "Connection: close",
            "Cache-Control: no-store",
            "X-Content-Type-Options: nosniff",
            "X-Frame-Options: DENY",
        ] {
            assert!(head.contains(header), "{path}: {header}");
        }
        assert_eq!(body, fs::read(web.join(file)).expect("the file"), "{path}");
    }
    for path in [
        "/link.js",
        "/secret.txt",
        "/../secret.txt",
        "/vendor/../main.js",
        "/./main.js",
        "/%2e%2e/secret.txt",
        "/main%2ejs",
        "/vendor\\x.mjs",
        "//main.js",
        "/vendor/",
        "/vendor",
        "/index.html/",
        "/main.js?v=1",
        "/nothing",
        "/api",
        "/api/run",
        "/api/runs/town-1/x",
    ] {
        let (status, _, body) = get(port, path);
        assert_eq!(
            (status, code(&body)),
            (404, "not_found".to_owned()),
            "{path}"
        );
    }
}

#[test]
fn each_request_is_checked_before_it_is_answered() {
    let (runs, web) = fixture("checks");
    let server = start(&runs, &web);
    let port = server.port;
    let refused = |raw: String| {
        let (status, head, body) = send(port, raw.as_bytes());
        (status, code(&body), head)
    };
    let cases = [
        ("GET / HTTP/1.1\r\n\r\n".to_owned(), 400, "host_missing"),
        (
            "GET / HTTP/1.1\r\nHost: example.org\r\n\r\n".to_owned(),
            421,
            "host_foreign",
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", port + 1),
            421,
            "host_foreign",
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
            400,
            "host_missing",
        ),
        (
            format!(
                "POST /api/runs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
            ),
            405,
            "method_not_allowed",
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 3\r\n\r\nabc"),
            400,
            "bad_request",
        ),
        (
            format!(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nTransfer-Encoding: chunked\r\n\r\n"
            ),
            400,
            "bad_request",
        ),
        (
            format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n",
                1024 * 1024 + 1
            ),
            413,
            "body_too_large",
        ),
        (
            format!(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Long: {}\r\n\r\n",
                "a".repeat(8 * 1024)
            ),
            431,
            "head_too_large",
        ),
        ("GET /\r\n\r\n".to_owned(), 400, "bad_request"),
        (
            format!("GET / HTTP/2\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
            400,
            "bad_request",
        ),
        (
            format!("get / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
            400,
            "bad_request",
        ),
        (
            format!("GET x HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
            400,
            "bad_request",
        ),
    ];
    for (raw, status, expected) in cases {
        let (found, code, head) = refused(raw.clone());
        assert_eq!((found, code.as_str()), (status, expected), "{raw:?}");
        if status == 405 {
            assert!(head.contains("\r\nAllow: GET"), "{head}");
        }
    }
    let (status, _, _) = send(
        port,
        format!("GET / HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n").as_bytes(),
    );
    assert_eq!(status, 200);
}

/// A request that does not come within five seconds is answered with 408, and the server
/// serves the next one.
#[test]
fn a_slow_request_times_out() {
    let (runs, web) = fixture("slow");
    let server = start(&runs, &web);
    let began = Instant::now();
    let (status, _, body) = send(server.port, b"GET / HTTP/1.1\r\n");
    assert_eq!((status, code(&body).as_str()), (408, "read_timeout"));
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_millis(4900) && waited < Duration::from_secs(9),
        "{waited:?}"
    );
    assert_eq!(get(server.port, "/").0, 200);
}

/// The server listens on 127.0.0.1 only: not on the IPv6 loopback, which a server bound to
/// every address would also answer.
#[test]
fn it_is_bound_to_127_0_0_1_only() {
    let (runs, web) = fixture("bound");
    let server = start(&runs, &web);
    assert!(TcpStream::connect(("::1", server.port)).is_err());
    assert_eq!(get(server.port, "/").0, 200);
}

#[test]
fn the_runs_and_their_manifests() {
    let (runs, web) = fixture("runs");
    let server = start(&runs, &web);
    let (status, head, body) = get(server.port, "/api/runs");
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    let list: Value = serde_json::from_slice(&body).expect("JSON");
    let manifest = |run: &str| -> Value {
        serde_json::from_slice(&fs::read(runs.join(run).join("manifest.json")).expect("read"))
            .expect("JSON")
    };
    assert_eq!(
        list,
        serde_json::json!({
            "format": "simagent.view.runs",
            "version": 1,
            "runs": [
                {"run": "broken", "error": "manifest_invalid"},
                {"run": "fork.b", "manifest": manifest("fork.b")},
                {"run": "town-1", "manifest": manifest("town-1")}
            ]
        })
    );
    for run in ["town-1", "fork.b"] {
        let (status, _, body) = get(server.port, &format!("/api/runs/{run}"));
        assert_eq!(status, 200);
        assert_eq!(
            body,
            fs::read(runs.join(run).join("manifest.json")).expect("read")
        );
    }
    for run in [".hidden", "notes", "plain.txt", "nothing"] {
        let (status, _, body) = get(server.port, &format!("/api/runs/{run}"));
        assert_eq!(
            (status, code(&body).as_str()),
            (404, "unknown_run"),
            "{run}"
        );
    }
}

#[test]
fn it_refuses_to_start_without_web_dist() {
    let (runs, web) = fixture("no-web");
    fs::remove_file(web.join("index.html")).expect("removed");
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .arg("view")
        .arg(&runs)
        .args(["--port", "0", "--web"])
        .arg(&web)
        .output()
        .expect("runs");
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).expect("text");
    assert!(error.starts_with("error: web_missing: "), "{error}");
}
