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
        Command::RebuildIndex { include_rules } => {
            cli::handle_rebuild_index(&store()?, include_rules)
        }
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
        Command::Rule { action } => handle_rule_command(action),
    }
}

fn handle_rule_command(action: cli::RuleCommand) -> tally_ng::error::Result<()> {
    let s = store()?;
    match action {
        cli::RuleCommand::Create {
            id,
            name,
            description,
            category,
            severity_hint,
            aliases,
            cwe_ids,
            scope_include,
            scope_exclude,
            tags,
        } => cli::rule::handle_rule_create(
            &s,
            &id,
            &name,
            &description,
            category.as_deref(),
            severity_hint.as_deref(),
            &aliases,
            &cwe_ids,
            &scope_include,
            &scope_exclude,
            &tags,
        ),
        cli::RuleCommand::Get { id } => cli::rule::handle_rule_get(&s, &id),
        cli::RuleCommand::List {
            category,
            status,
            format,
        } => cli::rule::handle_rule_list(&s, category.as_deref(), status.as_deref(), format),
        cli::RuleCommand::Search {
            query,
            method,
            limit,
        } => cli::rule::handle_rule_search(&s, &query, &method, limit),
        cli::RuleCommand::Reindex { embeddings } => cli::rule::handle_rule_reindex(&s, embeddings),
        cli::RuleCommand::Update {
            id,
            name,
            description,
            status,
            add_alias,
            remove_alias,
            add_cwe,
            scope_include,
            scope_exclude,
        } => cli::rule::handle_rule_update(
            &s,
            &id,
            name.as_deref(),
            description.as_deref(),
            status.as_deref(),
            &add_alias,
            &remove_alias,
            &add_cwe,
            &scope_include,
            &scope_exclude,
        ),
        cli::RuleCommand::Delete { id, reason } => cli::rule::handle_rule_delete(&s, &id, &reason),
        cli::RuleCommand::AddExample {
            id,
            example_type,
            language,
            code,
            explanation,
        } => cli::rule::handle_rule_add_example(
            &s,
            &id,
            &example_type,
            &language,
            &code,
            &explanation,
        ),
        cli::RuleCommand::Migrate => cli::rule::handle_rule_migrate(&s),
    }
}
