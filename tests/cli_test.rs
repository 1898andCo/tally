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
    builder
        .insert("README.md", blob, 0o100_644)
        .expect("insert");
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
fn cli_mcp_capabilities_lists_all() {
    // No repo needed — capabilities are reflected from the server struct
    tally()
        .arg("mcp-capabilities")
        .assert()
        .success()
        // Tools — dynamically reflected
        .stdout(predicate::str::contains("Tools (6)"))
        .stdout(predicate::str::contains("record_finding"))
        .stdout(predicate::str::contains("query_findings"))
        .stdout(predicate::str::contains("update_finding_status"))
        .stdout(predicate::str::contains("suppress_finding"))
        .stdout(predicate::str::contains("record_batch"))
        .stdout(predicate::str::contains("get_finding_context"))
        // Prompts — dynamically reflected
        .stdout(predicate::str::contains("Prompts (5)"))
        .stdout(predicate::str::contains("triage-file"))
        .stdout(predicate::str::contains("fix-finding"))
        .stdout(predicate::str::contains("summarize-findings"))
        .stdout(predicate::str::contains("review-pr"))
        .stdout(predicate::str::contains("explain-finding"))
        // Resources
        .stdout(predicate::str::contains("Resources (6)"))
        .stdout(predicate::str::contains("findings://summary"))
        // Config example
        .stdout(predicate::str::contains("mcp-server"));
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
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();
}

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
fn cli_query_json_returns_array() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record a finding
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
            "sql injection",
            "--rule",
            "sql-injection",
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
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

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
            "test finding",
            "--rule",
            "test-rule",
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
            "r",
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
            "s",
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
fn cli_query_json_includes_short_id() {
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
            "critical",
            "--title",
            "test",
            "--rule",
            "test-rule",
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
            "test",
            "--rule",
            "test-rule",
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
fn cli_export_sarif() {
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
            "critical",
            "--title",
            "test",
            "--rule",
            "test-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"version\": \"2.1.0\""))
        .stdout(predicate::str::contains("\"name\": \"tally\""))
        .stdout(predicate::str::contains("test-rule"));
}

#[test]
fn cli_export_csv() {
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
            "csv test",
            "--rule",
            "csv-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["export", "--format", "csv"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("uuid,severity,status"))
        .stdout(predicate::str::contains("csv-rule"));
}

#[test]
fn cli_export_to_file() {
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
            "suggestion",
            "--title",
            "t",
            "--rule",
            "r",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output_file = tmp.path().join("findings.json");
    tally()
        .args([
            "export",
            "--format",
            "json",
            "--output",
            output_file.to_str().expect("p"),
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let content = std::fs::read_to_string(&output_file).expect("read");
    assert!(
        content.contains("suggestion"),
        "exported file should contain findings"
    );
}

// --- Negative ---

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
fn cli_import_dclaude_format() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Create a mock dclaude state file
    let state_file = tmp.path().join("pr-review.json");
    std::fs::write(
        &state_file,
        r#"{
  "active_cycle": {
    "findings": [
      {"id": "C1", "severity": "critical", "title": "SQL injection", "file": "src/api.rs", "lines": [42], "category": "injection", "status": "pending"},
      {"id": "I1", "severity": "important", "title": "Missing auth", "file": "src/routes.rs", "lines": [10], "category": "auth", "status": "verified"}
    ]
  }
}"#,
    )
    .expect("write state file");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("2 imported"));

    // Verify findings were imported
    tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("SQL injection"))
        .stdout(predicate::str::contains("Missing auth"));
}

#[test]
fn cli_import_zclaude_format() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let state_file = tmp.path().join("zclaude-review.json");
    std::fs::write(
        &state_file,
        r#"{
  "reviews": [
    {
      "findings": [
        {"id": "S1", "severity": "suggestion", "title": "Missing test", "file": "src/lib.rs", "lines": [5], "status": "pending"}
      ]
    }
  ]
}"#,
    )
    .expect("write");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("1 imported"));
}

#[test]
fn cli_import_empty_file_no_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let state_file = tmp.path().join("empty.json");
    std::fs::write(&state_file, "{}").expect("write");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("No findings found"));
}

#[test]
fn cli_sync_before_init_fails() {
    let tmp = setup_cli_repo();
    // No init
    tally()
        .args(["sync"])
        .current_dir(tmp.path())
        .assert()
        .failure();
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

// =============================================================================
// AC-13: Multi-file findings
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
fn cli_sarif_export_includes_multiple_locations() {
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
            "important",
            "--title",
            "multi-loc",
            "--rule",
            "test-rule",
            "--location",
            "src/other.rs:20:secondary",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("src/main.rs"), "SARIF should have primary");
    assert!(
        stdout.contains("src/other.rs"),
        "SARIF should have secondary"
    );
}

// =============================================================================
// AC-14: Finding relationships
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
        let result: Result<tally::model::RelationshipType, _> = rel_type.parse();
        assert!(result.is_ok(), "should parse relationship type: {rel_type}");
    }
}

// =============================================================================
// Record with new fields
// =============================================================================

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

// =============================================================================
// Query tests
// =============================================================================

#[test]
fn cli_query_combined_filters() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record 3 findings with different severity/file combos
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
            "critical api issue",
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
            "src/lib.rs",
            "--line",
            "20",
            "--severity",
            "critical",
            "--title",
            "critical lib issue",
            "--rule",
            "r2",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args([
            "record",
            "--file",
            "src/api.rs",
            "--line",
            "30",
            "--severity",
            "suggestion",
            "--title",
            "suggestion api issue",
            "--rule",
            "r3",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query with --severity critical --file "src/api" -> should get only "critical api issue"
    let output = tally()
        .args([
            "query",
            "--format",
            "json",
            "--severity",
            "critical",
            "--file",
            "src/api",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("critical api issue"),
        "should include critical api finding: {stdout}"
    );
    assert!(
        !stdout.contains("critical lib issue"),
        "should exclude lib finding: {stdout}"
    );
    assert!(
        !stdout.contains("suggestion api issue"),
        "should exclude suggestion finding: {stdout}"
    );
}

#[test]
fn cli_query_related_to() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record first finding
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
            "finding A",
            "--rule",
            "rule-a",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_a.stdout)).expect("parse");
    let uuid_a = json_a["uuid"].as_str().expect("uuid");

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
            "finding B related",
            "--rule",
            "rule-b",
            "--related-to",
            uuid_a,
            "--relationship",
            "blocks",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query --related-to uuid_a should return finding B
    let output = tally()
        .args(["query", "--format", "json", "--related-to", uuid_a])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("finding B related"),
        "should find related finding: {stdout}"
    );
}

#[test]
fn cli_stats_empty_store() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .arg("stats")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Total:       0"));
}

// =============================================================================
// Update tests
// =============================================================================

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
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Index rebuilt"));
}

// =============================================================================
// Coverage tests — uncovered handler paths
// =============================================================================

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
fn cli_query_related_to_filter_returns_only_related() {
    // Create 3 findings, relate B to A, query --related-to A -> only B returned
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record A
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
            "finding A target",
            "--rule",
            "rule-a",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_a.stdout)).expect("parse");
    let uuid_a = json_a["uuid"].as_str().expect("uuid");

    // Record B related to A
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
            "finding B linked to A",
            "--rule",
            "rule-b",
            "--related-to",
            uuid_a,
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record C (unrelated)
    tally()
        .args([
            "record",
            "--file",
            "src/c.rs",
            "--line",
            "3",
            "--severity",
            "suggestion",
            "--title",
            "finding C unrelated",
            "--rule",
            "rule-c",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query --related-to A
    let output = tally()
        .args(["query", "--format", "json", "--related-to", uuid_a])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("finding B linked to A"),
        "should return B: {stdout}"
    );
    assert!(
        !stdout.contains("finding C unrelated"),
        "should NOT return C: {stdout}"
    );
    assert!(
        !stdout.contains("finding A target"),
        "should NOT return A itself: {stdout}"
    );
}

#[test]
fn cli_query_summary_format() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record findings of different severities
    for (sev, title) in [
        ("critical", "crit1"),
        ("critical", "crit2"),
        ("important", "imp1"),
        ("suggestion", "sug1"),
    ] {
        tally()
            .args([
                "record",
                "--file",
                "src/main.rs",
                "--line",
                "1",
                "--severity",
                sev,
                "--title",
                title,
                "--rule",
                &format!("rule-{title}"),
            ])
            .current_dir(tmp.path())
            .assert()
            .success();
    }

    let output = tally()
        .args(["query", "--format", "summary"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Query Results: 4 findings"),
        "should contain total count: {stdout}"
    );
    assert!(
        stdout.contains("CRITICAL: 2"),
        "should contain critical count: {stdout}"
    );
    assert!(
        stdout.contains("IMPORTANT: 1"),
        "should contain important count: {stdout}"
    );
    assert!(
        stdout.contains("SUGGESTION: 1"),
        "should contain suggestion count: {stdout}"
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
fn cli_import_severity_inferred_from_id_prefix() {
    // Import dclaude file where entries have NO "severity" field but have id prefixes
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let state_file = tmp.path().join("infer-severity.json");
    std::fs::write(
        &state_file,
        r#"{
  "active_cycle": {
    "findings": [
      {"id": "C1", "title": "Critical from prefix", "file": "src/a.rs", "lines": [1], "status": "pending"},
      {"id": "I1", "title": "Important from prefix", "file": "src/b.rs", "lines": [2], "status": "pending"},
      {"id": "TD1", "title": "Suggestion from TD prefix", "file": "src/c.rs", "lines": [3], "status": "pending"}
    ]
  }
}"#,
    )
    .expect("write");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("3 imported"));

    // Query and verify inferred severities
    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("parse");
    let severities: Vec<&str> = findings
        .iter()
        .map(|f| f["severity"].as_str().expect("severity"))
        .collect();
    assert!(
        severities.contains(&"critical"),
        "C1 prefix should infer critical: {severities:?}"
    );
    assert!(
        severities.contains(&"important"),
        "I1 prefix should infer important: {severities:?}"
    );
    assert!(
        severities.contains(&"suggestion"),
        "TD1 prefix should default to suggestion: {severities:?}"
    );
}

#[test]
fn cli_import_tolerates_non_object_entries() {
    // Import file with a non-object entry in findings array — import_single_finding
    // uses default values for missing fields, so even non-objects import with defaults.
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let state_file = tmp.path().join("partial-import.json");
    std::fs::write(
        &state_file,
        r#"{
  "active_cycle": {
    "findings": [
      {"id": "C1", "severity": "critical", "title": "good finding", "file": "src/a.rs", "lines": [1], "status": "pending"},
      "not-an-object",
      {"id": "I1", "severity": "important", "title": "another good", "file": "src/b.rs", "lines": [2], "status": "pending"}
    ]
  }
}"#,
    )
    .expect("write");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("3 imported"));
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
// Coverage tests — targeted uncovered lines
// =============================================================================

#[test]
fn cli_query_filter_by_rule() {
    // Covers handler line 149 (rule filter branch)
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record 2 findings with different rules
    tally()
        .args([
            "record",
            "--file",
            "src/a.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "finding with rule-a",
            "--rule",
            "rule-a",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

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
            "finding with rule-b",
            "--rule",
            "rule-b",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query with --rule rule-a -> should only return the first finding
    let output = tally()
        .args(["query", "--format", "json", "--rule", "rule-a"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("finding with rule-a"),
        "should include rule-a finding: {stdout}"
    );
    assert!(
        !stdout.contains("finding with rule-b"),
        "should exclude rule-b finding: {stdout}"
    );
}

#[test]
fn cli_query_table_empty() {
    // Covers handler line 812 ("No findings." in print_table)
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--format", "table"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No findings."));
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

#[test]
fn cli_import_tech_debt_severity() {
    // Covers handler line 1067 (tech_debt severity mapping in import)
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let state_file = tmp.path().join("tech-debt.json");
    std::fs::write(
        &state_file,
        r#"{
  "active_cycle": {
    "findings": [
      {"id": "TD1", "severity": "tech_debt", "title": "Tech debt finding", "file": "src/main.rs", "lines": [10], "status": "pending"}
    ]
  }
}"#,
    )
    .expect("write");

    tally()
        .args(["import", state_file.to_str().expect("path")])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("1 imported"));

    // Verify severity is tech_debt
    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tech_debt"),
        "should have tech_debt severity: {stdout}"
    );
}

#[test]
fn cli_query_filter_by_related_to_uuid() {
    // More direct coverage of handler lines 151-158 (related_to filter with UUID parsing)
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record finding A
    let output_a = tally()
        .args([
            "record",
            "--file",
            "src/x.rs",
            "--line",
            "1",
            "--severity",
            "critical",
            "--title",
            "target finding X",
            "--rule",
            "rule-x",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");
    let json_a: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output_a.stdout)).expect("parse");
    let uuid_a = json_a["uuid"].as_str().expect("uuid");

    // Record finding B with --related-to A
    tally()
        .args([
            "record",
            "--file",
            "src/y.rs",
            "--line",
            "2",
            "--severity",
            "important",
            "--title",
            "related finding Y",
            "--rule",
            "rule-y",
            "--related-to",
            uuid_a,
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record finding C (unrelated)
    tally()
        .args([
            "record",
            "--file",
            "src/z.rs",
            "--line",
            "3",
            "--severity",
            "suggestion",
            "--title",
            "unrelated finding Z",
            "--rule",
            "rule-z",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query --related-to uuid_a -> should return only finding B
    let query_out = tally()
        .args(["query", "--format", "json", "--related-to", uuid_a])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&query_out.stdout);
    assert!(
        stdout.contains("related finding Y"),
        "should return related finding Y: {stdout}"
    );
    assert!(
        !stdout.contains("unrelated finding Z"),
        "should NOT return unrelated finding Z: {stdout}"
    );
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
