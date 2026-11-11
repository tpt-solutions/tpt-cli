//! End-to-end tests that run the `tpt` binary against a local test server.
//! They check exit codes, the human and JSON output modes, and that stdout
//! is valid JSON in JSON mode.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_tpt");

/// Serves `responses` in order, one per connection, on a random local port.
/// Returns the base URL.
fn serve(responses: Vec<(u16, &'static str)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else { return };
            drain_request(&mut stream);
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    format!("http://{addr}/")
}

/// Accepts connections but never answers, to force a timeout.
fn serve_silent() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    });
    format!("http://{addr}/")
}

fn drain_request(stream: &mut TcpStream) {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut buf = [0u8; 1024];
    let mut seen = Vec::new();
    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => seen.extend_from_slice(&buf[..n]),
        }
    }
}

/// A port that was free a moment ago and is now closed, so connecting is refused.
fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/")
}

/// A fresh, empty working directory, so no stray `.tpt.toml` is picked up.
fn empty_cwd() -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tpt-cli-it-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tpt(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(empty_cwd())
        .env_remove("TPT_OUTPUT")
        .env_remove("TPT_LOG_LEVEL")
        .env_remove("TPT_TIMEOUT_SECS")
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("process was terminated by a signal")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn ping_reports_status_and_latency_in_human_mode() {
    let url = serve(vec![(200, "hello")]);
    let out = tpt(&["ping", &url]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(" 200 OK "), "stdout: {text}");
    assert!(text.contains(" ms "), "stdout: {text}");
}

#[test]
fn ping_json_mode_prints_a_valid_envelope_only() {
    let url = serve(vec![(204, "")]);
    let out = tpt(&["--output", "json", "ping", &url]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let value: Value = serde_json::from_str(&stdout(&out)).expect("stdout must be strict JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["status"], 204);
    assert!(value["data"]["latency_ms"].as_f64().is_some());
    assert_eq!(value["data"]["timeout_secs"], 10);
}

#[test]
fn ping_non_success_status_is_still_a_reply() {
    let url = serve(vec![(503, "busy")]);
    let out = tpt(&["--output", "json", "ping", &url]);
    assert_eq!(code(&out), 0);
    let value: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["data"]["status"], 503);
}

#[test]
fn connection_refused_exits_with_connection_code() {
    let out = tpt(&["ping", &closed_port_url()]);
    assert_eq!(code(&out), 6, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("hint:"), "stderr: {}", stderr(&out));
}

#[test]
fn connection_refused_json_mode_emits_error_envelope() {
    let out = tpt(&["--output", "json", "ping", &closed_port_url()]);
    assert_eq!(code(&out), 6);
    let value: Value = serde_json::from_str(&stdout(&out)).expect("error envelope must be JSON");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "connection");
    assert!(value["error"]["hint"].is_string());
}

#[test]
fn timeout_exits_with_timeout_code() {
    let url = serve_silent();
    let out = tpt(&["ping", &url, "--timeout", "1"]);
    assert_eq!(code(&out), 7, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("timed out"), "stderr: {}", stderr(&out));
}

#[test]
fn bad_url_is_a_usage_error() {
    let out = tpt(&["ping", "ftp://example.com/file"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("unsupported URL scheme"));
}

#[test]
fn zero_timeout_is_a_usage_error() {
    let out = tpt(&["ping", "http://127.0.0.1:9/", "--timeout", "0"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("at least 1 second"));
}

#[test]
fn unknown_flag_exits_with_usage_code() {
    let out = tpt(&["--no-such-flag"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn unknown_flag_in_json_mode_still_emits_json_on_stdout() {
    let out = tpt(&["--output", "json", "--no-such-flag"]);
    assert_eq!(code(&out), 2);
    let value: Value = serde_json::from_str(&stdout(&out)).expect("usage error must be JSON in json mode");
    assert_eq!(value["error"]["code"], "usage");
}

#[test]
fn invalid_output_value_is_rejected_by_clap() {
    let out = tpt(&["--output", "xml", "schema"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn help_exits_zero_and_lists_subcommands() {
    let out = tpt(&["--help"]);
    assert_eq!(code(&out), 0);
    let text = stdout(&out);
    assert!(text.contains("ping"));
    assert!(text.contains("schema"));
}

#[test]
fn no_arguments_prints_help_with_usage_code() {
    let out = tpt(&[]);
    assert_eq!(code(&out), 2);
}

#[test]
fn env_output_json_applies_to_errors() {
    let out = Command::new(BIN)
        .args(["ping", "ftp://example.com"])
        .current_dir(empty_cwd())
        .env("TPT_OUTPUT", "json")
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
    let value: Value = serde_json::from_str(&stdout(&out)).expect("TPT_OUTPUT=json must apply to errors");
    assert_eq!(value["ok"], false);
}

#[test]
fn local_config_file_sets_output_mode() {
    let cwd = empty_cwd();
    std::fs::write(cwd.join(".tpt.toml"), "output = \"json\"\n").unwrap();
    let out = Command::new(BIN)
        .args(["ping", &closed_port_url()])
        .current_dir(&cwd)
        .env_remove("TPT_OUTPUT")
        .output()
        .unwrap();
    assert_eq!(code(&out), 6);
    let value: Value = serde_json::from_str(&stdout(&out)).expect("local config should select json");
    assert_eq!(value["error"]["code"], "connection");
}

#[test]
fn malformed_local_config_reports_location_and_exits_3() {
    let cwd = empty_cwd();
    std::fs::write(cwd.join(".tpt.toml"), "output = \"json\"\ntimeout_secs = = 5\n").unwrap();
    let out = Command::new(BIN).args(["schema"]).current_dir(&cwd).env_remove("TPT_OUTPUT").output().unwrap();
    assert_eq!(code(&out), 3);
    let err = stderr(&out);
    assert!(err.contains(".tpt.toml:2:"), "stderr: {err}");
}

#[test]
fn quiet_hides_info_and_warn_logs() {
    let cwd = empty_cwd();
    std::fs::write(cwd.join(".tpt.toml"), "log_level = \"trace\"\n").unwrap();
    let out = Command::new(BIN)
        .args(["--quiet", "ping", &closed_port_url()])
        .current_dir(&cwd)
        .env_remove("TPT_LOG_LEVEL")
        .output()
        .unwrap();
    assert_eq!(code(&out), 6);
    assert!(!stderr(&out).contains("debug:"), "stderr: {}", stderr(&out));
}
