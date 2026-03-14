//! MCP server implementation using rmcp.
//!
//! `git2::Repository` is not `Send`/`Sync`, so we open the repo fresh per tool call.
//! This is safe — each call is independent and git2 handles file locking internally.

use chrono::Utc;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    Annotated, CallToolResult, Content, ErrorCode, Implementation, ListResourceTemplatesResult,
    ListResourcesResult, ProtocolVersion, RawResource, RawResourceTemplate,
    ReadResourceRequestParam, ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{
    AgentRecord, Finding, FindingIdentityResolver, IdentityResolution, LifecycleState, Location,
    LocationRole, Severity, StateTransition, Suppression, SuppressionType, compute_fingerprint,
};
use crate::storage::GitFindingsStore;

/// MCP server for tally.
#[derive(Clone)]
pub struct TallyMcpServer {
    repo_path: String,
    tool_router: ToolRouter<Self>,
}

// --- Input Types ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordFindingInput {
    #[schemars(description = "File path where the finding was discovered")]
    pub file_path: String,
    #[schemars(description = "Start line number")]
    pub line_start: u32,
    #[schemars(description = "End line number (defaults to line_start)")]
    pub line_end: Option<u32>,
    #[schemars(description = "Severity: critical, important, suggestion, tech_debt")]
    pub severity: String,
    #[schemars(description = "Short title of the finding")]
    pub title: String,
    #[schemars(description = "Rule ID for grouping (e.g., unsafe-unwrap)")]
    pub rule_id: String,
    #[schemars(description = "Detailed description")]
    pub description: Option<String>,
    #[schemars(description = "Agent identifier (e.g., claude-code)")]
    pub agent: Option<String>,
    #[schemars(
        description = "Additional locations (array of {file_path, line_start, line_end, role})"
    )]
    pub locations: Option<Vec<LocationInput>>,
    #[schemars(description = "Suggested fix or remediation")]
    pub suggested_fix: Option<String>,
    #[schemars(description = "Evidence or code snippet supporting the finding")]
    pub evidence: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LocationInput {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: Option<u32>,
    #[schemars(description = "primary, secondary, or context")]
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryFindingsInput {
    #[schemars(description = "Filter by status (e.g., open, resolved)")]
    pub status: Option<String>,
    #[schemars(description = "Filter by severity (e.g., critical, important)")]
    pub severity: Option<String>,
    #[schemars(description = "Filter by file path (substring match)")]
    pub file: Option<String>,
    #[schemars(description = "Filter by rule ID")]
    pub rule: Option<String>,
    #[schemars(description = "Max results (default 100)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateStatusInput {
    #[schemars(description = "Finding UUID")]
    pub finding_id: String,
    #[schemars(description = "Target status (e.g., in_progress, resolved)")]
    pub new_status: String,
    #[schemars(description = "Reason for the transition")]
    pub reason: Option<String>,
    #[schemars(description = "Agent performing the update")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetContextInput {
    #[schemars(description = "Finding UUID")]
    pub finding_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SuppressFindingInput {
    #[schemars(description = "Finding UUID")]
    pub finding_id: String,
    #[schemars(description = "Reason for suppression")]
    pub reason: String,
    #[schemars(description = "Expiry date (ISO 8601). Omit for permanent.")]
    pub expires_at: Option<String>,
    #[schemars(description = "Agent performing the suppression")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordBatchInput {
    #[schemars(description = "Array of findings to record")]
    pub findings: Vec<BatchFindingInput>,
    #[schemars(description = "Agent identifier")]
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchFindingInput {
    #[schemars(description = "File path")]
    pub file_path: String,
    #[schemars(description = "Start line number")]
    pub line_start: u32,
    #[schemars(description = "End line number")]
    pub line_end: Option<u32>,
    #[schemars(description = "Severity: critical, important, suggestion, tech_debt")]
    pub severity: String,
    #[schemars(description = "Short title")]
    pub title: String,
    #[schemars(description = "Rule ID")]
    pub rule_id: String,
    #[schemars(description = "Description")]
    pub description: Option<String>,
    #[schemars(description = "Suggested fix or remediation")]
    pub suggested_fix: Option<String>,
    #[schemars(description = "Evidence or code snippet supporting the finding")]
    pub evidence: Option<String>,
}

// --- Output Type ---

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

// --- Tool Implementations ---

#[tool_router]
impl TallyMcpServer {
    #[must_use]
    pub fn new(repo_path: String) -> Self {
        Self {
            repo_path,
            tool_router: Self::tool_router(),
        }
    }

    /// Get the repository path this server operates on.
    #[must_use]
    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    fn store(&self) -> Result<GitFindingsStore, McpError> {
        GitFindingsStore::open(&self.repo_path).map_err(|e| McpError {
            code: ErrorCode(-1),
            message: format!("Failed to open repo: {e}").into(),
            data: None,
        })
    }

    #[tool(description = "Record a new finding or deduplicate with an existing one")]
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

    #[tool(description = "Query findings with filters")]
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

    #[tool(description = "Update a finding's lifecycle status")]
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

    #[tool(description = "Get finding details with full context")]
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

    #[tool(description = "Record multiple findings in batch (partial success semantics)")]
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

    #[tool(description = "Suppress a finding with reason and optional expiry")]
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
}

#[tool_handler]
impl ServerHandler for TallyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation {
                name: "tally".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some("Git-backed findings tracker for AI coding agents. Tools: record_finding, record_batch, query_findings, update_finding_status, get_finding_context, suppress_finding. Resources: findings://summary, findings://file/{path}, findings://detail/{uuid}.".into()),
        }
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

/// Run the MCP server on stdio.
///
/// # Errors
///
/// Returns error if the server fails to start or encounters a fatal error.
pub async fn run_mcp_server(repo_path: &str) -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = TallyMcpServer::new(repo_path.to_string());
    let transport = rmcp::transport::io::stdio();

    eprintln!("tally MCP server starting on stdio...");
    let service = server.serve(transport).await?;
    eprintln!("tally MCP server connected.");

    service.waiting().await?;
    Ok(())
}
