//! In-process MCP server unit tests — call tool methods directly for coverage.
//!
//! These tests call `TallyMcpServer` methods directly (in-process) to get
//! coverage instrumentation, unlike the subprocess-based `mcp_test.rs`.

use rmcp::handler::server::wrapper::Parameters;
use tally_ng::mcp::server::{
    AddNoteInput, BatchFindingInput, ExportFindingsInput, GetContextInput, ImportFindingsInput,
    LocationInput, QueryFindingsInput, RecordBatchInput, RecordFindingInput, SuppressFindingInput,
    SyncFindingsInput, TagInput, TallyMcpServer, UpdateFindingInput, UpdateStatusInput,
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

// =============================================================================
// record_finding
// =============================================================================

#[tokio::test]
async fn mcp_unit_record_creates_finding() {
    let (_tmp, server) = setup_mcp();
    let input = make_record_input("src/main.rs", 42, "critical", "test finding", "test-rule");
    let result = server
        .record_finding(Parameters(input))
        .await
        .expect("record");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "created");
    assert!(json["uuid"].is_string());
}

#[tokio::test]
async fn mcp_unit_record_deduplicates() {
    let (_tmp, server) = setup_mcp();
    let input1 = make_record_input("src/main.rs", 42, "critical", "test", "rule-a");
    server
        .record_finding(Parameters(input1))
        .await
        .expect("first record");

    let input2 = make_record_input("src/main.rs", 42, "critical", "test", "rule-a");
    let result = server
        .record_finding(Parameters(input2))
        .await
        .expect("second record");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "deduplicated");
}

#[tokio::test]
async fn mcp_unit_record_related_finding() {
    let (_tmp, server) = setup_mcp();
    // Record at line 42
    let input1 = make_record_input("src/main.rs", 42, "critical", "first", "rule-a");
    server
        .record_finding(Parameters(input1))
        .await
        .expect("first");

    // Record same rule at line 44 (within proximity threshold of 5)
    let input2 = make_record_input("src/main.rs", 44, "critical", "second", "rule-a");
    let result = server
        .record_finding(Parameters(input2))
        .await
        .expect("second");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "created");
    assert!(json["related_to"].is_string(), "should have related_to");
    assert!(json["distance"].is_number(), "should have distance");
}

#[tokio::test]
async fn mcp_unit_record_invalid_severity() {
    let (_tmp, server) = setup_mcp();
    let input = make_record_input("src/main.rs", 42, "ultra-critical", "test", "rule-a");
    let err = server
        .record_finding(Parameters(input))
        .await
        .expect_err("should fail");
    assert!(
        err.message.contains("invalid severity"),
        "error should mention invalid severity"
    );
}

#[tokio::test]
async fn mcp_unit_record_with_locations() {
    let (_tmp, server) = setup_mcp();
    let input = RecordFindingInput {
        file_path: "src/main.rs".into(),
        line_start: 10,
        line_end: None,
        severity: "important".into(),
        title: "multi-loc".into(),
        rule_id: "rule-a".into(),
        description: None,
        agent: None,
        suggested_fix: Some("fix it".into()),
        evidence: Some("proof".into()),
        locations: Some(vec![
            LocationInput {
                file_path: "src/other.rs".into(),
                line_start: 20,
                line_end: Some(25),
                role: Some("secondary".into()),
            },
            LocationInput {
                file_path: "docs/spec.md".into(),
                line_start: 5,
                line_end: None,
                role: Some("context".into()),
            },
        ]),
        category: None,
        tags: None,
        pr_number: None,
        session_id: None,
        related_to: None,
        relationship_type: None,
    };
    let result = server
        .record_finding(Parameters(input))
        .await
        .expect("record");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "created");
}

#[tokio::test]
async fn mcp_unit_record_with_suggested_fix_and_evidence() {
    let (_tmp, server) = setup_mcp();
    let input = RecordFindingInput {
        file_path: "src/main.rs".into(),
        line_start: 10,
        line_end: None,
        severity: "suggestion".into(),
        title: "use ? operator".into(),
        rule_id: "unwrap-usage".into(),
        description: Some("found unwrap".into()),
        agent: Some("claude-code".into()),
        suggested_fix: Some("replace unwrap() with ?".into()),
        evidence: Some("line 10: x.unwrap()".into()),
        locations: None,
        category: None,
        tags: None,
        pr_number: None,
        session_id: None,
        related_to: None,
        relationship_type: None,
    };
    let result = server
        .record_finding(Parameters(input))
        .await
        .expect("record");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "created");
}

// =============================================================================
// query_findings
// =============================================================================

#[tokio::test]
async fn mcp_unit_query_empty() {
    let (_tmp, server) = setup_mcp();
    let input = QueryFindingsInput {
        status: None,
        severity: None,
        file: None,
        rule: None,
        limit: None,
        tag: None,
        filter: None,
        sort: None,
        since: None,
        before: None,
        agent: None,
        category: None,
        text: None,
    };
    let result = server
        .query_findings(Parameters(input))
        .await
        .expect("query");
    let text = extract_tool_text(&result);
    assert!(text.contains("[]"), "empty store should return []");
}

#[tokio::test]
async fn mcp_unit_query_with_severity_filter() {
    let (_tmp, server) = setup_mcp();

    // Record critical and suggestion
    server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "crit", "r1",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "b.rs",
            2,
            "suggestion",
            "sug",
            "r2",
        )))
        .await
        .expect("record");

    // Query critical only
    let input = QueryFindingsInput {
        status: None,
        severity: Some("critical".into()),
        file: None,
        rule: None,
        limit: None,
        tag: None,
        filter: None,
        sort: None,
        since: None,
        before: None,
        agent: None,
        category: None,
        text: None,
    };
    let result = server
        .query_findings(Parameters(input))
        .await
        .expect("query");
    let text = extract_tool_text(&result);
    assert!(text.contains("crit"), "should have critical finding");
    assert!(!text.contains("sug"), "should not have suggestion finding");
}

#[tokio::test]
async fn mcp_unit_query_with_file_filter() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "src/api.rs",
            10,
            "important",
            "api",
            "r1",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "src/db.rs",
            20,
            "important",
            "db",
            "r2",
        )))
        .await
        .expect("record");

    let input = QueryFindingsInput {
        status: None,
        severity: None,
        file: Some("api".into()),
        rule: None,
        limit: None,
        tag: None,
        filter: None,
        sort: None,
        since: None,
        before: None,
        agent: None,
        category: None,
        text: None,
    };
    let result = server
        .query_findings(Parameters(input))
        .await
        .expect("query");
    let text = extract_tool_text(&result);
    assert!(text.contains("api"), "should match api file");
    assert!(!text.contains("\"db\""), "should not match db file");
}

#[tokio::test]
async fn mcp_unit_query_with_rule_filter() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "sql",
            "sql-injection",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "b.rs",
            2,
            "critical",
            "xss",
            "xss-attack",
        )))
        .await
        .expect("record");

    let input = QueryFindingsInput {
        status: None,
        severity: None,
        file: None,
        rule: Some("sql-injection".into()),
        limit: None,
        tag: None,
        filter: None,
        sort: None,
        since: None,
        before: None,
        agent: None,
        category: None,
        text: None,
    };
    let result = server
        .query_findings(Parameters(input))
        .await
        .expect("query");
    let text = extract_tool_text(&result);
    assert!(text.contains("sql"), "should match sql rule");
    assert!(!text.contains("xss"), "should not match xss rule");
}

#[tokio::test]
async fn mcp_unit_query_with_limit() {
    let (_tmp, server) = setup_mcp();

    for i in 0..5 {
        server
            .record_finding(Parameters(make_record_input(
                &format!("f{i}.rs"),
                1,
                "critical",
                &format!("finding {i}"),
                &format!("rule-{i}"),
            )))
            .await
            .expect("record");
    }

    let input = QueryFindingsInput {
        status: None,
        severity: None,
        file: None,
        rule: None,
        limit: Some(3),
        tag: None,
        filter: None,
        sort: None,
        since: None,
        before: None,
        agent: None,
        category: None,
        text: None,
    };
    let result = server
        .query_findings(Parameters(input))
        .await
        .expect("query");
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&result)).expect("parse");
    assert_eq!(findings.len(), 3, "should return only 3 findings");
}

// =============================================================================
// update_finding_status
// =============================================================================

#[tokio::test]
async fn mcp_unit_update_valid_transition() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = UpdateStatusInput {
        finding_id: uuid,
        new_status: "in_progress".into(),
        reason: Some("fixing now".into()),
        agent: Some("claude-code".into()),
        commit_sha: None,
        related_to: None,
        relationship: None,
    };
    let result = server
        .update_finding_status(Parameters(input))
        .await
        .expect("update");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "in_progress");
}

#[tokio::test]
async fn mcp_unit_update_invalid_transition() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = UpdateStatusInput {
        finding_id: uuid,
        new_status: "closed".into(),
        reason: None,
        agent: None,
        commit_sha: None,
        related_to: None,
        relationship: None,
    };
    let err = server
        .update_finding_status(Parameters(input))
        .await
        .expect_err("should fail");
    assert!(
        err.message.contains("Invalid transition"),
        "should mention invalid transition"
    );
}

#[tokio::test]
async fn mcp_unit_update_with_short_id() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");

    // Short ID C1 should resolve (first critical finding)
    let input = UpdateStatusInput {
        finding_id: "C1".into(),
        new_status: "acknowledged".into(),
        reason: None,
        agent: None,
        commit_sha: None,
        related_to: None,
        relationship: None,
    };
    let result = server
        .update_finding_status(Parameters(input))
        .await
        .expect("update via short ID");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "acknowledged");
}

#[tokio::test]
async fn mcp_unit_update_invalid_id() {
    let (_tmp, server) = setup_mcp();

    let input = UpdateStatusInput {
        finding_id: "not-a-valid-id".into(),
        new_status: "acknowledged".into(),
        reason: None,
        agent: None,
        commit_sha: None,
        related_to: None,
        relationship: None,
    };
    let err = server
        .update_finding_status(Parameters(input))
        .await
        .expect_err("should fail");
    assert!(
        err.message.contains("not found") || err.message.contains("Invalid"),
        "should mention not found"
    );
}

// =============================================================================
// get_finding_context
// =============================================================================

#[tokio::test]
async fn mcp_unit_get_context_by_uuid() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "test finding",
            "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = GetContextInput {
        finding_id: uuid.clone(),
    };
    let result = server
        .get_finding_context(Parameters(input))
        .await
        .expect("get context");
    let text = extract_tool_text(&result);
    assert!(text.contains(&uuid), "should contain the UUID");
    assert!(text.contains("test finding"), "should contain title");
}

#[tokio::test]
async fn mcp_unit_get_context_by_short_id() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "important",
            "test",
            "r1",
        )))
        .await
        .expect("record");

    let input = GetContextInput {
        finding_id: "I1".into(),
    };
    let result = server
        .get_finding_context(Parameters(input))
        .await
        .expect("get context via short ID");
    let text = extract_tool_text(&result);
    assert!(text.contains("test"), "should contain finding data");
}

#[tokio::test]
async fn mcp_unit_get_context_not_found() {
    let (_tmp, server) = setup_mcp();

    let input = GetContextInput {
        finding_id: "00000000-0000-0000-0000-000000000000".into(),
    };
    let err = server
        .get_finding_context(Parameters(input))
        .await
        .expect_err("should fail");
    assert!(!err.message.is_empty());
}

// =============================================================================
// suppress_finding
// =============================================================================

#[tokio::test]
async fn mcp_unit_suppress() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = SuppressFindingInput {
        finding_id: uuid,
        reason: "accepted risk".into(),
        expires_at: None,
        agent: None,
        suppression_type: None,
        suppression_pattern: None,
    };
    let result = server
        .suppress_finding(Parameters(input))
        .await
        .expect("suppress");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "suppressed");
}

#[tokio::test]
async fn mcp_unit_suppress_with_expiry() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = SuppressFindingInput {
        finding_id: uuid,
        reason: "temporary".into(),
        expires_at: Some("2030-12-31T23:59:59Z".into()),
        agent: Some("claude-code".into()),
        suppression_type: None,
        suppression_pattern: None,
    };
    let result = server
        .suppress_finding(Parameters(input))
        .await
        .expect("suppress");
    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "suppressed");
    assert!(json["expires_at"].is_string(), "should include expires_at");
}

#[tokio::test]
async fn mcp_unit_suppress_invalid_date() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let input = SuppressFindingInput {
        finding_id: uuid,
        reason: "test".into(),
        expires_at: Some("not-a-date".into()),
        agent: None,
        suppression_type: None,
        suppression_pattern: None,
    };
    let err = server
        .suppress_finding(Parameters(input))
        .await
        .expect_err("should fail");
    assert!(
        err.message.contains("Invalid date"),
        "should mention invalid date"
    );
}

#[tokio::test]
async fn mcp_unit_suppress_from_invalid_state() {
    let (_tmp, server) = setup_mcp();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // Transition to in_progress -> resolved -> closed (terminal)
    server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid.clone(),
            new_status: "in_progress".into(),
            reason: None,
            agent: None,
            commit_sha: None,
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("to in_progress");
    server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid.clone(),
            new_status: "resolved".into(),
            reason: None,
            agent: None,
            commit_sha: None,
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("to resolved");
    server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid.clone(),
            new_status: "closed".into(),
            reason: None,
            agent: None,
            commit_sha: None,
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("to closed");

    // Try to suppress from closed
    let err = server
        .suppress_finding(Parameters(SuppressFindingInput {
            finding_id: uuid,
            reason: "test".into(),
            expires_at: None,
            agent: None,
            suppression_type: None,
            suppression_pattern: None,
        }))
        .await
        .expect_err("should fail");
    assert!(
        err.message.contains("Cannot suppress"),
        "should say cannot suppress"
    );
}

// =============================================================================
// record_batch
// =============================================================================

#[tokio::test]
async fn mcp_unit_batch_all_succeed() {
    let (_tmp, server) = setup_mcp();

    let input = RecordBatchInput {
        findings: vec![
            BatchFindingInput {
                file_path: "a.rs".into(),
                line_start: 1,
                line_end: None,
                severity: "critical".into(),
                title: "A".into(),
                rule_id: "r1".into(),
                description: None,
                suggested_fix: None,
                evidence: None,
                category: None,
                tags: None,
                pr_number: None,
                session_id: None,
            },
            BatchFindingInput {
                file_path: "b.rs".into(),
                line_start: 2,
                line_end: None,
                severity: "suggestion".into(),
                title: "B".into(),
                rule_id: "r2".into(),
                description: Some("desc".into()),
                suggested_fix: None,
                evidence: None,
                category: None,
                tags: None,
                pr_number: None,
                session_id: None,
            },
        ],
        agent: Some("test-agent".into()),
        pr_number: None,
        session_id: None,
    };
    let result = server.record_batch(Parameters(input)).await.expect("batch");
    let json = extract_tool_json(&result);
    assert_eq!(json["total"], 2);
    assert_eq!(json["succeeded"], 2);
    assert_eq!(json["failed"], 0);
}

#[tokio::test]
async fn mcp_unit_batch_partial_failure() {
    let (_tmp, server) = setup_mcp();

    let input = RecordBatchInput {
        findings: vec![
            BatchFindingInput {
                file_path: "a.rs".into(),
                line_start: 1,
                line_end: None,
                severity: "critical".into(),
                title: "ok".into(),
                rule_id: "r1".into(),
                description: None,
                suggested_fix: None,
                evidence: None,
                category: None,
                tags: None,
                pr_number: None,
                session_id: None,
            },
            BatchFindingInput {
                file_path: "b.rs".into(),
                line_start: 2,
                line_end: None,
                severity: "invalid-severity".into(),
                title: "bad".into(),
                rule_id: "r2".into(),
                description: None,
                suggested_fix: None,
                evidence: None,
                category: None,
                tags: None,
                pr_number: None,
                session_id: None,
            },
        ],
        agent: None,
        pr_number: None,
        session_id: None,
    };
    let result = server.record_batch(Parameters(input)).await.expect("batch");
    let json = extract_tool_json(&result);
    assert_eq!(json["total"], 2);
    assert_eq!(json["succeeded"], 1);
    assert_eq!(json["failed"], 1);
}

#[tokio::test]
async fn mcp_unit_batch_dedup() {
    let (_tmp, server) = setup_mcp();

    // Record one finding first
    server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");

    // Batch with same finding
    let input = RecordBatchInput {
        findings: vec![BatchFindingInput {
            file_path: "a.rs".into(),
            line_start: 1,
            line_end: None,
            severity: "critical".into(),
            title: "test".into(),
            rule_id: "r1".into(),
            description: None,
            suggested_fix: None,
            evidence: None,
            category: None,
            tags: None,
            pr_number: None,
            session_id: None,
        }],
        agent: None,
        pr_number: None,
        session_id: None,
    };
    let result = server.record_batch(Parameters(input)).await.expect("batch");
    let json = extract_tool_json(&result);
    let results = json["results"].as_array().expect("results array");
    let inner: serde_json::Value =
        serde_json::from_value(results[0]["result"].clone()).expect("inner");
    assert_eq!(inner["status"], "deduplicated");
}

// =============================================================================
// Resources (via ServerHandler trait)
// These use the read_resource_* helper functions indirectly
// =============================================================================

// Resource helper functions are pub for testability. The ServerHandler methods
// that call them (list_resources, read_resource) need RequestContext which is
// hard to construct — those are tested via subprocess in mcp_test.rs.

#[tokio::test]
async fn mcp_unit_resource_summary() {
    let (_tmp, server) = setup_mcp();
    let repo_path = &server.repo_path().to_string();

    // Record 2 findings
    server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "crit finding",
            "r1",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "b.rs",
            2,
            "suggestion",
            "sug finding",
            "r2",
        )))
        .await
        .expect("record");

    let store = GitFindingsStore::open(repo_path).expect("open");
    let summary = tally_ng::mcp::server::read_resource_summary(&store).expect("summary");
    let json: serde_json::Value = serde_json::from_str(&summary).expect("parse");

    assert_eq!(json["total"], 2);
    assert!(json["by_severity"].is_object());
    assert!(json["recent"].is_array());
    let recent = json["recent"].as_array().expect("recent array");
    assert_eq!(recent.len(), 2);
}

#[tokio::test]
async fn mcp_unit_resource_summary_empty() {
    let (_tmp, server) = setup_mcp();
    let repo_path = &server.repo_path().to_string();

    let store = GitFindingsStore::open(repo_path).expect("open");
    let summary = tally_ng::mcp::server::read_resource_summary(&store).expect("summary");
    let json: serde_json::Value = serde_json::from_str(&summary).expect("parse");

    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn mcp_unit_resource_file() {
    let (_tmp, server) = setup_mcp();
    let repo_path = &server.repo_path().to_string();

    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            10,
            "critical",
            "main issue",
            "r1",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "src/other.rs",
            20,
            "important",
            "other issue",
            "r2",
        )))
        .await
        .expect("record");

    let store = GitFindingsStore::open(repo_path).expect("open");

    // Query for main.rs
    let result = tally_ng::mcp::server::read_resource_file(&store, "src/main.rs").expect("file");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0]["title"]
            .as_str()
            .expect("title")
            .contains("main")
    );

    // Query for nonexistent file
    let result =
        tally_ng::mcp::server::read_resource_file(&store, "nonexistent.rs").expect("file empty");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 0, "nonexistent file should return empty");
}

#[tokio::test]
async fn mcp_unit_resource_detail() {
    let (_tmp, server) = setup_mcp();
    let repo_path = &server.repo_path().to_string();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "detailed finding",
            "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let store = GitFindingsStore::open(repo_path).expect("open");
    let result = tally_ng::mcp::server::read_resource_detail(&store, &uuid).expect("detail");
    let json: serde_json::Value = serde_json::from_str(&result).expect("parse");
    assert_eq!(json["title"], "detailed finding");
    assert_eq!(json["uuid"], uuid);
}

#[tokio::test]
async fn mcp_unit_resource_detail_not_found() {
    let (_tmp, server) = setup_mcp();
    let repo_path = &server.repo_path().to_string();

    let store = GitFindingsStore::open(repo_path).expect("open");
    let err =
        tally_ng::mcp::server::read_resource_detail(&store, "00000000-0000-0000-0000-000000000000");
    assert!(err.is_err(), "nonexistent UUID should error");
}

// =============================================================================
// New resource templates: severity, status, rule
// =============================================================================

#[tokio::test]
async fn mcp_unit_resource_by_severity() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "crit", "r1",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "b.rs",
            2,
            "suggestion",
            "sug",
            "r2",
        )))
        .await
        .expect("record");

    let store = GitFindingsStore::open(&repo_path).expect("open");

    let result =
        tally_ng::mcp::server::read_resource_by_severity(&store, "critical").expect("severity");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 1);
    assert!(findings[0]["title"].as_str().expect("t").contains("crit"));

    let result =
        tally_ng::mcp::server::read_resource_by_severity(&store, "suggestion").expect("severity");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 1);

    let result =
        tally_ng::mcp::server::read_resource_by_severity(&store, "invalid").expect("severity");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 0, "invalid severity returns empty");
}

#[tokio::test]
async fn mcp_unit_resource_by_status() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    let record_result = server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "test", "r1",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record_result)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // Update to in_progress
    server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid,
            new_status: "in_progress".into(),
            reason: None,
            agent: None,
            commit_sha: None,
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("update");

    let store = GitFindingsStore::open(&repo_path).expect("open");

    let result =
        tally_ng::mcp::server::read_resource_by_status(&store, "in_progress").expect("status");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 1);

    let result = tally_ng::mcp::server::read_resource_by_status(&store, "open").expect("status");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 0, "no open findings after update");
}

#[tokio::test]
async fn mcp_unit_resource_by_rule() {
    let (_tmp, server) = setup_mcp();
    let repo_path = server.repo_path().to_string();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "sql issue",
            "sql-injection",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "b.rs",
            2,
            "important",
            "xss issue",
            "xss-attack",
        )))
        .await
        .expect("record");

    let store = GitFindingsStore::open(&repo_path).expect("open");

    let result =
        tally_ng::mcp::server::read_resource_by_rule(&store, "sql-injection").expect("rule");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 1);
    assert!(findings[0]["title"].as_str().expect("t").contains("sql"));

    let result =
        tally_ng::mcp::server::read_resource_by_rule(&store, "nonexistent-rule").expect("rule");
    let findings: Vec<serde_json::Value> = serde_json::from_str(&result).expect("parse");
    assert_eq!(findings.len(), 0);
}

// =============================================================================
// Prompt tests
// =============================================================================

#[tokio::test]
async fn mcp_unit_prompt_triage_file() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            10,
            "critical",
            "sql injection",
            "sql-injection",
        )))
        .await
        .expect("record");
    server
        .record_finding(Parameters(make_record_input(
            "src/main.rs",
            20,
            "suggestion",
            "use const",
            "use-const",
        )))
        .await
        .expect("record");

    let result = server
        .triage_file(Parameters(tally_ng::mcp::server::TriageFileArgs {
            file_path: "src/main.rs".into(),
        }))
        .await
        .expect("triage prompt");

    assert_eq!(result.len(), 1, "should return 1 prompt message");
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(text.contains("src/main.rs"), "should mention file");
    assert!(
        text.contains("triage"),
        "should contain triage instructions"
    );
    assert!(
        text.contains("sql injection"),
        "should contain finding data"
    );
}

#[tokio::test]
async fn mcp_unit_prompt_fix_finding() {
    let (_tmp, server) = setup_mcp();

    let record = server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "unwrap usage",
            "unsafe-unwrap",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let result = server
        .fix_finding(Parameters(tally_ng::mcp::server::FixFindingArgs {
            finding_id: uuid,
        }))
        .await
        .expect("fix prompt");

    assert_eq!(result.len(), 1);
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(text.contains("unwrap usage"), "should contain finding");
    assert!(text.contains("code change"), "should ask for fix");
}

#[tokio::test]
async fn mcp_unit_prompt_summarize_findings() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs", 1, "critical", "crit", "r1",
        )))
        .await
        .expect("record");

    let result = server.summarize_findings().await.expect("summarize prompt");

    assert_eq!(result.len(), 1);
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(text.contains("stakeholder"), "should mention stakeholder");
    assert!(text.contains("total"), "should contain summary data");
}

#[tokio::test]
async fn mcp_unit_prompt_review_pr() {
    let (_tmp, server) = setup_mcp();

    server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "critical",
            "blocking issue",
            "r1",
        )))
        .await
        .expect("record");

    let result = server.review_pr().await.expect("review prompt");

    assert_eq!(result.len(), 1);
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(text.contains("PR review"), "should mention PR review");
    assert!(text.contains("blocking issue"), "should contain finding");
}

#[tokio::test]
async fn mcp_unit_prompt_explain_finding() {
    let (_tmp, server) = setup_mcp();

    let record = server
        .record_finding(Parameters(make_record_input(
            "a.rs",
            1,
            "important",
            "missing auth",
            "missing-auth",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    let result = server
        .explain_finding(Parameters(tally_ng::mcp::server::ExplainFindingArgs {
            finding_id: uuid,
        }))
        .await
        .expect("explain prompt");

    assert_eq!(result.len(), 1);
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(text.contains("missing auth"), "should contain finding");
    assert!(
        text.contains("plain language"),
        "should ask for explanation"
    );
}

// =============================================================================
// initialize_store
// =============================================================================

#[tokio::test]
async fn mcp_unit_initialize_store_idempotent() {
    // setup_mcp already calls init, so calling again should be idempotent
    let (_tmp, server) = setup_mcp();
    let result = server.initialize_store().await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("initialized"), "should report initialized");
}

#[tokio::test]
async fn mcp_unit_initialize_store_fresh_repo() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_path = tmp.path().to_str().expect("path").to_string();

    // git init + initial commit, but do NOT call store.init()
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

    let server = TallyMcpServer::new(repo_path);
    let result = server.initialize_store().await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("initialized"), "should report initialized");
}

// =============================================================================
// export_findings
// =============================================================================

async fn record_sample(server: &TallyMcpServer) {
    let input = make_record_input(
        "src/main.rs",
        42,
        "critical",
        "test finding",
        "unsafe-unwrap",
    );
    server
        .record_finding(Parameters(input))
        .await
        .expect("record");
}

#[tokio::test]
async fn mcp_unit_export_json() {
    let (_tmp, server) = setup_mcp();
    record_sample(&server).await;

    let result = server
        .export_findings(Parameters(ExportFindingsInput {
            format: "json".into(),
        }))
        .await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("unsafe-unwrap"), "should contain finding");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert!(parsed.is_array(), "should be array");
}

#[tokio::test]
async fn mcp_unit_export_csv() {
    let (_tmp, server) = setup_mcp();
    record_sample(&server).await;

    let result = server
        .export_findings(Parameters(ExportFindingsInput {
            format: "csv".into(),
        }))
        .await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(
        text.contains("uuid,severity,status,rule_id,file_path"),
        "should have CSV header"
    );
}

#[tokio::test]
async fn mcp_unit_export_sarif() {
    let (_tmp, server) = setup_mcp();
    record_sample(&server).await;

    let result = server
        .export_findings(Parameters(ExportFindingsInput {
            format: "sarif".into(),
        }))
        .await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("sarif-schema-2.1.0"), "should be SARIF");
}

#[tokio::test]
async fn mcp_unit_export_invalid_format() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .export_findings(Parameters(ExportFindingsInput {
            format: "xml".into(),
        }))
        .await;
    assert!(result.is_err(), "should reject unknown format");
}

// =============================================================================
// sync_findings
// =============================================================================

#[tokio::test]
async fn mcp_unit_sync_no_remote_fails() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .sync_findings(Parameters(SyncFindingsInput { remote: None }))
        .await;
    assert!(result.is_err(), "should fail without remote");
}

// =============================================================================
// rebuild_index
// =============================================================================

#[tokio::test]
async fn mcp_unit_rebuild_index() {
    let (_tmp, server) = setup_mcp();
    record_sample(&server).await;

    let result = server.rebuild_index().await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("rebuilt"), "should report rebuilt");
}

// =============================================================================
// import_findings
// =============================================================================

#[tokio::test]
async fn mcp_unit_import_dclaude_format() {
    let (_tmp, server) = setup_mcp();

    let import_file = tempfile::NamedTempFile::new().expect("temp file");
    let state = serde_json::json!({
        "active_cycle": {
            "findings": [
                {
                    "id": "C1",
                    "title": "SQL injection",
                    "file": "src/api.rs",
                    "lines": [42],
                    "severity": "critical",
                    "category": "injection",
                    "status": "open"
                },
                {
                    "id": "I1",
                    "title": "Missing validation",
                    "file": "src/api.rs",
                    "lines": [10],
                    "severity": "important",
                    "status": "verified"
                }
            ]
        }
    });
    std::fs::write(import_file.path(), state.to_string()).expect("write");

    let result = server
        .import_findings(Parameters(ImportFindingsInput {
            file_path: import_file.path().to_str().expect("path").into(),
        }))
        .await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed["imported"], 2);
    assert_eq!(parsed["skipped"], 0);
}

#[tokio::test]
async fn mcp_unit_import_missing_file() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .import_findings(Parameters(ImportFindingsInput {
            file_path: "/nonexistent/file.json".into(),
        }))
        .await;
    assert!(result.is_err(), "should fail for missing file");
}

#[tokio::test]
async fn mcp_unit_import_no_findings() {
    let (_tmp, server) = setup_mcp();

    let import_file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(import_file.path(), r#"{"other": "data"}"#).expect("write");

    let result = server
        .import_findings(Parameters(ImportFindingsInput {
            file_path: import_file.path().to_str().expect("path").into(),
        }))
        .await;
    assert!(result.is_ok());
    let text = extract_tool_text(&result.expect("ok"));
    assert!(text.contains("no_findings"), "should report no findings");
}

// =============================================================================
// Phase 3: update_finding, add_note, add_tag, remove_tag MCP tools
// =============================================================================

/// Helper to record a finding and return its UUID.
async fn record_and_get_uuid(server: &TallyMcpServer) -> String {
    let result = server
        .record_finding(Parameters(make_record_input(
            "src/lib.rs",
            10,
            "important",
            "test finding",
            "test-rule",
        )))
        .await
        .expect("record");
    let json = extract_tool_json(&result);
    json["uuid"].as_str().expect("uuid").to_string()
}

#[tokio::test]
async fn mcp_unit_update_finding_changes_description() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid.clone(),
            title: None,
            description: Some("updated desc".into()),
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await
        .expect("update");

    let json = extract_tool_json(&result);
    assert_eq!(json["description"], "updated desc");
}

#[tokio::test]
async fn mcp_unit_update_finding_changes_multiple_fields() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid,
            title: Some("new title".into()),
            description: Some("new desc".into()),
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await
        .expect("update");

    let json = extract_tool_json(&result);
    assert_eq!(json["title"], "new title");
    assert_eq!(json["description"], "new desc");
    assert_eq!(json["edit_history"].as_array().expect("array").len(), 2);
}

#[tokio::test]
async fn mcp_unit_update_finding_no_fields_returns_error() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid,
            title: None,
            description: None,
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await;

    assert!(result.is_err());
    let err = result.expect_err("should error");
    assert!(
        err.message.contains("At least one field"),
        "error: {}",
        err.message
    );
}

#[tokio::test]
async fn mcp_unit_update_finding_invalid_severity() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid,
            title: None,
            description: None,
            suggested_fix: None,
            evidence: None,
            severity: Some("ultra".into()),
            category: None,
            tags: None,
            agent: None,
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn mcp_unit_add_note_status_unchanged() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid,
            note: "Covered by Story 1.21".into(),
            agent: Some("dclaude:check-drift".into()),
        }))
        .await
        .expect("add_note");

    let json = extract_tool_json(&result);
    assert_eq!(json["status"], "open");
    assert_eq!(json["notes_count"], 1);
}

#[tokio::test]
async fn mcp_unit_add_note_twice_gives_two_notes() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid.clone(),
            note: "first".into(),
            agent: None,
        }))
        .await
        .expect("note1");

    let result = server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid,
            note: "second".into(),
            agent: None,
        }))
        .await
        .expect("note2");

    let json = extract_tool_json(&result);
    assert_eq!(json["notes_count"], 2);
}

#[tokio::test]
async fn mcp_unit_add_note_empty_text_returns_error() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid,
            note: String::new(),
            agent: None,
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn mcp_unit_add_tag_merges_without_duplicates() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    // Add tags
    server
        .add_tag(Parameters(TagInput {
            finding_id: uuid.clone(),
            tags: vec!["story:1.21".into(), "wave-1".into()],
            agent: None,
        }))
        .await
        .expect("add_tag");

    // Add again — duplicates should be ignored
    let result = server
        .add_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec!["story:1.21".into(), "new-tag".into()],
            agent: None,
        }))
        .await
        .expect("add_tag2");

    let json = extract_tool_json(&result);
    let tags = json["tags"].as_array().expect("tags array");
    assert_eq!(tags.len(), 3); // story:1.21, wave-1, new-tag
}

#[tokio::test]
async fn mcp_unit_remove_tag_removes_exact_matches() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    // Add tags first
    server
        .add_tag(Parameters(TagInput {
            finding_id: uuid.clone(),
            tags: vec!["a".into(), "b".into(), "c".into()],
            agent: None,
        }))
        .await
        .expect("add");

    let result = server
        .remove_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec!["b".into()],
            agent: None,
        }))
        .await
        .expect("remove");

    let json = extract_tool_json(&result);
    let tags: Vec<&str> = json["tags"]
        .as_array()
        .expect("arr")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(tags.contains(&"a"));
    assert!(!tags.contains(&"b"));
    assert!(tags.contains(&"c"));
}

#[tokio::test]
async fn mcp_unit_remove_tag_ignores_missing() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    // Remove tag that doesn't exist — should succeed, no-op
    let result = server
        .remove_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec!["nonexistent".into()],
            agent: None,
        }))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn mcp_unit_query_with_tag_filter() {
    let (_tmp, server) = setup_mcp();
    let uuid1 = record_and_get_uuid(&server).await;

    // Record another finding
    server
        .record_finding(Parameters(make_record_input(
            "src/other.rs",
            20,
            "suggestion",
            "other finding",
            "other-rule",
        )))
        .await
        .expect("record2");

    // Tag only the first
    server
        .add_tag(Parameters(TagInput {
            finding_id: uuid1,
            tags: vec!["story:1.21".into()],
            agent: None,
        }))
        .await
        .expect("tag");

    // Query with tag filter
    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: Some("story:1.21".into()),
            filter: None,
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query");

    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 1);
}

#[tokio::test]
async fn mcp_unit_add_tag_empty_tags_returns_error() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .add_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec![],
            agent: None,
        }))
        .await;

    assert!(result.is_err());
}

// =============================================================================
// Test gap coverage: get_finding_context with notes/edits, import defaults,
// rebuild index tags, update_finding error message
// =============================================================================

#[tokio::test]
async fn mcp_unit_get_context_includes_notes_and_edit_history() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    // Add a note
    server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid.clone(),
            note: "context note for test".into(),
            agent: Some("test-agent".into()),
        }))
        .await
        .expect("add_note");

    // Edit a field
    server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid.clone(),
            title: None,
            description: Some("updated via MCP".into()),
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await
        .expect("update_finding");

    // Get context — should include notes and edit_history
    let result = server
        .get_finding_context(Parameters(GetContextInput { finding_id: uuid }))
        .await
        .expect("get_context");
    let text = extract_tool_text(&result);

    assert!(
        text.contains("context note for test"),
        "get_context should include note text"
    );
    assert!(
        text.contains("edit_history"),
        "get_context should include edit_history key"
    );
}

#[tokio::test]
async fn mcp_unit_import_finding_gets_empty_notes_and_edit_history() {
    let (_tmp, server) = setup_mcp();

    let import_file = tempfile::NamedTempFile::new().expect("temp file");
    let state = serde_json::json!({
        "active_cycle": {
            "findings": [{
                "id": "I1",
                "title": "imported finding",
                "file": "src/lib.rs",
                "lines": [10],
                "severity": "important",
                "category": "test-import"
            }]
        }
    });
    std::fs::write(import_file.path(), state.to_string()).expect("write");

    server
        .import_findings(Parameters(ImportFindingsInput {
            file_path: import_file.path().to_str().expect("path").into(),
        }))
        .await
        .expect("import");

    // Query to get the imported finding
    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: Some("src/lib.rs".into()),
            rule: None,
            limit: None,
            tag: None,
            filter: None,
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert!(!findings.is_empty(), "should have imported finding");

    // Verify notes and edit_history are absent from JSON (empty = omitted)
    let finding_str = serde_json::to_string(&findings[0]).expect("serialize");
    assert!(
        !finding_str.contains("\"notes\""),
        "imported finding should not have notes key (empty = omitted)"
    );
    assert!(
        !finding_str.contains("\"edit_history\""),
        "imported finding should not have edit_history key (empty = omitted)"
    );
}

#[tokio::test]
async fn mcp_unit_rebuild_index_includes_tags() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    // Add tags
    server
        .add_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec!["story:1.21".into(), "wave-1".into()],
            agent: None,
        }))
        .await
        .expect("add_tag");

    // Rebuild index
    server.rebuild_index().await.expect("rebuild");

    // Verify tags survived through the rebuild by querying with tag filter
    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: Some("story:1.21".into()),
            filter: None,
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 1, "tagged finding should survive rebuild");
}

#[tokio::test]
async fn mcp_unit_update_finding_no_fields_error_is_actionable() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: uuid,
            title: None,
            description: None,
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await;
    assert!(result.is_err());
    let err = result.expect_err("should error");
    assert!(
        err.message.contains("At least one field"),
        "error should guide user: {}",
        err.message
    );
}

#[tokio::test]
async fn mcp_unit_update_finding_nonexistent_returns_error() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .update_finding(Parameters(UpdateFindingInput {
            finding_id: "00000000-0000-0000-0000-000000000000".into(),
            title: Some("x".into()),
            description: None,
            suggested_fix: None,
            evidence: None,
            severity: None,
            category: None,
            tags: None,
            agent: None,
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mcp_unit_add_note_nonexistent_returns_error() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .add_note(Parameters(AddNoteInput {
            finding_id: "00000000-0000-0000-0000-000000000000".into(),
            note: "test".into(),
            agent: None,
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mcp_unit_remove_tag_on_finding_with_no_tags() {
    let (_tmp, server) = setup_mcp();
    let uuid = record_and_get_uuid(&server).await;

    let result = server
        .remove_tag(Parameters(TagInput {
            finding_id: uuid,
            tags: vec!["nonexistent".into()],
            agent: None,
        }))
        .await;
    assert!(result.is_ok());
    let json = extract_tool_json(&result.expect("ok"));
    let tags = json["tags"].as_array().expect("tags");
    assert!(tags.is_empty());
}

// =============================================================================
// TallyQL query via MCP
// =============================================================================

#[tokio::test]
async fn mcp_unit_query_with_filter_expression() {
    let (_tmp, server) = setup_mcp();
    // Record two findings with different severities
    record_finding_with_severity(&server, "critical").await;
    record_finding_with_severity_and_file(&server, "suggestion", "tests/foo.rs").await;

    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: Some("severity = critical".into()),
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query with filter");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 1);
}

#[tokio::test]
async fn mcp_unit_query_with_sort() {
    let (_tmp, server) = setup_mcp();
    record_finding_with_severity(&server, "suggestion").await;
    record_finding_with_severity(&server, "critical").await;

    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: None,
            sort: Some("-severity".into()),
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query with sort");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 2);
    // First should be critical (desc sort)
    assert_eq!(findings[0]["severity"], "critical");
}

#[tokio::test]
async fn mcp_unit_query_with_text_search() {
    let (_tmp, server) = setup_mcp();
    record_and_get_uuid(&server).await; // title: "test finding"

    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: None,
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: Some("test finding".into()),
        }))
        .await
        .expect("query with text");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 1);
}

#[tokio::test]
async fn mcp_unit_query_with_since() {
    let (_tmp, server) = setup_mcp();
    record_and_get_uuid(&server).await;

    // Since 1 hour ago — should include the just-created finding
    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: None,
            sort: None,
            since: Some("1h".into()),
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query with since");
    let json = extract_tool_json(&result);
    let findings = json.as_array().expect("array");
    assert_eq!(findings.len(), 1);
}

#[tokio::test]
async fn mcp_unit_query_filter_parse_error() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: Some("foo = bar".into()),
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await;
    assert!(result.is_err(), "invalid TallyQL should return MCP error");
}

#[tokio::test]
async fn mcp_unit_query_invalid_sort_field() {
    let (_tmp, server) = setup_mcp();

    let result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: None,
            filter: None,
            sort: Some("nonexistent".into()),
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await;
    assert!(
        result.is_err(),
        "invalid sort field should return MCP error"
    );
}

/// Helper: record a finding with a specific severity.
async fn record_finding_with_severity(server: &TallyMcpServer, severity: &str) {
    server
        .record_finding(Parameters(RecordFindingInput {
            file_path: "src/lib.rs".into(),
            line_start: 1,
            line_end: None,
            severity: severity.into(),
            title: format!("{severity} finding"),
            rule_id: format!("rule-{severity}"),
            description: None,
            tags: None,
            agent: None,
            session_id: None,
            locations: None,
            related_to: None,
            relationship_type: None,
            category: None,
            suggested_fix: None,
            evidence: None,
            pr_number: None,
        }))
        .await
        .expect("record finding");
}

/// Helper: record a finding with severity and specific file.
async fn record_finding_with_severity_and_file(
    server: &TallyMcpServer,
    severity: &str,
    file: &str,
) {
    server
        .record_finding(Parameters(RecordFindingInput {
            file_path: file.into(),
            line_start: 1,
            line_end: None,
            severity: severity.into(),
            title: format!("{severity} finding in {file}"),
            rule_id: format!("rule-{severity}-{}", file.replace(['/', '.'], "-")),
            description: None,
            tags: None,
            agent: None,
            session_id: None,
            locations: None,
            related_to: None,
            relationship_type: None,
            category: None,
            suggested_fix: None,
            evidence: None,
            pr_number: None,
        }))
        .await
        .expect("record finding");
}
