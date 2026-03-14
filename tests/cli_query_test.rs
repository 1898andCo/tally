//! CLI integration tests -- query and stats.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// =============================================================================
// Query tests
// =============================================================================

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
fn cli_query_tag_filter() {
    let (tmp, uuid) = setup_with_finding();

    // Add tag to first finding
    tally()
        .args(["tag", &uuid, "--add", "story:1.21"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record second finding (no tag)
    tally()
        .args([
            "record",
            "--file",
            "src/other.rs",
            "--line",
            "20",
            "--severity",
            "suggestion",
            "--title",
            "other",
            "--rule",
            "other-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query with tag filter — should return only the tagged one
    let output = tally()
        .args(["query", "--tag", "story:1.21"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let findings = json.as_array().expect("arr");
    assert_eq!(findings.len(), 1);
}

// =============================================================================
// Stats tests
// =============================================================================

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

#[test]
fn cli_stats_shows_notes_and_edits_counts() {
    let (tmp, uuid) = setup_with_finding();

    // Add note and edit
    tally()
        .args(["note", &uuid, "test note"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update-fields", &uuid, "--title", "edited title"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .arg("stats")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Findings with notes: 1"))
        .stdout(predicate::str::contains("Findings with edits: 1"));
}

#[test]
fn cli_stats_shows_top_tags() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["tag", &uuid, "--add", "story:1.21", "--add", "wave-1"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .arg("stats")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Top tags:"))
        .stdout(predicate::str::contains("story:1.21"));
}
