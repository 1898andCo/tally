#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use tally_ng::cli::{self, Cli, Command};
use tally_ng::storage::GitFindingsStore;

fn init_tracing(verbose: u8, quiet: u8) {
    use tracing_subscriber::EnvFilter;

    // RUST_LOG takes precedence if set
    let filter = if let Ok(env_filter) = EnvFilter::try_from_default_env() {
        env_filter
    } else {
        // Default: warn
        // -q: error, -qq: off
        // -v: info, -vv: debug, -vvv: trace
        let level = if quiet > 0 {
            match quiet {
                1 => "error",
                _ => "off",
            }
        } else {
            match verbose {
                0 => "warn",
                1 => "info",
                2 => "debug",
                _ => "trace",
            }
        };
        EnvFilter::new(level)
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(cli.verbose, cli.quiet);

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            match e {
                tally_ng::error::TallyError::Git(_)
                | tally_ng::error::TallyError::BranchNotFound { .. } => ExitCode::from(2),
                _ => ExitCode::from(1),
            }
        }
    }
}

fn store() -> tally_ng::error::Result<GitFindingsStore> {
    GitFindingsStore::open(".")
}

#[allow(clippy::too_many_lines)] // dispatch function maps 1:1 with CLI subcommands
fn run(cli: Cli) -> tally_ng::error::Result<()> {
    match cli.command {
        Command::Init => cli::handle_init(&store()?),
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
        } => cli::handle_record(
            &store()?,
            &cli::RecordArgs {
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
            tag,
            filter,
            since,
            before,
            agent,
            category,
            not_status,
            text,
            sort,
            sort_dir,
            format,
            limit,
        } => cli::handle_query(
            &store()?,
            status.as_deref(),
            severity.as_deref(),
            file.as_deref(),
            rule.as_deref(),
            related_to.as_deref(),
            tag.as_deref(),
            filter.as_deref(),
            since.as_deref(),
            before.as_deref(),
            agent.as_deref(),
            category.as_deref(),
            not_status.as_deref(),
            text.as_deref(),
            &sort,
            sort_dir.as_deref(),
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
        } => cli::handle_update(
            &store()?,
            &cli::UpdateArgs {
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
        } => cli::handle_suppress(
            &store()?,
            &id,
            &reason,
            expires.as_deref(),
            &agent,
            &suppression_type,
            suppression_pattern.as_deref(),
        ),
        Command::RebuildIndex => cli::handle_rebuild_index(&store()?),
        Command::RecordBatch { input, agent } => {
            cli::handle_record_batch(&store()?, &input, &agent)
        }
        Command::Export { format, output } => {
            cli::handle_export(&store()?, format, output.as_deref())
        }
        Command::Sync { remote } => cli::handle_sync(&store()?, &remote),
        Command::Import { path } => cli::handle_import(&store()?, &path),
        Command::Stats => cli::handle_stats(&store()?),
        Command::McpServer => {
            let rt = tokio::runtime::Runtime::new().map_err(tally_ng::error::TallyError::Io)?;
            rt.block_on(tally_ng::mcp::server::run_mcp_server("."))
                .map_err(|e| tally_ng::error::TallyError::Io(std::io::Error::other(e.to_string())))
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "tally", &mut std::io::stdout());
            Ok(())
        }
        Command::UpdateFields {
            id,
            title,
            description,
            suggested_fix,
            evidence,
            severity,
            category,
            tags,
            agent,
            format,
        } => cli::handle_update_fields(
            &store()?,
            &id,
            title.as_deref(),
            description.as_deref(),
            suggested_fix.as_deref(),
            evidence.as_deref(),
            severity.as_deref(),
            category.as_deref(),
            tags.as_deref(),
            &agent,
            format,
        ),
        Command::AddNote { id, text, agent } => cli::handle_add_note(&store()?, &id, &text, &agent),
        Command::ManageTags {
            id,
            add,
            remove,
            agent,
        } => cli::handle_manage_tags(&store()?, &id, &add, &remove, &agent),
        Command::McpCapabilities => {
            cli::handle_mcp_capabilities();
            Ok(())
        }
    }
}
