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
use crate::proxy::ProxySettings;

/// Redirects followed before giving up. This matches the default of common HTTP clients.
const MAX_REDIRECTS: usize = 10;
/// Upper bound on the response headers we will buffer.
const MAX_HEADER_BYTES: usize = 64 * 1024;

/// Options for [`ping_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingOptions {
    /// The time limit for each attempt, covering the whole request.
    pub timeout: Duration,
    /// Extra attempts after a transient failure (a refused or reset connection, or a timeout).
    pub retries: u32,
    /// Extra request headers, as `Name: value` lines.
    pub headers: Vec<String>,
    /// The proxy to use. [`PingOptions::new`] sets none; see [`PingOptions::with_env_proxy`].
    pub proxy: ProxySettings,
}

impl PingOptions {
    /// Options with the given timeout, no retries, no extra headers and no proxy.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout, retries: 0, headers: Vec::new(), proxy: ProxySettings::none() }
    }

    /// Uses `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` from the environment.
    pub fn with_env_proxy(mut self) -> Result<Self, TptError> {
        self.proxy = ProxySettings::from_env()?;
        Ok(self)
    }
}

/// Time spent in each stage of the last request, in milliseconds.
///
/// `tls_ms` is `None` for plain `http` URLs. Redirects add their stages together.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Timings {
    /// Name lookup.
    pub dns_ms: f64,
    /// TCP connect.
    pub connect_ms: f64,
    /// TLS handshake, or `None` for plain http.
    pub tls_ms: Option<f64>,
    /// From sending the request until the response headers arrived.
    pub ttfb_ms: f64,
}

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
    /// The timeout that applied to each attempt, in seconds.
    pub timeout_secs: u64,
    /// How many attempts were made (1 when nothing needed a retry).
    pub attempts: u32,
    /// Per-stage timings of the successful attempt.
    pub timings: Timings,
}

/// Sends an HTTP GET to `url` and reports the status and latency.
///
/// Any HTTP status counts as a reply. Only failures to get a reply are errors.
/// Only `http` and `https` URLs are accepted. Redirects are followed. The timeout covers the
/// whole request, including DNS, connecting, the TLS handshake and every redirect.
pub fn ping(url: &str, timeout: Duration) -> Result<PingReport, TptError> {
    ping_with(url, &PingOptions::new(timeout))
}

/// Like [`ping`], with retries and extra headers.
///
/// Retries apply only to transient failures: a refused or reset connection, or a timeout.
/// They wait 200 ms, then 400 ms, and so on, up to 2 s. Each attempt gets the full timeout.
///
/// ```
/// use std::time::Duration;
/// use tpt::net::{PingOptions, ping_with};
///
/// let mut options = PingOptions::new(Duration::from_secs(5));
/// // A malformed header is rejected before any network activity.
/// options.headers = vec!["bad header".into()];
/// assert!(ping_with("https://example.com", &options).is_err());
/// ```
pub fn ping_with(url: &str, options: &PingOptions) -> Result<PingReport, TptError> {
    let timeout = options.timeout;
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
    let headers = parse_headers(&options.headers)?;

    let target = parsed.to_string();
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let mut timings = Timings::default();
        let started = Instant::now();
        match fetch(parsed.clone(), timeout, &headers, &options.proxy, &mut timings) {
            Ok(reply) => {
                let latency = started.elapsed();
                let reason = http::StatusCode::from_u16(reply.status)
                    .ok()
                    .and_then(|status| status.canonical_reason())
                    .unwrap_or("");
                return Ok(PingReport {
                    url: target,
                    status: reply.status,
                    reason: reason.to_owned(),
                    latency_ms: round_to_hundredths(latency.as_secs_f64() * 1000.0),
                    timeout_secs: timeout.as_secs(),
                    attempts: attempt,
                    timings: round_timings(timings),
                });
            }
            Err(fail) if attempt <= options.retries && fail.is_transient() => {
                thread::sleep(backoff(attempt));
            }
            Err(fail) => return Err(fail.into_error(&target, timeout)),
        }
    }
}

/// Delay before retry number `attempt` (1-based): 200 ms, 400 ms, ... capped at 2 s.
fn backoff(attempt: u32) -> Duration {
    let millis = 200u64.saturating_mul(1u64 << (attempt - 1).min(10));
    Duration::from_millis(millis.min(2_000))
}

/// Checks the `Name: value` header lines. Names that the client sets itself are refused.
fn parse_headers(lines: &[String]) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, TptError> {
    lines
        .iter()
        .map(|line| {
            let usage = |why: String| TptError::Usage(format!("invalid header `{line}`: {why}"));
            let (name, value) = line.split_once(':').ok_or_else(|| usage("expected `Name: value`".into()))?;
            let name =
                http::HeaderName::from_bytes(name.trim().as_bytes()).map_err(|e| usage(e.to_string()))?;
            if matches!(name.as_str(), "host" | "connection" | "content-length") {
                return Err(usage(format!("`{name}` is set by tptcli and cannot be overridden")));
            }
            let value = http::HeaderValue::from_str(value.trim()).map_err(|e| usage(e.to_string()))?;
            Ok((name, value))
        })
        .collect()
}

fn round_timings(t: Timings) -> Timings {
    Timings {
        dns_ms: round_to_hundredths(t.dns_ms),
        connect_ms: round_to_hundredths(t.connect_ms),
        tls_ms: t.tls_ms.map(round_to_hundredths),
        ttfb_ms: round_to_hundredths(t.ttfb_ms),
    }
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

    /// Timeouts and refused, reset or unreachable connections can succeed on a retry.
    fn is_transient(&self) -> bool {
        match self {
            Fail::Timeout => true,
            Fail::Net { kind, .. } => *kind == NetworkKind::Connect,
        }
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
fn fetch(
    start: Url,
    timeout: Duration,
    headers: &[(http::HeaderName, http::HeaderValue)],
    proxy: &ProxySettings,
    timings: &mut Timings,
) -> Result<Reply, Fail> {
    let deadline = Instant::now() + timeout;
    let mut tls: Option<Arc<ClientConfig>> = None;
    let mut current = start;
    let mut hops = 0usize;
    loop {
        let reply = request_once(&current, deadline, &mut tls, headers, proxy, timings)?;
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

/// Makes one request to `url` without following redirects. Goes through the proxy, if one applies.
fn request_once(
    url: &Url,
    deadline: Instant,
    tls: &mut Option<Arc<ClientConfig>>,
    headers: &[(http::HeaderName, http::HeaderValue)],
    proxy: &ProxySettings,
    timings: &mut Timings,
) -> Result<Reply, Fail> {
    let host = url.host_str().ok_or_else(|| Fail::net(NetworkKind::Other, "the URL has no host"))?;
    let port =
        url.port_or_known_default().ok_or_else(|| Fail::net(NetworkKind::Other, "the URL has no port"))?;
    // IPv6 literals come back in brackets. Socket and TLS APIs want them bare.
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');

    // With a proxy, the socket goes to the proxy. The origin host is still used for TLS and Host.
    let route = proxy.for_url(url).cloned();
    let (connect_host, connect_port) = match &route {
        Some(p) => (p.host.clone(), p.port),
        None => (bare_host.to_owned(), port),
    };

    let step = Instant::now();
    let addrs = resolve(&connect_host, connect_port, deadline)?;
    timings.dns_ms += ms_since(step);

    let step = Instant::now();
    let mut tcp = connect(&addrs, deadline)?;
    timings.connect_ms += ms_since(step);

    let absolute = route.is_some() && url.scheme() == "http";
    let request = build_request(url, headers, absolute);

    if url.scheme() == "https" {
        if route.is_some() {
            tunnel(&mut tcp, bare_host, port, deadline)?;
        }
        return over_tls(tcp, bare_host, &request, deadline, tls, timings);
    }
    let step = Instant::now();
    let reply = exchange(&mut tcp, deadline, &request)?;
    timings.ttfb_ms += ms_since(step);
    Ok(reply)
}

/// Runs TLS over `tcp` and sends the request through it.
fn over_tls(
    mut tcp: TcpStream,
    host: &str,
    request: &[u8],
    deadline: Instant,
    tls: &mut Option<Arc<ClientConfig>>,
    timings: &mut Timings,
) -> Result<Reply, Fail> {
    let config = tls_config(tls)?;
    let name = ServerName::try_from(host.to_owned())
        .map_err(|e| Fail::net(NetworkKind::Tls, format!("invalid server name `{host}`: {e}")))?;
    let mut conn =
        ClientConnection::new(config, name).map_err(|e| Fail::net(NetworkKind::Tls, e.to_string()))?;
    let step = Instant::now();
    handshake(&mut conn, &mut tcp, deadline)?;
    *timings.tls_ms.get_or_insert(0.0) += ms_since(step);
    let mut stream = StreamOwned::new(conn, tcp);
    let step = Instant::now();
    let reply = exchange(&mut stream, deadline, request)?;
    timings.ttfb_ms += ms_since(step);
    Ok(reply)
}

/// Asks the proxy for a tunnel to `host:port` with `CONNECT`. Fails unless the proxy answers 200.
fn tunnel(tcp: &mut TcpStream, host: &str, port: u16, deadline: Instant) -> Result<(), Fail> {
    let target = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
    arm(tcp, deadline)?;
    tcp.write_all(request.as_bytes()).map_err(classify_io)?;
    let head = read_head(tcp, deadline)?;
    let reply = parse_reply(&head)?;
    if reply.status != 200 {
        return Err(Fail::net(
            NetworkKind::Connect,
            format!("the proxy refused the tunnel to {target} (HTTP {})", reply.status),
        ));
    }
    Ok(())
}

fn ms_since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
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

/// The request line and headers for a `GET` of `url`, with any extra headers.
/// `absolute` puts the full URL on the request line, as a proxy expects.
fn build_request(url: &Url, extra: &[(http::HeaderName, http::HeaderValue)], absolute: bool) -> Vec<u8> {
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    // Through a plain-http proxy the request line carries the full URL instead.
    let target = if absolute { url.as_str().to_owned() } else { path };
    let host = url.host_str().unwrap_or_default();
    let host_header = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    let mut request = format!(
        "GET {target} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: tpt-cli/{}\r\nAccept: */*\r\nConnection: close\r\n",
        env!("CARGO_PKG_VERSION")
    );
    for (name, value) in extra {
        request.push_str(&format!("{name}: {}\r\n", value.to_str().unwrap_or_default()));
    }
    request.push_str("\r\n");
    request.into_bytes()
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
    fn extra_headers_are_sent_and_reserved_ones_refused() {
        let url = Url::parse("http://example.test/").unwrap();
        let headers = parse_headers(&["X-Trace: abc".to_owned()]).unwrap();
        let text = String::from_utf8(build_request(&url, &headers, false)).unwrap();
        // The `http` crate stores header names in lower case. HTTP treats names as case-insensitive.
        assert!(text.contains("\r\nx-trace: abc\r\n"));
        assert!(parse_headers(&["Host: evil.test".to_owned()]).is_err());
        assert!(parse_headers(&["no colon".to_owned()]).is_err());
        assert!(parse_headers(&["X-Bad: a\r\nInjected: 1".to_owned()]).is_err());
    }

    /// A deterministic pseudo-random byte stream, so the test is repeatable. It feeds the response
    /// parser arbitrary input, some of it shaped like HTTP, and checks that nothing panics. This is
    /// not coverage-guided fuzzing, which needs nightly. It only guards the parser against regressions.
    #[test]
    fn response_parser_survives_arbitrary_bytes() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let fragments: [&[u8]; 6] = [b"HTTP/1.1 ", b"\r\n", b"\r\n\r\n", b"Location: ", b"999", b":"];
        for round in 0..2000 {
            let len = (next() % 200) as usize;
            let mut raw: Vec<u8> = Vec::with_capacity(len + 16);
            if round % 2 == 0 {
                raw.extend_from_slice(fragments[(next() % fragments.len() as u64) as usize]);
            }
            for _ in 0..len {
                raw.push((next() & 0xFF) as u8);
            }
            let _ = head_end(&raw);
            let _ = parse_reply(&raw);
        }
    }

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!(backoff(1), Duration::from_millis(200));
        assert_eq!(backoff(2), Duration::from_millis(400));
        assert_eq!(backoff(20), Duration::from_secs(2));
    }

    #[test]
    fn refused_connections_are_retried_then_reported() {
        // Bind and drop a listener to get a local port that refuses connections.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}/");
        let mut options = PingOptions::new(Duration::from_secs(2));
        options.retries = 1;
        let err = ping_with(&url, &options).unwrap_err();
        // Windows can report a refused local connection as a timeout. Both are transient.
        assert!(
            matches!(err, TptError::Timeout { .. } | TptError::Network { kind: NetworkKind::Connect, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn successful_ping_reports_attempts_and_timings() {
        let base = serve(vec![OK]);
        let report = ping(&base, Duration::from_secs(5)).unwrap();
        assert_eq!(report.attempts, 1);
        assert_eq!(report.timings.tls_ms, None, "plain http has no TLS stage");
        assert!(report.timings.ttfb_ms >= 0.0);
    }

    /// A one-shot fake proxy. It sends the request head it received over the returned channel,
    /// then answers with `reply`. Returns the proxy URL.
    fn fake_proxy(reply: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut seen = Vec::new();
            let mut buf = [0u8; 512];
            while head_end(&seen).is_none() {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => seen.extend_from_slice(&buf[..n]),
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&seen).into_owned());
            let _ = stream.write_all(reply.as_bytes());
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    #[test]
    fn plain_http_through_a_proxy_sends_the_absolute_url() {
        let (proxy, seen) = fake_proxy(OK);
        let mut options = PingOptions::new(Duration::from_secs(5));
        options.proxy = crate::proxy::ProxySettings::explicit(Some(proxy.as_str()), None).unwrap();
        let report = ping_with("http://origin.invalid/path?q=1", &options).unwrap();
        assert_eq!(report.status, 200);
        let head = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(head.starts_with("GET http://origin.invalid/path?q=1 HTTP/1.1\r\n"), "{head}");
    }

    #[test]
    fn https_through_a_proxy_asks_for_a_connect_tunnel() {
        let (proxy, seen) = fake_proxy("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        let mut options = PingOptions::new(Duration::from_secs(5));
        options.proxy = crate::proxy::ProxySettings::explicit(None, Some(proxy.as_str())).unwrap();
        let err = ping_with("https://origin.invalid/", &options).unwrap_err();
        let head = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(head.starts_with("CONNECT origin.invalid:443 HTTP/1.1\r\n"), "{head}");
        assert!(err.to_string().contains("refused the tunnel"), "{err}");
    }

    #[test]
    fn request_has_host_path_and_connection_close() {
        let url = Url::parse("http://example.test:8080/a/b?x=1").unwrap();
        let text = String::from_utf8(build_request(&url, &[], false)).unwrap();
        assert!(text.starts_with("GET /a/b?x=1 HTTP/1.1\r\n"));
        assert!(text.contains("\r\nHost: example.test:8080\r\n"));
        assert!(text.contains("\r\nConnection: close\r\n\r\n"));
    }
}
