//! CLI integration tests for `tally rule` subcommands — spec task 9.6.
//!
//! Covers create, get, list, search, update, delete, add-example, and migrate.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a rule via CLI and assert success.
fn create_rule(tmp: &tempfile::TempDir, id: &str, name: &str, desc: &str) {
    tally()
        .args(["rule", "create", id, "--name", name, "--description", desc])
        .current_dir(tmp.path())
        .assert()
        .success();
}

/// Create a rule with a category.
fn create_rule_with_category(
    tmp: &tempfile::TempDir,
    id: &str,
    name: &str,
    desc: &str,
    category: &str,
) {
    tally()
        .args([
            "rule",
            "create",
            id,
            "--name",
            name,
            "--description",
            desc,
            "--category",
            category,
        ])
        .current_dir(tmp.path())
        .assert()
        .success();
}

/// Initialize a tally repo and return the temp dir.
fn init_repo() -> tempfile::TempDir {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();
    tmp
}

// ---------------------------------------------------------------------------
// Positive tests
// ---------------------------------------------------------------------------

#[test]
fn cli_rule_create_succeeds() {
    let tmp = init_repo();
    tally()
        .args([
            "rule",
            "create",
            "unsafe-unwrap",
            "--name",
            "Unsafe Unwrap",
            "--description",
            "Detects unwrap() in production code",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Created rule: unsafe-unwrap"));
}

#[test]
fn cli_rule_get_returns_json() {
    let tmp = init_repo();
    create_rule(&tmp, "get-test-rule", "Get Test", "A rule for testing get");

    tally()
        .args(["rule", "get", "get-test-rule"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"id\""))
        .stdout(predicate::str::contains("get-test-rule"))
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("Get Test"))
        .stdout(predicate::str::contains("\"description\""))
        .stdout(predicate::str::contains("\"status\""))
        .stdout(predicate::str::contains("\"created_at\""));
}

#[test]
fn cli_rule_list_table_format() {
    let tmp = init_repo();
    create_rule(&tmp, "rule-alpha", "Alpha Rule", "First rule");
    create_rule(&tmp, "rule-beta", "Beta Rule", "Second rule");

    tally()
        .args(["rule", "list", "--format", "table"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("rule-alpha"))
        .stdout(predicate::str::contains("rule-beta"));
}

#[test]
fn cli_rule_list_json_format() {
    let tmp = init_repo();
    create_rule(&tmp, "json-rule-a", "JSON A", "First");
    create_rule(&tmp, "json-rule-b", "JSON B", "Second");

    let output = tally()
        .args(["rule", "list", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("run list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("Expected valid JSON from rule list: {e}\nGot: {stdout}"));
    assert!(parsed.is_array(), "JSON output should be an array");
    let arr = parsed
        .as_array()
        .expect("rule list JSON should be an array");
    assert!(
        arr.len() >= 2,
        "Should have at least 2 rules, got {}",
        arr.len()
    );
}

#[test]
fn cli_rule_list_filter_by_category() {
    let tmp = init_repo();
    create_rule_with_category(
        &tmp,
        "sec-rule",
        "Security Rule",
        "Security check",
        "security",
    );
    create_rule_with_category(
        &tmp,
        "perf-rule",
        "Perf Rule",
        "Performance check",
        "performance",
    );

    tally()
        .args([
            "rule",
            "list",
            "--category",
            "security",
            "--format",
            "table",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("sec-rule"))
        .stdout(predicate::str::contains("perf-rule").not());
}

#[test]
fn cli_rule_search_finds_by_id() {
    let tmp = init_repo();
    create_rule(&tmp, "search-target", "Search Target", "A searchable rule");

    tally()
        .args(["rule", "search", "search-target"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("search-target"));
}

#[test]
fn cli_rule_update_adds_alias() {
    let tmp = init_repo();
    create_rule(&tmp, "alias-rule", "Alias Rule", "Rule to test aliases");

    tally()
        .args(["rule", "update", "alias-rule", "--add-alias", "my-alias"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated rule: alias-rule"));

    // Verify alias shows up in get
    tally()
        .args(["rule", "get", "alias-rule"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("my-alias"));
}

#[test]
fn cli_rule_delete_deprecates() {
    let tmp = init_repo();
    create_rule(&tmp, "deprecate-me", "Deprecate Me", "Rule to deprecate");

    tally()
        .args([
            "rule",
            "delete",
            "deprecate-me",
            "--reason",
            "replaced by better-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Deprecated rule: deprecate-me"));

    // Verify status is deprecated
    tally()
        .args(["rule", "get", "deprecate-me"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("deprecated"));
}

#[test]
fn cli_rule_add_example_appends() {
    let tmp = init_repo();
    create_rule(&tmp, "example-rule", "Example Rule", "Rule with examples");

    tally()
        .args([
            "rule",
            "add-example",
            "example-rule",
            "--type",
            "bad",
            "--language",
            "rust",
            "--code",
            "let x = foo.unwrap();",
            "--explanation",
            "unwrap can panic at runtime",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Added bad example"));

    // Verify example shows up in get
    tally()
        .args(["rule", "get", "example-rule"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap can panic"));
}

#[test]
fn cli_rule_migrate_registers_from_findings() {
    let tmp = init_repo();

    // Record findings with distinct rule IDs
    tally()
        .args([
            "record",
            "--file",
            "src/lib.rs",
            "--line",
            "10",
            "--severity",
            "important",
            "--title",
            "test finding one",
            "--rule",
            "migrate-rule-a",
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
            "suggestion",
            "--title",
            "test finding two",
            "--rule",
            "migrate-rule-b",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Run migrate
    tally()
        .args(["rule", "migrate"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Registered"))
        .stdout(predicate::str::contains("migrate-rule-a"))
        .stdout(predicate::str::contains("migrate-rule-b"));

    // Verify rules are now listed
    tally()
        .args(["rule", "list", "--format", "table"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("migrate-rule-a"))
        .stdout(predicate::str::contains("migrate-rule-b"));
}

// ---------------------------------------------------------------------------
// Negative tests
// ---------------------------------------------------------------------------

#[test]
fn cli_rule_create_duplicate_fails() {
    let tmp = init_repo();
    create_rule(&tmp, "dup-rule", "Dup Rule", "First creation");

    tally()
        .args([
            "rule",
            "create",
            "dup-rule",
            "--name",
            "Dup Again",
            "--description",
            "Second creation",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn cli_rule_create_invalid_id_fails() {
    let tmp = init_repo();

    tally()
        .args([
            "rule",
            "create",
            "invalid/id",
            "--name",
            "Bad ID",
            "--description",
            "Slash in ID",
        ])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_rule_get_nonexistent_fails() {
    let tmp = init_repo();

    tally()
        .args(["rule", "get", "does-not-exist"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_rule_delete_without_reason_fails() {
    let tmp = init_repo();
    create_rule(
        &tmp,
        "no-reason-rule",
        "No Reason",
        "Rule to delete without reason",
    );

    // --reason is required by clap, so omitting it should fail
    tally()
        .args(["rule", "delete", "no-reason-rule"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--reason"));
}
