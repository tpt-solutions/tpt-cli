//! Snapshot and drift tests for `tpt schema`.
//!
//! To accept an intentional change to the schema, run:
//! `UPDATE_SNAPSHOTS=1 cargo test --test schema`

use std::path::PathBuf;

use clap::CommandFactory;
use serde_json::Value;

use tpt::cli::Cli;
use tpt::schema;

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots").join("schema.json")
}

#[test]
fn schema_matches_snapshot() {
    let tree = schema::command_tree(&Cli::command());
    let mut rendered = serde_json::to_string_pretty(&tree).unwrap();
    rendered.push('\n');

    let path = snapshot_path();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rendered).unwrap();
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("snapshot missing at {}; run with UPDATE_SNAPSHOTS=1", path.display()));
    assert_eq!(
        expected.replace("\r\n", "\n"),
        rendered.replace("\r\n", "\n"),
        "schema changed; if intentional, run with UPDATE_SNAPSHOTS=1"
    );
}

/// Every clap subcommand must appear in the schema, so the schema cannot drift.
#[test]
fn every_registered_command_appears_in_schema() {
    let root = Cli::command();
    let tree = schema::command_tree(&root);
    let listed: Vec<&str> =
        tree["subcommands"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();

    for sub in root.get_subcommands() {
        assert!(listed.contains(&sub.get_name()), "`{}` missing from schema", sub.get_name());
        let single = schema::command_schema(&root, sub.get_name())
            .unwrap_or_else(|| panic!("`{}` has no single-command schema", sub.get_name()));
        assert_eq!(single["name"], sub.get_name());
    }
    assert!(listed.contains(&"schema"));
}

#[test]
fn schema_documents_exit_codes_and_envelope() {
    let tree: Value = schema::command_tree(&Cli::command());
    let codes = tree["exit_codes"].as_array().unwrap();
    assert!(codes.iter().any(|c| c["code"] == 2 && c["name"] == "usage"));
    assert!(tree["output"]["error"]["error"]["code"].is_string());
}

#[test]
fn ping_documents_its_url_argument_and_timeout() {
    let node = schema::command_schema(&Cli::command(), "ping").unwrap();
    let args = node["args"].as_array().unwrap();
    let url = args.iter().find(|a| a["id"] == "url").unwrap();
    assert_eq!(url["positional"], true);
    assert_eq!(url["required"], true);
    let timeout = args.iter().find(|a| a["id"] == "timeout").unwrap();
    assert_eq!(timeout["long"], "timeout");
}
