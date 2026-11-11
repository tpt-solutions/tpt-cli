//! HTTP helpers. Compiled only with the `net` feature, which is the only
//! feature that pulls in `ring` (Apache-2.0 AND ISC).

use std::time::{Duration, Instant};

use reqwest::Url;
use serde::Serialize;

use crate::errors::{NetworkKind, TptError, error_chain};

/// The result of a `ping`.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
/// Only `http` and `https` URLs are accepted. Redirects are followed.
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

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(TptError::Io)?;
    runtime.block_on(ping_once(parsed, timeout))
}

async fn ping_once(url: Url, timeout: Duration) -> Result<PingReport, TptError> {
    let target = url.to_string();
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .use_rustls_tls()
        .build()
        .map_err(|e| classify(&target, timeout, &e))?;

    let started = Instant::now();
    let response = client.get(url).send().await.map_err(|e| classify(&target, timeout, &e))?;
    let latency = started.elapsed();

    let status = response.status();
    Ok(PingReport {
        url: target,
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_owned(),
        latency_ms: round_to_hundredths(latency.as_secs_f64() * 1000.0),
        timeout_secs: timeout.as_secs(),
    })
}

/// Maps a reqwest error to a typed error. Classification is by message text, because
/// reqwest does not expose DNS and TLS failures as separate types.
fn classify(url: &str, timeout: Duration, err: &reqwest::Error) -> TptError {
    if err.is_timeout() {
        return TptError::Timeout { url: url.to_owned(), timeout_secs: timeout.as_secs() };
    }
    let chain = error_chain(err);
    let lower = chain.to_ascii_lowercase();
    let kind = if lower.contains("dns error")
        || lower.contains("failed to lookup address")
        || lower.contains("no such host")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
    {
        NetworkKind::Dns
    } else if lower.contains("certificate") || lower.contains("tls") || lower.contains("handshake") {
        NetworkKind::Tls
    } else if err.is_connect() {
        NetworkKind::Connect
    } else {
        NetworkKind::Other
    };
    TptError::Network { url: url.to_owned(), kind, reason: chain }
}

fn round_to_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
