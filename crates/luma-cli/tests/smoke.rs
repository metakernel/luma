//! CLI smoke tests.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn parse_json_emits_ast_without_engine_feature() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "name: Example\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args([
            "parse",
            path.to_str().unwrap(),
            "--output",
            "json",
            "--emit",
            "ast",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\":true"))
        .stdout(predicate::str::contains("\"ast\""));
}

#[test]
fn default_mode_checks_without_evaluation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "value: |expr\n  os.getenv('X')\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .arg(path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[cfg(not(feature = "engine-omnilua"))]
#[test]
fn eval_reports_missing_engine_when_backend_not_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.luma");
    fs::write(&path, "value: |expr\n  1 + 1\n").unwrap();

    Command::cargo_bin("luma")
        .unwrap()
        .args(["eval", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("engine-omnilua"));
}
