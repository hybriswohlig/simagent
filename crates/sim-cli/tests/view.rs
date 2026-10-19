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
const STATUS_LINES: [(u16, &str); 9] = [
    (200, "HTTP/1.1 200 OK"),
    (400, "HTTP/1.1 400 Bad Request"),
    (404, "HTTP/1.1 404 Not Found"),
    (405, "HTTP/1.1 405 Method Not Allowed"),
    (408, "HTTP/1.1 408 Request Timeout"),
    (413, "HTTP/1.1 413 Content Too Large"),
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
    assert_eq!(
        list,
        serde_json::json!({
            "format": "simagent.view.runs",
            "version": 1,
            "runs": [
                {"run": "broken", "error": "manifest_invalid"},
                {"run": "fork.b", "manifest": manifest("fork.b")},
                {"run": "shaped", "error": "manifest_invalid"},
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
