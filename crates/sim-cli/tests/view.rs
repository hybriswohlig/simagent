//! `simagent view` as a process (ADR-0021 section 1 and its Abnahmetest): bound to
//! 127.0.0.1 only, the limits and checks of each request, the file list of `web/dist`, the
//! run list and the manifests. The expected answers are the files the command line wrote
//! and the rules of the ADR.

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{Holder, free};
use serde_json::{Value, json};
use sim_cli::lock::{RunLock, lock_path};
use sim_core::data_id::DataId;
use sim_core::house::choice::mood;
use sim_core::lineage::LineageKey;
use sim_core::time::{SimTime, calendar};

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
    for run in [
        "town-1",
        "fork.b",
        ".hidden",
        "broken",
        "shaped",
        "linked-manifest",
    ] {
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
    write(
        &runs.join("shaped/manifest.json"),
        b"{\"format_version\":5}\n",
    );
    #[cfg(unix)]
    {
        let manifest = runs.join("linked-manifest/manifest.json");
        fs::remove_file(&manifest).expect("removed");
        std::os::unix::fs::symlink(runs.join("town-1/manifest.json"), manifest).expect("a link");
        std::os::unix::fs::symlink(runs.join("town-1"), runs.join("linked-run")).expect("a link");
    }
    write(&runs.join("notes/readme.txt"), b"no run");
    write(&runs.join("plain.txt"), b"no run");
    let web = dir.join("web");
    write(&web.join("index.html"), b"<!doctype html><p>index</p>");
    write(&web.join("main.js"), b"export const a = 1;\n");
    write(&web.join("vendor/x.mjs"), b"export const x = 2;\n");
    write(&web.join("vendor/data.json"), b"{}\n");
    write(&web.join("vendor/x.png"), b"png");
    write(&web.join("api/runs.json"), b"{}\n");
    write(&web.join("\u{e4}.js"), b"export const u = 3;\n");
    write(&web.join(".hidden.js"), b"export const h = 4;\n");
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
/// The status line of each status the server sends.
const STATUS_LINES: [(u16, &str); 12] = [
    (200, "HTTP/1.1 200 OK"),
    (400, "HTTP/1.1 400 Bad Request"),
    (403, "HTTP/1.1 403 Forbidden"),
    (404, "HTTP/1.1 404 Not Found"),
    (405, "HTTP/1.1 405 Method Not Allowed"),
    (408, "HTTP/1.1 408 Request Timeout"),
    (409, "HTTP/1.1 409 Conflict"),
    (413, "HTTP/1.1 413 Content Too Large"),
    (415, "HTTP/1.1 415 Unsupported Media Type"),
    (421, "HTTP/1.1 421 Misdirected Request"),
    (431, "HTTP/1.1 431 Request Header Fields Too Large"),
    (500, "HTTP/1.1 500 Internal Server Error"),
];

/// The status, the head and the body of the answer to `raw`; the status line must be the
/// one of its status. A server that never answers fails the test after 15 s.
fn send(port: u16, raw: &[u8]) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("a timeout");
    stream.write_all(raw).expect("sends");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let end = (answer.windows(4).position(|w| w == b"\r\n\r\n")).expect("a head");
    let head = String::from_utf8(answer[..end].to_vec()).expect("text");
    let status = head[9..12].parse().expect("a status");
    let line = STATUS_LINES
        .iter()
        .find(|(s, _)| *s == status)
        .expect("a known status");
    assert_eq!(head.lines().next(), Some(line.1));
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
        ("/vendor/data.json", "vendor/data.json", "application/json"),
        ("/vendor/x.png", "vendor/x.png", "application/octet-stream"),
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
        "/api/runs.json",
        "/api/runs/",
        "/api/runs/.",
        "/api/runs/..",
        "/api/runs/%74own-1",
        "/api/runs/town\\-1",
        "/\u{e4}.js",
        "/%C3%A4.js",
        "/.hidden.js",
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
            403,
            "origin_foreign",
        ),
        (
            format!(
                "PUT /api/runs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
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
    let host = format!("Host: 127.0.0.1:{port}\r\n");
    let malformed = [
        format!("GET / HTTP/1.0\r\n{host}\r\n"),
        format!("GET / HTTP/1.1\r\n{host}Content-Length: 0\r\nContent-Length: 0\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}Transfer-Encoding: a\r\nTransfer-Encoding: b\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X-A: a\nb\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X-A: a\rb\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X-A: a\u{7}\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}\r\nGET / HTTP/1.1\r\n{host}\r\n"),
        format!("POST / HTTP/1.1\r\n{host}Content-Length: 1\r\n\r\nab"),
        format!("GET / HTTP/1.1\r\n{host}: x\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X A: x\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X@A: x\r\n\r\n"),
        format!("GET / HTTP/1.1\r\n{host}X(A): x\r\n\r\n"),
    ];
    let cases =
        (cases.into_iter()).chain(malformed.into_iter().map(|raw| (raw, 400, "bad_request")));
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
    let tab = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-A: a\tb\r\n\r\n");
    assert_eq!(send(port, tab.as_bytes()).0, 200);
}

/// The limits hold at their bounds: a head of exactly 8 KiB is read, one byte more is not;
/// a body of exactly 1 MiB is read (and the method then refused), one byte more is not.
#[test]
fn the_limits_hold_at_their_bounds() {
    let (runs, web) = fixture("bounds");
    let server = start(&runs, &web);
    let port = server.port;
    let head = |size: usize| {
        let start = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Pad: ");
        let pad = "a".repeat(size - start.len());
        format!("{start}{pad}\r\n\r\n")
    };
    assert_eq!(head(8192).len(), 8192 + 4);
    assert_eq!(send(port, head(8192).as_bytes()).0, 200);
    assert_eq!(send(port, head(8193).as_bytes()).0, 431);
    // A head that does not end is refused once it is too long, not when time runs out.
    let began = Instant::now();
    let endless = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Pad: {}",
        "a".repeat(9000)
    );
    assert_eq!(send(port, endless.as_bytes()).0, 431);
    assert!(began.elapsed() < Duration::from_secs(4));
    let post = |size: usize| {
        let mut raw =
            format!("POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {size}\r\n\r\n")
                .into_bytes();
        raw.resize(raw.len() + size, b'x');
        raw
    };
    assert_eq!(send(port, &post(1024 * 1024)).0, 405);
    let over = format!(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n",
        1024 * 1024 + 1
    );
    assert_eq!(send(port, over.as_bytes()).0, 413);
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

/// A client that stops reading a large answer holds the one thread for one write time at
/// most; then the next request is served.
#[test]
fn a_client_that_stops_reading_lets_go() {
    let (runs, web) = fixture("stalled");
    write(&web.join("big.bin"), &vec![b'x'; 64 * 1024 * 1024]);
    let server = start(&runs, &web);
    let port = server.port;
    let mut stalled = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    let request = format!("GET /big.bin HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    stalled.write_all(request.as_bytes()).expect("sends");
    let began = Instant::now();
    assert_eq!(get(port, "/").0, 200);
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_secs(4) && waited < Duration::from_secs(12),
        "{waited:?}"
    );
    drop(stalled);
}

/// The server listens on 127.0.0.1 only: not on the IPv6 loopback, and not on the
/// machine's own IPv4 address in its network, which a server bound to every address would
/// also answer. That address is the source a UDP socket would use towards a documentation
/// address (no packet leaves); without a network the second check has nothing to try.
#[test]
fn it_is_bound_to_127_0_0_1_only() {
    let (runs, web) = fixture("bound");
    let server = start(&runs, &web);
    assert!(TcpStream::connect(("::1", server.port)).is_err());
    let own = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.0.2.1:9").map(|()| socket))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.ip());
    if let Ok(ip) = own
        && !ip.is_loopback()
        && !ip.is_unspecified()
    {
        assert!(TcpStream::connect((ip, server.port)).is_err(), "{ip}");
    }
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
    // Both runs end at the last second of year 0, 364 days of 86 400 s less one: the core's
    // calendar (ADR-0002, every unit from 0) makes it day 363, month 12 and its day 27,
    // week 51 and its day 6, season 3, second 86 399 of its day.
    let last_of_year_0 = serde_json::json!({
        "year": 0, "day_of_year": 363, "month": 12, "day_of_month": 27, "week": 51,
        "weekday": 6, "season": 3, "second_of_day": 86_399,
    });
    assert_eq!(
        list,
        serde_json::json!({
            "format": "simagent.view.runs",
            "version": 1,
            "runs": [
                {"run": "broken", "error": "manifest_invalid"},
                {"run": "fork.b", "manifest": manifest("fork.b"), "date": last_of_year_0},
                {"run": "shaped", "error": "manifest_invalid"},
                {"run": "town-1", "manifest": manifest("town-1"), "date": last_of_year_0}
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
    for run in [
        ".hidden",
        "notes",
        "plain.txt",
        "nothing",
        "TOWN-1",
        "linked-run",
        "linked-manifest",
    ] {
        let (status, _, body) = get(server.port, &format!("/api/runs/{run}"));
        assert_eq!(
            (status, code(&body).as_str()),
            (404, "unknown_run"),
            "{run}"
        );
    }
    for run in ["broken", "shaped"] {
        let (status, _, body) = get(server.port, &format!("/api/runs/{run}"));
        assert_eq!(
            (status, code(&body).as_str()),
            (500, "manifest_invalid"),
            "{run}"
        );
    }
}

#[test]
fn it_refuses_to_start_without_web_dist() {
    let (runs, web) = fixture("no-web");
    fs::remove_file(web.join("index.html")).expect("removed");
    let mut child = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .arg("view")
        .arg(&runs)
        .args(["--port", "0", "--web"])
        .arg(&web)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("runs");
    // A server that started would print its address; the refusal prints nothing there.
    let mut line = String::new();
    let stdout = child.stdout.take().expect("its output");
    BufReader::new(stdout).read_line(&mut line).expect("reads");
    if !line.is_empty() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().expect("ends");
    assert_eq!(line, "");
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).expect("text");
    assert!(error.starts_with("error: web_missing: "), "{error}");
}

/// A run of `tests/scenarios/dummy.json` over 80 years (550 events, so the events have two
/// pages), exported, beside the fixture's runs; one tiny run gets a broken census, events
/// and save.
fn long_fixture(name: &str) -> (PathBuf, PathBuf) {
    let (runs, web) = fixture(name);
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let long = runs.join("long");
    for args in [
        vec![
            "run".into(),
            "--scenario".into(),
            scenario.into_os_string(),
            "--out".into(),
            long.clone().into_os_string(),
            "--until".into(),
            "2515967999".into(),
        ],
        vec!["export".into(), long.clone().into_os_string()],
    ] {
        let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
            .args(&args)
            .stdout(Stdio::null())
            .status()
            .expect("runs");
        assert!(status.success(), "{args:?}");
    }
    write(&runs.join("fork.b/census.csv"), b"year,living\n0,x\n");
    write(&runs.join("broken/census.csv"), b"year,living\n0,1,2\n");
    // A census and a log the command line is still writing: the last line has no end yet.
    let partial = runs.join("town-1");
    write(
        &partial.join("census.csv"),
        b"year,living,lived\n0,3,18446744073709551615\n1,4",
    );
    let entry =
        |id: u64| format!("{{\"id\":{id},\"t\":0,\"subjects\":[],\"causes\":[],\"payload\":{{}}}}");
    let log = format!("{}\n{}\n{}", entry(1), entry(2), &entry(3)[..10]);
    write(&partial.join("events.ndjson"), log.as_bytes());
    write(&runs.join("fork.b/events.ndjson"), b"{\n");
    write(&runs.join("fork.b/state.save"), b"no save");
    (runs, web)
}

fn json_of(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("JSON")
}

#[test]
fn the_pedigree_is_what_export_writes_and_the_census_is_the_file() {
    let (runs, web) = long_fixture("reads");
    let server = start(&runs, &web);
    let (status, head, body) = get(server.port, "/api/runs/long/pedigree");
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    assert_eq!(
        body,
        fs::read(runs.join("long/pedigree.json")).expect("exported")
    );

    let census = fs::read_to_string(runs.join("long/census.csv")).expect("the census");
    let mut lines = census.lines();
    let columns: Vec<&str> = lines.next().expect("a head").split(',').collect();
    let rows: Vec<Vec<i64>> = lines
        .map(|line| {
            line.split(',')
                .map(|cell| cell.parse().expect("an integer"))
                .collect()
        })
        .collect();
    // Until the last second of year 79: the years 0 to 79.
    assert_eq!(rows.len(), 80);
    let (status, _, body) = get(server.port, "/api/runs/long/census");
    assert_eq!(status, 200);
    assert_eq!(
        json_of(&body),
        serde_json::json!({"format": "simagent.view.census", "version": 1, "columns": columns, "rows": rows})
    );

    let (status, _, body) = get(server.port, "/api/runs/town-1/census");
    assert_eq!(status, 200);
    assert_eq!(
        json_of(&body),
        serde_json::json!({"format": "simagent.view.census", "version": 1, "columns": ["year", "living", "lived"], "rows": [[0, 3, 18_446_744_073_709_551_615_u64]]})
    );
    let (status, _, body) = get(server.port, "/api/runs/town-1/events");
    assert_eq!(status, 200);
    let page = json_of(&body);
    let ids: Vec<&Value> = (page["events"].as_array().expect("events").iter())
        .map(|item| &item["entry"]["id"])
        .collect();
    assert_eq!(ids, [&Value::from(1), &Value::from(2)]);
    // Second 0 is the first of everything (ADR-0002, every unit from 0).
    assert_eq!(
        page["events"][0]["date"],
        serde_json::json!({
            "year": 0, "day_of_year": 0, "month": 0, "day_of_month": 0, "week": 0,
            "weekday": 0, "season": 0, "second_of_day": 0,
        })
    );

    for (path, code) in [
        ("/api/runs/fork.b/census", "census_invalid"),
        ("/api/runs/broken/census", "census_invalid"),
        ("/api/runs/fork.b/events", "events_invalid"),
        ("/api/runs/fork.b/pedigree", "save_rejected"),
    ] {
        let (status, _, body) = get(server.port, path);
        assert_eq!((status, code_of(&body)), (500, code.to_owned()), "{path}");
    }
    for route in ["pedigree", "census", "events"] {
        for run in ["nothing", "LONG", ".hidden", "linked-run"] {
            let (status, _, body) = get(server.port, &format!("/api/runs/{run}/{route}"));
            assert_eq!(
                (status, code_of(&body)),
                (404, "unknown_run".to_owned()),
                "{run}/{route}"
            );
        }
    }
}

fn code_of(body: &[u8]) -> String {
    json_of(body)["code"].as_str().expect("a code").to_owned()
}

/// The pages of the events, read after the id of each last entry, are the file; the
/// filters keep exactly the entries of their window or subject.
#[test]
fn the_events_come_in_pages_and_filtered() {
    let (runs, web) = long_fixture("events");
    let server = start(&runs, &web);
    let file: Vec<Value> = fs::read_to_string(runs.join("long/events.ndjson"))
        .expect("the log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect();
    assert_eq!(file.len(), 550);
    let page = |query: &str| {
        let (status, _, body) = get(server.port, &format!("/api/runs/long/events{query}"));
        assert_eq!(status, 200, "{query}");
        let page = json_of(&body);
        assert_eq!(
            (&page["format"], &page["version"]),
            (&"simagent.view.events".into(), &2.into())
        );
        // Each item is a line of the file with the core's calendar date of its second; the
        // rest of this test reads the lines.
        let items = page["events"].as_array().expect("events");
        let entries: Vec<Value> = (items.iter())
            .map(|item| {
                assert_eq!(item.as_object().map(serde_json::Map::len), Some(2));
                let t = item["entry"]["t"].as_i64().expect("a time");
                assert_eq!(item["date"], date(t), "{query}");
                item["entry"].clone()
            })
            .collect();
        serde_json::json!({"events": entries, "next": page["next"]})
    };
    let first = page("");
    let events = first["events"].as_array().expect("events");
    assert_eq!(events.len(), 500);
    assert_eq!(first["next"], file[499]["id"]);
    let second = page(&format!("?after={}", first["next"]));
    assert_eq!(second["next"], Value::Null);
    let mut all = events.clone();
    all.extend(second["events"].as_array().expect("events").iter().cloned());
    assert_eq!(all, file);

    let about = |entry: &Value, sim: u64| {
        (entry["subjects"].as_array().expect("subjects"))
            .iter()
            .any(|s| s["sim"].as_u64() == Some(sim))
    };
    let expected: Vec<&Value> = file.iter().filter(|e| about(e, 1)).collect();
    assert!(!expected.is_empty());
    assert_eq!(
        page("?subject=1")["events"]
            .as_array()
            .expect("events")
            .iter()
            .collect::<Vec<_>>(),
        expected
    );
    let (from, to) = (314_496_000_i64, 943_487_999_i64);
    let window: Vec<&Value> = (file.iter())
        .filter(|e| (from..=to).contains(&e["t"].as_i64().expect("a time")))
        .collect();
    assert!(!window.is_empty() && window.len() < file.len());
    let found = page(&format!("?from={from}&to={to}"));
    assert_eq!(
        found["events"]
            .as_array()
            .expect("events")
            .iter()
            .collect::<Vec<_>>(),
        window
    );
    let both: Vec<&Value> = window.iter().copied().filter(|e| about(e, 1)).collect();
    let found = page(&format!("?to={to}&subject=1&from={from}"));
    assert_eq!(
        found["events"]
            .as_array()
            .expect("events")
            .iter()
            .collect::<Vec<_>>(),
        both
    );
    assert_eq!(page("?from=-5&to=-1")["events"], serde_json::json!([]));

    // The bound of a page: 499, 500 and 501 entries after a cursor.
    let ids: Vec<u64> = file
        .iter()
        .map(|e| e["id"].as_u64().expect("an id"))
        .collect();
    for (cursor, count, next) in [
        (50, 499, Value::Null),
        (49, 500, Value::Null),
        (48, 500, file[548]["id"].clone()),
    ] {
        let found = page(&format!("?after={}", ids[cursor]));
        let events = found["events"].as_array().expect("events");
        assert_eq!(
            (events.len(), &found["next"]),
            (count, &next),
            "after {}",
            ids[cursor]
        );
        assert_eq!(events[0], file[cursor + 1]);
    }
    let beyond = page(&format!("?after={}", ids[549] + 1000));
    assert_eq!(
        (&beyond["events"], &beyond["next"]),
        (&serde_json::json!([]), &Value::Null)
    );
    // A filtered window over more than a page: its pages are its entries.
    let late = file[520]["t"].as_i64().expect("a time");
    let window: Vec<&Value> = (file.iter())
        .filter(|e| e["t"].as_i64() <= Some(late))
        .collect();
    assert!(window.len() > 500);
    let first = page(&format!("?to={late}"));
    assert_eq!(first["next"], window[499]["id"]);
    let second = page(&format!("?to={late}&after={}", first["next"]));
    let mut pages: Vec<Value> = first["events"].as_array().expect("events").clone();
    pages.extend(second["events"].as_array().expect("events").iter().cloned());
    assert_eq!(pages.iter().collect::<Vec<_>>(), window);
    assert_eq!(second["next"], Value::Null);
    // Equal bounds keep the entries of that second; reversed bounds keep none.
    let t = file[100]["t"].as_i64().expect("a time");
    let at: Vec<&Value> = file.iter().filter(|e| e["t"].as_i64() == Some(t)).collect();
    let found = page(&format!("?from={t}&to={t}"));
    assert_eq!(
        found["events"]
            .as_array()
            .expect("events")
            .iter()
            .collect::<Vec<_>>(),
        at
    );
    assert_eq!(
        page(&format!("?from={t}&to={}", t - 1))["events"],
        serde_json::json!([])
    );
    // Empty pairs are nothing.
    let found = page(&format!("?&after={}&", ids[547]));
    assert_eq!(found["events"], serde_json::json!([file[548], file[549]]));

    for query in [
        "?after=x",
        "?after=1&after=2",
        "?foo=1",
        "?after",
        "?after=",
        "?after=1.5",
        "?subject=-1",
        "?after=1;from=2",
        "?from=1&from=2",
        "?to=1&to=2",
        "?subject=1&subject=2",
        "?after=18446744073709551616",
        "?from=-9223372036854775809",
        "?to=9223372036854775808",
        "?after=+1",
        "?after=-1",
        "?after=-",
        "?after=%31",
        "?After=1",
    ] {
        let (status, _, body) = get(server.port, &format!("/api/runs/long/events{query}"));
        assert_eq!(
            (status, code_of(&body)),
            (400, "bad_query".to_owned()),
            "{query}"
        );
    }
    for path in [
        "/api/runs?x=1",
        "/api/runs/long?x",
        "/api/runs/long/census?after=1",
    ] {
        let (status, _, body) = get(server.port, path);
        assert_eq!(
            (status, code_of(&body)),
            (400, "bad_query".to_owned()),
            "{path}"
        );
    }
}

/// A runs directory the server cannot read is its own failure, not an unknown run.
#[cfg(unix)]
#[test]
fn an_unreadable_runs_directory_is_a_failure() {
    use std::os::unix::fs::PermissionsExt;
    let (runs, web) = fixture("unreadable");
    let server = start(&runs, &web);
    fs::set_permissions(&runs, fs::Permissions::from_mode(0o000)).expect("closed");
    if fs::read_dir(&runs).is_ok() {
        // As root the mode does not hold: nothing to test.
        fs::set_permissions(&runs, fs::Permissions::from_mode(0o755)).expect("opened");
        return;
    }
    let found: Vec<(u16, String)> = ["/api/runs", "/api/runs/town-1", "/api/runs/town-1/census"]
        .iter()
        .map(|path| {
            let (status, _, body) = get(server.port, path);
            (status, code_of(&body))
        })
        .collect();
    fs::set_permissions(&runs, fs::Permissions::from_mode(0o755)).expect("opened");
    assert_eq!(found, vec![(500, "runs_unreadable".to_owned()); 3]);
}

/// The person route sends the core's presentation state of the save as it is, with format,
/// version and the contract version; a living and a dead person; unknown sims are 404.
#[test]
fn a_person_is_the_cores_presentation_state() {
    let (runs, web) = long_fixture("person");
    let server = start(&runs, &web);
    let pedigree: Value =
        serde_json::from_slice(&fs::read(runs.join("long/pedigree.json")).expect("read"))
            .expect("JSON");
    let persons = pedigree["persons"].as_array().expect("persons");
    let living = persons
        .iter()
        .find(|p| p["died"].is_null() && p["emigrated"].is_null())
        .expect("a living person");
    let dead = persons
        .iter()
        .find(|p| !p["died"].is_null())
        .expect("a dead person");
    let save = fs::read(runs.join("long/state.save")).expect("the save");
    let (world, _) = sim_core::world::World::load(&save, sim_cli::data::core_data().expect("data"))
        .expect("loads");
    for person in [living, dead] {
        let sim = person["sim"].as_u64().expect("a sim");
        let (status, head, body) = get(server.port, &format!("/api/runs/long/persons/{sim}"));
        assert_eq!(status, 200, "{sim}");
        assert!(head.contains("\r\nContent-Type: application/json\r\n"));
        let view = world
            .person_view(sim_core::ids::SimId::try_from(sim).expect("an id"))
            .expect("a view");
        let mut expected = serde_json::to_value(&view).expect("JSON");
        let fields = expected.as_object_mut().expect("an object");
        fields.insert("format".into(), "simagent.view.person".into());
        fields.insert("version".into(), 2.into());
        fields.insert(
            "contract_version".into(),
            sim_core::body_contract::CONTRACT_VERSION.into(),
        );
        let id = sim_core::ids::SimId::try_from(sim).expect("an id");
        let partners: Vec<u64> = (world.partner_candidates(id).iter())
            .map(|p| p.get())
            .collect();
        fields.insert("partner_candidates".into(), partners.into());
        let card = world.genetics_card(id).expect("a card");
        fields.insert("card".into(), serde_json::to_value(card).expect("JSON"));
        fields.insert(
            "ages".into(),
            serde_json::to_value(world.ages()).expect("JSON"),
        );
        // The pregnancy: the core's seconds, each with its date (checked as the other dates).
        let shown = json_of(&body);
        match world.pregnancy(id) {
            None => assert!(shown["pregnancy"].is_null(), "{sim}"),
            Some(pregnancy) => {
                assert_eq!(
                    shown["pregnancy"]["conceived"]["t"],
                    pregnancy.conceived.as_secs()
                );
                assert_eq!(shown["pregnancy"]["due"]["t"], pregnancy.due.as_secs());
                assert!(shown["pregnancy"]["due"]["date"]["year"].is_i64());
            }
        }
        fields.insert("pregnancy".into(), shown["pregnancy"].clone());
        // No input of "Zeit zu zweit" in this run: no window.
        fields.insert("time_together".into(), Value::Null);
        fields.insert(
            "traits".into(),
            serde_json::to_value(world.traits(id)).expect("JSON"),
        );
        // The origin: hair, eyes (each maybe with its copies) and skin always lead.
        let origin = world.origin(id).expect("an origin");
        let kinds: Vec<String> = serde_json::to_value(&origin)
            .expect("JSON")
            .as_array()
            .expect("lines")
            .iter()
            .map(|line| line["kind"].as_str().expect("a kind").to_owned())
            .filter(|kind| !kind.ends_with("_copies"))
            .collect();
        assert_eq!(kinds[..3], ["hair", "eyes", "skin"], "{sim}");
        fields.insert("origin".into(), serde_json::to_value(origin).expect("JSON"));
        assert_eq!(shown, expected, "{sim}");
    }
    let dead_sim = dead["sim"].as_u64().expect("a sim");
    let answer = json_of(&get(server.port, &format!("/api/runs/long/persons/{dead_sim}")).2);
    assert_eq!(answer["ended"], "death");
    assert_eq!(answer["shown"]["at"]["t"], dead["died"]);
    for sim in ["0", "01", "+1", "x", "99999", "18446744073709551616"] {
        let (status, _, body) = get(server.port, &format!("/api/runs/long/persons/{sim}"));
        assert_eq!(
            (status, code_of(&body)),
            (404, "unknown_sim".to_owned()),
            "{sim}"
        );
    }
    let (status, _, body) = get(server.port, "/api/runs/fork.b/persons/1");
    assert_eq!((status, code_of(&body)), (500, "save_rejected".to_owned()));
}

/// The persons of a run for finding them (the filter of the person list): every living
/// person in the order of the ages the person answer lists, with its body now and its looks
/// in words from the core; the colours, curls and freckles by hand from that body at the
/// bounds of the words.
#[test]
fn the_people_are_the_living_with_their_looks() {
    let (runs, web) = long_fixture("people");
    let server = start(&runs, &web);
    let save = fs::read(runs.join("long/state.save")).expect("the save");
    let (world, _) = sim_core::world::World::load(&save, sim_cli::data::core_data().expect("data"))
        .expect("loads");
    let (status, head, body) = get(server.port, "/api/runs/long/people");
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    let answer = json_of(&body);
    assert_eq!(answer["format"], "simagent.view.people");
    assert_eq!(answer["version"], 1);
    assert_eq!(
        answer["contract_version"],
        sim_core::body_contract::CONTRACT_VERSION
    );
    let people = answer["people"].as_array().expect("people");
    let living: Vec<u64> = world.ages().iter().map(|age| age.sim.get()).collect();
    assert!(!living.is_empty());
    let sims: Vec<u64> = people
        .iter()
        .map(|p| p["sim"].as_u64().expect("a sim"))
        .collect();
    assert_eq!(sims, living);
    fn word(x: &Value, bounds: [f64; 3], names: [&'static str; 4]) -> &'static str {
        let x = x.as_f64().expect("a number");
        names[bounds.iter().filter(|&&bound| x >= bound).count()]
    }
    for person in people {
        let id =
            sim_core::ids::SimId::try_from(person["sim"].as_u64().expect("a sim")).expect("an id");
        let view = world.person_view(id).expect("a view");
        assert_eq!(
            person["shown"],
            serde_json::to_value(&view.shown).expect("JSON")
        );
        let looks = &person["looks"];
        assert_eq!(*looks, serde_json::to_value(world.looks(id)).expect("JSON"));
        assert_eq!(person["pregnant"], world.pregnancy(id).is_some());
        assert_eq!(
            person["traits"],
            serde_json::to_value(world.traits(id)).expect("JSON")
        );
        let body = &person["shown"]["body"];
        if looks["hair"] != "red" {
            let hair = ["blond", "light_brown", "dark_brown", "black"];
            assert_eq!(
                looks["hair"],
                word(&body["hair_eumelanin"], [0.16, 0.36, 0.75], hair)
            );
        }
        let eyes = ["blue", "green", "hazel", "brown"];
        assert_eq!(
            looks["eyes"],
            word(&body["iris_melanin"], [0.20, 0.45, 0.60], eyes)
        );
        let skin = ["very_light", "light", "beige", "tanned"];
        assert_eq!(
            looks["skin"],
            word(&body["skin_eumelanin"], [0.05, 0.10, 0.18], skin)
        );
        let curls = ["straight", "wavy", "curly", "kinky"];
        assert_eq!(
            looks["curls"],
            word(&body["hair_curl"], [0.15, 0.40, 0.70], curls)
        );
        let freckles = ["none", "few", "clear", "many"];
        assert_eq!(
            looks["freckles"],
            word(&body["freckle_density"], [0.10, 0.30, 0.60], freckles)
        );
    }
    let (status, _, body) = get(server.port, "/api/runs/nothing/people");
    assert_eq!((status, code_of(&body)), (404, "unknown_run".to_owned()));
    let (status, _, body) = get(server.port, "/api/runs/fork.b/people");
    assert_eq!((status, code_of(&body)), (500, "save_rejected".to_owned()));
}

/// The possible children of a couple (Grok's round 44): two daughters and two sons from the
/// core, the same on every request, and asking changes nothing. Only a pair the core would
/// pair now has them (K7). By hand from the dummy run's golden pedigree, as in
/// `a_person_with_its_kinship_to_another`: sim 8, the one living woman, may pair with sim 4
/// alone; her father 2, her brother 10 and her son 13 are refused, sim 1 died, and a man as
/// the mother or an unknown sim has none either.
#[test]
fn a_couple_shows_four_possible_children() {
    let (runs, web) = fixture("children");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), runs.join("dummy").as_os_str()])
        .stdout(Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let server = start(&runs, &web);
    let save = fs::read(runs.join("dummy/state.save")).expect("the save");
    let (world, _) = sim_core::world::World::load(&save, sim_cli::data::core_data().expect("data"))
        .expect("loads");
    let before = world.state_hash();
    let path = "/api/runs/dummy/children/8/4";
    let (status, head, body) = get(server.port, path);
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    let answer = json_of(&body);
    assert_eq!(answer["format"], "simagent.view.children");
    assert_eq!(answer["version"], 1);
    let id = |sim: u64| sim_core::ids::SimId::try_from(sim).expect("an id");
    let expected = world.preview_children(id(8), id(4)).expect("children");
    assert_eq!(
        answer["children"],
        serde_json::to_value(&expected).expect("JSON")
    );
    let sexes: Vec<&str> = (answer["children"].as_array().expect("children").iter())
        .map(|child| child["sex"].as_str().expect("a sex"))
        .collect();
    assert_eq!(sexes, ["female", "female", "male", "male"]);
    assert_eq!(json_of(&get(server.port, path).2), answer);
    assert_eq!(world.state_hash(), before);
    for (mother, father) in [
        ("4", "8"),
        ("8", "99999"),
        ("x", "4"),
        ("8", "2"),
        ("8", "10"),
        ("8", "13"),
        ("1", "4"),
    ] {
        let path = format!("/api/runs/dummy/children/{mother}/{father}");
        let (status, _, body) = get(server.port, &path);
        assert_eq!(
            (status, code_of(&body)),
            (404, "no_children".to_owned()),
            "{path}"
        );
    }
    for (mother, father) in [(8, 2), (8, 10), (8, 13), (1, 4)] {
        assert_eq!(world.preview_children(id(mother), id(father)), None);
    }
}

/// What the people of a person's family tree are to it, in one answer (Grok's round 46, P3):
/// the kinship of the person route for each sim of `?to=`, in the order asked. By hand from
/// the dummy run's golden pedigree: sim 13 is the son of 8 and 4, and 8 the daughter of 1
/// and 2; the couple 8 and 4 is unrelated, so 13's parents share half and its grandparents
/// on its mother's side a quarter with it. An unknown sim in the list or the path is an
/// unknown sim; a query that is not exactly `to=` and sims with commas is refused.
#[test]
fn the_kin_of_a_family_tree() {
    let (runs, web) = fixture("kin");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), runs.join("dummy").as_os_str()])
        .stdout(Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let server = start(&runs, &web);
    let (status, head, body) = get(server.port, "/api/runs/dummy/persons/13/kin?to=8,4,1,2");
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    assert_eq!(
        json_of(&body),
        json!({
            "format": "simagent.view.kin",
            "version": 1,
            "kin": [
                {"to": 8, "word": "mother", "foster": null, "r_percent": 50.0},
                {"to": 4, "word": "father", "foster": null, "r_percent": 50.0},
                {"to": 1, "word": "grandmother", "foster": null, "r_percent": 25.0},
                {"to": 2, "word": "grandfather", "foster": null, "r_percent": 25.0}
            ]
        })
    );
    for path in [
        "/api/runs/dummy/persons/13/kin?to=8,99",
        "/api/runs/dummy/persons/99/kin?to=8",
        "/api/runs/dummy/persons/13/kin?to=08",
    ] {
        let (status, _, body) = get(server.port, path);
        assert_eq!(
            (status, code_of(&body)),
            (404, "unknown_sim".to_owned()),
            "{path}"
        );
    }
    for path in [
        "/api/runs/dummy/persons/13/kin",
        "/api/runs/dummy/persons/13/kin?to=",
        "/api/runs/dummy/persons/13/kin?to=8,",
        "/api/runs/dummy/persons/13/kin?to=8&to=4",
        "/api/runs/dummy/persons/13/kin?of=8",
    ] {
        let (status, _, body) = get(server.port, path);
        assert_eq!(
            (status, code_of(&body)),
            (400, "bad_query".to_owned()),
            "{path}"
        );
    }
}

/// With `?to=` the person route adds what the second sim is to the first (ADR-0021 section
/// 2), from the core: on the dummy run, by hand from its golden pedigree, sim 10 is the uncle
/// of sim 13 (φ 1/8) and sim 5 the foster mother of sim 3 (unrelated). The query is exactly
/// `to=` and digits; digits that name no sim of the run are an unknown sim, as in the path.
#[test]
fn a_person_with_its_kinship_to_another() {
    let (runs, web) = fixture("kinship");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), runs.join("dummy").as_os_str()])
        .stdout(Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let server = start(&runs, &web);
    let kinship = |path: &str| {
        let (status, _, body) = get(server.port, path);
        assert_eq!(status, 200, "{path}");
        json_of(&body)["kinship"].clone()
    };
    assert_eq!(
        kinship("/api/runs/dummy/persons/13?to=10"),
        json!({"to": 10, "word": "uncle", "foster": null, "r_percent": 25.0})
    );
    assert_eq!(
        kinship("/api/runs/dummy/persons/3?to=5"),
        json!({"to": 5, "word": "unrelated", "foster": "foster_mother", "r_percent": 0.0})
    );
    // The partners a pairing would be accepted with, by hand from the golden pedigree (as in
    // `tests/partners.rs`): sim 8, the one living woman, and her former partner, sim 4.
    let partners = |sim: u64| {
        json_of(&get(server.port, &format!("/api/runs/dummy/persons/{sim}")).2)["partner_candidates"]
            .clone()
    };
    assert_eq!(partners(8), json!([4]));
    assert_eq!(partners(4), json!([8]));
    assert_eq!(partners(13), json!([]));
    // The ages of the living at the run's last second 943 487 999, in whole years of
    // 31 449 600 seconds, by hand from the births of the golden pedigree (Grok's round 33, P3
    // and P4): 2 born −794 880 000, 4 −628 992 000, 8 41 126 400, 10 101 606 400, 12
    // 203 212 800, 13 662 860 800.
    let answer = json_of(&get(server.port, "/api/runs/dummy/persons/13").2);
    assert_eq!(answer["version"], 2);
    assert_eq!(
        answer["ages"],
        json!([
            {"sim": 2, "age_years": 55},
            {"sim": 4, "age_years": 49},
            {"sim": 8, "age_years": 28},
            {"sim": 10, "age_years": 26},
            {"sim": 12, "age_years": 23},
            {"sim": 13, "age_years": 8}
        ])
    );
    // Without a query the answer stays as it was.
    let (_, _, plain) = get(server.port, "/api/runs/dummy/persons/13");
    let plain = json_of(&plain);
    assert!(plain.get("kinship").is_none());
    let mut with = json_of(&get(server.port, "/api/runs/dummy/persons/13?to=10").2);
    with.as_object_mut().expect("an object").remove("kinship");
    assert_eq!(with, plain);
    for to in ["0", "01", "999", "18446744073709551616"] {
        let (status, _, body) = get(server.port, &format!("/api/runs/dummy/persons/13?to={to}"));
        assert_eq!(
            (status, code_of(&body)),
            (404, "unknown_sim".to_owned()),
            "{to}"
        );
    }
    for query in [
        "?to=+1",
        "?to=x",
        "?to=",
        "?to=-1",
        "?to=1&to=2",
        "?from=1",
        "?to",
        "?To=1",
        "?to=1&",
        "?",
        "?to=1;x=2",
        "?to=%31",
    ] {
        let (status, _, body) = get(server.port, &format!("/api/runs/dummy/persons/13{query}"));
        assert_eq!(
            (status, code_of(&body)),
            (400, "bad_query".to_owned()),
            "{query}"
        );
    }
}

/// Three founders, ten years (as in `cli.rs`): a woman (sim 1) and two men to pair.
const TEN_YEARS: &str = r#"{
  "format_version": 5,
  "start": {
    "world_seed": "0000000000000001",
    "founder_tag": "test:cli",
    "world_params": {"hereditary_diseases": false, "nutrition": []},
    "founders": [
      {"born": -943488000, "sex": "female", "alleles": [{"locus": "core:disease_ar", "copies": [0, 1]}]},
      {"born": -794880000, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [1, 0]}]},
      {"born": -1, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [1, 1]}]}
    ],
    "couples": [],
    "capacity": null,
    "pool": null
  },
  "until": 314495999
}"#;

/// A year and a day of the calendar (ADR-0002): 364 days of 86 400 s.
const Y: i64 = 31_449_600;
const DAY: i64 = 86_400;
const RUN_FILES: [&str; 6] = [
    "manifest.json",
    "scenario.resolved.json",
    "input.ndjson",
    "events.ndjson",
    "census.csv",
    "state.save",
];

/// `simagent args`: success, or the first line of its error.
fn simagent(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .output()
        .expect("runs simagent");
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8(output.stderr).expect("UTF-8");
    Err(stderr.lines().next().unwrap_or("").to_owned())
}

fn first_code(error: &str) -> &str {
    let rest = error.trim_start_matches("error: ");
    rest.split(':').next().unwrap_or("")
}

fn resume_until(dir: &Path, until: i64, inputs: Option<&Path>) -> Result<(), String> {
    let until = until.to_string();
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "run".as_ref(),
        "--resume".as_ref(),
        "--out".as_ref(),
        dir.as_os_str(),
    ];
    if let Some(inputs) = inputs {
        args.extend::<[&std::ffi::OsStr; 2]>(["--input".as_ref(), inputs.as_os_str()]);
    }
    args.extend::<[&std::ffi::OsStr; 2]>(["--until".as_ref(), until.as_ref()]);
    simagent(&args)
}

/// The fixture's runs and `web/dist`, with the run `town` of the ten-year scenario at the
/// end of its third year, and its twin beside the runs, made the same way.
fn writable_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (runs, web) = fixture(name);
    let base = runs.parent().expect("a parent").to_path_buf();
    let scenario = base.join("ten.json");
    write(&scenario, TEN_YEARS.as_bytes());
    let twin = base.join("twin");
    let until = (4 * Y - 1).to_string();
    for out in [runs.join("town"), twin.clone()] {
        simagent(&[
            "run".as_ref(),
            "--scenario".as_ref(),
            scenario.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
            "--until".as_ref(),
            until.as_ref(),
        ])
        .expect("runs");
    }
    (runs, web, twin)
}

/// The bytes of every file of a run directory.
fn contents(dir: &Path) -> Vec<Vec<u8>> {
    RUN_FILES
        .iter()
        .map(|file| fs::read(dir.join(file)).expect("read"))
        .collect()
}

/// A `POST` as the server's own page sends it.
fn post(port: u16, path: &str, body: &str) -> (u16, String, Vec<u8>) {
    let raw = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    send(port, raw.as_bytes())
}

/// The core's calendar date of `t`, as the server answers it.
fn date(t: i64) -> Value {
    let date = calendar(SimTime::from_secs(t));
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

/// Each step of the clock over HTTP is `run --resume --until` to the step's target
/// (ADR-0021 section 2): 28 days of 86 400 s; the last second of the year the run stands in
/// or the next one, if it stands at a year end; ten such year ends. Every file byte for byte.
#[test]
fn advancing_over_http_is_resume_until() {
    let (runs, web, twin) = writable_fixture("advance");
    let server = start(&runs, &web);
    let after_28_days = 4 * Y - 1 + 28 * DAY;
    let steps = [
        ("days28", after_28_days),
        ("year", 5 * Y - 1),
        ("year", 6 * Y - 1),
        ("days28", 6 * Y - 1 + 28 * DAY),
        ("decade", 16 * Y - 1),
    ];
    for (step, target) in steps {
        let (status, head, body) = post(
            server.port,
            "/api/runs/town/advance",
            &format!(r#"{{"step":"{step}"}}"#),
        );
        assert_eq!(status, 200, "{step}: {}", String::from_utf8_lossy(&body));
        assert!(head.contains("\r\nContent-Type: application/json\r\n"));
        assert_eq!(
            json_of(&body),
            json!({
                "format": "simagent.view.advanced",
                "version": 1,
                "closed_through": target,
                "date": date(target),
            }),
            "{step}"
        );
        resume_until(&twin, target, None).expect("the twin goes on");
        assert!(
            contents(&runs.join("town")) == contents(&twin),
            "{step} to {target}"
        );
    }
}

/// An input over HTTP is `run --resume --input` at the stop, written before the answer;
/// a rejected one and a body that is no request change no file; the input log the browser
/// wrote replays to the run (ADR-0021 section 2, ADR-0004).
#[test]
fn an_input_over_http_is_resume_with_input() {
    let (runs, web, twin) = writable_fixture("input");
    let server = start(&runs, &web);
    let port = server.port;
    let town = runs.join("town");
    let before = contents(&town);
    let (status, _, body) = post(
        port,
        "/api/runs/town/input",
        r#"{"arrange_partnership":{"woman":2,"man":1}}"#,
    );
    let rejected = json_of(&body);
    assert_eq!(
        (status, &rejected["code"], &rejected["reason"]),
        (
            409,
            &json!("input_rejected"),
            &json!("partners_of_wrong_sex")
        )
    );
    assert!(contents(&town) == before, "a rejected input");
    for (route, body) in [
        ("input", r#"{"arrange_partnership":{"woman":1}}"#),
        (
            "input",
            r#"{"arrange_partnership":{"woman":1,"man":2,"x":0}}"#,
        ),
        ("input", "[]"),
        ("input", "\u{ff}"),
        ("advance", r#"{"step":"week"}"#),
        ("advance", r#"{"step":"year","times":2}"#),
        ("advance", "{}"),
        ("advance", ""),
    ] {
        let (status, _, answer) = post(port, &format!("/api/runs/town/{route}"), body);
        assert_eq!(
            (status, code(&answer).as_str()),
            (400, "body_invalid"),
            "{body}"
        );
        assert!(contents(&town) == before, "{body}");
    }
    let arrange = r#"{"arrange_partnership":{"woman":1,"man":2}}"#;
    let (status, _, body) = post(port, "/api/runs/town/input", arrange);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    // The start is the input with seq 1; the pair is the next, at the stop.
    assert_eq!(
        json_of(&body),
        json!({"format": "simagent.view.input", "version": 1, "t": 4 * Y - 1, "seq": 2})
    );
    let inputs = runs.parent().expect("a parent").join("inputs.json");
    write(&inputs, format!("[{arrange}]").as_bytes());
    resume_until(&twin, 4 * Y - 1, Some(&inputs)).expect("the twin takes it");
    assert!(contents(&town) == contents(&twin), "the input");
    let (status, _, _) = post(port, "/api/runs/town/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200);
    drop(server);
    let replayed = runs.parent().expect("a parent").join("replayed");
    simagent(&[
        "run".as_ref(),
        "--replay".as_ref(),
        town.join("input.ndjson").as_os_str(),
        "--manifest".as_ref(),
        town.join("manifest.json").as_os_str(),
        "--out".as_ref(),
        replayed.as_os_str(),
    ])
    .expect("the log replays to the run");
}

/// "Zeit zu zweit" over HTTP (ADR-0023, addendum of 2026-11-01, W2 and W5): without a
/// couple the core refuses with `not_partners`; after the input both person pages carry the
/// window's last month start with its date while it is ahead, none before the input, none
/// once it passed.
#[test]
fn time_together_shows_its_end_on_both_pages() {
    const M: i64 = 2_419_200;
    let (runs, web, _) = writable_fixture("together");
    let server = start(&runs, &web);
    let port = server.port;
    let together = r#"{"time_together":{"woman":1,"man":2}}"#;
    let (status, _, body) = post(port, "/api/runs/town/input", together);
    assert_eq!(
        (status, &json_of(&body)["reason"]),
        (409, &json!("not_partners"))
    );
    let arrange = r#"{"arrange_partnership":{"woman":1,"man":2}}"#;
    let (status, _, _) = post(port, "/api/runs/town/input", arrange);
    assert_eq!(status, 200);
    let shown = |sim: u64| {
        let (status, _, body) = get(port, &format!("/api/runs/town/persons/{sim}"));
        assert_eq!(status, 200);
        json_of(&body)["time_together"].clone()
    };
    assert_eq!((shown(1), shown(2)), (Value::Null, Value::Null));
    let (status, _, body) = post(port, "/api/runs/town/input", together);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    // It acts in T_4, a month start, so the window is the next three: its last is 4 Y + 3 M,
    // day 0 of month 3 (counted from 0) of year 4.
    for sim in [1, 2] {
        let end = shown(sim);
        assert_eq!(end["t"], 4 * Y + 3 * M, "{sim}");
        let date = &end["date"];
        assert_eq!(
            (&date["year"], &date["month"], &date["day_of_month"]),
            (&json!(4), &json!(3), &json!(0)),
            "{sim}"
        );
    }
    let (status, _, _) = post(port, "/api/runs/town/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200);
    assert_eq!((shown(1), shown(2)), (Value::Null, Value::Null));
}

/// The run's world (ADR-0021, addendum 2026-10-28; Grok round 40): where the switch stands,
/// the years whose nutrition index is below 1 (a hunger sets the calendar years from the
/// first that begins after its stamp, ADR-0023 section 3), and for each year X from 1 to the
/// clock's year the switch a fork from X keeps: the start's, changed by every input stamped
/// before `T_X - 1 s` (the inputs `fork` takes over). Each of these is what a real fork from
/// X without a change stands at.
#[test]
fn the_world_answer_names_the_switch_the_hunger_and_what_a_fork_keeps() {
    let (runs, web, _) = writable_fixture("world");
    let server = start(&runs, &web);
    let port = server.port;
    let world = |run: &str| {
        let (status, _, body) = get(port, &format!("/api/runs/{run}/world"));
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        json_of(&body)
    };
    let send = |path: &str, body: &str| {
        let (status, _, answer) = post(port, &format!("/api/runs/town/{path}"), body);
        assert_eq!(status, 200, "{body}: {}", String::from_utf8_lossy(&answer));
    };
    let standing = |on: bool, hunger: Value, keeps: Value| {
        json!({
            "format": "simagent.view.world",
            "version": 2,
            "hereditary_diseases": on,
            "hunger_years": hunger,
            "fork_keeps_diseases": keeps,
        })
    };
    // At the end of year 3, off since the start.
    assert_eq!(
        world("town"),
        standing(false, json!([]), json!([false, false, false]))
    );
    // In year 4: on, and three years of hunger from year 5.
    send("advance", r#"{"step":"days28"}"#);
    send(
        "input",
        r#"{"set_world_param":{"change":{"core:hereditary_diseases":true}}}"#,
    );
    send(
        "input",
        r#"{"trigger_shock":{"shock":"core:hunger_three_years"}}"#,
    );
    assert_eq!(
        world("town"),
        standing(true, json!([5, 6, 7]), json!([false, false, false, false]))
    );
    // In year 6: off. A fork from 5 or 6 keeps the switch of year 4, not the one of year 6.
    send("advance", r#"{"step":"year"}"#);
    send("advance", r#"{"step":"year"}"#);
    send("advance", r#"{"step":"days28"}"#);
    send(
        "input",
        r#"{"set_world_param":{"change":{"core:hereditary_diseases":false}}}"#,
    );
    let keeps = [false, false, false, false, true, true];
    assert_eq!(
        world("town"),
        standing(false, json!([5, 6, 7]), json!(keeps))
    );
    for (year, kept) in (1..).zip(keeps) {
        let fork = format!("fork-{year}");
        let body = format!(r#"{{"run":"{fork}","from_year":{year},"changes":[]}}"#);
        send("fork", &body);
        assert_eq!(world(&fork)["hereditary_diseases"], json!(kept), "{fork}");
    }
    // A fork's own override counts like any input: from year 6 it turns the switch off, gives
    // year 8 the index 1 (no hunger) and year 9 the index 0.5; a year on, a fork from 7 keeps
    // the override's switch.
    let changes = r#"[{"core:hereditary_diseases":false},{"core:nutrition_year":{"year":8,"index":1.0}},{"core:nutrition_year":{"year":9,"index":0.5}}]"#;
    send(
        "fork",
        &format!(r#"{{"run":"fork-of","from_year":6,"changes":{changes}}}"#),
    );
    for step in ["year", "days28"] {
        let body = format!(r#"{{"step":"{step}"}}"#);
        let (status, _, answer) = post(port, "/api/runs/fork-of/advance", &body);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
    }
    assert_eq!(
        world("fork-of"),
        standing(
            false,
            json!([5, 6, 7, 9]),
            json!([false, false, false, false, true, true, false])
        )
    );
    // And a fork of that fork from 7 stands where the override left the switch.
    let (status, _, answer) = post(
        port,
        "/api/runs/fork-of/fork",
        r#"{"run":"fork-of-7","from_year":7,"changes":[]}"#,
    );
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
    assert_eq!(world("fork-of-7")["hereditary_diseases"], json!(false));
    // A start with the switch on (the fun-test town): a fork from year 1 keeps it on.
    let (status, _, answer) = post(port, "/api/runs", r#"{"template":"stadt","run":"stadt"}"#);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
    for step in ["year", "days28"] {
        let body = format!(r#"{{"step":"{step}"}}"#);
        let (status, _, answer) = post(port, "/api/runs/stadt/advance", &body);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
    }
    assert_eq!(world("stadt"), standing(true, json!([]), json!([true])));
    let (status, _, body) = get(port, "/api/runs/elsewhere/world");
    assert_eq!((status, code(&body).as_str()), (404, "unknown_run"));
}

/// A run another process writes is not opened; a run the server opened is the server's
/// until it ends (ADR-0021 section 1).
#[test]
fn the_server_holds_the_lock_of_a_run_it_writes() {
    let (runs, web, _) = writable_fixture("lock");
    let server = start(&runs, &web);
    let town = runs.join("town");
    let before = contents(&town);
    let step = r#"{"step":"days28"}"#;
    let lock = lock_path(&town).expect("a path");
    {
        let _held = Holder::hold(&lock);
        let (status, _, body) = post(server.port, "/api/runs/town/advance", step);
        assert_eq!((status, code(&body).as_str()), (409, "run_locked"));
        assert!(contents(&town) == before, "a locked run");
    }
    let (status, _, _) = post(server.port, "/api/runs/town/advance", step);
    assert_eq!(status, 200);
    let refused = resume_until(&town, 5 * Y - 1, None).expect_err("the server's");
    assert_eq!(first_code(&refused), "run_locked");
    assert_eq!(
        RunLock::acquire(&town).err().map(|e| e.code),
        Some("run_locked")
    );
    assert!(!free(&lock), "the server's");
    drop(server);
    resume_until(&town, 5 * Y - 1, None).expect("free once the server ended");
}

/// A write comes from the server's own page as JSON (ADR-0021 section 1), or nothing is
/// written and no run is opened; each route answers its own method only.
#[test]
fn a_write_needs_the_own_origin_and_json() {
    let (runs, web, _) = writable_fixture("origin");
    let server = start(&runs, &web);
    let port = server.port;
    let town = runs.join("town");
    let before = contents(&town);
    let step = r#"{"step":"days28"}"#;
    let request = |path: &str, host: &str, headers: &str| {
        format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\n{headers}Content-Length: {}\r\n\r\n{step}",
            step.len()
        )
    };
    let own = format!("127.0.0.1:{port}");
    let origin = format!("Origin: http://{own}\r\n");
    let json = "Content-Type: application/json\r\n";
    let advance = "/api/runs/town/advance";
    let with_origin = |value: &str| request(advance, &own, &format!("{json}Origin: {value}\r\n"));
    let with_type =
        |value: &str| request(advance, &own, &format!("{origin}Content-Type: {value}\r\n"));
    let cases = [
        (request(advance, &own, json), 403, "origin_foreign"),
        (
            request(advance, &own, &format!("{json}{origin}{origin}")),
            403,
            "origin_foreign",
        ),
        (
            with_origin(&format!("http://127.0.0.1:{}", port + 1)),
            403,
            "origin_foreign",
        ),
        (
            with_origin(&format!("http://localhost:{port}")),
            403,
            "origin_foreign",
        ),
        (with_origin("null"), 403, "origin_foreign"),
        (
            with_origin(&format!("https://127.0.0.1:{port}")),
            403,
            "origin_foreign",
        ),
        (
            with_origin(&format!("http://127.0.0.1:{port}/")),
            403,
            "origin_foreign",
        ),
        (
            request(advance, &own, &origin),
            415,
            "content_type_not_json",
        ),
        (with_type("text/plain"), 415, "content_type_not_json"),
        (with_type("application/jsonx"), 415, "content_type_not_json"),
        (
            with_type("text/plain; a=application/json"),
            415,
            "content_type_not_json",
        ),
        (
            request("/api/runs/town/census", &own, &format!("{origin}{json}")),
            405,
            "method_not_allowed",
        ),
        (
            request("/api/runs/nobody/advance", &own, &format!("{origin}{json}")),
            404,
            "unknown_run",
        ),
        (
            request("/api/runs/town/advance/x", &own, &format!("{origin}{json}")),
            405,
            "method_not_allowed",
        ),
        (
            request(
                "/api/runs/town/advance?x=1",
                &own,
                &format!("{origin}{json}"),
            ),
            400,
            "bad_query",
        ),
        (
            request("/api/runs/%74own/advance", &own, &format!("{origin}{json}")),
            404,
            "not_found",
        ),
    ];
    for (raw, status, expected) in cases {
        let (found, head, body) = send(port, raw.as_bytes());
        assert_eq!((found, code(&body).as_str()), (status, expected), "{raw:?}");
        if status == 405 {
            assert!(head.lines().any(|line| line == "Allow: GET"), "{head}");
        }
        assert!(contents(&town) == before, "{raw:?}");
    }
    // Nor does a body that is no request, the first to reach the run.
    for (route, body) in [
        ("advance", ""),
        ("advance", "[]"),
        ("advance", r#"{"step":"week"}"#),
        ("input", r#"{"arrange_partnership":{"woman":1}}"#),
    ] {
        let (status, _, answer) = post(port, &format!("/api/runs/town/{route}"), body);
        assert_eq!(
            (status, code(&answer).as_str()),
            (400, "body_invalid"),
            "{body}"
        );
    }
    // No refused request opened the run.
    assert!(free(&lock_path(&town).expect("a path")), "not opened");
    let (status, head, body) = get(port, advance);
    assert_eq!((status, code(&body).as_str()), (405, "method_not_allowed"));
    assert!(head.lines().any(|line| line == "Allow: POST"), "{head}");
    // The page's own origin under either name of the host, and a type with a parameter.
    for (host, content_type) in [
        (own.clone(), "application/json; charset=utf-8"),
        (format!("localhost:{port}"), "Application/JSON"),
    ] {
        let headers = format!("Origin: http://{host}\r\nContent-Type: {content_type}\r\n");
        let (status, _, body) = send(port, request(advance, &host, &headers).as_bytes());
        assert_eq!(status, 200, "{host}: {}", String::from_utf8_lossy(&body));
    }
}

/// The mode of a file of a run, to make it unwritable for a while.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("permissions set");
}

/// A write that fails leaves the run to nobody (ADR-0021 section 2): the server lets its
/// lock go and reads none of its files, until a write opens the directory as one run again;
/// a directory left between two files stays refused.
#[cfg(unix)]
#[test]
fn a_failed_write_is_not_read_as_a_run() {
    let (runs, web, twin) = writable_fixture("failed");
    let server = start(&runs, &web);
    let port = server.port;
    let town = runs.join("town");
    let year = r#"{"step":"year"}"#;
    let reads = [
        "/api/runs/town",
        "/api/runs/town/pedigree",
        "/api/runs/town/persons/1",
        "/api/runs/town/census",
        "/api/runs/town/events",
    ];
    let refused = |when: &str| {
        for path in reads {
            let (status, _, body) = get(port, path);
            assert_eq!(
                (status, code(&body).as_str()),
                (500, "run_failed"),
                "{when}: {path}"
            );
        }
        let list = json_of(&get(port, "/api/runs").2);
        let entry = (list["runs"].as_array().expect("runs"))
            .iter()
            .find(|entry| entry["run"] == "town")
            .cloned();
        assert_eq!(
            entry,
            Some(json!({"run": "town", "error": "run_failed"})),
            "{when}"
        );
    };
    // The input log cannot be opened again, the first write of a step: nothing is
    // written, and the run stays whole.
    let before = contents(&town);
    set_mode(&town.join("input.ndjson"), 0o444);
    let (status, _, body) = post(port, "/api/runs/town/advance", year);
    assert_eq!((status, code(&body).as_str()), (500, "io"));
    assert!(contents(&town) == before, "nothing written");
    let lock = lock_path(&town).expect("a path");
    assert!(free(&lock), "let go");
    refused("input log unwritable");
    set_mode(&town.join("input.ndjson"), 0o644);
    let (status, _, _) = post(port, "/api/runs/town/advance", year);
    assert_eq!(status, 200, "opened whole again");
    for path in reads {
        assert_eq!(get(port, path).0, 200, "{path}");
    }
    // An accepted input whose log cannot be written is a failure, no rejection.
    let before = contents(&town);
    set_mode(&town.join("input.ndjson"), 0o444);
    let arrange = r#"{"arrange_partnership":{"woman":1,"man":2}}"#;
    let (status, _, body) = post(port, "/api/runs/town/input", arrange);
    assert_eq!((status, code(&body).as_str()), (500, "io"));
    assert!(contents(&town) == before, "nothing written");
    refused("input unwritable");
    set_mode(&town.join("input.ndjson"), 0o644);
    let (status, _, _) = post(port, "/api/runs/town/advance", year);
    assert_eq!(status, 200, "opened whole again");
    // The census cannot be written: the input log is open again and the events of the
    // year are written, the save is not.
    set_mode(&town.join("census.csv"), 0o444);
    let (status, _, body) = post(port, "/api/runs/town/advance", year);
    assert_eq!((status, code(&body).as_str()), (500, "io"));
    refused("census unwritable");
    set_mode(&town.join("census.csv"), 0o644);
    let (status, _, body) = post(port, "/api/runs/town/advance", year);
    assert_eq!(
        (status, code(&body).as_str()),
        (500, "run_dir_inconsistent")
    );
    assert!(free(&lock), "not kept");
    refused("between two files");
    // Another process puts a whole run in its place: the next write opens it.
    for file in RUN_FILES {
        fs::copy(twin.join(file), town.join(file)).expect("copied");
    }
    refused("replaced, not yet opened");
    let (status, _, _) = post(port, "/api/runs/town/advance", year);
    assert_eq!(status, 200, "opened whole");
    for path in reads {
        assert_eq!(get(port, path).0, 200, "{path}");
    }
}

/// A run found between two files at its first open is refused like one the server left so
/// (ADR-0021 section 2); a run another process holds is only busy.
#[test]
fn a_run_that_does_not_open_is_not_read() {
    let (runs, web, _) = writable_fixture("not-open");
    let server = start(&runs, &web);
    let port = server.port;
    let town = runs.join("town");
    let events = fs::read_to_string(town.join("events.ndjson")).expect("read");
    let last = events.lines().last().expect("an entry").to_owned();
    fs::write(town.join("events.ndjson"), format!("{events}{last}\n")).expect("written");
    let step = r#"{"step":"days28"}"#;
    let lock = lock_path(&town).expect("a path");
    {
        let _held = Holder::hold(&lock);
        let (status, _, body) = post(port, "/api/runs/town/advance", step);
        assert_eq!((status, code(&body).as_str()), (409, "run_locked"));
    }
    assert_eq!(get(port, "/api/runs/town/events").0, 200, "only busy");
    let (status, _, body) = post(port, "/api/runs/town/advance", step);
    assert_eq!(
        (status, code(&body).as_str()),
        (500, "run_dir_inconsistent")
    );
    assert!(free(&lock), "not kept");
    let (status, _, body) = get(port, "/api/runs/town/events");
    assert_eq!((status, code(&body).as_str()), (500, "run_failed"));
}

/// The template of the fun-test town (ADR-0021 section 6).
fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/scenarios/stadt.json")
}

/// Founding a town is `run --scenario <template> --until 0` (ADR-0021 section 2): the run
/// stands in its first second, with its founders, every file byte for byte; a given seed
/// replaces the template's. A name in use, a bad name, an unknown template or a bad seed make nothing; the
/// new run is the server's until it ends.
#[test]
fn founding_a_town_is_the_template_in_its_first_second() {
    let (runs, web) = fixture("found");
    let base = runs.parent().expect("a parent").to_path_buf();
    let server = start(&runs, &web);
    let port = server.port;
    let (status, _, body) = post(port, "/api/runs", r#"{"template":"stadt","run":"neu"}"#);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(
        json_of(&body),
        json!({"format": "simagent.view.created", "version": 1, "run": "neu"})
    );
    let twin = base.join("twin");
    let made = |scenario: &Path, out: &Path| {
        simagent(&[
            "run".as_ref(),
            "--scenario".as_ref(),
            scenario.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
            "--until".as_ref(),
            "0".as_ref(),
        ])
        .expect("runs")
    };
    made(&template(), &twin);
    assert!(
        contents(&runs.join("neu")) == contents(&twin),
        "the template"
    );
    // Another seed: the template with that seed.
    let body = r#"{"template":"stadt","run":"neu.2","world_seed":"00000000000000ff"}"#;
    let (status, _, answer) = post(port, "/api/runs", body);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
    let seeded = base.join("seeded.json");
    let text = fs::read_to_string(template()).expect("read");
    let other = text.replace("\"000000000000a001\"", "\"00000000000000ff\"");
    assert_ne!(other, text, "the seed is replaced");
    write(&seeded, other.as_bytes());
    let twin2 = base.join("twin2");
    made(&seeded, &twin2);
    assert!(contents(&runs.join("neu.2")) == contents(&twin2), "a seed");
    // Refused: nothing made.
    for (body, status, expected) in [
        (r#"{"template":"stadt","run":"neu"}"#, 409, "run_exists"),
        (r#"{"template":"stadt","run":"notes"}"#, 409, "run_exists"),
        (
            r#"{"template":"stadt","run":"plain.txt"}"#,
            409,
            "run_exists",
        ),
        (
            r#"{"template":"stadt","run":"linked-run"}"#,
            409,
            "run_exists",
        ),
        (r#"{"template":"stadt","run":".x"}"#, 400, "body_invalid"),
        (r#"{"template":"stadt","run":"a/b"}"#, 400, "body_invalid"),
        (r#"{"template":"stadt","run":""}"#, 400, "body_invalid"),
        (r#"{"template":"dorf","run":"x1"}"#, 400, "body_invalid"),
        (
            r#"{"template":"stadt","run":"x2","world_seed":"ff"}"#,
            400,
            "body_invalid",
        ),
        (
            r#"{"template":"stadt","run":"x3","world_seed":"00000000000000FF"}"#,
            400,
            "body_invalid",
        ),
        (
            r#"{"template":"stadt","run":"x4","extra":1}"#,
            400,
            "body_invalid",
        ),
        (r#"{"run":"x5"}"#, 400, "body_invalid"),
    ] {
        let (found, _, answer) = post(port, "/api/runs", body);
        assert_eq!(
            (found, code(&answer).as_str()),
            (status, expected),
            "{body}"
        );
    }
    for name in ["x1", "x2", "x3", "x4", "x5", "a"] {
        assert!(!runs.join(name).exists(), "{name}");
    }
    // Listed, standing in its first second (the first second of year 0, ADR-0002).
    let list = json_of(&get(port, "/api/runs").2);
    let entry = (list["runs"].as_array().expect("runs"))
        .iter()
        .find(|entry| entry["run"] == "neu")
        .cloned()
        .expect("listed");
    assert_eq!(entry["date"], date(0));
    assert_eq!(entry["date"]["year"], 0);
    // The server holds it.
    let refused = resume_until(&runs.join("neu"), Y - 1, None).expect_err("the server's");
    assert_eq!(first_code(&refused), "run_locked");
    let (status, _, _) = post(port, "/api/runs/neu/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200, "it goes on");
    // Another method on the run list names both of its own.
    let raw = format!("PUT /api/runs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    let (status, head, _) = send(port, raw.as_bytes());
    assert_eq!(status, 405);
    assert!(
        head.lines().any(|line| line == "Allow: GET, POST"),
        "{head}"
    );
}

/// A name whose run failed and went is founded anew, and the new run is read: the mark of
/// the old one does not stay with the name.
#[cfg(unix)]
#[test]
fn a_name_founded_again_is_read_again() {
    let (runs, web) = fixture("found-again");
    let server = start(&runs, &web);
    let port = server.port;
    let found = r#"{"template":"stadt","run":"neu"}"#;
    assert_eq!(post(port, "/api/runs", found).0, 200);
    set_mode(&runs.join("neu/input.ndjson"), 0o444);
    let (status, _, body) = post(port, "/api/runs/neu/advance", r#"{"step":"year"}"#);
    assert_eq!((status, code(&body).as_str()), (500, "io"));
    assert_eq!(code(&get(port, "/api/runs/neu").2), "run_failed");
    set_mode(&runs.join("neu/input.ndjson"), 0o644);
    fs::remove_dir_all(runs.join("neu")).expect("removed");
    assert_eq!(post(port, "/api/runs", found).0, 200);
    assert_eq!(get(port, "/api/runs/neu").0, 200, "read again");
}

/// The stdout of a call of the binary that must succeed.
fn stdout_of(args: &[&std::ffi::OsStr]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .output()
        .expect("runs simagent");
    assert!(output.status.success(), "{args:?}");
    String::from_utf8(output.stdout).expect("UTF-8")
}

/// The override "hereditary diseases on" as the file of `simagent fork --override`.
fn switch_on(base: &Path) -> PathBuf {
    let file = base.join("switch_on.json");
    write(&file, br#"[{"core:hereditary_diseases":true}]"#);
    file
}

/// ADR-0021 section 2 and its addendum (G2): `POST /api/runs/{run}/fork` makes beside the
/// parent the fork the command line makes, also of a run the server holds open after a
/// step; the parent is written on afterwards.
#[test]
fn a_fork_over_http_is_the_fork_of_the_command_line() {
    let (runs, web, twin) = writable_fixture("fork_http");
    let base = runs.parent().expect("a parent").to_path_buf();
    let server = start(&runs, &web);
    let port = server.port;
    let (status, _, _) = post(port, "/api/runs/town/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200);
    resume_until(&twin, 5 * Y - 1, None).expect("the twin steps the same year");
    assert!(contents(&runs.join("town")) == contents(&twin), "the step");
    let body = r#"{"run":"fork","from_year":2,"changes":[{"core:hereditary_diseases":true}]}"#;
    let (status, _, answer) = post(port, "/api/runs/town/fork", body);
    assert_eq!(
        (status, json_of(&answer)),
        (
            200,
            json!({"format": "simagent.view.forked", "version": 1, "run": "fork"})
        )
    );
    let twin_fork = base.join("twin_fork");
    let block = switch_on(&base);
    simagent(&[
        "fork".as_ref(),
        twin.as_os_str(),
        "--from-year".as_ref(),
        "2".as_ref(),
        "--override".as_ref(),
        block.as_os_str(),
        "--out".as_ref(),
        twin_fork.as_os_str(),
    ])
    .expect("forks");
    assert!(
        contents(&runs.join("fork")) == contents(&twin_fork),
        "the fork"
    );
    let (_, _, list) = get(port, "/api/runs");
    let listed: Vec<Value> = (json_of(&list)["runs"].as_array().expect("runs").iter())
        .map(|entry| entry["run"].clone())
        .collect();
    assert!(listed.contains(&json!("fork")), "{listed:?}");
    let (status, _, _) = post(port, "/api/runs/town/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200, "the parent is written on");
}

/// The addendum (G2): a fork that cannot be is refused with the code of the command line
/// (and the core's code of a rejected override), and leaves no directory behind.
#[test]
fn a_fork_that_cannot_be_is_refused() {
    let (runs, web, _) = writable_fixture("fork_refused");
    let server = start(&runs, &web);
    let port = server.port;
    let fork = |body: &str| {
        let (status, _, answer) = post(port, "/api/runs/town/fork", body);
        (status, json_of(&answer))
    };
    let (status, answer) = fork(r#"{"run":"early","from_year":0,"changes":[]}"#);
    assert_eq!(
        (status, &answer["code"]),
        (409, &json!("fork_point_out_of_range"))
    );
    assert!(!runs.join("early").exists());
    let twice = r#"{"run":"twice","from_year":2,"changes":[{"core:hereditary_diseases":true},{"core:hereditary_diseases":false}]}"#;
    let (status, answer) = fork(twice);
    assert_eq!(
        (status, &answer["code"], &answer["reason"]),
        (
            409,
            &json!("override_rejected"),
            &json!("override_param_twice")
        )
    );
    assert!(!runs.join("twice").exists());
    let (status, answer) = fork(r#"{"run":"town","from_year":2,"changes":[]}"#);
    assert_eq!((status, &answer["code"]), (409, &json!("run_exists")));
    for body in [
        r#"{"run":"x","from_year":2}"#,
        r#"{"run":".x","from_year":2,"changes":[]}"#,
        r#"{"run":"x","from_year":"2","changes":[]}"#,
        r#"{"run":"x","from_year":2,"changes":[],"y":0}"#,
        "[]",
    ] {
        let (status, answer) = fork(body);
        assert_eq!(
            (status, &answer["code"]),
            (400, &json!("body_invalid")),
            "{body}"
        );
    }
    let (status, _, answer) = post(
        port,
        "/api/runs/elsewhere/fork",
        r#"{"run":"x","from_year":2,"changes":[]}"#,
    );
    assert_eq!((status, code(&answer).as_str()), (404, "unknown_run"));
}

/// The addendum (G2): `GET /api/runs/{a}/diff/{b}` is the line of `simagent diff` with the
/// names of the runs and each second with its date. On `tests/scenarios/diff_switch.json`
/// and its fork with the hereditary diseases on from year 5 that line is the reference's
/// (`tests/golden/diff_switch.json`). The death of key `5e51a4f2e3aba5e9` at 485 900 217 is,
/// by hand after ADR-0002 (`YEAR` 31 449 600 s, 28-day months, 7-day weeks, 91-day
/// seasons): year 15, day 163 of the year with 73 017 s into it, month 5, day 23 of it,
/// week 23, weekday 2, season 1.
#[test]
fn the_diff_over_http_is_the_diff_of_the_command_line() {
    let (runs, web) = fixture("diff_http");
    let base = runs.parent().expect("a parent").to_path_buf();
    let scenario =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/diff_switch.json");
    let (parent, fork) = (runs.join("parent"), runs.join("fork"));
    simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        parent.as_os_str(),
    ])
    .expect("runs");
    let block = switch_on(&base);
    simagent(&[
        "fork".as_ref(),
        parent.as_os_str(),
        "--from-year".as_ref(),
        "5".as_ref(),
        "--override".as_ref(),
        block.as_os_str(),
        "--out".as_ref(),
        fork.as_os_str(),
    ])
    .expect("forks");
    let line = stdout_of(&["diff".as_ref(), parent.as_os_str(), fork.as_os_str()]);
    assert_eq!(line, include_str!("../../../tests/golden/diff_switch.json"));
    // The same fork, gone two years further (the addendum of 2026-10-29).
    let further = runs.join("further");
    simagent(&[
        "fork".as_ref(),
        parent.as_os_str(),
        "--from-year".as_ref(),
        "5".as_ref(),
        "--override".as_ref(),
        block.as_os_str(),
        "--out".as_ref(),
        further.as_os_str(),
    ])
    .expect("forks");
    simagent(&[
        "run".as_ref(),
        "--resume".as_ref(),
        "--out".as_ref(),
        further.as_os_str(),
        "--until".as_ref(),
        (37 * 31_449_600 - 1).to_string().as_ref(),
    ])
    .expect("resumes");
    let server = start(&runs, &web);
    let port = server.port;
    let (status, _, body) = get(port, "/api/runs/parent/diff/fork");
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let answer = json_of(&body);
    let died = &answer["changed"][1]["b"]["died"];
    assert_eq!(
        answer["changed"][1]["lineage_key"],
        json!("5e51a4f2e3aba5e9")
    );
    assert_eq!(
        died,
        &json!({"t": 485_900_217, "date": {"year": 15, "day_of_year": 163, "month": 5, "day_of_month": 23, "week": 23, "weekday": 2, "season": 1, "second_of_day": 73_017}})
    );
    // Without the dates and with the format of the command line it is that line.
    let mut seconds = answer.clone();
    strip_dates(&mut seconds);
    let mut expected: Value = serde_json::from_str(&line).expect("json");
    expected["format_version"] = json!(null);
    seconds["format_version"] = json!(null);
    // Both runs stand at the end of the scenario, 35 years less a second: nothing is
    // recomputed.
    for (field, value) in [
        ("format", json!("simagent.view.diff")),
        ("version", json!(4)),
        ("a", json!("parent")),
        ("b", json!("fork")),
        ("at", json!(1_100_735_999)),
        ("recomputed", json!(null)),
    ] {
        assert_eq!(seconds[field], value, "{field}");
        expected[field] = value;
    }
    assert_eq!(seconds, expected);
    // The last second of year 34, by hand after ADR-0002: day 363 of the year with 86 399 s
    // into it, month 12, day 27 of it, week 51, weekday 6, season 3.
    assert_eq!(
        answer["at"],
        json!({"t": 1_100_735_999, "date": {"year": 34, "day_of_year": 363, "month": 12, "day_of_month": 27, "week": 51, "weekday": 6, "season": 3, "second_of_day": 86_399}})
    );
    // The fork gone further is compared at the parent's second, recomputed up to it: the
    // same comparison, but for its name and that it was recomputed, either way round.
    let (status, _, body) = get(port, "/api/runs/parent/diff/further");
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let mut later = json_of(&body);
    assert_eq!(
        (&later["at"], &later["recomputed"]),
        (&answer["at"], &json!("further"))
    );
    later["b"] = json!("fork");
    later["recomputed"] = json!(null);
    assert_eq!(later, answer);
    let (_, _, body) = get(port, "/api/runs/further/diff/parent");
    let back = json_of(&body);
    assert_eq!(
        (&back["a"], &back["b"], &back["at"], &back["recomputed"]),
        (
            &json!("further"),
            &json!("parent"),
            &answer["at"],
            &json!("further")
        )
    );
    assert_eq!(
        (&back["a_only"], &back["b_only"]),
        (&answer["b_only"], &answer["a_only"])
    );
    let (status, _, body) = get(port, "/api/runs/parent/diff/parent");
    let same = json_of(&body);
    assert_eq!(
        (
            status,
            &same["summary"],
            &same["a_only"],
            &same["b_only"],
            &same["changed"]
        ),
        (200, &json!([]), &json!([]), &json!([]), &json!([]))
    );
    let (status, _, body) = get(port, "/api/runs/parent/diff/elsewhere");
    assert_eq!((status, code(&body).as_str()), (404, "unknown_run"));
}

/// Each `{"t", "date"}` of a comparison over HTTP back to its second.
fn strip_dates(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.len() == 2 && map.contains_key("t") && map.contains_key("date") {
                *value = map["t"].clone();
                return;
            }
            map.values_mut().for_each(strip_dates);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_dates),
        _ => {}
    }
}

/// Astra's review of G2: a fork that fails after it has made its directory leaves nothing
/// of it. The parent's manifest names another hash of year 0, so the fork diverges before
/// its fork point: a failure of the parent, not of the request (500).
#[test]
fn a_fork_that_fails_after_it_began_leaves_nothing() {
    let (runs, web, _) = writable_fixture("fork_failed");
    let manifest = runs.join("town").join("manifest.json");
    let text = fs::read_to_string(&manifest).expect("reads");
    let year_0 = text
        .split(r#"{"year":0,"hash":""#)
        .nth(1)
        .and_then(|rest| rest.get(..16))
        .expect("the hash of year 0")
        .to_owned();
    let other = if year_0.starts_with('0') { "1" } else { "0" };
    write(
        &manifest,
        text.replacen(&year_0, &format!("{other}{}", &year_0[1..]), 1)
            .as_bytes(),
    );
    let server = start(&runs, &web);
    let (status, _, answer) = post(
        server.port,
        "/api/runs/town/fork",
        r#"{"run":"diverged","from_year":2,"changes":[]}"#,
    );
    assert_eq!(
        (status, code(&answer).as_str()),
        (500, "fork_diverged_before_fork_point")
    );
    assert!(!runs.join("diverged").exists());
}

/// Astra's review of G2: a name whose lock another process holds is refused with
/// `run_locked` and nothing is made there; a name with content before the request is
/// `run_exists`, and the content stays.
#[test]
fn a_fork_into_a_run_another_writes_touches_nothing() {
    let (runs, web, _) = writable_fixture("fork_taken");
    let server = start(&runs, &web);
    let fork_into = |name: &str| {
        let body = format!(r#"{{"run":"{name}","from_year":2,"changes":[]}}"#);
        let (status, _, answer) = post(server.port, "/api/runs/town/fork", &body);
        (status, code(&answer))
    };
    let taken = runs.join("taken");
    let held = RunLock::acquire(&taken).expect("the lock");
    assert_eq!(fork_into("taken"), (409, "run_locked".to_owned()));
    assert!(!taken.exists());
    drop(held);
    let theirs = runs.join("theirs");
    write(&theirs.join("file"), b"kept");
    assert_eq!(fork_into("theirs"), (409, "run_exists".to_owned()));
    assert_eq!(fs::read(theirs.join("file")).expect("reads"), b"kept");
}

/// `GET /api/sheet` (ADR-0021 G10 and its addendum of 2026-10-24): the sheet's scenario run in
/// memory, a row per founder in the order of Grok's round 37 (B8: early, base, late, red,
/// light iris, each a woman, then a man), each with the seven ages of the sheet and the
/// bodies without environment that the core gives a run of the same scenario.
#[test]
fn the_sheet_is_the_scenarios_founders_without_environment() {
    let (runs, web) = fixture("sheet");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/scenarios/bogen.json");
    let run = runs.join("bogen");
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), run.as_os_str()])
        .stdout(Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let save = fs::read(run.join("state.save")).expect("the save");
    let (world, _) = sim_core::world::World::load(&save, sim_cli::data::core_data().expect("data"))
        .expect("loads");
    let server = start(&runs, &web);
    let (status, head, body) = get(server.port, "/api/sheet");
    assert_eq!(status, 200);
    assert!(head.contains("\r\nContent-Type: application/json\r\n"));
    let answer = json_of(&body);
    assert_eq!(answer["format"], "simagent.view.sheet");
    assert_eq!(answer["version"], 1);
    assert_eq!(
        answer["contract_version"],
        sim_core::body_contract::CONTRACT_VERSION
    );
    let rows = answer["rows"].as_array().expect("rows");
    let variants = ["early", "base", "late", "red", "light_iris"];
    assert_eq!(rows.len(), 10);
    for (n, row) in rows.iter().enumerate() {
        assert_eq!(row["variant"], variants[n / 2], "row {n}");
        assert_eq!(
            row["sex"],
            if n % 2 == 0 { "female" } else { "male" },
            "row {n}"
        );
        let sim =
            sim_core::ids::SimId::try_from(u64::try_from(n + 1).expect("small")).expect("an id");
        let view = world.neutral_person_view(sim).expect("a view");
        assert_eq!(
            row["at_ages"],
            serde_json::to_value(&view.at_ages).expect("JSON"),
            "row {n}"
        );
        let ages: Vec<i64> = (row["at_ages"].as_array().expect("ages").iter())
            .map(|shown| shown["at"]["age_years"].as_i64().expect("an age"))
            .collect();
        assert_eq!(ages, [2, 8, 12, 16, 25, 40, 60], "row {n}");
    }
}

/// A run a process left between two stops (a year of events and census written, its input
/// log open, manifest and save of the last stop) goes back to that stop when the server
/// starts, which says so on its error output; the run then goes on like any other
/// (ADR-0021 section 2, addendum 2026-10-27).
#[test]
fn the_server_brings_an_interrupted_run_back_to_its_last_stop() {
    let (runs, web, twin) = writable_fixture("recover");
    let town = runs.join("town");
    let stopped = contents(&town);
    let lines = |dir: &Path, file: &str| {
        let text = fs::read_to_string(dir.join(file)).expect("read");
        text.lines().count()
    };
    let (events, census) = (lines(&town, "events.ndjson"), lines(&town, "census.csv"));
    // The twin runs the year the interrupted step ran: its events and census are the step's.
    resume_until(&twin, 5 * Y - 1, None).expect("the twin runs a year");
    for file in ["events.ndjson", "census.csv"] {
        fs::copy(twin.join(file), town.join(file)).expect("copied");
    }
    let log = fs::read_to_string(town.join("input.ndjson")).expect("read");
    let (open, _end) = log.trim_end().rsplit_once('\n').expect("an end line");
    fs::write(town.join("input.ndjson"), format!("{open}\n")).expect("written");
    let (cut_events, cut_rows) = (
        lines(&twin, "events.ndjson") - events,
        lines(&twin, "census.csv") - census,
    );
    assert!(cut_events > 0 && cut_rows == 1, "the step wrote a year");
    // A run another process holds is its own: the server starts and leaves it.
    let damaged = contents(&town);
    {
        let _held = Holder::hold(&lock_path(&town).expect("a path"));
        let _server = start(&runs, &web);
        assert!(contents(&town) == damaged, "a held run is left");
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .arg("view")
        .arg(&runs)
        .args(["--port", "0", "--web"])
        .arg(&web)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("starts");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("its output"))
        .read_line(&mut line)
        .expect("its address");
    let port: u16 = line
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/\n"))
        .and_then(|port| port.parse().ok())
        .unwrap_or_else(|| panic!("the address line: {line:?}"));
    assert!(contents(&town) == stopped, "back at its last stop");
    let (status, _, body) = post(port, "/api/runs/town/advance", r#"{"step":"year"}"#);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert!(contents(&town) == contents(&twin), "the year again");
    let _ = child.kill();
    let _ = child.wait();
    let mut said = String::new();
    child
        .stderr
        .take()
        .expect("its error output")
        .read_to_string(&mut said)
        .expect("text");
    let closed = 4 * Y - 1;
    let year = calendar(SimTime::from_secs(closed)).year;
    assert_eq!(
        said,
        format!(
            "view: run town went back to its last stop at {closed} (year {year}): cut {cut_events} events, {cut_rows} census rows, 0 inputs\n"
        )
    );
}

/// A couple of 30 and 32 (ADR-0024), together from the start, run to 23:59:58 of day 0; moved
/// into the flat at 23:59:59, their first morning is at 6:00 of day 1.
const COUPLE: &str = r#"{
  "format_version": 5,
  "start": {
    "world_seed": "0000000000000007",
    "founder_tag": "test:house",
    "world_params": {"hereditary_diseases": false, "nutrition": []},
    "founders": [
      {"born": -943488000, "sex": "female", "alleles": [{"locus": "core:disease_ar", "copies": [0, 0]}]},
      {"born": -1006387200, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [0, 0]}]}
    ],
    "couples": [{"woman": 0, "man": 1}],
    "capacity": null,
    "pool": null
  },
  "until": 86398
}"#;

/// The first 6:00 of day 1.
const MORNING: i64 = DAY + 6 * 3_600;

/// The offers of the flat a command may name for the couple of `COUPLE` (30 and 32) by day
/// (version 5 of the house answer): every offer of `kinds.json` open from 18 to 66 years, the
/// table, the fridge, the sofa's `sit` (its second offer), the television, the bookshelf, the
/// toilet and the shower, but neither the toy chest (6 to 12) nor the sleep of sofa, bed and
/// single bed outside the night.
fn by_day() -> Value {
    json!(
        [(0, 0), (1, 0), (2, 1), (3, 0), (4, 0), (5, 0), (6, 0)]
            .map(|(object, offer)| json!({"object": object, "offer": offer}))
    )
}

fn dated(t: i64) -> Value {
    json!({"t": t, "date": date(t)})
}

/// The server over the run `couple` of the scenario `COUPLE`, the couple's household, and the
/// run's directory.
fn couple(name: &str) -> (Server, Value, PathBuf) {
    served(name, COUPLE)
}

/// A couple as in `COUPLE` with a founder girl of 3: founders are unrelated, so the orphan rule
/// gives her (sim 3) to the woman of the open partnership as a foster child (ADR-0018 section
/// 4), a household of three in the flat.
const YOUNG: &str = r#"{
  "format_version": 5,
  "start": {
    "world_seed": "0000000000000007",
    "founder_tag": "test:house",
    "world_params": {"hereditary_diseases": false, "nutrition": []},
    "founders": [
      {"born": -943488000, "sex": "female", "alleles": [{"locus": "core:disease_ar", "copies": [0, 0]}]},
      {"born": -1006387200, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [0, 0]}]},
      {"born": -94348800, "sex": "female", "alleles": [{"locus": "core:disease_ar", "copies": [0, 0]}]}
    ],
    "couples": [{"woman": 0, "man": 1}],
    "capacity": null,
    "pool": null
  },
  "until": 86398
}"#;

/// The server over the run `couple` of `scenario`, its first household, and the run's
/// directory.
fn served(name: &str, scenario_json: &str) -> (Server, Value, PathBuf) {
    let (runs, web) = fixture(name);
    let base = runs.parent().expect("a parent").to_path_buf();
    let scenario = base.join("couple.json");
    write(&scenario, scenario_json.as_bytes());
    let out = runs.join("couple");
    simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ])
    .expect("runs");
    let server = start(&runs, &web);
    let (_, _, pedigree) = get(server.port, "/api/runs/couple/pedigree");
    let home = json_of(&pedigree)["households"][0]["household"].clone();
    (server, home, out)
}

/// The house of a household over HTTP (ADR-0021, addendum of 2026-11-03): none before it moves
/// in; moved in, its house without persons until its first morning; the steps of ten minutes,
/// an hour and to the next morning; at 6:00 both walk (she, the smaller SimId, to the toilet
/// 3 s away, he finds it taken and goes to the shower, 3 s; `sim-core/tests/house_day.rs` has
/// the choices by hand), with the morning values; at 6:10 she waits in the bath for the shower,
/// he showers there from 6:00:03 until 6:15:03; hunger 600 − 16.7, energy 900 − 6.7, her
/// bladder 1 000 at 6:02:33 − 14.9, his 500 − 20, her hygiene 700 − 5, his full, fun 800 − 10,
/// all truncated (ADR-0026). Both work full time (their draws 30 and 3, ADR-0024 section 4) and
/// leave at 8:40, no need of theirs below 150 by then (at most 2:40 h at the awake rates from
/// the morning values: bladder 500 − 320); at 9:10 both are at work until 17:20 (version 2).
/// By day each may be told every offer `by_day` names (version 5). Each need weighs in a choice
/// as far as it stands below its comfort (700, 1 000, 700, 900, 1 000), at 6:00 100, 100, 200,
/// 200, 200; no action is a time-filler (version 7, ADR-0026, addendum of 2026-11-04 on comfort
/// and the time-filler).
#[test]
fn a_house_over_http() {
    let (server, home, _) = couple("house");
    let port = server.port;
    let path = format!("/api/runs/couple/house/{home}");
    let (status, _, body) = get(port, &path);
    assert_eq!((status, code_of(&body)), (404, "not_in_house".to_owned()));
    let (status, _, body) = get(port, "/api/runs/couple/house/99");
    assert_eq!((status, code_of(&body)), (404, "not_in_house".to_owned()));
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    let (status, _, _) = post(port, "/api/runs/couple/input", &move_in);
    assert_eq!(status, 200);
    let house = |at: i64| {
        let (status, _, body) = get(port, &path);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let answer = json_of(&body);
        assert_eq!(
            (&answer["format"], &answer["version"], &answer["at"]),
            (&json!("simagent.view.house"), &json!(7), &dated(at))
        );
        assert_eq!(
            (&answer["household"], &answer["house"], &answer["comfort"]),
            (
                &home,
                &json!("core:flat"),
                &json!([700, 1000, 700, 900, 1000])
            )
        );
        answer
    };
    // The clock stays at the last second it ran; the input stands one second later.
    let before = house(DAY - 2);
    assert_eq!(
        (&before["morning"], &before["persons"], &before["children"]),
        (&json!(false), &json!([]), &json!([]))
    );
    // The drawing of the flat with each object's places and offers (`kinds.json`).
    let layout = &before["layout"];
    let rooms: Vec<&Value> = (layout["rooms"].as_array().expect("rooms").iter())
        .map(|room| &room["id"])
        .collect();
    assert_eq!(
        rooms,
        ["hall", "kitchen", "living", "bath", "bedroom", "kids"]
    );
    assert_eq!(layout["rooms"][1]["w"], json!(4.0));
    assert_eq!(layout["doors"].as_array().map(Vec::len), Some(5));
    assert_eq!(
        (
            &layout["objects"][0]["kind"],
            &layout["objects"][0]["slots"]
        ),
        (&json!("core:dining_table"), &json!(4))
    );
    assert_eq!(layout["objects"][0]["offers"], json!(["eat"]));
    assert_eq!(layout["objects"][7]["offers"], json!(["sleep"]));
    let step = |step: &str, target: i64| {
        let body = format!(r#"{{"step":"{step}"}}"#);
        let (status, _, answer) = post(port, "/api/runs/couple/advance", &body);
        assert_eq!(status, 200, "{step}: {}", String::from_utf8_lossy(&answer));
        assert_eq!(json_of(&answer)["closed_through"], json!(target), "{step}");
    };
    step("morning", MORNING);
    let morning = [600, 900, 500, 700, 800];
    let walking = |to: u8, arrive: i64| json!({"walking": {"from": 0, "to": to, "depart": dated(MORNING), "arrive": dated(arrive), "out": false}});
    let at_six = house(MORNING);
    assert_eq!(at_six["morning"], json!(true));
    let weights = [100, 100, 200, 200, 200];
    assert_eq!(
        at_six["persons"],
        json!([
            {"sim": 1, "place": walking(3, MORNING + 3),
             "action": {"object": 5, "offer": 0, "slot": 0, "until": null, "filler": false},
             "needs": morning, "weights": weights,
             "collapsed": false, "smells": false, "commands": by_day()},
            {"sim": 2, "place": walking(3, MORNING + 3),
             "action": {"object": 6, "offer": 0, "slot": 0, "until": null, "filler": false},
             "needs": morning, "weights": weights,
             "collapsed": false, "smells": false, "commands": by_day()},
        ])
    );
    step("minutes10", MORNING + 600);
    assert_eq!(
        house(MORNING + 600)["persons"],
        json!([
            {"sim": 1, "place": {"room": 3}, "action": null,
             "needs": [584, 894, 986, 695, 790], "weights": [116, 106, 0, 205, 210],
             "collapsed": false, "smells": false, "commands": by_day()},
            {"sim": 2, "place": {"room": 3},
             "action": {"object": 6, "offer": 0, "slot": 0, "until": dated(MORNING + 903),
                        "filler": false},
             "needs": [584, 894, 480, 1000, 790], "weights": [116, 106, 220, 0, 210],
             "collapsed": false, "smells": false, "commands": by_day()},
        ])
    );
    step("hour", MORNING + 4_200);
    step("hour", MORNING + 7_800);
    step("hour", MORNING + 11_400);
    let at_work = json!({"away": {"place": "work", "until": dated(DAY + 62_400)}});
    let persons = house(MORNING + 11_400)["persons"].clone();
    for (index, sim) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            (
                &persons[index]["sim"],
                &persons[index]["place"],
                &persons[index]["action"]
            ),
            (&json!(sim), &at_work, &Value::Null)
        );
    }
    // From 9:10 the next morning is tomorrow's, and from 6:00 itself too.
    step("morning", MORNING + DAY);
    step("morning", MORNING + 2 * DAY);
}

/// Children under 6 in the house over HTTP (ADR-0024 sections 2 and 4, since version 3): the foster
/// child of `YOUNG` has no needs, so it is no person; before the first morning no child is
/// shown either. From it, where the child is by the clock: on Tuesday (day 1) at 6:00 asleep
/// in the flat's night room, at 9:00 at the kindergarten until 15:00 of that day, at 16:00 in
/// its day room, at 20:00 asleep again; on Saturday (day 5) at 9:00 in its day room. Both rooms
/// of the flat are `kids`, its room 5.
#[test]
fn young_children_in_the_house() {
    const KIDS: u8 = 5;
    let (server, home, _) = served("young", YOUNG);
    let port = server.port;
    let path = format!("/api/runs/couple/house/{home}");
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    assert_eq!(post(port, "/api/runs/couple/input", &move_in).0, 200);
    let house = |at: i64| {
        let (status, _, body) = get(port, &path);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let answer = json_of(&body);
        assert_eq!(
            (&answer["version"], &answer["at"], &answer["house"]),
            (&json!(7), &dated(at), &json!("core:flat"))
        );
        answer
    };
    let step = |step: &str, target: i64| {
        let body = format!(r#"{{"step":"{step}"}}"#);
        let (status, _, answer) = post(port, "/api/runs/couple/advance", &body);
        assert_eq!(status, 200, "{step}: {}", String::from_utf8_lossy(&answer));
        assert_eq!(json_of(&answer)["closed_through"], json!(target), "{step}");
    };
    let hours = |from: i64, count: i64| {
        for hour in 1..=count {
            step("hour", from + hour * 3_600);
        }
    };
    let before = house(DAY - 2);
    assert_eq!(
        (&before["morning"], &before["persons"], &before["children"]),
        (&json!(false), &json!([]), &json!([]))
    );
    let at_home = |asleep: bool| json!([{"sim": 3, "place": {"room": KIDS, "asleep": asleep}}]);
    step("morning", MORNING);
    let at_six = house(MORNING);
    let persons: Vec<&Value> = (at_six["persons"].as_array().expect("persons").iter())
        .map(|person| &person["sim"])
        .collect();
    assert_eq!(persons, [1, 2]);
    assert_eq!(at_six["children"], at_home(true));
    hours(MORNING, 3);
    let kita = json!({"away": {"place": "kita", "until": dated(DAY + 54_000)}});
    assert_eq!(
        house(DAY + 32_400)["children"],
        json!([{"sim": 3, "place": kita}])
    );
    hours(DAY + 32_400, 7);
    assert_eq!(house(DAY + 57_600)["children"], at_home(false));
    hours(DAY + 57_600, 4);
    assert_eq!(house(DAY + 72_000)["children"], at_home(true));
    for day in 2..=5 {
        step("morning", day * DAY + 21_600);
    }
    hours(5 * DAY + 21_600, 3);
    assert_eq!(house(5 * DAY + 32_400)["children"], at_home(false));
}

/// "Warum?" over HTTP (ADR-0021, addendum of 2026-11-03; ADR-0026 section 6, version 2 of its
/// addendum of 2026-11-04 on the night, version 3 of the one on the mood, version 6 of the one on
/// comfort and the time-filler): the first choices of the couple at 6:00 in the hall at the
/// morning values, recomputed (the points by hand in `sim-core/tests/house_choice.rs`): what the
/// offers fill, each need weighed below its comfort, the toilet 100 000, the shower 60 000, the
/// table, television and bookshelf 40 000, the fridge 20 000, the sofa's `sit` 12 000, each
/// scaled by `(100 + mood) / 100` with the person's mood for it in that second (the draw `mood`,
/// as its addendum gives it), less the walk, 150 or 200; the most first. The sleep of sofa and
/// beds is closed by day, listed after them without points or mood, and their night is the next,
/// from 22:00 to 6:00; the departure at 8:40 closes nothing yet. She takes the toilet (at least
/// 89 850 against at most 65 850), for him it is taken and he takes the shower (at least 53 850
/// against at most 43 850). No need is critical at the morning values, so no offer is urgent, and
/// none stands at its comfort; the needs of the choice and each need's comfort stand beside them,
/// and a start has no comparison at the clock (version 4) and is no time-filler. Any other entry
/// is no choice.
#[test]
fn why_over_http() {
    let (server, home, run) = couple("why");
    let port = server.port;
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    assert_eq!(post(port, "/api/runs/couple/input", &move_in).0, 200);
    let morning = r#"{"step":"morning"}"#;
    assert_eq!(post(port, "/api/runs/couple/advance", morning).0, 200);
    let starts = |sim: u64| -> Vec<u64> {
        let path = format!("/api/runs/couple/events?subject={sim}&from={MORNING}");
        let (_, _, body) = get(port, &path);
        (json_of(&body)["events"].as_array().expect("events").iter())
            .filter(|row| row["entry"]["payload"].get("core:action_started").is_some())
            .map(|row| row["entry"]["id"].as_u64().expect("an id"))
            .collect()
    };
    // Sims 1 and 2 are the founders 0 and 1 of `test:house`, the world seed 7.
    let tag = DataId::new("test:house").expect("valid");
    let scored = |sim: u64| -> Vec<(u16, u16, i8, i64)> {
        let filled = [
            (5, 0, 100_000, 150),
            (6, 0, 60_000, 150),
            (0, 0, 40_000, 200),
            (3, 0, 40_000, 150),
            (4, 0, 40_000, 150),
            (1, 0, 20_000, 200),
            (2, 1, 12_000, 150),
        ];
        let key = LineageKey::founder(&tag, sim - 1);
        let mut scored: Vec<(u16, u16, i8, i64)> = (filled.iter())
            .map(|&(object, offer, filled, walk)| {
                // The repository's day moves the points by at most 10 percent either way.
                let mood = mood(7, key, SimTime::from_secs(MORNING), (object, offer), 10);
                (
                    object,
                    offer,
                    mood,
                    filled * (100 + i64::from(mood)) / 100 - walk,
                )
            })
            .collect();
        scored
            .sort_by_key(|&(object, offer, _, points)| (std::cmp::Reverse(points), object, offer));
        scored
    };
    let offers =
        |sim: u64, busy: Option<u16>| {
            let mut offers: Vec<serde_json::Value> = (scored(sim).into_iter())
                .map(|(object, offer, mood, points)| {
                    json!({"object": object, "offer": offer, "points": points, "mood": mood,
                       "busy": Some(object) == busy, "closed": false, "departs": null,
                       "urgent": [], "sated": [], "held": false, "outside": false,
                       "bed_for": null})
                })
                .collect();
            // By day the beds are closed: the sofa for nobody in particular, the bed for couples,
            // the single bed for children (`data/house/kinds.json`).
            offers.extend([(2, json!(null)), (7, json!("couples")), (8, json!("children"))].map(
            |(object, bed_for)| {
                json!({"object": object, "offer": 0, "points": null, "mood": null, "busy": false,
                       "closed": true, "departs": null, "urgent": [], "sated": [],
                       "held": false, "outside": false, "bed_for": bed_for})
            },
        ));
            offers
        };
    let chosen = |sim: u64, object: u16| {
        let (_, _, mood, points) = (scored(sim).into_iter())
            .find(|&(each, offer, _, _)| (each, offer) == (object, 0))
            .expect("scored");
        json!({"object": object, "offer": 0, "points": points, "mood": mood, "urgent": []})
    };
    let night = json!({
        "from": dated(MORNING + 16 * 3_600),
        "until": dated(MORNING + 24 * 3_600),
        "now": false,
    });
    for (sim, object, busy) in [(1, 5, None), (2, 6, Some(5))] {
        let entry = starts(sim)[0];
        let (status, _, body) = get(port, &format!("/api/runs/couple/why/{entry}"));
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        assert_eq!(
            json_of(&body),
            json!({
                "format": "simagent.view.why",
                "version": 6,
                "entry": entry,
                "sim": sim,
                "at": dated(MORNING),
                "enough": 40_000,
                "commanded": false,
                "needs": [600, 900, 500, 700, 800],
                "comfort": [700, 1000, 700, 900, 1000],
                "night": night,
                "chosen": chosen(sim, object),
                "waits": null,
                "goes_to": null,
                "filler": null,
                "offers": offers(sim, busy),
                "now": null,
            }),
            "{sim}"
        );
    }
    for path in [
        "/api/runs/couple/why/1",
        "/api/runs/couple/why/999999",
        "/api/runs/couple/why/x",
    ] {
        let (status, _, body) = get(port, path);
        assert_eq!(
            (status, code_of(&body)),
            (404, "not_a_choice".to_owned()),
            "{path}"
        );
    }
    // A last line without its line end is one the command line is still writing (review): the
    // last entry again with the next id is not served yet; once two lines carry that id, the
    // log is broken.
    let log = run.join("events.ndjson");
    let text = fs::read_to_string(&log).expect("the log");
    let last = text.lines().last().expect("a last entry");
    let id = last["{\"id\":".len()..].split(',').next().expect("an id");
    let next = id.parse::<u64>().expect("a number") + 1;
    let again = last.replacen(&format!("{{\"id\":{id},"), &format!("{{\"id\":{next},"), 1);
    write(&log, format!("{text}{again}").as_bytes());
    let path = format!("/api/runs/couple/why/{next}");
    let (status, _, body) = get(port, &path);
    assert_eq!((status, code_of(&body)), (404, "not_a_choice".to_owned()));
    write(&log, format!("{text}{again}\n{again}\n").as_bytes());
    let (status, _, body) = get(port, &path);
    assert_eq!((status, code_of(&body)), (500, "events_invalid".to_owned()));
}

/// A command over HTTP (ADR-0024 section 5): the input route takes `command` like any input.
/// At 6:00 she walks from the hall to the toilet (`a_house_over_http`). Told to play at the toy
/// chest (object 9, from 6 to 12 only) she is refused for her age, and nothing is written; told
/// to watch television (object 3) she stops in the second the command acts, back in the hall,
/// and walks to the living room, 3 s, on its first place, with the morning values. At 6:10 he
/// showers (`a_house_over_http`); told to watch as well he stops in the second the command acts
/// and walks from the bath, 6 s, to the television's second place. His hygiene, written down
/// then, stands in the answer at 6:10 with that value (700 + 2 800 · 598 / 3 600, full), his
/// other needs as at 6:10; `the_house_answer_after_a_command` tells the clock and the next second
/// apart. "Warum?" for her commanded walk says so; a command is no choice and has no mood
/// (ADR-0026, addendum of 2026-11-04 on the mood), so the points beside it are those of the needs
/// and the walk alone, at the morning values in the hall as in `why_over_http` without a mood,
/// every mood `null`.
#[test]
fn a_command_over_http() {
    let (server, home, run) = couple("command");
    let port = server.port;
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    assert_eq!(post(port, "/api/runs/couple/input", &move_in).0, 200);
    let morning = r#"{"step":"morning"}"#;
    assert_eq!(post(port, "/api/runs/couple/advance", morning).0, 200);
    let inputs = || fs::read_to_string(run.join("input.ndjson")).expect("the input log");
    let before = inputs();
    let toys = r#"{"command":{"sim":1,"object":9,"offer":0}}"#;
    let (status, _, body) = post(port, "/api/runs/couple/input", toys);
    assert_eq!(
        (status, code_of(&body), &json_of(&body)["reason"]),
        (409, "input_rejected".to_owned(), &json!("wrong_age"))
    );
    assert_eq!(inputs(), before);
    let television = r#"{"command":{"sim":1,"object":3,"offer":0}}"#;
    let (status, _, body) = post(port, "/api/runs/couple/input", television);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(
        json_of(&body),
        json!({"format": "simagent.view.input", "version": 1, "t": MORNING, "seq": 3})
    );
    assert!(inputs().contains(r#""input":{"command":{"sim":1,"object":3,"offer":0}}"#));
    let (status, _, body) = get(port, &format!("/api/runs/couple/house/{home}"));
    assert_eq!(status, 200);
    let walking = json!({"walking": {"from": 0, "to": 2, "depart": dated(MORNING + 1), "arrive": dated(MORNING + 4), "out": false}});
    assert_eq!(
        json_of(&body)["persons"][0],
        json!({"sim": 1, "place": walking,
               "action": {"object": 3, "offer": 0, "slot": 0, "until": null, "filler": false},
               "needs": [600, 900, 500, 700, 800], "weights": [100, 100, 200, 200, 200],
               "collapsed": false, "smells": false, "commands": by_day()})
    );
    let minutes = r#"{"step":"minutes10"}"#;
    assert_eq!(post(port, "/api/runs/couple/advance", minutes).0, 200);
    let (_, _, body) = get(
        port,
        &format!("/api/runs/couple/events?subject=1&from={MORNING}"),
    );
    let commanded = (json_of(&body)["events"].as_array().expect("events").iter())
        .find(|row| row["entry"]["payload"]["core:action_started"]["commanded"] == true)
        .map(|row| row["entry"]["id"].as_u64().expect("an id"))
        .expect("her commanded walk");
    let (status, _, body) = get(port, &format!("/api/runs/couple/why/{commanded}"));
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let why = json_of(&body);
    assert_eq!(why["commanded"], json!(true), "{why}");
    let television = json!({"object": 3, "offer": 0, "points": 39_850, "mood": null, "urgent": []});
    assert_eq!(why["chosen"], television, "{why}");
    let offers = why["offers"].as_array().expect("offers");
    let open: Vec<(u64, i64)> = (offers.iter())
        .filter(|offer| offer["closed"] == false)
        .map(|offer| {
            let number = |key: &str| offer[key].as_i64().expect("a number");
            (
                offer["object"].as_u64().expect("an object"),
                number("points"),
            )
        })
        .collect();
    let by_hand = [
        (5, 99_850),
        (6, 59_850),
        (3, 39_850),
        (4, 39_850),
        (0, 39_800),
        (1, 19_800),
        (2, 11_850),
    ];
    assert_eq!(open, by_hand, "{why}");
    assert!(offers.iter().all(|offer| offer["mood"].is_null()), "{why}");
    let his = r#"{"command":{"sim":2,"object":3,"offer":0}}"#;
    let (status, _, body) = post(port, "/api/runs/couple/input", his);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_of(&body)["seq"], json!(4));
    let (status, _, body) = get(port, &format!("/api/runs/couple/house/{home}"));
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let walking = json!({"walking": {"from": 3, "to": 2, "depart": dated(MORNING + 601), "arrive": dated(MORNING + 607), "out": false}});
    assert_eq!(
        json_of(&body)["persons"][1],
        json!({"sim": 2, "place": walking,
               "action": {"object": 3, "offer": 0, "slot": 1, "until": null, "filler": false},
               "needs": [584, 894, 480, 1000, 790], "weights": [116, 106, 220, 0, 210],
               "collapsed": false, "smells": false, "commands": by_day()})
    );
}

/// The house answer right after a command (ADR-0024 section 5, the answer called directly at a
/// stop the steps never reach): what the command wrote down in the second it acts stands with
/// that value. The couple of `COUPLE` in the flat with the data `simagent` uses: at 6:00 she
/// walks to the toilet, there at 6:00:03 with the bladder at 500, +12 000 an hour. Told at
/// 6:01:40 to watch television (object 3) she stops at 6:01:41 with the bladder at 826 (500 +
/// 326.7), where the clock would give 823 (500 + 323.3), and walks from the bath to the living
/// room, 6 s; her other needs at the clock: hunger 598 (600 − 2.8), energy 899 (900 − 1.1),
/// hygiene 700 (700 − 0.8), fun 799 (800 − 1.7).
#[test]
fn the_house_answer_after_a_command() {
    use sim_core::input::{Command as Order, Couple, Founder, Input, MoveIn, Start};
    use sim_core::log::LogPayload;
    use sim_core::world::{World, YearOut};
    let founder = |born: i64, sex: sim_core::genome::Sex| Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: Vec::new(),
    };
    let mut world = World::new(sim_cli::data::core_data().expect("the data"));
    let start = Start {
        world_seed: 7,
        founder_tag: sim_core::data_id::DataId::new("test:house").expect("valid"),
        world_params: sim_core::params::WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(-943_488_000, sim_core::genome::Sex::Female),
            founder(-1_006_387_200, sim_core::genome::Sex::Male),
        ],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    };
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    let mut advance = |world: &mut World, until: i64| {
        world.advance_until(&mut out, SimTime::from_secs(until), &mut |_| false);
        world.take_log()
    };
    let household = (advance(&mut world, DAY - 2).into_iter())
        .find_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .expect("their household");
    world
        .submit(Input::MoveIn(MoveIn { household }))
        .expect("moves in");
    advance(&mut world, MORNING + 100);
    let television = Order {
        sim: sim_core::ids::SimId::try_from(1).expect("valid"),
        object: 3,
        offer: 0,
    };
    world.submit(Input::Command(television)).expect("accepted");
    let answer = sim_cli::house_answer(&world, household.get())
        .expect("answers")
        .expect("a house");
    let walking = json!({"walking": {"from": 3, "to": 2, "depart": dated(MORNING + 101), "arrive": dated(MORNING + 107), "out": false}});
    assert_eq!(answer["at"], dated(MORNING + 100));
    assert_eq!(
        answer["persons"][0],
        json!({"sim": 1, "place": walking,
               "action": {"object": 3, "offer": 0, "slot": 0, "until": null, "filler": false},
               "needs": [598, 899, 826, 700, 799], "weights": [102, 101, 0, 200, 201],
               "collapsed": false, "smells": false, "commands": by_day()})
    );
}

/// The couple of `the_house_answer_after_a_command` (born −943 488 000 and −1 006 387 200, both
/// at work full time, ADR-0024 section 4) moved in at 23:59:59 of day 0, in the flat, on the
/// repository's data with its house data changed by `change` (needs, day): the world and the
/// household.
fn couple_in_the_flat(
    change: impl FnOnce(&mut Value, &mut Value),
) -> (sim_core::world::World, u64) {
    use sim_core::input::{Couple, Founder, Input, MoveIn, Start};
    use sim_core::log::LogPayload;
    use sim_core::world::{World, YearOut};
    let mut needs: Value =
        serde_json::from_str(include_str!("../../../data/house/needs.json")).expect("json");
    let mut day: Value =
        serde_json::from_str(include_str!("../../../data/house/day.json")).expect("json");
    change(&mut needs, &mut day);
    let houses = sim_cli::house::house_data_from(
        &needs.to_string(),
        &day.to_string(),
        include_str!("../../../data/house/kinds.json"),
        &[
            ("flat", include_str!("../../../data/house/houses/flat.json")),
            (
                "house",
                include_str!("../../../data/house/houses/house.json"),
            ),
            (
                "terraced",
                include_str!("../../../data/house/houses/terraced.json"),
            ),
        ],
    )
    .expect("the houses");
    let data = sim_cli::data::core_data()
        .expect("the data")
        .with_houses(houses);
    let founder = |born: i64, sex: sim_core::genome::Sex| Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: Vec::new(),
    };
    let mut world = World::new(data);
    let start = Start {
        world_seed: 7,
        founder_tag: sim_core::data_id::DataId::new("test:house").expect("valid"),
        world_params: sim_core::params::WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(-943_488_000, sim_core::genome::Sex::Female),
            founder(-1_006_387_200, sim_core::genome::Sex::Male),
        ],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    };
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(DAY - 2), &mut |_| false);
    let household = (world.take_log().into_iter())
        .find_map(|entry| match entry.payload {
            LogPayload::HouseholdFormed { household } => Some(household),
            _ => None,
        })
        .expect("their household");
    world
        .submit(Input::MoveIn(MoveIn { household }))
        .expect("moves in");
    (world, household.get())
}

/// The house answer of one lying and of one who smells (version 4, ADR-0026, addendum of
/// 2026-11-04 on a need at 0), called directly. With energy falling 900 an hour from 18 to 66
/// both lie down at 7:00, wherever the first hour took them: no offer but sleep sets energy, so
/// it is at 0 3 600 s after the morning. On the floor no other need comes near 50 in five hours
/// (the bladder from at least 380, hunger from at least 500, hygiene from at least 670, falling
/// 40, 40 and 10 an hour), so both get up at 12:00. At 6:59:59 neither lies, at 7:00 and at
/// 11:59:59 both do, at 12:00 neither, and neither smells. With the jump of hygiene on coming
/// home at −1 000, both are home at 17:20 with hygiene 0 and smell; away the second before, with
/// hygiene above 50, they do not.
#[test]
fn the_house_answer_of_one_lying_and_of_one_who_smells() {
    use sim_core::world::YearOut;
    let at = |world: &mut sim_core::world::World, until: i64| {
        let mut out = YearOut::default();
        world.advance_until(&mut out, SimTime::from_secs(until), &mut |_| false);
        world.take_log();
    };
    let field = |world: &sim_core::world::World, household: u64, name: &str| -> Vec<Value> {
        let answer = sim_cli::house_answer(world, household)
            .expect("answers")
            .expect("a house");
        assert_eq!(answer["version"], json!(7));
        (answer["persons"].as_array().expect("persons").iter())
            .map(|person| person[name].clone())
            .collect()
    };
    let (mut world, household) = couple_in_the_flat(|needs, _| {
        needs["awake"]["energy"][2] = json!(-900);
    });
    let both = |value: bool| [json!(value), json!(value)];
    at(&mut world, MORNING + 3_599);
    assert_eq!(field(&world, household, "collapsed"), both(false));
    at(&mut world, MORNING + 3_600);
    assert_eq!(field(&world, household, "collapsed"), both(true));
    assert_eq!(field(&world, household, "smells"), both(false));
    at(&mut world, MORNING + 21_599);
    assert_eq!(field(&world, household, "collapsed"), both(true));
    at(&mut world, MORNING + 21_600);
    assert_eq!(field(&world, household, "collapsed"), both(false));
    let (mut world, household) = couple_in_the_flat(|_, day| {
        day["return_jumps"]["hygiene"] = json!(-1000);
    });
    at(&mut world, DAY + 62_399);
    assert_eq!(field(&world, household, "smells"), both(false));
    at(&mut world, DAY + 62_400);
    assert_eq!(field(&world, household, "smells"), both(true));
    assert_eq!(field(&world, household, "collapsed"), both(false));
}

/// The offers a command may name (version 5 of the house answer, ADR-0024 section 5), judged in
/// the second the command acts, the one after the clock. The couple of `couple_in_the_flat` (30
/// and 32, the band of 18 to 66, its night from 22:00 to 6:00): at 21:59:58 of day 1 a command
/// acts at 21:59:59, by day, and may name what `by_day` lists; at 21:59:59 it acts at 22:00, in
/// the night, and may name sofa (object 2), bed (7) and single bed (8) too; at 5:59:58 of day 2
/// still the night, at 5:59:59, acting at 6:00, the day again. The toy chest (9, 6 to 12)
/// never; the sofa's `sit` (its second offer) by day and in the night.
#[test]
fn the_offers_a_command_may_name_follow_the_night() {
    use sim_core::world::YearOut;
    let (mut world, household) = couple_in_the_flat(|_, _| {});
    let in_the_night = json!(
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (2, 1),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 0),
            (7, 0),
            (8, 0)
        ]
        .map(|(object, offer)| json!({"object": object, "offer": offer}))
    );
    let cases = [
        (DAY + 79_198, by_day()),
        (DAY + 79_199, in_the_night.clone()),
        (2 * DAY + 21_598, in_the_night),
        (2 * DAY + 21_599, by_day()),
    ];
    for (until, commands) in cases {
        let mut out = YearOut::default();
        world.advance_until(&mut out, SimTime::from_secs(until), &mut |_| false);
        world.take_log();
        let answer = sim_cli::house_answer(&world, household)
            .expect("answers")
            .expect("a house");
        assert_eq!(answer["version"], json!(7));
        let persons = answer["persons"].as_array().expect("persons");
        assert_eq!(persons.len(), 2);
        for person in persons {
            assert_eq!(person["commands"], commands, "{until}: {person}");
        }
    }
}

/// What is urgent in "Warum?" (version 4, ADR-0026 section 3), for a choice of hers (sim 1 of
/// `couple_in_the_flat`) in the hall at 6:00 of day 1 with hunger 100, bladder 120 and hygiene
/// 50, all critical (at most 150), energy 900 and fun 800 not. Table and fridge raise hunger,
/// the toilet the bladder, the shower hygiene; television, bookshelf and the sofa's `sit` raise
/// fun alone, and the sleep of sofa, bed and single bed, closed by day, energy: nothing of
/// theirs is urgent. The urgent part, (151 − v) · 10⁹ for each critical need an offer raises,
/// puts the shower first (101 · 10⁹), then table and fridge (51 · 10⁹; hunger weighs 600 from its
/// comfort 700, the table fills 600 · 600, the fridge 600 · 200), then the toilet (31 · 10⁹), then
/// television, bookshelf and `sit`. The shower's points by hand: 101 · 10⁹ + 850 · 700 · (100 +
/// her mood for it) / 100 − 3 · 50, hygiene weighing 850 from its comfort 900, the walk from the
/// hall to the bath 3 s. The needs of the choice stand beside the offers.
#[test]
fn why_names_what_is_urgent() {
    use sim_core::log::{LogPayload, Subject};
    use sim_core::world::YearOut;
    let (mut world, _) = couple_in_the_flat(|_, _| {});
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(MORNING), &mut |_| false);
    let hers = (world.take_log().into_iter())
        .find(|entry| {
            entry.subjects
                == [Subject::Sim(
                    sim_core::ids::SimId::try_from(1).expect("valid"),
                )]
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        })
        .expect("her first choice");
    let needs = [100, 900, 120, 50, 800];
    let entry = sim_core::log::LogEntry {
        payload: LogPayload::Idle {
            house: DataId::new("core:flat").expect("valid"),
            room: 0,
            needs,
            busy: Vec::new(),
            waits: None,
            held: Vec::new(),
            tier: Vec::new(),
        },
        ..hers
    };
    let why = sim_cli::why_answer(&world, &entry, 7, false, None)
        .expect("answers")
        .expect("a choice");
    assert_eq!(why["version"], json!(6));
    assert_eq!(why["needs"], json!(needs));
    assert_eq!((&why["chosen"], &why["now"]), (&Value::Null, &Value::Null));
    let offers = why["offers"].as_array().expect("offers");
    let urgent: Vec<(u64, &Value)> = (offers.iter())
        .map(|offer| {
            (
                offer["object"].as_u64().expect("an object"),
                &offer["urgent"],
            )
        })
        .collect();
    let (hunger, none) = (json!(["hunger"]), json!([]));
    assert_eq!(
        urgent[..4],
        [
            (6, &json!(["hygiene"])),
            (0, &hunger),
            (1, &hunger),
            (5, &json!(["bladder"]))
        ]
    );
    let mut rest: Vec<(u64, &Value)> = urgent[4..].to_vec();
    rest.sort_by_key(|&(object, _)| object);
    assert_eq!(
        rest,
        [
            (2, &none),
            (2, &none),
            (3, &none),
            (4, &none),
            (7, &none),
            (8, &none)
        ]
    );
    let key = LineageKey::founder(&DataId::new("test:house").expect("valid"), 0);
    let shower = mood(7, key, SimTime::from_secs(MORNING), (6, 0), 10);
    assert_eq!(
        offers[0]["points"],
        json!(101_000_000_000_i64 + 595_000 * (100 + i64::from(shower)) / 100 - 150)
    );
}

/// The walk from each room of the flat (hall, kitchen, living room, bath, bedroom, children's
/// room) to the toilet in the bath, in seconds (`flat.json`): from the hall 3, through it from
/// the kitchen 4 + 3, the living room 3 + 3, the bedroom and the children's room 5 + 3; within
/// the bath 5 (ADR-0025 section 4).
const TO_BATH: [i64; 6] = [3, 7, 6, 5, 8, 8];

/// "Warum?" of an idle person at the clock (version 4): the couple of `COUPLE` at its first
/// morning, and the clock steps ten minutes at a time until one of them stands idle in a room
/// (with time-fillers nobody idles by day but who waits: at 6:10 she waits in the bath for the
/// shower he uses, `a_house_over_http`). Its last choice is idle, and "Warum?" of it adds the
/// comparison at the clock: the second, the needs the house answer gives then, and the offers as
/// "Warum?" computes them for an idle choice in that second, from that room, with those needs and
/// the objects without a free place then, which is the object of the other's action if it has one
/// place, else none. The toilet among them by hand: it fills the missing bladder whole in its
/// 300 s at 12 000 an hour, weighed by what the bladder lacks of its comfort 700 (ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler), scaled by the person's mood for it in
/// that second, less 50 a second of the walk to the bath (`TO_BATH`), and (151 − v) · 10⁹ if the
/// bladder is critical. No earlier choice of that person has the comparison, and once the
/// person does something again its last idle choice has none either.
#[test]
fn why_compares_an_idle_person_at_the_clock() {
    let (server, home, run) = couple("now");
    let port = server.port;
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    assert_eq!(post(port, "/api/runs/couple/input", &move_in).0, 200);
    let step = |step: &str| {
        let body = format!(r#"{{"step":"{step}"}}"#);
        assert_eq!(
            post(port, "/api/runs/couple/advance", &body).0,
            200,
            "{step}"
        );
    };
    step("morning");
    let house = || json_of(&get(port, &format!("/api/runs/couple/house/{home}")).2);
    let idle = |person: &Value| person["action"].is_null() && person["place"]["room"].is_u64();
    // Ten minutes at a time from 6:00 until one of them stands idle, by 11:00 at the latest.
    let mut found = None;
    for _ in 0..30 {
        step("minutes10");
        let now = house();
        let persons = now["persons"].as_array().expect("persons").clone();
        if let Some(index) = persons.iter().position(idle) {
            found = Some((now, index));
            break;
        }
    }
    let (now, index) = found.expect("one of them idle in the morning");
    let clock = now["at"]["t"].as_i64().expect("the clock");
    let person = &now["persons"][index];
    let other = &now["persons"][1 - index];
    let sim = person["sim"].as_u64().expect("a sim");
    let room = person["place"]["room"].as_u64().expect("a room");
    let busy: Vec<u64> = (other["action"]["object"].as_u64())
        .filter(|&object| {
            let object = usize::try_from(object).expect("an index");
            now["layout"]["objects"][object]["slots"] == 1
        })
        .into_iter()
        .collect();
    // The person's choices since the morning, a day's worth on one page.
    let choices = || -> Vec<Value> {
        let path = format!("/api/runs/couple/events?subject={sim}&from={MORNING}");
        let page = json_of(&get(port, &path).2);
        assert_eq!(page["next"], Value::Null);
        (page["events"].as_array().expect("events").iter())
            .map(|row| row["entry"].clone())
            .filter(|entry| {
                let payload = &entry["payload"];
                payload.get("core:idle").is_some() || payload.get("core:action_started").is_some()
            })
            .collect()
    };
    let all = choices();
    let (last, earlier) = all.split_last().expect("its choices");
    assert!(last["payload"].get("core:idle").is_some(), "{last}");
    let why = |entry: &Value| {
        let (status, _, body) = get(port, &format!("/api/runs/couple/why/{}", entry["id"]));
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        json_of(&body)
    };
    let compared = why(last)["now"].clone();
    assert_eq!(compared["at"], dated(clock));
    assert_eq!(compared["needs"], person["needs"]);
    // The same comparison as for an idle choice in that second, from that room, with those
    // needs and objects.
    let mut asked = (*last).clone();
    asked["t"] = json!(clock);
    asked["payload"] = json!({"core:idle": {"house": "core:flat", "room": room,
        "needs": person["needs"], "busy": busy, "waits": null, "held": [], "tier": []}});
    let asked: sim_core::log::LogEntry = serde_json::from_value(asked).expect("an entry");
    let world = sim_cli::load_world(&run).expect("the world");
    let same = sim_cli::why_answer(&world, &asked, 0, false, None)
        .expect("answers")
        .expect("a choice");
    let open = |answer: &Value| -> Vec<Value> {
        (answer["offers"].as_array().expect("offers").iter())
            .filter(|offer| offer["closed"] == false)
            .cloned()
            .collect()
    };
    assert_eq!(open(&compared), open(&same));
    let toilet = (compared["offers"].as_array().expect("offers").iter())
        .find(|offer| offer["object"] == 5)
        .expect("the toilet");
    let bladder = person["needs"][2].as_i64().expect("the bladder");
    let missing = 1_000 - bladder;
    let weight = (700 - bladder).max(0);
    let key = LineageKey::founder(&DataId::new("test:house").expect("valid"), sim - 1);
    let felt = i64::from(mood(7, key, SimTime::from_secs(clock), (5, 0), 10));
    let urgent = if bladder <= 150 {
        (151 - bladder) * 1_000_000_000
    } else {
        0
    };
    let walk = TO_BATH[usize::try_from(room).expect("an index")] * 50;
    assert_eq!(
        toilet["points"],
        json!(weight * missing * (100 + felt) / 100 - walk + urgent),
        "{compared}"
    );
    assert_eq!(toilet["mood"], json!(felt));
    for entry in earlier {
        assert_eq!(why(entry)["now"], Value::Null, "{entry}");
    }
    // Ten minutes at a time until the person chooses again, by bedtime at the latest: until
    // then its last idle choice is compared at each clock, after it no more.
    for _ in 0..30 {
        step("minutes10");
        let later = house();
        let again = (later["persons"].as_array().expect("persons").iter())
            .find(|p| p["sim"] == json!(sim))
            .expect("the person");
        let newest = choices().last().map(|entry| entry["id"].clone());
        if !idle(again) || newest.as_ref() != Some(&last["id"]) {
            assert_eq!(why(last)["now"], Value::Null);
            return;
        }
        assert_eq!(why(last)["now"]["at"], later["at"]);
    }
    panic!("the person never chose again");
}

/// The households with a house (ADR-0021, addendum of 2026-11-03): none, then the couple's,
/// before and after its first morning.
#[test]
fn houses_over_http() {
    let (server, home, _) = couple("houses");
    let port = server.port;
    let houses = || {
        let (status, _, body) = get(port, "/api/runs/couple/houses");
        assert_eq!(status, 200);
        let answer = json_of(&body);
        assert_eq!(
            (&answer["format"], &answer["version"]),
            (&json!("simagent.view.houses"), &json!(1))
        );
        answer["households"].clone()
    };
    assert_eq!(houses(), json!([]));
    let move_in = format!(r#"{{"move_in":{{"household":{home}}}}}"#);
    assert_eq!(post(port, "/api/runs/couple/input", &move_in).0, 200);
    assert_eq!(
        houses(),
        json!([{"household": home, "house": "core:flat", "morning": false}])
    );
    assert_eq!(
        post(port, "/api/runs/couple/advance", r#"{"step":"morning"}"#).0,
        200
    );
    assert_eq!(
        houses(),
        json!([{"household": home, "house": "core:flat", "morning": true}])
    );
}
