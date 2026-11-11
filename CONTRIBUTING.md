# Contributing to tpt-cli

Thank you for helping. These are the rules for changes to this repository.

## Setup

You need a recent stable Rust toolchain (1.88 or newer, the MSRV). `rust-toolchain.toml`
selects stable with `rustfmt` and `clippy`.

```sh
cargo build
cargo test --all-targets
cargo build --no-default-features   # the core library must build without networking
```

## Before you open a pull request

All of these must pass. CI runs the same checks.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --no-default-features --lib
cargo deny check
```

If you change the output of `tpt schema` on purpose, regenerate the snapshot and commit it:

```sh
UPDATE_SNAPSHOTS=1 cargo test --test schema
```

## Dependencies

Dependencies must follow the licence policy in `README.md` and `deny.toml`:

- Allowed: MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense.
- Apache-2.0 only as one option of an `OR` expression.
- No copyleft, MPL or CDLA.
- Put heavy dependencies behind a Cargo feature. Keep the default core small.
- Adding an exception to `deny.toml` needs a reason in the file and the maintainer's approval.

Run `cargo deny check licenses` after any dependency change.

## Coding guidelines

- Every public item has rustdoc. Doctests should compile and run.
- Library code returns `TptError`. `anyhow` is used only in `src/main.rs`.
- Never panic on user input. Bad flags, bad TOML and unreachable hosts must produce a typed error and an exit code.
- In JSON mode, nothing but the envelope goes to stdout. Logs go to stderr.
- Keep human-readable output and JSON output in step. A new command needs both.
- Add a test for every new error variant, exit code or config key.

## Licence of contributions

Unless you say otherwise, any contribution you intentionally submit for inclusion in
`tpt-cli` is dual-licensed as in the README, under MIT OR Apache-2.0, without any additional
terms or conditions. This follows the Apache-2.0 licence's section 5 (Submission of
Contributions).

## Commits and pull requests

- Keep each commit focused. Write the subject in the imperative mood, for example "Add schema snapshot test".
- Describe what changed and why in the pull request. Link the matching item in `todo.md` where there is one.
