# tally

Git-backed findings tracker for AI coding agents.

Provides persistent, content-addressable finding identity across sessions, agents, PRs, and branches with full lifecycle tracking.

## Features

- **Stable identity**: UUID v7 + content fingerprint (SHA-256) for deduplication and re-matching after refactoring
- **Lifecycle tracking**: 10-state machine with validated transitions and full audit trail
- **Multi-agent**: Cross-agent deduplication via fingerprint matching, session-scoped short IDs (C1, I2, S3, TD4)
- **Git-backed**: One-file-per-finding on an orphan branch, zero merge conflicts for concurrent writes
- **Dual interface**: CLI for scripts/CI + MCP server for Claude Code, Cursor, Windsurf
- **Export**: SARIF 2.1.0 (GitHub Code Scanning), CSV, JSON
- **Import**: dclaude and zclaude state file migration

## Installation

### From source

```bash
cargo install --path .
```

### From crates.io (planned)

```bash
cargo install tally
```

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
```

## CLI Reference

### Subcommands

| Command | Description |
|---------|-------------|
| `init` | Initialize `findings-data` orphan branch |
| `record` | Create or deduplicate a single finding |
| `record-batch` | Batch record from JSONL file or stdin |
| `query` | Search findings with filters |
| `update` | Change lifecycle status |
| `suppress` | Suppress with reason and optional expiry |
| `stats` | Summary statistics |
| `sync` | Pull + merge + push findings-data branch |
| `import` | Import from dclaude/zclaude state files |
| `export` | Export as SARIF 2.1.0, CSV, or JSON |
| `mcp-server` | Run as MCP server over stdio |

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
```

### `tally query`

```
--status <state>        Filter by lifecycle status
--severity <level>      Filter by severity
--file <pattern>        Filter by file path (substring match)
--rule <id>             Filter by rule ID
--format <fmt>          json | table | summary (default: json)
--limit <n>             Max results (default: 100)
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

### Tools (6)

| Tool | Description |
|------|-------------|
| `record_finding` | Create or deduplicate a finding |
| `record_batch` | Batch record multiple findings |
| `query_findings` | Search with filters |
| `update_finding_status` | Transition lifecycle state |
| `get_finding_context` | Retrieve finding with full context |
| `suppress_finding` | Suppress with reason and expiry |

### Resources (3)

| URI | Description |
|-----|-------------|
| `findings://summary` | Counts by severity/status, 10 most recent |
| `findings://file/{path}` | All findings in a specific file |
| `findings://detail/{uuid}` | Full finding with history |

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
  |         +-> Deferred ------> Open
  +-> Suppressed --------------> Open
```

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

## License

Apache-2.0
