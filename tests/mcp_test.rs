//! MCP server integration tests — spawn `tally mcp-server`, send JSON-RPC over stdio.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

// =============================================================================
// Test harness
// =============================================================================

struct McpTestClient {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    stdin: std::process::ChildStdin,
    next_id: u64,
}

impl McpTestClient {
    fn start(repo_path: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tally"))
            .arg("mcp-server")
            .current_dir(repo_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn tally mcp-server");

        let stdin = child.stdin.take().expect("stdin");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));

        let mut client = Self {
            child,
            reader,
            stdin,
            next_id: 1,
        };

        client.initialize();
        client
    }

    fn initialize(&mut self) {
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }
        });
        self.send_raw(&init_req);
        let resp = self.read_response();
        assert_eq!(
            resp["result"]["serverInfo"]["name"], "tally",
            "init should return tally server info"
        );

        // Send initialized notification (no response expected)
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.send_raw(&notif);
    }

    fn send_raw(&mut self, msg: &Value) {
        let line = serde_json::to_string(msg).expect("serialize");
        writeln!(self.stdin, "{line}").expect("write");
        self.stdin.flush().expect("flush");
    }

    fn call_tool(&mut self, name: &str, arguments: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });
        self.send_raw(&req);
        self.read_response()
    }

    fn list_tools(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list"
        });
        self.send_raw(&req);
        self.read_response()
    }

    fn list_resources(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "resources/list"
        });
        self.send_raw(&req);
        self.read_response()
    }

    fn list_resource_templates(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "resources/templates/list"
        });
        self.send_raw(&req);
        self.read_response()
    }

    fn read_resource(&mut self, uri: &str) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "resources/read",
            "params": {"uri": uri}
        });
        self.send_raw(&req);
        self.read_response()
    }

    fn read_response(&mut self) -> Value {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .expect("read response line from MCP server");
        assert!(!line.is_empty(), "MCP server returned empty response");
        serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("parse response JSON: {e}\nraw: {line}"))
    }

    /// Extract the inner JSON payload from a tool call response.
    /// Tool results arrive as `result.content[0].text` containing a JSON string.
    fn tool_output(&mut self, name: &str, arguments: &Value) -> Value {
        let resp = self.call_tool(name, arguments);
        extract_tool_text(&resp)
    }
}

impl Drop for McpTestClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Parse the text content from a successful tool call response.
fn extract_tool_text(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in tool response: {resp}"));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("parse tool output JSON: {e}\nraw text: {text}"))
}

/// Parse the text content from a resource read response.
fn extract_resource_text(resp: &Value) -> Value {
    let text = resp["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in resource response: {resp}"));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("parse resource output JSON: {e}\nraw text: {text}"))
}

// =============================================================================
// Repo setup (mirrors cli_test)
// =============================================================================

fn setup_mcp_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = git2::Repository::init(tmp.path()).expect("init");
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

    // Initialize tally
    assert_cmd::Command::cargo_bin("tally")
        .expect("bin")
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    tmp
}

/// Standard finding arguments for reuse across tests.
fn sample_finding(severity: &str) -> Value {
    serde_json::json!({
        "file_path": "src/main.rs",
        "line_start": 42,
        "severity": severity,
        "title": format!("Test finding ({severity})"),
        "rule_id": "test-rule"
    })
}

// =============================================================================
// Tool tests
// =============================================================================

#[test]
fn mcp_list_tools() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let resp = client.list_tools();
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    let expected = [
        "record_finding",
        "record_batch",
        "query_findings",
        "update_finding_status",
        "get_finding_context",
        "suppress_finding",
        "initialize_store",
        "export_findings",
        "sync_findings",
        "rebuild_index",
        "import_findings",
        "update_finding",
        "add_note",
        "add_tag",
        "remove_tag",
    ];
    for name in &expected {
        assert!(names.contains(name), "missing tool: {name}");
    }
    assert_eq!(
        names.len(),
        expected.len(),
        "unexpected extra tools: {names:?}"
    );
}

#[test]
fn mcp_record_finding_creates() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let output = client.tool_output("record_finding", &sample_finding("critical"));
    assert_eq!(output["status"], "created");
    assert!(output["uuid"].as_str().is_some(), "should return UUID");
}

#[test]
fn mcp_record_finding_deduplicates() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let args = sample_finding("critical");
    let first = client.tool_output("record_finding", &args);
    assert_eq!(first["status"], "created");

    let second = client.tool_output("record_finding", &args);
    assert_eq!(second["status"], "deduplicated");
}

#[test]
fn mcp_record_finding_invalid_severity() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let resp = client.call_tool(
        "record_finding",
        &serde_json::json!({
            "file_path": "src/main.rs",
            "line_start": 1,
            "severity": "ultra",
            "title": "bad severity",
            "rule_id": "test"
        }),
    );
    assert!(
        resp.get("error").is_some(),
        "invalid severity should produce error: {resp}"
    );
}

#[test]
fn mcp_query_findings_empty() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let output = client.tool_output("query_findings", &serde_json::json!({}));
    let findings = output.as_array().expect("array");
    assert!(findings.is_empty(), "no findings yet");
}

#[test]
fn mcp_query_findings_with_filters() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    // Record critical finding
    let critical = client.tool_output("record_finding", &sample_finding("critical"));
    assert_eq!(critical["status"], "created");

    // Record suggestion finding (different rule_id to avoid dedup)
    let suggestion = client.tool_output(
        "record_finding",
        &serde_json::json!({
            "file_path": "src/lib.rs",
            "line_start": 10,
            "severity": "suggestion",
            "title": "suggestion finding",
            "rule_id": "other-rule"
        }),
    );
    assert_eq!(suggestion["status"], "created");

    // Query only critical
    let output = client.tool_output(
        "query_findings",
        &serde_json::json!({"severity": "critical"}),
    );
    let results = output.as_array().expect("array");
    assert_eq!(results.len(), 1, "should return only critical");
    assert_eq!(results[0]["severity"], "critical");
}

#[test]
fn mcp_update_finding_status() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("important"));
    let uuid = created["uuid"].as_str().expect("uuid");

    // Open -> InProgress is valid
    let output = client.tool_output(
        "update_finding_status",
        &serde_json::json!({
            "finding_id": uuid,
            "new_status": "in_progress"
        }),
    );
    assert_eq!(output["status"], "in_progress");
    assert_eq!(output["uuid"], uuid);
}

#[test]
fn mcp_update_finding_invalid_transition() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("critical"));
    let uuid = created["uuid"].as_str().expect("uuid");

    // Open -> Closed is NOT valid
    let resp = client.call_tool(
        "update_finding_status",
        &serde_json::json!({
            "finding_id": uuid,
            "new_status": "closed"
        }),
    );
    assert!(
        resp.get("error").is_some(),
        "Open->Closed should be invalid: {resp}"
    );
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Invalid transition"),
        "error should mention invalid transition: {msg}"
    );
}

#[test]
fn mcp_get_finding_context() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("suggestion"));
    let uuid = created["uuid"].as_str().expect("uuid");

    let output = client.tool_output(
        "get_finding_context",
        &serde_json::json!({"finding_id": uuid}),
    );
    assert_eq!(output["uuid"], uuid);
    assert_eq!(output["severity"], "suggestion");
    assert_eq!(output["rule_id"], "test-rule");
}

#[test]
fn mcp_suppress_finding() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("tech_debt"));
    let uuid = created["uuid"].as_str().expect("uuid");

    let output = client.tool_output(
        "suppress_finding",
        &serde_json::json!({
            "finding_id": uuid,
            "reason": "accepted risk"
        }),
    );
    assert_eq!(output["status"], "suppressed");
    assert_eq!(output["uuid"], uuid);
}

#[test]
fn mcp_suppress_finding_with_expiry() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("suggestion"));
    let uuid = created["uuid"].as_str().expect("uuid");

    let output = client.tool_output(
        "suppress_finding",
        &serde_json::json!({
            "finding_id": uuid,
            "reason": "temporary ignore",
            "expires_at": "2099-12-31T23:59:59Z"
        }),
    );
    assert_eq!(output["status"], "suppressed");
    assert!(
        output["expires_at"].as_str().is_some(),
        "should include expires_at"
    );
}

#[test]
fn mcp_record_batch() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let output = client.tool_output(
        "record_batch",
        &serde_json::json!({
            "findings": [
                {
                    "file_path": "src/a.rs",
                    "line_start": 1,
                    "severity": "critical",
                    "title": "batch-1",
                    "rule_id": "rule-a"
                },
                {
                    "file_path": "src/b.rs",
                    "line_start": 2,
                    "severity": "important",
                    "title": "batch-2",
                    "rule_id": "rule-b"
                },
                {
                    "file_path": "src/c.rs",
                    "line_start": 3,
                    "severity": "suggestion",
                    "title": "batch-3",
                    "rule_id": "rule-c"
                }
            ]
        }),
    );
    assert_eq!(output["succeeded"], 3);
    assert_eq!(output["failed"], 0);
    assert_eq!(output["total"], 3);
}

#[test]
fn mcp_record_batch_partial_failure() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let output = client.tool_output(
        "record_batch",
        &serde_json::json!({
            "findings": [
                {
                    "file_path": "src/ok.rs",
                    "line_start": 1,
                    "severity": "critical",
                    "title": "valid-1",
                    "rule_id": "rule-ok-1"
                },
                {
                    "file_path": "src/bad.rs",
                    "line_start": 2,
                    "severity": "ultra",
                    "title": "invalid severity",
                    "rule_id": "rule-bad"
                },
                {
                    "file_path": "src/ok2.rs",
                    "line_start": 3,
                    "severity": "important",
                    "title": "valid-2",
                    "rule_id": "rule-ok-2"
                }
            ]
        }),
    );
    assert_eq!(output["succeeded"], 2);
    assert_eq!(output["failed"], 1);
}

#[test]
fn mcp_record_finding_with_locations() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let output = client.tool_output(
        "record_finding",
        &serde_json::json!({
            "file_path": "src/main.rs",
            "line_start": 10,
            "severity": "important",
            "title": "multi-location finding",
            "rule_id": "multi-loc",
            "locations": [
                {
                    "file_path": "src/helper.rs",
                    "line_start": 20,
                    "role": "secondary"
                },
                {
                    "file_path": "src/context.rs",
                    "line_start": 30,
                    "role": "context"
                }
            ]
        }),
    );
    assert_eq!(output["status"], "created");
    let uuid = output["uuid"].as_str().expect("uuid");

    // Verify locations stored correctly
    let detail = client.tool_output(
        "get_finding_context",
        &serde_json::json!({"finding_id": uuid}),
    );
    let locations = detail["locations"].as_array().expect("locations array");
    assert_eq!(locations.len(), 3, "primary + 2 additional locations");
}

#[test]
fn mcp_update_with_short_id() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    // Record a critical finding — short ID should be C1
    let created = client.tool_output("record_finding", &sample_finding("critical"));
    assert_eq!(created["status"], "created");

    // Try get_finding_context with short ID "C1"
    let output = client.tool_output(
        "get_finding_context",
        &serde_json::json!({"finding_id": "C1"}),
    );
    assert_eq!(output["severity"], "critical");
    assert_eq!(output["rule_id"], "test-rule");
}

// =============================================================================
// Resource tests
// =============================================================================

#[test]
fn mcp_list_resources() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let resp = client.list_resources();
    let resources = resp["result"]["resources"]
        .as_array()
        .expect("resources array");
    assert!(
        !resources.is_empty(),
        "should have at least summary resource"
    );

    let uris: Vec<&str> = resources.iter().filter_map(|r| r["uri"].as_str()).collect();
    assert!(
        uris.contains(&"findings://summary"),
        "should have summary resource"
    );
}

#[test]
fn mcp_list_resource_templates() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let resp = client.list_resource_templates();
    let templates = resp["result"]["resourceTemplates"]
        .as_array()
        .expect("resource templates array");

    let uris: Vec<&str> = templates
        .iter()
        .filter_map(|t| t["uriTemplate"].as_str())
        .collect();
    assert!(
        uris.contains(&"findings://file/{path}"),
        "should have file template"
    );
    assert!(
        uris.contains(&"findings://detail/{uuid}"),
        "should have detail template"
    );
}

#[test]
fn mcp_read_resource_summary() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    // Record 2 findings
    client.tool_output("record_finding", &sample_finding("critical"));
    client.tool_output(
        "record_finding",
        &serde_json::json!({
            "file_path": "src/lib.rs",
            "line_start": 5,
            "severity": "suggestion",
            "title": "another finding",
            "rule_id": "other-rule"
        }),
    );

    let resp = client.read_resource("findings://summary");
    let summary = extract_resource_text(&resp);
    assert_eq!(summary["total"], 2);
    assert!(
        summary["by_severity"].is_object(),
        "should have by_severity"
    );
    assert!(summary["recent"].is_array(), "should have recent");
}

#[test]
fn mcp_read_resource_file() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    client.tool_output("record_finding", &sample_finding("critical"));

    let resp = client.read_resource("findings://file/src/main.rs");
    let findings = extract_resource_text(&resp);
    let arr = findings.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["severity"], "critical");
}

#[test]
fn mcp_read_resource_detail() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let created = client.tool_output("record_finding", &sample_finding("important"));
    let uuid = created["uuid"].as_str().expect("uuid");

    let uri = format!("findings://detail/{uuid}");
    let resp = client.read_resource(&uri);
    let detail = extract_resource_text(&resp);
    assert_eq!(detail["uuid"], uuid);
    assert_eq!(detail["severity"], "important");
}

#[test]
fn mcp_read_resource_unknown_uri() {
    let tmp = setup_mcp_repo();
    let mut client = McpTestClient::start(tmp.path());

    let resp = client.read_resource("findings://unknown");
    assert!(
        resp.get("error").is_some(),
        "unknown URI should produce error: {resp}"
    );
}
