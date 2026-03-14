# Story: tally — Git-Backed Findings Tracker for AI Coding Agents

Status: proposal
Repository: `1898andCo/tally` (new, to be created)
Language: Rust
License: Apache-2.0
Date: 2026-03-13

---

## Problem Statement

AI coding agent review tools (dclaude, zclaude) use **session-scoped sequential IDs** (C1, I2, S3) that reset every review pass. This creates three critical problems:

1. **No cross-session identity**: Finding C1 in session X has no relationship to C1 in session Y, even if they describe the same issue
2. **No history**: When a finding is fixed and regresses, there's no record it was seen before
3. **No deduplication**: Multiple agents (Claude Code, Cursor, Aider) discover the same issue independently with no way to correlate

## Solution

A Rust CLI tool and MCP server that provides **persistent, content-addressable finding identity** backed by git. Findings survive across sessions, agents, PRs, and branches with full lifecycle tracking.

## Story

As an **AI coding agent** (Claude Code, Cursor, Aider),
I want **a persistent findings database with stable IDs**,
So that **findings can be tracked, deduplicated, and queried across sessions and agents**.

As a **developer using AI coding agents**,
I want **to see the full history of a finding** (when discovered, by which agent, status changes),
So that **I can understand trends, track regressions, and avoid re-investigating known issues**.

---

## Prerequisites

Before Phase 1 implementation:

1. **GitHub repo creation**: Create `1898andCo/tally` (public)
2. **Branch protection**: Enable on `develop` and `main` with required status checks: `Rustfmt`, `Clippy`, `Build`, `Test`, `Cargo Deny`
3. **Branch strategy**: Git Flow — `develop` is default branch, PRs target `develop`, releases merge to `main`
4. **Runners**: `[self-hosted, Ubuntu, Common]` (same as axiathon)
5. **Secrets**: None required (no external services in MVP)
6. **crates.io**: Account setup deferred — publish decision after MVP validation

---

## Acceptance Criteria

### AC-1: Finding Creation with Stable Identity

**Given** an AI agent discovers an issue in code
**When** it calls `tally record --file src/main.rs --line 42 --severity important --title "unwrap on Option" --rule unsafe-unwrap`
**Then** a finding is created with:
- A stable UUID (v7, time-ordered)
- A content fingerprint (SHA-256 of file + line range + rule)
- A rule ID for grouping
- Status `Open`
**And** the finding is persisted to the git-backed store
**And** if a finding with the same fingerprint already exists, the existing UUID is returned (deduplication)

### AC-2: Cross-Session Finding Persistence

**Given** a finding was created in session A by Claude Code
**When** session B starts (same or different agent)
**And** the agent queries `tally query --file src/main.rs`
**Then** the finding from session A is returned with its original UUID and full history

### AC-3: Finding Lifecycle State Machine

**Given** a finding exists with status `Open`
**When** the agent calls `tally update <uuid> --status in-progress`
**Then** the status transitions to `InProgress`
**And** the transition is recorded with timestamp, agent ID, and reason
**And** invalid transitions are rejected with exit code 1 and an error message listing valid transitions

### AC-4: MCP Server Mode

**Given** the tool is configured as an MCP server in `.mcp.json`
**When** Claude Code starts a session
**Then** it discovers 5 MCP tools: `record_finding`, `query_findings`, `update_finding_status`, `get_finding_context`, `suppress_finding`
**And** 3 MCP resources: `findings://summary`, `findings://file/{path}`, `findings://detail/{id}`
**And** tool responses complete in <100ms for repos with <10k findings

### AC-5: CLI Mode

**Given** the tool is invoked from the command line
**When** I run `tally query --severity important --status open --format json`
**Then** matching findings are returned as structured JSON to stdout (exit code 0)
**And** `--format table` produces human-readable table output
**And** `--format summary` produces a count-by-severity summary
**And** no results produces empty JSON array `[]` (exit code 0, not an error)

### AC-6: Git-Backed Storage

**Given** the tool is initialized with `tally init`
**Then** a `findings-data` orphan branch is created in the current git repo
**And** each finding is stored as an individual JSON file: `findings/<uuid>.json`
**And** a derived index file `index.json` maps UUIDs to metadata for fast queries (regenerable)
**And** the working tree is not affected (storage is branch-only, accessed via `git2`)
**And** concurrent writes by multiple agents produce zero merge conflicts (each file is unique)

### AC-7: Content Fingerprint Deduplication

**Given** two agents independently discover the same issue
**When** both call `record_finding` with the same file, line range, and rule
**Then** only one finding UUID exists
**And** both agents are recorded in the finding's `discovered_by` list
**And** the finding shows "first detected by agent A, also detected by agent B"

### AC-8: Finding Re-Matching After Refactoring

**Given** a finding exists at `src/main.rs:42` with fingerprint `sha256:abc...`
**When** code is refactored and the same pattern now appears at `src/main.rs:55`
**And** a new scan detects the pattern
**Then** the existing finding is returned (matched by fingerprint)
**And** the finding's location history records the move from line 42 to line 55

### AC-9: Session-Scoped Short IDs

**Given** an agent is working in a review session
**When** findings are presented to the user
**Then** each finding has both a stable UUID and a session-scoped short ID (C1, I2, S3)
**And** the short ID is mapped to the UUID for the duration of the session
**And** CLI and MCP tools accept both UUID and short ID as finding identifiers

### AC-10: Suppression System

**Given** a finding is a known issue that should not be re-reported
**When** the agent calls `tally suppress <uuid> --reason "accepted risk" --expires 2026-06-01`
**Then** the finding status changes to `Suppressed`
**And** future `record` calls with the same fingerprint return the existing UUID with status `Suppressed` (not creating a new finding)
**And** after the expiry date, `query` returns the finding with status `Open` (auto-reopened)

### AC-11: Batch Recording

**Given** an AI agent discovers 20 findings in a single review pass
**When** it calls `tally record-batch findings.jsonl` (CLI) or the `record_batch` MCP tool
**Then** all valid findings are recorded with UUIDs assigned
**And** invalid findings are reported with per-item errors (partial success)
**And** the response includes a summary: `{"total": 20, "succeeded": 17, "failed": 3}`
**And** duplicate findings (same fingerprint) are deduplicated, not errored

### AC-12: Multi-User Concurrency

**Given** developer A and developer B are both running AI agents simultaneously
**When** both record findings to the same repo's `findings-data` branch
**Then** no findings are lost — both sets of findings appear after sync
**And** git's native 3-way merge auto-resolves (each finding is a separate file — zero conflicts)
**And** if a push fails due to non-fast-forward, tally retries with pull + merge (up to 3 attempts)
**And** `index.json` is rebuilt automatically after merge

### AC-13: Multi-File Findings

**Given** an AI agent discovers a cross-file issue (spec says X, code says Y)
**When** it records a finding with two locations
**Then** the finding has a `locations` array with both files and their line ranges
**And** the primary location is marked with `role: "primary"`, secondary with `role: "secondary"`
**And** SARIF export includes both locations in the `locations` array

### AC-14: Finding Relationships

**Given** a finding C2 was discovered while fixing finding C1
**When** the agent records C2 with `--related-to C1 --relationship discovered-while-fixing`
**Then** C2's `relationships` array contains a link to C1's UUID
**And** querying C1 shows "related findings: C2 (discovered while fixing)"
**And** supported relationship types: `duplicate_of`, `blocks`, `related_to`, `causes`, `discovered_while_fixing`, `supersedes`

---

## Hosting & Infrastructure Decisions

| Decision | Value | Rationale |
|----------|-------|-----------|
| GitHub org | `1898andCo` | Organization standard |
| Repo name | `tally` | Descriptive, generic (not axiathon-specific) |
| Visibility | Public | General-purpose tool, open-source |
| License | Apache-2.0 | Matches axiathon |
| Default branch | `develop` | Git Flow |
| Branch protection | Required: Rustfmt, Clippy, Build, Test, Cargo Deny | 5 status checks |
| Runners | `[self-hosted, Ubuntu, Common]` | Org standard |
| crates.io publish | Deferred to post-MVP | Validate tool first |

---

## Architecture

### Storage Model

```
findings-data branch (orphan, no working tree pollution):
  findings/
    019e1a2b-3c4d-7e5f-8a9b-0c1d2e3f4a5b.json   # One file per finding
    019e1a2b-4d5e-7f6a-9b0c-1d2e3f4a5b6c.json
    ...
  index.json              # Derived index: maps UUIDs to severity/status/file/rule (regenerable)
  schema.json             # Schema version for migration
```

**Why one-file-per-finding (not JSONL):**

Deep research (Mar 2026) revealed that single-JSONL has critical multi-user problems:
- **GitHub ignores custom merge drivers** — JSONL merge driver won't run on PR merges
- **libgit2 does NOT retry ref locks** — concurrent `git2` commits to same branch fail hard
- **EOF contention** — two appends to same file position require custom conflict resolution

One-file-per-finding solves all three:
- Git's default 3-way merge auto-resolves new files (zero conflicts)
- No lock contention (each finding writes to a unique filename)
- Works on GitHub, GitLab, local — no custom merge driver needed
- Per-finding git blame/diff history
- Scales to hundreds of concurrent agents

**Trade-off**: directory scan for queries is slightly slower than sequential JSONL read. Mitigated by `index.json` (regenerable from finding files, cached in memory).

### Remote Sync & Merge Strategy

The `findings-data` branch is pushed to remote for multi-user collaboration. Concurrent writes are handled by git's native merge:

1. **One-file-per-finding**: Each finding is a separate `findings/<uuid>.json` file. New findings are new files — git auto-merges without conflict.
2. **Index regeneration**: `index.json` uses `merge=ours` in `.gitattributes` — always regenerated locally from finding files after merge.
3. **Optimistic retry**: On push failure (non-fast-forward), tally pulls, auto-merges, retries (up to 3x with exponential backoff starting at 100ms).
4. **No custom merge driver needed**: Unlike JSONL, one-file-per-finding works with GitHub's server-side merge, CI, and all git hosting providers.

```
# .gitattributes (on findings-data branch)
index.json merge=ours
```

**Conflict scenarios**:
- Two agents create different findings simultaneously → **auto-resolves** (different files)
- Two agents update the same finding → **conflict** (same file modified). Resolved by: last-write-wins on `updated_at` timestamp, or manual merge.
- In practice, same-finding concurrent updates are rare (findings are usually created once, status-updated sequentially by one agent at a time).

### Finding Data Model

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A finding represents a single issue discovered in code by an AI agent.
///
/// Identity is a hybrid of UUID (stable reference), content fingerprint
/// (deduplication), and rule ID (grouping). Modeled after SonarQube
/// (content hash + rule), CodeClimate (UUID + remapping), and git-bug
/// (content-addressed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    // --- Identity ---
    /// Stable UUID v7 (time-ordered). Assigned on first creation, never changes.
    pub uuid: Uuid,
    /// SHA-256 of (file_path + line_range + rule_id). For deduplication and
    /// re-matching after refactoring.
    pub content_fingerprint: String,
    /// Grouping key: "unsafe-unwrap", "sql-injection", "missing-test", etc.
    /// Enables "show all instances of this rule" queries.
    pub rule_id: String,

    // --- Locations (mutable — code moves; multi-file supported) ---
    /// Primary location + optional secondary locations (cross-file findings).
    /// Maps to SARIF multi-location representation.
    pub locations: Vec<Location>,

    // --- Classification ---
    pub severity: Severity,
    pub category: String,
    pub tags: Vec<String>,

    // --- Description ---
    pub title: String,
    pub description: String,
    pub suggested_fix: Option<String>,
    pub evidence: Option<String>,

    // --- Lifecycle ---
    pub status: LifecycleState,
    pub state_history: Vec<StateTransition>,

    // --- Provenance ---
    pub discovered_by: Vec<AgentRecord>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // --- Context ---
    pub repo_id: String,
    pub branch: Option<String>,
    pub pr_number: Option<u64>,
    pub commit_sha: Option<String>,

    // --- Relationships ---
    pub relationships: Vec<FindingRelationship>,

    // --- Suppression ---
    pub suppression: Option<Suppression>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    /// "primary", "secondary", "context" — distinguishes the main issue location
    /// from supporting evidence locations
    pub role: LocationRole,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationRole {
    Primary,
    Secondary,
    Context,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingRelationship {
    pub related_finding_id: Uuid,
    pub relationship_type: RelationshipType,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    /// This finding is a duplicate of the related finding
    DuplicateOf,
    /// This finding blocks resolution of the related finding
    Blocks,
    /// This finding is related but neither blocks nor duplicates
    RelatedTo,
    /// Fixing this finding may resolve the related finding
    Causes,
    /// This finding was discovered while fixing the related finding
    DiscoveredWhileFixing,
    /// This finding supersedes (replaces) the related finding
    Supersedes,
}

/// 4-tier severity matching dclaude/zclaude conventions.
/// Maps to SARIF on export: Critical->error, Important->warning,
/// Suggestion->note, TechDebt->none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Important,
    Suggestion,
    TechDebt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Open,
    Acknowledged,
    InProgress,
    Resolved,
    FalsePositive,
    WontFix,
    Deferred,
    Suppressed,
    Reopened,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
    pub reason: Option<String>,
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub agent_id: String,
    pub session_id: String,
    pub detected_at: DateTime<Utc>,
    pub session_short_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suppression {
    pub suppressed_at: DateTime<Utc>,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub suppression_type: SuppressionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionType {
    Global,
    FileLevel,
    InlineComment { pattern: String },
}
```

### State Transition Matrix

All valid `(from, to)` transitions. Any transition not in this table is rejected with an error.

| From | Valid Targets |
|------|-------------|
| `Open` | `Acknowledged`, `InProgress`, `FalsePositive`, `Deferred`, `Suppressed` |
| `Acknowledged` | `InProgress`, `FalsePositive`, `WontFix`, `Deferred` |
| `InProgress` | `Resolved`, `WontFix`, `Deferred` |
| `Resolved` | `Reopened`, `Closed` |
| `FalsePositive` | `Reopened`, `Closed` |
| `WontFix` | `Reopened`, `Closed` |
| `Deferred` | `Open`, `Closed` |
| `Suppressed` | `Open`, `Closed` |
| `Reopened` | `Acknowledged`, `InProgress` |
| `Closed` | *(terminal — no transitions)* |

### Error Types

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FindingsError {
    #[error("finding not found: {uuid}")]
    NotFound { uuid: String },

    #[error("invalid state transition: {from:?} -> {to:?} (valid: {valid:?})")]
    InvalidTransition {
        from: LifecycleState,
        to: LifecycleState,
        valid: Vec<LifecycleState>,
    },

    #[error("git storage error: {0}")]
    Git(#[from] git2::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("findings branch '{branch}' not found — run `tally init`")]
    BranchNotFound { branch: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid severity: {0} (valid: critical, important, suggestion, tech_debt)")]
    InvalidSeverity(String),

    #[error("invalid fingerprint: {0}")]
    InvalidFingerprint(String),
}

pub type Result<T> = std::result::Result<T, FindingsError>;
```

### Interface Modes

```
tally
  ├── mcp-server       # MCP stdio protocol (Claude Code, Cursor, Windsurf)
  ├── init             # Initialize findings-data branch + merge driver
  ├── record           # Create/deduplicate single finding
  ├── record-batch     # Batch record from JSONL file or stdin
  ├── query            # Search findings with filters
  ├── update           # Change lifecycle status
  ├── suppress         # Suppress finding with reason + optional expiry
  ├── stats            # Summary statistics
  ├── sync             # Pull + merge + push findings-data branch
  ├── import-dclaude   # Import from dclaude state files
  ├── import-zclaude   # Import from zclaude state files
  └── export           # Export (SARIF 2.1.0, CSV, JSON)
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (including empty query results) |
| 1 | Application error (invalid args, invalid transition, etc.) |
| 2 | Git storage error (branch not found, commit failed) |

### MCP Tools (6)

| Tool | Description | Key Parameters |
|------|-------------|----------------|
| `record_finding` | Create or deduplicate a finding | locations, severity, title, rule_id |
| `record_batch` | Batch record multiple findings (partial success) | findings[], agent |
| `query_findings` | Search by status, severity, file, rule, tags | filters, limit, offset |
| `update_finding_status` | Transition lifecycle state (validates transitions) | finding_id, new_status, reason |
| `get_finding_context` | Retrieve finding + surrounding code context | finding_id, context_lines |
| `suppress_finding` | Suppress with reason and optional expiry | finding_id, reason, expires_at |

### MCP Resources (3)

| Resource URI | Description |
|-------------|-------------|
| `findings://summary` | Counts by severity/status, 10 most recent findings |
| `findings://file/{path}` | All findings in a specific file |
| `findings://detail/{uuid}` | Full finding with history and code context |

---

## Project Structure

```
tally/
├── .github/
│   └── workflows/
│       ├── ci.yml                    # CI: fmt, clippy, build, test, deny
│       ├── ci_status.yml             # Doc-only PR companion
│       └── ci_typos.yml              # Typo checking
├── .claude/
│   └── rules/
│       ├── _index.md
│       ├── rust.md
│       └── git-commits.md
├── src/
│   ├── main.rs                       # CLI entry point + MCP server mode
│   ├── lib.rs                        # Public API re-exports
│   ├── model/
│   │   ├── mod.rs
│   │   ├── finding.rs                # Finding, Severity, AgentRecord, Suppression
│   │   ├── identity.rs               # FindingIdentityResolver, fingerprint computation
│   │   └── state_machine.rs          # LifecycleState, StateTransition, validation
│   ├── storage/
│   │   ├── mod.rs                    # FindingsStore trait
│   │   ├── git_store.rs              # Git-backed one-file-per-finding on orphan branch
│   │   └── index.rs                  # In-memory index management
│   ├── cli/
│   │   ├── mod.rs                    # Clap CLI definition
│   │   ├── init.rs                   # init subcommand
│   │   ├── record.rs                 # record subcommand
│   │   ├── record_batch.rs           # record-batch subcommand
│   │   ├── query.rs                  # query subcommand
│   │   ├── update.rs                 # update subcommand
│   │   ├── suppress.rs               # suppress subcommand
│   │   ├── stats.rs                  # stats subcommand
│   │   ├── sync.rs                   # sync subcommand (pull + merge + push)
│   │   ├── import.rs                 # import-dclaude / import-zclaude
│   │   └── export.rs                 # export subcommand (SARIF, CSV, JSON)
│   ├── mcp/
│   │   ├── mod.rs                    # MCP server setup (rmcp)
│   │   ├── tools.rs                  # Tool handlers
│   │   └── resources.rs              # Resource handlers
│   ├── session.rs                    # SessionIdMapper (UUID <-> short IDs)
│   └── error.rs                      # FindingsError
├── tests/
│   ├── model_test.rs                 # Data model + state machine
│   ├── storage_test.rs               # Git storage round-trip
│   ├── identity_test.rs              # Fingerprint + dedup
│   ├── cli_test.rs                   # CLI integration
│   └── property_identity.rs          # Proptest: fingerprint determinism
├── Cargo.toml
├── Cargo.lock                        # Committed (binary crate)
├── rust-toolchain.toml
├── deny.toml
├── .typos.toml
├── .lefthook.yml
├── .gitignore
├── .mcp.json                         # Example MCP config for Claude Code
├── CLAUDE.md
├── CONTRIBUTING.md
├── LICENSE
└── README.md
```

---

## Tasks / Subtasks

### Phase 1: Core Data Model & Storage (MVP)

- [ ] Task 1: Project scaffolding
  - [ ] 1.1: Create Cargo project (not workspace — single crate)
  - [ ] 1.2: Write `Cargo.toml` (see Deliverable: Cargo.toml)
  - [ ] 1.3: Write `rust-toolchain.toml`, `.typos.toml`, `deny.toml`, `.lefthook.yml`, `.gitignore`
  - [ ] 1.4: Write `.github/workflows/ci.yml`, `ci_status.yml`, `ci_typos.yml`
  - [ ] 1.5: Write `CLAUDE.md`, `.claude/rules/_index.md`, `.claude/rules/rust.md`, `.claude/rules/git-commits.md`
  - [ ] 1.6: Write `justfile` with all recipes
  - [ ] 1.7: Write `LICENSE` (Apache-2.0) and `CONTRIBUTING.md`
  - [ ] 1.8: Verify: `just check` passes on empty crate

- [ ] Task 2: Finding data model (AC: 1, 3)
  - [ ] 2.1: Implement `Finding`, `Severity`, `LifecycleState`, `StateTransition`, `AgentRecord`, `Suppression`, `SuppressionType` in `src/model/finding.rs`
  - [ ] 2.2: Implement `LifecycleState::allowed_transitions()` and `can_transition_to()` in `src/model/state_machine.rs`
  - [ ] 2.3: Implement content fingerprint: `SHA-256(file_path || ":" || line_start || "-" || line_end || ":" || rule_id)` in `src/model/identity.rs`
  - [ ] 2.4: Implement `FindingIdentityResolver` with three indexes: by_fingerprint, by_location (within 5 lines), by_rule
  - [ ] 2.5: Implement `FindingsError` in `src/error.rs`
  - [ ] 2.6: Tests (positive): all 24 valid state transitions from the matrix
  - [ ] 2.7: Tests (negative): all invalid transitions (e.g., Open -> Closed, Closed -> anything)
  - [ ] 2.8: Tests: fingerprint determinism — same input produces same fingerprint
  - [ ] 2.9: Tests: identity resolution — existing fingerprint -> existing UUID, nearby location -> related, new -> new UUID
  - [ ] 2.10: Proptest: arbitrary (file_path, line, rule) triples produce deterministic fingerprints

- [ ] Task 3: Git-backed storage (AC: 6, 12)
  - [ ] 3.1: Implement `FindingsStore::init()` — create orphan `findings-data` branch with empty `findings/` directory and `schema.json` via `git2` (empty parents `&[]`, no checkout)
  - [ ] 3.2: Implement `FindingsStore::save_finding()` — write `findings/<uuid>.json` to the findings branch tree, commit (all via `git2` blob/tree/commit API, never checkout)
  - [ ] 3.3: Implement `FindingsStore::load_finding()` — read single `findings/<uuid>.json` from branch
  - [ ] 3.4: Implement `FindingsStore::load_all()` — list `findings/` directory on branch, deserialize all files, build in-memory indexes
  - [ ] 3.5: Implement `FindingsStore::rebuild_index()` — scan all finding files, generate `index.json` with uuid/severity/status/file/rule/fingerprint for each
  - [ ] 3.6: Implement `FindingsStore::push()` — push findings-data branch to remote with optimistic retry (pull + merge + retry, up to 3 attempts, 100ms/200ms/400ms backoff)
  - [ ] 3.7: Implement libgit2 ref lock retry — wrap `repo.commit()` with retry loop (100ms backoff, 3 attempts) since libgit2 does NOT retry locks automatically
  - [ ] 3.8: Tests: init creates orphan branch (verify via `git2` branch lookup)
  - [ ] 3.9: Tests: save + load round-trip (write finding, read back, verify all fields)
  - [ ] 3.10: Tests: two sequential saves create two files in `findings/` directory
  - [ ] 3.11: Tests: index regeneration matches loaded state
  - [ ] 3.12: Tests: init on already-initialized repo is idempotent (no error)
  - [ ] 3.13: Tests: operations don't modify working tree or HEAD
  - [ ] 3.14: Tests: concurrent save from two threads — both succeed (different UUIDs, different files)

- [ ] Task 4: CLI interface (AC: 1, 2, 3, 5)
  - [ ] 4.1: Implement clap CLI definition in `src/cli/mod.rs` (all subcommands, flags, args)
  - [ ] 4.2: Implement `tally init` — calls `FindingsStore::init()`
  - [ ] 4.3: Implement `tally record` — creates finding with identity resolution, dedup
  - [ ] 4.4: Implement `tally query` — filters by status, severity, file (glob), rule, tags; formats: json, table, summary
  - [ ] 4.5: Implement `tally update` — state transition with validation
  - [ ] 4.6: Implement `tally suppress` — suppression with reason + optional expiry
  - [ ] 4.7: Implement `tally stats` — summary counts by severity, status, rule, top files
  - [ ] 4.8: Implement `tally export --format sarif` — SARIF 2.1.0 output
  - [ ] 4.9: Implement `tally export --format csv` — CSV with headers
  - [ ] 4.10: Tests: CLI integration tests — invoke binary via `std::process::Command`, verify stdout JSON and exit codes
  - [ ] 4.11: Tests: `--format table` produces aligned columns
  - [ ] 4.12: Tests: empty query returns `[]` with exit code 0

### Phase 2: MCP Server (AC: 4)

- [ ] Task 5: MCP server implementation
  - [ ] 5.1: Add `rmcp` v0.8 dependency with `features = ["server", "macros"]`
  - [ ] 5.2: Implement `FindingsMcpServer` struct with `FindingsStore` and `SessionIdMapper`
  - [ ] 5.3: Implement `ServerHandler` trait for `FindingsMcpServer`
  - [ ] 5.4: Implement `record_finding` tool via `#[tool]` macro
  - [ ] 5.5: Implement `query_findings` tool
  - [ ] 5.6: Implement `update_finding_status` tool — validates transitions, records history
  - [ ] 5.7: Implement `get_finding_context` tool — returns finding + code lines from git
  - [ ] 5.8: Implement `suppress_finding` tool
  - [ ] 5.9: Implement MCP resources: `findings://summary`, `findings://file/{path}`, `findings://detail/{uuid}`
  - [ ] 5.10: Implement `mcp-server` subcommand to run as MCP stdio server (newline-delimited JSON-RPC, output to stdout, logs to stderr)
  - [ ] 5.11: Write `.mcp.json` example config file
  - [ ] 5.12: Tests: tool request -> response serialization round-trip
  - [ ] 5.13: Performance: all tool responses <100ms for 10k findings

### Phase 3: Session IDs, Deduplication, Polish (AC: 7, 8, 9)

- [ ] Task 6: Session-scoped short IDs (AC: 9)
  - [ ] 6.1: Implement `SessionIdMapper` — assigns short IDs (C1, I2, S3) based on severity, maps to UUID
  - [ ] 6.2: Accept short IDs in CLI `update` and `suppress` subcommands (resolve to UUID transparently)
  - [ ] 6.3: Accept short IDs in MCP tools
  - [ ] 6.4: Include both UUID and short ID in all output formats
  - [ ] 6.5: Tests: short ID assignment is severity-prefixed and sequential
  - [ ] 6.6: Tests: short IDs resolve to correct UUID

- [ ] Task 7: Cross-agent deduplication (AC: 7, 8)
  - [ ] 7.1: On `record`: check fingerprint index first, return existing UUID if match
  - [ ] 7.2: On `record`: if no fingerprint match, check location index (within 5 lines + same rule), offer as "related"
  - [ ] 7.3: Track `discovered_by` list — append new agent records without duplicating
  - [ ] 7.4: On `record` with existing fingerprint but different location: update location, add to location history
  - [ ] 7.5: Tests: two agents record same (file, line, rule) -> one UUID
  - [ ] 7.6: Tests: same rule at nearby line (within 5) -> flagged as related
  - [ ] 7.7: Tests: same rule at distant line -> new finding

- [ ] Task 8: Batch recording (AC: 11)
  - [ ] 8.1: Implement `tally record-batch` CLI subcommand — accepts JSONL file or `--stdin`
  - [ ] 8.2: Implement `record_batch` MCP tool — accepts `findings` array
  - [ ] 8.3: Implement partial success semantics — per-item success/error, summary counts
  - [ ] 8.4: Implement idempotency key detection (SHA-256 of agent + rule + file + line + title) — retried batch returns existing UUIDs
  - [ ] 8.5: Tests: batch of 10 valid findings -> 10 UUIDs
  - [ ] 8.6: Tests: batch of 10 with 3 invalid -> 7 succeed, 3 errors, exit code 0
  - [ ] 8.7: Tests: re-submitting same batch -> same UUIDs (idempotent)

- [ ] Task 9: Multi-file findings (AC: 13)
  - [ ] 9.1: Update `record` CLI to accept `--location` flag (repeatable): `--location src/spec.md:42:primary --location src/code.rs:100:secondary`
  - [ ] 9.2: Update `record_finding` MCP tool to accept `locations` array
  - [ ] 9.3: Update fingerprint computation to use primary location only
  - [ ] 9.4: Update SARIF export to emit multi-location `locations` array
  - [ ] 9.5: Tests: record finding with 2 locations, query back, verify both present
  - [ ] 9.6: Tests: SARIF export includes both locations

- [ ] Task 10: Finding relationships (AC: 14)
  - [ ] 10.1: Add `--related-to <id> --relationship <type>` flags to `record` and `update` CLI
  - [ ] 10.2: Add `relationships` field to `record_finding` MCP tool
  - [ ] 10.3: Implement relationship storage in finding JSON file (relationships array)
  - [ ] 10.4: Implement `query --related-to <id>` — returns all findings linked to a given UUID
  - [ ] 10.5: Tests: record C2 related to C1, query C1's related findings, verify C2 appears
  - [ ] 10.6: Tests: all 6 relationship types create valid links

- [ ] Task 11: Multi-user concurrency (AC: 12)
  - [ ] 11.1: Implement `tally sync` command — pull + merge + push findings-data branch
  - [ ] 11.2: Implement `tally init` to add `.gitattributes` (`index.json merge=ours`) to findings-data branch
  - [ ] 11.3: Implement optimistic retry on push failure (pull, auto-merge, retry, up to 3 attempts with 100ms/200ms/400ms backoff)
  - [ ] 11.4: Implement auto-rebuild of `index.json` after merge (index is always regenerable)
  - [ ] 11.5: Tests: simulate concurrent writes — two threads save different findings to same repo, both succeed after sync
  - [ ] 11.6: Tests: push conflict + retry — mock non-fast-forward, verify retry succeeds after pull
  - [ ] 11.7: Tests: `git merge` of two branches with different new finding files auto-resolves (no conflict)

- [ ] Task 12: dclaude/zclaude import (migration)
  - [ ] 12.1: Implement `tally import-dclaude` — reads `.claude/pr-reviews/$OWNER/$REPO/$PR.json` and imports findings
  - [ ] 12.2: Implement `tally import-zclaude` — reads `.claude/pr-reviews/$PR.json` and imports findings
  - [ ] 12.3: Map dclaude severity (C/I/S/TD) to tally severity (Critical/Important/Suggestion/TechDebt)
  - [ ] 12.4: Map dclaude status (pending/verified/skipped/wont_fix) to tally lifecycle states
  - [ ] 12.5: Tests: import sample dclaude state file, verify findings with correct severity and status

- [ ] Task 13: Documentation
  - [ ] 13.1: Write README.md (see Deliverable: README.md)
  - [ ] 13.2: Write CONTRIBUTING.md (see Deliverable: CONTRIBUTING.md)

---

## Dev Notes

### Technical Gotchas

1. **git2 orphan branch**: There is no `create_orphan_branch()` API. Create a commit with empty parents (`&[]`), then create a branch ref pointing to it. All reads/writes use blob/tree/commit objects — never checkout.

2. **git2 one-file-per-finding**: Each `save_finding()` creates a new blob for `findings/<uuid>.json`, reads the existing tree, inserts the new entry, and commits. This is a read-modify-commit cycle at the tree level, but since each finding has a unique filename, concurrent writes to different findings never conflict. Same-finding concurrent updates (rare) conflict at the file level and resolve via last-write-wins on `updated_at`.

3. **git2 vendoring**: `git2` bundles `libgit2` via `libgit2-sys` with the `vendored` feature (default). No system dependency needed. OpenSSL is vendored on most platforms. No special CI setup.

4. **MCP wire protocol**: Newline-delimited JSON-RPC 2.0 over stdio — NOT Content-Length headers like LSP. Each message is a complete JSON object on one line. Server MUST NOT write non-protocol data to stdout (use stderr for logs).

5. **rmcp crate**: Official Anthropic MCP SDK for Rust. Crate: `rmcp` v0.8+ from `modelcontextprotocol/rust-sdk`. Uses `#[tool]` macro for tool definitions, `ServerHandler` trait for server impl, `(stdin(), stdout())` for stdio transport.

6. **UUID v7**: `uuid` crate v1.6+, `features = ["v7", "serde"]`. `Uuid::now_v7()` generates time-ordered UUIDs. Lexicographically sortable by creation time.

7. **Content fingerprint**: `SHA-256(file_path + ":" + line_start + "-" + line_end + ":" + rule_id)`. Does NOT include description or title — those can change without creating a new finding. Does NOT include severity — severity can be reclassified.

8. **SARIF output**: No Rust crate for SARIF. Implement with serde structs. Minimum fields: `version: "2.1.0"`, `runs[].tool.driver.name`, `runs[].results[].message.text`, `runs[].results[].locations[].physicalLocation.artifactLocation.uri`.

9. **Concurrent access**: `git2::Repository` is NOT thread-safe. Wrap in `Mutex` for MCP server (multiple tool calls may arrive concurrently). For MVP, single-threaded tool dispatch is acceptable.

10. **libgit2 ref lock retry**: libgit2 does NOT retry on ref lock failure (unlike CLI git which retries ~1s). Tally must implement retry with exponential backoff: 100ms, 200ms, 400ms (3 attempts). Ref lock file is `.git/refs/heads/findings-data.lock`.

11. **One-file-per-finding trade-offs**: Directory scan is slower than sequential JSONL read for queries. Mitigated by `index.json` (loaded into memory on startup, ~1ms for 10k findings). Index is regenerable — if corrupt or stale, `tally rebuild-index` recreates it from finding files.

12. **GitHub server-side merge**: GitHub ignores `.gitattributes` merge drivers during PR merges and web merges. One-file-per-finding avoids this limitation entirely — git's native 3-way merge handles new files without any custom logic.

### Key Design Decisions

**Storage: One-file-per-finding on orphan branch**
- Researched 5 approaches: single JSONL, one-file-per-finding (git-bug style), git notes, SQLite blob (Fossil), sharded JSONL
- One-file-per-finding wins: zero merge conflicts for concurrent writes, works with GitHub server-side merge (custom merge drivers are ignored by GitHub), per-finding git history, scales to hundreds of concurrent agents
- Single JSONL rejected: EOF contention, GitHub ignores custom merge drivers, libgit2 doesn't retry ref locks
- Orphan branch: no working tree pollution, accessed via `git2` plumbing
- Index: `index.json` is derived (regenerable from finding files), uses `merge=ours` in `.gitattributes`

**Identity: UUID v7 + Content Fingerprint + Rule ID**
- UUID v7: stable forever, time-ordered
- Fingerprint (SHA-256): deterministic dedup, survives code relocation
- Rule ID: grouping key
- Resolution: fingerprint match > nearby location (5 lines) > new finding
- Modeled after SonarQube + CodeClimate + git-bug

**State Machine: 10 states**
- Modeled after SonarQube + CodeClimate + Semgrep
- `FALSE_POSITIVE` vs `WONT_FIX` vs `SUPPRESSED` — three different "not fixing" reasons
- All transitions recorded with agent, timestamp, reason, commit

**Interface: MCP + CLI dual mode**
- MCP: Claude Code, Cursor, Windsurf (native tool discovery via `rmcp`)
- CLI: Aider, scripts, CI (structured JSON output)
- Same core logic, two entry points

---

## Deliverable: Cargo.toml

```toml
[package]
name = "tally"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "Apache-2.0"
description = "Git-backed findings tracker for AI coding agents"
repository = "https://github.com/1898andCo/tally"

[dependencies]
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
comfy-table = "7"
git2 = "0.19"
hex = "0.4"
rmcp = { version = "0.8", features = ["server", "transport-io", "macros"] }
schemars = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "2"
tokio = { version = "1", features = ["io-std", "rt", "macros"] }
uuid = { version = "1", features = ["v7", "serde"] }

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
proptest = "1"
tempfile = "3"

[lints.clippy]
all = { level = "deny" }
pedantic = { level = "warn" }
unwrap_used = { level = "deny" }

[lints.rust]
unsafe_code = { level = "forbid" }
```

## Deliverable: rust-toolchain.toml

```toml
[toolchain]
channel = "stable"
```

## Deliverable: .typos.toml

```toml
[default.extend-words]
jsonl = "jsonl"
rmcp = "rmcp"
sarif = "sarif"

[files]
extend-exclude = [
  ".claude/",
  "target/",
]
```

## Deliverable: deny.toml

```toml
[advisories]
ignore = []
yanked = "warn"

[licenses]
allow = [
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "MIT",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Unicode-3.0",
  "Zlib",
  "CC0-1.0",
]

[bans]
multiple-versions = "warn"
```

## Deliverable: .lefthook.yml

```yaml
glob_matcher: doublestar

pre-commit:
  parallel: true
  commands:
    fmt-check:
      glob: "**/*.rs"
      run: test -f "$HOME/.cargo/env" && . "$HOME/.cargo/env"; cargo +nightly fmt --all -- --check
    clippy:
      glob: "**/*.rs"
      run: test -f "$HOME/.cargo/env" && . "$HOME/.cargo/env"; cargo clippy --all-targets --all-features -- -D warnings
    taplo:
      glob: "**/*.toml"
      run: test -f "$HOME/.cargo/env" && . "$HOME/.cargo/env"; taplo fmt --check
    test:
      glob: "**/*.rs"
      run: test -f "$HOME/.cargo/env" && . "$HOME/.cargo/env"; cargo test
    typos:
      run: test -f "$HOME/.cargo/env" && . "$HOME/.cargo/env"; typos
```

## Deliverable: .gitignore

```
/target
.DS_Store
*.swp
*.swo
*~
.idea/
.vscode/
```

## Deliverable: .github/workflows/ci.yml

```yaml
name: CI

on:
  push:
    branches: [develop, main, "release/**", "hotfix/**"]
    paths-ignore:
      - '**.md'
      - 'docs/**'
      - 'LICENSE'
      - '.gitignore'
      - '.gitattributes'
  pull_request:
    paths-ignore:
      - '**.md'
      - 'docs/**'
      - 'LICENSE'
      - '.gitignore'
      - '.gitattributes'
  workflow_dispatch:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  CARGO_INCREMENTAL: 0
  RUST_BACKTRACE: 1

jobs:
  check-fmt:
    name: Rustfmt
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
          fetch-depth: 1
      - uses: dtolnay/rust-toolchain@nightly
        with:
          components: rustfmt
      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true
      - run: cargo +nightly fmt --all -- --check

  check-clippy:
    name: Clippy
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
          fetch-depth: 1
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true
      - run: cargo clippy --all-targets --all-features -- -D warnings

  build:
    name: Build
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
          fetch-depth: 1
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true
      - run: cargo build --all-targets

  test:
    name: Test
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
          fetch-depth: 1
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true
      - run: cargo test --all-targets
      - run: cargo test --doc --all-features

  deny:
    name: Cargo Deny
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
          fetch-depth: 1
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories licenses bans
```

## Deliverable: .github/workflows/ci_status.yml

```yaml
name: CI Status

on:
  push:
    branches: [develop, main, "release/**", "hotfix/**"]
  pull_request:
  workflow_dispatch:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  detect:
    name: Detect Changes
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 5
    outputs:
      doc_only: ${{ steps.check.outputs.doc_only }}
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false
      - id: check
        run: |
          if [ "${{ github.event_name }}" = "pull_request" ]; then
            BASE="${{ github.event.pull_request.base.sha }}"
          else
            BASE="${{ github.event.before }}"
          fi
          CHANGED=$(git diff --name-only "$BASE" HEAD 2>/dev/null || git diff --name-only HEAD~1 HEAD)
          CODE_CHANGES=$(echo "$CHANGED" | grep -v -E '\.md$|^docs/|^LICENSE$|^\.gitignore$|^\.gitattributes$' || true)
          if [ -z "$CODE_CHANGES" ]; then
            echo "doc_only=true" >> "$GITHUB_OUTPUT"
          else
            echo "doc_only=false" >> "$GITHUB_OUTPUT"
          fi

  fmt-status:
    name: Rustfmt
    needs: detect
    if: needs.detect.outputs.doc_only == 'true'
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 1
    steps:
      - run: echo "Skipped — doc-only change"

  clippy-status:
    name: Clippy
    needs: detect
    if: needs.detect.outputs.doc_only == 'true'
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 1
    steps:
      - run: echo "Skipped — doc-only change"

  build-status:
    name: Build
    needs: detect
    if: needs.detect.outputs.doc_only == 'true'
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 1
    steps:
      - run: echo "Skipped — doc-only change"

  test-status:
    name: Test
    needs: detect
    if: needs.detect.outputs.doc_only == 'true'
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 1
    steps:
      - run: echo "Skipped — doc-only change"

  deny-status:
    name: Cargo Deny
    needs: detect
    if: needs.detect.outputs.doc_only == 'true'
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 1
    steps:
      - run: echo "Skipped — doc-only change"
```

## Deliverable: .github/workflows/ci_typos.yml

```yaml
name: Typos

on:
  push:
    branches: [develop, main]
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  typos:
    name: Check Typos
    runs-on: [self-hosted, Ubuntu, Common]
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
      - uses: crate-ci/typos@v1
```

## Deliverable: .mcp.json (example for Claude Code)

```json
{
  "mcpServers": {
    "tally": {
      "command": "tally",
      "args": ["mcp-server"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

## Deliverable: CLAUDE.md

```markdown
# CLAUDE.md

## Build & Test

\`\`\`bash
cargo build                    # Build
cargo test                     # Run all tests
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo +nightly fmt --all       # Format (requires nightly)
just check                     # Quick pre-commit (fmt + clippy + deny)
just ci                        # Full CI mirror
\`\`\`

Rust 1.85+, Edition 2024. Nightly rustfmt required.

## Git Workflow

Git Flow — branch from \`develop\`, PRs target \`develop\`.

- Branch naming: \`feature/desc\`, \`fix/desc\`
- Conventional commits enforced by lefthook
- No AI attribution in commit messages

## Architecture

- Single binary crate (not a workspace)
- Git-backed storage on orphan \`findings-data\` branch via \`git2\`
- Dual interface: CLI (clap) + MCP server (rmcp)
- \`#![forbid(unsafe_code)]\`
- No \`unwrap()\` in production code

@.claude/rules/_index.md
```

## Deliverable: .claude/rules/_index.md

```markdown
## Claude Code Rules

@rust.md
@git-commits.md
```

## Deliverable: .claude/rules/rust.md

```markdown
# Rust Coding Rules

## Safety

- \`#![forbid(unsafe_code)]\` in main crate
- No \`unwrap()\` in production code — use \`?\` or \`expect()\` with message
- No blocking the async runtime — use \`spawn_blocking\` for CPU work

## Error Handling

- Use \`thiserror\` for error enums
- \`pub type Result<T> = std::result::Result<T, FindingsError>\`
- Structured error variants, not string bags

## Module Structure

- \`lib.rs\`: public API re-exports only
- \`error.rs\`: crate error types
- Domain modules in subdirectories

## Testing

- Unit tests: \`#[cfg(test)] mod tests {}\` for private internals
- Integration tests: \`tests/\` directory
- Property tests: \`tests/property_*.rs\` with \`proptest\`
- Test names as documentation: \`fingerprint_deterministic_for_same_input()\`
```

## Deliverable: .claude/rules/git-commits.md

```markdown
# Git Commit Standards

Format: \`<type>[optional scope]: <description>\`

Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore

- Imperative present tense ("add" not "added")
- No capital first letter, no trailing period
- No AI attribution lines
```

## Deliverable: justfile

```just
# tally Development Commands

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments := true

# List available recipes
default:
    @just --list

# ---------------------------------------------------------------------------
# Private guards
# ---------------------------------------------------------------------------

[private]
_require cmd install_hint:
    @command -v {{ cmd }} &>/dev/null || { echo "{{ cmd }} not installed — run: {{ install_hint }}"; exit 1; }

[private]
_require-nightly:
    @rustup run nightly rustfmt --version &>/dev/null || { echo "nightly toolchain required — run: rustup toolchain install nightly --component rustfmt"; exit 1; }

# ---------------------------------------------------------------------------
# Build & Test
# ---------------------------------------------------------------------------

[group('build')]
build:
    cargo build --all-targets

[group('test')]
test:
    cargo test --all-targets

[group('test')]
test-doc:
    cargo test --doc --all-features

# ---------------------------------------------------------------------------
# Quality Checks
# ---------------------------------------------------------------------------

[group('check')]
check: check-fmt check-clippy check-deny

[group('check')]
check-fmt: _require-nightly
    cargo +nightly fmt --all -- --check

[group('check')]
check-clippy:
    cargo clippy --all-targets --all-features -- -D warnings

[group('check')]
check-deny: (_require "cargo-deny" "cargo install cargo-deny --locked")
    cargo deny check advisories licenses bans

[group('check')]
check-toml: (_require "taplo" "cargo install taplo-cli --locked")
    taplo fmt --check

[group('check')]
lint: check

# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

[group('format')]
fmt: _require-nightly
    cargo +nightly fmt --all

[group('format')]
fmt-toml: (_require "taplo" "cargo install taplo-cli --locked")
    taplo fmt

[group('format')]
fmt-all: fmt fmt-toml

# ---------------------------------------------------------------------------
# Development
# ---------------------------------------------------------------------------

[group('dev')]
dev: (_require "cargo-watch" "cargo install cargo-watch --locked")
    cargo watch -x check -x 'test --lib'

[group('dev')]
install-hooks: (_require "lefthook" "brew install lefthook")
    lefthook install

[group('dev')]
clean:
    cargo clean

# ---------------------------------------------------------------------------
# CI
# ---------------------------------------------------------------------------

[group('ci')]
ci: check-fmt check-clippy build check-deny test test-doc
    @echo ""
    @echo "All CI jobs passed."

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

[group('setup')]
setup:
    #!/usr/bin/env bash
    echo "Installing development tools..."
    rustup component add clippy
    rustup toolchain install nightly --component rustfmt
    cargo install cargo-watch cargo-deny cargo-nextest --locked
    if command -v brew &>/dev/null; then
        brew install taplo lefthook
    else
        cargo install taplo-cli --locked
    fi
    echo "Done. Run 'just check' to verify."
```

---

## Output Format Examples

### CLI: `--format json`

```json
[
  {
    "uuid": "019e1a2b-3c4d-7e5f-8a9b-0c1d2e3f4a5b",
    "short_id": "C1",
    "content_fingerprint": "sha256:a3f4c8e291d7b6c5e4f3d2a1b0c9d8e7f6a5b4c3",
    "rule_id": "unsafe-unwrap",
    "file_path": "src/main.rs",
    "line_start": 42,
    "line_end": 42,
    "severity": "important",
    "status": "open",
    "title": "Unwrap on Option without error handling",
    "description": "Line 42 calls .unwrap() on an Option that could be None.",
    "created_at": "2026-03-13T22:17:00Z",
    "discovered_by": [
      {"agent_id": "claude-code", "session_id": "sess_abc123", "detected_at": "2026-03-13T22:17:00Z"}
    ]
  }
]
```

### CLI: `--format table`

```
 UUID (short)   Severity   Status   File             Line   Title
 C1             CRITICAL     OPEN     src/main.rs      42     Unwrap on Option without error handling
 I1             IMPORTANT    OPEN     src/storage.rs   88     Missing error context in git operation
 S1             SUGGESTION   OPEN     src/cli/mod.rs   15     Consider using clap derive groups
```

### CLI: `--format summary`

```
Findings Summary
  Critical:    1
  Important:   1
  Suggestion:  1
  Tech Debt:   0
  Total:       3

  Open:          3
  In Progress:   0
  Resolved:      0
```

### Export: SARIF 2.1.0

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [
    {
      "tool": {
        "driver": {
          "name": "tally",
          "version": "0.1.0",
          "informationUri": "https://github.com/1898andCo/tally",
          "rules": [
            {
              "id": "unsafe-unwrap",
              "shortDescription": {"text": "Unsafe unwrap on Option or Result"}
            }
          ]
        }
      },
      "results": [
        {
          "ruleId": "unsafe-unwrap",
          "level": "warning",
          "message": {"text": "Unwrap on Option without error handling"},
          "locations": [
            {
              "physicalLocation": {
                "artifactLocation": {"uri": "src/main.rs"},
                "region": {"startLine": 42, "endLine": 42}
              }
            }
          ]
        }
      ]
    }
  ]
}
```

### Export: CSV

```csv
uuid,short_id,severity,status,rule_id,file_path,line_start,line_end,title,created_at
019e1a2b-3c4d-7e5f-8a9b-0c1d2e3f4a5b,C1,critical,open,unsafe-unwrap,src/main.rs,42,42,Unwrap on Option without error handling,2026-03-13T22:17:00Z
```

---

## Test Fixtures

### Fixture 1: Basic Round-Trip (AC-1, AC-2, AC-6)

```
tests/fixtures/basic-roundtrip/
  setup.sh              # git init, tally init
  expected_branch.txt   # "findings-data" — verify branch exists
  record_input.json     # {"file": "src/main.rs", "line": 42, ...}
  expected_output.json  # {"uuid": "...", "status": "open", ...}
```

**Test**: Init repo, record finding, query back. Verify UUID is present, status is `Open`, fingerprint matches.

### Fixture 2: Deduplication (AC-7)

**Test**: Record same finding twice (same file, line, rule). Verify only one UUID exists, `discovered_by` has two entries.

### Fixture 3: Invalid Transition (AC-3)

**Test**: Create finding (status `Open`), attempt `update --status closed`. Verify exit code 1, error message includes "valid transitions".

### Fixture 4: Suppression with Expiry (AC-10)

**Test**: Create finding, suppress with expiry `2026-01-01T00:00:00Z` (past date). Query. Verify status is `Open` (auto-reopened).

### Fixture 5: Empty Query (AC-5)

**Test**: Init repo (no findings), query with `--format json`. Verify stdout is `[]`, exit code is 0.

### Fixture 6: Orphan Branch Isolation (AC-6)

**Test**: Init repo, record finding. Verify `git status` shows clean working tree. Verify `git log` on main/HEAD has no findings commits. Verify `git log findings-data` has findings commits.

---

## Non-Goals (Explicit Exclusions)

- **Not a replacement for dclaude/zclaude** — those tools discover findings; this tool persists them
- **Not a static analysis engine** — no code scanning; agents provide findings
- **Not a CI/CD gate** — no pass/fail decisions; agents use findings to inform verdicts
- **Not a dashboard/UI** — CLI and MCP only; visualization is future work
- **Not multi-repo aggregation** — MVP is single-repo; cross-repo is future
- **No shell scripts** — all logic in Rust; justfile uses inline bash for recipes only

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Finding deduplication accuracy | >95% (same issue from two agents -> one UUID) |
| Cross-session retrieval | 100% (every finding from session A visible in session B) |
| MCP tool response time | <100ms p99 for repos with <10k findings |
| CLI cold start | <500ms (scan finding files + build indexes) |
| State transition validation | 100% coverage (all invalid transitions rejected) |
| Working tree isolation | 100% (no git operations modify working tree or HEAD) |

---

## Deliverable: README.md

````markdown
# tally

Git-backed findings tracker for AI coding agents. Persistent, content-addressable finding identity across sessions, agents, and PRs.

## Installation

```bash
# From source
git clone https://github.com/1898andCo/tally.git
cd tally
cargo install --path .

# Or directly
cargo install --git https://github.com/1898andCo/tally.git
```

## Quick Start

```bash
# Initialize findings storage in your repo
cd your-project
tally init

# Record a finding
tally record \
  --file src/main.rs \
  --line 42 \
  --severity high \
  --title "unwrap on Option" \
  --rule unsafe-unwrap \
  --description "Line 42 calls .unwrap() without error handling"

# Query findings
tally query --status open --format table

# Update status
tally update <uuid> --status resolved --reason "fixed in commit abc123"

# Summary
tally stats
```

## CLI Reference

### `tally init`

Initialize the `findings-data` orphan branch in the current git repo.

### `tally record`

Create or deduplicate a finding.

| Flag | Required | Description |
|------|----------|-------------|
| `--file` | Yes | File path |
| `--line` | Yes | Line number (or `--line-start`/`--line-end` for ranges) |
| `--severity` | Yes | `critical`, `important`, `suggestion`, `tech-debt` |
| `--title` | Yes | Short finding title |
| `--rule` | Yes | Rule ID for grouping (e.g., `unsafe-unwrap`) |
| `--description` | No | Detailed description |
| `--tags` | No | Comma-separated tags |
| `--agent` | No | Agent identifier (default: `cli`) |
| `--session` | No | Session identifier |

### `tally query`

Search findings with filters.

| Flag | Description |
|------|-------------|
| `--status` | Filter by status (`open`, `in-progress`, `resolved`, etc.) |
| `--severity` | Filter by severity |
| `--file` | Filter by file path (glob supported) |
| `--rule` | Filter by rule ID |
| `--format` | Output format: `json` (default), `table`, `summary` |
| `--limit` | Max results (default: 100) |

### `tally update <id>`

Transition finding lifecycle status. Accepts UUID or session short ID (C1, I2).

| Flag | Required | Description |
|------|----------|-------------|
| `--status` | Yes | Target status |
| `--reason` | No | Reason for transition |
| `--commit` | No | Commit SHA that resolved the finding |

### `tally suppress <id>`

Suppress a finding.

| Flag | Required | Description |
|------|----------|-------------|
| `--reason` | Yes | Suppression reason |
| `--expires` | No | Expiry date (ISO 8601) |

### `tally stats`

Display summary statistics by severity and status.

### `tally export`

| Flag | Description |
|------|-------------|
| `--format` | `sarif` (SARIF 2.1.0), `csv`, `json` |
| `--output` | Output file (default: stdout) |

### `tally mcp-server`

Run as MCP server over stdio for Claude Code / Cursor / Windsurf integration.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Application error (invalid args, invalid transition) |
| 2 | Git storage error |

## MCP Server Setup

### Claude Code

Add to `.mcp.json` in your project root:

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

### Cursor

Add to `.cursor/mcp.json`:

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

### Aider

Use CLI mode directly:

```bash
aider --tool "tally query --format json"
```

## Storage Model

Findings are stored on a `findings-data` orphan branch as individual JSON files (`findings/<uuid>.json`). This branch never affects your working tree. All operations use git plumbing (no checkout). One-file-per-finding enables zero-conflict concurrent writes from multiple agents.

## License

Apache-2.0
````

## Deliverable: LICENSE

Standard Apache License 2.0 text. Use the canonical file from https://www.apache.org/licenses/LICENSE-2.0.txt — do NOT abbreviate or modify.

## Deliverable: CONTRIBUTING.md

````markdown
# Contributing to tally

## Development Setup

```bash
git clone https://github.com/1898andCo/tally.git
cd tally
just setup        # Install dev tools
just install-hooks # Install git hooks
just check        # Verify setup
```

## Git Workflow

- Branch from `develop`, PRs target `develop`
- Conventional commits enforced by lefthook
- No AI attribution in commit messages

## Running Tests

```bash
just test         # All tests
just test-doc     # Doc tests
just ci           # Full CI mirror
```

## Code Standards

- `#![forbid(unsafe_code)]`
- No `unwrap()` in production code
- `cargo clippy -- -D warnings` must pass
- `cargo +nightly fmt --all` for formatting
````

---

## Research Basis

- **dclaude v2.5.0**: 20 agents, 4 severity tiers (C/I/S/TD), pr-fix-verify state file (schema v2.0), known-spec-conflicts suppression, adaptive scanning, finding data model with id/severity/title/file/lines/category/agents/status
- **zclaude v1.1.1**: 6 parallel agents, severity-based auto-approval, `.claude/pr-reviews/$PR.json` state, diff-matching for re-reviews, scope-aware downgrade
- **Deep research — architecture (Mar 2026)**: git-bug (content-addressed, one-file-per-bug), Fossil (SQLite), SonarQube (rule+hash identity), Snyk (CVE registry), Semgrep (rule+location), CodeClimate (UUID+remap), MCP spec (newline-delimited JSON-RPC), rmcp crate v0.8 (official Anthropic Rust SDK), Claude Code config (`.mcp.json`), git2 orphan branch API (no checkout)
- **Deep research — multi-user concurrency (Mar 2026)**: libgit2 ref locks (no retry, `.git/refs/heads/<branch>.lock`), GitHub ignores custom merge drivers on server-side merges, git 3-way merge auto-resolves new files (zero conflicts for one-file-per-finding), git worktrees share ref locks (don't help concurrency), git-bug serializes writes + uses eventual consistency, JSONL EOF contention in concurrent appends, one-file-per-finding eliminates all merge conflict scenarios for concurrent creation
- **Deep research — batch/severity/relationships (Mar 2026)**: SARIF severity mapping (error/warning/note/none), SARIF multi-location findings, SARIF relatedLocations, partial success batch semantics (207 Multi-Status), idempotency keys for retry safety, finding relationship types (duplicate_of/blocks/related_to/causes/discovered_while_fixing/supersedes)
