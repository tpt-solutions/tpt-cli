//! Typed errors, exit codes and hints.
//!
//! Every [`TptError`] maps to one stable error `code` (used in the JSON
//! envelope) and one exit code (listed in [`EXIT_CODES`]).

use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

/// Exit code for success.
pub const EXIT_SUCCESS: u8 = 0;
/// Exit code for unexpected internal errors (a bug).
pub const EXIT_INTERNAL: u8 = 1;
/// Exit code for invalid command-line usage.
pub const EXIT_USAGE: u8 = 2;
/// Exit code for configuration errors (malformed TOML, invalid values).
pub const EXIT_CONFIG: u8 = 3;
/// Exit code for a DNS lookup failure.
pub const EXIT_DNS: u8 = 4;
/// Exit code for a TLS or certificate failure.
pub const EXIT_TLS: u8 = 5;
/// Exit code for a failed connection (refused, reset, unreachable).
pub const EXIT_CONNECTION: u8 = 6;
/// Exit code for a request that timed out.
pub const EXIT_TIMEOUT: u8 = 7;
/// Exit code for other I/O errors.
pub const EXIT_IO: u8 = 8;
/// Exit code when the user aborts (for example with Ctrl-C at a prompt).
pub const EXIT_ABORTED: u8 = 130;

/// One row of the exit-code table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCodeInfo {
    /// The process exit code.
    pub code: u8,
    /// Stable short name.
    pub name: &'static str,
    /// What the code means.
    pub meaning: &'static str,
}

/// The exit-code table, as documented in the README and emitted by `tpt schema`.
pub const EXIT_CODES: &[ExitCodeInfo] = &[
    ExitCodeInfo { code: EXIT_SUCCESS, name: "success", meaning: "The command succeeded." },
    ExitCodeInfo {
        code: EXIT_INTERNAL,
        name: "internal",
        meaning: "An unexpected internal error (please report it).",
    },
    ExitCodeInfo { code: EXIT_USAGE, name: "usage", meaning: "Invalid command-line usage." },
    ExitCodeInfo { code: EXIT_CONFIG, name: "config", meaning: "A configuration file or value is invalid." },
    ExitCodeInfo { code: EXIT_DNS, name: "dns", meaning: "The host name could not be resolved." },
    ExitCodeInfo { code: EXIT_TLS, name: "tls", meaning: "The TLS handshake or certificate check failed." },
    ExitCodeInfo {
        code: EXIT_CONNECTION,
        name: "connection",
        meaning: "The connection was refused, reset or unreachable.",
    },
    ExitCodeInfo { code: EXIT_TIMEOUT, name: "timeout", meaning: "The request timed out." },
    ExitCodeInfo { code: EXIT_IO, name: "io", meaning: "A local file or I/O operation failed." },
    ExitCodeInfo { code: EXIT_ABORTED, name: "aborted", meaning: "The user aborted the operation." },
];

/// The category of a network failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkKind {
    /// DNS resolution failed.
    Dns,
    /// The TLS handshake or certificate validation failed.
    Tls,
    /// The connection was refused, reset or unreachable.
    Connect,
    /// Any other network failure.
    Other,
}

/// A configuration problem, optionally located in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The file the problem was found in, if any.
    pub path: Option<PathBuf>,
    /// 1-based line of the problem, if known.
    pub line: Option<usize>,
    /// 1-based column of the problem, if known.
    pub column: Option<usize>,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}", path.display())?;
            if let (Some(line), Some(column)) = (self.line, self.column) {
                write!(f, ":{line}:{column}")?;
            }
            write!(f, ": ")?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Every error the framework reports.
#[derive(Debug, Error)]
pub enum TptError {
    /// Invalid command-line usage or arguments.
    #[error("{0}")]
    Usage(String),
    /// A configuration file or value is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// A network request failed.
    #[error("could not reach {url}: {reason}")]
    Network {
        /// The URL that was requested.
        url: String,
        /// The failure category.
        kind: NetworkKind,
        /// Details from the underlying error.
        reason: String,
    },
    /// A network request timed out.
    #[error("request to {url} timed out after {timeout_secs}s")]
    Timeout {
        /// The URL that was requested.
        url: String,
        /// The timeout that was exceeded.
        timeout_secs: u64,
    },
    /// A local I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The user aborted the operation.
    #[error("aborted by user")]
    Aborted,
    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl TptError {
    /// A stable, machine-readable code, used in the JSON envelope.
    pub fn code(&self) -> &'static str {
        match self {
            TptError::Usage(_) => "usage",
            TptError::Config(_) => "config",
            TptError::Network { kind, .. } => match kind {
                NetworkKind::Dns => "dns",
                NetworkKind::Tls => "tls",
                NetworkKind::Connect | NetworkKind::Other => "connection",
            },
            TptError::Timeout { .. } => "timeout",
            TptError::Io(_) => "io",
            TptError::Aborted => "aborted",
            TptError::Internal(_) => "internal",
        }
    }

    /// The process exit code for this error (see [`EXIT_CODES`]).
    pub fn exit_code(&self) -> u8 {
        match self {
            TptError::Usage(_) => EXIT_USAGE,
            TptError::Config(_) => EXIT_CONFIG,
            TptError::Network { kind, .. } => match kind {
                NetworkKind::Dns => EXIT_DNS,
                NetworkKind::Tls => EXIT_TLS,
                NetworkKind::Connect | NetworkKind::Other => EXIT_CONNECTION,
            },
            TptError::Timeout { .. } => EXIT_TIMEOUT,
            TptError::Io(_) => EXIT_IO,
            TptError::Aborted => EXIT_ABORTED,
            TptError::Internal(_) => EXIT_INTERNAL,
        }
    }

    /// An actionable next step, if there is a useful one.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            TptError::Usage(_) => Some("Run the command with --help to see valid usage."),
            TptError::Config(e) if e.path.is_some() => {
                Some("Fix the syntax at the location above, or remove the file to use defaults.")
            }
            TptError::Config(_) => Some("Check the TPT_* environment variables and config files."),
            TptError::Network { kind: NetworkKind::Dns, .. } => {
                Some("Check the host name for typos, and your DNS or proxy settings.")
            }
            TptError::Network { kind: NetworkKind::Tls, .. } => Some(
                "Check the server certificate. For a corporate or self-signed CA, install it in the OS trust store.",
            ),
            TptError::Network { .. } => {
                Some("Check that the server is running and the port is reachable from this machine.")
            }
            TptError::Timeout { .. } => {
                Some("Increase the timeout with --timeout <SECS> or timeout_secs in your config.")
            }
            TptError::Internal(_) => Some("This is a bug. Please report it with the command you ran."),
            TptError::Io(_) | TptError::Aborted => None,
        }
    }
}

/// Joins an error and its sources into one line, for example `a: b: c`.
pub fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(next) = source {
        parts.push(next.to_string());
        source = next.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_table_is_unique_and_matches_variants() {
        let mut codes: Vec<u8> = EXIT_CODES.iter().map(|e| e.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), EXIT_CODES.len(), "exit codes must be unique");

        let samples = [
            (TptError::Usage("x".into()), EXIT_USAGE),
            (
                TptError::Config(ConfigError { path: None, line: None, column: None, message: "x".into() }),
                EXIT_CONFIG,
            ),
            (TptError::Timeout { url: "u".into(), timeout_secs: 1 }, EXIT_TIMEOUT),
            (TptError::Aborted, EXIT_ABORTED),
            (TptError::Internal("x".into()), EXIT_INTERNAL),
            (TptError::Io(std::io::Error::other("x")), EXIT_IO),
        ];
        for (err, code) in samples {
            assert_eq!(err.exit_code(), code, "{err}");
            assert!(EXIT_CODES.iter().any(|e| e.code == code));
        }
    }

    #[test]
    fn network_kinds_map_to_distinct_exit_codes() {
        let make = |kind| TptError::Network { url: "u".into(), kind, reason: "r".into() };
        assert_eq!(make(NetworkKind::Dns).exit_code(), EXIT_DNS);
        assert_eq!(make(NetworkKind::Tls).exit_code(), EXIT_TLS);
        assert_eq!(make(NetworkKind::Connect).exit_code(), EXIT_CONNECTION);
        assert_eq!(make(NetworkKind::Other).exit_code(), EXIT_CONNECTION);
        assert_eq!(make(NetworkKind::Dns).code(), "dns");
    }

    #[test]
    fn config_error_display_includes_location() {
        let err = ConfigError {
            path: Some(PathBuf::from("a.toml")),
            line: Some(3),
            column: Some(7),
            message: "expected a value".into(),
        };
        assert_eq!(err.to_string(), "a.toml:3:7: expected a value");
    }
}
