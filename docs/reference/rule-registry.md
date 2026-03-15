# Rule Registry Reference

## Overview

The rule registry provides centralized rule management for tally findings. Rules
define the categories of issues that agents discover, enabling consistent naming,
deduplication across agents, and lifecycle management.

## Rule Format

Rules are stored as individual JSON files at `rules/<rule-id>.json` on the
`findings-data` branch. Each rule has:

- **id**: Canonical rule ID (lowercase alphanumeric + hyphens, 2-64 chars)
- **name**: Human-readable name
- **description**: What this rule checks
- **category**: Domain grouping (safety, security, spec-compliance, etc.)
- **severity_hint**: Suggested severity for findings
- **aliases**: Alternative names that resolve to this rule
- **cwe_ids**: Associated CWE identifiers
- **scope**: Include/exclude glob patterns for file applicability
- **examples**: Bad/good code patterns
- **status**: active, deprecated, or experimental

## Matching Pipeline

When recording a finding, the rule ID goes through a matching pipeline:

1. **Normalize**: lowercase, `_` to `-`, strip agent namespace prefix
2. **Exact match**: HashMap lookup on canonical IDs
3. **Alias lookup**: Check all rules' alias arrays
4. **CWE cross-reference**: Match by CWE ID (suggestion only, confidence 0.7)
5. **Jaro-Winkler similarity**: Fuzzy match on IDs (suggestion only)
6. **Token Jaccard**: Description similarity (suggestion only)
7. **Semantic embedding**: Cosine similarity (feature-gated, optional)

Only exact match and alias lookup auto-resolve. All fuzzy matches populate
`similar_rules` as suggestions for the agent to consider.

Unknown rule IDs are auto-registered with status `experimental`.

## CLI Commands

```bash
tally rule create <id> --name "..." --description "..." [--alias ...] [--cwe ...]
tally rule get <id>
tally rule list [--category ...] [--status ...] [--format table|json]
tally rule search <query> [--limit 10]
tally rule update <id> [--name ...] [--add-alias ...] [--status ...]
tally rule delete <id> --reason "..."
tally rule add-example <id> --type bad --language rust --code "..." --explanation "..."
tally rule migrate
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `create_rule` | Register a new rule |
| `get_rule` | Retrieve rule by ID |
| `search_rules` | Search by query text |
| `list_rules` | List with optional filters |
| `update_rule` | Update mutable fields |
| `delete_rule` | Deprecate a rule |
| `add_rule_example` | Add code example |
| `migrate_rules` | Auto-register from findings |

## Record Response Fields

When recording findings, the response includes:

- `rule_id`: Canonical rule ID (after normalization/alias resolution)
- `original_rule_id`: What the agent originally sent (if different)
- `normalized_by`: Method used (exact, alias, auto_registered)
- `similar_rules`: Array of suggested similar rules
- `scope_warning`: Advisory if file is outside rule's scope

## Rule ID Format

Rule IDs must match: `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$`

- 2-64 characters
- Lowercase alphanumeric and hyphens only
- No leading/trailing hyphens
- Examples: `unsafe-unwrap`, `sql-injection`, `spec-drift`
