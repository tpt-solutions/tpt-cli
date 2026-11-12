//! Standard confirmation prompt.
//!
//! The prompt never blocks in JSON mode or without a terminal. It fails with a
//! clear error instead, and `--yes` skips it altogether.

use std::io::{self, IsTerminal};

use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Error as DialoguerError};

use crate::errors::TptError;
use crate::output::Output;

/// Asks the user to confirm an action.
///
/// * `assume_yes` (the `--yes` flag) returns `Ok(true)` without prompting.
/// * JSON mode and non-interactive sessions return a usage error. They never wait for input.
/// * Ctrl-C returns [`TptError::Aborted`].
///
/// Returns `Ok(false)` if the user answers no.
///
/// With `assume_yes` set, the prompt is skipped and the answer is `Ok(true)`.
///
/// ```
/// use tpt::config::OutputMode;
/// use tpt::output::Output;
/// use tpt::prompts::confirm_action;
///
/// let output = Output::new(OutputMode::Json);
/// assert!(confirm_action(&output, "Delete?", true).unwrap());
/// ```
pub fn confirm_action(output: &Output, prompt: &str, assume_yes: bool) -> Result<bool, TptError> {
    if assume_yes {
        return Ok(true);
    }
    if output.is_json() {
        return Err(TptError::Usage(format!(
            "{prompt} Confirmation is needed, but --output json disables prompts. Re-run with --yes."
        )));
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(TptError::Usage(format!(
            "{prompt} Confirmation is needed, but no terminal is attached. Re-run with --yes."
        )));
    }
    match Confirm::with_theme(&theme()).with_prompt(prompt).default(false).interact() {
        Ok(answer) => Ok(answer),
        Err(DialoguerError::IO(err)) if err.kind() == io::ErrorKind::Interrupted => Err(TptError::Aborted),
        Err(DialoguerError::IO(err)) => Err(TptError::Io(err)),
    }
}

/// The shared prompt theme, so every prompt looks the same.
pub fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputMode;

    #[test]
    fn assume_yes_skips_the_prompt() {
        let output = Output::new(OutputMode::Human);
        assert!(confirm_action(&output, "Delete?", true).unwrap());
    }

    #[test]
    fn json_mode_never_prompts() {
        let output = Output::new(OutputMode::Json);
        let err = confirm_action(&output, "Delete?", false).unwrap_err();
        assert!(matches!(err, TptError::Usage(ref m) if m.contains("--yes")), "{err:?}");
    }

    #[test]
    fn no_terminal_never_prompts() {
        // A developer running the tests in a terminal would otherwise get a live prompt.
        if io::stdin().is_terminal() && io::stderr().is_terminal() {
            return;
        }
        let output = Output::new(OutputMode::Human);
        let err = confirm_action(&output, "Delete?", false).unwrap_err();
        assert!(matches!(err, TptError::Usage(ref m) if m.contains("no terminal")), "{err:?}");
    }
}
