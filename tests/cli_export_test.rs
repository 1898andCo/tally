//! CLI integration tests -- export, sarif, import.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// =============================================================================
// Export tests
// =============================================================================

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
            "r1",
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

// =============================================================================
// SARIF tests
// =============================================================================

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

#[test]
fn cli_sarif_export_includes_tally_notes_in_properties() {
    let (tmp, uuid) = setup_with_finding();

    // Add a note and tag
    tally()
        .args(["note", &uuid, "SARIF test note"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["tag", &uuid, "--add", "sarif-test"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");
    let result = &sarif["runs"][0]["results"][0];

    assert!(
        result["properties"]["tally_notes"].is_array(),
        "SARIF should have tally_notes property"
    );
    let notes = result["properties"]["tally_notes"]
        .as_array()
        .expect("notes");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["text"], "SARIF test note");
    assert!(notes[0]["timestamp"].is_string());
}

#[test]
fn cli_sarif_export_includes_tally_edit_history() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["update-fields", &uuid, "--description", "edited desc"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");
    let result = &sarif["runs"][0]["results"][0];

    assert!(
        result["properties"]["tally_editHistory"].is_array(),
        "SARIF should have tally_editHistory property"
    );
    let edits = result["properties"]["tally_editHistory"]
        .as_array()
        .expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["field"], "description");
    assert_eq!(edits[0]["newValue"], "edited desc");
}

#[test]
fn cli_sarif_export_includes_tally_tags() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["tag", &uuid, "--add", "security", "--add", "owasp"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");
    let result = &sarif["runs"][0]["results"][0];

    let tags = result["properties"]["tally_tags"].as_array().expect("tags");
    assert!(tags.iter().any(|t| t == "security"));
    assert!(tags.iter().any(|t| t == "owasp"));
}

#[test]
fn cli_sarif_export_includes_result_provenance() {
    let (tmp, _uuid) = setup_with_finding();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");
    let result = &sarif["runs"][0]["results"][0];

    assert!(
        result["resultProvenance"]["firstDetectionTimeUtc"].is_string(),
        "SARIF should have resultProvenance.firstDetectionTimeUtc"
    );
}

#[test]
fn cli_sarif_export_omits_empty_properties() {
    let (tmp, _uuid) = setup_with_finding();
    // No notes, no edits, no tags -> properties should be absent or empty

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");
    let result = &sarif["runs"][0]["results"][0];

    // properties key should either be absent or not contain tally_* keys
    if let Some(props) = result.get("properties") {
        assert!(
            props.get("tally_notes").is_none(),
            "empty notes should not produce tally_notes"
        );
        assert!(
            props.get("tally_editHistory").is_none(),
            "empty edits should not produce tally_editHistory"
        );
        assert!(
            props.get("tally_tags").is_none(),
            "empty tags should not produce tally_tags"
        );
    }
    // If properties key is absent entirely, that's also correct
}

#[test]
fn cli_sarif_export_validates_required_fields() {
    let (tmp, _uuid) = setup_with_finding();

    let output = tally()
        .args(["export", "--format", "sarif"])
        .current_dir(tmp.path())
        .output()
        .expect("export");

    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse SARIF");

    // SARIF 2.1.0 required fields
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["$schema"].is_string());
    assert!(sarif["runs"].is_array());
    let run = &sarif["runs"][0];
    assert!(run["tool"]["driver"]["name"].is_string());
    assert!(run["tool"]["driver"]["version"].is_string());
    assert!(run["results"].is_array());

    // Each result has required fields
    let result = &run["results"][0];
    assert!(result["ruleId"].is_string());
    assert!(result["level"].is_string());
    assert!(result["message"]["text"].is_string());
    assert!(result["locations"].is_array());
}

// =============================================================================
// Import tests
// =============================================================================

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
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("imported=2"));

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
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("imported=1"));
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
        .env("RUST_LOG", "warn")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("No findings found"));
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
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("imported=3"));

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
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("imported=3"));
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
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("imported=1"));

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
