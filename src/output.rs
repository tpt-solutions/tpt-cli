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

use console::Style;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::OutputMode;
use crate::errors::TptError;
use crate::logger::{self, LogLevel};

/// Builds the JSON envelope for a successful result.
pub fn success_envelope<T: Serialize + ?Sized>(data: &T) -> Result<Value, serde_json::Error> {
    Ok(json!({ "ok": true, "data": serde_json::to_value(data)? }))
}

/// Builds the JSON envelope for an error.
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
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// `true` in JSON mode.
    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    /// Whether this writer colours its own messages.
    pub fn colors_enabled(&self) -> bool {
        self.color
    }

    /// Prints a result. Human mode prints `human` to stdout. JSON mode prints the envelope.
    pub fn data<T: Serialize + ?Sized>(&self, human: &str, value: &T) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        match self.mode {
            OutputMode::Human => writeln!(stdout, "{human}"),
            OutputMode::Json => {
                let envelope = success_envelope(value).map_err(io::Error::other)?;
                writeln!(stdout, "{envelope}")
            }
        }
    }

    /// Prints a short success line to stderr in human mode. Nothing is printed in JSON mode.
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
