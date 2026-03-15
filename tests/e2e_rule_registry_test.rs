//! End-to-end rule registry tests — spec task 9.10.
//!
//! Tests the full CLI workflow for rule creation, alias resolution,
//! auto-registration, dedup via canonical IDs, and migration idempotency.

mod cli_common;

use serde_json::Value;

use cli_common::{setup_cli_repo, tally};

fn run_record_with_agent(
    dir: &std::path::Path,
    file: &str,
    line: u32,
    severity: &str,
    title: &str,
    rule: &str,
    agent: Option<&str>,
) -> Value {
    let mut args = vec![
        "record".to_string(),
        "--file".to_string(),
        file.to_string(),
        "--line".to_string(),
        line.to_string(),
        "--severity".to_string(),
        severity.to_string(),
        "--title".to_string(),
        title.to_string(),
        "--rule".to_string(),
        rule.to_string(),
    ];
    if let Some(a) = agent {
        args.push("--agent".to_string());
        args.push(a.to_string());
    }

    let output = tally()
        .args(&args)
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
    assert!(
        output.status.success(),
        "query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse query output")
}

// =============================================================================
// E2E 9.10a: Record with alias → dedup → query by canonical ID
// =============================================================================

#[test]
fn e2e_record_with_alias_dedup_query_by_canonical() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // 1. Create a rule with an alias
    tally()
        .args([
            "rule",
            "create",
            "unsafe-unwrap",
            "--name",
            "Unsafe unwrap usage",
            "--description",
            "Detects .unwrap() calls in production code",
            "--alias",
            "unwrap-usage",
        ])
        .current_dir(dir)
        .assert()
        .success();

    // 2. Record a finding using the ALIAS (not canonical ID)
    let record1 = run_record_with_agent(
        dir,
        "src/api.rs",
        42,
        "important",
        "unwrap in handler",
        "unwrap-usage",
        None,
    );
    assert_eq!(record1["status"], "created");
    let uuid1 = record1["uuid"].as_str().expect("uuid").to_string();

    // 3. Verify the finding has canonical rule_id, not the alias
    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0]["rule_id"], "unsafe-unwrap",
        "finding should use canonical rule ID, not alias"
    );

    // 4. Record same finding again with alias from a different agent — should dedup
    let record2 = run_record_with_agent(
        dir,
        "src/api.rs",
        42,
        "important",
        "unwrap in handler",
        "unwrap-usage",
        Some("agent-2"),
    );
    assert_eq!(
        record2["status"], "deduplicated",
        "second record via alias should dedup"
    );
    assert_eq!(
        record2["uuid"].as_str().expect("uuid"),
        uuid1,
        "dedup should return same UUID"
    );

    // 5. Query by canonical ID — should find the finding
    let by_canonical = run_query_json(dir, &["--rule", "unsafe-unwrap"]);
    assert_eq!(
        by_canonical.len(),
        1,
        "query by canonical ID should find the finding"
    );
    assert_eq!(by_canonical[0]["uuid"], uuid1);

    // 6. Verify discovered_by has both agents
    let discovered_by = by_canonical[0]["discovered_by"]
        .as_array()
        .expect("discovered_by");
    assert_eq!(discovered_by.len(), 2, "two agents should be recorded");
}

// =============================================================================
// E2E 9.10b: Auto-registration creates experimental rule for unknown IDs
// =============================================================================

#[test]
fn e2e_auto_registration_creates_experimental_rule() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // 1. Record a finding with a brand-new rule ID (no pre-existing rule)
    let record = run_record_with_agent(
        dir,
        "src/api.rs",
        1,
        "suggestion",
        "new pattern detected",
        "brand-new-rule",
        None,
    );
    assert_eq!(record["status"], "created");

    // 2. Verify the rule was auto-registered
    let output = tally()
        .args(["rule", "get", "brand-new-rule"])
        .current_dir(dir)
        .output()
        .expect("rule get");
    assert!(
        output.status.success(),
        "rule get failed — rule was not auto-registered: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rule: Value = serde_json::from_slice(&output.stdout).expect("parse rule");
    assert_eq!(rule["id"], "brand-new-rule");
    assert_eq!(
        rule["status"], "experimental",
        "auto-registered rule should have experimental status"
    );
    assert_eq!(
        rule["created_by"], "auto",
        "auto-registered rule should be created_by 'auto'"
    );
}

// =============================================================================
// E2E 9.10c: Migrate then idempotent
// =============================================================================

#[test]
fn e2e_migrate_then_idempotent() {
    let tmp = setup_cli_repo();
    let dir = tmp.path();
    tally().arg("init").current_dir(dir).assert().success();

    // 1. Record 3 findings with 2 different rules
    run_record_with_agent(
        dir,
        "src/a.rs",
        10,
        "important",
        "finding one",
        "rule-alpha",
        None,
    );
    run_record_with_agent(
        dir,
        "src/b.rs",
        20,
        "critical",
        "finding two",
        "rule-beta",
        None,
    );
    run_record_with_agent(
        dir,
        "src/c.rs",
        30,
        "suggestion",
        "finding three",
        "rule-alpha",
        None,
    );

    let findings = run_query_json(dir, &[]);
    assert_eq!(findings.len(), 3, "should have 3 findings");

    // 2. First migrate — rules were already auto-registered during record,
    //    so migrate should report 0 new (they already exist)
    let migrate1 = tally()
        .args(["rule", "migrate"])
        .current_dir(dir)
        .output()
        .expect("migrate 1");
    assert!(
        migrate1.status.success(),
        "first migrate failed: {}",
        String::from_utf8_lossy(&migrate1.stderr)
    );
    let _stdout1 = String::from_utf8_lossy(&migrate1.stdout);

    // 3. Second migrate — must be idempotent (0 new rules)
    let migrate2 = tally()
        .args(["rule", "migrate"])
        .current_dir(dir)
        .output()
        .expect("migrate 2");
    assert!(
        migrate2.status.success(),
        "second migrate failed: {}",
        String::from_utf8_lossy(&migrate2.stderr)
    );
    let stdout2 = String::from_utf8_lossy(&migrate2.stdout);
    assert!(
        stdout2.contains("Registered 0 rules"),
        "second migrate should register 0 new rules, got: {stdout2}"
    );

    // 4. Rule list should show exactly 2 rules
    let output = tally()
        .args(["rule", "list", "--format", "json"])
        .current_dir(dir)
        .output()
        .expect("rule list");
    assert!(output.status.success());
    let rules: Vec<Value> = serde_json::from_slice(&output.stdout).expect("parse rules");
    assert_eq!(rules.len(), 2, "should have exactly 2 rules");

    // Verify both rule IDs are present
    let rule_ids: Vec<&str> = rules
        .iter()
        .map(|r| r["id"].as_str().expect("id"))
        .collect();
    assert!(
        rule_ids.contains(&"rule-alpha"),
        "should contain rule-alpha"
    );
    assert!(rule_ids.contains(&"rule-beta"), "should contain rule-beta");
}
