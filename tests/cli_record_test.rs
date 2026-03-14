//! CLI integration tests -- record, dedup, batch, locations, relationships.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// =============================================================================
// Record tests
// =============================================================================

#[test]
fn cli_record_creates_finding() {
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
            "42",
            "--severity",
            "important",
            "--title",
            "unwrap on Option",
            "--rule",
            "unsafe-unwrap",
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
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record same finding twice
    for _ in 0..2 {
        tally()
            .args([
                "record",
                "--file",
                "src/main.rs",
                "--line",
                "42",
                "--severity",
                "important",
                "--title",
                "unwrap on Option",
                "--rule",
                "unsafe-unwrap",
            ])
            .current_dir(tmp.path())
            .assert()
            .success();
    }

    // Second call should say "deduplicated"
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
            "unwrap on Option",
            "--rule",
            "unsafe-unwrap",
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
fn cli_record_before_init_fails() {
    let tmp = setup_cli_repo();

    tally()
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
        .assert()
        .failure()
        .stderr(predicate::str::contains("findings-data"));
}

#[test]
fn cli_record_invalid_severity_fails() {
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
            "42",
            "--severity",
            "ultra-critical",
            "--title",
            "test",
            "--rule",
            "test",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid severity"));
}

#[test]
fn cli_record_with_category() {
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
            "src/auth.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "missing auth check",
            "--rule",
            "auth-check",
            "--category",
            "auth",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"category\": \"auth\""),
        "query output should contain category field: {stdout}"
    );
}

#[test]
fn cli_record_with_suggested_fix() {
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
            "10",
            "--severity",
            "suggestion",
            "--title",
            "use ? operator",
            "--rule",
            "error-handling",
            "--suggested-fix",
            "use ? operator",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("use ? operator"),
        "query output should contain suggested_fix: {stdout}"
    );
}

#[test]
fn cli_record_with_evidence() {
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
            "42",
            "--severity",
            "important",
            "--title",
            "unwrap usage",
            "--rule",
            "unsafe-unwrap",
            "--evidence",
            "line 42: unwrap()",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("line 42: unwrap()"),
        "query output should contain evidence: {stdout}"
    );
}

// =============================================================================
// Dedup tests
// =============================================================================

#[test]
fn cli_dedup_same_location_unchanged() {
    // Record finding, then record same finding again -> dedup, location unchanged
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record finding
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
            "dedup test",
            "--rule",
            "dedup-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Record same finding again (same file+line+rule = same fingerprint = dedup)
    let dedup_output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "important",
            "--title",
            "dedup test",
            "--rule",
            "dedup-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let dedup_stdout = String::from_utf8_lossy(&dedup_output.stdout);
    assert!(
        dedup_stdout.contains("\"status\": \"deduplicated\""),
        "should be deduplicated: {dedup_stdout}"
    );
    assert!(
        dedup_stdout.contains(uuid),
        "should reference original uuid: {dedup_stdout}"
    );
}

#[test]
fn cli_dedup_when_suppressed() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record a finding
    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "critical",
            "--title",
            "test",
            "--rule",
            "dedup-rule",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Suppress the finding
    tally()
        .args(["suppress", uuid, "--reason", "accepted"])
        .current_dir(dir)
        .assert()
        .success();

    // Record same fingerprint again — should dedup, not create new
    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "critical",
            "--title",
            "test",
            "--rule",
            "dedup-rule",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse");
    assert_eq!(
        json["status"], "deduplicated",
        "should dedup even when suppressed"
    );
    assert_eq!(json["uuid"], uuid, "should return same UUID");
}

#[test]
fn cli_dedup_ignores_severity_difference() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record as critical
    let output = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "test",
            "--rule",
            "my-rule",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Record same file+line+rule but with different severity — fingerprint
    // doesn't include severity, so this should still dedup
    let output = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "10",
            "--severity",
            "suggestion",
            "--title",
            "test",
            "--rule",
            "my-rule",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse");
    assert_eq!(
        json["status"], "deduplicated",
        "severity difference should still dedup"
    );
    assert_eq!(json["uuid"], uuid, "should return original UUID");
}

#[test]
fn cli_record_dedup_with_different_agent() {
    // Covers handler lines 599-606 (adding new agent to discovered_by on dedup)
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record finding with agent "claude-code"
    tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "important",
            "--title",
            "multi-agent finding",
            "--rule",
            "multi-agent-rule",
            "--agent",
            "claude-code",
            "--session",
            "sess-1",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record same finding with different agent "cursor"
    let dedup_output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "important",
            "--title",
            "multi-agent finding",
            "--rule",
            "multi-agent-rule",
            "--agent",
            "cursor",
            "--session",
            "sess-2",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let dedup_stdout = String::from_utf8_lossy(&dedup_output.stdout);
    assert!(
        dedup_stdout.contains("deduplicated"),
        "should be deduplicated: {dedup_stdout}"
    );

    // Query and verify both agents appear in discovered_by
    let query_out = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_out.stdout);
    assert!(
        stdout.contains("claude-code"),
        "should contain first agent claude-code: {stdout}"
    );
    assert!(
        stdout.contains("cursor"),
        "should contain second agent cursor: {stdout}"
    );
}

// =============================================================================
// No-dedup tests
// =============================================================================

#[test]
fn cli_no_dedup_different_rule_same_location() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record with rule-a
    let output1 = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "issue A",
            "--rule",
            "rule-a",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json1: serde_json::Value = serde_json::from_slice(&output1.stdout).expect("parse");
    let uuid1 = json1["uuid"].as_str().expect("uuid").to_string();

    // Record same file+line but different rule — different fingerprint, new finding
    let output2 = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "issue B",
            "--rule",
            "rule-b",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json2: serde_json::Value = serde_json::from_slice(&output2.stdout).expect("parse");
    assert_eq!(
        json2["status"], "created",
        "different rule should create new finding"
    );
    assert_ne!(
        json2["uuid"].as_str().expect("uuid"),
        uuid1,
        "should have different UUID"
    );
}

#[test]
fn cli_no_dedup_same_rule_distant_line() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record at line 10
    tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "first",
            "--rule",
            "my-rule",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Record same rule at line 100 — well beyond proximity threshold of 5
    let output = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "100",
            "--severity",
            "critical",
            "--title",
            "second",
            "--rule",
            "my-rule",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse");
    assert_eq!(
        json["status"], "created",
        "distant line should create new finding (not dedup or related)"
    );

    // Verify we have 2 findings total
    let findings: Vec<serde_json::Value> = serde_json::from_slice(
        &tally()
            .args(["query", "--format", "json"])
            .current_dir(dir)
            .output()
            .expect("query")
            .stdout,
    )
    .expect("parse");
    assert_eq!(findings.len(), 2, "should have 2 separate findings");
}

// =============================================================================
// Batch tests
// =============================================================================

#[test]
fn cli_record_batch_from_file() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Create a JSONL batch file
    let batch = tmp.path().join("batch.jsonl");
    std::fs::write(
        &batch,
        r#"{"file_path":"a.rs","line_start":1,"severity":"critical","title":"finding A","rule_id":"rule-a"}
{"file_path":"b.rs","line_start":2,"severity":"suggestion","title":"finding B","rule_id":"rule-b"}
{"file_path":"c.rs","line_start":3,"severity":"important","title":"finding C","rule_id":"rule-c"}
"#,
    )
    .expect("write batch");

    tally()
        .args(["record-batch", batch.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"succeeded\": 3"))
        .stdout(predicate::str::contains("\"failed\": 0"));
}

#[test]
fn cli_record_batch_partial_success() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let batch = tmp.path().join("batch.jsonl");
    std::fs::write(
        &batch,
        r#"{"file_path":"a.rs","line_start":1,"severity":"critical","title":"ok","rule_id":"r"}
{"bad json line
{"file_path":"c.rs","line_start":3,"severity":"important","title":"ok","rule_id":"r2"}
"#,
    )
    .expect("write batch");

    tally()
        .args(["record-batch", batch.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"succeeded\": 2"))
        .stdout(predicate::str::contains("\"failed\": 1"));
}

#[test]
fn cli_record_batch_empty_file() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let batch = tmp.path().join("empty.jsonl");
    std::fs::write(&batch, "").expect("write empty");

    tally()
        .args(["record-batch", batch.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"total\": 0"));
}

#[test]
fn cli_record_batch_dedup_in_batch() {
    // Covers handler line 909 (ExistingFinding branch in process_batch_line)
    // The batch resolver is built from existing findings before processing,
    // so we pre-record the finding, then batch-import a duplicate.
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Pre-record a finding so it exists before batch runs
    tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "42",
            "--severity",
            "critical",
            "--title",
            "pre-existing finding",
            "--rule",
            "dup-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Create a JSONL file with the same finding (same file/line/rule -> same fingerprint)
    let batch_file = tmp.path().join("batch.jsonl");
    std::fs::write(
        &batch_file,
        r#"{"file_path":"src/main.rs","line_start":42,"severity":"critical","title":"dup finding","rule_id":"dup-rule"}
"#,
    )
    .expect("write batch file");

    let output = tally()
        .args(["record-batch", batch_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse batch output");

    assert_eq!(json["total"], 1, "should process 1 entry");
    assert_eq!(json["succeeded"], 1, "should succeed");

    // The entry should be deduplicated since it matches the pre-existing finding
    let results = json["results"].as_array().expect("results array");
    let status = results[0]["result"]["status"]
        .as_str()
        .expect("status string");
    assert_eq!(
        status, "deduplicated",
        "batch entry matching existing finding should be deduplicated"
    );
}

// =============================================================================
// Location tests
// =============================================================================

#[test]
fn cli_record_with_additional_location() {
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
            "src/spec.md",
            "--line",
            "42",
            "--severity",
            "important",
            "--title",
            "spec-code mismatch",
            "--rule",
            "spec-drift",
            "--location",
            "src/code.rs:100:secondary",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"created\""));

    // Query back and verify both locations present
    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("src/spec.md"),
        "should have primary location"
    );
    assert!(
        stdout.contains("src/code.rs"),
        "should have secondary location"
    );
}

#[test]
fn cli_record_with_multiple_locations() {
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
            "src/api.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "cross-file issue",
            "--rule",
            "consistency",
            "--location",
            "src/types.rs:20:30:secondary",
            "--location",
            "docs/spec.md:5:context",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("src/api.rs"), "primary location");
    assert!(stdout.contains("src/types.rs"), "secondary location");
    assert!(stdout.contains("docs/spec.md"), "context location");
}

#[test]
fn cli_parse_location_flag_3_part() {
    // --location "file.rs:42:secondary" (3-part format)
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
            "important",
            "--title",
            "loc3 test",
            "--rule",
            "loc-rule",
            "--location",
            "file.rs:42:secondary",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("file.rs"), "should have secondary location");
    assert!(
        stdout.contains("\"line_start\": 42"),
        "3-part location should set line_start to 42: {stdout}"
    );
}

#[test]
fn cli_parse_location_flag_4_part() {
    // --location "file.rs:10:20:context" (4-part format)
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
            "important",
            "--title",
            "loc4 test",
            "--rule",
            "loc-rule",
            "--location",
            "file.rs:10:20:context",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("file.rs"), "should have context location");
    assert!(
        stdout.contains("\"line_start\": 10"),
        "4-part location should set line_start to 10: {stdout}"
    );
    assert!(
        stdout.contains("\"line_end\": 20"),
        "4-part location should set line_end to 20: {stdout}"
    );
}

#[test]
fn cli_parse_location_flag_invalid_line() {
    // --location "file.rs:abc:primary" -> should fail
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
            "important",
            "--title",
            "t",
            "--rule",
            "r",
            "--location",
            "file.rs:abc:primary",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid line number"));
}

#[test]
fn cli_parse_location_flag_invalid_role() {
    // --location "file.rs:42:invalid" -> should fail
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
            "important",
            "--title",
            "t",
            "--rule",
            "r",
            "--location",
            "file.rs:42:invalid",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid location role"));
}

#[test]
fn cli_parse_location_flag_wrong_part_count() {
    // --location "file.rs" -> only 1 part, should fail
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
            "important",
            "--title",
            "t",
            "--rule",
            "r",
            "--location",
            "file.rs",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid location format"));
}

// =============================================================================
// Relationship tests
// =============================================================================

#[test]
fn cli_record_with_relationship() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record first finding
    let output = tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "finding A",
            "--rule",
            "rule-a",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse");
    let uuid_a = json["uuid"].as_str().expect("uuid");

    // Record second finding related to first
    tally()
        .args([
            "record",
            "--file",
            "src/b.rs",
            "--line",
            "2",
            "--severity",
            "important",
            "--title",
            "finding B",
            "--rule",
            "rule-b",
            "--related-to",
            uuid_a,
            "--relationship",
            "discovered_while_fixing",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"created\""));

    // Query and verify relationship exists
    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("discovered_while_fixing"),
        "should have relationship type in output"
    );
}

#[test]
fn cli_all_relationship_types_parse() {
    // Verify all 6 relationship types parse correctly
    for rel_type in [
        "duplicate_of",
        "blocks",
        "related_to",
        "causes",
        "discovered_while_fixing",
        "supersedes",
    ] {
        let result: Result<tally_ng::model::RelationshipType, _> = rel_type.parse();
        assert!(result.is_ok(), "should parse relationship type: {rel_type}");
    }
}

#[test]
fn cli_related_finding_proximity_match() {
    // Two findings with same rule + same file, within 5 lines -> RelatedFinding branch
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record first finding at line 10
    tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "finding one",
            "--rule",
            "same-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record second finding with same rule+file but at line 13 (within 5 lines)
    // Different line -> different fingerprint -> triggers RelatedFinding branch
    let output = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "13",
            "--severity",
            "critical",
            "--title",
            "finding two nearby",
            "--rule",
            "same-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"related_to\""),
        "proximity match should produce related_to in output: {stdout}"
    );
    assert!(
        stdout.contains("\"distance\""),
        "proximity match should produce distance in output: {stdout}"
    );
}

#[test]
fn cli_related_finding_with_explicit_related_to() {
    // Record finding A, then record finding B near A AND with --related-to A
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record finding A at line 10
    let output_a = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "10",
            "--severity",
            "critical",
            "--title",
            "finding A",
            "--rule",
            "prox-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_a.stdout)).expect("parse");
    let uuid_a = json_a["uuid"].as_str().expect("uuid");

    // Record finding B near A (line 12, within 5) AND with --related-to A
    let output_b = tally()
        .args([
            "record",
            "--file",
            "src/main.rs",
            "--line",
            "12",
            "--severity",
            "important",
            "--title",
            "finding B with both auto and explicit",
            "--rule",
            "prox-rule",
            "--related-to",
            uuid_a,
            "--relationship",
            "blocks",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let stdout_b = String::from_utf8_lossy(&output_b.stdout);
    assert!(
        stdout_b.contains("\"related_to\""),
        "should have auto proximity relationship: {stdout_b}"
    );

    // Query and verify both relationship types present on finding B
    let query_out = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_out.stdout);
    assert!(
        stdout.contains("blocks"),
        "should have explicit 'blocks' relationship: {stdout}"
    );
    assert!(
        stdout.contains("related_to"),
        "should have auto 'related_to' relationship: {stdout}"
    );
}

#[test]
fn cli_check_expiry_and_reopen() {
    // Suppress a finding with a past expiry date, then query -> should auto-reopen
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
            "expires soon",
            "--rule",
            "expiry-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid");

    // Suppress with a date in the past
    tally()
        .args([
            "suppress",
            uuid,
            "--reason",
            "temporary",
            "--expires",
            "2020-01-01T00:00:00Z",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query -> suppression should be expired, finding auto-reopened to Open
    let query_out = tally()
        .args(["query", "--format", "json", "--status", "open"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_out.stdout);
    assert!(
        stdout.contains("expires soon"),
        "expired suppression should reopen to open: {stdout}"
    );
    assert!(
        stdout.contains("Suppression expired"),
        "state_history should contain system auto-reopen reason: {stdout}"
    );
}
