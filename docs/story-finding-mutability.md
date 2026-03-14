# Story: Finding Mutability & Lifecycle Enhancements

Status: proposal
Repository: `1898andCo/tally`
Language: Rust
License: Apache-2.0
Date: 2026-03-14

---

## Problem Statement

During a check-drift session on the axiathon project (Story 5.1 — Query Language Design), three operational pain points emerged with tally v0.4.0:

1. **Findings are immutable after creation.** When deep research changed our understanding of a finding (e.g., D1 was initially "replace MATCHES with =~" but became "support both as aliases"), the `suggested_fix` and `description` fields became stale. The resolution `reason` on the status transition captured the correct decision, but the finding itself still displays the wrong fix to anyone querying it.

2. **State machine has a gap.** After deferring DT1 (`OcsfVersion` re-export), we discovered it was covered by Story 1.21 AC-2 and wanted to add that context. The `deferred → reopened` transition is invalid (only `deferred → open` and `deferred → closed` are allowed). We couldn't reattach context without changing state, and we couldn't change state to a useful intermediate.

3. **No annotation mechanism.** We wanted to attach a note ("covered by Story 1.21 AC-2") to a finding without changing its status. There's no way to add context to a finding after creation except through status transitions (which require state changes).

These are not edge cases — they represent the normal workflow of AI-assisted code review where understanding evolves during a session.

---

## Solution

Add four capabilities to tally:

1. **`update_finding` MCP tool + `tally update-fields` CLI** — edit mutable fields (`description`, `suggested_fix`, `evidence`, `title`, `severity`, `category`, `tags`) on existing findings
2. **`add_note` MCP tool + `tally note` CLI** — append timestamped notes to findings without changing status
3. **State machine expansion** — add `deferred → reopened` and `suppressed → reopened` transitions
4. **`add_tag` MCP tool + `tally tag` CLI** — add/remove tags for lightweight classification without full field edits

---

## Story

As an **AI coding agent** performing multi-pass reviews,
I want **to update finding details when my understanding improves**,
So that **the finding record reflects the final decision, not just the initial assessment**.

As a **developer reviewing tally findings**,
I want **to see notes and context attached to findings**,
So that **I understand why a finding was deferred, suppressed, or resolved without reading the full session transcript**.

As an **AI coding agent** tracking deferred work,
I want **to reopen deferred findings when new information surfaces**,
So that **findings can re-enter the active workflow without manual workarounds**.

---

## Prerequisites

Before implementation:

1. **Branch**: Create `feature/finding-mutability` from `develop`
2. **No new dependencies**: All changes use existing crate dependencies
3. **Schema migration**: Finding JSON format gains a `notes` field — must be backward-compatible via `#[serde(default)]`
4. **Node.js**: Required for BMAD installation (`npx bmad-method install`). Node.js 18+ LTS recommended.

---

## Acceptance Criteria

### AC-1: Mutable field editing via MCP

**Given** a finding exists with UUID `019cebd0-5c2a-...`
**When** the agent calls `update_finding` with `{ finding_id: "019cebd0-5c2a-...", suggested_fix: "Support both =~ and MATCHES as aliases" }`
**Then** the finding's `suggested_fix` field is updated in the stored JSON
**And** `updated_at` is set to the current timestamp
**And** a `FieldEdit` entry is appended to the finding's `edit_history` array with: field name, old value, new value, timestamp, agent_id
**And** the finding's `uuid`, `content_fingerprint`, `rule_id`, `created_at`, `status`, and `state_history` are NOT modified

### AC-2: Mutable field editing via CLI

**Given** a finding exists
**When** I run `tally update-fields <uuid> --suggested-fix "new fix" --description "updated description"`
**Then** the same behavior as AC-1 occurs
**And** the output shows the updated finding in the requested format (json/table)
**And** multiple fields can be updated in a single command

### AC-3: Editable fields are explicitly defined

**Given** the Finding struct
**Then** exactly these fields are editable after creation:
- `title` — concise summary (may need rewording after research)
- `description` — detailed explanation (understanding evolves)
- `suggested_fix` — recommended fix (deep research changes the fix)
- `evidence` — supporting evidence (new evidence surfaces)
- `severity` — reclassification (initial triage vs. final assessment)
- `category` — reclassification
- `tags` — lightweight classification

**And** these fields are immutable (identity + provenance):
- `uuid` — stable identity
- `content_fingerprint` — deduplication key
- `rule_id` — grouping key
- `locations` — handled by separate location-update mechanism (future)
- `created_at` — provenance
- `discovered_by` — provenance
- `status` — managed by `update_finding_status` (state machine)
- `state_history` — append-only audit trail
- `relationships` — managed by `record_finding` relationship params
- `suppression` — managed by `suppress_finding`
- `repo_id`, `branch`, `pr_number`, `commit_sha` — context provenance

### AC-4: Edit history is recorded

**Given** a finding's `description` is edited from "old" to "new"
**Then** the finding's `edit_history` array contains:
```json
{
  "field": "description",
  "old_value": "old",
  "new_value": "new",
  "timestamp": "2026-03-14T10:15:00Z",
  "agent_id": "dclaude:check-drift"
}
```
**And** `edit_history` is append-only (entries are never removed)
**And** `get_finding_context` includes the edit history in its response

### AC-5: Notes can be added to findings

**Given** a finding exists in any lifecycle state
**When** the agent calls `add_note` with `{ finding_id: "...", note: "Covered by Story 1.21 AC-2", agent: "dclaude:check-drift" }`
**Then** a `Note` entry is appended to the finding's `notes` array with: text, timestamp, agent_id
**And** `updated_at` is set to the current timestamp
**And** the finding's status is NOT changed
**And** `get_finding_context` includes notes in its response

### AC-6: Notes via CLI

**Given** a finding exists
**When** I run `tally note <uuid> "Covered by Story 1.21 AC-2" --agent dclaude:check-drift`
**Then** the same behavior as AC-5 occurs
**And** short IDs (C1, I2) are accepted in place of UUID

### AC-7: State machine allows deferred → reopened

**Given** a finding in `deferred` status
**When** the agent calls `update_finding_status` with `new_status: "reopened"`
**Then** the transition succeeds
**And** a `StateTransition` is recorded with from=deferred, to=reopened
**And** the finding can then transition to `acknowledged` or `in_progress` (existing reopened transitions)

### AC-8: State machine allows suppressed → reopened

**Given** a finding in `suppressed` status
**When** the agent calls `update_finding_status` with `new_status: "reopened"`
**Then** the transition succeeds
**And** a `StateTransition` is recorded with from=suppressed, to=reopened

### AC-9: Tags can be added/removed independently

**Given** a finding exists with tags `["check-drift"]`
**When** the agent calls `add_tag` with `{ finding_id: "...", tags: ["story:1.21", "wave-1"] }`
**Then** the finding's tags become `["check-drift", "story:1.21", "wave-1"]`
**And** duplicate tags are not added
**When** the agent calls `remove_tag` with `{ finding_id: "...", tags: ["check-drift"] }`
**Then** the finding's tags become `["story:1.21", "wave-1"]`

### AC-10: Tags via CLI

**Given** a finding exists
**When** I run `tally tag <uuid> --add story:1.21 --add wave-1 --remove check-drift`
**Then** the same behavior as AC-9 occurs

### AC-11: Backward compatibility

**Given** a finding JSON file created by tally v0.4.0 (no `notes` or `edit_history` fields)
**When** tally v0.5.0 loads it
**Then** `notes` defaults to `[]` and `edit_history` defaults to `[]`
**And** the finding is fully functional (can be queried, updated, edited, noted)
**And** no migration step is required

### AC-12: Query filtering by tags

**Given** findings exist with various tags
**When** the agent calls `query_findings` with `{ tag: "story:1.21" }`
**Then** only findings with that tag are returned
**And** tag filtering works in both MCP and CLI interfaces

---

## Architecture

### New Data Types

```rust
/// A timestamped note attached to a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub text: String,
    #[serde(default = "default_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub agent_id: String,
}

/// A record of a field edit for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldEdit {
    pub field: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
    #[serde(default = "default_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub agent_id: String,
}
```

### Finding Struct Changes

```rust
pub struct Finding {
    // ... existing fields unchanged ...

    /// Timestamped notes (append-only). Added in v0.5.0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,

    /// Field edit audit trail (append-only). Added in v0.5.0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edit_history: Vec<FieldEdit>,
}
```

### State Machine Changes

```
Before (v0.4.0):
  Deferred   → Open, Closed
  Suppressed → Open, Closed

After (v0.5.0):
  Deferred   → Open, Reopened, Closed
  Suppressed → Open, Reopened, Closed
```

Only two transitions added. No existing transitions removed. `Reopened` already supports `→ Acknowledged` and `→ InProgress`, so deferred/suppressed findings can re-enter the active workflow through the existing path.

### New MCP Tools (4)

| Tool | Description | Key Parameters |
|------|-------------|----------------|
| `update_finding` | Edit mutable fields on an existing finding | finding_id, title?, description?, suggested_fix?, evidence?, severity?, category?, tags?, agent? |
| `add_note` | Append a timestamped note to a finding | finding_id, note, agent? |
| `add_tag` | Add tags to a finding | finding_id, tags[], agent? |
| `remove_tag` | Remove tags from a finding | finding_id, tags[], agent? |

### New CLI Commands (3)

| Command | Description |
|---------|-------------|
| `tally update-fields <id> [--title X] [--description X] [--suggested-fix X] [--evidence X] [--severity X] [--category X] [--tags X]` | Edit mutable fields |
| `tally note <id> "text" [--agent X]` | Add a note |
| `tally tag <id> [--add X]... [--remove X]...` | Add/remove tags |

### Updated MCP Tool: `query_findings`

Add `tag` parameter (string, optional) to filter by tag substring match.

### Updated CLI: `tally query`

Add `--tag <tag>` filter flag.

---

## Tasks / Subtasks

### Phase 0: Project Foundation Updates

- [ ] Task 0: Create SOUL.md
  - [ ] 0.1: Create `SOUL.md` at repo root (see Deliverable: SOUL.md)
  - [ ] 0.2: Principles adapted from axiathon's SOUL.md — universally applicable subset only, no multi-tenant or UI-specific principles
  - [ ] 0.3: Verify: no axiathon-specific terminology leaks into tally's SOUL.md

- [ ] Task 0.1: Expand `.claude/rules/rust.md`
  - [ ] 0.1.1: Replace current 27-line `rust.md` with expanded version (see Deliverable: .claude/rules/rust.md)
  - [ ] 0.1.2: Adds: type design (newtypes, validated constructors, `#[non_exhaustive]` guidance), security error comments, test boundary patterns, `Display` for logging only

- [ ] Task 0.2: Expand `.claude/rules/git-commits.md`
  - [ ] 0.2.1: Replace current 10-line `git-commits.md` with full spec (see Deliverable: .claude/rules/git-commits.md)
  - [ ] 0.2.2: Adds: type table, scope guidance, body/footer format, breaking changes, `--admin` bypass rule

- [ ] Task 0.3: Update `CLAUDE.md`
  - [ ] 0.3.1: Add `@SOUL.md` reference
  - [ ] 0.3.2: Add "Project State" section with current version, test count, architecture summary
  - [ ] 0.3.3: Add "On-Demand References" table linking to story docs and release process
  - [ ] 0.3.4: See Deliverable: CLAUDE.md

- [ ] Task 0.4: Update `.claude/rules/_index.md`
  - [ ] 0.4.1: Verify index references exactly the rules files that exist — no more, no fewer

- [ ] Task 0.5: Update `CONTRIBUTING.md`
  - [ ] 0.5.1: Replace current 33-line `CONTRIBUTING.md` with full guide (see Deliverable: CONTRIBUTING.md)
  - [ ] 0.5.2: Includes: prerequisites table with Homebrew Rust warning, first-time setup steps, Git Flow branch structure, `findings-data` branch protection note, conventional commit format with scopes, pre-commit checks, PR process, project structure, testing conventions, release process summary

- [ ] Task 0.6: Install and configure BMAD framework
  - [ ] 0.6.1: Install BMAD v6.0.3 via `npx bmad-method install` in the tally repo root. This creates the `_bmad/` directory with core, bmm, cis, and tea modules.
  - [ ] 0.6.2: Update `_bmad/bmm/config.yaml` for tally:
    ```yaml
    project_name: tally
    user_skill_level: intermediate
    planning_artifacts: "{project-root}/_bmad-output/planning-artifacts"
    implementation_artifacts: "{project-root}/_bmad-output/implementation-artifacts"
    project_knowledge: "{project-root}/docs"
    user_name: Joshua
    communication_language: English
    document_output_language: English
    output_folder: "{project-root}/_bmad-output"
    ```
  - [ ] 0.6.3: Create `_bmad-output/` directory structure:
    ```
    _bmad-output/
    ├── planning-artifacts/
    ├── implementation-artifacts/
    └── project-context.md
    ```
  - [ ] 0.6.4: Run `generate-project-context` workflow to create initial `project-context.md` from tally's codebase
  - [ ] 0.6.5: Create `.claude/settings.local.json` adapted for tally (see Deliverable: .claude/settings.local.json)
  - [ ] 0.6.6: Create `scripts/bmad-post-update.sh` adapted for tally — idempotent script to restore customizations after `npx bmad-method install` upgrades. For now, only patches upstream BMAD bugs (same bugs as axiathon v6.0.3). No custom agents needed yet.
  - [ ] 0.6.7: Add `_bmad-output/` to `.gitignore` if generated artifacts should not be tracked, OR commit them if they serve as the spec source of truth (decision: commit, matching axiathon's pattern)
  - [ ] 0.6.8: Move existing `docs/story.md` and `docs/story-finding-mutability.md` to `_bmad-output/implementation-artifacts/` for consistency with BMAD's artifact structure
  - [ ] 0.6.9: Verify: `just check` still passes after BMAD installation (no file conflicts)

  **What is NOT needed for tally:**
  - `_bmad-custom/` directory — no custom agents needed (tally is a single-crate CLI, not a platform with CI/CD/K8s concerns)
  - Sam (Platform Engineer) agent — axiathon-specific
  - Infrastructure standards memories — axiathon-specific
  - Agent memory injection script — no custom memories to inject
  - `.windsurf/workflows/` — only needed if using Windsurf IDE

  **What ships as-is from the BMAD installer:**
  - All 17 standard agents (pm, analyst, architect, dev, sm, qa, tech-writer, ux-designer, tea, cis agents)
  - All 40 workflows (analysis, planning, solutioning, implementation, quick-flow)
  - Step-based execution architecture
  - Memory system
  - Documentation standards (CommonMark, no time estimates)
  - Developer execution standards (TDD, test-first, full suite validation)

### Phase 1: Data Model Changes

- [ ] Task 1: Add `Note` and `FieldEdit` types to data model
  - [ ] 1.1: Add `Note` struct to `src/model/finding.rs` — `text: String`, `timestamp: DateTime<Utc>`, `agent_id: String`
  - [ ] 1.2: Add `FieldEdit` struct to `src/model/finding.rs` — `field: String`, `old_value: serde_json::Value`, `new_value: serde_json::Value`, `timestamp: DateTime<Utc>`, `agent_id: String`
  - [ ] 1.3: Add `notes: Vec<Note>` field to `Finding` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  - [ ] 1.4: Add `edit_history: Vec<FieldEdit>` field to `Finding` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  - [ ] 1.5: Tests: deserialize v0.4.0 finding JSON (no notes/edit_history) — fields default to `[]`. Use this fixture:
    ```json
    {
      "schema_version": "1.0.0",
      "uuid": "019cebd0-5c2a-7b73-8fb9-bdc551dce811",
      "content_fingerprint": "sha256:e7f50c20ae2afdc066f9839ff3b481429bcac80b85a5308f7f3e74d6fa906488",
      "rule_id": "spec-drift",
      "locations": [{"file_path": "src/main.rs", "line_start": 42, "line_end": 42, "role": "primary"}],
      "severity": "important",
      "category": "",
      "title": "unwrap on Option",
      "description": "Line 42 calls .unwrap() on an Option.",
      "suggested_fix": "Use ? or expect()",
      "evidence": null,
      "status": "open",
      "state_history": [],
      "discovered_by": [{"agent_id": "test", "session_id": "", "detected_at": "2026-03-14T10:00:00Z"}],
      "created_at": "2026-03-14T10:00:00Z",
      "updated_at": "2026-03-14T10:00:00Z",
      "repo_id": ""
    }
    ```
    Expected: `finding.notes == []`, `finding.edit_history == []`, all other fields populated correctly.
  - [ ] 1.6: Tests: serialize finding with notes → JSON contains `"notes": [...]` array
  - [ ] 1.7: Tests: serialize finding with empty notes → `"notes"` field omitted from JSON (verified by `!json_string.contains("notes")`)
  - [ ] 1.8: Audit `import_single_finding()` in `src/cli/import.rs` — verify imported findings get empty `notes` and `edit_history` via serde defaults. Add a test: import a dclaude-format finding → resulting Finding has `notes == []` and `edit_history == []`

- [ ] Task 2: Implement field editing logic
  - [ ] 2.1: Add `Finding::edit_field(&mut self, field: &str, new_value: serde_json::Value, agent_id: &str) -> Result<()>` method
  - [ ] 2.2: Method validates field name is in the editable set: `title`, `description`, `suggested_fix`, `evidence`, `severity`, `category`, `tags`
  - [ ] 2.3: Method captures old value as `serde_json::Value`, applies new value, appends `FieldEdit` to `edit_history`, updates `updated_at`
  - [ ] 2.4: Method returns `TallyError::InvalidInput` for non-editable fields with message listing editable fields
  - [ ] 2.5: For `severity` edits: validate new value is a valid severity string before applying
  - [ ] 2.6: For `tags` edits: accept either a single tag string or an array, merge with existing tags (don't replace)
  - [ ] 2.7: Add `Finding::add_note(&mut self, text: &str, agent_id: &str)` method — appends `Note`, updates `updated_at`
  - [ ] 2.8: Tests: edit `suggested_fix` — old value captured, new value applied, edit_history has 1 entry
  - [ ] 2.9: Tests: edit `uuid` — returns `InvalidInput` error
  - [ ] 2.10: Tests: edit `severity` with invalid value — returns error
  - [ ] 2.11: Tests: add note — notes array grows, status unchanged
  - [ ] 2.12: Tests: multiple edits — edit_history grows sequentially with correct timestamps

### Phase 2: State Machine Expansion

- [ ] Task 3: Add deferred/suppressed → reopened transitions
  - [ ] 3.1: Update `LifecycleState::allowed_transitions()` — add `Self::Reopened` to `Deferred` and `Suppressed` match arms
  - [ ] 3.2: Tests: `deferred → reopened` succeeds
  - [ ] 3.3: Tests: `suppressed → reopened` succeeds
  - [ ] 3.4: Tests: `reopened → in_progress` still works (existing transition, verify not broken)
  - [ ] 3.5: Tests: all existing transitions still work (run full matrix test). **Note:** existing negative tests asserting `deferred → reopened` and `suppressed → reopened` are *invalid* must be flipped to positive tests — these transitions are now valid.
  - [ ] 3.6: Update state transition matrix in `docs/story.md` (the original story)

### Phase 3: MCP Tools

- [ ] Task 4: Implement `update_finding` MCP tool
  - [ ] 4.1: Add `UpdateFindingInput` struct with `finding_id: String`, optional fields: `title`, `description`, `suggested_fix`, `evidence`, `severity`, `category`, `tags`, `agent`
  - [ ] 4.2: Implement `#[tool]` handler: load finding → validate at least one field provided → call `edit_field()` for each provided field → save finding → return updated finding JSON
  - [ ] 4.3: Support short IDs (C1, I2) via `SessionIdMapper` resolution
  - [ ] 4.4: Return error if finding not found, if field is non-editable, or if severity value is invalid
  - [ ] 4.5: Tests: MCP tool call updates description, returns finding with new value
  - [ ] 4.6: Tests: MCP tool call with no fields provided → returns error "at least one field must be specified"
  - [ ] 4.7: Tests: MCP tool call with non-editable field → returns error listing editable fields
  - [ ] 4.8: Verify new tools appear in `mcp-capabilities` output — rmcp's `#[tool]` macro auto-registers tools, but verify by running `tally mcp-capabilities` and confirming `update_finding` appears in the tools list

- [ ] Task 5: Implement `add_note` MCP tool
  - [ ] 5.1: Add `AddNoteInput` struct with `finding_id: String`, `note: String`, `agent: Option<String>`
  - [ ] 5.2: Implement `#[tool]` handler: load finding → call `add_note()` → save finding → return confirmation with note count
  - [ ] 5.3: Support short IDs
  - [ ] 5.4: Tests: MCP add_note → finding has 1 note, status unchanged
  - [ ] 5.5: Tests: MCP add_note twice → finding has 2 notes in chronological order

- [ ] Task 6: Implement `add_tag` and `remove_tag` MCP tools
  - [ ] 6.1: Add `TagInput` struct with `finding_id: String`, `tags: Vec<String>`, `agent: Option<String>`
  - [ ] 6.2: Implement `add_tag` handler: load finding → merge tags (dedup) → record `FieldEdit` → save finding
  - [ ] 6.3: Implement `remove_tag` handler: load finding → remove matching tags → record `FieldEdit` → save finding
  - [ ] 6.4: Tests: add_tag merges without duplicates
  - [ ] 6.5: Tests: remove_tag removes exact matches, ignores missing tags
  - [ ] 6.6: Tests: add_tag + remove_tag combined workflow

- [ ] Task 7: Update `query_findings` with tag filter
  - [ ] 7.1: Add `tag: Option<String>` parameter to `QueryFindingsInput` in `src/mcp/server.rs`
  - [ ] 7.2: Add `"tags": f.tags` to the index entry in `rebuild_index()` in `src/storage/git_store.rs` — tags must be indexed for fast query filtering without loading all finding files
  - [ ] 7.3: Implement tag filtering: substring match against finding's tags array (works on both index and loaded findings)
  - [ ] 7.4: Tests: query with tag filter returns only matching findings
  - [ ] 7.5: Tests: query with tag filter and other filters (AND combination)
  - [ ] 7.6: Tests: `rebuild-index` includes tags in regenerated index

- [ ] Task 8: Update `get_finding_context` response
  - [ ] 8.1: Include `notes` array in response (already included via Finding serialization, but verify)
  - [ ] 8.2: Include `edit_history` array in response
  - [ ] 8.3: Tests: get_finding_context on finding with notes/edits includes both arrays

### Phase 4: CLI Commands

- [ ] Task 9: Implement `tally update-fields` CLI command
  - [ ] 9.1: Add `UpdateFields` variant to `Command` enum in `src/cli/mod.rs` with `id: String` positional arg and optional flags: `--title`, `--description`, `--suggested-fix`, `--evidence`, `--severity`, `--category`, `--tags`, `--agent`
  - [ ] 9.2: Add match arm for `Command::UpdateFields` in `run()` function in `src/main.rs` — dispatches to handler
  - [ ] 9.3: Create `src/cli/update_fields.rs` — implement handler: resolve ID (UUID or short ID) → load finding → apply edits → save → display updated finding
  - [ ] 9.4: Support `--format json|table` output
  - [ ] 9.5: Tests: CLI update-fields changes description, verify JSON output
  - [ ] 9.6: Tests: CLI update-fields with short ID resolves correctly
  - [ ] 9.7: Tests: CLI update-fields with multiple flags applies all changes
  - [ ] 9.8: Verify `tally --help` lists `update-fields` with meaningful description (clap derive `about` attribute)

- [ ] Task 10: Implement `tally note` CLI command
  - [ ] 10.1: Add `AddNote` variant to `Command` enum in `src/cli/mod.rs` with `id: String` and `text: String` positional args, `--agent` optional flag (use `AddNote` not `Note` to avoid collision with the `Note` data struct)
  - [ ] 10.2: Add match arm for `Command::AddNote` in `run()` in `src/main.rs`
  - [ ] 10.3: Create `src/cli/note.rs` — implement handler: resolve ID → load finding → add note → save → display confirmation
  - [ ] 10.4: Tests: CLI note adds note, status unchanged
  - [ ] 10.5: Tests: CLI note with short ID works
  - [ ] 10.6: Verify `tally --help` lists `note` with meaningful description

- [ ] Task 11: Implement `tally tag` CLI command
  - [ ] 11.1: Add `ManageTags` variant to `Command` enum in `src/cli/mod.rs` with `id: String` positional arg, `--add` (repeatable) and `--remove` (repeatable) flags (use `ManageTags` not `Tag` to avoid collision with potential tag types)
  - [ ] 11.2: Add match arm for `Command::ManageTags` in `run()` in `src/main.rs`
  - [ ] 11.3: Create `src/cli/tag.rs` — implement handler: resolve ID → load finding → add/remove tags → save → display updated tags
  - [ ] 11.4: Tests: CLI tag --add adds, --remove removes
  - [ ] 11.5: Tests: CLI tag with both --add and --remove in same call
  - [ ] 11.6: Verify `tally --help` lists `tag` with meaningful description

- [ ] Task 12: Update `tally query` CLI with tag filter
  - [ ] 12.1: Add `--tag <tag>` flag to query subcommand
  - [ ] 12.2: Implement tag filtering in query handler
  - [ ] 12.3: Tests: CLI query --tag filters correctly

### Phase 5: Documentation & Release

- [ ] Task 13: SARIF export and documentation updates
  - [ ] 13.1: Update README.md — add `update-fields`, `note`, `tag` commands to CLI reference
  - [ ] 13.2: Update README.md — add `update_finding`, `add_note`, `add_tag`, `remove_tag` to MCP tools table
  - [ ] 13.3: Update README.md — update state transition matrix with new deferred/suppressed → reopened paths
  - [ ] 13.4: Update `docs/story.md` — add note about v0.5.0 enhancements in a "Post-MVP Enhancements" section
  - [ ] 13.5: Update MCP tool descriptions to mention the new tools in server capabilities
  - [ ] 13.6: Update SARIF export to include `notes` and `edit_history` in the `result.properties` bag using tool-prefixed keys per SARIF 2.1.0 extension conventions (deep researched Mar 2026):
    - `result.properties.tally_notes` — array of `{ text, timestamp, agent_id }` objects
    - `result.properties.tally_editHistory` — array of `{ field, oldValue, newValue, timestamp, agent_id }` objects
    - Use camelCase keys per SARIF property bag conventions
    - These are safely ignored by consumers that don't understand them (GitHub Code Scanning, SonarQube, etc.)
    - `result.properties.tally_tags` — the finding's tags array (already a first-class field, but not in SARIF standard)
    - Do NOT use `result.fixes[]` for `suggested_fix` — SARIF fixes require `artifactChanges` with concrete code edits, which tally's text-based suggestions don't provide
    - DO use `resultProvenance.firstDetectionTimeUtc` for `created_at` (SARIF standard field)
  - [ ] 13.7: Tests: SARIF export of finding with notes includes `tally_notes` in properties
  - [ ] 13.8: Tests: SARIF export validates against SARIF 2.1.0 JSON schema (custom properties in property bags are allowed by `additionalProperties: true`)
  - [ ] 13.9: Update `tally stats` output to include: findings with notes count, findings with edits count, top 5 tags by frequency. No new subcommand needed — extend existing stats handler.

### Phase 6: Fix sync_findings Authentication & Branch Protection

- [ ] Task 14: Add git2 credential callback to sync operations
  - [ ] 14.1: Add `build_remote_callbacks()` helper in `src/storage/git_store.rs` that creates `RemoteCallbacks` with a `credentials` callback implementing this priority chain:
    1. `git2::Cred::credential_helper()` — reads `credential.helper` from `~/.gitconfig` (supports osxkeychain, GitHub CLI `gh auth setup-git`, git-credential-store, Git Credential Manager)
    2. `GIT_TOKEN` or `GITHUB_TOKEN` environment variable — `Cred::userpass_plaintext("git", token)` for CI/Actions
    3. SSH agent — `Cred::ssh_key_from_agent(username)` when `allowed_types` includes `SSH_KEY`
    4. Return `Err` after first failure (no infinite retry — libgit2 calls the callback repeatedly on auth failure; track attempt count and bail after 1 attempt per strategy)
  - [ ] 14.2: Add `build_fetch_options()` helper that creates `FetchOptions` with the remote callbacks from 15.1
  - [ ] 14.3: Add `build_push_options()` helper that creates `PushOptions` with the remote callbacks from 15.1
  - [ ] 14.4: Update `sync()` method — pass `FetchOptions` to `remote.fetch()` and `PushOptions` to `remote.push()` instead of `None`
  - [ ] 14.5: Track auth attempt count via `Cell<u32>` in the callback closure — return error after exhausting all strategies to prevent libgit2's infinite retry loop
  - [ ] 14.6: Improve error message on auth failure — wrap the raw git2 error with: `"Authentication failed for remote '{remote_url}'. Configure credentials with one of:\n  - gh auth setup-git\n  - git config --global credential.helper osxkeychain\n  - Set GITHUB_TOKEN environment variable"`. Return as `TallyError::Git` with this message.
  - [ ] 14.7: Tests: sync with no credentials configured → error message contains `"Authentication failed"` AND `"gh auth setup-git"` (not raw `"class=Http (34); code=Auth (-16)"`)
  - [ ] 14.8: Tests: sync with `GITHUB_TOKEN=test_token` env var set → credentials callback returns `Cred::userpass_plaintext("git", "test_token")`
  - [ ] 14.9: Tests: auth callback invocation count ≤ 3 (one per strategy: credential_helper, env var, ssh agent) — verified via `Cell<u32>` counter accessible after sync attempt

- [ ] Task 15: Protect findings-data branch from deletion
  - [ ] 15.1: Document in README.md that repos using tally should add a branch protection rule (or ruleset) for `findings-data` with **"Restrict deletions"** enabled — no status checks or PR requirements needed, just prevent `git push --delete origin findings-data`
  - [ ] 15.2: Add `tally init` post-init message: "Tip: protect the findings-data branch from deletion — see README.md"
  - [ ] 15.3: For the `1898andCo/tally` repo itself and any repo that uses tally as an MCP server: create a GitHub ruleset via `gh api` or web UI targeting `findings-data` with deletion restriction only
  - [ ] 15.4: Add a `tally doctor` check (new subcommand or addition to `tally stats`) that warns if `findings-data` has no upstream tracking branch: "Warning: findings-data branch is not pushed to remote — findings are local-only. Run `tally sync` to push."

- [ ] Task 16: Schema version update
  - [ ] 16.1: Update `schema.json` version to `1.1.0` (minor — backward-compatible addition)
  - [ ] 16.2: Update `default_schema_version()` in `finding.rs` to return `"1.1.0"`
  - [ ] 16.3: Verify: v1.0.0 findings load correctly with v1.1.0 schema (serde defaults)

---

## Release Process

tally uses a tag-driven release pipeline. Here is the full process:

### Pre-Release Checklist

1. All CI checks pass on `develop` (Rustfmt, Clippy, Build, Test, Cargo Deny)
2. All new features have tests (unit + integration + CLI)
3. README.md is updated with new commands/tools
4. No uncommitted changes

### Release Steps

```bash
# 1. Ensure you're on develop with clean state
git checkout develop
git pull origin develop
git status  # must be clean

# 2. Run full CI locally
just ci

# 3. Prepare the release (updates Cargo.toml version, generates CHANGELOG.md, commits, tags)
just release 0.5.0

# 4. Push to remote (triggers release workflow)
git push origin develop --tags
```

### What `just release <version>` Does

1. Updates `version = "..."` in `Cargo.toml`
2. Runs `cargo check` to verify the version bump doesn't break anything
3. Generates full `CHANGELOG.md` via `git-cliff --config cliff.toml --tag "v<version>"`
4. Stages `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`
5. Creates commit: `chore(release): prepare v<version>`
6. Creates annotated tag: `v<version>`

### What the Release Workflow Does (`.github/workflows/release.yml`)

Triggered by pushing a `v*` tag:

1. **create-release** job:
   - Verifies tag version matches `Cargo.toml` version
   - Generates changelog for the release via `git-cliff`
   - Creates GitHub Release (draft=false, prerelease if tag contains `-`)

2. **build-binaries** job (matrix: linux x86_64, macOS aarch64, Windows x86_64):
   - Builds `cargo build --release --target <target>`
   - Strips debug symbols (Unix)
   - Packages as `.tar.gz` (Unix) or `.zip` (Windows)
   - Computes SHA-256 checksums
   - Uploads archives + checksums to the GitHub Release

3. **publish-crate** job (skipped for pre-releases):
   - Runs `cargo package --allow-dirty` to verify
   - Publishes to crates.io via `cargo publish --token $CARGO_REGISTRY_TOKEN`

4. **update-homebrew** job (skipped for pre-releases):
   - Downloads the source tarball and computes SHA-256
   - Generates updated `tally.rb` formula
   - Pushes to `1898andCo/homebrew-tap` via GitHub API (requires `HOMEBREW_TAP_TOKEN` secret)

5. **sync-main** job (skipped for pre-releases):
   - Fast-forward merges `develop` into `main`
   - Pushes `main` to remote

### Required Secrets

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | crates.io publish token |
| `HOMEBREW_TAP_TOKEN` | PAT with write access to `1898andCo/homebrew-tap` |
| `GITHUB_TOKEN` | Automatic — used for release creation and main sync |

### Installation Methods (Post-Release)

```bash
# crates.io
cargo install tally-ng

# cargo-binstall (prebuilt binaries)
cargo binstall tally-ng

# Homebrew
brew tap 1898andCo/tap && brew install tally

# Direct download
# Download from GitHub Releases: https://github.com/1898andCo/tally/releases
```

### Version Naming

- `0.x.0` — minor releases (new features, backward-compatible)
- `0.x.y` — patch releases (bug fixes)
- Tags with `-` (e.g., `v0.5.0-rc1`) — pre-releases (no crates.io, no Homebrew, no main sync)

---

## Dev Notes

### Technical Gotchas

1. **`serde(default)` for backward compatibility**: The `notes` and `edit_history` fields use `#[serde(default)]` so v0.4.0 finding files (which lack these fields) deserialize correctly. The `skip_serializing_if = "Vec::is_empty"` ensures we don't bloat JSON files when these fields are unused.

2. **Field edit atomicity**: `edit_field()` modifies the finding in memory, then `save_finding()` writes the entire file. If the process crashes between edits and save, no partial state is persisted. This is the same model as existing `update_finding_status`.

3. **Edit history uses `serde_json::Value`**: Old and new values are stored as generic JSON values because fields have heterogeneous types (String, Severity enum, Vec<String> for tags). This avoids a complex enum for each editable field type.

4. **Tag operations record `FieldEdit`**: Adding/removing tags creates a `FieldEdit` entry with the full old and new tag arrays. This maintains the audit trail for tag changes, consistent with how other field edits are tracked.

5. **State machine changes are minimal**: Only two transitions are added (`Deferred → Reopened`, `Suppressed → Reopened`). This is a strict superset of the existing state machine — no existing transitions are removed or modified. All 24 existing transition tests continue to pass.

6. **`update_finding` vs `update_finding_status`**: These are deliberately separate tools. `update_finding` edits content fields (description, title, etc.). `update_finding_status` transitions lifecycle state. They serve different purposes and have different validation logic. Keeping them separate follows the principle of single responsibility.

7. **Short ID resolution**: All new MCP tools and CLI commands accept both UUIDs and session-scoped short IDs (C1, I2, S3, TD4) via `SessionIdMapper::resolve()`. This is consistent with existing tools.

8. **No batch edit**: Unlike `record_batch`, there's no `update_batch` or `note_batch`. Field edits and notes are per-finding operations that require individual review. Batch operations can be composed by the calling agent if needed.

9. **git2 credential callbacks (sync auth)**: `git2::Repository` does not use the system git credential helper by default. `remote.fetch()` and `remote.push()` require explicit `RemoteCallbacks` with a `credentials` callback. Without this, HTTPS remotes fail with `ErrorCode::Auth (-16)`. The fix uses `git2::Cred::credential_helper()` which reads `credential.helper` from `~/.gitconfig` — this automatically supports osxkeychain, GitHub CLI (`gh auth setup-git`), and git-credential-store. libgit2 calls the credentials callback repeatedly on failure (infinite retry by default); the implementation tracks attempt count via `Cell<u32>` and returns error after exhausting all strategies. See `auth-git2-rs` crate for reference, and Cargo's `with_authentication` function in `cargo/sources/git/utils.rs` for the canonical implementation.

### Key Design Decisions

**Append-only edit history (not versioning)**
- Considered: full finding versioning with `Finding::version` counter
- Rejected: over-engineering for the use case. The edit history captures what changed and why. Full versioning would require a separate storage model (version chains, diffing) with minimal practical benefit. Agents don't need to "roll back" findings — they need to see what changed.

**Notes as a separate concept from edit history**
- Notes are free-text annotations that don't change any field
- Edit history is structured records of field changes
- Keeping them separate avoids confusion: "did this note change a field or not?"
- Both are append-only, both have timestamps and agent IDs

**`update_finding` accepts individual fields, not a full Finding**
- Accepting a full Finding object would require the caller to first fetch the complete finding, modify it, and send it back — error-prone and verbose
- Accepting individual fields means the caller only sends what they want to change
- The tool validates each field against the editable set

**SARIF export uses property bags for custom metadata**
- SARIF 2.1.0 has no native concept of "notes" or "edit history" on results — these are workflow concepts, not analysis concepts
- The `result.properties` bag is SARIF's formal extension mechanism (section 3.8) — tool-prefixed keys (`tally_notes`, `tally_editHistory`) are forward-compatible and safely ignored by consumers
- `result.fixes[]` is NOT appropriate for `suggested_fix` — SARIF fixes require concrete `artifactChanges` with code edits, not free-text suggestions
- `resultProvenance.firstDetectionTimeUtc` IS used for `created_at` — this is a standard SARIF field for temporal tracking
- Deep research (Mar 2026) confirmed: SonarQube, GitHub Code Scanning, Semgrep, and GCC all use property bags for tool-specific extensions

**Tags as first-class concept**
- Tags were already in the Finding struct but had no dedicated management tools
- `add_tag`/`remove_tag` are separate from `update_finding` because tag operations are additive/subtractive (merge/remove), not replacement-based
- The `query --tag` filter enables workflow patterns like "show all findings tagged story:1.21"

---

## Affected Files

### New Files

| File | Purpose |
|------|---------|
| `src/cli/update_fields.rs` | Handler for `update-fields` subcommand |
| `src/cli/note.rs` | Handler for `note` subcommand |
| `src/cli/tag.rs` | Handler for `tag` subcommand |

### Modified Files

| File | Changes |
|------|---------|
| `src/storage/git_store.rs` | Add `build_remote_callbacks()`, `build_fetch_options()`, `build_push_options()` helpers; update `sync()` to pass auth callbacks |
| `src/model/finding.rs` | Add `Note`, `FieldEdit` structs; add `notes`, `edit_history` fields to `Finding`; add `edit_field()`, `add_note()` methods |
| `src/model/state_machine.rs` | Add `Reopened` to `Deferred` and `Suppressed` allowed transitions |
| `src/model/mod.rs` | Re-export `Note`, `FieldEdit` |
| `src/mcp/server.rs` | Add `UpdateFindingInput`, `AddNoteInput`, `TagInput` input types; add 4 new `#[tool]` handlers; add `tag` param to `QueryFindingsInput` |
| `src/main.rs` | Add match arms for `UpdateFields`, `AddNote`, `ManageTags` commands in `run()` dispatch |
| `src/cli/mod.rs` | Add `UpdateFields`, `AddNote`, `ManageTags` variants to `Command` enum |
| `src/cli/import.rs` | Audit import handler for notes/edit_history serde defaults (no code change expected, add test) |
| `src/cli/update_fields.rs` | New file — handler for `update-fields` subcommand |
| `src/cli/note.rs` | New file — handler for `note` subcommand |
| `src/cli/tag.rs` | New file — handler for `tag` subcommand |
| `src/cli/query.rs` | Add `--tag` filter flag and filtering logic |
| `src/lib.rs` | Re-export new public types |
| `tests/model_test.rs` | Add tests for `edit_field()`, `add_note()`, field validation, edit history |
| `tests/mcp_test.rs` | Add tests for 4 new MCP tools |
| `tests/mcp_unit_test.rs` | Add unit tests for new tool input validation |
| `tests/cli_test.rs` | Add integration tests for `update-fields`, `note`, `tag` commands |
| `tests/e2e_lifecycle_test.rs` | Add deferred→reopened and suppressed→reopened workflow tests |
| `README.md` | Update CLI reference, MCP tools table, state transition matrix |
| `CONTRIBUTING.md` | Full rewrite — prerequisites, Git Flow, findings-data protection, commit conventions, PR process |
| `docs/story.md` | Add post-MVP enhancements section |

### New Files (Phase 0)

| File | Purpose |
|------|---------|
| `SOUL.md` | Project principles — 8 principles adapted from axiathon |
| `_bmad/` | BMAD framework v6.0.3 (installed via `npx bmad-method install`) |
| `_bmad-output/` | Planning and implementation artifacts |
| `.claude/settings.local.json` | Claude Code permissions for tally development |
| `scripts/bmad-post-update.sh` | Idempotent post-upgrade restore script |

### Replaced Files (Phase 0)

| File | Change |
|------|--------|
| `CLAUDE.md` | Expanded — adds SOUL.md ref, project state, on-demand references |
| `.claude/rules/rust.md` | Expanded — 27 → ~65 lines, adds type design, security comments, test patterns |
| `.claude/rules/git-commits.md` | Expanded — 10 → ~55 lines, adds full conventional commit spec with types table, scopes, breaking changes |

### Estimated Scope
- ~500 lines of new production code (data model, MCP tools, CLI commands, auth callbacks, doctor check)
- ~400 lines of new tests (model, MCP, CLI, e2e lifecycle, auth)
- ~200 lines of documentation updates (README, CONTRIBUTING, story.md)
- ~200 lines of foundation files (SOUL.md, expanded rules, CLAUDE.md)
- BMAD framework installation (vendor directory, not counted as project code)

---

## Deliverable: SOUL.md

```markdown
# SOUL.md — The Spirit of Tally

*Principles that guide how Claude should work in this project.*

These principles are adapted from the axiathon project's SOUL.md, filtered to the subset relevant for a single-crate CLI/MCP tool. Language-specific rules live in `.claude/rules/`.

---

## 1. Pragmatism Over Purity

Every rule in this document has a cost. Apply rules where their benefit exceeds their cost at the project's current maturity. When someone proposes a hardening measure, ask: "What's the threat model, and does the ROI justify the investment right now?"

This principle governs all others. When two principles conflict, the one with better ROI wins.

---

## 2. Make the Wrong Thing Impossible

Don't rely on discipline. Rely on design. If a state transition shouldn't happen, make the type system reject it. If IDs shouldn't be mixed up, make them distinct types.

**Push policy into the type system.** Humans forget. Toolchains don't.

- The state machine validates transitions at the type level — invalid transitions return errors, not silent success
- Content fingerprints are deterministic — deduplication is automatic, not manual
- UUID v7 provides stable identity — no sequential IDs that reset between sessions

---

## 3. The Spec Is More Important Than the Code

The code is disposable. The spec is the product. If you deleted every line of code and regenerated from `docs/story.md`, the result should be correct.

- Read the spec before writing code. It is the single source of truth.
- If the spec is wrong, fix the spec first, then implement. Never silently deviate.

**When fixing a bug, ask: "What spec gap allowed this?"** Then fix the right file:

| Gap type | Fix it in |
|----------|-----------|
| Universal principle | SOUL.md |
| Project-specific instruction | CLAUDE.md or `.claude/rules/` |
| Under-specified requirement | `docs/story.md` acceptance criteria |

---

## 4. Silent Failures Are the Enemy

The single most common review finding: **something can fail silently.** A git operation that errors without surfacing the problem. A finding save that succeeds but writes to the wrong branch. A deduplication check that skips because the index is stale.

- Git operations must surface errors, not swallow them
- Test assertions must fail explicitly when tools error
- A vacuously true test is worse than no test

---

## 5. Errors Are Domain Knowledge, Not Strings

Errors should be structured, semantic, and domain-specific. Not string bags.

- `TallyError::InvalidTransition { from, to, valid }` — the error tells you what's wrong AND what's right
- `TallyError::NotFound { uuid }` — not "error: not found"
- Use `thiserror` for error enums with structured variants

---

## 6. Explain Why, Not Just What

PR descriptions lead with motivation. Comments explain design constraints, not just behavior.

Good: `// One file per finding — git auto-merges new files without conflicts (JSONL has EOF contention)`
Bad: `// Save the finding`

---

## 7. Test Boundaries, Name Tests Like Documentation

Test names are executable documentation: `fingerprint_deterministic_for_same_input()` tells you the contract. Not `test_1()`.

Test boundaries explicitly: empty inputs, maximum lengths, invalid formats, concurrent access, edge cases in state transitions. Boundaries are where bugs live. See `.claude/rules/rust.md` for test conventions.

---

## 8. Standards Over Invention

When a standard exists and fits, adopt it. SARIF 2.1.0 for exports. UUID v7 for identity. JSON-RPC for MCP. SHA-256 for fingerprints. Conventional commits for git history.

When a standard doesn't exist, document why you're inventing.

---

## Current State (Mar 2026)

Single binary crate, v0.4.0. 255 tests, 90.6% coverage. Dual interface (CLI + MCP).
All 8 principles are actively applied. Update this section during releases.
```

## Deliverable: .claude/rules/rust.md

```markdown
# Rust Coding Rules

Project-specific Rust conventions for tally.

## Safety

- `#![forbid(unsafe_code)]` in main crate
- No `unwrap()` in production code — use `?` or `expect()` with actionable message
- No blocking the async runtime — use `spawn_blocking` for CPU-intensive work

## Type Design

- **Newtypes for distinct concepts:** UUIDs are `Uuid` type (not raw strings), severity is `Severity` enum (not strings)
- **Validated constructors at trust boundaries:** MCP inputs and CLI args are validated; internal code trusts validated types
- **`#[non_exhaustive]` on enums that will grow** — `LifecycleState`, `Severity`, `RelationshipType` all have it because new variants are expected. Don't add it to semantically closed enums (e.g., a boolean-like enum with only two variants).
- **Private fields with getters** on types where post-construction mutation would break invariants

## Error Handling

- Use `thiserror` for error enums — structured, semantic variants (not string bags)
- Define `pub type Result<T> = std::result::Result<T, TallyError>` for the crate
- `TallyError::InvalidTransition` includes `valid` targets — errors tell you what's right, not just what's wrong
- `Display` impl is for logging and CLI output — sanitize before sending to external systems
- Mark this with a `/// **SECURITY:** ...` comment on error types that could leak internal details

## Module Structure

```
src/
  lib.rs          # Public API re-exports only
  main.rs         # CLI entry point + MCP server mode
  error.rs        # Crate error types
  session.rs      # Session-scoped short ID mapper
  model/          # Data model (finding, identity, state machine)
  storage/        # Git-backed persistence
  cli/            # Clap CLI commands
  mcp/            # MCP server (rmcp)
```

## Dependencies

- Single crate (not a workspace)
- Edition 2024, MSRV 1.85+
- Key crates: `git2` (storage), `rmcp` (MCP server), `clap` (CLI), `serde`/`serde_json` (serialization), `uuid` (identity), `sha2` (fingerprints), `thiserror` (errors), `tokio` (async), `tracing` (observability)
- Use `cargo clippy -- -D warnings` — warnings are errors
- `clippy::unwrap_used = "deny"` in `Cargo.toml`

## Testing

- Unit: `#[cfg(test)] mod tests {}` in same file
- Integration: `tests/` directory, named by feature (`cli_test.rs`, `storage_test.rs`)
- Property: `tests/property_*.rs` with `proptest`
- E2E lifecycle: `tests/e2e_lifecycle_test.rs` for full workflows
- Test names as documentation: `deferred_can_transition_to_reopened()`, not `test_transition()`
- Test boundaries: empty inputs, invalid state transitions, concurrent git writes, malformed JSON
- Snapshot tests with `insta` where output format stability matters
```

## Deliverable: .claude/rules/git-commits.md

```markdown
# Git Commit Standards

All commits MUST follow [Conventional Commits](https://www.conventionalcommits.org/).

## Format

\`\`\`
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
\`\`\`

## Required: Type

| Type     | Purpose                      |
|----------|------------------------------|
| feat     | New feature (MINOR version)  |
| fix      | Bug fix (PATCH version)      |
| docs     | Documentation only           |
| style    | Code style (no logic change) |
| refactor | Neither fix nor feature      |
| perf     | Performance improvement      |
| test     | Adding/fixing tests          |
| build    | Build system/dependencies    |
| ci       | CI configuration             |
| chore    | Other non-src/test changes   |

## Required: Description

- Use imperative, present tense ("add" not "added")
- Do NOT capitalize the first letter
- Do NOT end with a period

## Optional: Scope

Enclose in parentheses after type: `feat(mcp): add update_finding tool`

Common scopes: `model`, `mcp`, `cli`, `storage`, `state`

## Optional: Body

- Separate from description with a blank line
- Explain motivation and contrast with previous behavior

## Optional: Footer

- `Refs: #123` — Issue references
- `Closes: #123` — Issues closed by commit
- `BREAKING CHANGE:` — Breaking change description

## Breaking Changes

Indicate with either:
1. `!` after type/scope: `feat(mcp)!: rename tool parameter`
2. Footer: `BREAKING CHANGE: tool parameter renamed from X to Y`

## AI Attribution

Do NOT include in commit messages:
- The "Generated with Claude Code" line
- The "Co-Authored-By: Claude" line
- Any other AI attribution

## GitHub Operations

### Admin bypass is never implicit

NEVER use `--admin` flag on `gh pr merge` or any other branch protection bypass unless the user explicitly grants permission for that specific merge in that moment. Prior permission does not carry forward — each use requires fresh explicit approval.
```

## Deliverable: CLAUDE.md

```markdown
# CLAUDE.md

@SOUL.md
@.claude/rules/_index.md

## Project State (Mar 2026)

Single binary crate (`tally-ng` on crates.io, binary name `tally`). v0.4.0.
255 tests, 90.6% coverage. Dual interface: CLI (clap) + MCP server (rmcp).
Git-backed storage on orphan `findings-data` branch via `git2`.

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

Git Flow — branch from `develop`, PRs target `develop`.

- Branch naming: `feature/desc`, `fix/desc`
- Conventional commits enforced by lefthook
- No AI attribution in commit messages

## Architecture

- Single binary crate (not a workspace)
- Git-backed storage on orphan `findings-data` branch via `git2`
- Dual interface: CLI (clap) + MCP server (rmcp)
- `#![forbid(unsafe_code)]`
- No `unwrap()` in production code

## On-Demand References

| Topic | Location |
|-------|----------|
| Original story spec | `docs/story.md` |
| Finding mutability story | `docs/story-finding-mutability.md` |
| Release process | `docs/story-finding-mutability.md` → "Release Process" section |
| MCP server config | `.mcp.json` |
```

## Deliverable: .claude/rules/_index.md

```markdown
## Claude Code Rules

@rust.md
@git-commits.md
```

## Deliverable: CONTRIBUTING.md

See the full file content in the repository at `CONTRIBUTING.md`. Key sections:

1. **Prerequisites** — tool table with versions and install commands, Homebrew Rust warning
2. **First-Time Setup** — 4 steps from clone to running tests (<10 minutes)
3. **Git Workflow** — Git Flow branch structure, `findings-data` orphan branch protection note
4. **Commit Messages** — Conventional Commits with tally-specific scopes (`model`, `mcp`, `cli`, `storage`, `state`, `session`)
5. **Code Quality** — pre-commit checks (lefthook), manual quality check commands, just recipes
6. **Pull Request Process** — 7-step checklist, reviewer expectations
7. **Project Structure** — annotated directory tree
8. **Testing** — test types, locations, conventions, coverage commands
9. **Release Process** — summary with link to full docs

## Deliverable: .claude/settings.local.json

```json
{
  "permissions": {
    "allow": [
      "Bash(gh pr list:*)",
      "Bash(gh pr view:*)",
      "Bash(gh pr:*)",
      "Bash(gh repo:*)",
      "Bash(git pull:*)",
      "Bash(git config:*)",
      "Bash(grep:*)",
      "Bash(find:*)",
      "Bash(wc:*)",
      "Bash(ls:*)",
      "Bash(cargo build:*)",
      "Bash(cargo check:*)",
      "Bash(cargo test:*)",
      "Bash(cargo clippy:*)",
      "Bash(cargo doc:*)",
      "Bash(cargo search:*)",
      "Bash(cargo +nightly fmt --all)",
      "Bash(cargo llvm-cov:*)",
      "mcp__perplexity__deep_research",
      "mcp__perplexity__reason",
      "mcp__perplexity__search",
      "mcp__tavily__tavily_research",
      "mcp__tavily__tavily_search",
      "mcp__context7__resolve-library-id",
      "mcp__context7__query-docs",
      "mcp__tally__query_findings",
      "mcp__tally__initialize_store",
      "mcp__tally__record_batch",
      "mcp__tally__get_finding_context",
      "mcp__tally__update_finding_status"
    ]
  },
  "enableAllProjectMcpServers": true,
  "enabledMcpjsonServers": [
    "perplexity",
    "tavily",
    "playwright",
    "tally"
  ]
}
```

Note: Removed axiathon-specific permissions (`WebFetch(domain:schema.ocsf.io)`, `for f` loop patterns). Added standard cargo/git/gh permissions. tally MCP tools are included so tally can track its own findings during development.
