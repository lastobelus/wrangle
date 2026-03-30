use assert_cmd::Command;
use predicates::prelude::*;

fn parallel_input(tasks: &[(&str, &str, &[&str])]) -> String {
    tasks
        .iter()
        .map(|(id, task, deps)| {
            let deps_json = deps
                .iter()
                .map(|d| format!("\"{}\"", d))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"id":"{}","task":"{}","dependencies":[{}]}}"#,
                id, task, deps_json
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn parallel_dry_run_prints_plan_with_phases() {
    let input = parallel_input(&[
        ("plan", "inspect the repo", &[]),
        ("implement", "implement the plan", &["plan"]),
        ("test", "run the tests", &["implement"]),
    ]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"taskCount\": 3"))
        .stdout(predicate::str::contains("\"phases\""))
        .stdout(predicate::str::contains("\"maxWorkers\""));
}

#[test]
fn parallel_dry_run_shows_execution_phases() {
    let input = parallel_input(&[
        ("a", "task a", &[]),
        ("b", "task b", &[]),
        ("c", "task c", &["a", "b"]),
    ]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"tasks\""));
}

#[test]
fn parallel_rejects_circular_dependency() {
    let input = parallel_input(&[("a", "task a", &["b"]), ("b", "task b", &["a"])]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("circular dependency"));
}

#[test]
fn parallel_rejects_unknown_dependency() {
    let input = parallel_input(&[("a", "task a", &["nonexistent"])]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown"));
}

#[test]
fn parallel_rejects_self_dependency() {
    let input = r#"{"id":"a","task":"task a","dependencies":["a"]}"#;
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("depend on itself"));
}

#[test]
fn parallel_rejects_duplicate_ids() {
    let input = r#"{"id":"a","task":"task 1","dependencies":[]}
{"id":"a","task":"task 2","dependencies":[]}"#;
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate"));
}

#[test]
fn parallel_rejects_empty_task_list() {
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no tasks"));
}

#[test]
fn parallel_rejects_empty_task_field() {
    let input = r#"{"id":"a","task":"","dependencies":[]}"#;
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("non-empty"));
}

#[test]
fn parallel_three_node_cycle_diagnosed() {
    let input = parallel_input(&[
        ("a", "task a", &["c"]),
        ("b", "task b", &["a"]),
        ("c", "task c", &["b"]),
    ]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("circular dependency"))
        .stderr(
            predicate::str::contains("a")
                .and(predicate::str::contains("b"))
                .and(predicate::str::contains("c")),
        );
}

#[test]
fn parallel_dry_run_accepts_valid_diamond_graph() {
    let input = parallel_input(&[
        ("root", "root task", &[]),
        ("left", "left branch", &["root"]),
        ("right", "right branch", &["root"]),
        ("merge", "merge results", &["left", "right"]),
    ]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"taskCount\": 4"));
}

#[test]
fn parallel_dry_run_unknown_dependency_names_task() {
    let input = parallel_input(&[("a", "task a", &[]), ("b", "task b", &["missing"])]);
    let mut cmd = Command::cargo_bin("wrangle").unwrap();
    cmd.args(["--parallel", "--dry-run", "--backend", "qwen"])
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("b"))
        .stderr(predicate::str::contains("missing"));
}
