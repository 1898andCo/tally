//! Enhanced MCP tests for resources and batch status updates.
//!
//! Tests cover:
//! - Resource reads (version, rules summary, rule detail, agent, timeline)
//! - Batch status update tool (positive, negative, partial failure)
//!
//! These tests call `TallyMcpServer` methods and `read_resource_*` helpers directly.

use rmcp::handler::server::wrapper::Parameters;
use tally_ng::mcp::server::{
    CreateRuleInput, RecordFindingInput, TallyMcpServer, UpdateBatchStatusInput,
};
use tally_ng::storage::GitFindingsStore;

/// Create a temp git repo and return `(TempDir, TallyMcpServer)`.
fn setup_mcp() -> (tempfile::TempDir, TallyMcpServer) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_path = tmp.path().to_str().expect("path").to_string();

    // git init + initial commit
    {
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
    }

    // Initialize tally findings branch
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init tally");
    drop(store);

    let server = TallyMcpServer::new(repo_path);
    (tmp, server)
}

fn make_record_input(
    file: &str,
    line: u32,
    severity: &str,
    title: &str,
    rule: &str,
) -> RecordFindingInput {
    RecordFindingInput {
        file_path: file.to_string(),
        line_start: line,
        line_end: None,
        severity: severity.to_string(),
        title: title.to_string(),
        rule_id: rule.to_string(),
        description: None,
        agent: None,
        suggested_fix: None,
        evidence: None,
        locations: None,
        category: None,
        tags: None,
        pr_number: None,
        session_id: None,
        related_to: None,
        relationship_type: None,
    }
}

fn make_record_input_with_agent(
    file: &str,
    line: u32,
    severity: &str,
    title: &str,
    rule: &str,
    agent: &str,
) -> RecordFindingInput {
    RecordFindingInput {
        agent: Some(agent.to_string()),
        ..make_record_input(file, line, severity, title, rule)
    }
}

fn extract_tool_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn extract_tool_json(result: &rmcp::model::CallToolResult) -> serde_json::Value {
    let text = extract_tool_text(result);
    serde_json::from_str(&text).expect("parse tool output JSON")
}

fn make_create_rule_input(rule_id: &str, name: &str, category: &str) -> CreateRuleInput {
    CreateRuleInput {
        rule_id: rule_id.to_string(),
        name: name.to_string(),
        description: format!("Description for {name}"),
        category: Some(category.to_string()),
        severity_hint: None,
        aliases: None,
        cwe_ids: None,
        tags: None,
        scope_include: None,
        scope_exclude: None,
    }
}

// =============================================================================
// Resources — Positive
// =============================================================================

#[tokio::test]
async fn mcp_resource_version_returns_version_and_counts() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Record one finding so finding_count > 0
    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            10,
            "critical",
            "version test finding",
            "version-rule",
        )))
        .await
        .expect("record finding");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_version(&store).expect("version resource");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse version JSON");

    assert!(
        json["version"].is_string(),
        "version field should be a string"
    );
    assert!(
        !json["version"].as_str().expect("version str").is_empty(),
        "version should not be empty"
    );
    assert!(
        json["rule_count"].is_number(),
        "rule_count field should be a number"
    );
    assert!(
        json["finding_count"].is_number(),
        "finding_count field should be a number"
    );
    assert_eq!(
        json["finding_count"], 1,
        "finding_count should reflect the single recorded finding"
    );
}

#[tokio::test]
async fn mcp_resource_rules_summary_with_rules() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Create 2 rules in different categories
    server
        .create_rule(Parameters(make_create_rule_input(
            "unsafe-unwrap",
            "Unsafe Unwrap",
            "safety",
        )))
        .await
        .expect("create rule 1");
    server
        .create_rule(Parameters(make_create_rule_input(
            "sql-injection",
            "SQL Injection",
            "security",
        )))
        .await
        .expect("create rule 2");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_rules_summary(&store).expect("rules summary");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse");

    assert_eq!(json["total_rules"], 2, "should have 2 rules");
    assert!(json["by_category"].is_object(), "by_category should exist");

    let by_category = json["by_category"].as_object().expect("by_category obj");
    assert_eq!(
        by_category
            .get("safety")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "safety category should have 1 rule"
    );
    assert_eq!(
        by_category
            .get("security")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "security category should have 1 rule"
    );

    assert!(json["by_status"].is_object(), "by_status should exist");
}

#[tokio::test]
async fn mcp_resource_rules_summary_empty() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_rules_summary(&store).expect("rules summary");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse");

    assert_eq!(json["total_rules"], 0, "should have 0 rules");
    let by_category = json["by_category"].as_object().expect("by_category obj");
    assert!(
        by_category.is_empty(),
        "by_category should be empty with no rules"
    );
}

#[tokio::test]
async fn mcp_resource_rule_detail_returns_rule_and_findings() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Create a rule
    server
        .create_rule(Parameters(make_create_rule_input(
            "unsafe-unwrap",
            "Unsafe Unwrap",
            "safety",
        )))
        .await
        .expect("create rule");

    // Record a finding that uses that rule
    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            42,
            "critical",
            "unwrap found",
            "unsafe-unwrap",
        )))
        .await
        .expect("record finding");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_rule_detail(&store, "unsafe-unwrap")
        .expect("rule detail");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse");

    // Verify rule details are present
    assert!(json["rule"].is_object(), "response should contain rule");
    assert_eq!(json["rule"]["id"], "unsafe-unwrap", "rule id should match");

    // Verify findings are included
    assert_eq!(
        json["finding_count"], 1,
        "should have 1 finding for this rule"
    );
    let findings = json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1, "findings array should have 1 element");
    assert_eq!(
        findings[0]["title"], "unwrap found",
        "finding title should match"
    );
}

#[tokio::test]
async fn mcp_resource_agent_returns_agent_findings() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Record 2 findings with different agents
    server
        .record_finding(Parameters(make_record_input_with_agent(
            "src/api.rs",
            10,
            "critical",
            "security issue",
            "rule-sec",
            "dclaude:security-reviewer",
        )))
        .await
        .expect("record agent-1 finding");

    server
        .record_finding(Parameters(make_record_input_with_agent(
            "src/db.rs",
            20,
            "important",
            "db issue",
            "rule-db",
            "dclaude:code-reviewer",
        )))
        .await
        .expect("record agent-2 finding");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_by_agent(&store, "dclaude:security-reviewer")
        .expect("agent resource");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");

    assert_eq!(
        findings.len(),
        1,
        "should only return findings from dclaude:security-reviewer"
    );
    assert_eq!(
        findings[0]["title"], "security issue",
        "should be the security-reviewer finding"
    );
}

#[tokio::test]
async fn mcp_resource_timeline_returns_grouped_data() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Record a finding so the timeline has data
    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            1,
            "critical",
            "timeline finding",
            "timeline-rule",
        )))
        .await
        .expect("record finding");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result =
        tally_ng::mcp::server::read_resource_timeline(&store, "30d").expect("timeline resource");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse");

    assert!(json["timeline"].is_array(), "timeline should be an array");
    let timeline = json["timeline"].as_array().expect("timeline array");
    assert!(
        !timeline.is_empty(),
        "timeline should have at least one entry for today"
    );

    // Each entry should have date, created, resolved fields
    let entry = &timeline[0];
    assert!(entry["date"].is_string(), "entry should have a date");
    assert!(
        entry["created"].is_number(),
        "entry should have created count"
    );
    assert!(
        entry["resolved"].is_number(),
        "entry should have resolved count"
    );
}

// =============================================================================
// Resources — Negative
// =============================================================================

#[tokio::test]
async fn mcp_resource_rule_detail_nonexistent_returns_error() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_rule_detail(&store, "nonexistent-rule");

    assert!(
        result.is_err(),
        "reading a nonexistent rule should return an error"
    );
}

#[tokio::test]
async fn mcp_resource_agent_unknown_returns_empty() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    // Record a finding with a known agent
    server
        .record_finding(Parameters(make_record_input_with_agent(
            "src/main.rs",
            1,
            "critical",
            "known agent finding",
            "rule-a",
            "known-agent",
        )))
        .await
        .expect("record");

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_by_agent(&store, "unknown-agent")
        .expect("agent resource");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");

    assert!(
        findings.is_empty(),
        "unknown agent should return empty results"
    );
}

#[tokio::test]
async fn mcp_resource_timeline_invalid_duration() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    let store = GitFindingsStore::open(&repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_timeline(&store, "invalid");

    assert!(result.is_err(), "invalid duration should return an error");
}

// =============================================================================
// Batch Tool — Positive
// =============================================================================

#[tokio::test]
async fn mcp_batch_status_transitions_multiple() {
    let (_tmp, server) = setup_mcp();

    // Record 3 findings and capture their UUIDs
    let mut uuids = Vec::new();
    for i in 0u32..3 {
        let input = make_record_input(
            &format!("src/file{i}.rs"),
            i + 1,
            "critical",
            &format!("finding {i}"),
            &format!("rule-{i}"),
        );
        let result = server
            .record_finding(Parameters(input))
            .await
            .expect("record");
        let json = extract_tool_json(&result);
        uuids.push(
            json["uuid"]
                .as_str()
                .expect("uuid should be a string")
                .to_string(),
        );
    }

    // Batch update all to acknowledged
    let batch_input = UpdateBatchStatusInput {
        finding_ids: uuids.clone(),
        status: "acknowledged".to_string(),
        reason: None,
        agent: None,
    };
    let result = server
        .update_batch_status(Parameters(batch_input))
        .await
        .expect("batch update");
    let json = extract_tool_json(&result);

    assert_eq!(json["total"], 3, "total should be 3");
    let results = json["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3, "should have 3 results");

    for (i, r) in results.iter().enumerate() {
        assert_eq!(
            r["status"], "success",
            "finding {i} should have transitioned successfully"
        );
        assert_eq!(
            r["new_status"], "acknowledged",
            "finding {i} should be acknowledged"
        );
    }
}

#[tokio::test]
async fn mcp_batch_status_with_reason() {
    let (_tmp, server) = setup_mcp();

    // Record a finding
    let input = make_record_input("src/main.rs", 1, "critical", "batch reason test", "rule-a");
    let result = server
        .record_finding(Parameters(input))
        .await
        .expect("record");
    let uuid = extract_tool_json(&result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // Batch update with a reason
    let batch_input = UpdateBatchStatusInput {
        finding_ids: vec![uuid.clone()],
        status: "acknowledged".to_string(),
        reason: Some("batch triage pass".to_string()),
        agent: Some("test-agent".to_string()),
    };
    let result = server
        .update_batch_status(Parameters(batch_input))
        .await
        .expect("batch update");
    let json = extract_tool_json(&result);

    assert_eq!(json["total"], 1);
    let results = json["results"].as_array().expect("results array");
    assert_eq!(results[0]["status"], "success");
    assert_eq!(results[0]["new_status"], "acknowledged");

    // Verify the reason was recorded by loading the finding directly
    let store = GitFindingsStore::open(server.repo_path()).expect("open");
    let finding_uuid: uuid::Uuid = uuid.parse().expect("parse uuid");
    let finding = store.load_finding(&finding_uuid).expect("load finding");
    let last_transition = finding
        .state_history
        .last()
        .expect("should have state history");
    assert_eq!(
        last_transition.reason.as_deref(),
        Some("batch triage pass"),
        "reason should be recorded in state history"
    );
    assert_eq!(
        last_transition.agent_id, "test-agent",
        "agent should be recorded in state history"
    );
}

// =============================================================================
// Batch Tool — Negative
// =============================================================================

#[tokio::test]
async fn mcp_batch_status_invalid_status() {
    let (_tmp, server) = setup_mcp();

    // Record a finding so we have a valid ID
    let input = make_record_input(
        "src/main.rs",
        1,
        "critical",
        "invalid status test",
        "rule-a",
    );
    let result = server
        .record_finding(Parameters(input))
        .await
        .expect("record");
    let uuid = extract_tool_json(&result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // Try batch update with invalid status
    let batch_input = UpdateBatchStatusInput {
        finding_ids: vec![uuid],
        status: "nonexistent_status".to_string(),
        reason: None,
        agent: None,
    };
    let err = server
        .update_batch_status(Parameters(batch_input))
        .await
        .expect_err("should fail with invalid status");

    assert!(
        err.message.to_string().contains("invalid")
            || err.message.to_string().contains("Invalid")
            || err.message.to_string().contains("status"),
        "error should mention invalid status, got: {}",
        err.message
    );
}

#[tokio::test]
async fn mcp_batch_status_nonexistent_id() {
    let (_tmp, server) = setup_mcp();

    let fake_uuid = "00000000-0000-0000-0000-000000000000";
    let batch_input = UpdateBatchStatusInput {
        finding_ids: vec![fake_uuid.to_string()],
        status: "acknowledged".to_string(),
        reason: None,
        agent: None,
    };
    let result = server
        .update_batch_status(Parameters(batch_input))
        .await
        .expect("batch update should succeed at top level");
    let json = extract_tool_json(&result);

    assert_eq!(json["total"], 1);
    let results = json["results"].as_array().expect("results array");
    assert_eq!(
        results[0]["status"], "error",
        "nonexistent finding should report per-finding error"
    );
    assert!(
        results[0]["error"].is_string(),
        "error field should describe the problem"
    );
}

#[tokio::test]
async fn mcp_batch_status_partial_failure() {
    let (_tmp, server) = setup_mcp();

    // Record 2 valid findings
    let input1 = make_record_input("src/a.rs", 1, "critical", "valid finding 1", "rule-a");
    let rec1 = server
        .record_finding(Parameters(input1))
        .await
        .expect("record 1");
    let uuid1 = extract_tool_json(&rec1)["uuid"]
        .as_str()
        .expect("uuid1")
        .to_string();

    let input2 = make_record_input("src/b.rs", 2, "important", "valid finding 2", "rule-b");
    let rec2 = server
        .record_finding(Parameters(input2))
        .await
        .expect("record 2");
    let uuid2 = extract_tool_json(&rec2)["uuid"]
        .as_str()
        .expect("uuid2")
        .to_string();

    let fake_uuid = "00000000-0000-0000-0000-000000000000";

    // Mix valid and invalid IDs
    let batch_input = UpdateBatchStatusInput {
        finding_ids: vec![uuid1.clone(), fake_uuid.to_string(), uuid2.clone()],
        status: "acknowledged".to_string(),
        reason: None,
        agent: None,
    };
    let batch_result = server
        .update_batch_status(Parameters(batch_input))
        .await
        .expect("batch update should succeed at top level");
    let json = extract_tool_json(&batch_result);

    assert_eq!(json["total"], 3, "total should reflect all 3 IDs");
    let items = json["results"].as_array().expect("results array");
    assert_eq!(items.len(), 3, "should have 3 results");

    // First and third should succeed (valid UUIDs)
    assert_eq!(
        items[0]["status"], "success",
        "first valid finding should succeed"
    );
    assert_eq!(items[0]["new_status"], "acknowledged");

    // Second should fail (fake UUID)
    assert_eq!(items[1]["status"], "error", "fake UUID should report error");
    assert!(
        items[1]["error"].is_string(),
        "error field should describe the problem"
    );

    // Third should succeed (valid UUID)
    assert_eq!(
        items[2]["status"], "success",
        "second valid finding should succeed"
    );
    assert_eq!(items[2]["new_status"], "acknowledged");
}
