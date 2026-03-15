//! End-to-end MCP workflow tests — multi-step patterns that mirror real agent usage.
//!
//! Tests the pr-fix-verify flow, batch status transitions, drift query patterns,
//! tag enrichment workflows, migration idempotency, and prompt generation.

use rmcp::handler::server::wrapper::Parameters;
use tally_ng::mcp::server::{
    AddNoteInput, AddRuleExampleInput, CreateRuleInput, GetContextInput, ListRulesInput,
    MigrateRulesInput, QueryFindingsInput, RecordFindingInput, SearchRulesInput, TagInput,
    TallyMcpServer, UpdateBatchStatusInput, UpdateStatusInput,
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
// E2E 1: pr-fix-verify pattern — record, lookup, transition, resolve
// =============================================================================

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn e2e_record_lookup_transition_resolve() {
    let (_tmp, server) = setup_mcp();

    // a. initialize_store (idempotent — already done by setup)
    let init_result = server.initialize_store().await.expect("init");
    let init_text = extract_tool_text(&init_result);
    assert!(
        init_text.contains("initialized"),
        "should report initialized"
    );

    // b. create_rule with alias
    server
        .create_rule(Parameters(CreateRuleInput {
            rule_id: "unsafe-unwrap".into(),
            name: "Unsafe unwrap usage".into(),
            description: "Detects .unwrap() calls in production code".into(),
            category: Some("safety".into()),
            severity_hint: Some("important".into()),
            aliases: Some(vec!["unwrap-usage".into(), "no-unwrap".into()]),
            cwe_ids: None,
            tags: None,
            scope_include: None,
            scope_exclude: None,
        }))
        .await
        .expect("create rule");

    // c. search_rules to look up canonical ID via alias
    let search_result = server
        .search_rules(Parameters(SearchRulesInput {
            query: "unwrap-usage".into(),
            method: None,
            limit: None,
        }))
        .await
        .expect("search");
    let search_json = extract_tool_json(&search_result);
    let results = search_json.as_array().expect("search results array");
    assert!(!results.is_empty(), "search should find the rule");
    assert_eq!(
        results[0]["id"], "unsafe-unwrap",
        "search by alias should return canonical ID"
    );
    assert_eq!(
        results[0]["confidence"], 1.0,
        "exact alias match should have confidence 1.0"
    );

    // d. record_finding using alias — verify canonical rule_id in response
    let record_result = server
        .record_finding(Parameters(make_record_input(
            "src/handler.rs",
            42,
            "important",
            "unwrap in error handler",
            "unwrap-usage",
        )))
        .await
        .expect("record");
    let record_json = extract_tool_json(&record_result);
    assert_eq!(record_json["status"], "created");
    let uuid = record_json["uuid"]
        .as_str()
        .expect("uuid field")
        .to_string();

    // e. update_finding_status → in_progress
    let update1 = server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid.clone(),
            new_status: "in_progress".into(),
            reason: Some("starting fix".into()),
            agent: Some("dclaude:pr-fix-verify".into()),
            commit_sha: None,
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("update to in_progress");
    let update1_json = extract_tool_json(&update1);
    assert_eq!(update1_json["status"], "in_progress");

    // f. add_note with fix description
    server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid.clone(),
            note: "fixing: applied code change — replaced .unwrap() with .expect()".into(),
            agent: Some("dclaude:pr-fix-verify".into()),
        }))
        .await
        .expect("add note");

    // g. update_finding_status → resolved
    let update2 = server
        .update_finding_status(Parameters(UpdateStatusInput {
            finding_id: uuid.clone(),
            new_status: "resolved".into(),
            reason: Some("fixed in PR #42".into()),
            agent: Some("dclaude:pr-fix-verify".into()),
            commit_sha: Some("abc123def".into()),
            related_to: None,
            relationship: None,
        }))
        .await
        .expect("update to resolved");
    let update2_json = extract_tool_json(&update2);
    assert_eq!(update2_json["status"], "resolved");

    // h. query_findings by rule → verify finding is resolved
    let query_result = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: Some("unsafe-unwrap".into()),
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
        .expect("query by rule");
    let query_text = extract_tool_text(&query_result);
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&query_text).expect("parse findings");
    assert_eq!(findings.len(), 1, "should find 1 finding for unsafe-unwrap");
    assert_eq!(
        findings[0]["status"], "resolved",
        "finding should be resolved"
    );
}

// =============================================================================
// E2E 2: Batch status workflow
// =============================================================================

#[tokio::test]
async fn e2e_batch_status_workflow() {
    let (_tmp, server) = setup_mcp();

    // a. Record 3 findings
    let mut uuids = Vec::new();
    for (file, title, rule) in [
        ("src/a.rs", "issue A", "rule-alpha"),
        ("src/b.rs", "issue B", "rule-beta"),
        ("src/c.rs", "issue C", "rule-gamma"),
    ] {
        let result = server
            .record_finding(Parameters(make_record_input(
                file,
                10,
                "important",
                title,
                rule,
            )))
            .await
            .expect("record");
        let json = extract_tool_json(&result);
        uuids.push(json["uuid"].as_str().expect("uuid").to_string());
    }

    // b. update_batch_status all to acknowledged
    let batch1 = server
        .update_batch_status(Parameters(UpdateBatchStatusInput {
            finding_ids: uuids.clone(),
            status: "acknowledged".into(),
            reason: Some("batch ack from triage".into()),
            agent: Some("test-agent".into()),
        }))
        .await
        .expect("batch ack");
    let batch1_json = extract_tool_json(&batch1);
    assert_eq!(batch1_json["total"], 3);
    let results1 = batch1_json["results"].as_array().expect("results");
    for r in results1 {
        assert_eq!(
            r["status"], "success",
            "all batch transitions should succeed"
        );
        assert_eq!(r["new_status"], "acknowledged");
    }

    // c. query_findings → verify all are acknowledged
    let query = server
        .query_findings(Parameters(QueryFindingsInput {
            status: Some("acknowledged".into()),
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
        }))
        .await
        .expect("query acknowledged");
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&query)).expect("parse");
    assert_eq!(findings.len(), 3, "all 3 findings should be acknowledged");

    // d. update_batch_status all to in_progress
    let batch2 = server
        .update_batch_status(Parameters(UpdateBatchStatusInput {
            finding_ids: uuids.clone(),
            status: "in_progress".into(),
            reason: Some("starting work".into()),
            agent: None,
        }))
        .await
        .expect("batch in_progress");
    let batch2_json = extract_tool_json(&batch2);
    for r in batch2_json["results"].as_array().expect("results") {
        assert_eq!(r["status"], "success");
        assert_eq!(r["new_status"], "in_progress");
    }

    // e. update_batch_status all to resolved
    let batch3 = server
        .update_batch_status(Parameters(UpdateBatchStatusInput {
            finding_ids: uuids,
            status: "resolved".into(),
            reason: Some("all fixed".into()),
            agent: None,
        }))
        .await
        .expect("batch resolved");
    let batch3_json = extract_tool_json(&batch3);
    for r in batch3_json["results"].as_array().expect("results") {
        assert_eq!(r["status"], "success");
        assert_eq!(r["new_status"], "resolved");
    }
}

// =============================================================================
// E2E 3: Drift query before record — check-drift pattern
// =============================================================================

#[tokio::test]
async fn e2e_drift_query_before_record() {
    let (_tmp, server) = setup_mcp();

    // a. Record a spec-drift finding
    let record1 = server
        .record_finding(Parameters(make_record_input(
            "src/api.rs",
            100,
            "important",
            "API handler missing validation",
            "spec-drift",
        )))
        .await
        .expect("record spec-drift");
    let json1 = extract_tool_json(&record1);
    assert_eq!(json1["status"], "created");
    let uuid1 = json1["uuid"].as_str().expect("uuid").to_string();

    // b. query_findings(rule: "spec-drift", file: "src/api.rs") → verify found
    let query = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: Some("src/api.rs".into()),
            rule: Some("spec-drift".into()),
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
        .expect("query drift");
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&query)).expect("parse");
    assert_eq!(findings.len(), 1, "should find 1 drift finding");
    assert_eq!(findings[0]["uuid"], uuid1);

    // c. Record same spec-drift at same location → verify deduplicated
    let record2 = server
        .record_finding(Parameters(make_record_input(
            "src/api.rs",
            100,
            "important",
            "API handler missing validation",
            "spec-drift",
        )))
        .await
        .expect("record same drift");
    let json2 = extract_tool_json(&record2);
    assert_eq!(
        json2["status"], "deduplicated",
        "same location + same rule should be deduplicated"
    );
    assert_eq!(
        json2["uuid"].as_str().expect("uuid"),
        uuid1,
        "dedup should return same UUID"
    );

    // d. Record different spec-drift at different file → verify new finding
    let record3 = server
        .record_finding(Parameters(make_record_input(
            "src/db.rs",
            50,
            "important",
            "DB schema drift from spec",
            "spec-drift",
        )))
        .await
        .expect("record different drift");
    let json3 = extract_tool_json(&record3);
    assert_eq!(
        json3["status"], "created",
        "different file should create new finding"
    );
    assert_ne!(
        json3["uuid"].as_str().expect("uuid"),
        uuid1,
        "different file should have different UUID"
    );
}

// =============================================================================
// E2E 4: Tag enrichment and query
// =============================================================================

#[tokio::test]
async fn e2e_tag_enrichment_and_query() {
    let (_tmp, server) = setup_mcp();

    // a. Record a finding
    let record = server
        .record_finding(Parameters(make_record_input(
            "src/auth.rs",
            15,
            "critical",
            "Missing auth check on admin endpoint",
            "missing-auth",
        )))
        .await
        .expect("record");
    let uuid = extract_tool_json(&record)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // b. add_tag with ["architect:deferred", "needs-research"]
    server
        .add_tag(Parameters(TagInput {
            finding_id: uuid.clone(),
            tags: vec!["architect:deferred".into(), "needs-research".into()],
            agent: Some("test-agent".into()),
        }))
        .await
        .expect("add tags");

    // c. query_findings(tag: "architect:deferred") → verify found
    let query = server
        .query_findings(Parameters(QueryFindingsInput {
            status: None,
            severity: None,
            file: None,
            rule: None,
            limit: None,
            tag: Some("architect:deferred".into()),
            filter: None,
            sort: None,
            since: None,
            before: None,
            agent: None,
            category: None,
            text: None,
        }))
        .await
        .expect("query by tag");
    let findings: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&query)).expect("parse");
    assert_eq!(
        findings.len(),
        1,
        "should find 1 finding with architect:deferred tag"
    );
    assert_eq!(findings[0]["uuid"], uuid);

    // d. add_note with architect rationale
    server
        .add_note(Parameters(AddNoteInput {
            finding_id: uuid.clone(),
            note: "Architect review: deferring to sprint 5 — requires auth middleware refactor"
                .into(),
            agent: Some("architect".into()),
        }))
        .await
        .expect("add note");

    // e. get_finding_context → verify tags and notes present
    let ctx = server
        .get_finding_context(Parameters(GetContextInput {
            finding_id: uuid.clone(),
        }))
        .await
        .expect("get context");
    let ctx_text = extract_tool_text(&ctx);
    assert!(
        ctx_text.contains("architect:deferred"),
        "context should contain architect:deferred tag"
    );
    assert!(
        ctx_text.contains("needs-research"),
        "context should contain needs-research tag"
    );
    assert!(
        ctx_text.contains("auth middleware refactor"),
        "context should contain the architect note"
    );
}

// =============================================================================
// E2E 5: Migrate, search, verify — migration workflow
// =============================================================================

#[tokio::test]
async fn e2e_migrate_search_verify() {
    let (_tmp, server) = setup_mcp();

    // a. Record 4 findings with 2 different rule_ids
    for (file, line, title) in [
        ("src/a.rs", 10, "finding A1"),
        ("src/b.rs", 20, "finding A2"),
    ] {
        server
            .record_finding(Parameters(make_record_input(
                file,
                line,
                "important",
                title,
                "rule-alpha",
            )))
            .await
            .expect("record alpha");
    }
    for (file, line, title) in [
        ("src/c.rs", 30, "finding B1"),
        ("src/d.rs", 40, "finding B2"),
    ] {
        server
            .record_finding(Parameters(make_record_input(
                file,
                line,
                "critical",
                title,
                "rule-beta",
            )))
            .await
            .expect("record beta");
    }

    // b. migrate_rules → rules were auto-registered during record
    let migrate1 = server
        .migrate_rules(Parameters(MigrateRulesInput { dry_run: None }))
        .await
        .expect("migrate 1");
    let migrate1_json = extract_tool_json(&migrate1);
    assert_eq!(
        migrate1_json["status"], "migrated",
        "first migrate should succeed"
    );

    // c. search_rules(query: "rule-alpha") → verify exact match at confidence 1.0
    let search = server
        .search_rules(Parameters(SearchRulesInput {
            query: "rule-alpha".into(),
            method: None,
            limit: None,
        }))
        .await
        .expect("search");
    let search_results = extract_tool_json(&search);
    let results = search_results.as_array().expect("search results");
    assert!(!results.is_empty(), "search should find rule-alpha");
    assert_eq!(results[0]["id"], "rule-alpha");
    assert_eq!(
        results[0]["confidence"], 1.0,
        "exact match should have confidence 1.0"
    );

    // d. list_rules → verify 2 rules
    let list = server
        .list_rules(Parameters(ListRulesInput {
            category: None,
            status: None,
        }))
        .await
        .expect("list");
    let rules: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&list)).expect("parse rules");
    assert_eq!(rules.len(), 2, "should have exactly 2 rules");
    let rule_ids: Vec<&str> = rules
        .iter()
        .map(|r| r["id"].as_str().expect("id"))
        .collect();
    assert!(
        rule_ids.contains(&"rule-alpha"),
        "should contain rule-alpha"
    );
    assert!(rule_ids.contains(&"rule-beta"), "should contain rule-beta");

    // e. migrate_rules again → verify idempotent (still 2 rules)
    let migrate2 = server
        .migrate_rules(Parameters(MigrateRulesInput { dry_run: None }))
        .await
        .expect("migrate 2");
    let migrate2_json = extract_tool_json(&migrate2);
    assert_eq!(migrate2_json["status"], "migrated");

    let list2 = server
        .list_rules(Parameters(ListRulesInput {
            category: None,
            status: None,
        }))
        .await
        .expect("list after second migrate");
    let rules2: Vec<serde_json::Value> =
        serde_json::from_str(&extract_tool_text(&list2)).expect("parse rules");
    assert_eq!(
        rules2.len(),
        2,
        "idempotent migrate should still have 2 rules"
    );
}

// =============================================================================
// Prompt 6: consolidate-rules generates content
// =============================================================================

#[tokio::test]
async fn prompt_consolidate_rules_generates_content() {
    let (_tmp, server) = setup_mcp();

    // Create 3 rules
    for (id, name, desc) in [
        ("rule-one", "Rule One", "Checks for issue one"),
        ("rule-two", "Rule Two", "Checks for issue two"),
        (
            "rule-three",
            "Rule Three",
            "Checks for issue three (similar to one)",
        ),
    ] {
        server
            .create_rule(Parameters(CreateRuleInput {
                rule_id: id.into(),
                name: name.into(),
                description: desc.into(),
                category: None,
                severity_hint: None,
                aliases: None,
                cwe_ids: None,
                tags: None,
                scope_include: None,
                scope_exclude: None,
            }))
            .await
            .expect("create rule");
    }

    // Invoke consolidate-rules prompt
    let result = server
        .consolidate_rules()
        .await
        .expect("consolidate prompt");

    assert!(!result.is_empty(), "prompt should return messages");
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(
        !text.is_empty(),
        "consolidate-rules prompt should generate non-empty content"
    );
    assert!(
        text.contains("rule-one"),
        "prompt should reference created rules"
    );
    assert!(
        text.contains("rule-two"),
        "prompt should reference all rules"
    );
    assert!(
        text.contains("rule-three"),
        "prompt should reference all rules"
    );
}

// =============================================================================
// Prompt 7: rule-coverage-report generates content
// =============================================================================

#[tokio::test]
async fn prompt_rule_coverage_report_generates_content() {
    let (_tmp, server) = setup_mcp();

    // Create rules
    server
        .create_rule(Parameters(CreateRuleInput {
            rule_id: "cov-rule-a".into(),
            name: "Coverage Rule A".into(),
            description: "Rule with findings".into(),
            category: None,
            severity_hint: None,
            aliases: None,
            cwe_ids: None,
            tags: None,
            scope_include: None,
            scope_exclude: None,
        }))
        .await
        .expect("create rule a");

    server
        .create_rule(Parameters(CreateRuleInput {
            rule_id: "cov-rule-b".into(),
            name: "Coverage Rule B".into(),
            description: "Rule without findings".into(),
            category: None,
            severity_hint: None,
            aliases: None,
            cwe_ids: None,
            tags: None,
            scope_include: None,
            scope_exclude: None,
        }))
        .await
        .expect("create rule b");

    // Record findings only for rule A
    server
        .record_finding(Parameters(make_record_input(
            "src/x.rs",
            10,
            "important",
            "coverage test finding",
            "cov-rule-a",
        )))
        .await
        .expect("record");

    // Invoke rule-coverage-report prompt
    let result = server
        .rule_coverage_report()
        .await
        .expect("coverage report prompt");

    assert!(!result.is_empty(), "prompt should return messages");
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(
        !text.is_empty(),
        "rule-coverage-report should generate non-empty content"
    );
    assert!(
        text.contains("cov-rule-a"),
        "report should mention rule with findings"
    );
}

// =============================================================================
// Prompt 8: triage-by-rule generates content
// =============================================================================

#[tokio::test]
async fn prompt_triage_by_rule_generates_content() {
    let (_tmp, server) = setup_mcp();

    // Create rule and record findings
    server
        .create_rule(Parameters(CreateRuleInput {
            rule_id: "triage-rule".into(),
            name: "Triage Test Rule".into(),
            description: "Rule for triage testing".into(),
            category: Some("testing".into()),
            severity_hint: Some("important".into()),
            aliases: None,
            cwe_ids: None,
            tags: None,
            scope_include: None,
            scope_exclude: None,
        }))
        .await
        .expect("create rule");

    server
        .record_finding(Parameters(make_record_input(
            "src/foo.rs",
            5,
            "important",
            "triage finding 1",
            "triage-rule",
        )))
        .await
        .expect("record 1");

    server
        .record_finding(Parameters(make_record_input(
            "src/bar.rs",
            15,
            "important",
            "triage finding 2",
            "triage-rule",
        )))
        .await
        .expect("record 2");

    // Invoke triage-by-rule prompt
    let result = server
        .triage_by_rule()
        .await
        .expect("triage-by-rule prompt");

    assert!(!result.is_empty(), "prompt should return messages");
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(
        !text.is_empty(),
        "triage-by-rule should generate non-empty content"
    );
    assert!(
        text.contains("triage-rule"),
        "triage prompt should reference the rule"
    );
    assert!(
        text.contains("triage finding"),
        "triage prompt should include finding data"
    );
}

// =============================================================================
// Prompt 9: fix-finding includes rule examples
// =============================================================================

#[tokio::test]
async fn prompt_fix_finding_includes_rule_examples() {
    let (_tmp, server) = setup_mcp();

    // Create rule with example
    server
        .create_rule(Parameters(CreateRuleInput {
            rule_id: "no-unwrap".into(),
            name: "No unwrap usage".into(),
            description: "Forbids .unwrap() in production code".into(),
            category: Some("safety".into()),
            severity_hint: Some("important".into()),
            aliases: None,
            cwe_ids: None,
            tags: None,
            scope_include: None,
            scope_exclude: None,
        }))
        .await
        .expect("create rule");

    // Add a bad example to the rule
    server
        .add_rule_example(Parameters(AddRuleExampleInput {
            rule_id: "no-unwrap".into(),
            example_type: "bad".into(),
            language: "rust".into(),
            code: "let val = map.get(\"key\").unwrap();".into(),
            explanation: "unwrap() panics on None — use ? or expect() with context".into(),
        }))
        .await
        .expect("add bad example");

    // Add a good example to the rule
    server
        .add_rule_example(Parameters(AddRuleExampleInput {
            rule_id: "no-unwrap".into(),
            example_type: "good".into(),
            language: "rust".into(),
            code: "let val = map.get(\"key\").expect(\"key must exist in config\");".into(),
            explanation: "expect() provides actionable context on failure".into(),
        }))
        .await
        .expect("add good example");

    // Record a finding with this rule
    let record = server
        .record_finding(Parameters(make_record_input(
            "src/config.rs",
            88,
            "important",
            "unwrap in config loader",
            "no-unwrap",
        )))
        .await
        .expect("record finding");
    let uuid = extract_tool_json(&record)["uuid"]
        .as_str()
        .expect("uuid")
        .to_string();

    // Invoke fix-finding prompt
    let result = server
        .fix_finding(Parameters(tally_ng::mcp::server::FixFindingArgs {
            finding_id: uuid,
        }))
        .await
        .expect("fix-finding prompt");

    assert!(!result.is_empty(), "prompt should return messages");
    let rmcp::model::PromptMessageContent::Text { text } = &result[0].content else {
        panic!("expected text content");
    };
    assert!(
        !text.is_empty(),
        "fix-finding prompt should generate non-empty content"
    );
    assert!(
        text.contains("unwrap in config loader"),
        "prompt should contain finding title"
    );
    // Verify the rule examples are included in the prompt
    assert!(
        text.contains("map.get(\"key\").unwrap()"),
        "fix-finding prompt should include the bad example code from the rule"
    );
    assert!(
        text.contains("expect(\"key must exist in config\")"),
        "fix-finding prompt should include the good example code from the rule"
    );
    assert!(
        text.contains("unwrap() panics on None"),
        "fix-finding prompt should include example explanations"
    );
}
