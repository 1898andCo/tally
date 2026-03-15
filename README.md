# tally

Git-backed findings tracker for AI coding agents.

Provides persistent, content-addressable finding identity across sessions, agents, PRs, and branches with full lifecycle tracking.

## Features

- **Stable identity**: UUID v7 + content fingerprint (SHA-256) for deduplication and re-matching after refactoring
- **Lifecycle tracking**: 10-state machine with validated transitions and full audit trail
- **Multi-agent**: Cross-agent deduplication via fingerprint matching, session-scoped short IDs (C1, I2, S3, TD4)
- **Git-backed**: One-file-per-finding on an orphan branch, zero merge conflicts for concurrent writes
- **Dual interface**: CLI for scripts/CI + MCP server for Claude Code, Cursor, Windsurf
- **MCP server**: 11 tools, 7 resource templates, 5 prompt templates with rich descriptions for AI agents
- **Export**: SARIF 2.1.0 (GitHub Code Scanning), CSV, JSON
- **Import**: dclaude and zclaude state file migration
- **Schema evolution**: Versioned findings with forward-compatible deserialization
- **Structured logging**: tracing spans with `-v`/`-q` verbosity control and `RUST_LOG` support
- **Shell completions**: bash, zsh, fish, powershell via `tally completions`

## Installation

### From crates.io

```bash
cargo install tally-ng
```

### With cargo-binstall (prebuilt binaries)

```bash
cargo binstall tally-ng
```

### With Homebrew (macOS/Linux)

```bash
brew tap 1898andCo/tap
brew install tally
```

### From source

```bash
cargo install --path .
```

### Upgrading

Same commands as installation — they replace the existing binary:

```bash
# From crates.io
cargo install tally-ng

# With cargo-binstall
cargo binstall tally-ng

# With Homebrew (update tap first to fetch latest formula)
brew update && brew upgrade tally
```

**v0.4.0 → v0.5.0 migration:** No action required. Existing findings on the `findings-data` branch are fully compatible — tally v0.5.0 reads v0.4.0 finding JSON files without migration. New fields (`notes`, `edit_history`) default to empty arrays via `#[serde(default)]`.

## Quick Start

```bash
# Initialize findings storage in your git repo
tally init

# Record a finding
tally record \
  --file src/main.rs --line 42 \
  --severity important \
  --title "unwrap on Option" \
  --rule unsafe-unwrap

# Query findings
tally query --severity critical --format table

# Update status
tally update <uuid> --status in-progress --reason "fixing now"

# Suppress a finding
tally suppress <uuid> --reason "accepted risk" --expires 2026-06-01T00:00:00Z

# Sync with remote
tally sync

# Export for GitHub Code Scanning
tally export --format sarif --output findings.sarif

# List MCP capabilities
tally mcp-capabilities

# Generate shell completions
tally completions zsh > ~/.zfunc/_tally
```

## CLI Reference

### Global Flags

```
-v, --verbose    Increase logging verbosity (-v info, -vv debug, -vvv trace)
-q, --quiet      Decrease logging verbosity (-q error, -qq off)
```

Logging can also be controlled with the `RUST_LOG` environment variable (e.g., `RUST_LOG=tally=debug`).

### Subcommands

| Command | Description |
|---------|-------------|
| `init` | Initialize `findings-data` orphan branch |
| `record` | Create or deduplicate a single finding |
| `record-batch` | Batch record from JSONL file or stdin |
| `query` | Search findings with filters (including `--tag`) |
| `update` | Change lifecycle status |
| `update-fields` | Edit mutable fields (title, description, severity, etc.) |
| `note` | Add timestamped note without changing status |
| `tag` | Add/remove tags (`--add X --remove Y`) |
| `suppress` | Suppress with reason and optional expiry |
| `stats` | Summary statistics (severity, status, notes, edits, top tags) |
| `sync` | Pull + merge + push findings-data branch |
| `import` | Import from dclaude/zclaude state files |
| `export` | Export as SARIF 2.1.0, CSV, or JSON |
| `rebuild-index` | Rebuild `index.json` from finding files |
| `mcp-server` | Run as MCP server over stdio |
| `mcp-capabilities` | List available MCP tools, resources, and prompts |
| `completions` | Generate shell completions (bash/zsh/fish/powershell) |

### `tally record`

```
--file <path>           File path (required)
--line <n>              Start line (required)
--line-end <n>          End line (defaults to --line)
--severity <level>      critical | important | suggestion | tech-debt (required)
--title <text>          Short title (required)
--rule <id>             Rule ID for grouping (required)
--description <text>    Detailed description
--tags <csv>            Comma-separated tags
--agent <id>            Agent identifier (default: cli)
--session <id>          Session identifier
--location <spec>       Additional location: file:line:role or file:start:end:role (repeatable)
--related-to <id>       Related finding ID (UUID or short ID)
--relationship <type>   Relationship type (default: related_to)
--category <name>       Category for grouping (e.g., injection, auth)
--suggested-fix <text>  Suggested fix or remediation
--evidence <text>       Evidence or code snippet supporting the finding
```

### `tally query`

```
--status <state>        Filter by lifecycle status
--severity <level>      Filter by severity
--file <pattern>        Filter by file path (substring match)
--rule <id>             Filter by rule ID
--related-to <id>       Filter by related finding ID
--format <fmt>          json | table | summary (default: json)
--limit <n>             Max results (default: 100)
```

### `tally suppress`

```
<id>                    Finding UUID or session short ID
--reason <text>         Reason for suppression (required)
--expires <datetime>    Expiry date (ISO 8601, omit for permanent)
--agent <id>            Agent identifier (default: cli)
--suppression-type <t>  global | file | inline (default: global)
--suppression-pattern   Inline suppression pattern (with --suppression-type inline)
```

### `tally completions`

```bash
# Generate and install completions
tally completions bash > ~/.bash_completion.d/tally
tally completions zsh > ~/.zfunc/_tally
tally completions fish > ~/.config/fish/completions/tally.fish
```

### `tally update`

```
<id>                    Finding UUID or session short ID (C1, I2, etc.)
--status <state>        Target lifecycle status (required)
--reason <text>         Reason for the transition
--commit <sha>          Commit SHA that fixed the finding
--agent <id>            Agent identifier (default: cli)
--related-to <id>       Add relationship to another finding
--relationship <type>   Relationship type (default: related_to)
```

### `tally export`

```
--format <fmt>          sarif | csv | json (required)
--output <path>         Output file (defaults to stdout)
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (including empty query results) |
| 1 | Application error (invalid args, invalid transition) |
| 2 | Git storage error (branch not found, commit failed) |

## MCP Server

Configure in `.mcp.json` for Claude Code:

```json
{
  "mcpServers": {
    "tally": {
      "command": "tally",
      "args": ["mcp-server"]
    }
  }
}
```

### Tools (15)

| Tool | Description |
|------|-------------|
| `initialize_store` | Initialize the findings-data branch (idempotent) |
| `record_finding` | Create or deduplicate a finding |
| `record_batch` | Batch record multiple findings |
| `query_findings` | Search with filters (status, severity, file, rule, tag) |
| `update_finding_status` | Transition lifecycle state |
| `update_finding` | Edit mutable fields (title, description, severity, etc.) with audit trail |
| `get_finding_context` | Retrieve finding with full context, notes, and edit history |
| `add_note` | Append timestamped note without changing status |
| `add_tag` | Add tags to a finding (merge, dedup) |
| `remove_tag` | Remove tags from a finding (exact match) |
| `suppress_finding` | Suppress with reason and expiry |
| `export_findings` | Export as JSON, CSV, or SARIF 2.1.0 |
| `sync_findings` | Sync findings-data branch with remote |
| `rebuild_index` | Rebuild index.json from finding files |
| `import_findings` | Import from dclaude/zclaude state files |

### Resources (7)

| URI | Description |
|-----|-------------|
| `findings://summary` | Counts by severity/status, 10 most recent |
| `findings://file/{path}` | All findings in a specific file |
| `findings://detail/{uuid}` | Full finding with history, relationships, tags, PR context |
| `findings://severity/{level}` | All findings at a severity level |
| `findings://status/{state}` | All findings in a lifecycle state |
| `findings://rule/{rule_id}` | All findings matching a rule ID |
| `findings://pr/{pr_number}` | All findings from a specific PR |

### Prompts (5)

| Prompt | Description |
|--------|-------------|
| `triage-file` | Triage all findings in a file by severity and suggest resolution order |
| `fix-finding` | Generate a fix plan for a specific finding with code changes |
| `summarize-findings` | Executive summary of all open findings with risk assessment |
| `review-pr` | Review a PR's changes against tracked findings |
| `explain-finding` | Explain a finding's context, impact, and remediation options |

## Storage Model

Findings are stored on an orphan branch `findings-data` as individual JSON files:

```
findings-data branch:
  findings/
    019e1a2b-3c4d-7e5f-8a9b-0c1d2e3f4a5b.json
    019e1a2b-4d5e-7f6a-9b0c-1d2e3f4a5b6c.json
  index.json          # Derived index (regenerable)
  schema.json         # Schema version
  .gitattributes      # merge=ours for index.json
```

The working tree is never modified. All operations use `git2` plumbing (blob/tree/commit objects).

### Protecting the findings-data branch

Repos using tally should add a branch protection rule (or GitHub ruleset) for `findings-data` with **"Restrict deletions"** enabled. No status checks or PR requirements are needed — just prevent accidental `git push --delete origin findings-data`.

```bash
# Via GitHub CLI (requires admin access)
gh api repos/OWNER/REPO/rulesets -f name="protect-findings-data" \
  -f target=branch -f enforcement=active \
  -f 'conditions[ref_name][include][]=refs/heads/findings-data' \
  -f 'rules[][type]=deletion'
```

`tally stats` will warn if findings-data has no upstream tracking branch (local-only findings).

### Why one-file-per-finding

- Zero merge conflicts for concurrent writes (each finding is a unique file)
- Works with GitHub server-side merge (custom merge drivers are ignored by GitHub)
- Per-finding git history via `git log`
- Scales to hundreds of concurrent agents

## Severity Levels

| Level | Short ID Prefix | SARIF Mapping |
|-------|----------------|---------------|
| Critical | C | error |
| Important | I | warning |
| Suggestion | S | note |
| Tech Debt | TD | none |

## Lifecycle States

```
Open -> Acknowledged -> InProgress -> Resolved -> Closed
  |         |              |
  |         +-> FalsePositive -> Reopened -> Acknowledged
  |         +-> WontFix -------> Reopened -> InProgress
  |         +-> Deferred ------> Open / Reopened / Closed
  +-> Suppressed --------------> Open / Reopened / Closed
```

Deferred and Suppressed findings can be reopened when new information surfaces (v0.5.0). Reopened findings can then transition to Acknowledged or InProgress through the existing path. Closed is terminal.

## Relationship Types

| Type | Description |
|------|-------------|
| `duplicate_of` | This finding duplicates another |
| `blocks` | This finding blocks resolution of another |
| `related_to` | General relationship |
| `causes` | Fixing this may resolve another |
| `discovered_while_fixing` | Found while working on another |
| `supersedes` | This finding replaces another |

## Integration Examples

### CI Pipeline

```bash
# Export SARIF for GitHub Code Scanning
tally export --format sarif --output findings.sarif
```

### Import from dclaude/zclaude

```bash
# Import dclaude findings
tally import .claude/pr-reviews/owner/repo/42.json

# Import zclaude findings
tally import .claude/pr-reviews/42.json
```

### Pre-commit Hook

```bash
# Record findings from a linter
my-linter --output jsonl | tally record-batch --agent my-linter
```

## Schema Evolution

Every finding includes a `schema_version` field. All fields use `#[serde(default)]` for forward-compatible deserialization — findings created by older versions of tally are readable by newer versions without migration. Enums use `#[non_exhaustive]` to allow adding variants without breaking existing consumers.

## License

Apache-2.0
