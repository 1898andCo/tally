#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use tally::cli::handlers;
use tally::cli::{Cli, Command};
use tally::storage::GitFindingsStore;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            match e {
                tally::error::TallyError::Git(_) | tally::error::TallyError::BranchNotFound { .. } => {
                    ExitCode::from(2)
                }
                _ => ExitCode::from(1),
            }
        }
    }
}

fn run(cli: Cli) -> tally::error::Result<()> {
    match cli.command {
        Command::Init => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_init(&store)
        }
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
        } => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_record(
                &store,
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
                },
            )
        }
        Command::Query {
            status,
            severity,
            file,
            rule,
            format,
            limit,
        } => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_query(
                &store,
                status.as_deref(),
                severity.as_deref(),
                file.as_deref(),
                rule.as_deref(),
                format,
                limit,
            )
        }
        Command::Update {
            id,
            status,
            reason,
            commit,
            agent,
        } => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_update(
                &store,
                &id,
                &status,
                reason.as_deref(),
                commit.as_deref(),
                &agent,
            )
        }
        Command::Suppress {
            id,
            reason,
            expires,
            agent,
        } => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_suppress(
                &store,
                &id,
                &reason,
                expires.as_deref(),
                &agent,
            )
        }
        Command::Stats => {
            let store = GitFindingsStore::open(".")?;
            handlers::handle_stats(&store)
        }
        Command::McpServer => {
            eprintln!("MCP server not yet implemented (Task 5)");
            Ok(())
        }
    }
}
