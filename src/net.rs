//! HTTP helpers. Compiled only with the `net` feature.
//!
//! This is a small blocking HTTP/1.1 client: one `GET` per hop, `Connection: close`,
//! and only the response headers are read. TLS comes from the pure-Rust
//! `tpt-crypto-rustls` provider. Trust comes from the operating system's root store.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde::Serialize;
use url::Url;

use crate::errors::{NetworkKind, TptError};

/// Redirects followed before giving up. This matches the default of common HTTP clients.
const MAX_REDIRECTS: usize = 10;
/// Upper bound on the response headers we will buffer.
const MAX_HEADER_BYTES: usize = 64 * 1024;

/// The result of a `ping`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct PingReport {
    /// The URL that was requested.
    pub url: String,
    /// HTTP status code of the response.
    pub status: u16,
    /// Canonical reason phrase, such as `OK`. Empty if unknown.
    pub reason: String,
    /// Time from sending the request to receiving response headers, in milliseconds.
    pub latency_ms: f64,
    /// The timeout that applied to the request, in seconds.
    pub timeout_secs: u64,
}

/// Sends an HTTP GET to `url` and reports the status and latency.
///
/// Any HTTP status counts as a reply. Only failures to get a reply are errors.
/// Only `http` and `https` URLs are accepted. Redirects are followed. The timeout covers the
/// whole request, including DNS, connecting, the TLS handshake and every redirect.
pub fn ping(url: &str, timeout: Duration) -> Result<PingReport, TptError> {
    if timeout.is_zero() {
        return Err(TptError::Usage("the timeout must be at least 1 second".into()));
    }
    let parsed = Url::parse(url).map_err(|e| TptError::Usage(format!("invalid URL `{url}`: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(TptError::Usage(format!(
            "unsupported URL scheme `{}`: use http or https",
            parsed.scheme()
        )));
    }

    let target = parsed.to_string();
    let started = Instant::now();
    let reply = fetch(parsed, timeout).map_err(|fail| fail.into_error(&target, timeout))?;
    let latency = started.elapsed();

    let reason = http::StatusCode::from_u16(reply.status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .unwrap_or("");
    Ok(PingReport {
        url: target,
        status: reply.status,
        reason: reason.to_owned(),
        latency_ms: round_to_hundredths(latency.as_secs_f64() * 1000.0),
        timeout_secs: timeout.as_secs(),
    })
}

/// A failure inside the client. It is converted to a [`TptError`] once the target URL is known.
#[derive(Debug)]
enum Fail {
    Timeout,
    Net { kind: NetworkKind, reason: String },
}

impl Fail {
    fn net(kind: NetworkKind, reason: impl Into<String>) -> Fail {
        Fail::Net { kind, reason: reason.into() }
    }

    fn into_error(self, url: &str, timeout: Duration) -> TptError {
        match self {
            Fail::Timeout => TptError::Timeout { url: url.to_owned(), timeout_secs: timeout.as_secs() },
            Fail::Net { kind, reason } => TptError::Network { url: url.to_owned(), kind, reason },
        }
    }
}

/// The status line and the `Location` header of one response.
#[derive(Debug, PartialEq)]
struct Reply {
    status: u16,
    location: Option<String>,
}

/// Follows redirects, under one deadline shared by every hop.
fn fetch(start: Url, timeout: Duration) -> Result<Reply, Fail> {
    let deadline = Instant::now() + timeout;
    let mut tls: Option<Arc<ClientConfig>> = None;
    let mut current = start;
    let mut hops = 0usize;
    loop {
        let reply = request_once(&current, deadline, &mut tls)?;
        let Some(location) = reply.location.as_deref().filter(|_| is_redirect(reply.status)) else {
            return Ok(reply);
        };
        if hops == MAX_REDIRECTS {
            return Err(Fail::net(
                NetworkKind::Other,
                format!("too many redirects (more than {MAX_REDIRECTS})"),
            ));
        }
        hops += 1;
        let next = current.join(location).map_err(|e| {
            Fail::net(NetworkKind::Other, format!("invalid redirect location `{location}`: {e}"))
        })?;
        if !matches!(next.scheme(), "http" | "https") {
            return Err(Fail::net(
                NetworkKind::Other,
                format!("redirect to unsupported scheme `{}`", next.scheme()),
            ));
        }
        current = next;
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Makes one request to `url` without following redirects.
fn request_once(url: &Url, deadline: Instant, tls: &mut Option<Arc<ClientConfig>>) -> Result<Reply, Fail> {
    let host = url.host_str().ok_or_else(|| Fail::net(NetworkKind::Other, "the URL has no host"))?;
    let port =
        url.port_or_known_default().ok_or_else(|| Fail::net(NetworkKind::Other, "the URL has no port"))?;
    // IPv6 literals come back in brackets. Socket and TLS APIs want them bare.
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');

    let addrs = resolve(bare_host, port, deadline)?;
    let mut tcp = connect(&addrs, deadline)?;
    let request = build_request(url);

    if url.scheme() == "https" {
        let config = tls_config(tls)?;
        let name = ServerName::try_from(bare_host.to_owned())
            .map_err(|e| Fail::net(NetworkKind::Tls, format!("invalid server name `{bare_host}`: {e}")))?;
        let mut conn =
            ClientConnection::new(config, name).map_err(|e| Fail::net(NetworkKind::Tls, e.to_string()))?;
        handshake(&mut conn, &mut tcp, deadline)?;
        let mut stream = StreamOwned::new(conn, tcp);
        exchange(&mut stream, deadline, &request)
    } else {
        exchange(&mut tcp, deadline, &request)
    }
}

/// The time left before `deadline`. Fails with a timeout once none is left.
fn remaining(deadline: Instant) -> Result<Duration, Fail> {
    deadline.checked_duration_since(Instant::now()).filter(|left| !left.is_zero()).ok_or(Fail::Timeout)
}

/// A byte stream that can report its socket, so timeouts are set on the handle that does the I/O.
///
/// Timeouts must not be set on a `try_clone`d handle. On Windows the clone does not apply
/// its timeouts to the original, and a read can then block forever.
trait Transport: Read + Write {
    fn socket(&self) -> &TcpStream;
}

impl Transport for TcpStream {
    fn socket(&self) -> &TcpStream {
        self
    }
}

impl Transport for StreamOwned<ClientConnection, TcpStream> {
    fn socket(&self) -> &TcpStream {
        &self.sock
    }
}

/// Sets the socket's read and write timeouts to the time left before `deadline`.
fn arm(socket: &TcpStream, deadline: Instant) -> Result<(), Fail> {
    let left = remaining(deadline)?;
    socket.set_read_timeout(Some(left)).map_err(classify_io)?;
    socket.set_write_timeout(Some(left)).map_err(classify_io)?;
    Ok(())
}

/// Resolves `host` on a helper thread, so the deadline can bound it. A plain
/// `to_socket_addrs` call cannot be interrupted.
fn resolve(host: &str, port: u16, deadline: Instant) -> Result<Vec<SocketAddr>, Fail> {
    let left = remaining(deadline)?;
    let (tx, rx) = mpsc::channel();
    let owned = host.to_owned();
    thread::spawn(move || {
        // The receiver may have gone away after a timeout. Nothing to do then.
        let _ = tx.send((owned.as_str(), port).to_socket_addrs().map(|addrs| addrs.collect::<Vec<_>>()));
    });
    match rx.recv_timeout(left) {
        Ok(Ok(addrs)) if !addrs.is_empty() => Ok(addrs),
        Ok(Ok(_)) => Err(Fail::net(NetworkKind::Dns, format!("no addresses found for {host}"))),
        Ok(Err(e)) => Err(Fail::net(NetworkKind::Dns, format!("could not resolve {host}: {e}"))),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Fail::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(Fail::net(NetworkKind::Dns, format!("name lookup for {host} failed")))
        }
    }
}

/// Connects to the first address that accepts the connection.
fn connect(addrs: &[SocketAddr], deadline: Instant) -> Result<TcpStream, Fail> {
    let mut last_error = None;
    for addr in addrs {
        let left = remaining(deadline)?;
        match TcpStream::connect_timeout(addr, left) {
            Ok(socket) => return Ok(socket),
            Err(e) => last_error = Some(e),
        }
    }
    Err(match last_error {
        Some(e) => classify_io(e),
        None => Fail::net(NetworkKind::Connect, "no address to connect to"),
    })
}

/// Builds the TLS client config: the `tpt-crypto-rustls` provider, the default
/// protocol versions, and the operating system's root certificates. It is built once per ping.
fn tls_config(cache: &mut Option<Arc<ClientConfig>>) -> Result<Arc<ClientConfig>, Fail> {
    if let Some(config) = cache {
        return Ok(Arc::clone(config));
    }

    let loaded = rustls_native_certs::load_native_certs();
    let mut roots = RootCertStore::empty();
    let mut rejected = 0usize;
    for cert in loaded.certs {
        if roots.add(cert).is_err() {
            rejected += 1;
        }
    }
    if roots.is_empty() {
        let reason = if rejected == 0 {
            "no root certificates were found in the operating system certificate store".to_owned()
        } else {
            format!("none of the {rejected} operating system root certificates could be used")
        };
        return Err(Fail::net(NetworkKind::Tls, reason));
    }

    let config = ClientConfig::builder_with_provider(Arc::new(tpt_crypto_rustls::provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| Fail::net(NetworkKind::Tls, format!("TLS configuration failed: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let config = Arc::new(config);
    *cache = Some(Arc::clone(&config));
    Ok(config)
}

/// Runs the TLS handshake to completion.
fn handshake(conn: &mut ClientConnection, tcp: &mut TcpStream, deadline: Instant) -> Result<(), Fail> {
    while conn.is_handshaking() {
        arm(tcp, deadline)?;
        conn.complete_io(tcp).map_err(classify_io)?;
    }
    Ok(())
}

/// Sends the request and reads the response headers. The body is never read.
fn exchange<S: Transport>(stream: &mut S, deadline: Instant, request: &[u8]) -> Result<Reply, Fail> {
    arm(stream.socket(), deadline)?;
    stream.write_all(request).map_err(classify_io)?;
    let head = read_head(stream, deadline)?;
    parse_reply(&head)
}

/// Reads until the blank line that ends the response headers.
fn read_head<S: Transport>(stream: &mut S, deadline: Instant) -> Result<Vec<u8>, Fail> {
    let mut head = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if head_end(&head).is_some() {
            return Ok(head);
        }
        if head.len() > MAX_HEADER_BYTES {
            return Err(Fail::net(NetworkKind::Other, "the response headers are too large"));
        }
        arm(stream.socket(), deadline)?;
        let read = stream.read(&mut chunk).map_err(classify_io)?;
        if read == 0 {
            return Err(Fail::net(
                NetworkKind::Other,
                "the connection closed before the response headers arrived",
            ));
        }
        head.extend_from_slice(&chunk[..read]);
    }
}

/// The index just past the `\r\n\r\n` that ends the headers, if it has arrived.
fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// Parses the status line and the `Location` header from the raw response headers.
fn parse_reply(raw: &[u8]) -> Result<Reply, Fail> {
    let invalid = |why: &str| Fail::net(NetworkKind::Other, format!("invalid HTTP response: {why}"));
    let end = head_end(raw).unwrap_or(raw.len());
    let text = String::from_utf8_lossy(&raw[..end]);
    let mut lines = text.split("\r\n");

    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(invalid("missing status line"));
    }
    let status = parts
        .next()
        .and_then(|code| code.parse::<u16>().ok())
        .filter(|code| (100..=999).contains(code))
        .ok_or_else(|| invalid("bad status code"))?;

    let location = lines.take_while(|line| !line.is_empty()).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim().eq_ignore_ascii_case("location").then(|| value.trim().to_owned())
    });
    Ok(Reply { status, location })
}

/// The request line and headers for a `GET` of `url`.
fn build_request(url: &Url) -> Vec<u8> {
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let host = url.host_str().unwrap_or_default();
    let host_header = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: tpt-cli/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    )
    .into_bytes()
}

/// Classifies an I/O error. TLS errors come through as `io::Error` wrapping a `rustls::Error`.
fn classify_io(err: io::Error) -> Fail {
    if err.get_ref().is_some_and(|inner| inner.downcast_ref::<rustls::Error>().is_some()) {
        return Fail::net(NetworkKind::Tls, err.to_string());
    }
    match err.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Fail::Timeout,
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::NotConnected
        | io::ErrorKind::HostUnreachable
        | io::ErrorKind::NetworkUnreachable => Fail::net(NetworkKind::Connect, err.to_string()),
        _ => Fail::net(NetworkKind::Other, err.to_string()),
    }
}

fn round_to_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Serves one canned response per connection, in order, on a random local port.
    /// Returns the base URL, which ends in `/`.
    fn serve(responses: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut seen = Vec::new();
                let mut buf = [0u8; 512];
                while head_end(&seen).is_none() {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}/")
    }

    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    const NO_CONTENT: &str = "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n";
    const TO_NEXT: &str =
        "HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    const TO_SELF: &str =
        "HTTP/1.1 301 Moved\r\nLocation: /\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    #[test]
    fn rejects_non_http_schemes() {
        let err = ping("ftp://example.com", Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, TptError::Usage(_)));
    }

    #[test]
    fn rejects_zero_timeout() {
        let err = ping("https://example.com", Duration::ZERO).unwrap_err();
        assert!(matches!(err, TptError::Usage(_)));
    }

    #[test]
    fn rejects_unparseable_urls() {
        let err = ping("not a url", Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, TptError::Usage(_)));
    }

    #[test]
    fn rounds_latency_to_hundredths() {
        assert_eq!(round_to_hundredths(1.23456), 1.23);
        assert_eq!(round_to_hundredths(0.005), 0.01);
    }

    #[test]
    fn parses_status_and_location() {
        let reply = parse_reply(TO_NEXT.as_bytes()).unwrap();
        assert_eq!(reply, Reply { status: 302, location: Some("/next".into()) });
        let reply = parse_reply(OK.as_bytes()).unwrap();
        assert_eq!(reply, Reply { status: 200, location: None });
    }

    #[test]
    fn location_header_name_is_case_insensitive() {
        let raw = b"HTTP/1.1 307 Temporary Redirect\r\nLOCATION:   https://x.test/a \r\n\r\n";
        let reply = parse_reply(raw).unwrap();
        assert_eq!(reply.location.as_deref(), Some("https://x.test/a"));
    }

    #[test]
    fn rejects_malformed_status_lines() {
        for raw in [&b"ICY 200 OK\r\n\r\n"[..], b"HTTP/1.1 abc\r\n\r\n", b"HTTP/1.1 99 X\r\n\r\n"] {
            let err = parse_reply(raw).unwrap_err();
            assert!(matches!(err, Fail::Net { kind: NetworkKind::Other, .. }), "{raw:?}");
        }
    }

    #[test]
    fn follows_redirects_and_reports_the_final_status() {
        let base = serve(vec![TO_NEXT, NO_CONTENT]);
        let start = format!("{base}start");
        let report = ping(&start, Duration::from_secs(5)).unwrap();
        assert_eq!(report.status, 204);
        assert_eq!(report.reason, "No Content");
        assert_eq!(report.url, Url::parse(&start).unwrap().to_string());
    }

    #[test]
    fn gives_up_after_too_many_redirects() {
        let base = serve(vec![TO_SELF; MAX_REDIRECTS + 1]);
        let err = ping(&base, Duration::from_secs(5)).unwrap_err();
        match err {
            TptError::Network { kind: NetworkKind::Other, reason, .. } => {
                assert!(reason.contains("too many redirects"), "{reason}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn request_has_host_path_and_connection_close() {
        let url = Url::parse("http://example.test:8080/a/b?x=1").unwrap();
        let text = String::from_utf8(build_request(&url)).unwrap();
        assert!(text.starts_with("GET /a/b?x=1 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: example.test:8080\r\n"));
        assert!(text.contains("\r\nConnection: close\r\n\r\n"));
    }
}
