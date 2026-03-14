#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use tally::cli::handlers;
use tally::cli::{Cli, Command};
use tally::storage::GitFindingsStore;

fn init_tracing(mcp_mode: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    if mcp_mode {
        // MCP mode: log to stderr only, never stdout (stdout is JSON-RPC)
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .compact()
            .init();
    } else {
        // CLI mode: log to stderr
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .compact()
            .init();
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(matches!(cli.command, Command::McpServer));

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            match e {
                tally::error::TallyError::Git(_)
                | tally::error::TallyError::BranchNotFound { .. } => ExitCode::from(2),
                _ => ExitCode::from(1),
            }
        }
    }
}

fn store() -> tally::error::Result<GitFindingsStore> {
    GitFindingsStore::open(".")
}

#[allow(clippy::too_many_lines)] // dispatch function maps 1:1 with CLI subcommands
fn run(cli: Cli) -> tally::error::Result<()> {
    match cli.command {
        Command::Init => handlers::handle_init(&store()?),
        Command::Record {
            file,
            line,
            line_end,
            severity,
            title,
            rule,
            description,
            tags,
            agent,
            session,
            location,
            related_to,
            relationship,
            category,
            suggested_fix,
            evidence,
        } => handlers::handle_record(
            &store()?,
            &handlers::RecordArgs {
                file: &file,
                line,
                line_end,
                severity: &severity,
                title: &title,
                rule: &rule,
                description: &description,
                tags: &tags,
                agent: &agent,
                session: &session,
                extra_locations: &location,
                related_to: related_to.as_deref(),
                relationship: &relationship,
                category: &category,
                suggested_fix: suggested_fix.as_deref(),
                evidence: evidence.as_deref(),
            },
        ),
        Command::Query {
            status,
            severity,
            file,
            rule,
            related_to,
            format,
            limit,
        } => handlers::handle_query(
            &store()?,
            status.as_deref(),
            severity.as_deref(),
            file.as_deref(),
            rule.as_deref(),
            related_to.as_deref(),
            format,
            limit,
        ),
        Command::Update {
            id,
            status,
            reason,
            commit,
            agent,
            related_to,
            relationship,
        } => handlers::handle_update(
            &store()?,
            &handlers::UpdateArgs {
                id: &id,
                status: &status,
                reason: reason.as_deref(),
                commit: commit.as_deref(),
                agent: &agent,
                related_to: related_to.as_deref(),
                relationship: &relationship,
            },
        ),
        Command::Suppress {
            id,
            reason,
            expires,
            agent,
            suppression_type,
            suppression_pattern,
        } => handlers::handle_suppress(
            &store()?,
            &id,
            &reason,
            expires.as_deref(),
            &agent,
            &suppression_type,
            suppression_pattern.as_deref(),
        ),
        Command::RebuildIndex => handlers::handle_rebuild_index(&store()?),
        Command::RecordBatch { input, agent } => {
            handlers::handle_record_batch(&store()?, &input, &agent)
        }
        Command::Export { format, output } => {
            handlers::handle_export(&store()?, format, output.as_deref())
        }
        Command::Sync { remote } => handlers::handle_sync(&store()?, &remote),
        Command::Import { path } => handlers::handle_import(&store()?, &path),
        Command::Stats => handlers::handle_stats(&store()?),
        Command::McpServer => {
            let rt = tokio::runtime::Runtime::new().map_err(tally::error::TallyError::Io)?;
            rt.block_on(tally::mcp::server::run_mcp_server("."))
                .map_err(|e| tally::error::TallyError::Io(std::io::Error::other(e.to_string())))
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "tally", &mut std::io::stdout());
            Ok(())
        }
        Command::McpCapabilities => {
            handlers::handle_mcp_capabilities();
            Ok(())
        }
    }
}
