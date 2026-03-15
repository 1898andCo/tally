# Story 1.1: TallyQL — Advanced Query Language & Enhanced Filters

Status: ready-for-dev
Repository: `1898andCo/tally`
Language: Rust
License: Apache-2.0
Date: 2026-03-14
Epic: Query & Search Enhancements

---

## Problem Statement

Tally's current query capability is limited to 6 simple AND-combined CLI flags (`--status`, `--severity`, `--file`, `--rule`, `--related-to`, `--tag`) with no boolean logic, no negation, no date filtering, no sorting, no full-text search, and no expression language. As finding counts grow across projects, users cannot answer basic questions like:

1. **"Show me all open critical findings NOT in test files"** — impossible (no negation)
2. **"Findings created in the last 7 days"** — impossible (no date filtering)
3. **"Critical OR important findings in src/api/"** — impossible (no OR operator)
4. **"Findings sorted by severity, then by creation date"** — impossible (no sorting)
5. **"Search for 'unwrap' across titles and descriptions"** — impossible (no full-text search)
6. **"Findings discovered by dclaude agent"** — impossible (no agent filter)

The axiathon project has a mature query language (AxiQL) built on Chumsky 0.10 with boolean operators, string ops, comparisons, and structured error reporting. Much of this design translates directly to tally's domain.

---

## Solution

Add a two-layer query system:

1. **Enhanced CLI flags (Option A)** — new flags for missing filters (`--since`, `--before`, `--agent`, `--category`, `--not-status`, `--sort`, `--text`, multi-value `--severity critical,important`), fully backward compatible
2. **TallyQL expression language (Option B)** — a mini filter expression language adapted from axiathon's AxiQL parser, supporting boolean operators (AND, OR, NOT), comparisons, string ops (CONTAINS, STARTSWITH, ENDSWITH), date literals, and parenthesized grouping

Both layers compile to the same internal `FilterExpr` AST, evaluated in-memory against `Vec<Finding>`.

---

## Story

As a **developer using tally to track findings**,
I want **powerful query expressions with boolean logic, date ranges, sorting, and full-text search**,
So that **I can quickly find the findings I care about without writing scripts or manually scanning JSON output**.

As an **AI coding agent** using tally's MCP interface,
I want **a filter expression parameter that supports complex predicates**,
So that **I can query findings precisely without needing one MCP call per filter combination**.

---

## Acceptance Criteria

### AC-1: Enhanced CLI Flags
- `--since <duration|datetime>` filters findings created after the given time (e.g., `--since 7d`, `--since 2026-03-01`)
- `--before <duration|datetime>` filters findings created before the given time
- `--agent <id>` filters by `discovered_by` agent ID (exact match)
- `--category <name>` filters by category field (exact match)
- `--not-status <state>` excludes findings with the given status
- `--sort <field>` sorts results by field (severity, created_at, updated_at, file, rule, title) with optional `--sort-dir asc|desc` (default: desc for dates, asc for text)
- `--text <search>` searches across title, description, suggested_fix, and evidence (case-insensitive substring)
- `--severity` accepts comma-separated values: `--severity critical,important`
- `--status` accepts comma-separated values: `--status open,acknowledged,in_progress`
- All new flags combine with existing flags via AND logic
- All new flags are backward compatible — existing queries produce identical results

### AC-2: TallyQL Expression Language
- New `--filter <expr>` flag accepts a TallyQL expression string
- Supported operators: `=`, `!=`, `>`, `<`, `>=`, `<=`
- Boolean operators: `AND`, `OR`, `NOT` (case-insensitive), plus `&&`, `||`, `!` aliases
- String operators: `CONTAINS`, `STARTSWITH`, `ENDSWITH` (case-insensitive keywords)
- Existence operators: `HAS`, `MISSING` (for optional fields like suggested_fix, category)
- IN lists: `severity IN (critical, important)`
- Parenthesized grouping with nesting depth limit (64)
- Field names: `severity`, `status`, `file`, `rule`, `title`, `description`, `suggested_fix`, `evidence`, `category`, `agent`, `created_at`, `updated_at`, `tag`
- Date literals: relative durations (`7d`, `24h`, `30m`) and ISO 8601 (`2026-03-01`, `2026-03-01T12:00:00Z`)
- Quoted string values: `title CONTAINS "unwrap"`
- Unquoted enum values for severity/status: `severity = critical`
- `--filter` combines with CLI flags via AND
- Parse errors produce structured messages with span, expected, found, and hint

### AC-3: MCP Integration
- `query_findings` MCP tool accepts new `filter` parameter (TallyQL expression string)
- `query_findings` MCP tool accepts new `sort` parameter
- All new CLI filters available as MCP tool parameters
- MCP tool description updated with TallyQL syntax examples

### AC-4: Sorting
- Results can be sorted by: `severity`, `status`, `created_at`, `updated_at`, `file`, `rule`, `title`
- Multi-field sorting: `--sort severity --sort created_at` (first key primary)
- Severity sort order: critical > important > suggestion > tech_debt
- Default sort: `created_at desc` (newest first) when `--sort` is specified without `--sort-dir`

### AC-5: Parser Adapted from Axiathon
- Parser uses Chumsky 0.10 (same version as axiathon-query)
- AST reuses axiathon's `FilterExpr` pattern (And, Or, Not, Comparison, StringMatch, Has, Missing, InList)
- Max query length: 8 KB (CWE-400, proportional to tally's scale)
- Max nesting depth: 64 (CWE-674)
- No SQL mode, no pipe stages, no CIDR, no regex, no wildcards (tally doesn't need them)
- Structured error type with `thiserror` (adapted from axiathon's `AxiQLError`)

### AC-6: Documentation
- README.md query section updated with all new flags and TallyQL syntax
- `tally query --help` shows TallyQL examples in help text
- MCP tool descriptions include filter expression examples

---

## Tasks / Subtasks

- [ ] Task 1: Add Chumsky dependency and create query module (AC: 5)
  - [ ] 1.1: Add `chumsky = "0.10"` and `humantime = "2"` to Cargo.toml dependencies (`proptest = "1"` already in dev-dependencies)
  - [ ] 1.2: Create `src/query/mod.rs` with public API re-exports
  - [ ] 1.3: Create `src/query/ast.rs` — TallyQL AST types (FilterExpr, CompareOp, StringOp, Value, SortSpec)
  - [ ] 1.4: Create `src/query/error.rs` — TallyQLError with Parse variant, span, expected, found, hint
  - [ ] 1.5: Create `src/query/fields.rs` — field name registry mapping TallyQL field names to Finding struct accessors
  - [ ] 1.6: Update `src/main.rs` to declare `mod query`

- [ ] Task 2: Build TallyQL parser (AC: 2, 5)
  - [ ] 2.1: Create `src/query/parser.rs` — Chumsky recursive descent parser
  - [ ] 2.2: Implement value parsers: quoted strings, unquoted enums (severity/status names), integers, relative durations via `humantime::parse_duration()` (`7d`, `24h`), ISO 8601 dates as quoted strings with post-hoc detection in evaluator
  - [ ] 2.3: Implement field ref parser (simple identifiers, no dotted paths needed)
  - [ ] 2.4: Implement comparison operators (=, !=, >, <, >=, <=)
  - [ ] 2.5: Implement string operators (CONTAINS, STARTSWITH, ENDSWITH)
  - [ ] 2.6: Implement existence operators (HAS, MISSING)
  - [ ] 2.7: Implement IN lists: `field IN (val1, val2, val3)`
  - [ ] 2.8: Implement boolean operators with precedence: NOT > AND > OR
  - [ ] 2.9: Implement parenthesized grouping with depth guard
  - [ ] 2.10: Implement `parse_tallyql()` entry point with length check and error mapping

- [ ] Task 3: Build AST evaluator (AC: 2)
  - [ ] 3.1: Create `src/query/eval.rs` — evaluate FilterExpr against a Finding
  - [ ] 3.2: Implement field value extraction: map field names to Finding struct fields
  - [ ] 3.3: Implement comparison evaluation with type coercion (string, enum, datetime)
  - [ ] 3.4: Implement string op evaluation (contains, starts_with, ends_with — case-insensitive)
  - [ ] 3.5: Implement boolean logic (and, or, not)
  - [ ] 3.6: Implement HAS/MISSING for optional fields (suggested_fix, category, evidence, tags)
  - [ ] 3.7: Implement IN list evaluation
  - [ ] 3.8: Implement date comparison: relative durations (`7d`) resolve to `Utc::now() - chrono::TimeDelta::from_std(humantime_duration)`, ISO 8601 strings parse via `chrono::DateTime::parse_from_rfc3339()` or `NaiveDate::parse_from_str("%Y-%m-%d")` + midnight UTC

- [ ] Task 4: Enhanced CLI flags (AC: 1, 4)
  - [ ] 4.1: Add new flags to `Command::Query` in `src/cli/mod.rs`
  - [ ] 4.2: Update `src/cli/query.rs` — add since/before date parsing and filtering
  - [ ] 4.3: Add agent and category filters
  - [ ] 4.4: Add not-status exclusion filter
  - [ ] 4.5: Add text search across title/description/suggested_fix/evidence
  - [ ] 4.6: Add multi-value parsing for --severity and --status (comma-separated)
  - [ ] 4.7: Add --filter flag that parses TallyQL expression and applies it
  - [ ] 4.8: Add --sort and --sort-dir flags with multi-field support
  - [ ] 4.9: Implement sorting logic (severity uses custom ordering, dates use chronological)
  - [ ] 4.10: Update match arm in `src/main.rs` to pass new parameters

- [ ] Task 5: MCP integration (AC: 3)
  - [ ] 5.1: Add `filter`, `sort`, `since`, `before`, `agent`, `category`, `text` to `QueryFindingsInput`
  - [ ] 5.2: Update `query_findings` MCP handler to apply TallyQL filter expression
  - [ ] 5.3: Update `query_findings` MCP handler to apply new simple filters
  - [ ] 5.4: Update `query_findings` MCP tool description with TallyQL syntax examples
  - [ ] 5.5: Update MCP capabilities listing

- [ ] Task 6: Tests (AC: 1-5)
  - [ ] 6.1: Parser tests — each operator, boolean logic, precedence, parentheses, error messages
  - [ ] 6.2: Evaluator tests — each field, each comparison type, date arithmetic
  - [ ] 6.3: CLI integration tests — new flags, combined filters, backward compatibility
  - [ ] 6.4: MCP tests — filter parameter, new input fields
  - [ ] 6.5: Property tests — arbitrary expressions roundtrip, filter never panics on any input
  - [ ] 6.6: Negative tests — invalid syntax, unknown fields, type mismatches, depth exceeded, length exceeded

- [ ] Task 7: Documentation (AC: 6)
  - [ ] 7.1: Update README.md query section with new flags and TallyQL syntax reference
  - [ ] 7.2: Update CLI help text with examples
  - [ ] 7.3: Update MCP tool descriptions

---

## Dev Notes

### Architecture Decision: Single Binary, No Workspace Crate

Unlike axiathon which has `axiathon-query` as a separate crate in a workspace, tally is a single binary crate. The query module lives at `src/query/` as an internal module, not a separate crate. This keeps tally simple.

### What We're Adapting from Axiathon's AxiQL

**Reuse directly (adapt to tally's domain):**
- `FilterExpr` AST pattern (And, Or, Not, Comparison, StringMatch, Has, Missing, InList)
- Chumsky 0.10 recursive descent parser structure
- `kw()` helper for case-insensitive keywords
- `quoted_string()` parser with escape handling
- Boolean operator precedence: NOT > AND > OR via `foldl` chains
- Nesting depth guard with `Rc<Cell<usize>>`
- `AxiQLError` -> `TallyQLError` structured error pattern
- Comment stripping (// and #)
- MAX_QUERY_LENGTH / MAX_NESTING_DEPTH safety constants

**Strip out (not relevant to tally):**
- SQL mode (SELECT/FROM/WHERE/GROUP BY) — tally queries findings in memory, not tables
- Pipe stages (stats/sort/head/tail/dedup/fields) — sorting handled by CLI flag, not expression
- CIDR matching — findings don't have IP addresses
- Regex matching — overkill for finding queries
- Wildcard patterns — not needed for structured field queries
- FieldRef dotted paths with array indexing — tally fields are flat, not nested
- AggFunction / AggregationExpr — no aggregation in expression language
- Source enum — tally has one data source (findings)
- CompareOp / StringOp / Value / FieldRef from axiathon-core — define tally-specific versions

**New for tally (not in axiathon):**
- Date literal parsing: `humantime::parse_duration()` for relative (`7d`, `24h`), `chrono` for ISO 8601 (`"2026-03-01"`)
- Unquoted enum values for severity/status (e.g., `severity = critical` without quotes)
- Field-to-Finding accessor mapping (the evaluator layer)
- Multi-value CLI flag parsing (comma-separated)
- Sorting as a separate concern from filtering

### New Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `chumsky` | `"0.10"` | Parser combinator library (recursive descent, error recovery) |
| `humantime` | `"2"` | Relative duration parsing (`7d`, `24h`, `30m`) — lightweight, zero external deps |
| `proptest` | `"1"` | (dev-dependency, already present) Property-based testing for parser/evaluator |

Note: `chrono` is already a dependency for ISO 8601 date handling.

### Field Name Registry

TallyQL field names map to Finding struct fields:

| TallyQL Field | Finding Field | Type | Operators |
|---------------|---------------|------|-----------|
| `severity` | `severity` | `Severity` enum | =, !=, IN, >, < (ordered: critical=3, important=2, suggestion=1, tech_debt=0) |
| `status` | `status` | `LifecycleState` enum | =, !=, IN |
| `file` | `locations[*].file_path` | `String` | =, !=, CONTAINS, STARTSWITH, ENDSWITH (matches ANY location) |
| `rule` | `rule_id` | `String` | =, !=, CONTAINS |
| `title` | `title` | `String` | =, !=, CONTAINS, STARTSWITH, ENDSWITH |
| `description` | `description` | `String` | =, !=, CONTAINS, STARTSWITH, ENDSWITH |
| `suggested_fix` | `suggested_fix` | `Option<String>` | =, !=, CONTAINS, HAS, MISSING (HAS = is_some, MISSING = is_none) |
| `evidence` | `evidence` | `Option<String>` | =, !=, CONTAINS, HAS, MISSING (HAS = is_some, MISSING = is_none) |
| `category` | `category` | `String` | =, !=, CONTAINS |
| `agent` | `discovered_by[*].agent_id` | `Vec<AgentRecord>` | =, != (matches ANY agent_id), HAS = non-empty, MISSING = empty |
| `tag` | `tags[*]` | `Vec<String>` | =, CONTAINS (matches ANY tag), HAS = non-empty, MISSING = empty |
| `created_at` | `created_at` | `DateTime<Utc>` | =, !=, >, <, >=, <= |
| `updated_at` | `updated_at` | `DateTime<Utc>` | =, !=, >, <, >=, <= |

### Severity Ordering (for comparisons and sorting)

`critical > important > suggestion > tech_debt`

This enables `severity > important` (matches only critical) and `--sort severity` (critical first).

### Source Files from Axiathon to Reference

| Axiathon File | Tally Adaptation |
|---------------|-----------------|
| `crates/axiathon-query/src/ast.rs` | `src/query/ast.rs` — strip SQL/pipe/CIDR/regex, add date values |
| `crates/axiathon-query/src/parser.rs` | `src/query/parser.rs` — keep filter_parser, strip sql_parser/pipe_parser |
| `crates/axiathon-query/src/error.rs` | `src/query/error.rs` — rename to TallyQLError |
| `crates/axiathon-query/src/type_system.rs` | Not needed — tally evaluates dynamically, no DataFusion plan |
| `crates/axiathon-query/src/aliases.rs` | `src/query/fields.rs` — simpler flat field registry |
| `crates/axiathon-query/src/config.rs` | Not needed — use constants for limits |

### Technical Gotchas

1. **Chumsky 0.10 API**: Uses `extra::Err<Rich<'a, char>>` for error type, `Boxed` for recursive parsers. The `text::keyword()` is case-sensitive — use `text::ident().try_map()` pattern (see `kw()` helper in "Key Chumsky 0.10 Patterns" section below).

2. **Rc<Cell> not Arc<AtomicUsize>**: The nesting depth counter uses `Rc<Cell<usize>>` because the parser is synchronous and stack-scoped. `Arc<AtomicUsize>` would add unnecessary overhead. This is safe because `parse_tallyql()` is never held across `.await` points.

3. **Date parsing precedence**: Duration literals (`7d`, `24h`) use `humantime::parse_duration()` and must be tried before bare integers in the value parser. ISO 8601 strings are quoted (`"2026-03-01"`) so they're handled by the string parser; the evaluator detects date-like strings when comparing against `DateTime` fields and parses via `chrono::NaiveDate::parse_from_str("%Y-%m-%d")` or `DateTime::parse_from_rfc3339()`.

4. **Severity ordering**: Severity comparison (`severity > important`) requires a numeric mapping. Implement `Ord` for `Severity` or use a helper: critical=3, important=2, suggestion=1, tech_debt=0.

5. **File field is multi-location**: `file CONTAINS "api"` must check ANY location in `locations[]`, not just the first. This matches the existing behavior in `query.rs` line 49.

6. **Tag field is array**: `tag = "story:5.1"` must check if ANY tag matches. `HAS tag` means tags is non-empty. `MISSING tag` means tags is empty.

7. **Optional fields**: `suggested_fix`, `evidence` are `Option<String>`. `HAS suggested_fix` means `is_some()`. String ops on a `None` field should return false (not error).

8. **Backward compatibility**: The existing `--status`, `--severity`, `--file`, `--rule`, `--related-to`, `--tag` flags must continue to work identically. New flags and `--filter` are additive.

9. **`discovered_by` is `Vec<AgentRecord>`**, not a simple String. Each `AgentRecord` has `agent_id: String`, `session_id: String`, `detected_at: DateTime<Utc>`. The `agent` field in TallyQL checks `finding.discovered_by.iter().any(|a| a.agent_id == value)`. `HAS agent` means `!discovered_by.is_empty()`.

10. **Current version**: tally is v0.5.1 with 389 tests across 17 test files. The query module adds a new `src/query/` directory — this is the first time tally has a non-trivial internal module beyond `model/`, `cli/`, `mcp/`, `storage/`, `session/`.

11. **Chumsky compile time**: Long `or()` chains cause exponential type solver growth. Use `choice()` instead, and add `.boxed()` after combining multiple alternatives — this can yield 90x compile time improvement. Place `.boxed()` at boundaries between major parser sections (comparisons, string ops, existence checks).

12. **humantime for relative durations**: Use `humantime::parse_duration("7d")` which returns `std::time::Duration`. Convert via `chrono::TimeDelta::from_std(duration).expect("duration in range")` then `Utc::now() - time_delta`. In chrono 0.4.44, `Duration` is a type alias for `TimeDelta` — use `TimeDelta` directly for new code. `humantime` 2.3.0 is zero-dep and supports `s`, `m`, `h`, `d` suffixes.

13. **Chumsky 0.10.1 is latest stable** (released Apr 2025). `1.0.0-alpha.8` exists but is alpha — do NOT use it. MSRV is 1.65, fully compatible with Rust 1.85+ / Edition 2024. A minor lifetime issue exists with `text::ascii::keyword()` + Rich errors (fixed in main, not yet released) — irrelevant since we use `text::ident().try_map()` pattern instead.

14. **`DateTime::parse_from_rfc3339()` returns `DateTime<FixedOffset>`**, not `DateTime<Utc>`. The evaluator must call `.with_timezone(&Utc)` to convert before comparing against Finding timestamps. Pattern: `DateTime::parse_from_rfc3339(s)?.with_timezone(&Utc)`.

15. **rmcp version context**: tally currently uses rmcp 0.8.5 (resolves from `"0.8"`). Latest on crates.io is rmcp 1.2.0. This story does NOT upgrade rmcp — it only adds `Option<String>` fields to the existing `QueryFindingsInput` struct, which is safe on 0.8.x. An rmcp upgrade is a separate story.

16. **chrono 0.4.44 TimeDelta**: `chrono::Duration` is now a type alias for `chrono::TimeDelta`. Use `TimeDelta` directly in new code. Both `Sub<TimeDelta>` and `Sub<Duration>` are implemented on `DateTime<Utc>`, so either works, but prefer `TimeDelta` to avoid deprecation warnings.

### Project Structure Notes

New files:
```
src/
  query/
    mod.rs          # pub use ast, parser, eval, error, fields
    ast.rs          # FilterExpr, CompareOp, StringOp, Value, SortSpec
    parser.rs       # parse_tallyql() entry, Chumsky 0.10 parser
    eval.rs         # evaluate(expr, finding) -> bool
    error.rs        # TallyQLError with thiserror
    fields.rs       # Field name -> Finding accessor mapping
tests/
  query_parser_test.rs    # Parser unit tests
  query_eval_test.rs      # Evaluator unit tests
  cli_query_enhanced_test.rs  # CLI integration tests for new flags
  property_query.rs       # Proptest for parser/evaluator
```

Modified files:
```
Cargo.toml              # Add chumsky = "0.10", humantime = "2" (proptest already in dev-deps)
src/main.rs             # Add mod query, update match arm
src/cli/mod.rs          # Add new flags to Query variant
src/cli/query.rs        # Integrate TallyQL filter + new flags + sorting
src/mcp/server.rs       # Add filter/sort/new params to query_findings
README.md               # Query section with TallyQL reference
```

### Test Fixtures

#### Parser Test Fixtures — Expression to AST

```
Input: severity = critical
Expected AST: Comparison { field: "severity", op: Eq, value: Enum("critical") }

Input: severity = critical AND file CONTAINS "api"
Expected AST: And(
  Comparison { field: "severity", op: Eq, value: Enum("critical") },
  StringMatch { field: "file", op: Contains, value: "api" }
)

Input: severity IN (critical, important) OR status = open
Expected AST: Or(
  InList { field: "severity", values: [Enum("critical"), Enum("important")], negated: false },
  Comparison { field: "status", op: Eq, value: Enum("open") }
)

Input: NOT (status = closed) AND created_at > 7d
Expected AST: And(
  Not(Comparison { field: "status", op: Eq, value: Enum("closed") }),
  Comparison { field: "created_at", op: Gt, value: Duration(7d) }
)

Input: HAS suggested_fix AND tag CONTAINS "security"
Expected AST: And(
  Has("suggested_fix"),
  StringMatch { field: "tag", op: Contains, value: "security" }
)

Input: title CONTAINS "unwrap" OR description CONTAINS "unwrap"
Expected AST: Or(
  StringMatch { field: "title", op: Contains, value: "unwrap" },
  StringMatch { field: "description", op: Contains, value: "unwrap" }
)
```

#### Parser Negative Test Fixtures — Expected Errors

```
Input: foo = "bar"
Error: unknown field 'foo' at 0..3, expected one of: severity, status, file, rule, title, ...
Hint: did you mean 'file'?

Input: severity > critical AND (((((...nested 65 levels...
Error: expression nesting exceeds maximum depth of 64

Input: (empty string)
Error: expected query at 0..0
```

#### Evaluator Test Fixture — Sample Finding JSON

```json
{
  "uuid": "019e1a2b-3c4d-7e5f-8a9b-0c1d2e3f4a5b",
  "schema_version": "1.1.0",
  "rule_id": "unsafe-unwrap",
  "locations": [
    { "file_path": "src/api/handler.rs", "start_line": 42, "end_line": 42, "role": "primary" },
    { "file_path": "src/api/routes.rs", "start_line": 15, "end_line": 15, "role": "secondary" }
  ],
  "severity": "critical",
  "status": "open",
  "category": "safety",
  "tags": ["story:5.1", "security", "check-drift"],
  "title": "unwrap on Option in request handler",
  "description": "Using .unwrap() on user input that may be None",
  "suggested_fix": "Use .ok_or() with a proper error type",
  "evidence": "handler.rs:42: let val = input.get(\"key\").unwrap();",
  "discovered_by": [
    { "agent_id": "dclaude", "session_id": "sess-001", "detected_at": "2026-03-10T14:30:00Z" }
  ],
  "created_at": "2026-03-10T14:30:00Z",
  "updated_at": "2026-03-12T09:00:00Z",
  "notes": [],
  "edit_history": []
}
```

**Evaluator assertions against this fixture:**

| Expression | Expected Result |
|---|---|
| `severity = critical` | true |
| `severity = important` | false |
| `severity > important` | true (critical=3 > important=2) |
| `status = open` | true |
| `file CONTAINS "api"` | true (matches handler.rs AND routes.rs) |
| `file CONTAINS "tests"` | false |
| `rule = "unsafe-unwrap"` | true |
| `tag = "security"` | true |
| `tag CONTAINS "story"` | true (matches "story:5.1") |
| `HAS suggested_fix` | true |
| `MISSING suggested_fix` | false |
| `MISSING evidence` | false |
| `agent = "dclaude"` | true |
| `agent = "zclaude"` | false |
| `created_at > 7d` | depends on eval time (relative) |
| `created_at > "2026-03-09"` | true |
| `title CONTAINS "unwrap"` | true |
| `NOT status = closed` | true |
| `severity = critical AND file CONTAINS "api"` | true |
| `severity = suggestion OR status = closed` | false |

#### Property Test Strategy

```rust
// Generator for arbitrary FilterExpr (proptest)
// - Leaf nodes: random field name from KNOWN_FIELDS, random op, random value
// - Branch nodes: And/Or/Not with recursive sub-expressions
// - Depth bounded to MAX_NESTING_DEPTH
// Properties to verify:
// 1. parse(display(expr)) roundtrips for any generated expr
// 2. evaluate(expr, finding) never panics for any expr + any finding
// 3. evaluate(And(a, b)) == evaluate(a) && evaluate(b)
// 4. evaluate(Not(a)) == !evaluate(a)
// 5. evaluate(Or(a, b)) == evaluate(a) || evaluate(b)
```

### Testing Standards

- Parser tests named as documentation: `severity_equals_critical_parses()`, `and_binds_tighter_than_or()`
- Evaluator tests verify field extraction: `file_contains_matches_any_location()`
- Negative tests for every error path: `unknown_field_returns_error()`, `depth_exceeded_returns_error()`
- Property tests: `arbitrary_filter_expr_never_panics()`, `parse_then_eval_is_deterministic()`
- Backward compatibility: existing query tests unchanged, existing flags produce same results

### TallyQL Grammar (EBNF)

```ebnf
query           = filter_expr ;
filter_expr     = or_expr ;
or_expr         = and_expr , { ( "OR" | "||" ) , and_expr } ;
and_expr        = not_expr , { ( "AND" | "&&" ) , not_expr } ;
not_expr        = ( "NOT" | "!" ) , atom | atom ;
atom            = comparison | string_match | existence | in_list | "(" , filter_expr , ")" ;

comparison      = field , compare_op , value ;
string_match    = field , string_op , quoted_string ;
existence       = ( "HAS" | "MISSING" ) , field ;
in_list         = field , "IN" , "(" , value , { "," , value } , ")" ;

compare_op      = "=" | "!=" | ">" | "<" | ">=" | "<=" ;
string_op       = "CONTAINS" | "STARTSWITH" | "ENDSWITH" ;   (* case-insensitive keywords *)

field           = "severity" | "status" | "file" | "rule" | "title" | "description"
                | "suggested_fix" | "evidence" | "category" | "agent" | "tag"
                | "created_at" | "updated_at" ;

value           = quoted_string | duration | integer | enum_value ;
quoted_string   = '"' , { char - '"' | '\\"' | '\\\\' | '\\n' | '\\t' } , '"' ;
duration        = digit , { digit } , ( "s" | "m" | "h" | "d" ) ;
integer         = [ "-" ] , digit , { digit } ;
enum_value      = identifier ;   (* severity/status values: critical, open, etc. *)
identifier      = ( letter | "_" ) , { letter | digit | "_" | "-" } ;

(* Operator precedence: NOT (highest) > AND > OR (lowest) *)
(* All keywords are case-insensitive *)
(* Max query length: 8192 bytes *)
(* Max nesting depth: 64 *)
```

### Current Code to Modify (inline for self-containment)

#### Current `Command::Query` in `src/cli/mod.rs` (lines 129-161)

```rust
Query {
    /// Filter by lifecycle status.
    #[arg(long)]
    status: Option<String>,

    /// Filter by severity.
    #[arg(long)]
    severity: Option<String>,

    /// Filter by file path (substring match).
    #[arg(long)]
    file: Option<String>,

    /// Filter by rule ID (exact match).
    #[arg(long)]
    rule: Option<String>,

    /// Filter by related finding ID.
    #[arg(long)]
    related_to: Option<String>,

    /// Filter by tag (substring match).
    #[arg(long)]
    tag: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value = "json")]
    format: OutputFormat,

    /// Max results (default: 100).
    #[arg(long, default_value = "100")]
    limit: usize,
},
```

**Add these fields** (before `format`):
- `filter: Option<String>` — TallyQL expression
- `since: Option<String>` — duration or ISO 8601
- `before: Option<String>` — duration or ISO 8601
- `agent: Option<String>` — agent ID filter
- `category: Option<String>` — category filter
- `not_status: Option<String>` — status exclusion
- `text: Option<String>` — full-text search
- `sort: Vec<String>` — repeatable sort fields
- `sort_dir: Option<String>` — asc or desc

Note: `--severity` and `--status` stay as `Option<String>` but the handler parses comma-separated values internally.

#### Current `QueryFindingsInput` in `src/mcp/server.rs` (lines 119-140)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryFindingsInput {
    #[schemars(description = "Filter by lifecycle status...")]
    pub status: Option<String>,
    #[schemars(description = "Filter by severity level...")]
    pub severity: Option<String>,
    #[schemars(description = "Filter by file path substring...")]
    pub file: Option<String>,
    #[schemars(description = "Filter by rule ID...")]
    pub rule: Option<String>,
    #[schemars(description = "Maximum number of results (default: 100)")]
    pub limit: Option<usize>,
    #[schemars(description = "Filter by tag substring...")]
    pub tag: Option<String>,
}
```

**Add these fields:**
- `filter: Option<String>` — TallyQL expression
- `sort: Option<String>` — sort field
- `since: Option<String>` — duration or ISO 8601
- `before: Option<String>` — duration or ISO 8601
- `agent: Option<String>` — agent ID
- `category: Option<String>` — category
- `text: Option<String>` — full-text search

#### Current `main.rs` match arm (lines 107-126)

```rust
Command::Query {
    status, severity, file, rule, related_to, tag, format, limit,
} => cli::handle_query(
    &store()?,
    status.as_deref(),
    severity.as_deref(),
    file.as_deref(),
    rule.as_deref(),
    related_to.as_deref(),
    tag.as_deref(),
    format,
    limit,
),
```

Add new fields to destructuring and pass to `handle_query()`.

### Key Chumsky 0.10 Patterns (inline — adapted from axiathon)

#### Case-insensitive keyword helper (`kw()`)

```rust
use chumsky::prelude::*;

fn kw<'a>(keyword: &'a str) -> Boxed<'a, 'a, &'a str, (), extra::Err<Rich<'a, char>>> {
    text::ident()
        .try_map(move |ident: &str, span| {
            if ident.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(Rich::custom(span, format!("expected keyword '{keyword}'")))
            }
        })
        .boxed()
}
```

#### Quoted string parser with escape handling

```rust
fn quoted_string<'a>() -> impl Parser<'a, &'a str, String, extra::Err<Rich<'a, char>>> {
    let known_escape = just('\\').ignore_then(choice((
        just('"').to('"'),
        just('\\').to('\\'),
        just('n').to('\n'),
        just('t').to('\t'),
    )));
    let regular_char = none_of(['"', '\\']).map(|c: char| String::from(c));

    known_escape
        .map(String::from)
        .or(regular_char)
        .repeated()
        .collect::<Vec<String>>()
        .map(|parts| parts.join(""))
        .delimited_by(just('"'), just('"'))
}
```

#### Boolean operator precedence via foldl

```rust
// Precedence: NOT (highest) > AND > OR (lowest)
// Each level: parse tighter-binding, then foldl with current operator

let atom = choice((comparison, existence, string_match, in_list, parenthesized)).boxed();

let not = choice((kw("NOT").to(()), just('!').to(())))
    .padded()
    .ignore_then(atom.clone())
    .map(|e| FilterExpr::Not(Box::new(e)))
    .or(atom);

let and = not.clone().foldl(
    choice((kw("AND").to(()), just("&&").to(())))
        .padded()
        .ignore_then(not)
        .repeated(),
    |left, right| FilterExpr::And(Box::new(left), Box::new(right)),
);

and.clone().foldl(
    choice((kw("OR").to(()), just("||").to(())))
        .padded()
        .ignore_then(and)
        .repeated(),
    |left, right| FilterExpr::Or(Box::new(left), Box::new(right)),
)
```

#### Nesting depth guard

```rust
use std::cell::Cell;
use std::rc::Rc;

const MAX_NESTING_DEPTH: usize = 64;

// Inside recursive():
let depth = Rc::new(Cell::new(0usize));
let depth_open = Rc::clone(&depth);
let depth_close = Rc::clone(&depth);

let depth_guarded_paren = just('(')
    .padded()
    .try_map(move |_, span| {
        let d = depth_open.get() + 1;
        if d > MAX_NESTING_DEPTH {
            Err(Rich::custom(span, format!(
                "expression nesting exceeds maximum depth of {MAX_NESTING_DEPTH}"
            )))
        } else {
            depth_open.set(d);
            Ok(())
        }
    })
    .ignore_then(expr.clone())
    .then_ignore(just(')').padded().map(move |_| {
        depth_close.set(depth_close.get().saturating_sub(1));
    }));
```

#### Structured error type (TallyQLError)

```rust
use std::ops::Range;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TallyQLError {
    #[error("parse error at {span:?}: {expected}, {}", found.as_deref().unwrap_or("end of input"))]
    Parse {
        span: Range<usize>,
        expected: String,
        found: Option<String>,
        hint: Option<String>,
    },
}

pub type Result<T> = std::result::Result<T, TallyQLError>;

impl TallyQLError {
    pub fn unexpected_token(span: Range<usize>, expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::Parse { span, expected: expected.into(), found: Some(found.into()), hint: None }
    }
    pub fn unexpected_eof(span: Range<usize>, expected: impl Into<String>) -> Self {
        Self::Parse { span, expected: expected.into(), found: None, hint: None }
    }
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        if let Self::Parse { hint: ref mut h, .. } = self { *h = Some(hint.into()); }
        self
    }
}
```

#### TallyQL AST types (target design)

```rust
/// Filter expression — boolean tree of field predicates.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterExpr {
    Comparison { field: String, op: CompareOp, value: Value },
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
    Not(Box<FilterExpr>),
    Has(String),
    Missing(String),
    StringMatch { field: String, op: StringOp, value: String },
    InList { field: String, values: Vec<Value> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp { Eq, Ne, Gt, Lt, GtEq, LtEq }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringOp { Contains, StartsWith, EndsWith }

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Duration(std::time::Duration),
    Enum(String),  // severity/status values: "critical", "open", etc.
}
```

### References

- axiathon source at `/Users/jmagady/Dev/axiathon/crates/axiathon-query/src/` — full parser is there for deeper reference if needed, but the inline patterns above are sufficient for implementation
- tally source at `/Users/jmagady/Dev/tally/src/` — current codebase to modify

---

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
