//! The HTTP of `simagent view` (ADR-0021 section 1): a small part of HTTP/1.1, one request
//! per connection (`Connection: close`), with the limits of the ADR.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// The request line and the headers together.
pub const HEAD_LIMIT: usize = 8 * 1024;
/// The body.
pub const BODY_LIMIT: usize = 1024 * 1024;
/// The whole request, from the first byte awaited to the last.
pub const READ_TIME: Duration = Duration::from_secs(5);

/// A request as the server reads it.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path of the target, without its query.
    pub path: String,
    pub query: Option<String>,
    /// Names in lower case, in the order of the request.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The value of the header `name` (lower case) if it appears exactly once.
    pub fn header(&self, name: &str) -> Option<&str> {
        let mut values = self.headers.iter().filter(|(n, _)| n == name);
        match (values.next(), values.next()) {
            (Some((_, value)), None) => Some(value),
            _ => None,
        }
    }
}

/// An answer: status, type and body; `allow` for a 405.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub allow: Option<&'static str>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type,
            body,
            allow: None,
        }
    }

    /// A JSON error of the server: `{"format":"simagent.view.error","version":1,"code":…}`.
    pub fn error(status: u16, code: &str) -> Response {
        let body = serde_json::json!({
            "format": "simagent.view.error",
            "version": 1,
            "code": code,
        });
        Response::new(status, JSON, format!("{body}\n").into_bytes())
    }
}

pub const JSON: &str = "application/json";

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

/// The head of a request: its line and its headers, without the blank line.
pub fn parse_head(head: &[u8]) -> Result<Request, Response> {
    let bad = || Response::error(400, "bad_request");
    let text = std::str::from_utf8(head).map_err(|_| bad())?;
    let mut lines = text.split("\r\n");
    let line = lines.next().ok_or_else(bad)?;
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    let token = !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase());
    if !token || !target.starts_with('/') || !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(bad());
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query.to_owned())),
        None => (target, None),
    };
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(bad)?;
        let name_fits = !name.is_empty() && name.bytes().all(|b| b.is_ascii_graphic());
        if !name_fits {
            return Err(bad());
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
    }
    Ok(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        query,
        headers,
        body: Vec::new(),
    })
}

/// Reads one request within [`READ_TIME`] and the limits; a refusal is the answer to send.
pub fn read_request(stream: &mut TcpStream) -> Result<Request, Response> {
    let deadline = Instant::now() + READ_TIME;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let end = loop {
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if buffer.len() > HEAD_LIMIT {
            return Err(Response::error(431, "head_too_large"));
        }
        let read = read_some(stream, &mut chunk, deadline)?;
        buffer.extend_from_slice(&chunk[..read]);
    };
    if end > HEAD_LIMIT {
        return Err(Response::error(431, "head_too_large"));
    }
    let mut request = parse_head(&buffer[..end])?;
    if request.header("transfer-encoding").is_some() {
        return Err(Response::error(400, "bad_request"));
    }
    let length = match request.header("content-length") {
        Some(text) => text
            .parse::<usize>()
            .map_err(|_| Response::error(400, "bad_request"))?,
        None if request.headers.iter().any(|(n, _)| n == "content-length") => {
            return Err(Response::error(400, "bad_request"));
        }
        None => 0,
    };
    if length > BODY_LIMIT {
        return Err(Response::error(413, "body_too_large"));
    }
    let mut body = buffer.split_off(end + 4);
    while body.len() < length {
        let read = read_some(stream, &mut chunk, deadline)?;
        body.extend_from_slice(&chunk[..read]);
    }
    if body.len() > length {
        return Err(Response::error(400, "bad_request"));
    }
    request.body = body;
    Ok(request)
}

/// One read before the deadline; the end of the stream before a whole request is a 400.
fn read_some(
    stream: &mut TcpStream,
    chunk: &mut [u8],
    deadline: Instant,
) -> Result<usize, Response> {
    let timeout = || Response::error(408, "read_timeout");
    let left = deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(timeout)?;
    stream.set_read_timeout(Some(left)).map_err(|_| timeout())?;
    match stream.read(chunk) {
        Ok(0) => Err(Response::error(400, "bad_request")),
        Ok(read) => Ok(read),
        Err(_) => Err(timeout()),
    }
}

/// Writes the answer with the headers every answer carries.
pub fn write_response(stream: &mut impl Write, response: &Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    if let Some(allow) = response.allow {
        head.push_str(&format!("Allow: {allow}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}
