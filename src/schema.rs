//! Describes a clap command tree as JSON, so agents can discover commands
//! without parsing `--help` output.
//!
//! The functions here work with any clap [`Command`]. The output has these fields:
//!
//! * `schema_version`: bumped on breaking changes to this format.
//! * `name`, `about`, `args`, `subcommands`: one node per command.
//! * Each arg has `id`, `long`, `short`, `positional`, `required`, `global`,
//!   `type` (`string`, `bool`, `count` or `enum`), `multiple`, `values` (enum
//!   choices, or `null`), `default`, `env` and `help`.
//! * `exit_codes` and `output`: the framework's exit codes and output envelope.
//!
//! Global args are listed on the root. Single-command output also includes the
//! globals, so each object stands alone.

use clap::{Arg, ArgAction, Command};
use serde_json::{Map, Value, json};

use crate::errors::EXIT_CODES;

/// Version of the schema format. Increment it on breaking changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Returns the full schema for `root` and every subcommand beneath it.
///
/// ```
/// use clap::Command;
/// use tpt::schema::{command_tree, SCHEMA_VERSION};
///
/// let root = Command::new("acme").subcommand(Command::new("ping"));
/// let schema = command_tree(&root);
///
/// assert_eq!(schema["schema_version"], SCHEMA_VERSION);
/// assert_eq!(schema["subcommands"][0]["name"], "ping");
/// ```
pub fn command_tree(root: &Command) -> Value {
    let root = built(root);
    let mut node = node(&root, true);
    node.insert("schema_version".into(), json!(SCHEMA_VERSION));
    node.insert("exit_codes".into(), exit_codes());
    node.insert("output".into(), output_spec());
    Value::Object(node)
}

/// Returns the schema for one top-level subcommand of `root`, or `None` if it does not exist.
///
/// ```
/// use clap::Command;
/// use tpt::schema::command_schema;
///
/// let root = Command::new("acme").subcommand(Command::new("ping"));
/// assert!(command_schema(&root, "ping").is_some());
/// assert!(command_schema(&root, "nope").is_none());
/// ```
pub fn command_schema(root: &Command, name: &str) -> Option<Value> {
    let root = built(root);
    let sub = root
        .get_subcommands()
        .find(|c| c.get_name() == name || c.get_all_aliases().any(|alias| alias == name))?;
    Some(Value::Object(node(sub, true)))
}

fn built(cmd: &Command) -> Command {
    let mut cmd = cmd.clone();
    cmd.build();
    cmd
}

fn node(cmd: &Command, include_globals: bool) -> Map<String, Value> {
    let args: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| !matches!(a.get_action(), ArgAction::Help | ArgAction::Version))
        .filter(|a| include_globals || !a.is_global_set())
        .map(arg_node)
        .collect();
    let subcommands: Vec<Value> =
        cmd.get_subcommands().filter(|c| !c.is_hide_set()).map(|c| Value::Object(node(c, false))).collect();

    let mut map = Map::new();
    map.insert("name".into(), json!(cmd.get_name()));
    map.insert("about".into(), json!(cmd.get_about().map(|s| s.to_string())));
    map.insert("args".into(), Value::Array(args));
    map.insert("subcommands".into(), Value::Array(subcommands));
    map
}

fn arg_node(arg: &Arg) -> Value {
    let action = arg.get_action();
    let (kind, values) = match action {
        ArgAction::SetTrue | ArgAction::SetFalse => ("bool", Vec::new()),
        ArgAction::Count => ("count", Vec::new()),
        _ => {
            let values: Vec<String> = arg
                .get_possible_values()
                .iter()
                .filter(|v| !v.is_hide_set())
                .map(|v| v.get_name().to_owned())
                .collect();
            if values.is_empty() { ("string", values) } else { ("enum", values) }
        }
    };

    json!({
        "id": arg.get_id().as_str(),
        "help": arg.get_help().map(|h| h.to_string()),
        "long": arg.get_long(),
        "short": arg.get_short().map(|c| c.to_string()),
        "positional": arg.is_positional(),
        "required": arg.is_required_set(),
        "global": arg.is_global_set(),
        "type": kind,
        "multiple": matches!(action, ArgAction::Append),
        "values": if values.is_empty() { Value::Null } else { json!(values) },
        "default": arg.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        "env": arg.get_env().map(|e| e.to_string_lossy().into_owned()),
    })
}

fn exit_codes() -> Value {
    Value::Array(
        EXIT_CODES.iter().map(|e| json!({ "code": e.code, "name": e.name, "meaning": e.meaning })).collect(),
    )
}

fn output_spec() -> Value {
    json!({
        "success": { "ok": true, "data": "command-specific object" },
        "error": {
            "ok": false,
            "error": { "code": "string", "message": "string", "hint": "string or null" }
        },
        "notes": "In json mode, stdout holds only the envelope. Logs go to stderr."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Command {
        Command::new("demo")
            .about("demo tool")
            .arg(
                Arg::new("output")
                    .long("output")
                    .global(true)
                    .value_parser(["human", "json"])
                    .env("DEMO_OUTPUT"),
            )
            .arg(Arg::new("verbose").short('v').long("verbose").action(ArgAction::SetTrue))
            .subcommand(
                Command::new("greet")
                    .about("say hello")
                    .arg(Arg::new("name").required(true))
                    .arg(Arg::new("times").long("times").default_value("1")),
            )
    }

    #[test]
    fn tree_lists_every_subcommand() {
        let tree = command_tree(&sample());
        let names: Vec<&str> =
            tree["subcommands"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
        // clap adds its own `help` subcommand during build(), so it is listed too.
        assert_eq!(names, ["greet", "help"]);
        assert_eq!(tree["schema_version"], json!(SCHEMA_VERSION));
    }

    #[test]
    fn arg_types_and_values_are_described() {
        let tree = command_tree(&sample());
        let root_args = tree["args"].as_array().unwrap();
        let output = root_args.iter().find(|a| a["id"] == "output").unwrap();
        assert_eq!(output["type"], "enum");
        assert_eq!(output["values"], json!(["human", "json"]));
        assert_eq!(output["global"], true);
        assert_eq!(output["env"], "DEMO_OUTPUT");
        let verbose = root_args.iter().find(|a| a["id"] == "verbose").unwrap();
        assert_eq!(verbose["type"], "bool");
        assert_eq!(verbose["short"], "v");
    }

    #[test]
    fn single_command_includes_globals_and_positionals() {
        let node = command_schema(&sample(), "greet").unwrap();
        let ids: Vec<&str> =
            node["args"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"name"));
        assert!(ids.contains(&"output"), "globals are included for single commands");
        let name = node["args"].as_array().unwrap().iter().find(|a| a["id"] == "name").unwrap();
        assert_eq!(name["positional"], true);
        assert_eq!(name["required"], true);
        let times = node["args"].as_array().unwrap().iter().find(|a| a["id"] == "times").unwrap();
        assert_eq!(times["default"], json!(["1"]));
    }

    #[test]
    fn unknown_command_returns_none() {
        assert!(command_schema(&sample(), "missing").is_none());
    }

    #[test]
    fn help_and_version_args_are_omitted() {
        let tree = command_tree(&sample());
        let ids: Vec<&str> =
            tree["args"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"help"));
        assert!(!ids.contains(&"version"));
    }
}
