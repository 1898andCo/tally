//! CLI interface for tally — clap-based subcommands.

pub mod handlers;

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
pub enum Command {
    /// Initialize the findings-data branch in the current repo.
    Init,

    /// Record a new finding (or deduplicate with existing).
    Record {
        /// File path where the finding was discovered.
        #[arg(long)]
        file: String,

        /// Line number (start).
        #[arg(long)]
        line: u32,

        /// Line number (end). Defaults to same as --line.
        #[arg(long)]
        line_end: Option<u32>,

        /// Severity: critical, important, suggestion, tech-debt.
        #[arg(long)]
        severity: String,

        /// Short title of the finding.
        #[arg(long)]
        title: String,

        /// Rule ID for grouping (e.g., "unsafe-unwrap", "sql-injection").
        #[arg(long)]
        rule: String,

        /// Detailed description.
        #[arg(long, default_value = "")]
        description: String,

        /// Comma-separated tags.
        #[arg(long, default_value = "")]
        tags: String,

        /// Agent identifier (e.g., "claude-code", "cursor").
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Session identifier.
        #[arg(long, default_value = "")]
        session: String,

        /// Additional locations (format: `file:line_start:line_end:role`).
        /// Role is primary, secondary, or context. Can be repeated.
        #[arg(long)]
        location: Vec<String>,

        /// Related finding ID (UUID or short ID like C1).
        #[arg(long)]
        related_to: Option<String>,

        /// Relationship type (used with `--related-to`).
        #[arg(long, default_value = "related_to")]
        relationship: String,

        /// Category for grouping (e.g., "injection", "auth").
        #[arg(long, default_value = "")]
        category: String,

        /// Suggested fix or remediation.
        #[arg(long)]
        suggested_fix: Option<String>,

        /// Evidence or code snippet supporting the finding.
        #[arg(long)]
        evidence: Option<String>,
    },

    /// Query findings with filters.
    Query {
        /// Filter by status (e.g., "open", "resolved").
        #[arg(long)]
        status: Option<String>,

        /// Filter by severity (e.g., "critical", "important").
        #[arg(long)]
        severity: Option<String>,

        /// Filter by file path (substring match).
        #[arg(long)]
        file: Option<String>,

        /// Filter by rule ID.
        #[arg(long)]
        rule: Option<String>,

        /// Filter by related finding ID (shows findings related to this ID).
        #[arg(long)]
        related_to: Option<String>,

        /// Output format.
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,

        /// Max results.
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Update a finding's lifecycle status.
    Update {
        /// Finding UUID (or session short ID like C1, I2).
        id: String,

        /// Target status.
        #[arg(long)]
        status: String,

        /// Reason for the transition.
        #[arg(long)]
        reason: Option<String>,

        /// Commit SHA that fixed the finding.
        #[arg(long)]
        commit: Option<String>,

        /// Agent performing the update.
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Add relationship to another finding (UUID or short ID).
        #[arg(long)]
        related_to: Option<String>,

        /// Relationship type (used with --related-to).
        #[arg(long, default_value = "related_to")]
        relationship: String,
    },

    /// Suppress a finding.
    Suppress {
        /// Finding UUID (or session short ID).
        id: String,

        /// Reason for suppression.
        #[arg(long)]
        reason: String,

        /// Expiry date (ISO 8601). Omit for permanent suppression.
        #[arg(long)]
        expires: Option<String>,

        /// Agent performing the suppression.
        #[arg(long, default_value = "cli")]
        agent: String,

        /// Suppression type: global, file, or inline.
        #[arg(long, default_value = "global")]
        suppression_type: String,

        /// Inline suppression pattern (used with --suppression-type inline).
        #[arg(long)]
        suppression_pattern: Option<String>,
    },

    /// Rebuild the index.json from finding files.
    RebuildIndex,

    /// Record multiple findings from a JSONL file or stdin.
    RecordBatch {
        /// Path to JSONL file. Use "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Agent identifier.
        #[arg(long, default_value = "cli")]
        agent: String,
    },

    /// Export findings in SARIF, CSV, or JSON format.
    Export {
        /// Output format.
        #[arg(long, value_enum)]
        format: ExportFormat,

        /// Output file. Defaults to stdout.
        #[arg(long)]
        output: Option<String>,
    },

    /// Sync findings with remote (fetch + merge + push).
    Sync {
        /// Remote name.
        #[arg(long, default_value = "origin")]
        remote: String,
    },

    /// Import findings from dclaude/zclaude state files.
    Import {
        /// Path to the state JSON file.
        path: String,
    },

    /// Show summary statistics.
    Stats,

    /// Run as MCP server over stdio.
    McpServer,

    /// Generate shell completions for bash, zsh, fish, or powershell.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// List available MCP capabilities (tools, resources, prompts).
    McpCapabilities,
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
