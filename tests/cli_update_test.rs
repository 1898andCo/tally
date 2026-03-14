//! CLI integration tests -- update and suppress.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// =============================================================================
// Update tests
// =============================================================================

#[test]
fn cli_update_invalid_transition_fails() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record a finding
    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "important",
            "--title",
            "test",
            "--rule",
            "test",
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
fn cli_update_with_reason() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "important",
            "--title",
            "test finding",
            "--rule",
            "test",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "update",
            uuid,
            "--status",
            "acknowledged",
            "--reason",
            "reviewed and confirmed",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verify state_history has the reason
    let query_output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_output.stdout);
    assert!(
        stdout.contains("reviewed and confirmed"),
        "state_history should contain the reason: {stdout}"
    );
}

#[test]
fn cli_update_with_commit() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "important",
            "--title",
            "test finding",
            "--rule",
            "test",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Transition open -> acknowledged first, then acknowledged -> in_progress with commit
    tally()
        .args(["update", uuid, "--status", "acknowledged"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args([
            "update",
            uuid,
            "--status",
            "in_progress",
            "--commit",
            "abc123def456",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let query_output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_output.stdout);
    assert!(
        stdout.contains("abc123def456"),
        "state_history should contain the commit SHA: {stdout}"
    );
}

#[test]
fn cli_update_with_relationship() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record two findings
    let output_a = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "A",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_a.stdout)).expect("parse");
    let uuid_a = json_a["uuid"].as_str().expect("uuid");

    let output_b = tally()
        .args([
            "record",
            "--file",
            "src/b.rs",
            "--line",
            "2",
            "--severity",
            "important",
            "--title",
            "B",
            "--rule",
            "r2",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_b: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_b.stdout)).expect("parse");
    let uuid_b = json_b["uuid"].as_str().expect("uuid");

    // Update B with relationship to A
    tally()
        .args([
            "update",
            uuid_b,
            "--status",
            "acknowledged",
            "--related-to",
            uuid_a,
            "--relationship",
            "blocks",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn cli_update_with_short_id() {
    // Record finding, query to get short ID C1, then update using short ID
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "short id test",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verify short ID C1 exists
    tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"short_id\": \"C1\""));

    // Update using short ID "C1"
    tally()
        .args(["update", "C1", "--status", "acknowledged"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"acknowledged\""));
}

// =============================================================================
// Rebuild Index
// =============================================================================

#[test]
fn cli_rebuild_index() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args([
            "record",
            "--file",
            "a.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "a",
            "--rule",
            "r1",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args([
            "record",
            "--file",
            "b.rs",
            "--line",
            "2",
            "--severity",
            "suggestion",
            "--title",
            "b",
            "--rule",
            "r2",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .arg("rebuild-index")
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Index rebuilt"));
}

// =============================================================================
// Suppress tests
// =============================================================================

#[test]
fn cli_suppress_with_expiry() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "minor issue",
            "--rule",
            "minor",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "not relevant now",
            "--expires",
            "2099-12-31T00:00:00Z",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("suppressed"));
}

#[test]
fn cli_suppress_invalid_date() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "test",
            "--expires",
            "not-a-date",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_suppress_file_level_type() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "file-level suppression",
            "--suppression-type",
            "file",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("suppressed"));

    // Verify suppression type via query
    let query_output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_output.stdout);
    assert!(
        stdout.contains("file_level"),
        "should have file_level suppression type: {stdout}"
    );
}

#[test]
fn cli_suppress_inline_type_with_pattern() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "inline suppression",
            "--suppression-type",
            "inline",
            "--suppression-pattern",
            "tally:suppress",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("suppressed"));

    let query_output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_output.stdout);
    assert!(
        stdout.contains("tally:suppress"),
        "should have inline suppression pattern: {stdout}"
    );
}

#[test]
fn cli_suppress_inline_without_pattern_fails() {
    // --suppression-type inline without --suppression-pattern -> should fail
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "test",
            "--suppression-type",
            "inline",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --suppression-pattern"));
}

#[test]
fn cli_suppress_invalid_type_fails() {
    // --suppression-type forever -> should fail
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "test",
            "--suppression-type",
            "forever",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid suppression type"));
}

#[test]
fn cli_suppress_already_resolved_fails() {
    // Record finding, transition to resolved, then try to suppress -> InvalidTransition
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "important",
            "--title",
            "resolved finding",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Transition: open -> acknowledged -> in_progress -> resolved
    tally()
        .args(["update", uuid, "--status", "acknowledged"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", uuid, "--status", "in_progress"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", uuid, "--status", "resolved"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Try to suppress resolved finding -> should fail
    tally()
        .args(["suppress", uuid, "--reason", "try suppress resolved"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_suppress_cannot_suppress_from_closed() {
    // Covers the InvalidTransition path for suppress from closed state
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "1",
            "--severity",
            "important",
            "--title",
            "will be closed",
            "--rule",
            "close-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Transition: open -> in_progress -> resolved -> closed
    tally()
        .args(["update", uuid, "--status", "in_progress"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", uuid, "--status", "resolved"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", uuid, "--status", "closed"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Try to suppress from closed -> should fail (closed is terminal)
    tally()
        .args(["suppress", uuid, "--reason", "try suppress closed"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}
