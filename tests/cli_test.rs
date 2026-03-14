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
