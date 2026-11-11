# tpt-cli TODO

Publisher: TPT Solutions. Licence: MIT OR Apache-2.0 (dual). Binary: `tpt`. Crate: `tpt-cli` (lib + bin).

Decisions locked in:
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
- [ ] Create `Cargo.toml` (name `tpt-cli`, edition 2024, `rust-version`, `license = "MIT OR Apache-2.0"`, authors/repository metadata for TPT Solutions)
- [ ] Add `LICENSE-MIT` (copyright TPT Solutions)
- [ ] Add `LICENSE-APACHE` (copyright TPT Solutions)
- [ ] Add `.gitignore`, `rustfmt.toml`, `rust-toolchain.toml` (stable)
- [ ] `git init` and make the first commit
- [ ] Write the licence section in `README.md` (standard "MIT or Apache-2.0, at your option" + contribution clause)
- [ ] Add `deny.toml` for cargo-deny: allowlist MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense; Apache-2.0 only inside `OR` expressions; deny copyleft, MPL and CDLA
- [ ] Run `cargo deny check licenses` against the full dependency tree, with and without the `net` feature
- [ ] Verify each crate's licence from its actual `Cargo.toml` (do not rely on memory), especially ring, rustls, rustls-native-certs, rustls-webpki
- [ ] Audit every direct and transitive dependency for Apache-only or `AND Apache` licences
- [ ] Add a single documented `ring` exception to `deny.toml`, scoped to the `net` feature only
- [ ] Replace or exempt any other dependency that fails the audit (document each exception in `deny.toml`)
- [ ] Set the release profile: `opt-level = "z"` or `3` (benchmark), `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`
- [ ] Record baseline metrics: clean build time, incremental build time, binary size

## Phase 1: Skeleton (Spec Milestone 1)
- [ ] Add dependencies with minimal features:
  - [ ] `clap` (`default-features = false`; `derive`, `env`, `help`, `std`)
  - [ ] `serde`, `toml`, `dirs`
  - [ ] `tokio` (`rt` only, no multi-thread)
  - [ ] `reqwest` (`default-features = false`, rustls only)
  - [ ] `thiserror`, `anyhow`, `serde_json`
  - [ ] `dialoguer`, `console`
- [ ] Gate heavier features (reqwest, tokio, dialoguer) behind Cargo features
- [ ] `net` feature: reqwest + tokio + rustls + `rustls-native-certs` (no `webpki-roots`); the only feature that pulls in ring
- [ ] Verify `cargo tree --no-default-features` contains no ring and no Apache-only crates
- [ ] The `tpt` binary enables `net`; library consumers can opt out with `default-features = false`
- [ ] Create the `src/` layout: `main.rs`, `lib.rs`, `config.rs`, `errors.rs`, `output.rs`, `prompts.rs`, plus `examples/basic_cmd.rs`
- [ ] `errors.rs`: typed `TptError` enum (thiserror) covering config, network, IO, usage and user-abort errors
- [ ] `errors.rs`: map each error variant to a specific exit code
- [ ] `errors.rs`: document the exit-code table
- [ ] `errors.rs`: actionable message formatting with a hint line
- [ ] `errors.rs`: `anyhow` used only at the binary boundary
- [ ] `config.rs`: define the `Config` struct (serde) with defaults
- [ ] `config.rs`: locate the global config at `~/.config/tpt/config.toml` using `dirs` (check the Windows and macOS equivalents behave sensibly)
- [ ] `config.rs`: load `.tpt.toml` from the current directory
- [ ] `config.rs`: read env vars with the `TPT_` prefix (e.g. `TPT_LOG_LEVEL`)
- [ ] `config.rs`: merge in priority order: CLI args > env > local > global > defaults
- [ ] `config.rs`: clean error for malformed TOML (file path, line and column)
- [ ] `config.rs`: missing files are not an error
- [ ] Unit tests for each merge-priority combination
- [ ] Unit tests for missing, empty and malformed config files
- [ ] Milestone check: CLI, env and TOML merge proven by tests

## Phase 2: UX and output (Spec Milestone 2)
- [ ] `output.rs`: global `--output <human|json>` flag (also settable via env and config)
- [ ] `output.rs`: `Output` abstraction (`success`, `data`, `info`, `warn`, `error`)
- [ ] `output.rs`: JSON mode writes strict JSON to stdout only; all human logs go to stderr
- [ ] `output.rs`: JSON mode disables colours, spinners and prompts
- [ ] `output.rs`: define a stable JSON envelope (e.g. `{ "ok": true, "data": ... }` / `{ "ok": false, "error": { "code", "message", "hint" } }`)
- [ ] `output.rs`: respect `NO_COLOR` and TTY detection for colours
- [ ] Hand-rolled stderr logger: levels error/warn/info/debug/trace
- [ ] Logger honours `TPT_LOG_LEVEL`, config and a `--verbose`/`--quiet` flag
- [ ] `prompts.rs`: standardized "Confirm Action" prompt (dialoguer wrapper)
- [ ] `prompts.rs`: `--yes` flag to skip prompts
- [ ] `prompts.rs`: non-TTY and JSON mode never block; they fail with a clear error or use the default
- [ ] `prompts.rs`: consistent theme via `console`
- [ ] `main.rs`: clap root command with global flags
- [ ] `main.rs`: single-threaded tokio runtime bootstrap
- [ ] `main.rs`: top-level error handler that prints through `output.rs` and sets the exit code
- [ ] `main.rs`: `tpt ping <url>` command (HTTP GET, status code, latency, timeout flag)
- [ ] `ping`: human output and JSON output
- [ ] `ping`: network failures, DNS and TLS errors map to typed errors with hints
- [ ] Integration tests for `ping` (local test server) in human and JSON modes
- [ ] Integration tests that assert exit codes and that stdout is valid JSON in JSON mode
- [ ] Milestone check: `tpt ping` works end to end

## Phase 3: Agentic polish (Spec Milestone 3)
- [ ] Define the JSON schema format for the command tree (name, about, args, flags, types, defaults, env vars, required, subcommands)
- [ ] Implement `tpt schema` by walking clap's `Command` structure
- [ ] `schema` always outputs JSON (no human formatting needed)
- [ ] Include global flags, exit-code table and output envelope in the schema output
- [ ] Support `tpt schema <command>` to describe a single command
- [ ] Snapshot test for the schema output
- [ ] Test that every registered command appears in the schema (no drift)
- [ ] Document the schema format in the README
- [ ] Milestone check: an agent can discover all commands from `tpt schema` alone

## Phase 4: Library API for other TPT repos
- [ ] Design the public API in `lib.rs` (re-exports, prelude, feature flags)
- [ ] Make config loading reusable by other CLIs (custom config struct, custom app name and prefix)
- [ ] Make the output, error and prompt helpers usable without the `tpt` binary's commands
- [ ] Make `schema` generation reusable for any clap `Command`
- [ ] `examples/basic_cmd.rs`: minimal consumer CLI built on the lib
- [ ] Verify the example builds and runs in CI
- [ ] Rustdoc on every public item, with doctests
- [ ] Review the public surface for semver stability
- [ ] Decide feature-flag defaults so consumers pay only for what they use

## Phase 5: Quality, performance and compliance
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Full test suite passes on Linux, macOS and Windows
- [ ] Confirm the dependency tree has no OpenSSL or other C dependencies (`cargo tree`)
- [ ] Re-run `cargo deny check` (licenses, bans, advisories) on the final tree
- [ ] Generate a third-party licence list for the repo
- [ ] Compare final binary size and build time against the Phase 0 baseline
- [ ] Remove unused dependencies and features (`cargo machete` or `cargo udeps`)
- [ ] Check there are no panics on bad input (fuzz-style manual tests: bad flags, bad TOML, unreachable hosts)
- [ ] Verify Windows terminal behaviour (colours, prompts, paths, config location)
- [ ] Set the MSRV and test against it in CI

## Phase 6: CI/CD (GitHub Actions)
- [ ] Create the GitHub repo under TPT Solutions
- [ ] Workflow: fmt + clippy
- [ ] Workflow: test matrix (Linux, macOS, Windows; stable + MSRV)
- [ ] Workflow: `cargo deny check`
- [ ] Workflow: build with the release profile and report binary size
- [ ] Add `dependabot.yml` or equivalent for dependency updates
- [ ] Add a CI badge to the README
- [ ] Protect the main branch with required checks

## Phase 7: Documentation and community files
- [ ] README: overview, install, quick start, `ping` and `schema` examples
- [ ] README: configuration resolution order and example `config.toml` / `.tpt.toml`
- [ ] README: agent/machine mode (`--output json`) and the JSON envelope
- [ ] README: exit-code table
- [ ] README: library usage guide for other TPT repos
- [ ] README: licensing and the dependency-licence policy
- [ ] README: state clearly that the `net` feature pulls in ring (Apache-2.0 AND ISC) and the core lib does not
- [ ] `CHANGELOG.md`
- [ ] `CONTRIBUTING.md` (including the dual-licence contribution clause)
- [ ] `SECURITY.md`
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
