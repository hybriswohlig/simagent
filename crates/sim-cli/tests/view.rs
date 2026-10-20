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
        fields.insert("version".into(), 1.into());
        fields.insert(
            "contract_version".into(),
            sim_core::body_contract::CONTRACT_VERSION.into(),
        );
        assert_eq!(json_of(&body), expected, "{sim}");
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

/// Founding a town is `run --scenario <template> --until -1` (ADR-0021 section 2): the run
/// stands before its first second, every file byte for byte; a given seed replaces the
/// template's. A name in use, a bad name, an unknown template or a bad seed make nothing; the
/// new run is the server's until it ends.
#[test]
fn founding_a_town_is_the_template_before_its_start() {
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
            "-1".as_ref(),
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
    // Listed, standing before its first second (the last second of year -1, ADR-0002).
    let list = json_of(&get(port, "/api/runs").2);
    let entry = (list["runs"].as_array().expect("runs"))
        .iter()
        .find(|entry| entry["run"] == "neu")
        .cloned()
        .expect("listed");
    assert_eq!(entry["date"], date(-1));
    assert_eq!(entry["date"]["year"], -1);
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
