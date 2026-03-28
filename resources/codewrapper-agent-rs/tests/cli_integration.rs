//! CLI integration tests

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("codeagent").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("codeagent"))
        .stdout(predicate::str::contains("--backend"))
        .stdout(predicate::str::contains("--model"));
}

#[test]
fn test_version_output() {
    let mut cmd = Command::cargo_bin("codeagent").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_unknown_backend_error() {
    let mut cmd = Command::cargo_bin("codeagent").unwrap();
    cmd.args(["--backend", "unknown", "test task"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Backend not found"));
}

#[test]
fn test_init_command() {
    let mut cmd = Command::cargo_bin("codeagent").unwrap();
    cmd.arg("init").assert().success();
}

#[test]
fn test_cleanup_command() {
    let mut cmd = Command::cargo_bin("codeagent").unwrap();
    cmd.arg("--cleanup").assert().success();
}
