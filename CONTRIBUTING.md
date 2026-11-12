# Contributing to tpt-cli

Contributions are accepted as issues only. Please open an issue to report a bug, ask a question
or suggest a change. Do not open pull requests; they will not be reviewed or merged.

## Reporting an issue

Include:

- The `tpt` version (`tptcli --version`) and how you installed it.
- Your operating system and terminal.
- The exact command you ran, and its output. For machine use, add `--output json`.
- What you expected to happen and what happened instead.

To reproduce a problem from source, you need a recent stable Rust toolchain (1.88 or newer, the MSRV):

```sh
cargo build
cargo test --all-targets
```

Security issues are not for the public tracker. Report them privately to the TPT Solutions maintainers.

## Licence of contributions

Unless you say otherwise, any contribution you intentionally submit for inclusion in
`tpt-cli` is dual-licensed as in the README, under MIT OR Apache-2.0, without any additional
terms or conditions. This follows the Apache-2.0 licence's section 5 (Submission of
Contributions).
