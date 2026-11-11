# tpt-cli TODO

Publisher: TPT Solutions. Licence: MIT OR Apache-2.0 (dual). Binary: `tpt`. Crate: `tpt-cli` (lib + bin).

Decisions locked in:
- Implementation notes (from building Phases 0-7):
  - MSRV is 1.88, not 1.85, because the code uses let-chains. Tested on 1.88 and stable locally.
  - `dirs` was dropped for `std::env::home_dir`. `dirs` 6 and 7 both pull in `option-ext` (MPL-2.0), which the policy forbids.
  - The global config path is `~/.config/tpt/config.toml` on every OS, so Windows and macOS match Linux.
  - `reqwest` is 0.12, not 0.13. The 0.13 `rustls` feature uses `aws-lc-rs`, which is C-based. 0.12 uses `ring` with `rustls-native-certs`.
  - Two `deny.toml` exceptions were added: `ryu` (take its Apache-2.0 OR option) and `sync_wrapper` (Apache-2.0 only, needed by reqwest). The `sync_wrapper` one needs the maintainer's sign-off.
  - `ping --timeout` sets the timeout. The report includes `timeout_secs`.
- Dependency licence policy: allow MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense. Apache-2.0 is accepted only as an "OR" alternative in a dual licence, never Apache-only or `AND`. No copyleft/MPL/CDLA.
- Networking sits behind a `net` Cargo feature. The core lib stays fully MIT-clean. The `net` feature carries one documented exception: `ring` (Apache-2.0 AND ISC).
- Use `rustls-native-certs` (OS cert store) instead of `webpki-roots`
- Long term: replace `ring` (see the last phase)
- Single crate, lib + `tpt` binary
- `tpt ping <url>` does an HTTP GET and reports status and latency
- Linux, macOS and Windows supported
- Hand-rolled stderr logger (no logging crate)
- GitHub + GitHub Actions
- `tpt schema` emits a custom JSON command tree

---

## Phase 0: Project setup and licensing
- [x] Create `Cargo.toml` (name `tpt-cli`, edition 2024, `rust-version`, `license = "MIT OR Apache-2.0"`, authors/repository metadata for TPT Solutions)
- [x] Add `LICENSE-MIT` (copyright TPT Solutions)
- [x] Add `LICENSE-APACHE` (copyright TPT Solutions)
- [x] Add `.gitignore`, `rustfmt.toml`, `rust-toolchain.toml` (stable)
- [x] `git init` and make the first commit
- [x] Write the licence section in `README.md` (standard "MIT or Apache-2.0, at your option" + contribution clause)
- [x] Add `deny.toml` for cargo-deny: allowlist MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense; Apache-2.0 only inside `OR` expressions; deny copyleft, MPL and CDLA
- [x] Run `cargo deny check licenses` against the full dependency tree, with and without the `net` feature
- [x] Verify each crate's licence from its actual `Cargo.toml` (do not rely on memory), especially ring, rustls, rustls-native-certs, rustls-webpki
- [x] Audit every direct and transitive dependency for Apache-only or `AND Apache` licences
- [x] Add a single documented `ring` exception to `deny.toml`, scoped to the `net` feature only
- [x] Replace or exempt any other dependency that fails the audit (document each exception in `deny.toml`)
- [x] Set the release profile: `opt-level = "z"` or `3` (benchmark), `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`
- [x] Record baseline metrics: clean build time, incremental build time, binary size

## Phase 1: Skeleton (Spec Milestone 1)
- [x] Add dependencies with minimal features:
  - [x] `clap` (`default-features = false`; `derive`, `env`, `help`, `std`)
  - [x] `serde`, `toml`, `dirs`
  - [x] `tokio` (`rt` only, no multi-thread)
  - [x] `reqwest` (`default-features = false`, rustls only)
  - [x] `thiserror`, `anyhow`, `serde_json`
  - [x] `dialoguer`, `console`
- [x] Gate heavier features (reqwest, tokio, dialoguer) behind Cargo features
- [x] `net` feature: reqwest + tokio + rustls + `rustls-native-certs` (no `webpki-roots`); the only feature that pulls in ring
- [x] Verify `cargo tree --no-default-features` contains no ring and no Apache-only crates
- [x] The `tpt` binary enables `net`; library consumers can opt out with `default-features = false`
- [x] Create the `src/` layout: `main.rs`, `lib.rs`, `config.rs`, `errors.rs`, `output.rs`, `prompts.rs`, plus `examples/basic_cmd.rs`
- [x] `errors.rs`: typed `TptError` enum (thiserror) covering config, network, IO, usage and user-abort errors
- [x] `errors.rs`: map each error variant to a specific exit code
- [x] `errors.rs`: document the exit-code table
- [x] `errors.rs`: actionable message formatting with a hint line
- [x] `errors.rs`: `anyhow` used only at the binary boundary
- [x] `config.rs`: define the `Config` struct (serde) with defaults
- [x] `config.rs`: locate the global config at `~/.config/tpt/config.toml` using `dirs` (check the Windows and macOS equivalents behave sensibly)
- [x] `config.rs`: load `.tpt.toml` from the current directory
- [x] `config.rs`: read env vars with the `TPT_` prefix (e.g. `TPT_LOG_LEVEL`)
- [x] `config.rs`: merge in priority order: CLI args > env > local > global > defaults
- [x] `config.rs`: clean error for malformed TOML (file path, line and column)
- [x] `config.rs`: missing files are not an error
- [x] Unit tests for each merge-priority combination
- [x] Unit tests for missing, empty and malformed config files
- [x] Milestone check: CLI, env and TOML merge proven by tests

## Phase 2: UX and output (Spec Milestone 2)
- [x] `output.rs`: global `--output <human|json>` flag (also settable via env and config)
- [x] `output.rs`: `Output` abstraction (`success`, `data`, `info`, `warn`, `error`)
- [x] `output.rs`: JSON mode writes strict JSON to stdout only; all human logs go to stderr
- [x] `output.rs`: JSON mode disables colours, spinners and prompts
- [x] `output.rs`: define a stable JSON envelope (e.g. `{ "ok": true, "data": ... }` / `{ "ok": false, "error": { "code", "message", "hint" } }`)
- [x] `output.rs`: respect `NO_COLOR` and TTY detection for colours
- [x] Hand-rolled stderr logger: levels error/warn/info/debug/trace
- [x] Logger honours `TPT_LOG_LEVEL`, config and a `--verbose`/`--quiet` flag
- [x] `prompts.rs`: standardized "Confirm Action" prompt (dialoguer wrapper)
- [x] `prompts.rs`: `--yes` flag to skip prompts
- [x] `prompts.rs`: non-TTY and JSON mode never block; they fail with a clear error or use the default
- [x] `prompts.rs`: consistent theme via `console`
- [x] `main.rs`: clap root command with global flags
- [x] `main.rs`: single-threaded tokio runtime bootstrap
- [x] `main.rs`: top-level error handler that prints through `output.rs` and sets the exit code
- [x] `main.rs`: `tpt ping <url>` command (HTTP GET, status code, latency, timeout flag)
- [x] `ping`: human output and JSON output
- [x] `ping`: network failures, DNS and TLS errors map to typed errors with hints
- [x] Integration tests for `ping` (local test server) in human and JSON modes
- [x] Integration tests that assert exit codes and that stdout is valid JSON in JSON mode
- [x] Milestone check: `tpt ping` works end to end

## Phase 3: Agentic polish (Spec Milestone 3)
- [x] Define the JSON schema format for the command tree (name, about, args, flags, types, defaults, env vars, required, subcommands)
- [x] Implement `tpt schema` by walking clap's `Command` structure
- [x] `schema` always outputs JSON (no human formatting needed)
- [x] Include global flags, exit-code table and output envelope in the schema output
- [x] Support `tpt schema <command>` to describe a single command
- [x] Snapshot test for the schema output
- [x] Test that every registered command appears in the schema (no drift)
- [x] Document the schema format in the README
- [x] Milestone check: an agent can discover all commands from `tpt schema` alone

## Phase 4: Library API for other TPT repos
- [x] Design the public API in `lib.rs` (re-exports, prelude, feature flags)
- [x] Make config loading reusable by other CLIs (custom config struct, custom app name and prefix)
- [x] Make the output, error and prompt helpers usable without the `tpt` binary's commands
- [x] Make `schema` generation reusable for any clap `Command`
- [x] `examples/basic_cmd.rs`: minimal consumer CLI built on the lib
- [ ] Verify the example builds and runs in CI
- [ ] Rustdoc on every public item, with doctests
- [ ] Review the public surface for semver stability
- [x] Decide feature-flag defaults so consumers pay only for what they use

## Phase 5: Quality, performance and compliance
- [x] `cargo fmt --check` clean
- [x] `cargo clippy -- -D warnings` clean
- [ ] Full test suite passes on Linux, macOS and Windows
- [ ] Confirm the dependency tree has no OpenSSL or other C dependencies (`cargo tree`)
- [x] Re-run `cargo deny check` (licenses, bans, advisories) on the final tree
- [ ] Generate a third-party licence list for the repo
- [ ] Compare final binary size and build time against the Phase 0 baseline
- [ ] Remove unused dependencies and features (`cargo machete` or `cargo udeps`)
- [ ] Check there are no panics on bad input (fuzz-style manual tests: bad flags, bad TOML, unreachable hosts)
- [ ] Verify Windows terminal behaviour (colours, prompts, paths, config location)
- [x] Set the MSRV and test against it in CI

## Phase 6: CI/CD (GitHub Actions)
- [ ] Create the GitHub repo under TPT Solutions
- [x] Workflow: fmt + clippy
- [x] Workflow: test matrix (Linux, macOS, Windows; stable + MSRV)
- [x] Workflow: `cargo deny check`
- [x] Workflow: build with the release profile and report binary size
- [x] Add `dependabot.yml` or equivalent for dependency updates
- [ ] Add a CI badge to the README
- [ ] Protect the main branch with required checks

## Phase 7: Documentation and community files
- [x] README: overview, install, quick start, `ping` and `schema` examples
- [x] README: configuration resolution order and example `config.toml` / `.tpt.toml`
- [x] README: agent/machine mode (`--output json`) and the JSON envelope
- [x] README: exit-code table
- [x] README: library usage guide for other TPT repos
- [x] README: licensing and the dependency-licence policy
- [x] README: state clearly that the `net` feature pulls in ring (Apache-2.0 AND ISC) and the core lib does not
- [x] `CHANGELOG.md`
- [x] `CONTRIBUTING.md` (including the dual-licence contribution clause)
- [x] `SECURITY.md`
- [ ] Optional: `CODE_OF_CONDUCT.md`, issue and PR templates

## Phase 8: Release (v0.1.0)
- [ ] Final review against `spec.txt` (every requirement ticked off)
- [ ] Bump the version to 0.1.0 and update the changelog
- [ ] Tag `v0.1.0`
- [ ] Draft the GitHub release notes
- [ ] Smoke test a fresh install and build on Linux, macOS and Windows
- [ ] Post-release: open the backlog for the next version

## Phase 9: Long term, ring replacement (separate project/crate)
- [ ] List the exact workarounds hit so far and which ring features tpt actually needs (likely only a rustls CryptoProvider: TLS 1.3 AEADs, X25519, ECDSA/Ed25519/RSA verify, HKDF, HMAC, SHA-2, RNG)
- [ ] Decide the approach: wrap the RustCrypto crates (MIT/Apache dual) as a `rustls` `CryptoProvider`, do not write primitives from scratch
- [ ] Check the licences of each RustCrypto crate needed (must pass the same allowlist)
- [ ] Survey existing providers (e.g. `rustls-rustcrypto`) for maturity, licence and gaps; reuse or contribute before building
- [ ] Create the new crate (dual MIT/Apache-2.0, TPT Solutions), with a `rustls` provider as its first deliverable
- [ ] Test vectors: Wycheproof, RFC vectors and rustls's own interop tests
- [ ] Constant-time review of anything touching secret data
- [ ] Plan an independent security review before recommending it for production use
- [ ] Benchmark against ring (handshake time, throughput, binary size)
- [ ] Swap into `tpt-cli`, drop the `ring` exception from `deny.toml` and update the README

## Backlog / not in scope (decide later)
- [ ] Publish to crates.io (check the `tpt-cli` name is available)
- [ ] Prebuilt binaries on GitHub Releases (cross-compiled, checksums)
- [ ] Plugin / subcommand extension system for other TPT repos
- [ ] Shell completions and man pages
- [ ] Docs website
