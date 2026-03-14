//! MCP server implementation using rmcp.
//!
//! `git2::Repository` is not `Send`/`Sync`, so we open the repo fresh per tool call.
//! This is safe — each call is independent and git2 handles file locking internally.

use chrono::Utc;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    Annotated, CallToolResult, Content, ErrorCode, Implementation, ListResourceTemplatesResult,
    ListResourcesResult, PromptMessage, PromptMessageRole, ProtocolVersion, RawResource,
    RawResourceTemplate, ReadResourceRequestParam, ReadResourceResult, ResourceContents,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, prompt, prompt_router, tool, tool_handler,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{
    AgentRecord, Finding, FindingIdentityResolver, IdentityResolution, LifecycleState, Location,
    LocationRole, Severity, StateTransition, Suppression, SuppressionType, compute_fingerprint,
    default_schema_version,
};
use crate::storage::GitFindingsStore;

/// MCP server for tally.
#[derive(Clone)]
pub struct TallyMcpServer {
    repo_path: String,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

// --- Input Types ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordFindingInput {
    #[schemars(description = "Relative file path from repo root (e.g., src/main.rs)")]
    pub file_path: String,
    #[schemars(description = "Start line number (1-based)")]
    pub line_start: u32,
    #[schemars(description = "End line number (defaults to line_start for single-line findings)")]
    pub line_end: Option<u32>,
    #[schemars(description = "Severity level. One of: critical, important, suggestion, tech_debt")]
    pub severity: String,
    #[schemars(description = "Concise title summarizing the issue (e.g., 'unwrap on user input')")]
    pub title: String,
    #[schemars(
        description = "Rule identifier for grouping related findings (e.g., unsafe-unwrap, sql-injection, missing-test)"
    )]
    pub rule_id: String,
    #[schemars(description = "Detailed explanation of the issue and why it matters")]
    pub description: Option<String>,
    #[schemars(
        description = "Your agent identifier for provenance tracking (e.g., claude-code, cursor)"
    )]
    pub agent: Option<String>,
    #[schemars(
        description = "Additional locations for cross-file findings. Each has file_path, line_start, optional line_end, and role (secondary or context). The primary location is set by the top-level file_path/line_start."
    )]
    pub locations: Option<Vec<LocationInput>>,
    #[schemars(description = "Recommended fix or remediation steps")]
    pub suggested_fix: Option<String>,
    #[schemars(description = "Evidence supporting the finding (e.g., code snippet, stack trace)")]
    pub evidence: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LocationInput {
    #[schemars(description = "Relative file path from repo root")]
    pub file_path: String,
    #[schemars(description = "Start line number (1-based)")]
    pub line_start: u32,
    #[schemars(description = "End line number (defaults to line_start)")]
    pub line_end: Option<u32>,
    #[schemars(
        description = "Location role: 'secondary' for supporting evidence, 'context' for additional context. Defaults to 'secondary'."
    )]
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryFindingsInput {
    #[schemars(
        description = "Filter by lifecycle status. One of: open, acknowledged, in_progress, resolved, false_positive, wont_fix, deferred, suppressed, reopened, closed"
    )]
    pub status: Option<String>,
    #[schemars(
        description = "Filter by severity level. One of: critical, important, suggestion, tech_debt"
    )]
    pub severity: Option<String>,
    #[schemars(
        description = "Filter by file path substring match (e.g., 'src/api' matches src/api/handler.rs)"
    )]
    pub file: Option<String>,
    #[schemars(description = "Filter by rule ID (exact match, e.g., unsafe-unwrap)")]
    pub rule: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 100)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateStatusInput {
    #[schemars(
        description = "Finding identifier — either a full UUID or a session short ID (e.g., C1, I2, S3, TD1)"
    )]
    pub finding_id: String,
    #[schemars(
        description = "Target lifecycle status. Valid transitions depend on current state: Open can go to acknowledged/in_progress/false_positive/deferred/suppressed. InProgress can go to resolved/wont_fix/deferred. Resolved can go to reopened/closed."
    )]
    pub new_status: String,
    #[schemars(
        description = "Reason for the status change (e.g., 'fixed in PR #42', 'accepted risk')"
    )]
    pub reason: Option<String>,
    #[schemars(description = "Your agent identifier for audit trail (e.g., claude-code, cursor)")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetContextInput {
    #[schemars(
        description = "Finding identifier — either a full UUID or a session short ID (e.g., C1, I2)"
    )]
    pub finding_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SuppressFindingInput {
    #[schemars(
        description = "Finding identifier — either a full UUID or a session short ID (e.g., C1, I2)"
    )]
    pub finding_id: String,
    #[schemars(
        description = "Why this finding should be suppressed (e.g., 'accepted risk', 'false positive in test code')"
    )]
    pub reason: String,
    #[schemars(
        description = "ISO 8601 expiry date after which the finding auto-reopens (e.g., 2026-06-01T00:00:00Z). Omit for permanent suppression."
    )]
    pub expires_at: Option<String>,
    #[schemars(description = "Your agent identifier for audit trail (e.g., claude-code, cursor)")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordBatchInput {
    #[schemars(
        description = "Array of findings to record. Each is processed independently — invalid entries don't block valid ones (partial success)."
    )]
    pub findings: Vec<BatchFindingInput>,
    #[schemars(description = "Your agent identifier applied to all findings in the batch")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchFindingInput {
    #[schemars(description = "Relative file path from repo root")]
    pub file_path: String,
    #[schemars(description = "Start line number (1-based)")]
    pub line_start: u32,
    #[schemars(description = "End line number (defaults to line_start)")]
    pub line_end: Option<u32>,
    #[schemars(description = "Severity level. One of: critical, important, suggestion, tech_debt")]
    pub severity: String,
    #[schemars(description = "Concise title summarizing the issue")]
    pub title: String,
    #[schemars(description = "Rule identifier for grouping (e.g., unsafe-unwrap, sql-injection)")]
    pub rule_id: String,
    #[schemars(description = "Detailed explanation of the issue")]
    pub description: Option<String>,
    #[schemars(description = "Recommended fix or remediation steps")]
    pub suggested_fix: Option<String>,
    #[schemars(description = "Evidence supporting the finding (e.g., code snippet)")]
    pub evidence: Option<String>,
}

// --- Output Type ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportFindingsInput {
    #[schemars(
        description = "Export format: json (full finding objects), csv (spreadsheet-compatible), or sarif (GitHub Code Scanning compatible SARIF 2.1.0)"
    )]
    pub format: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SyncFindingsInput {
    #[schemars(description = "Git remote name to sync with (default: origin)")]
    pub remote: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportFindingsInput {
    #[schemars(
        description = "Absolute or relative path to a dclaude or zclaude JSON state file to import findings from"
    )]
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ToolOutput {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    related_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    distance: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

// --- Constructor ---

impl TallyMcpServer {
    #[must_use]
    pub fn new(repo_path: String) -> Self {
        Self {
            repo_path,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Get the repository path this server operates on.
    #[must_use]
    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    /// List all registered MCP tools (reflected from the tool router).
    #[must_use]
    pub fn list_tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// List all registered MCP prompts (reflected from the prompt router).
    #[must_use]
    pub fn list_prompts(&self) -> Vec<rmcp::model::Prompt> {
        self.prompt_router.list_all()
    }

    fn store(&self) -> Result<GitFindingsStore, McpError> {
        GitFindingsStore::open(&self.repo_path).map_err(|e| McpError {
            code: ErrorCode(-1),
            message: format!("Failed to open repo: {e}").into(),
            data: None,
        })
    }
}

// --- Tool Implementations ---

#[tool_router]
impl TallyMcpServer {
    #[tool(
        description = "Record a code finding with stable identity. If the same file+line+rule was already recorded, returns the existing UUID (dedup). If a similar finding exists nearby (within 5 lines, same rule), creates a new finding linked as related. Returns JSON with status (created/deduplicated), uuid, and optional related_to/distance."
    )]
    pub async fn record_finding(
        &self,
        params: Parameters<RecordFindingInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let severity: Severity = input.severity.parse().map_err(|e: String| McpError {
            code: ErrorCode::INVALID_REQUEST,
            message: e.into(),
            data: None,
        })?;

        let primary_location = Location {
            file_path: input.file_path.clone(),
            line_start: input.line_start,
            line_end: input.line_end.unwrap_or(input.line_start),
            role: LocationRole::Primary,
            message: None,
        };

        // Build locations from input
        let locations = if let Some(ref loc_inputs) = input.locations {
            let mut locs = vec![primary_location.clone()];
            for loc_input in loc_inputs {
                let role = loc_input
                    .role
                    .as_deref()
                    .map_or(LocationRole::Secondary, |r| {
                        match r.to_ascii_lowercase().as_str() {
                            "primary" => LocationRole::Primary,
                            "context" => LocationRole::Context,
                            _ => LocationRole::Secondary,
                        }
                    });
                locs.push(Location {
                    file_path: loc_input.file_path.clone(),
                    line_start: loc_input.line_start,
                    line_end: loc_input.line_end.unwrap_or(loc_input.line_start),
                    role,
                    message: None,
                });
            }
            locs
        } else {
            vec![primary_location.clone()]
        };

        let fingerprint = compute_fingerprint(&primary_location, &input.rule_id);
        let existing = store.load_all().unwrap_or_default();
        let resolver = FindingIdentityResolver::from_findings(&existing);
        let resolution = resolver.resolve(
            &fingerprint,
            &input.file_path,
            input.line_start,
            &input.rule_id,
            5,
        );
        let agent = input.agent.as_deref().unwrap_or("mcp-client");
        let (repo_id, branch, commit_sha) = store.git_context();
        let ctx = GitContext {
            repo_id,
            branch,
            commit_sha,
        };

        let output = match resolution {
            IdentityResolution::ExistingFinding { uuid } => ToolOutput {
                status: "deduplicated".into(),
                uuid: Some(uuid.to_string()),
                message: None,
                related_to: None,
                distance: None,
                expires_at: None,
            },
            IdentityResolution::RelatedFinding { uuid, distance } => {
                let new_uuid = Uuid::now_v7();
                let finding = build_finding(
                    new_uuid,
                    fingerprint,
                    &input,
                    severity,
                    locations,
                    agent,
                    &ctx,
                );
                store.save_finding(&finding).map_err(to_mcp_err)?;
                ToolOutput {
                    status: "created".into(),
                    uuid: Some(new_uuid.to_string()),
                    message: None,
                    related_to: Some(uuid.to_string()),
                    distance: Some(distance),
                    expires_at: None,
                }
            }
            IdentityResolution::NewFinding => {
                let new_uuid = Uuid::now_v7();
                let finding = build_finding(
                    new_uuid,
                    fingerprint,
                    &input,
                    severity,
                    locations,
                    agent,
                    &ctx,
                );
                store.save_finding(&finding).map_err(to_mcp_err)?;
                ToolOutput {
                    status: "created".into(),
                    uuid: Some(new_uuid.to_string()),
                    message: None,
                    related_to: None,
                    distance: None,
                    expires_at: None,
                }
            }
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Search findings with optional filters. Returns a JSON array of matching findings. All filters are AND-combined. Omit all filters to get all findings. Empty result returns []. Each finding includes uuid, severity, status, title, locations, relationships, and full state_history."
    )]
    pub async fn query_findings(
        &self,
        params: Parameters<QueryFindingsInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let mut findings = store.load_all().map_err(to_mcp_err)?;

        if let Some(ref s) = input.status {
            if let Ok(status) = s.parse::<LifecycleState>() {
                findings.retain(|f| f.status == status);
            }
        }
        if let Some(ref s) = input.severity {
            if let Ok(sev) = s.parse::<Severity>() {
                findings.retain(|f| f.severity == sev);
            }
        }
        if let Some(ref pat) = input.file {
            findings.retain(|f| {
                f.locations
                    .iter()
                    .any(|l| l.file_path.contains(pat.as_str()))
            });
        }
        if let Some(ref rule) = input.rule {
            findings.retain(|f| f.rule_id == *rule);
        }
        findings.truncate(input.limit.unwrap_or(100));

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&findings).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Transition a finding's lifecycle status. Valid transitions: Open→acknowledged/in_progress/false_positive/deferred/suppressed, Acknowledged→in_progress/false_positive/wont_fix/deferred, InProgress→resolved/wont_fix/deferred, Resolved→reopened/closed, Reopened→acknowledged/in_progress. Closed is terminal. Invalid transitions return an error listing valid targets."
    )]
    pub async fn update_finding_status(
        &self,
        params: Parameters<UpdateStatusInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = resolve_id_mcp(&store, &input.finding_id)?;
        let new_status: LifecycleState =
            input.new_status.parse().map_err(|e: String| McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: e.into(),
                data: None,
            })?;

        let mut finding = store.load_finding(&uuid).map_err(to_mcp_err)?;
        if !finding.status.can_transition_to(new_status) {
            return Err(McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: format!(
                    "Invalid transition: {} -> {} (valid: {})",
                    finding.status,
                    new_status,
                    finding
                        .status
                        .allowed_transitions()
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .into(),
                data: None,
            });
        }

        finding.state_history.push(StateTransition {
            from: finding.status,
            to: new_status,
            timestamp: Utc::now(),
            agent_id: input.agent.unwrap_or_else(|| "mcp-client".into()),
            reason: input.reason,
            commit_sha: None,
        });
        finding.status = new_status;
        finding.updated_at = Utc::now();
        store.save_finding(&finding).map_err(to_mcp_err)?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&ToolOutput {
                status: finding.status.to_string(),
                uuid: Some(uuid.to_string()),
                message: None,
                related_to: None,
                distance: None,
                expires_at: None,
            })
            .unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Retrieve a finding's complete details including all locations, state history, relationships, discovered_by agents, and suppression info. Accepts UUID or session short ID (C1, I2, etc.)."
    )]
    pub async fn get_finding_context(
        &self,
        params: Parameters<GetContextInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = resolve_id_mcp(&store, &input.finding_id)?;
        let finding = store.load_finding(&uuid).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&finding).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Record multiple findings at once. Uses partial success semantics — valid findings are recorded even if others fail. Returns JSON with total/succeeded/failed counts and per-item results. Duplicates are automatically deduplicated (not counted as failures)."
    )]
    pub async fn record_batch(
        &self,
        params: Parameters<RecordBatchInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let existing = store.load_all().unwrap_or_default();
        let resolver = FindingIdentityResolver::from_findings(&existing);
        let agent = input.agent.as_deref().unwrap_or("mcp-client");

        let mut total = 0u32;
        let mut succeeded = 0u32;
        let mut failed = 0u32;
        let mut results: Vec<serde_json::Value> = Vec::new();

        for (idx, entry) in input.findings.iter().enumerate() {
            total += 1;
            match record_batch_entry(&store, &resolver, entry, agent) {
                Ok(result) => {
                    succeeded += 1;
                    results
                        .push(serde_json::json!({"index": idx, "status": "ok", "result": result}));
                }
                Err(e) => {
                    failed += 1;
                    results.push(serde_json::json!({"index": idx, "status": "error", "error": e}));
                }
            }
        }

        let output = serde_json::json!({
            "total": total,
            "succeeded": succeeded,
            "failed": failed,
            "results": results,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Suppress a finding so it won't be re-reported. Optionally set an expiry date — after expiry, the finding auto-reopens on next query. Only works from Open status. Returns the finding's UUID and suppression status."
    )]
    pub async fn suppress_finding(
        &self,
        params: Parameters<SuppressFindingInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = resolve_id_mcp(&store, &input.finding_id)?;
        let mut finding = store.load_finding(&uuid).map_err(to_mcp_err)?;

        if !finding.status.can_transition_to(LifecycleState::Suppressed) {
            return Err(McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: format!("Cannot suppress from {}", finding.status).into(),
                data: None,
            });
        }

        let expires_at = input
            .expires_at
            .as_deref()
            .map(|s| {
                s.parse::<chrono::DateTime<Utc>>().map_err(|e| McpError {
                    code: ErrorCode::INVALID_REQUEST,
                    message: format!("Invalid date: {e}").into(),
                    data: None,
                })
            })
            .transpose()?;

        finding.state_history.push(StateTransition {
            from: finding.status,
            to: LifecycleState::Suppressed,
            timestamp: Utc::now(),
            agent_id: input.agent.unwrap_or_else(|| "mcp-client".into()),
            reason: Some(input.reason.clone()),
            commit_sha: None,
        });
        finding.status = LifecycleState::Suppressed;
        finding.suppression = Some(Suppression {
            suppressed_at: Utc::now(),
            reason: input.reason,
            expires_at,
            suppression_type: SuppressionType::Global,
        });
        finding.updated_at = Utc::now();
        store.save_finding(&finding).map_err(to_mcp_err)?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&ToolOutput {
                status: "suppressed".into(),
                uuid: Some(uuid.to_string()),
                message: None,
                related_to: None,
                distance: None,
                expires_at: expires_at.map(|d| d.to_rfc3339()),
            })
            .unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Initialize the tally findings store. Creates the findings-data orphan branch with schema.json and empty findings directory. Idempotent — safe to call multiple times. Must be called before recording any findings."
    )]
    pub async fn initialize_store(&self) -> Result<CallToolResult, McpError> {
        let store = self.store()?;
        store.init().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&ToolOutput {
                status: "initialized".into(),
                uuid: None,
                message: Some("findings-data branch ready".into()),
                related_to: None,
                distance: None,
                expires_at: None,
            })
            .unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Export all findings in the specified format. Returns the exported content as text. Formats: json (full finding objects), csv (spreadsheet-compatible with columns: uuid, severity, status, file, line, title, rule), sarif (SARIF 2.1.0 for GitHub Code Scanning upload)."
    )]
    pub async fn export_findings(
        &self,
        params: Parameters<ExportFindingsInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let findings = store.load_all().map_err(to_mcp_err)?;

        let content = match input.format.to_ascii_lowercase().as_str() {
            "json" => serde_json::to_string_pretty(&findings).unwrap_or_default(),
            "csv" => crate::cli::handlers::export_csv(&findings),
            "sarif" => crate::cli::handlers::export_sarif(&findings),
            other => {
                return Err(McpError {
                    code: ErrorCode::INVALID_REQUEST,
                    message: format!("Unknown format '{other}'. Use json, csv, or sarif.").into(),
                    data: None,
                });
            }
        };

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(
        description = "Sync the findings-data branch with a remote. Fetches remote changes, merges them with local findings, and pushes the result. Use this for multi-agent collaboration to share findings across machines."
    )]
    pub async fn sync_findings(
        &self,
        params: Parameters<SyncFindingsInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let remote = input.remote.as_deref().unwrap_or("origin");
        let result = store.sync(remote).map_err(to_mcp_err)?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "synced",
                "fetched": result.fetched,
                "merged": result.merged,
                "pushed": result.pushed,
            }))
            .unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Rebuild the index.json from finding files on the findings-data branch. Use this if the index becomes out of sync, or after manual edits to finding files."
    )]
    pub async fn rebuild_index(&self) -> Result<CallToolResult, McpError> {
        let store = self.store()?;
        store.rebuild_index().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&ToolOutput {
                status: "rebuilt".into(),
                uuid: None,
                message: Some("index.json rebuilt from finding files".into()),
                related_to: None,
                distance: None,
                expires_at: None,
            })
            .unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Import findings from a dclaude or zclaude JSON state file. Converts findings from the legacy format into tally's native format. Returns count of imported and skipped findings."
    )]
    pub async fn import_findings(
        &self,
        params: Parameters<ImportFindingsInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;

        let content = std::fs::read_to_string(&input.file_path).map_err(|e| McpError {
            code: ErrorCode(-1),
            message: format!("Failed to read file: {e}").into(),
            data: None,
        })?;
        let state: serde_json::Value = serde_json::from_str(&content).map_err(|e| McpError {
            code: ErrorCode::INVALID_REQUEST,
            message: format!("Invalid JSON: {e}").into(),
            data: None,
        })?;

        let findings_arr = state
            .get("active_cycle")
            .and_then(|c| c.get("findings"))
            .and_then(|f| f.as_array())
            .or_else(|| {
                state
                    .get("reviews")
                    .and_then(|r| r.as_array())
                    .and_then(|reviews| reviews.last())
                    .and_then(|r| r.get("findings"))
                    .and_then(|f| f.as_array())
            });

        let Some(findings) = findings_arr else {
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "no_findings",
                    "message": "No findings found. Expected dclaude or zclaude format.",
                    "imported": 0,
                    "skipped": 0,
                }))
                .unwrap_or_default(),
            )]));
        };

        let mut imported: u32 = 0;
        let mut skipped: u32 = 0;

        for entry in findings {
            match import_finding_from_json(entry, &store) {
                Ok(_) => imported += 1,
                Err(_) => skipped += 1,
            }
        }

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "imported",
                "imported": imported,
                "skipped": skipped,
            }))
            .unwrap_or_default(),
        )]))
    }
}

/// Import a single finding from dclaude/zclaude JSON format (MCP helper).
fn import_finding_from_json(
    entry: &serde_json::Value,
    store: &GitFindingsStore,
) -> Result<Uuid, McpError> {
    use crate::model::{
        AgentRecord, Finding, Location, LocationRole, Severity, compute_fingerprint,
        default_schema_version,
    };

    let title = entry
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported finding");
    let file = entry
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let line: u32 = entry
        .get("lines")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1);

    let severity = match entry.get("severity").and_then(|v| v.as_str()) {
        Some("critical") => Severity::Critical,
        Some("important") => Severity::Important,
        Some("suggestion") => Severity::Suggestion,
        Some("tech_debt") => Severity::TechDebt,
        _ => {
            let id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.starts_with('C') {
                Severity::Critical
            } else if id.starts_with('I') {
                Severity::Important
            } else {
                Severity::Suggestion
            }
        }
    };

    let status = match entry.get("status").and_then(|v| v.as_str()) {
        Some("verified") => LifecycleState::Resolved,
        Some("skipped") => LifecycleState::Deferred,
        Some("wont_fix") => LifecycleState::WontFix,
        _ => LifecycleState::Open,
    };

    let category = entry
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let location = Location {
        file_path: file.to_string(),
        line_start: line,
        line_end: line,
        role: LocationRole::Primary,
        message: None,
    };

    let rule_id = if category.is_empty() {
        "imported".to_string()
    } else {
        category.clone()
    };

    let fingerprint = compute_fingerprint(&location, &rule_id);
    let new_uuid = Uuid::now_v7();

    let finding = Finding {
        schema_version: default_schema_version(),
        uuid: new_uuid,
        content_fingerprint: fingerprint,
        rule_id,
        locations: vec![location],
        severity,
        category,
        tags: vec!["imported".to_string()],
        title: title.to_string(),
        description: String::new(),
        suggested_fix: None,
        evidence: None,
        status,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "import".to_string(),
            session_id: String::new(),
            detected_at: Utc::now(),
            session_short_id: entry.get("id").and_then(|v| v.as_str()).map(String::from),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        repo_id: String::new(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
    };

    store.save_finding(&finding).map_err(to_mcp_err)?;
    Ok(new_uuid)
}

// --- Prompt Implementations ---

/// Arguments for the triage-file prompt.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TriageFileArgs {
    #[schemars(description = "File path to triage findings for")]
    pub file_path: String,
}

/// Arguments for the fix-finding prompt.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FixFindingArgs {
    #[schemars(description = "Finding ID (UUID or short ID like C1)")]
    pub finding_id: String,
}

/// Arguments for the explain-finding prompt.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainFindingArgs {
    #[schemars(description = "Finding ID (UUID or short ID like C1)")]
    pub finding_id: String,
}

#[prompt_router]
impl TallyMcpServer {
    /// Triage all findings in a specific file — classify priority and suggest fix order.
    #[prompt(
        name = "triage-file",
        description = "Load all findings for a file and ask the AI to classify priority, assess impact, and suggest a fix order"
    )]
    pub async fn triage_file(
        &self,
        Parameters(args): Parameters<TriageFileArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let store = self.store()?;
        let findings_json = read_resource_file(&store, &args.file_path)?;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here are all the findings for `{}`:\n\n```json\n{findings_json}\n```\n\n\
                 Please triage these findings:\n\
                 1. Classify each by priority (fix now, fix soon, defer, ignore)\n\
                 2. Assess the impact of each finding\n\
                 3. Suggest an optimal fix order (dependencies, quick wins first)\n\
                 4. For each \"fix now\" finding, provide a brief remediation approach",
                args.file_path
            ),
        )])
    }

    /// Generate a code fix for a specific finding.
    #[prompt(
        name = "fix-finding",
        description = "Load a finding's details and ask the AI to generate a concrete code fix"
    )]
    pub async fn fix_finding(
        &self,
        Parameters(args): Parameters<FixFindingArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let store = self.store()?;
        let finding_json = read_resource_detail(&store, &args.finding_id)?;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here is a code finding that needs to be fixed:\n\n\
                 ```json\n{finding_json}\n```\n\n\
                 Please:\n\
                 1. Explain what the issue is and why it matters\n\
                 2. Show the exact code change needed to fix it\n\
                 3. Explain any edge cases or risks with the fix\n\
                 4. Suggest a test to verify the fix"
            ),
        )])
    }

    /// Summarize all findings for a stakeholder-ready report.
    #[prompt(
        name = "summarize-findings",
        description = "Load the findings summary and generate a stakeholder-ready report with counts, trends, and recommendations"
    )]
    pub async fn summarize_findings(&self) -> Result<Vec<PromptMessage>, McpError> {
        let store = self.store()?;
        let summary_json = read_resource_summary(&store)?;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here is the current findings summary:\n\n\
                 ```json\n{summary_json}\n```\n\n\
                 Please create a concise stakeholder-ready report that includes:\n\
                 1. Executive summary (1-2 sentences on overall code health)\n\
                 2. Breakdown by severity with counts\n\
                 3. Top 3 most critical findings that need immediate attention\n\
                 4. Recommendations for the team"
            ),
        )])
    }

    /// Generate a PR review comment from open findings.
    #[prompt(
        name = "review-pr",
        description = "Load all open findings and generate a structured PR review comment"
    )]
    pub async fn review_pr(&self) -> Result<Vec<PromptMessage>, McpError> {
        let store = self.store()?;
        let open_json = read_resource_by_status(&store, "open")?;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here are all open findings in this repository:\n\n\
                 ```json\n{open_json}\n```\n\n\
                 Please write a PR review comment that:\n\
                 1. Lists critical and important findings as blocking issues\n\
                 2. Lists suggestions as non-blocking recommendations\n\
                 3. Groups findings by file for easy navigation\n\
                 4. Uses a professional, constructive tone\n\
                 5. Formats as a GitHub PR review comment with markdown"
            ),
        )])
    }

    /// Explain a finding's impact and context.
    #[prompt(
        name = "explain-finding",
        description = "Load a finding's details and ask the AI to explain the issue, its security/quality impact, and real-world consequences"
    )]
    pub async fn explain_finding(
        &self,
        Parameters(args): Parameters<ExplainFindingArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let store = self.store()?;
        let finding_json = read_resource_detail(&store, &args.finding_id)?;

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here is a code finding:\n\n\
                 ```json\n{finding_json}\n```\n\n\
                 Please explain:\n\
                 1. What this issue is in plain language\n\
                 2. Why it matters (security, reliability, maintainability)\n\
                 3. What could happen if left unfixed (real-world consequences)\n\
                 4. How common this type of issue is\n\
                 5. Whether this is a false positive or a genuine concern"
            ),
        )])
    }
}

#[tool_handler]
impl ServerHandler for TallyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
            server_info: Implementation {
                name: "tally".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some("tally — persistent findings tracker backed by git. Use record_finding when you discover an issue in code. Use query_findings to check existing findings before recording (avoids duplicates). Use update_finding_status to track progress (open→in_progress→resolved→closed). Findings persist across sessions with stable UUIDs. Short IDs (C1, I2, S3) are accepted anywhere a UUID is expected. Severity levels: critical, important, suggestion, tech_debt.".into()),
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, McpError> {
        let prompts = self.prompt_router.list_all();
        Ok(rmcp::model::ListPromptsResult {
            next_cursor: None,
            prompts,
        })
    }

    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, McpError> {
        let prompt_context = rmcp::handler::server::prompt::PromptContext::new(
            self,
            request.name,
            request.arguments,
            context,
        );
        self.prompt_router.get_prompt(prompt_context).await
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resource = RawResource::new("findings://summary", "Findings Summary");
        resource.description = Some("Counts by severity/status, 10 most recent findings".into());
        resource.mime_type = Some("application/json".into());

        Ok(ListResourcesResult {
            next_cursor: None,
            resources: vec![Annotated::new(resource, None)],
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: vec![
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "findings://file/{path}".into(),
                        name: "Findings by File".into(),
                        title: None,
                        description: Some("All findings in a specific file".into()),
                        mime_type: Some("application/json".into()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "findings://detail/{uuid}".into(),
                        name: "Finding Detail".into(),
                        title: None,
                        description: Some("Full finding with history and code context".into()),
                        mime_type: Some("application/json".into()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "findings://severity/{level}".into(),
                        name: "Findings by Severity".into(),
                        title: None,
                        description: Some(
                            "All findings at a severity level (critical, important, suggestion, tech_debt)"
                                .into(),
                        ),
                        mime_type: Some("application/json".into()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "findings://status/{status}".into(),
                        name: "Findings by Status".into(),
                        title: None,
                        description: Some(
                            "All findings with a lifecycle status (open, in_progress, resolved, etc.)"
                                .into(),
                        ),
                        mime_type: Some("application/json".into()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "findings://rule/{rule_id}".into(),
                        name: "Findings by Rule".into(),
                        title: None,
                        description: Some(
                            "All findings for a specific rule ID (e.g., unsafe-unwrap, sql-injection)"
                                .into(),
                        ),
                        mime_type: Some("application/json".into()),
                    },
                    None,
                ),
            ],
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let store = self.store()?;
        let uri = &request.uri;

        let content = if uri == "findings://summary" {
            read_resource_summary(&store)?
        } else if let Some(path) = uri.strip_prefix("findings://file/") {
            read_resource_file(&store, path)?
        } else if let Some(uuid_str) = uri.strip_prefix("findings://detail/") {
            read_resource_detail(&store, uuid_str)?
        } else if let Some(level) = uri.strip_prefix("findings://severity/") {
            read_resource_by_severity(&store, level)?
        } else if let Some(status) = uri.strip_prefix("findings://status/") {
            read_resource_by_status(&store, status)?
        } else if let Some(rule) = uri.strip_prefix("findings://rule/") {
            read_resource_by_rule(&store, rule)?
        } else {
            return Err(McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: format!("Unknown resource URI: {uri}").into(),
                data: None,
            });
        };

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(content, uri)],
        })
    }
}

// --- Helpers ---

/// Git context for populating findings.
struct GitContext {
    repo_id: String,
    branch: Option<String>,
    commit_sha: Option<String>,
}

#[allow(clippy::needless_pass_by_value)] // map_err requires FnOnce(E) -> F by value
fn to_mcp_err(e: crate::error::TallyError) -> McpError {
    McpError {
        code: ErrorCode(-1),
        message: e.to_string().into(),
        data: None,
    }
}

/// Resolve a finding ID that can be either a UUID or a session short ID.
fn resolve_id_mcp(store: &GitFindingsStore, id: &str) -> Result<Uuid, McpError> {
    // Try UUID first
    if let Ok(uuid) = Uuid::parse_str(id) {
        return Ok(uuid);
    }

    // Try short ID — load all findings to build the mapper
    let findings = store.load_all().map_err(to_mcp_err)?;
    let mut mapper = crate::session::SessionIdMapper::new();
    for finding in &findings {
        mapper.assign(finding.uuid, finding.severity);
    }

    mapper.resolve(id).ok_or_else(|| McpError {
        code: ErrorCode::INVALID_REQUEST,
        message: format!("Invalid finding ID: {id} (not a UUID or known short ID)").into(),
        data: None,
    })
}

fn record_batch_entry(
    store: &GitFindingsStore,
    resolver: &FindingIdentityResolver,
    entry: &BatchFindingInput,
    agent: &str,
) -> Result<serde_json::Value, String> {
    let severity: Severity = entry.severity.parse().map_err(|e: String| e)?;

    let location = Location {
        file_path: entry.file_path.clone(),
        line_start: entry.line_start,
        line_end: entry.line_end.unwrap_or(entry.line_start),
        role: LocationRole::Primary,
        message: None,
    };

    let fingerprint = compute_fingerprint(&location, &entry.rule_id);
    let resolution = resolver.resolve(
        &fingerprint,
        &entry.file_path,
        entry.line_start,
        &entry.rule_id,
        5,
    );

    match resolution {
        IdentityResolution::ExistingFinding { uuid } => {
            Ok(serde_json::json!({"status": "deduplicated", "uuid": uuid.to_string()}))
        }
        IdentityResolution::NewFinding | IdentityResolution::RelatedFinding { .. } => {
            let new_uuid = Uuid::now_v7();
            let finding = Finding {
                schema_version: default_schema_version(),
                uuid: new_uuid,
                content_fingerprint: fingerprint,
                rule_id: entry.rule_id.clone(),
                locations: vec![location],
                severity,
                category: String::new(),
                tags: vec![],
                title: entry.title.clone(),
                description: entry.description.clone().unwrap_or_default(),
                suggested_fix: entry.suggested_fix.clone(),
                evidence: entry.evidence.clone(),
                status: LifecycleState::Open,
                state_history: vec![],
                discovered_by: vec![AgentRecord {
                    agent_id: agent.to_string(),
                    session_id: String::new(),
                    detected_at: Utc::now(),
                    session_short_id: None,
                }],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                repo_id: String::new(),
                branch: None,
                pr_number: None,
                commit_sha: None,
                relationships: vec![],
                suppression: None,
            };
            store.save_finding(&finding).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"status": "created", "uuid": new_uuid.to_string()}))
        }
    }
}

fn build_finding(
    uuid: Uuid,
    fingerprint: String,
    input: &RecordFindingInput,
    severity: Severity,
    locations: Vec<Location>,
    agent: &str,
    ctx: &GitContext,
) -> Finding {
    Finding {
        schema_version: default_schema_version(),
        uuid,
        content_fingerprint: fingerprint,
        rule_id: input.rule_id.clone(),
        locations,
        severity,
        category: String::new(),
        tags: vec![],
        title: input.title.clone(),
        description: input.description.clone().unwrap_or_default(),
        suggested_fix: input.suggested_fix.clone(),
        evidence: input.evidence.clone(),
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: agent.to_string(),
            session_id: String::new(),
            detected_at: Utc::now(),
            session_short_id: None,
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        repo_id: ctx.repo_id.clone(),
        branch: ctx.branch.clone(),
        pr_number: None,
        commit_sha: ctx.commit_sha.clone(),
        relationships: vec![],
        suppression: None,
    }
}

/// Read the `findings://summary` resource — counts by severity/status + 10 most recent.
///
/// # Errors
///
/// Returns `McpError` if storage or serialization fails.
pub fn read_resource_summary(store: &GitFindingsStore) -> Result<String, McpError> {
    let findings = store.load_all().map_err(to_mcp_err)?;

    let mut by_severity = std::collections::HashMap::new();
    let mut by_status = std::collections::HashMap::new();
    for f in &findings {
        *by_severity.entry(f.severity.to_string()).or_insert(0u32) += 1;
        *by_status.entry(f.status.to_string()).or_insert(0u32) += 1;
    }

    let total = findings.len();

    // 10 most recent findings
    let mut recent = findings;
    recent.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    recent.truncate(10);

    let recent_summaries: Vec<serde_json::Value> = recent
        .iter()
        .map(|f| {
            serde_json::json!({
                "uuid": f.uuid.to_string(),
                "title": f.title,
                "severity": f.severity,
                "status": f.status,
                "created_at": f.created_at.to_rfc3339(),
            })
        })
        .collect();

    let summary = serde_json::json!({
        "total": total,
        "by_severity": by_severity,
        "by_status": by_status,
        "recent": recent_summaries,
    });

    serde_json::to_string_pretty(&summary).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Read the `findings://file/{path}` resource — all findings in a specific file.
///
/// # Errors
///
/// Returns `McpError` if storage or serialization fails.
pub fn read_resource_file(store: &GitFindingsStore, path: &str) -> Result<String, McpError> {
    let findings = store.load_all().map_err(to_mcp_err)?;
    let matched: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.locations.iter().any(|l| l.file_path.contains(path)))
        .collect();

    serde_json::to_string_pretty(&matched).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Read the `findings://detail/{uuid}` resource — full finding with history.
///
/// # Errors
///
/// Returns `McpError` if the finding is not found or serialization fails.
pub fn read_resource_detail(store: &GitFindingsStore, uuid_str: &str) -> Result<String, McpError> {
    let uuid = resolve_id_mcp(store, uuid_str)?;
    let finding = store.load_finding(&uuid).map_err(to_mcp_err)?;

    serde_json::to_string_pretty(&finding).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Read the `findings://severity/{level}` resource.
///
/// # Errors
///
/// Returns `McpError` if storage or serialization fails.
pub fn read_resource_by_severity(
    store: &GitFindingsStore,
    level: &str,
) -> Result<String, McpError> {
    let findings = store.load_all().map_err(to_mcp_err)?;
    let matched: Vec<&Finding> = if let Ok(sev) = level.parse::<Severity>() {
        findings.iter().filter(|f| f.severity == sev).collect()
    } else {
        vec![]
    };
    serde_json::to_string_pretty(&matched).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Read the `findings://status/{status}` resource.
///
/// # Errors
///
/// Returns `McpError` if storage or serialization fails.
pub fn read_resource_by_status(
    store: &GitFindingsStore,
    status_str: &str,
) -> Result<String, McpError> {
    let findings = store.load_all().map_err(to_mcp_err)?;
    let matched: Vec<&Finding> = if let Ok(status) = status_str.parse::<LifecycleState>() {
        findings.iter().filter(|f| f.status == status).collect()
    } else {
        vec![]
    };
    serde_json::to_string_pretty(&matched).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Read the `findings://rule/{rule_id}` resource.
///
/// # Errors
///
/// Returns `McpError` if storage or serialization fails.
pub fn read_resource_by_rule(store: &GitFindingsStore, rule_id: &str) -> Result<String, McpError> {
    let findings = store.load_all().map_err(to_mcp_err)?;
    let matched: Vec<&Finding> = findings.iter().filter(|f| f.rule_id == rule_id).collect();
    serde_json::to_string_pretty(&matched).map_err(|e| McpError {
        code: ErrorCode(-1),
        message: format!("Serialization error: {e}").into(),
        data: None,
    })
}

/// Run the MCP server on stdio.
///
/// # Errors
///
/// Returns error if the server fails to start or encounters a fatal error.
pub async fn run_mcp_server(repo_path: &str) -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = TallyMcpServer::new(repo_path.to_string());
    let transport = rmcp::transport::io::stdio();

    tracing::info!("MCP server starting on stdio");
    let service = server.serve(transport).await?;
    tracing::info!("MCP server connected");

    service.waiting().await?;
    Ok(())
}
