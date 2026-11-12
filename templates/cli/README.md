# {{ app_name }}

A command-line tool built on the TPT framework (`tpt-cli`).

```sh
cargo run -- --help
cargo run -- greet Ada
cargo run -- --output json greet Ada
cargo run -- schema
```

## Configuration

Settings resolve in this order, later items win: defaults, the global file, `./.{{ app_name }}.toml`,
`{{ app_name | upcase | replace: "-", "_" }}_*` environment variables, and command-line flags.

```toml
output = "human"      # or "json"
greeting = "Hello"
```

## Adding a command

Add a variant to `Command` in `src/main.rs`, handle it in `main`, and it appears in `schema` automatically.
Print results with `output.data(human, &value)` so they work in both output modes.
