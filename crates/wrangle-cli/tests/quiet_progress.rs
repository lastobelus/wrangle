use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_includes_quiet_and_progress_flags() {
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--progress-file"))
        .stdout(predicate::str::contains("--quiet-until-complete"));
}

#[test]
fn dry_run_accepts_quiet_and_progress_flags() {
    let progress = std::env::temp_dir().join("wrangle-dry-run-progress.jsonl");
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args([
        "--dry-run",
        "--backend",
        "qwen",
        "--progress-file",
        progress.to_str().unwrap(),
        "--quiet-until-complete",
        "Inspect this crate",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"backend\""))
    .stdout(predicate::str::contains("\"request\""));
}
