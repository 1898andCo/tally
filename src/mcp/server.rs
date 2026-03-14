//! MCP server implementation using rmcp.
//!
//! `git2::Repository` is not `Send`/`Sync`, so we open the repo fresh per tool call.
//! This is safe — each call is independent and git2 handles file locking internally.

use chrono::Utc;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorCode, Implementation, ProtocolVersion, ServerCapabilities,
    ServerInfo,
};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
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

    fn store(&self) -> Result<GitFindingsStore, McpError> {
        GitFindingsStore::open(&self.repo_path).map_err(|e| McpError {
            code: ErrorCode(-1),
            message: format!("Failed to open repo: {e}").into(),
            data: None,
        })
    }

    #[tool(description = "Record a new finding or deduplicate with an existing one")]
    async fn record_finding(
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

        let location = Location {
            file_path: input.file_path.clone(),
            line_start: input.line_start,
            line_end: input.line_end.unwrap_or(input.line_start),
            role: LocationRole::Primary,
            message: None,
        };

        let fingerprint = compute_fingerprint(&location, &input.rule_id);
        let existing = store.load_all().unwrap_or_default();
        let resolver = FindingIdentityResolver::from_findings(&existing);
        let resolution =
            resolver.resolve(&fingerprint, &input.file_path, input.line_start, &input.rule_id, 5);
        let agent = input.agent.as_deref().unwrap_or("mcp-client");

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
                let finding = build_finding(new_uuid, fingerprint, &input, severity, location, agent);
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
                let finding = build_finding(new_uuid, fingerprint, &input, severity, location, agent);
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
    async fn query_findings(
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
            findings.retain(|f| f.locations.iter().any(|l| l.file_path.contains(pat.as_str())));
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
    async fn update_finding_status(
        &self,
        params: Parameters<UpdateStatusInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = parse_uuid_mcp(&input.finding_id)?;
        let new_status: LifecycleState = input.new_status.parse().map_err(|e: String| McpError {
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
                    finding.status.allowed_transitions().iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>().join(", ")
                ).into(),
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
                message: None, related_to: None, distance: None, expires_at: None,
            }).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Get finding details with full context")]
    async fn get_finding_context(
        &self,
        params: Parameters<GetContextInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = parse_uuid_mcp(&input.finding_id)?;
        let finding = store.load_finding(&uuid).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&finding).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Suppress a finding with reason and optional expiry")]
    async fn suppress_finding(
        &self,
        params: Parameters<SuppressFindingInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let store = self.store()?;
        let uuid = parse_uuid_mcp(&input.finding_id)?;
        let mut finding = store.load_finding(&uuid).map_err(to_mcp_err)?;

        if !finding.status.can_transition_to(LifecycleState::Suppressed) {
            return Err(McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: format!("Cannot suppress from {}", finding.status).into(),
                data: None,
            });
        }

        let expires_at = input.expires_at.as_deref()
            .map(|s| s.parse::<chrono::DateTime<Utc>>().map_err(|e| McpError {
                code: ErrorCode::INVALID_REQUEST,
                message: format!("Invalid date: {e}").into(),
                data: None,
            }))
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
                message: None, related_to: None, distance: None,
                expires_at: expires_at.map(|d| d.to_rfc3339()),
            }).unwrap_or_default(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for TallyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "tally".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some("Git-backed findings tracker for AI coding agents. Tools: record_finding, query_findings, update_finding_status, get_finding_context, suppress_finding.".into()),
        }
    }
}

// --- Helpers ---

#[allow(clippy::needless_pass_by_value)] // map_err requires FnOnce(E) -> F by value
fn to_mcp_err(e: crate::error::TallyError) -> McpError {
    McpError {
        code: ErrorCode(-1),
        message: e.to_string().into(),
        data: None,
    }
}

fn parse_uuid_mcp(id: &str) -> Result<Uuid, McpError> {
    Uuid::parse_str(id).map_err(|_| McpError {
        code: ErrorCode::INVALID_REQUEST,
        message: format!("Invalid UUID: {id}").into(),
        data: None,
    })
}

fn build_finding(
    uuid: Uuid,
    fingerprint: String,
    input: &RecordFindingInput,
    severity: Severity,
    location: Location,
    agent: &str,
) -> Finding {
    Finding {
        uuid,
        content_fingerprint: fingerprint,
        rule_id: input.rule_id.clone(),
        locations: vec![location],
        severity,
        category: String::new(),
        tags: vec![],
        title: input.title.clone(),
        description: input.description.clone().unwrap_or_default(),
        suggested_fix: None,
        evidence: None,
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
    }
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
