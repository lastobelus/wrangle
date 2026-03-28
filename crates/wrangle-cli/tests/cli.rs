use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn backends_json_lists_qwen_and_opencode() {
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["backends", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"qwen\""))
        .stdout(predicate::str::contains("\"name\": \"opencode\""))
        .stdout(predicate::str::contains(
            "\"supportsPersistentBackend\": true",
        ));
}

#[test]
fn playbook_land_work_dry_run_prints_playbook_plan() {
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args([
        "--dry-run",
        "--backend",
        "qwen",
        "playbook",
        "land-work",
        "Ship the feature",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"playbook\": \"land-work\""))
    .stdout(predicate::str::contains("Run the `land-work` playbook."));
}
