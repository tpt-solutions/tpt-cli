//! `tpt-cli`: a small, permissively licensed framework for building
//! human- and agent-friendly command-line tools.
//!
//! The crate is both a library (for other TPT projects) and the `tpt` binary.
//!
//! * [`config`]: layered configuration (defaults < global file < local file < env < CLI).
//! * [`errors`]: typed errors with stable codes, exit codes and hints.
//! * [`output`]: human or JSON output, with a stable JSON envelope.
//! * [`logger`]: a small stderr logger with no external dependencies.
//! * [`prompts`]: confirmation prompts (feature `prompts`).
//! * [`net`]: HTTP helpers (feature `net`).
//! * [`schema`]: a JSON description of any clap command tree.
//!
//! Building with `default-features = false` leaves out the networking and
//! prompt dependencies, and with them `ring`.
//!
//! # Example
//!
//! Resolve configuration from defaults and environment variables. The global
//! and local files are disabled here so the example does not depend on the
//! machine it runs on.
//!
//! ```
//! use tpt::prelude::*;
//!
//! let config: TptConfig = ConfigLoader::new("acme")
//!     .global_file(None)
//!     .local_file(None)
//!     .env_vars([("ACME_TIMEOUT_SECS", "30")])
//!     .load()
//!     .expect("defaults always load");
//!
//! assert_eq!(config.timeout_secs, 30);
//! assert_eq!(config.output, OutputMode::Human);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod config;
pub mod errors;
pub mod logger;
#[cfg(feature = "net")]
pub mod net;
pub mod output;
#[cfg(feature = "prompts")]
pub mod prompts;
pub mod schema;

/// Commonly used items, for `use tpt::prelude::*;`.
pub mod prelude {
    pub use crate::config::{CliOverrides, ConfigLoader, OutputMode, TptConfig};
    pub use crate::errors::TptError;
    pub use crate::logger::LogLevel;
    pub use crate::output::Output;
}
