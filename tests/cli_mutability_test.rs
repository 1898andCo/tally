//! CLI integration tests -- v0.5.0 features: update-fields, note, tag.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// =============================================================================
// update-fields tests
// =============================================================================

#[test]
fn cli_update_fields_changes_description() {
    let (tmp, uuid) = setup_with_finding();

    let output = tally()
        .args(["update-fields", &uuid, "--description", "updated desc"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    assert_eq!(json["description"], "updated desc");
}

#[test]
fn cli_update_fields_with_short_id() {
    let (tmp, _uuid) = setup_with_finding();

    // Short ID I1 should resolve (first important finding)
    let output = tally()
        .args(["update-fields", "I1", "--title", "new title"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    assert_eq!(json["title"], "new title");
}

#[test]
fn cli_update_fields_multiple_flags() {
    let (tmp, uuid) = setup_with_finding();

    let output = tally()
        .args([
            "update-fields",
            &uuid,
            "--title",
            "new title",
            "--description",
            "new desc",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    assert_eq!(json["title"], "new title");
    assert_eq!(json["description"], "new desc");
    assert_eq!(json["edit_history"].as_array().expect("arr").len(), 2);
}

#[test]
fn cli_update_fields_no_flags_returns_error() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["update-fields", &uuid])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least one field"));
}

#[test]
fn cli_update_fields_nonexistent_id_returns_error() {
    let (tmp, _) = setup_with_finding();

    tally()
        .args([
            "update-fields",
            "00000000-0000-0000-0000-000000000000",
            "--title",
            "x",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_update_fields_invalid_severity_returns_error() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["update-fields", &uuid, "--severity", "ultra"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// =============================================================================
// note tests
// =============================================================================

#[test]
fn cli_note_adds_note_status_unchanged() {
    let (tmp, uuid) = setup_with_finding();

    let output = tally()
        .args(["note", &uuid, "Covered by Story 1.21"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    assert_eq!(json["status"], "open");
    assert_eq!(json["notes_count"], 1);
}

#[test]
fn cli_note_with_short_id() {
    let (tmp, _) = setup_with_finding();

    tally()
        .args(["note", "I1", "test note"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn cli_note_empty_text_returns_error() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["note", &uuid, ""])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty"));
}

// =============================================================================
// tag tests
// =============================================================================

#[test]
fn cli_tag_add() {
    let (tmp, uuid) = setup_with_finding();

    let output = tally()
        .args(["tag", &uuid, "--add", "story:1.21"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let tags = json["tags"].as_array().expect("tags");
    assert!(tags.iter().any(|t| t == "story:1.21"));
}

#[test]
fn cli_tag_remove() {
    let (tmp, uuid) = setup_with_finding();

    // Add then remove
    tally()
        .args(["tag", &uuid, "--add", "to-remove", "--add", "keep"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args(["tag", &uuid, "--remove", "to-remove"])
        .current_dir(tmp.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let tags: Vec<&str> = json["tags"]
        .as_array()
        .expect("arr")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(!tags.contains(&"to-remove"));
    assert!(tags.contains(&"keep"));
}

#[test]
fn cli_tag_add_and_remove_combined() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["tag", &uuid, "--add", "old"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["tag", &uuid, "--add", "new", "--remove", "old"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn cli_tag_no_add_or_remove_returns_error() {
    let (tmp, uuid) = setup_with_finding();

    tally()
        .args(["tag", &uuid])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least one"));
}

// =============================================================================
// help lists tests
// =============================================================================

#[test]
fn cli_help_lists_update_fields() {
    tally()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("update-fields"));
}

#[test]
fn cli_help_lists_note() {
    tally()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("note"));
}

#[test]
fn cli_help_lists_tag() {
    tally()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

// =============================================================================
// rebuild-index with tags
// =============================================================================

#[test]
fn cli_rebuild_index_includes_tags() {
    let (tmp, uuid) = setup_with_finding();

    // Add tags
    tally()
        .args(["tag", &uuid, "--add", "indexed-tag"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Rebuild index
    tally()
        .arg("rebuild-index")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query with tag filter should still work after rebuild
    let output = tally()
        .args(["query", "--tag", "indexed-tag"])
        .current_dir(tmp.path())
        .output()
        .expect("query");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("parse");
    let findings = json.as_array().expect("arr");
    assert_eq!(findings.len(), 1);
}
