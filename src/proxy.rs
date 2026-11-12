//! Proxy settings from the environment: `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY`
//! (the lower-case forms are also read).
//!
//! Only `http://` proxy URLs are supported. A plain `http` request goes to the proxy with the
//! full URL on the request line. An `https` request asks the proxy for a `CONNECT` tunnel, and
//! TLS runs inside it, so the proxy only sees the host name. Proxy credentials in the URL
//! (`http://user:pass@proxy`) are not supported yet.

use url::Url;

use crate::errors::TptError;

/// A proxy to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proxy {
    /// The proxy host name or address, without brackets.
    pub host: String,
    /// The proxy port.
    pub port: u16,
}

/// The proxy configuration that applies to requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxySettings {
    http: Option<Proxy>,
    https: Option<Proxy>,
    no_proxy: Vec<String>,
}

impl ProxySettings {
    /// No proxy at all. Every request connects directly.
    pub fn none() -> Self {
        Self::default()
    }

    /// Sets the proxies explicitly, without reading the environment. Each value is an `http://` URL.
    ///
    /// ```
    /// use tpt::proxy::ProxySettings;
    ///
    /// assert!(ProxySettings::explicit(Some("http://proxy.test:3128"), None).is_ok());
    /// assert!(ProxySettings::explicit(Some("https://proxy.test"), None).is_err());
    /// ```
    pub fn explicit(http: Option<&str>, https: Option<&str>) -> Result<Self, TptError> {
        Ok(Self {
            http: http.map(|v| parse_proxy("HTTP_PROXY", v)).transpose()?,
            https: https.map(|v| parse_proxy("HTTPS_PROXY", v)).transpose()?,
            no_proxy: Vec::new(),
        })
    }

    /// Reads `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY`. Empty values count as unset.
    ///
    /// Fails if a proxy variable is set but is not a usable `http://` URL.
    pub fn from_env() -> Result<Self, TptError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, TptError> {
        let pick = |upper: &str, lower: &str| {
            get(upper).or_else(|| get(lower)).filter(|value| !value.trim().is_empty())
        };
        let http = pick("HTTP_PROXY", "http_proxy").map(|v| parse_proxy("HTTP_PROXY", &v)).transpose()?;
        let https = pick("HTTPS_PROXY", "https_proxy").map(|v| parse_proxy("HTTPS_PROXY", &v)).transpose()?;
        let no_proxy = pick("NO_PROXY", "no_proxy")
            .map(|list| {
                list.split(',')
                    .map(|entry| entry.trim().to_ascii_lowercase())
                    .filter(|entry| !entry.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { http, https, no_proxy })
    }

    /// The proxy to use for `url`, or `None` to connect directly.
    ///
    /// ```
    /// use tpt::proxy::ProxySettings;
    /// use url::Url;
    ///
    /// let settings = ProxySettings::none();
    /// assert!(settings.for_url(&Url::parse("https://example.com").unwrap()).is_none());
    /// ```
    pub fn for_url(&self, url: &Url) -> Option<&Proxy> {
        let host = url.host_str()?.trim_start_matches('[').trim_end_matches(']');
        if no_proxy_matches(&self.no_proxy, host) {
            return None;
        }
        match url.scheme() {
            "http" => self.http.as_ref(),
            "https" => self.https.as_ref(),
            _ => None,
        }
    }
}

fn parse_proxy(var: &str, value: &str) -> Result<Proxy, TptError> {
    let url = Url::parse(value).map_err(|e| TptError::Usage(format!("{var} is not a valid URL: {e}")))?;
    if url.scheme() != "http" {
        return Err(TptError::Usage(format!(
            "{var} must be an http:// URL. HTTPS to the proxy itself is not supported."
        )));
    }
    let host = url
        .host_str()
        .ok_or_else(|| TptError::Usage(format!("{var} has no host name")))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = url.port_or_known_default().unwrap_or(80);
    Ok(Proxy { host, port })
}

/// Whether `host` is listed in `NO_PROXY` entries.
///
/// `*` matches every host. `example.com` and `.example.com` (or `*.example.com`) match
/// `example.com` and all of its subdomains. A port in an entry is ignored.
///
/// ```
/// use tpt::proxy::no_proxy_matches;
///
/// let list = vec!["example.com".to_owned()];
/// assert!(no_proxy_matches(&list, "api.example.com"));
/// assert!(!no_proxy_matches(&list, "notexample.com"));
/// ```
pub fn no_proxy_matches(entries: &[String], host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    entries.iter().any(|entry| {
        if entry == "*" {
            return true;
        }
        let name = entry.split(':').next().unwrap_or("");
        let suffix = name.trim_start_matches("*.").trim_start_matches('.');
        !suffix.is_empty() && (host == suffix || host.ends_with(&format!(".{suffix}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(vars: &[(&str, &str)]) -> Result<ProxySettings, TptError> {
        ProxySettings::from_vars(|name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()))
    }

    #[test]
    fn reads_proxy_variables_and_lower_case_fallbacks() {
        let s = settings(&[("http_proxy", "http://proxy.test:3128"), ("HTTPS_PROXY", "http://sec.test")])
            .unwrap();
        let http = s.for_url(&Url::parse("http://a.test/").unwrap()).unwrap();
        assert_eq!(http, &Proxy { host: "proxy.test".into(), port: 3128 });
        let https = s.for_url(&Url::parse("https://a.test/").unwrap()).unwrap();
        assert_eq!(https, &Proxy { host: "sec.test".into(), port: 80 });
    }

    #[test]
    fn empty_values_are_unset() {
        let s = settings(&[("HTTP_PROXY", "   ")]).unwrap();
        assert_eq!(s, ProxySettings::none());
    }

    #[test]
    fn https_proxy_urls_are_rejected() {
        assert!(matches!(settings(&[("HTTPS_PROXY", "https://p.test")]), Err(TptError::Usage(_))));
        assert!(matches!(settings(&[("HTTP_PROXY", "not a url")]), Err(TptError::Usage(_))));
    }

    #[test]
    fn no_proxy_skips_the_proxy_for_listed_hosts() {
        let s = settings(&[("HTTP_PROXY", "http://p.test:8080"), ("NO_PROXY", "internal.test, .corp.test")])
            .unwrap();
        assert!(s.for_url(&Url::parse("http://internal.test/").unwrap()).is_none());
        assert!(s.for_url(&Url::parse("http://x.corp.test/").unwrap()).is_none());
        assert!(s.for_url(&Url::parse("http://public.test/").unwrap()).is_some());
    }

    #[test]
    fn no_proxy_wildcard_and_suffix_rules() {
        let all = vec!["*".to_owned()];
        assert!(no_proxy_matches(&all, "anything.test"));
        let suffix = vec!["*.corp.test".to_owned(), "host.test:8080".to_owned()];
        assert!(no_proxy_matches(&suffix, "a.corp.test"));
        assert!(no_proxy_matches(&suffix, "corp.test"));
        assert!(no_proxy_matches(&suffix, "host.test"));
        assert!(!no_proxy_matches(&suffix, "evilcorp.test"));
    }
}
