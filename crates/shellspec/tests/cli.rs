//! The commands of `spec/cli.yaml` through the generated adapter, so every result is checked
//! against the shape the binding declares.

use std::ffi::OsString;

use cli_contract::{Handler, HandlerReply, Invocation, OsSources, ProcessOutput};
use serde_json::{Value, json};

struct Commands;

impl Handler for Commands {
    fn call(&mut self, inv: &Invocation<'_>) -> HandlerReply {
        match shellspec::cli::call(inv.callable, &inv.input) {
            Ok(Some(v)) => HandlerReply::Success(v),
            Ok(None) => HandlerReply::Unavailable,
            Err((code, detail)) => HandlerReply::Error {
                code,
                data: json!({ "detail": detail }),
            },
        }
    }
}

fn run(args: &[&str]) -> ProcessOutput {
    let mut argv: Vec<OsString> = vec!["shellspec".into()];
    argv.extend(args.iter().map(OsString::from));
    cli_contract::run(argv, &mut OsSources, &mut Commands, None)
}

fn ok(args: &[&str]) -> Value {
    let out = run(args);
    assert_eq!(out.exit_code, 0, "{args:?}: {} {}", out.stdout, out.stderr);
    serde_json::from_str(&out.stdout).unwrap()
}

#[test]
fn invocations_answers_the_declared_shape() {
    let v = ok(&[
        "invocations",
        "--dialect",
        "zsh",
        "--command",
        "cd x && slot cargo test 2>&1 | tail -20",
    ]);
    let cats: Vec<&str> = v["invocations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["category"].as_str().unwrap())
        .collect();
    assert_eq!(cats, ["navigate", "test", "filter"]);
    assert_eq!(v["invocations"][1]["wrappers"], json!(["slot"]));
    assert_eq!(v["dominant"], 1);
    assert_eq!(v["ruleset"], shellspec::Ruleset::shared().digest);
}

#[test]
fn lift_shape_and_rules_check_answer() {
    let v = ok(&["lift", "--dialect", "bash", "--command", "a | b"]);
    assert_eq!(v["format"], "shell-ir/1");
    assert_eq!(v["script"]["body"][0]["body"]["kind"], "Pipeline");
    let v = ok(&[
        "shape",
        "--dialect",
        "fish",
        "--command",
        "git status; and echo ok",
    ]);
    assert_eq!(v["shape"], "git status && echo <arg>");
    let v = ok(&["rules", "check"]);
    assert_eq!(
        v["files"].as_array().unwrap().len(),
        shellspec::rules::builtin_files().len()
    );
}

#[test]
fn a_ruleset_that_cannot_be_read_is_a_declared_refusal() {
    let out = run(&[
        "--output",
        "json",
        "rules",
        "check",
        "--ruleset",
        "/nonexistent/shellspec-rules",
    ]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.contains("ruleset_invalid") || out.stderr.contains("ruleset_invalid"));
}

#[test]
fn corpus_stats_counts_without_text() {
    let dir = tmp_dir();
    let input = dir.join("commands.jsonl");
    std::fs::write(
        &input,
        "\"cd x && cargo test | tail\"\n{\"command\": \"frobnicate --secret-flag\"}\n\"echo `oops\"\nnot json\n",
    )
    .unwrap();
    let v = ok(&[
        "corpus",
        "stats",
        "--dialect",
        "zsh",
        "--input",
        input.to_str().unwrap(),
    ]);
    assert_eq!(v["commands"], 3);
    assert_eq!(v["refused"], 1);
    assert_eq!(v["invocations"], 5);
    assert_eq!(v["unk_invocations"], 2);
    assert_eq!(v["unk_permille"], 400);
    assert_eq!(
        v["unk_programs"],
        json!([{"key": "frobnicate", "count": 1}])
    );
    assert!(!v.to_string().contains("secret-flag"));
}

/// A directory under the target directory, never `/tmp` (workspace rule).
fn tmp_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
