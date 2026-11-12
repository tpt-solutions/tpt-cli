//! Human and JSON output.
//!
//! In JSON mode, results go to stdout as a strict envelope:
//!
//! ```json
//! {"ok": true, "data": { ... }}
//! {"ok": false, "error": {"code": "...", "message": "...", "hint": "..." | null}}
//! ```
//!
//! Human logs (info, warn, ...) always go to stderr, so stdout stays parseable.

use std::io::{self, IsTerminal, Write};
use std::sync::{Mutex, OnceLock};

use console::Style;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::OutputMode;
use crate::errors::TptError;
use crate::logger::{self, LogLevel};

/// Builds the JSON envelope for a successful result.
///
/// ```
/// use tpt::output::success_envelope;
///
/// let envelope = success_envelope(&serde_json::json!({ "status": 200 })).unwrap();
/// assert_eq!(envelope["ok"], true);
/// assert_eq!(envelope["data"]["status"], 200);
/// ```
pub fn success_envelope<T: Serialize + ?Sized>(data: &T) -> Result<Value, serde_json::Error> {
    Ok(json!({ "ok": true, "data": serde_json::to_value(data)? }))
}

/// Builds the JSON envelope for an error.
///
/// ```
/// use tpt::errors::TptError;
/// use tpt::output::error_envelope;
///
/// let envelope = error_envelope(&TptError::Usage("missing URL".into()));
/// assert_eq!(envelope["ok"], false);
/// assert_eq!(envelope["error"]["code"], "usage");
/// ```
pub fn error_envelope(err: &TptError) -> Value {
    json!({
        "ok": false,
        "error": {
            "code": err.code(),
            "message": err.to_string(),
            "hint": err.hint(),
        }
    })
}

static SELECT: OnceLock<String> = OnceLock::new();
static PROGRESS_SHOWN: Mutex<bool> = Mutex::new(false);

/// Sets the field path that JSON output is narrowed to, such as `data.status`.
/// Only the first call has an effect, like the logger level.
pub fn set_select(path: Option<&str>) {
    if let Some(path) = path {
        let _ = SELECT.set(path.to_owned());
    }
}

/// Returns the part of `value` at a dot-separated `path`. Array items are addressed by index.
///
/// ```
/// use serde_json::json;
/// use tpt::output::select_path;
///
/// let envelope = json!({ "ok": true, "data": { "status": 200, "items": ["a", "b"] } });
/// assert_eq!(select_path(&envelope, "data.status").unwrap(), json!(200));
/// assert_eq!(select_path(&envelope, "data.items.1").unwrap(), json!("b"));
/// assert!(select_path(&envelope, "data.missing").is_err());
/// ```
pub fn select_path(value: &Value, path: &str) -> Result<Value, TptError> {
    let mut current = value;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let next = match current {
            Value::Object(map) => map.get(part),
            Value::Array(items) => part.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        };
        current =
            next.ok_or_else(|| TptError::Usage(format!("--select: no field `{path}` in the output")))?;
    }
    Ok(current.clone())
}

/// Writes results and messages in the chosen [`OutputMode`].
#[derive(Debug, Clone, Copy)]
pub struct Output {
    mode: OutputMode,
    color: bool,
}

impl Output {
    /// Creates an output writer. Colours are used only in human mode, on a
    /// terminal, and when `NO_COLOR` is not set.
    pub fn new(mode: OutputMode) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let color = mode == OutputMode::Human && io::stderr().is_terminal() && !no_color;
        Self { mode, color }
    }

    /// The output mode in use.
    ///
    /// ```
    /// use tpt::config::OutputMode;
    /// use tpt::output::Output;
    ///
    /// assert_eq!(Output::new(OutputMode::Json).mode(), OutputMode::Json);
    /// ```
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// `true` in JSON mode.
    ///
    /// ```
    /// use tpt::config::OutputMode;
    /// use tpt::output::Output;
    ///
    /// assert!(!Output::new(OutputMode::Human).is_json());
    /// assert!(Output::new(OutputMode::Json).is_json());
    /// ```
    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    /// Whether this writer colours its own messages.
    ///
    /// Colours are never used in JSON mode.
    ///
    /// ```
    /// use tpt::config::OutputMode;
    /// use tpt::output::Output;
    ///
    /// assert!(!Output::new(OutputMode::Json).colors_enabled());
    /// ```
    pub fn colors_enabled(&self) -> bool {
        self.color
    }

    /// Prints a result. Human mode prints `human` to stdout. JSON mode prints the envelope,
    /// or the part picked by `--select`.
    pub fn data<T: Serialize + ?Sized>(&self, human: &str, value: &T) -> Result<(), TptError> {
        self.clear_progress();
        let mut stdout = io::stdout().lock();
        match self.mode {
            OutputMode::Human => writeln!(stdout, "{human}")?,
            OutputMode::Json => {
                let envelope = success_envelope(value).map_err(|e| TptError::Internal(e.to_string()))?;
                let shown = match SELECT.get() {
                    Some(path) => select_path(&envelope, path)?,
                    None => envelope,
                };
                writeln!(stdout, "{shown}")?;
            }
        }
        Ok(())
    }

    /// Shows a one-line progress message on stderr while a slow step runs. Only on a human
    /// terminal, so JSON output and pipes never see it. Call [`Output::clear_progress`] when done.
    ///
    /// This is a plain status line, not an animated spinner.
    pub fn progress(&self, message: &str) {
        if self.is_json() || !io::stderr().is_terminal() || logger::level() == LogLevel::Error {
            return;
        }
        if let Ok(mut shown) = PROGRESS_SHOWN.lock() {
            *shown = true;
        }
        eprint!("{message}...");
        let _ = io::stderr().flush();
    }

    /// Removes the line written by [`Output::progress`], if there is one.
    pub fn clear_progress(&self) {
        let Ok(mut shown) = PROGRESS_SHOWN.lock() else { return };
        if *shown {
            console::Term::stderr().clear_line().ok();
            *shown = false;
        }
    }

    /// Prints a short success line to stderr in human mode. Nothing is printed in JSON mode.
    ///
    /// ```
    /// use tpt::config::OutputMode;
    /// use tpt::output::Output;
    ///
    /// // JSON mode prints nothing for this call, so the doctest output stays empty.
    /// Output::new(OutputMode::Json).success("saved");
    /// ```
    pub fn success(&self, message: &str) {
        if self.is_json() {
            return;
        }
        let mark = Style::new().green().bold().force_styling(self.color).apply_to("ok");
        eprintln!("{mark}: {message}");
    }

    /// Logs at info level (stderr).
    pub fn info(&self, message: &str) {
        logger::log(LogLevel::Info, message);
    }

    /// Logs at warn level (stderr).
    pub fn warn(&self, message: &str) {
        logger::log(LogLevel::Warn, message);
    }

    /// Reports an error. Human mode prints to stderr with the hint. JSON mode prints the envelope to stdout.
    pub fn error(&self, err: &TptError) {
        self.clear_progress();
        match self.mode {
            OutputMode::Human => {
                let label = Style::new().red().bold().force_styling(self.color).apply_to("error");
                eprintln!("{label}: {err}");
                if let Some(hint) = err.hint() {
                    let hint_label = Style::new().dim().force_styling(self.color).apply_to("hint");
                    eprintln!("  {hint_label}: {hint}");
                }
            }
            OutputMode::Json => {
                println!("{}", error_envelope(err));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_envelope_has_ok_and_data() {
        let value = success_envelope(&json!({"status": 200})).unwrap();
        assert_eq!(value, json!({"ok": true, "data": {"status": 200}}));
    }

    #[test]
    fn error_envelope_has_code_message_and_hint() {
        let value = error_envelope(&TptError::Aborted);
        assert_eq!(value["ok"], json!(false));
        assert_eq!(value["error"]["code"], json!("aborted"));
        assert_eq!(value["error"]["hint"], Value::Null);
    }

    #[test]
    fn error_envelope_includes_hint_when_available() {
        let value = error_envelope(&TptError::Usage("bad flag".into()));
        assert_eq!(value["error"]["code"], json!("usage"));
        assert!(value["error"]["hint"].as_str().is_some_and(|h| !h.is_empty()));
    }
}
