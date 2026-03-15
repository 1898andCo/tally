//! CLI integration tests for enhanced query flags and `TallyQL` filter expressions.

mod cli_common;
use cli_common::*;
use predicates::prelude::*;

/// Record a finding with specific fields for query testing.
fn record_finding(dir: &std::path::Path, severity: &str, title: &str, rule: &str, file: &str) {
    tally()
        .args([
            "record",
            "--file",
            file,
            "--line",
            "10",
            "--severity",
            severity,
            "--title",
            title,
            "--rule",
            rule,
            "--agent",
            "test-agent",
            "--category",
            "test-cat",
        ])
        .current_dir(dir)
        .assert()
        .success();
}

fn init_with_findings(dir: &std::path::Path) {
    tally().arg("init").current_dir(dir).assert().success();
    record_finding(
        dir,
        "critical",
        "unwrap bug",
        "unsafe-unwrap",
        "src/api/handler.rs",
    );
    record_finding(
        dir,
        "important",
        "missing test",
        "no-test",
        "tests/api_test.rs",
    );
    record_finding(
        dir,
        "suggestion",
        "use clippy lint",
        "clippy-warn",
        "src/lib.rs",
    );
}

// =============================================================================
// --filter (TallyQL expression)
// =============================================================================

#[test]
fn cli_filter_severity_equals() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--filter", "severity = critical"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test").not());
}

#[test]
fn cli_filter_compound_and() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args([
            "query",
            "--filter",
            r#"severity = critical AND file CONTAINS "api""#,
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test").not());
}

#[test]
fn cli_filter_or_expression() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args([
            "query",
            "--filter",
            "severity = critical OR severity = important",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test"))
        .stdout(predicate::str::contains("clippy lint").not());
}

#[test]
fn cli_filter_not_expression() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--filter", "NOT severity = suggestion"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test"))
        .stdout(predicate::str::contains("clippy lint").not());
}

#[test]
fn cli_filter_parse_error_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--filter", "foo = bar"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("TallyQL parse error"));
}

// =============================================================================
// --filter combined with basic flags
// =============================================================================

#[test]
fn cli_filter_combined_with_severity_flag() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // --severity critical AND --filter 'file CONTAINS "api"'
    tally()
        .args([
            "query",
            "--severity",
            "critical",
            "--filter",
            r#"file CONTAINS "api""#,
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"));
}

// =============================================================================
// --severity multi-value (comma-separated)
// =============================================================================

#[test]
fn cli_severity_multi_value() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--severity", "critical,important"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test"))
        .stdout(predicate::str::contains("clippy lint").not());
}

// =============================================================================
// --not-status
// =============================================================================

#[test]
fn cli_not_status_excludes() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // All findings are "open", so --not-status open should return nothing
    tally()
        .args(["query", "--not-status", "open", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 findings"));
}

// =============================================================================
// --text (full-text search)
// =============================================================================

#[test]
fn cli_text_search_matches_title() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--text", "unwrap"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test").not());
}

#[test]
fn cli_text_search_case_insensitive() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--text", "UNWRAP"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"));
}

#[test]
fn cli_text_search_no_match() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args([
            "query",
            "--text",
            "xyzzy_nonexistent",
            "--format",
            "summary",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 findings"));
}

// =============================================================================
// --agent
// =============================================================================

#[test]
fn cli_agent_filter() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--agent", "test-agent", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("3 findings"));
}

#[test]
fn cli_agent_filter_no_match() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--agent", "nonexistent", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 findings"));
}

// =============================================================================
// --category
// =============================================================================

#[test]
fn cli_category_filter() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--category", "test-cat", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("3 findings"));
}

// =============================================================================
// --sort
// =============================================================================

#[test]
fn cli_sort_by_severity_desc() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    let output = tally()
        .args(["query", "--sort", "severity", "--sort-dir", "desc"])
        .current_dir(tmp.path())
        .output()
        .expect("run tally");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Critical should appear before important, which should appear before suggestion
    let crit_pos = stdout.find("unwrap bug").expect("critical finding");
    let imp_pos = stdout.find("missing test").expect("important finding");
    let sug_pos = stdout.find("clippy lint").expect("suggestion finding");
    assert!(crit_pos < imp_pos, "critical should be before important");
    assert!(imp_pos < sug_pos, "important should be before suggestion");
}

#[test]
fn cli_sort_invalid_field_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--sort", "nonexistent"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot sort by"));
}

#[test]
fn cli_sort_invalid_direction_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--sort", "severity", "--sort-dir", "sideways"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid sort direction"));
}

// =============================================================================
// --since / --before
// =============================================================================

#[test]
fn cli_since_duration() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // All findings just created, so --since 1h should return all
    tally()
        .args(["query", "--since", "1h", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("3 findings"));
}

#[test]
fn cli_since_far_future_returns_empty() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // --since 0s means "since right now" — nothing created after now
    tally()
        .args(["query", "--before", "30d", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 findings"));
}

#[test]
fn cli_since_invalid_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--since", "not-a-date"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid date/duration"));
}

// =============================================================================
// Backward compatibility: existing flags still work
// =============================================================================

#[test]
fn cli_existing_severity_flag_unchanged() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--severity", "critical"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test").not());
}

#[test]
fn cli_existing_rule_flag_unchanged() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--rule", "unsafe-unwrap"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"));
}

// =============================================================================
// Help text includes TallyQL
// =============================================================================

#[test]
fn cli_query_help_mentions_filter() {
    tally()
        .args(["query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--filter"))
        .stdout(predicate::str::contains("--since"))
        .stdout(predicate::str::contains("--text"))
        .stdout(predicate::str::contains("--sort"))
        .stdout(predicate::str::contains("--not-status"))
        .stdout(predicate::str::contains("--agent"))
        .stdout(predicate::str::contains("--category"))
        .stdout(predicate::str::contains("TallyQL"));
}

// =============================================================================
// --status multi-value
// =============================================================================

#[test]
fn cli_status_multi_value() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // All findings are "open", so open,acknowledged should return all 3
    tally()
        .args([
            "query",
            "--status",
            "open,acknowledged",
            "--format",
            "summary",
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("3 findings"));
}

// =============================================================================
// --not-status invalid value
// =============================================================================

#[test]
fn cli_not_status_invalid_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--not-status", "invalid_status"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid"));
}

// =============================================================================
// --category no match
// =============================================================================

#[test]
fn cli_category_no_match() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--category", "nonexistent", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 findings"));
}

// =============================================================================
// --sort title ascending (default direction for text fields)
// =============================================================================

#[test]
fn cli_sort_by_title_asc() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    let output = tally()
        .args(["query", "--sort", "title"])
        .current_dir(tmp.path())
        .output()
        .expect("run tally");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Default for title is asc: "missing test" < "unwrap bug" < "use clippy lint"
    let miss_pos = stdout.find("missing test").expect("missing test");
    let unwrap_pos = stdout.find("unwrap bug").expect("unwrap bug");
    let clippy_pos = stdout.find("clippy lint").expect("clippy lint");
    assert!(
        miss_pos < unwrap_pos,
        "missing test before unwrap bug (asc)"
    );
    assert!(
        unwrap_pos < clippy_pos,
        "unwrap bug before clippy lint (asc)"
    );
}

// =============================================================================
// --sort created_at default desc
// =============================================================================

#[test]
fn cli_sort_created_at_default_desc() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // All created at roughly the same time, so just verify it doesn't error
    tally()
        .args(["query", "--sort", "created_at"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

// =============================================================================
// --since ISO 8601 date
// =============================================================================

#[test]
fn cli_since_iso_date() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // All findings created now, so --since 2020-01-01 should return all
    tally()
        .args(["query", "--since", "2020-01-01", "--format", "summary"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("3 findings"));
}

// =============================================================================
// --before invalid
// =============================================================================

#[test]
fn cli_before_invalid_returns_error() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--before", "not-a-date"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid date/duration"));
}

// =============================================================================
// Backward compat: --file and --tag
// =============================================================================

#[test]
fn cli_existing_file_flag_unchanged() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--file", "src/api"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test").not());
}

#[test]
fn cli_existing_tag_flag_unchanged() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record a finding with tags
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
            "tagged finding",
            "--rule",
            "test-rule",
            "--tags",
            "story:5.1,security",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--tag", "story:5.1"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("tagged finding"));
}

// =============================================================================
// --filter HAS/MISSING
// =============================================================================

#[test]
fn cli_filter_has_expression() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record one with suggested_fix, one without
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
            "with fix",
            "--rule",
            "r1",
            "--suggested-fix",
            "do this",
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
            "1",
            "--severity",
            "important",
            "--title",
            "without fix",
            "--rule",
            "r2",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    tally()
        .args(["query", "--filter", "HAS suggested_fix"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("with fix"))
        .stdout(predicate::str::contains("without fix").not());
}

// =============================================================================
// --filter IN list
// =============================================================================

#[test]
fn cli_filter_in_list() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    tally()
        .args(["query", "--filter", "severity IN (critical, important)"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unwrap bug"))
        .stdout(predicate::str::contains("missing test"))
        .stdout(predicate::str::contains("clippy lint").not());
}

// =============================================================================
// --filter combined with --sort
// =============================================================================

#[test]
fn cli_filter_with_sort() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    let output = tally()
        .args([
            "query",
            "--filter",
            "severity IN (critical, important)",
            "--sort",
            "severity",
            "--sort-dir",
            "desc",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run tally");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let crit_pos = stdout.find("unwrap bug").expect("critical");
    let imp_pos = stdout.find("missing test").expect("important");
    assert!(
        crit_pos < imp_pos,
        "critical before important after sort desc"
    );
    // suggestion should not be present
    assert!(!stdout.contains("clippy lint"));
}

// =============================================================================
// Multiple --sort fields
// =============================================================================

#[test]
fn cli_multi_sort_fields() {
    let tmp = setup_cli_repo();
    init_with_findings(tmp.path());

    // Sort by severity desc, then title asc — should not error
    tally()
        .args([
            "query",
            "--sort",
            "severity",
            "--sort",
            "title",
            "--sort-dir",
            "desc",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();
}

// =============================================================================
// E2E: record → tag → query with --filter tag expression
// =============================================================================

#[test]
fn e2e_record_tag_query_filter() {
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
            "tagged issue",
            "--rule",
            "test-rule",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Get the short ID from query output
    let output = tally()
        .args(["query", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("query");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tagged issue"));

    // Add a tag
    tally()
        .args(["tag", "C1", "--add", "story:5.1"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query using TallyQL tag filter
    tally()
        .args(["query", "--filter", r#"tag CONTAINS "story:5.1""#])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("tagged issue"));
}

// =============================================================================
// E2E: record → update status → query --not-status
// =============================================================================

#[test]
fn e2e_lifecycle_then_not_status_query() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Record two findings
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
            "will close",
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
            "src/b.rs",
            "--line",
            "1",
            "--severity",
            "important",
            "--title",
            "stays open",
            "--rule",
            "r2",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Transition first to acknowledged → in_progress → resolved → closed
    tally()
        .args(["update", "C1", "--status", "acknowledged"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", "C1", "--status", "in_progress"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", "C1", "--status", "resolved"])
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .args(["update", "C1", "--status", "closed"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Query excluding closed
    tally()
        .args(["query", "--not-status", "closed"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stays open"))
        .stdout(predicate::str::contains("will close").not());
}
