//! CLI interface for tally — clap-based subcommands.

mod batch;
mod capabilities;
mod common;
mod export;
mod import;
mod init;
mod note;
mod query;
mod rebuild_index;
mod record;
pub mod rule; // TODO: refactor to `mod rule` + `pub use rule::handle_*` to match other CLI modules
mod stats;
mod suppress;
mod sync_cmd;
mod tag;
mod update;
mod update_fields;

pub use batch::handle_record_batch;
pub use capabilities::handle_mcp_capabilities;
pub use export::{export_csv, export_sarif, handle_export};
pub use import::handle_import;
pub use init::handle_init;
pub use note::handle_add_note;
pub use query::handle_query;
pub use rebuild_index::handle_rebuild_index;
pub use record::{RecordArgs, handle_record};
pub use stats::handle_stats;
pub use suppress::handle_suppress;
pub use sync_cmd::handle_sync;
pub use tag::handle_manage_tags;
pub use update::{UpdateArgs, handle_update};
pub use update_fields::handle_update_fields;

use clap::{Parser, Subcommand, ValueEnum};

/// tally — git-backed findings tracker for AI coding agents.
#[derive(Parser)]
#[command(name = "tally", version, about)]
pub struct Cli {
    /// Increase logging verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Decrease logging verbosity (-q error, -qq off)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub quiet: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::doc_markdown)]
pub enum Command {
    /// Initialize the findings-data branch in the current repo. Idempotent — safe to call multiple times.
    Init,

    /// Record a new finding (or deduplicate with existing). If the same file+line+rule already exists, returns the existing UUID. If a similar finding exists nearby (within 5 lines, same rule), creates a new finding linked as related.
    Record {
        /// File path where the finding was discovered (relative to repo root).
        #[arg(long)]
        file: String,

        /// Line number (start, 1-based).
        #[arg(long)]
        line: u32,

        /// Line number (end). Defaults to same as --line for single-line findings.
        #[arg(long)]
        line_end: Option<u32>,

        /// Severity level. Values: critical, important, suggestion, tech-debt. Critical/important block PR approval; suggestion/tech-debt are advisory.
        #[arg(long)]
        severity: String,

        /// Short title of the finding (e.g., "unwrap on user input").
        #[arg(long)]
        title: String,

        /// Rule ID for grouping related findings across files and PRs (e.g., "unsafe-unwrap", "sql-injection", "missing-test"). Findings with the same rule+file+line are deduplicated.
        #[arg(long)]
        rule: String,

        /// Detailed description of the issue and why it matters.
        #[arg(long, default_value = "")]
        description: String,

        /// Comma-separated tags for filtering (e.g., "pr-review,sweep,security").
        #[arg(long, default_value = "")]
        tags: String,

        /// Agent identifier for provenance tracking (e.g., "dclaude:security-reviewer", "claude-code", "cursor").
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Session identifier to group findings from the same review session.
        #[arg(long, default_value = "")]
        session: String,

        /// Additional locations (format: `file:line_start:line_end:role`).
        /// Role values: primary, secondary, context. Can be repeated. The --file/--line flags set the primary location.
        #[arg(long)]
        location: Vec<String>,

        /// Related finding ID (UUID or short ID like C1). Creates a relationship between this finding and another.
        #[arg(long)]
        related_to: Option<String>,

        /// Relationship type (used with --related-to). Values: related_to (default), duplicate_of, blocks, causes, discovered_while_fixing, supersedes.
        #[arg(long, default_value = "related_to")]
        relationship: String,

        /// Category for grouping by domain (e.g., "injection", "auth", "pattern-break", "missing-test"). Distinct from --rule — category is the broad class, rule is the specific check.
        #[arg(long, default_value = "")]
        category: String,

        /// Suggested fix or remediation steps.
        #[arg(long)]
        suggested_fix: Option<String>,

        /// Evidence or code snippet supporting the finding.
        #[arg(long)]
        evidence: Option<String>,
    },

    /// Query findings with filters. All filters are AND-combined. Omit all filters to get all findings.
    ///
    /// TallyQL expression language (--filter): supports boolean operators (AND, OR, NOT),
    /// comparisons (=, !=, >, <), string ops (CONTAINS, STARTSWITH, ENDSWITH),
    /// existence checks (HAS, MISSING), IN lists, and date literals (7d, 24h, "2026-03-01").
    ///
    /// Examples:
    ///   tally query --filter 'severity = critical AND file CONTAINS "api"'
    ///   tally query --severity critical,important --since 7d --sort severity
    ///   tally query --text unwrap --not-status closed
    Query {
        /// Filter by lifecycle status (comma-separated). Values: open, acknowledged, in_progress, resolved, false_positive, wont_fix, deferred, suppressed, reopened, closed.
        #[arg(long)]
        status: Option<String>,

        /// Filter by severity (comma-separated). Values: critical, important, suggestion, tech-debt.
        #[arg(long)]
        severity: Option<String>,

        /// Filter by file path (substring match, e.g., "src/api" matches src/api/handler.rs).
        #[arg(long)]
        file: Option<String>,

        /// Filter by rule ID (exact match).
        #[arg(long)]
        rule: Option<String>,

        /// Filter by related finding ID (shows findings related to this UUID or short ID).
        #[arg(long)]
        related_to: Option<String>,

        /// Filter by tag (substring match against finding's tags).
        #[arg(long)]
        tag: Option<String>,

        /// TallyQL filter expression. Examples: 'severity = critical AND file CONTAINS "api"', 'HAS suggested_fix', 'created_at > 7d'.
        #[arg(long)]
        filter: Option<String>,

        /// Filter by creation date (findings created after). Accepts duration (7d, 24h) or ISO 8601 date (2026-03-01).
        #[arg(long)]
        since: Option<String>,

        /// Filter by creation date (findings created before). Accepts duration (7d, 24h) or ISO 8601 date (2026-03-01).
        #[arg(long)]
        before: Option<String>,

        /// Filter by agent ID (exact match against discovered_by agent_id).
        #[arg(long)]
        agent: Option<String>,

        /// Filter by category (exact match).
        #[arg(long)]
        category: Option<String>,

        /// Exclude findings with this status.
        #[arg(long)]
        not_status: Option<String>,

        /// Full-text search across title, description, suggested_fix, and evidence (case-insensitive).
        #[arg(long)]
        text: Option<String>,

        /// Sort by field (repeatable for multi-field sort). Fields: severity, status, created_at, updated_at, file, rule, title.
        #[arg(long)]
        sort: Vec<String>,

        /// Sort direction. Default: desc for date fields, asc for others.
        #[arg(long)]
        sort_dir: Option<String>,

        /// Output format.
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,

        /// Max results (default: 100).
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Update a finding's lifecycle status. State machine: Open->acknowledged/in_progress/false_positive/deferred/suppressed, Acknowledged->in_progress/false_positive/wont_fix/deferred, InProgress->resolved/wont_fix/deferred, Resolved->reopened/closed, Deferred->open/reopened/closed, Suppressed->open/reopened/closed, Reopened->acknowledged/in_progress. Closed is terminal.
    Update {
        /// Finding UUID (or session short ID like C1, I2, S3, TD1).
        id: String,

        /// Target status. Values: open, acknowledged, in_progress, resolved, false_positive, wont_fix, deferred, suppressed, reopened, closed. Invalid transitions return an error listing valid targets.
        #[arg(long)]
        status: String,

        /// Reason for the transition (e.g., "fixed in PR #42", "accepted risk", "deferred to next sprint").
        #[arg(long)]
        reason: Option<String>,

        /// Commit SHA that fixed the finding (for traceability in state history).
        #[arg(long)]
        commit: Option<String>,

        /// Agent performing the update (e.g., "dclaude:pr-fix-verify", "cli").
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Add relationship to another finding (UUID or short ID).
        #[arg(long)]
        related_to: Option<String>,

        /// Relationship type (used with --related-to). Values: related_to (default), duplicate_of, blocks, causes, discovered_while_fixing, supersedes.
        #[arg(long, default_value = "related_to")]
        relationship: String,
    },

    /// Suppress a finding so it won't be re-reported. Only valid from Open status. Optionally set an expiry date for auto-reopening.
    Suppress {
        /// Finding UUID (or session short ID like C1, I2).
        id: String,

        /// Reason for suppression (e.g., "accepted risk", "false positive in test code", "known spec conflict").
        #[arg(long)]
        reason: String,

        /// Expiry date (ISO 8601, e.g., 2026-06-01T00:00:00Z). Omit for permanent suppression. After expiry, the finding auto-reopens on next query.
        #[arg(long)]
        expires: Option<String>,

        /// Agent performing the suppression.
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Suppression scope. Values: global (suppress everywhere, default), file (suppress only in this file), inline (suppress at specific code pattern — requires --suppression-pattern).
        #[arg(long, default_value = "global")]
        suppression_type: String,

        /// Inline suppression pattern (required with --suppression-type inline). Matches against source lines. Example: "tally:suppress unsafe-unwrap".
        #[arg(long)]
        suppression_pattern: Option<String>,
    },

    /// Rebuild the index.json from finding files. Use if the index becomes out of sync or after manual edits.
    RebuildIndex {
        /// Also recalculate finding_count for each rule in the registry.
        #[arg(long)]
        include_rules: bool,
    },

    /// Record multiple findings from a JSONL file or stdin. Each line is a JSON object with the same fields as `record`. Partial success — invalid entries don't block valid ones.
    RecordBatch {
        /// Path to JSONL file. Use "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Agent identifier applied to all findings in the batch.
        #[arg(long, default_value = "cli")]
        agent: String,
    },

    /// Export findings in SARIF 2.1.0 (GitHub Code Scanning), CSV (spreadsheet), or JSON (full objects) format.
    Export {
        /// Output format.
        #[arg(long, value_enum)]
        format: ExportFormat,

        /// Output file path. Defaults to stdout.
        #[arg(long)]
        output: Option<String>,
    },

    /// Sync findings with remote (fetch + merge + push). For multi-agent collaboration.
    Sync {
        /// Remote name (default: origin).
        #[arg(long, default_value = "origin")]
        remote: String,
    },

    /// Import findings from dclaude/zclaude state files. Converts legacy format to tally's native format.
    Import {
        /// Path to the state JSON file (dclaude or zclaude format).
        path: String,
    },

    /// Show summary statistics (counts by severity and status).
    Stats,

    /// Run as MCP server over stdio (JSON-RPC). Configure in .mcp.json.
    McpServer,

    /// Generate shell completions for bash, zsh, fish, or powershell.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Edit mutable fields on a finding (title, description, suggested_fix, evidence, severity, category, tags). Identity fields (uuid, fingerprint, rule_id) are immutable.
    UpdateFields {
        /// Finding UUID (or session short ID like C1, I2).
        id: String,

        /// New title.
        #[arg(long)]
        title: Option<String>,

        /// New description.
        #[arg(long)]
        description: Option<String>,

        /// New suggested fix.
        #[arg(long)]
        suggested_fix: Option<String>,

        /// New evidence.
        #[arg(long)]
        evidence: Option<String>,

        /// New severity (critical, important, suggestion, tech_debt).
        #[arg(long)]
        severity: Option<String>,

        /// New category.
        #[arg(long)]
        category: Option<String>,

        /// Replace tags (comma-separated).
        #[arg(long)]
        tags: Option<String>,

        /// Agent performing the edit.
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Output format.
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },

    /// Add a timestamped note to a finding without changing its status.
    #[command(name = "note")]
    AddNote {
        /// Finding UUID (or session short ID like C1, I2).
        id: String,

        /// Note text.
        text: String,

        /// Agent adding the note.
        #[arg(long, default_value = "cli")]
        agent: String,
    },

    /// Add or remove tags on a finding.
    #[command(name = "tag")]
    ManageTags {
        /// Finding UUID (or session short ID like C1, I2).
        id: String,

        /// Tags to add (repeatable).
        #[arg(long = "add")]
        add: Vec<String>,

        /// Tags to remove (repeatable).
        #[arg(long = "remove")]
        remove: Vec<String>,

        /// Agent performing the change.
        #[arg(long, default_value = "cli")]
        agent: String,
    },

    /// List available MCP capabilities (tools, resources, prompts) with descriptions and arguments.
    McpCapabilities,

    /// Manage rules in the rule registry.
    Rule {
        #[command(subcommand)]
        action: RuleCommand,
    },
}

/// Rule registry subcommands.
#[derive(Subcommand)]
pub enum RuleCommand {
    /// Register a new rule in the registry.
    Create {
        /// Rule ID (lowercase alphanumeric with hyphens, 2-64 chars).
        id: String,
        /// Human-readable name.
        #[arg(long)]
        name: String,
        /// Description of what this rule checks.
        #[arg(long)]
        description: String,
        /// Domain category (e.g., "safety", "security").
        #[arg(long)]
        category: Option<String>,
        /// Suggested severity for findings.
        #[arg(long)]
        severity_hint: Option<String>,
        /// Alternative names that map to this rule (repeatable).
        #[arg(long = "alias")]
        aliases: Vec<String>,
        /// CWE identifiers (repeatable).
        #[arg(long = "cwe")]
        cwe_ids: Vec<String>,
        /// Scope include patterns (repeatable glob patterns).
        #[arg(long = "scope-include")]
        scope_include: Vec<String>,
        /// Scope exclude patterns (repeatable glob patterns).
        #[arg(long = "scope-exclude")]
        scope_exclude: Vec<String>,
        /// Tags (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },

    /// Show full rule details (JSON).
    Get {
        /// Rule ID.
        id: String,
    },

    /// List all rules with optional filters.
    List {
        /// Filter by category.
        #[arg(long)]
        category: Option<String>,
        /// Filter by status (active, deprecated, experimental).
        #[arg(long)]
        status: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// Search rules by query text.
    Search {
        /// Search query (matched against rule IDs, aliases, descriptions).
        query: String,
        /// Max results (default: 10).
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Update rule fields.
    Update {
        /// Rule ID to update.
        id: String,
        /// New name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// New status (active, deprecated, experimental).
        #[arg(long)]
        status: Option<String>,
        /// Add alias (repeatable).
        #[arg(long = "add-alias")]
        add_alias: Vec<String>,
        /// Remove alias (repeatable).
        #[arg(long = "remove-alias")]
        remove_alias: Vec<String>,
        /// Add CWE ID (repeatable).
        #[arg(long = "add-cwe")]
        add_cwe: Vec<String>,
        /// Replace scope include patterns (repeatable).
        #[arg(long = "scope-include")]
        scope_include: Vec<String>,
        /// Replace scope exclude patterns (repeatable).
        #[arg(long = "scope-exclude")]
        scope_exclude: Vec<String>,
    },

    /// Deprecate a rule (set status to deprecated).
    Delete {
        /// Rule ID to deprecate.
        id: String,
        /// Reason for deprecation.
        #[arg(long)]
        reason: String,
    },

    /// Add a code example to a rule.
    AddExample {
        /// Rule ID.
        id: String,
        /// Example type (bad or good).
        #[arg(long = "type")]
        example_type: String,
        /// Programming language.
        #[arg(long)]
        language: String,
        /// Code snippet.
        #[arg(long)]
        code: String,
        /// Explanation.
        #[arg(long)]
        explanation: String,
    },

    /// Migrate existing findings into rule registry.
    Migrate,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Json,
    Table,
    Summary,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExportFormat {
    Sarif,
    Csv,
    Json,
}
