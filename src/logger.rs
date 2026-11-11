//! A hand-rolled stderr logger. No logging crate is used.
//!
//! The level and colour settings are process-global. The binary sets them
//! once after configuration is resolved.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use clap::ValueEnum;
use console::Style;
use serde::{Deserialize, Serialize};

/// Log verbosity, from least to most verbose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Errors and warnings.
    Warn,
    /// Normal progress messages.
    Info,
    /// Developer detail.
    Debug,
    /// Everything.
    Trace,
}

impl LogLevel {
    /// The lowercase name used in config files and on the command line.
    ///
    /// ```
    /// use tpt::logger::LogLevel;
    ///
    /// assert_eq!(LogLevel::Debug.as_str(), "debug");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }

    fn from_u8(value: u8) -> LogLevel {
        match value {
            0 => LogLevel::Error,
            1 => LogLevel::Warn,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            _ => LogLevel::Trace,
        }
    }
}

static LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Warn as u8);
static COLOR: AtomicBool = AtomicBool::new(false);

/// Sets the maximum level that will be printed.
///
/// ```
/// use tpt::logger::{self, LogLevel};
///
/// logger::set_level(LogLevel::Debug);
/// assert_eq!(logger::level(), LogLevel::Debug);
/// assert!(logger::enabled(LogLevel::Info));
/// assert!(!logger::enabled(LogLevel::Trace));
/// ```
pub fn set_level(level: LogLevel) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Returns the current maximum level.
///
/// The default is [`LogLevel::Warn`].
///
/// ```
/// use tpt::logger::{self, LogLevel};
///
/// assert_eq!(logger::level(), LogLevel::Warn);
/// ```
pub fn level() -> LogLevel {
    LogLevel::from_u8(LEVEL.load(Ordering::Relaxed))
}

/// Returns `true` if messages at `level` would be printed.
///
/// ```
/// use tpt::logger::{self, LogLevel};
///
/// logger::set_level(LogLevel::Error);
/// assert!(logger::enabled(LogLevel::Error));
/// assert!(!logger::enabled(LogLevel::Warn));
/// ```
pub fn enabled(level: LogLevel) -> bool {
    (level as u8) <= LEVEL.load(Ordering::Relaxed)
}

/// Enables or disables coloured log prefixes.
///
/// ```
/// use tpt::logger;
///
/// logger::set_color(false);
/// ```
pub fn set_color(enabled: bool) {
    COLOR.store(enabled, Ordering::Relaxed);
}

/// Writes `message` to stderr if `level` is enabled.
///
/// Messages above the current level are dropped without output.
///
/// ```
/// use tpt::logger::{self, LogLevel};
///
/// logger::set_level(LogLevel::Error);
/// // Below the threshold: nothing is written to stderr.
/// logger::log(LogLevel::Debug, "not shown");
/// ```
pub fn log(level: LogLevel, message: &str) {
    if !enabled(level) {
        return;
    }
    let (tag, style) = match level {
        LogLevel::Error => ("error", Style::new().red().bold()),
        LogLevel::Warn => ("warn", Style::new().yellow()),
        LogLevel::Info => ("info", Style::new().cyan()),
        LogLevel::Debug => ("debug", Style::new().dim()),
        LogLevel::Trace => ("trace", Style::new().dim()),
    };
    let style = style.force_styling(COLOR.load(Ordering::Relaxed));
    eprintln!("{}: {message}", style.apply_to(tag));
}

/// Logs at [`LogLevel::Error`].
pub fn error(message: &str) {
    log(LogLevel::Error, message);
}

/// Logs at [`LogLevel::Warn`].
pub fn warn(message: &str) {
    log(LogLevel::Warn, message);
}

/// Logs at [`LogLevel::Info`].
pub fn info(message: &str) {
    log(LogLevel::Info, message);
}

/// Logs at [`LogLevel::Debug`].
pub fn debug(message: &str) {
    log(LogLevel::Debug, message);
}

/// Logs at [`LogLevel::Trace`].
pub fn trace(message: &str) {
    log(LogLevel::Trace, message);
}
