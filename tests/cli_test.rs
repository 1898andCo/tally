//! CLI integration tests — invoke the tally binary and verify output/exit codes.

use assert_cmd::Command;
use predicates::prelude::*;

/// Create a temporary git repo for CLI tests.
fn setup_cli_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let repo_path = tmp.path();

    // git init + initial commit
    let repo = git2::Repository::init(repo_path).expect("init");
    let sig = git2::Signature::now("test", "test@test.com").expect("sig");
    let blob = repo.blob(b"# test").expect("blob");
    let mut builder = repo.treebuilder(None).expect("tb");
    builder.insert("README.md", blob, 0o100_644).expect("insert");
    let tree_oid = builder.write().expect("write");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .expect("commit");

    tmp
}

fn tally() -> Command {
    Command::cargo_bin("tally").expect("tally binary")
}

// =============================================================================
// Positive tests
// =============================================================================

#[test]
fn cli_help_shows_subcommands() {
    tally()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("record"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("stats"));
}

#[test]
fn cli_init_succeeds() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Initialized"));
}

#[test]
fn cli_init_is_idempotent() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();
    tally().arg("init").current_dir(tmp.path()).assert().success();
}

#[test]
fn cli_record_creates_finding() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "unwrap on Option",
            "--rule", "unsafe-unwrap",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"created\""))
        .stdout(predicate::str::contains("\"uuid\""));
}

#[test]
fn cli_record_deduplicates() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    // Record same finding twice
    for _ in 0..2 {
        tally()
            .args([
                "record",
                "--file", "src/main.rs",
                "--line", "42",
                "--severity", "important",
                "--title", "unwrap on Option",
                "--rule", "unsafe-unwrap",
            ])
            .current_dir(tmp.path())
            .assert()
            .success();
    }

    // Second call should say "deduplicated"
    let output = tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "unwrap on Option",
            "--rule", "unsafe-unwrap",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("deduplicated"),
        "third record of same finding should deduplicate: {stdout}"
    );
}

#[test]
fn cli_query_json_returns_array() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    // Record a finding
    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "critical",
            "--title", "sql injection",
            "--rule", "sql-injection",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query
    tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("sql injection"))
        .stdout(predicate::str::contains("critical"));
}

#[test]
fn cli_query_empty_returns_empty_array() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn cli_query_table_format() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "test finding",
            "--rule", "test-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--format", "table"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Severity"))
        .stdout(predicate::str::contains("IMPORTANT"));
}

#[test]
fn cli_stats_shows_counts() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "a.rs", "--line", "1",
            "--severity", "critical", "--title", "a", "--rule", "r",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args([
            "record",
            "--file", "b.rs", "--line", "2",
            "--severity", "suggestion", "--title", "b", "--rule", "s",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .arg("stats")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Total:       2"));
}

// =============================================================================
// Negative tests
// =============================================================================

#[test]
fn cli_record_before_init_fails() {
    let tmp = setup_cli_repo();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "test", "--rule", "test",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("findings-data"));
}

#[test]
fn cli_record_invalid_severity_fails() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "ultra-critical",
            "--title", "test", "--rule", "test",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid severity"));
}

#[test]
fn cli_update_invalid_transition_fails() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    // Record a finding
    let output = tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "test", "--rule", "test",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse json");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Try invalid transition: open -> closed
    tally()
        .args(["update", uuid, "--status", "closed"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid state transition"));
}

#[test]
fn cli_query_json_includes_short_id() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "critical",
            "--title", "test",
            "--rule", "test-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"short_id\": \"C1\""));
}

#[test]
fn cli_query_table_includes_short_id_column() {
    let tmp = setup_cli_repo();
    tally().arg("init").current_dir(tmp.path()).assert().success();

    tally()
        .args([
            "record",
            "--file", "src/main.rs",
            "--line", "42",
            "--severity", "important",
            "--title", "test",
            "--rule", "test-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--format", "table"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("I1"));
}

#[test]
fn cli_outside_git_repo_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // No git init — just an empty directory

    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .failure();
}
