//! End-to-end lifecycle tests — full user workflows via CLI binary.
//!
//! Each test walks through a complete scenario that mirrors real usage,
//! exercising multiple commands in sequence and verifying the final state.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn setup_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = git2::Repository::init(tmp.path()).expect("init");
    let sig = git2::Signature::now("test", "test@test.com").expect("sig");
    let blob = repo.blob(b"# test").expect("blob");
    let mut builder = repo.treebuilder(None).expect("tb");
    builder
        .insert("README.md", blob, 0o100_644)
        .expect("insert");
    let tree_oid = builder.write().expect("write");
    let tree = repo.find_tree(tree_oid).expect("tree");
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .expect("commit");
    tmp
}

fn tally() -> Command {
    Command::cargo_bin("tally").expect("tally binary")
}

fn run_record(
    dir: &std::path::Path,
    file: &str,
    line: u32,
    severity: &str,
    title: &str,
    rule: &str,
) -> Value {
    let output = tally()
        .args([
            "record",
            "--file",
            file,
            "--line",
            &line.to_string(),
            "--severity",
            severity,
            "--title",
            title,
            "--rule",
            rule,
        ])
        .current_dir(dir)
        .output()
        .expect("run record");
    assert!(
        output.status.success(),
        "record failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse record output")
}

fn run_query_json(dir: &std::path::Path, args: &[&str]) -> Vec<Value> {
    let mut cmd_args = vec!["query", "--format", "json"];
    cmd_args.extend_from_slice(args);
    let output = tally()
        .args(&cmd_args)
        .current_dir(dir)
        .output()
        .expect("run query");
    assert!(output.status.success(), "query failed");
    serde_json::from_slice(&output.stdout).expect("parse query output")
}

fn get_uuid(json: &Value) -> String {
    json["uuid"].as_str().expect("uuid field").to_string()
}

// =============================================================================
// E2E 1: Full finding lifecycle — Open → Acknowledged → InProgress → Resolved → Closed
// =============================================================================

#[test]
fn e2e_full_finding_lifecycle() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // 1. Record a finding — starts as Open
    let record = run_record(
        dir,
        "src/main.rs",
        42,
        "critical",
        "SQL injection risk",
        "sql-injection",
    );
    assert_eq!(record["status"], "created");
    let uuid = get_uuid(&record);

    // 2. Verify it's Open
    let findings = run_query_json(dir, &["--status", "open"]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["uuid"], uuid);
    assert_eq!(findings[0]["status"], "open");

    // 3. Acknowledge → adds to state history
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "acknowledged",
            "--reason",
            "reviewing",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // 4. Start working → InProgress
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "in_progress",
            "--reason",
            "fixing now",
            "--agent",
            "developer",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // 5. Resolve with commit reference
    tally()
        .args([
            "update", &uuid, "--status", "resolved", "--reason", "fixed", "--commit", "abc123",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // 6. Close
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "closed",
            "--reason",
            "verified in prod",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // 7. Verify final state — closed with full history
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 1);
    let f = &findings[0];
    assert_eq!(f["status"], "closed");

    // 8. Verify state history has all 4 transitions
    let history = f["state_history"].as_array().expect("state_history");
    assert_eq!(history.len(), 4, "should have 4 transitions");
    assert_eq!(history[0]["from"], "open");
    assert_eq!(history[0]["to"], "acknowledged");
    assert_eq!(history[1]["from"], "acknowledged");
    assert_eq!(history[1]["to"], "in_progress");
    assert_eq!(history[2]["from"], "in_progress");
    assert_eq!(history[2]["to"], "resolved");
    assert_eq!(history[2]["commit_sha"], "abc123");
    assert_eq!(history[3]["from"], "resolved");
    assert_eq!(history[3]["to"], "closed");

    // 9. Verify closed is terminal — can't update further
    tally()
        .args(["update", &uuid, "--status", "open"])
        .current_dir(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid state transition"));
}

// =============================================================================
// E2E 2: Multi-agent dedup — two agents discover same issue
// =============================================================================

#[test]
fn e2e_multi_agent_dedup() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Agent 1 (Claude Code) discovers an issue
    let record1 = run_record(
        dir,
        "src/api.rs",
        15,
        "important",
        "missing auth check",
        "missing-auth",
    );
    assert_eq!(record1["status"], "created");
    let uuid = get_uuid(&record1);

    // Agent 2 (Cursor) discovers the SAME issue — same file, line, rule
    let output = tally()
        .args([
            "record",
            "--file",
            "src/api.rs",
            "--line",
            "15",
            "--severity",
            "important",
            "--title",
            "missing auth check",
            "--rule",
            "missing-auth",
            "--agent",
            "cursor",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let record2: Value = serde_json::from_slice(&output.stdout).expect("parse");
    assert_eq!(
        record2["status"], "deduplicated",
        "second agent should dedup"
    );
    assert_eq!(record2["uuid"], uuid, "should return same UUID");

    // Verify discovered_by has both agents
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 1, "should be only 1 finding");
    let discovered_by = findings[0]["discovered_by"]
        .as_array()
        .expect("discovered_by");
    assert_eq!(discovered_by.len(), 2, "two agents should be recorded");

    let agents: Vec<&str> = discovered_by
        .iter()
        .map(|a| a["agent_id"].as_str().expect("agent_id"))
        .collect();
    assert!(agents.contains(&"cli"), "first agent");
    assert!(agents.contains(&"cursor"), "second agent");
}

// =============================================================================
// E2E 3: Suppression with expiry auto-reopen
// =============================================================================

#[test]
fn e2e_suppression_lifecycle() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record a finding
    let record = run_record(
        dir,
        "src/config.rs",
        10,
        "suggestion",
        "hardcoded timeout",
        "magic-number",
    );
    let uuid = get_uuid(&record);

    // Suppress with past expiry — should auto-reopen on next query
    tally()
        .args([
            "suppress",
            &uuid,
            "--reason",
            "accepted for now",
            "--expires",
            "2020-01-01T00:00:00Z",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Query — the finding should have auto-reopened (expiry in the past)
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0]["status"], "open",
        "expired suppression should auto-reopen"
    );

    // State history should show: Open → Suppressed, then Suppressed → Open (system)
    let history = findings[0]["state_history"].as_array().expect("history");
    assert!(history.len() >= 2, "should have at least 2 transitions");
    let last = history.last().expect("last");
    assert_eq!(last["from"], "suppressed");
    assert_eq!(last["to"], "open");
    assert_eq!(last["agent_id"], "system");
    assert_eq!(last["reason"], "Suppression expired");

    // Suppression field should be cleared
    assert!(
        findings[0]["suppression"].is_null(),
        "suppression should be cleared after expiry"
    );

    // Now suppress permanently (no expiry)
    tally()
        .args(["suppress", &uuid, "--reason", "permanently accepted"])
        .current_dir(dir)
        .assert()
        .success();

    // Verify it stays suppressed (no expiry → never auto-reopens)
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings[0]["status"], "suppressed");
}

// =============================================================================
// E2E 4: Batch record → query → export SARIF
// =============================================================================

#[test]
fn e2e_batch_to_sarif_export() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Batch record multiple findings
    let batch = dir.join("findings.jsonl");
    std::fs::write(
        &batch,
        r#"{"file_path":"src/auth.rs","line_start":10,"severity":"critical","title":"SQL injection in login","rule_id":"sql-injection"}
{"file_path":"src/api.rs","line_start":25,"severity":"important","title":"Missing rate limit","rule_id":"missing-rate-limit"}
{"file_path":"src/utils.rs","line_start":5,"severity":"suggestion","title":"Consider using const","rule_id":"use-const"}
{"file_path":"src/db.rs","line_start":100,"severity":"tech_debt","title":"Legacy query builder","rule_id":"legacy-code"}
"#,
    )
    .expect("write batch");

    tally()
        .args(["record-batch", batch.to_str().expect("p")])
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"succeeded\": 4"));

    // Verify all 4 findings exist
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 4);

    // Query by severity
    let critical = run_query_json(dir, &["--severity", "critical"]);
    assert_eq!(critical.len(), 1);
    assert!(
        critical[0]["title"]
            .as_str()
            .expect("t")
            .contains("SQL injection")
    );

    // Export to SARIF
    let sarif_path = dir.join("findings.sarif");
    tally()
        .args([
            "export",
            "--format",
            "sarif",
            "--output",
            sarif_path.to_str().expect("p"),
        ])
        .current_dir(dir)
        .assert()
        .success();

    let sarif: Value =
        serde_json::from_str(&std::fs::read_to_string(&sarif_path).expect("read sarif"))
            .expect("parse sarif");

    assert_eq!(sarif["version"], "2.1.0");
    let results = sarif["runs"][0]["results"].as_array().expect("results");
    assert_eq!(results.len(), 4, "SARIF should have 4 results");

    // Verify SARIF levels match severity
    let levels: Vec<&str> = results
        .iter()
        .map(|r| r["level"].as_str().expect("level"))
        .collect();
    assert!(levels.contains(&"error"), "critical → error");
    assert!(levels.contains(&"warning"), "important → warning");
    assert!(levels.contains(&"note"), "suggestion → note");
    assert!(levels.contains(&"none"), "tech_debt → none");

    // Export to CSV
    tally()
        .args(["export", "--format", "csv"])
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("uuid,severity,status"))
        .stdout(predicate::str::contains("sql-injection"));

    // Stats should show all 4
    tally()
        .arg("stats")
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Total:       4"));
}

// =============================================================================
// E2E 5: Short IDs across commands
// =============================================================================

#[test]
fn e2e_short_ids_workflow() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record findings of different severities
    run_record(dir, "a.rs", 1, "critical", "crit-1", "r1");
    run_record(dir, "b.rs", 2, "important", "imp-1", "r2");
    run_record(dir, "c.rs", 3, "suggestion", "sug-1", "r3");
    run_record(dir, "d.rs", 4, "tech_debt", "td-1", "r4");

    // Query and verify short IDs in JSON output
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 4);

    let short_ids: Vec<&str> = findings
        .iter()
        .map(|f| f["short_id"].as_str().expect("short_id"))
        .collect();
    assert!(short_ids.contains(&"C1"), "critical → C1");
    assert!(short_ids.contains(&"I1"), "important → I1");
    assert!(short_ids.contains(&"S1"), "suggestion → S1");
    assert!(short_ids.contains(&"TD1"), "tech_debt → TD1");

    // Update using short ID
    tally()
        .args([
            "update",
            "C1",
            "--status",
            "acknowledged",
            "--reason",
            "via short ID",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Suppress using short ID
    tally()
        .args(["suppress", "I1", "--reason", "accepted risk"])
        .current_dir(dir)
        .assert()
        .success();

    // Verify the updates took effect
    let findings = run_query_json(dir, &["--status", "acknowledged"]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0]["title"].as_str().expect("t").contains("crit-1"));

    let findings = run_query_json(dir, &["--status", "suppressed"]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0]["title"].as_str().expect("t").contains("imp-1"));
}

// =============================================================================
// E2E 6: Multi-file findings with relationships and SARIF export
// =============================================================================

#[test]
fn e2e_multi_file_with_relationships() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record a spec-code mismatch with multiple locations
    let output = tally()
        .args([
            "record",
            "--file",
            "docs/api-spec.md",
            "--line",
            "42",
            "--severity",
            "critical",
            "--title",
            "API spec says POST, code implements GET",
            "--rule",
            "spec-drift",
            "--location",
            "src/routes.rs:100:secondary",
            "--location",
            "tests/api_test.rs:50:context",
            "--category",
            "spec-compliance",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let finding_a: Value = serde_json::from_slice(&output.stdout).expect("parse");
    let uuid_a = get_uuid(&finding_a);

    // Record a related finding discovered while investigating the first
    let output = tally()
        .args([
            "record",
            "--file",
            "src/routes.rs",
            "--line",
            "105",
            "--severity",
            "important",
            "--title",
            "Missing validation on GET endpoint",
            "--rule",
            "missing-validation",
            "--related-to",
            &uuid_a,
            "--relationship",
            "discovered_while_fixing",
        ])
        .current_dir(dir)
        .output()
        .expect("run");
    let finding_b: Value = serde_json::from_slice(&output.stdout).expect("parse");
    let uuid_b = get_uuid(&finding_b);

    // Query all — should see 2 findings
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 2);

    // Find finding A and verify 3 locations
    let a = findings
        .iter()
        .find(|f| f["uuid"] == uuid_a)
        .expect("find A");
    let locations = a["locations"].as_array().expect("locations");
    assert_eq!(locations.len(), 3, "A should have 3 locations");
    assert_eq!(locations[0]["role"], "primary");
    assert_eq!(locations[1]["role"], "secondary");
    assert_eq!(locations[2]["role"], "context");

    // Find finding B and verify relationship
    let b = findings
        .iter()
        .find(|f| f["uuid"] == uuid_b)
        .expect("find B");
    let rels = b["relationships"].as_array().expect("relationships");
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["related_finding_id"], uuid_a);
    assert_eq!(rels[0]["relationship_type"], "discovered_while_fixing");

    // Query related to A
    let related = run_query_json(dir, &["--related-to", &uuid_a]);
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["uuid"], uuid_b);

    // SARIF export should include multi-location
    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(dir)
        .output()
        .expect("export");
    let sarif: Value = serde_json::from_slice(&output.stdout).expect("parse sarif");
    let results = sarif["runs"][0]["results"].as_array().expect("results");

    // Find the spec-drift result (has 3 locations)
    let spec_drift = results
        .iter()
        .find(|r| r["ruleId"] == "spec-drift")
        .expect("find spec-drift result");
    let sarif_locations = spec_drift["locations"].as_array().expect("locations");
    assert_eq!(
        sarif_locations.len(),
        3,
        "SARIF should export all 3 locations"
    );
}

// =============================================================================
// E2E 7: Import dclaude → triage → export workflow
// =============================================================================

#[test]
fn e2e_import_triage_export() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Simulate a dclaude PR review state file
    let state = dir.join("pr-review.json");
    std::fs::write(
        &state,
        r#"{
  "active_cycle": {
    "findings": [
      {"id": "C1", "severity": "critical", "title": "SQL injection in user input", "file": "src/handler.rs", "lines": [42], "category": "injection", "status": "pending"},
      {"id": "I1", "severity": "important", "title": "Missing error handling", "file": "src/db.rs", "lines": [100], "category": "error-handling", "status": "pending"},
      {"id": "S1", "severity": "suggestion", "title": "Consider using Iterator", "file": "src/utils.rs", "lines": [10], "category": "style", "status": "verified"},
      {"id": "I2", "severity": "important", "title": "Unvalidated config", "file": "src/config.rs", "lines": [5], "category": "validation", "status": "wont_fix"}
    ]
  }
}"#,
    )
    .expect("write state");

    // Import
    tally()
        .args(["import", state.to_str().expect("p")])
        .current_dir(dir)
        .assert()
        .success()
        .stderr(predicate::str::contains("4 imported"));

    // Verify imported findings
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 4);

    // Verify status mapping: pending→Open, verified→Resolved, wont_fix→WontFix
    let open = run_query_json(dir, &["--status", "open"]);
    assert_eq!(open.len(), 2, "2 pending → open");

    let resolved = run_query_json(dir, &["--status", "resolved"]);
    assert_eq!(resolved.len(), 1, "1 verified → resolved");

    let wont_fix = run_query_json(dir, &["--status", "wont_fix"]);
    assert_eq!(wont_fix.len(), 1, "1 wont_fix → wont_fix");

    // Triage: fix the critical, defer the important
    let critical = run_query_json(dir, &["--severity", "critical"]);
    let critical_uuid = critical[0]["uuid"].as_str().expect("uuid");
    tally()
        .args([
            "update",
            critical_uuid,
            "--status",
            "in_progress",
            "--agent",
            "developer",
        ])
        .current_dir(dir)
        .assert()
        .success();
    tally()
        .args([
            "update",
            critical_uuid,
            "--status",
            "resolved",
            "--commit",
            "fix123",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Export final state
    let output = tally()
        .args(["export", "--format", "json"])
        .current_dir(dir)
        .output()
        .expect("export");
    let exported: Vec<Value> = serde_json::from_slice(&output.stdout).expect("parse");
    assert_eq!(exported.len(), 4);

    // Verify the critical finding is resolved with commit
    let critical_final = exported
        .iter()
        .find(|f| f["uuid"] == critical_uuid)
        .expect("find");
    assert_eq!(critical_final["status"], "resolved");
    let history = critical_final["state_history"].as_array().expect("history");
    assert!(
        history.iter().any(|h| h["commit_sha"] == "fix123"),
        "history should contain commit SHA"
    );

    // Rebuild index and verify
    tally()
        .arg("rebuild-index")
        .current_dir(dir)
        .assert()
        .success();

    // Stats should reflect the triage
    tally()
        .arg("stats")
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Total:       4"));
}

// =============================================================================
// E2E 8: False positive → reopen → resolve lifecycle
// =============================================================================

#[test]
fn e2e_false_positive_reopen() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Record a finding
    let record = run_record(dir, "src/auth.rs", 30, "critical", "Potential XSS", "xss");
    let uuid = get_uuid(&record);

    // Mark as false positive
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "false_positive",
            "--reason",
            "sanitized input",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Verify it's false positive
    let findings = run_query_json(dir, &["--status", "false_positive"]);
    assert_eq!(findings.len(), 1);

    // Oops — actually it IS a real issue. Reopen it.
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "reopened",
            "--reason",
            "missed a code path",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Work on it and resolve properly
    tally()
        .args(["update", &uuid, "--status", "in_progress"])
        .current_dir(dir)
        .assert()
        .success();
    tally()
        .args([
            "update",
            &uuid,
            "--status",
            "resolved",
            "--commit",
            "real-fix-sha",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Verify full history: Open→FalsePositive→Reopened→InProgress→Resolved
    let findings = run_query_json(dir, &[]);
    let history = findings[0]["state_history"].as_array().expect("history");
    assert_eq!(history.len(), 4);
    let states: Vec<(&str, &str)> = history
        .iter()
        .map(|h| (h["from"].as_str().expect("f"), h["to"].as_str().expect("t")))
        .collect();
    assert_eq!(
        states,
        vec![
            ("open", "false_positive"),
            ("false_positive", "reopened"),
            ("reopened", "in_progress"),
            ("in_progress", "resolved"),
        ]
    );
}

// =============================================================================
// E2E 9: Cross-session persistence — findings survive across invocations
// =============================================================================

#[test]
#[allow(clippy::similar_names)]
fn e2e_cross_session_persistence() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // Session 1: Claude Code records 3 findings
    let uuid1 = get_uuid(&run_record(
        dir,
        "src/a.rs",
        10,
        "critical",
        "finding 1",
        "r1",
    ));
    let uuid2 = get_uuid(&run_record(
        dir,
        "src/b.rs",
        20,
        "important",
        "finding 2",
        "r2",
    ));
    let uuid3 = get_uuid(&run_record(
        dir,
        "src/c.rs",
        30,
        "suggestion",
        "finding 3",
        "r3",
    ));

    // Session 1: Update one
    tally()
        .args(["update", &uuid1, "--status", "in_progress"])
        .current_dir(dir)
        .assert()
        .success();

    // Session 2: Different agent queries — all 3 findings should be there
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 3, "all 3 findings persist across sessions");

    // Session 2: Verify UUIDs are stable
    let uuids: Vec<&str> = findings
        .iter()
        .map(|f| f["uuid"].as_str().expect("uuid"))
        .collect();
    assert!(uuids.contains(&uuid1.as_str()));
    assert!(uuids.contains(&uuid2.as_str()));
    assert!(uuids.contains(&uuid3.as_str()));

    // Session 2: Verify the update from session 1 persisted
    let f1 = findings
        .iter()
        .find(|f| f["uuid"] == uuid1)
        .expect("find uuid1");
    assert_eq!(
        f1["status"], "in_progress",
        "update from session 1 should persist"
    );

    // Session 2: Continue the work
    tally()
        .args([
            "update", &uuid1, "--status", "resolved", "--agent", "cursor",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // Verify the history shows both agents
    let findings = run_query_json(dir, &[]);
    let f1 = findings.iter().find(|f| f["uuid"] == uuid1).expect("find");
    let history = f1["state_history"].as_array().expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["agent_id"], "cli");
    assert_eq!(history[1]["agent_id"], "cursor");
}

// =============================================================================
// E2E 10: All output formats consistent
// =============================================================================

#[test]
fn e2e_output_format_consistency() {
    let tmp = setup_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    run_record(dir, "src/a.rs", 1, "critical", "Critical Bug", "r1");
    run_record(dir, "src/b.rs", 2, "important", "Important Issue", "r2");

    // JSON format — structured, machine-readable
    let json_output = tally()
        .args(["query", "--format", "json"])
        .current_dir(dir)
        .output()
        .expect("json query");
    let json: Vec<Value> = serde_json::from_slice(&json_output.stdout).expect("parse");
    assert_eq!(json.len(), 2);

    // Table format — human-readable with columns
    tally()
        .args(["query", "--format", "table"])
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("C1"))
        .stdout(predicate::str::contains("I1"))
        .stdout(predicate::str::contains("Critical Bug"))
        .stdout(predicate::str::contains("Important Issue"));

    // Summary format — counts only
    tally()
        .args(["query", "--format", "summary"])
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("2 findings"))
        .stdout(predicate::str::contains("CRITICAL: 1"))
        .stdout(predicate::str::contains("IMPORTANT: 1"));

    // SARIF — GitHub Code Scanning compatible
    let sarif_out = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(dir)
        .output()
        .expect("sarif");
    let sarif: Value = serde_json::from_slice(&sarif_out.stdout).expect("parse sarif");
    assert_eq!(sarif["version"], "2.1.0");

    // CSV — spreadsheet compatible
    tally()
        .args(["export", "--format", "csv"])
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("uuid,severity,status"));

    // Stats — dashboard view
    tally()
        .arg("stats")
        .current_dir(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Total:       2"));
}
