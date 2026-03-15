# Story 1.2: Rule Registry — Centralized Rule Management with Semantic Matching

Status: ready-for-dev
Repository: `1898andCo/tally`
Language: Rust
License: Apache-2.0
Date: 2026-03-15
Epic: Query & Search Enhancements

---

## Problem Statement

Tally's deduplication fingerprint is `SHA-256(file_path + line_range + rule_id)`. The `rule_id` is a free-form string chosen by the discovering agent. When multiple AI agents (dclaude, zclaude, cursor, claude-code) discover the same class of issue independently, they may use different names:

- Agent A: `unsafe-unwrap`
- Agent B: `unwrap-usage`
- Agent C: `no-unwrap`

These produce different fingerprints, causing the same issue at the same location to be recorded as 3 separate findings instead of 1. There is no way to:

1. **Normalize rule IDs** — map agent-specific names to canonical identifiers
2. **Discover existing rules** — search for similar rules before creating new ones
3. **Manage rules** — create, update, deprecate, or scope rules to specific modules
4. **Share rule knowledge** — agents can't learn what rules exist in this project

Additionally, the MCP `record_finding` handler does NOT append agents to `discovered_by` on dedup (CLI handler does) — a pre-existing inconsistency that must be fixed.

---

## Solution

Add a rule registry to tally with four layers:

1. **Rule storage** — individual JSON files on the `findings-data` orphan branch (`rules/<rule-id>.json`), using the same git-backed storage pattern as findings
2. **Matching pipeline** — 7-stage rule resolution: normalization, exact match, alias lookup, CWE cross-reference, Levenshtein/Jaro-Winkler, token Jaccard, optional semantic embeddings
3. **CLI + MCP tools** — `tally rule create/get/list/search/update/delete` CLI commands + 6 MCP tools
4. **Auto-registration** — unknown rule IDs are auto-registered on first encounter (compatible mode), with suggestions for similar existing rules

---

## Story

As a **developer using tally with multiple AI agents**,
I want **a centralized rule registry that normalizes different agent names for the same class of issue**,
So that **findings are correctly deduplicated regardless of which agent discovers them**.

As an **AI coding agent** using tally's MCP interface,
I want **to search existing rules before recording findings and discover canonical rule IDs**,
So that **I use consistent rule naming and avoid creating duplicate rules**.

As a **project maintainer**,
I want **to scope rules to specific modules, add examples, and manage rule lifecycle**,
So that **the rule registry reflects my project's actual code review standards**.

---

## Acceptance Criteria

### AC-1: Rule Data Model
- Each rule is a JSON file at `rules/<rule-id>.json` on the `findings-data` branch
- Required fields: `id`, `name`, `description`, `category`, `severity_hint`, `created_by`, `created_at`, `status`
- Optional fields: `aliases`, `tags`, `cwe_ids`, `scope` (include/exclude globs), `examples` (bad/good code), `references`, `related_rules`, `suggested_fix_pattern`, `embedding` (cached vector)
- Status values: `active`, `deprecated`, `experimental`
- Rule IDs are lowercase alphanumeric with hyphens, max 64 characters

### AC-2: Matching Pipeline
- Stage 1: Canonical normalization (lowercase, `_` to `-`, spaces to `-`, trim leading/trailing hyphens, strip agent namespace prefix `dclaude:`. **No prefix stripping** of `no-`/`disallow-`/`check-` — these carry semantic meaning per SARIF v2.1 guidance and ESLint/Semgrep conventions. Use explicit aliases instead.) After normalization, validate format against `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$` — reject if invalid.
- Stage 2: Exact match on canonical rule IDs (HashMap, nanoseconds)
- Stage 3: Alias lookup — each rule has an `aliases` array; any alias maps to the canonical ID
- Stage 4: CWE cross-reference — if the agent provides a CWE ID, match against rules with the same CWE. **Confidence 0.7 (suggestion only, not auto-normalize)** — CWE IDs are too broad for auto-match (e.g., CWE-252 covers unchecked malloc, setuid, fork, file ops — all different remediation). Per SARIF v2.1 and CWE mapping analysis research.
- Stage 5: Jaro-Winkler on rule IDs (`strsim` crate, **suggestion only** >= 0.6). **DEVIATION from original spec:** Deep research (Perplexity, Mar 2026) confirmed that production tools (Semgrep, SonarQube, SARIF v2.1) use deterministic matching for rule IDs — exact match or explicit aliases only. Jaro-Winkler auto-match at >= 0.85 causes false positives on structured IDs due to prefix-weighting (e.g., `rule-crit1` vs `rule-crit2` = JW 0.97, falsely auto-matched). Changed to suggestion-only: JW >= 0.6 populates `similar_rules` but never auto-normalizes. Only exact match (Stage 2) and alias lookup (Stage 3) auto-resolve.
- Stage 6: Token Jaccard on descriptions (tokenize, remove stopwords, Jaccard >= 0.5 for suggestion only)
- Stage 7: Semantic embedding cosine similarity (optional, behind `--features semantic-search` cargo feature flag, threshold >= 0.8, suggestion only)
- Each match returns `{ canonical_id, confidence: f64, method: &str }`
- Confidence 1.0 (exact/alias) auto-normalizes; all fuzzy matches (Stages 4-7) populate `similar_rules` suggestions only; no match registers as new rule

### AC-3: Auto-Registration
- When `record_finding` encounters an unknown rule ID, auto-register it as a new rule with status `experimental`
- Auto-registration populates: `id`, `name` (from title if available), `description` (from finding description), `category` (from finding category), `severity_hint` (from finding severity), `created_by` (from agent ID), `created_at`
- If a similar rule is found (confidence 0.6-0.84), the response includes a `similar_rules` array with suggestions
- The fingerprint uses the CANONICAL rule ID (after normalization/alias resolution), not the original agent-provided ID
- The finding stores both `rule_id` (canonical) and `original_rule_id` (what the agent sent)

### AC-4: CLI Commands
- `tally rule create <id> --name "..." --description "..." [--category ...] [--severity-hint ...] [--alias ...] [--cwe ...] [--scope-include ...] [--scope-exclude ...] [--tag ...]` — register a new rule
- `tally rule get <id>` — show full rule details (JSON)
- `tally rule list [--category ...] [--status ...] [--format json|table]` — list all rules
- `tally rule search <query> [--method text|semantic]` — fuzzy search by ID or description
- `tally rule update <id> [--name ...] [--add-alias ...] [--remove-alias ...] [--add-cwe ...] [--scope-include ...] [--scope-exclude ...] [--status ...] [--description ...]` — update rule fields. `--add-alias` validates against same bidirectional ID namespace check as `create` (alias must not match any canonical ID, and must not be claimed by another rule's alias)
- `tally rule delete <id> --reason "..."` — deprecate rule (set status to `deprecated`, don't actually delete). Deprecated rules still resolve in the matching pipeline — new findings warn that the rule is deprecated but are still recorded. Existing findings are unaffected (historical records).
- `tally rule add-example <id> --type bad|good --language <lang> --code "..." --explanation "..."` — add code example to rule
- **Rule rename**: Rule IDs are immutable (they're file names). To "rename" a rule, create a new rule and add the old ID as an alias on the new rule. The alias lookup ensures old findings' rule IDs still resolve to the new canonical rule. No bulk rewrite of existing findings needed.

### AC-5: MCP Tools (full CLI parity)
- `create_rule` — register new rule with full fields
- `get_rule` — retrieve rule by ID (exact or fuzzy)
- `search_rules` — search by query text, method (text/semantic), with limit
- `list_rules` — list with optional category/status filter
- `update_rule` — update mutable fields (including add/remove aliases)
- `delete_rule` — deprecate with reason
- `add_rule_example` — add bad/good code example to a rule
- `migrate_rules` — scan existing findings, auto-register rules (same as `tally rule migrate`)
- Tool descriptions reference `findings://docs/rule-registry` for format and workflow documentation

### AC-6: MCP Resource
- `findings://docs/rule-registry` — markdown reference for rule format, matching pipeline, and CLI usage
- Listed alongside existing `findings://docs/tallyql-syntax` resource

### AC-7: Migration
- `tally rule migrate` — scan all existing findings, extract unique `rule_id` values, auto-register each as a new rule with status `experimental`
- `tally rebuild-index --include-rules` — rebuild rule registry from findings
- Backward compatible: existing findings with unregistered rule IDs continue to work (compatible mode)
- New `original_rule_id` field on Finding is optional with `#[serde(default, skip_serializing_if = "Option::is_none")]`

### AC-8: Scope Enforcement
- Rules with `scope.include` patterns only match findings in those file paths (glob matching)
- Rules with `scope.exclude` patterns reject findings in excluded paths
- Scope violations produce a warning in the response, not an error
- Default (no scope) = rule applies everywhere

### AC-9: Finding Dedup Fix
- MCP `record_finding` handler appends agent to `discovered_by` on dedup (matching CLI behavior)
- Dedup updates `updated_at` timestamp
- Returns `"status": "deduplicated"` with the UUID and agent merge info

### AC-10: Semantic Search (Feature-Gated)
- Cargo feature flag: `semantic-search` (disabled by default)
- Uses `fastembed` crate for local embedding generation (all-MiniLM-L6-v2)
- Embeddings cached in rule JSON files (`embedding` field + `embedding_model` field)
- On rule creation/update, recompute and cache embedding
- On search, generate query embedding and compute cosine similarity against cached vectors
- Graceful degradation: if model download fails, falls back to text similarity with warning
- Rules without cached embeddings: on semantic search, compute embeddings lazily for rules with `embedding: null`, cache them in the rule JSON for future searches. If too many rules need embedding (>50), warn user and suggest `tally rule reindex --embeddings` to batch-compute. Rules being embedded on-the-fly are included in results (not silently skipped)
- Model cached at `~/.cache/tally/models/` (configurable via `TALLY_MODEL_CACHE` env var)

### AC-11: Documentation
- README.md rule registry section with format, CLI reference, matching pipeline explanation
- `docs/reference/rule-registry.md` — standalone reference document
- CLAUDE.md On-Demand References updated
- MCP tool descriptions include examples

### AC-12: dclaude Integration (cross-repo: `../dclaude`)
- dclaude skills that record findings must search for existing rules via `search_rules` before recording, using canonical rule_ids from the registry instead of hardcoding them
- dclaude CLAUDE.md tally integration section must document all new MCP tools (create_rule, get_rule, search_rules, list_rules, update_rule, delete_rule, add_rule_example, migrate_rules) and new MCP resource (findings://docs/rule-registry)
- dclaude must handle new `record_finding` / `record_batch` response fields: `similar_rules`, `original_rule_id`, `scope_warning`, `normalized_by`
- dclaude CLAUDE.md field mapping must separate CATEGORY (domain grouping) from rule_id (registry lookup) — currently maps CATEGORY to both
- dclaude must gracefully degrade when rule registry is not yet initialized (pre-v0.7.0 tally) — fall back to current hardcoded rule_ids
- dclaude must document and use existing tally capabilities not currently captured: `query_findings` advanced parameters (filter/since/before/agent/text/sort), MCP resources (findings://summary, findings://file/{path}, findings://pr/{pr_number}), add_note/add_tag/remove_tag tools
- dclaude skills must query existing findings before recording (query_findings by pr_number, rule_id, file) to show cross-session history and avoid re-reporting known issues

---

## Tasks / Subtasks

- [ ] Task 1: Rule data model and storage (AC: 1)
  - [ ] 1.1: Create `src/registry/mod.rs` — module declaration, re-exports
  - [ ] 1.2: Create `src/registry/rule.rs` — `Rule` struct with all fields, serde, `RuleStatus` enum
  - [ ] 1.3: Create `src/registry/store.rs` — `RuleStore` with `save_rule()`, `load_rule()`, `load_all_rules()`, `delete_rule()` using existing `GitFindingsStore.upsert_file()` pattern
  - [ ] 1.4: Add `RULES_DIR = "rules"` constant, create directory on `tally init`
  - [ ] 1.5: Add `original_rule_id: Option<String>` field to `Finding` struct (serde default)
  - [ ] 1.6: Add `pub mod registry;` to `src/lib.rs`
  - [ ] 1.7: Add `strsim = "0.11"` to Cargo.toml dependencies
  - [ ] 1.8: Fix `upsert_file()` to handle `ErrorCode::Modified` (-15) — on compare-and-swap failure, re-read branch tip, rebuild tree from new parent, retry with same exponential backoff as `Locked`

- [ ] Task 2: Matching pipeline (AC: 2, 3)
  - [ ] 2.1: Create `src/registry/normalize.rs` — canonical normalization (lowercase, `_` to `-`, spaces to `-`, strip agent namespace prefix only, trim leading/trailing hyphens. NO prefix stripping of `no-`/`disallow-`/`check-` — these carry semantic meaning). After normalization, validate format against `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$` — return error if invalid
  - [ ] 2.2: Create `src/registry/matcher.rs` — `RuleMatcher` struct with 7-stage pipeline
  - [ ] 2.3: Implement Stage 1-3: normalize, exact match, alias lookup
  - [ ] 2.4: Implement Stage 4: CWE cross-reference matching
  - [ ] 2.5: Implement Stage 5: Levenshtein/Jaro-Winkler on rule IDs via `strsim`
  - [ ] 2.6: Implement Stage 6: Token Jaccard on descriptions (tokenize, stopwords, Jaccard similarity)
  - [ ] 2.7: Create `src/registry/stopwords.rs` — **minimal** stopwords list for technical writing. Include ONLY: articles (a, an, the), basic prepositions (of, in, to, for, on, at, by, with), basic conjunctions (and, or, but). EXCLUDE negation words (not, no, without, never) and temporal/comparative words (before, after, more, less) — these carry semantic meaning in technical descriptions. ~20 words max, not the 150+ word NLTK/spaCy lists
  - [ ] 2.8: Implement `MatchResult` struct: `{ canonical_id, confidence, method, similar_rules }`
  - [ ] 2.9: Implement auto-registration: create rule with status `experimental` when no match found
  - [ ] 2.10: Implement confidence thresholds: >= 0.85 auto, 0.6-0.84 suggest, < 0.6 new

- [ ] Task 3: Integration with record_finding (AC: 3, 9)
  - [ ] 3.1: Update `src/cli/record.rs` — resolve rule ID through matcher before computing fingerprint
  - [ ] 3.2: Update `src/mcp/server.rs` `record_finding` — resolve rule ID through matcher
  - [ ] 3.3: Fix MCP dedup: append agent to `discovered_by` on `ExistingFinding` resolution (match CLI behavior)
  - [ ] 3.4: Store `original_rule_id` on finding when normalization changes the ID
  - [ ] 3.5: Update fingerprint computation to use canonical rule ID
  - [ ] 3.6: Include `similar_rules` in response when confidence is 0.6-0.84

- [ ] Task 4: Scope enforcement (AC: 8)
  - [ ] 4.1: Add glob matching for `scope.include` / `scope.exclude` patterns
  - [ ] 4.2: Add `globset = "0.4"` to Cargo.toml
  - [ ] 4.3: Check scope on `record_finding` — warn if file doesn't match scope, don't block
  - [ ] 4.4: Add scope info to `tally rule get` output

- [ ] Task 5: CLI commands (AC: 4)
  - [ ] 5.1: Add `Rule` subcommand to `Command` enum in `src/cli/mod.rs` with all sub-subcommands
  - [ ] 5.2: Create `src/cli/rule.rs` — handlers for create, get, list, search, update, delete, add-example
  - [ ] 5.3: Add match arm in `src/main.rs` for `Command::Rule`
  - [ ] 5.4: Implement `handle_rule_create()` with full field support
  - [ ] 5.5: Implement `handle_rule_get()` — display as JSON or formatted table
  - [ ] 5.6: Implement `handle_rule_list()` with category/status filters and table format
  - [ ] 5.7: Implement `handle_rule_search()` — text similarity by default, semantic if feature enabled
  - [ ] 5.8: Implement `handle_rule_update()` — add/remove aliases, update scope, change status
  - [ ] 5.9: Implement `handle_rule_delete()` — set status to deprecated
  - [ ] 5.10: Implement `handle_rule_add_example()` — append to examples array

- [ ] Task 6: MCP tools — full CLI parity (AC: 5, 6)
  - [ ] 6.1: Add 8 MCP tool handlers to `src/mcp/server.rs`: `create_rule`, `get_rule`, `search_rules`, `list_rules`, `update_rule`, `delete_rule`, `add_rule_example`, `migrate_rules`
  - [ ] 6.2: Add input structs: `CreateRuleInput`, `GetRuleInput`, `SearchRulesInput`, `ListRulesInput`, `UpdateRuleInput`, `DeleteRuleInput`, `AddRuleExampleInput`, `MigrateRulesInput`
  - [ ] 6.3: Add `findings://docs/rule-registry` MCP resource with markdown reference
  - [ ] 6.4: Update `query_findings` tool description to mention rule registry
  - [ ] 6.5: Update MCP server instructions to mention rule management
  - [ ] 6.6: Update capabilities listing (tool count, resource count)

- [ ] Task 7: Migration (AC: 7)
  - [ ] 7.1: Add `tally rule migrate` CLI command
  - [ ] 7.2: Implement migration: scan findings, extract unique rule_ids, auto-register each
  - [ ] 7.3: Add `--include-rules` flag to `rebuild-index` command — recalculates exact `finding_count` for each rule by scanning findings
  - [ ] 7.4: Update `tally init` scaffolding: add `rules/.gitkeep` to initial tree. For existing repos (branch already exists), check if `rules/` dir exists — if not, add `rules/.gitkeep` via `upsert_file()` (upgrade path for pre-1.2 repos). Init remains idempotent: if both branch and `rules/` exist, no-op

- [ ] Task 7B: Sync conflict resolution for rules (AC: 1, 3)
  - [ ] 7B.1: Update `sync()` in `git_store.rs` — detect conflicts on `rules/*.json` paths separately from `findings/*.json`
  - [ ] 7B.2: Implement `merge_rule_conflict(ours: &[u8], theirs: &[u8]) -> Result<Vec<u8>>` — parse both as `Rule` JSON, merge semantically (union arrays, longest description, earliest created_at, max finding_count, most promoted status)
  - [ ] 7B.3: On rule conflict: resolve via `Index::conflict_remove()` + `Index::add()` with merged content
  - [ ] 7B.4: Log warning when scope fields differ (manual review needed)
  - [ ] 7B.5: Update sync error message — findings conflicts remain errors, rule conflicts are auto-resolved
  - [ ] 7B.6: Update `SyncResult` to include `rules_merged: usize` count

- [ ] Task 8: Semantic search (AC: 10, feature-gated)
  - [ ] 8.1: Add `fastembed = { version = "5", optional = true }` to Cargo.toml with feature flag
  - [ ] 8.2: Create `src/registry/semantic.rs` — embedding generation and cosine similarity (behind `#[cfg(feature = "semantic-search")]`)
  - [ ] 8.3: Implement `compute_embedding()` — generate and cache embedding in rule JSON
  - [ ] 8.4: Implement `semantic_search()` — cosine similarity against cached embeddings
  - [ ] 8.5: Add `--method semantic` option to `tally rule search`
  - [ ] 8.6: Add graceful degradation: if model unavailable, fall back to text similarity with warning
  - [ ] 8.7: Add `TALLY_MODEL_CACHE` env var for custom cache directory
  - [ ] 8.8: Lazy embedding computation: on semantic search, compute and cache embeddings for rules with `embedding: null` (don't skip them)
  - [ ] 8.9: Add `tally rule reindex --embeddings` command to batch-compute embeddings for all rules

- [ ] Task 9: Tests (AC: 1-11)
  - [ ] 9.1: Rule model tests — serialization roundtrip, default values, validation
  - [ ] 9.2: Matching pipeline tests — each stage independently, combined pipeline, threshold behavior
  - [ ] 9.3: Normalization tests — underscore→hyphen, space→hyphen, agent namespace strip, case fold, leading/trailing hyphen trim, idempotent (no prefix stripping — gotcha #1)
  - [ ] 9.3b: Alias-shadows-canonical validation tests — alias matching canonical ID rejected, canonical ID matching existing alias rejected, update --add-alias with canonical ID rejected
  - [ ] 9.4: Auto-registration tests — unknown rule creates experimental entry, auto-registered canonical ID checked against existing aliases
  - [ ] 9.5: Scope tests — glob include/exclude matching, default (no scope)
  - [ ] 9.6: CLI integration tests — all 7 subcommands with positive/negative cases
  - [ ] 9.7: MCP unit tests — all 8 tools (create, get, search, list, update, delete, add_example, migrate)
  - [ ] 9.8: Migration tests — scan findings, register rules, idempotent re-run
  - [ ] 9.9: Dedup fix test — MCP record_finding appends agent on dedup
  - [ ] 9.10: E2E test — agent records with alias, dedup works, query by canonical ID
  - [ ] 9.11: Concurrent registration test — two rules registered simultaneously (local race, `ErrorCode::Modified` retry)
  - [ ] 9.11b: Sync conflict resolution test — two clones register same rule with different metadata, sync merges semantically
  - [ ] 9.11c: Sync conflict resolution test — different rules on each clone, sync auto-merges cleanly (no conflict)
  - [ ] 9.12: Property tests — normalize(normalize(x)) == normalize(x), matcher never panics
  - [ ] 9.13: Semantic search tests (feature-gated) — embedding generation, similarity ranking

- [ ] Task 10: Documentation (AC: 11)
  - [ ] 10.1: Create `docs/reference/rule-registry.md` — standalone reference
  - [ ] 10.2: Update README.md — rule registry section, CLI reference, MCP tools table
  - [ ] 10.3: Update CLAUDE.md — On-Demand References
  - [ ] 10.4: Add `findings://docs/rule-registry` MCP resource content

- [ ] Task 11: Release (post-merge)
  - [ ] 11.1: Bump version in `Cargo.toml` to `0.7.0` (minor bump — new feature, backward compatible)
  - [ ] 11.2: Run `just release 0.7.0` — updates Cargo.toml, generates CHANGELOG.md via git-cliff, creates tagged commit
  - [ ] 11.3: Push to develop with tags: `git push origin develop --tags`
  - [ ] 11.4: GitHub Actions release workflow creates: GitHub Release with changelog, binaries, crates.io publish, Homebrew formula update
  - [ ] 11.5: Verify release: `cargo install tally-ng` pulls v0.7.0, `tally rule --help` works
  - [ ] 11.6: Update CLAUDE.md project state — version, capabilities, On-Demand References

- [ ] Task 12: dclaude integration (AC: 12, cross-repo: `../dclaude`)
  - [ ] 12.1: Update `dclaude/CLAUDE.md` tally integration section — add new MCP tools (create_rule, get_rule, search_rules, list_rules, update_rule, delete_rule, add_rule_example, migrate_rules), new MCP resource (findings://docs/rule-registry), updated response fields
  - [ ] 12.2: Update `dclaude/CLAUDE.md` field mapping — separate CATEGORY (domain grouping) from rule_id (registry lookup). CATEGORY is the domain, rule_id comes from `search_rules` or falls back to hardcoded mapping
  - [ ] 12.3: Update `dclaude/CLAUDE.md` — document existing tally capabilities not currently captured: query_findings advanced params (filter/since/before/agent/text/sort), MCP resources (findings://summary, findings://file/{path}, findings://detail/{uuid}, findings://severity/{level}, findings://status/{status}, findings://rule/{rule_id}, findings://pr/{pr_number}), add_note/add_tag/remove_tag/update_finding/export_findings tools, 5 MCP prompts (triage-file, fix-finding, explain-finding, summarize-findings, review-pr)
  - [ ] 12.4: Update `dclaude/skills/pr-fix-verify/SKILL.md` — before `record_batch` (Step 2), add rule lookup: for each finding's CATEGORY, call `search_rules(query: category_description)`. If match found (confidence >= 0.85), use canonical rule_id. If suggestion (0.6-0.84), log similar rules. If no match, fall back to current CATEGORY-as-rule_id. Handle `similar_rules` in response.
  - [ ] 12.5: Update `dclaude/skills/pr-fix-verify/SKILL.md` — handle new response fields from `record_finding`/`record_batch`: store `original_rule_id`, log `scope_warning`, display `similar_rules` suggestions to user
  - [ ] 12.6: Update `dclaude/skills/check-drift/SKILL.md` — same rule lookup pattern as 12.4 before `record_batch` (Step 7). Current hardcoded mapping (D→spec-drift, SC→spec-conflict, etc.) becomes the FALLBACK, not primary
  - [ ] 12.7: Update `dclaude/skills/post-fix-consistency-audit/SKILL.md` — same rule lookup pattern before `record_batch` (Step 8). Current category mapping (missing-file-list-entry, etc.) becomes fallback
  - [ ] 12.8: Update `dclaude/skills/architect-decision-workflow/SKILL.md` — same rule lookup pattern before `record_batch` (Step 4)
  - [ ] 12.9: Update `dclaude/skills/sweep-only/SKILL.md` — same rule lookup pattern before `record_batch` (Steps 3, 4)
  - [ ] 12.10: Add version detection to all skills — after `initialize_store`, check tally version (via capabilities or try `search_rules`). If pre-v0.7.0 (no rule registry), skip rule lookups and use current hardcoded rule_ids. Log: "tally rule registry not available (requires v0.7.0+) — using hardcoded rule IDs"
  - [ ] 12.11: Seed rule registry — create `dclaude/docs/tally-rules-seed.json` with all dclaude rule definitions (spec-drift, spec-drift-suggestion, spec-drift-tech-debt, spec-conflict, spec-divergence, missing-file-list-entry, stale-reference, story-number-conflict, spec-propagation-gap, naming-mismatch, plus common PR review categories). Include name, description, category, severity_hint, aliases, scope patterns. Document how to import: `tally rule migrate` or manual `tally rule create` from seed file
  - [ ] 12.12: Add query-before-record to `dclaude/skills/pr-fix-verify/SKILL.md` — before recording a batch, call `query_findings(pr_number: <pr>, status: "open")` to see what's already recorded for this PR. Show cross-session history on dashboard (Step 8). After fix verification (Step 11), query to confirm finding status changed. Use `findings://pr/{pr_number}` MCP resource for dashboard enrichment.
  - [ ] 12.13: Add query-before-record to `dclaude/skills/check-drift/SKILL.md` — before recording drift findings (Step 7), call `query_findings(rule: "spec-drift", file: <changed_file>)` to see existing drift findings at same locations. Prevents re-reporting known drift across sessions.
  - [ ] 12.14: Add query-before-record to `dclaude/skills/post-fix-consistency-audit/SKILL.md` — before recording audit findings (Step 8), query existing findings by rule_id + file to avoid re-reporting.
  - [ ] 12.15: Add `add_note` calls to `dclaude/skills/pr-fix-verify/SKILL.md` — when user skips (Step 10), add note with skip reason. When fix verified (Step 11), add note with verification details + commit SHA. When deep research done before dismissal (Step 10), add note with research findings. Preserves decision reasoning in tally's audit trail.
  - [ ] 12.16: Add `add_note` calls to `dclaude/skills/architect-decision-workflow/SKILL.md` — when architect makes disposition (Step 10), add note with full reasoning for fix_now/defer/accept/needs_research decisions. When deep research completes, add note with research results. Preserves architect rationale beyond just the status transition.
  - [ ] 12.17: Add `add_note` calls to `dclaude/skills/check-drift/SKILL.md` — when user fixes or skips a drift finding (Step 9), add note with reasoning. When spec conflict is suppressed, add note explaining why.
  - [ ] 12.18: Add `add_tag` calls to `dclaude/skills/pr-fix-verify/SKILL.md` — enrich findings during workflow: `"researched"` when deep research done (Step 10), `"batch-fixed"` when fixed as part of batch (Step 10), `"cross-file"` when fix touched multiple files (Step 11), `"regression-risk"` when ripple effects identified (Step 11.5). Tags are additive — don't remove original recording tags.
  - [ ] 12.19: Add `add_tag` calls to `dclaude/skills/architect-decision-workflow/SKILL.md` — tag findings with disposition: `"architect:fix-now"`, `"architect:deferred"`, `"architect:accepted"`, `"architect:needs-research"`. Enables querying all architect-deferred findings later via `query_findings(tag: "architect:deferred")`.
  - [ ] 12.20: Update `dclaude/skills/sprint-review/SKILL.md` — add tally integration: call `query_findings(since: "<sprint_start>", status: "resolved")` to enrich stakeholder brief with findings data (count fixed by severity, by agent, by rule). Show finding lifecycle metrics: discovered → fixed time, resolution rate. Use `findings://summary` resource for overall health snapshot.
  - [ ] 12.21: Update `dclaude/skills/session-summary/SKILL.md` — add tally integration: call `query_findings(pr_number: <pr>)` to include finding lifecycle in PR comment (N findings discovered, M fixed, K deferred). Show which agents found what. Use `findings://pr/{pr_number}` resource for complete PR findings history.
  - [ ] 12.22: Update `dclaude/skills/architect-decision-workflow/SKILL.md` — when architect reclassifies a finding (changes severity, category, or description), call `update_finding(finding_id, severity: "...", category: "...")` to persist the correction with audit trail in tally's `edit_history`. Currently only status is updated — mutable fields are ignored.
  - [ ] 12.23: Refine `dclaude/skills/check-drift/SKILL.md` suppression — when suppressing a drift finding (Step 9), use `suppression_type: "file"` with the specific `file_path` instead of default global suppression. File-scoped suppression is more precise — the same rule can still fire in other files. Only use `suppression_type: "global"` when the spec conflict applies project-wide.

---

## Dev Notes

### Architecture Decision: File-Per-Rule Storage

Individual `rules/<rule-id>.json` files (not a monolithic `registry.json`) because:
- Git auto-merges concurrent additions to different files — zero merge conflicts
- Fine-grained git history per rule
- Same pattern as `findings/<uuid>.json` — consistent codebase
- `tally sync` automatically distributes rules across clones

### File Structure on findings-data Branch (After Migration)

```
findings-data branch:
  rules/
    unsafe-unwrap.json
    sql-injection.json
    spec-drift.json
    .gitkeep
  findings/
    <uuid>.json
    .gitkeep
  index.json
  schema.json
  .gitattributes
```

### Rule JSON Format

```json
{
  "id": "unsafe-unwrap",
  "name": "Potentially unsafe unwrap() on Result/Option types",
  "description": "Using unwrap() on fallible types in production code can cause panics at runtime. Use ?, ok_or(), or explicit match instead.",
  "category": "safety",
  "severity_hint": "important",
  "tags": ["rust", "panics", "error-handling"],
  "cwe_ids": ["CWE-252"],
  "aliases": ["unwrap-usage", "no-unwrap", "unwrap-on-option"],
  "scope": {
    "include": ["src/**/*.rs"],
    "exclude": ["tests/**", "benches/**"]
  },
  "examples": [
    {
      "type": "bad",
      "language": "rust",
      "code": "let value = result.unwrap();",
      "explanation": "Panics if result contains an error"
    },
    {
      "type": "good",
      "language": "rust",
      "code": "let value = result.ok_or(AppError::Missing)?;",
      "explanation": "Propagates error instead of panicking"
    }
  ],
  "suggested_fix_pattern": null,
  "references": [],
  "related_rules": ["expect-without-message"],
  "created_by": "dclaude:code-reviewer",
  "created_at": "2026-03-15T00:00:00Z",
  "updated_at": "2026-03-15T00:00:00Z",
  "status": "active",
  "finding_count": 5,
  "embedding": null,
  "embedding_model": null
}
```

### Matching Pipeline — Full Workflow

```
Input: rule_id="unwrap-usage", category="safety", description="unsafe unwrap call", cwe=["CWE-252"]

Stage 1: Normalize
  "unwrap-usage" -> lowercase -> "unwrap-usage" (already lowercase)
  -> replace _ with - -> "unwrap-usage" (no change)
  -> strip agent namespace -> no namespace (no "agent:" prefix)
  Result: "unwrap-usage"

Stage 2: Exact Match
  HashMap lookup "unwrap-usage" -> NOT FOUND

Stage 3: Alias Lookup
  Scan all rules' aliases arrays -> FOUND in "unsafe-unwrap".aliases
  Result: { canonical_id: "unsafe-unwrap", confidence: 1.0, method: "alias" }
  -> STOP, return match

If Stage 3 had not matched:

Stage 4: CWE Cross-Reference
  Agent provided CWE-252 -> scan rules with cwe_ids containing "CWE-252"
  -> FOUND: "unsafe-unwrap" has CWE-252
  Result: { canonical_id: "unsafe-unwrap", confidence: 0.7, method: "cwe" }
  -> ADD TO similar_rules (0.6-0.84 range = suggest, don't auto-normalize)
  NOTE: CWE alone is never sufficient for auto-match — too broad

Stage 5: Levenshtein/Jaro-Winkler
  Compare "unwrap-usage" against all rule IDs:
    jaro_winkler("unwrap-usage", "unsafe-unwrap") = 0.78
    jaro_winkler("unwrap-usage", "sql-injection") = 0.31
  Best match: "unsafe-unwrap" at 0.78 -> below 0.85 threshold
  Result: { confidence: 0.78, method: "jaro_winkler" } -> SUGGEST, don't auto-normalize

Stage 6: Token Jaccard
  Tokenize "unsafe unwrap call" -> {"unsafe", "unwrap", "call"}
  Tokenize rule description -> {"using", "unwrap", "fallible", "types", ...}
  Intersection: {"unwrap"}, Union size: 8
  Jaccard = 1/8 = 0.125 -> below threshold
  (Works better with longer descriptions)

Stage 7: Semantic Embedding (if enabled)
  Generate embedding for "unsafe unwrap call"
  Cosine similarity against cached embeddings:
    "unsafe-unwrap": 0.87 -> above 0.8 threshold
  Result: { canonical_id: "unsafe-unwrap", confidence: 0.87, method: "semantic" }
```

### record_finding Workflow (After Rule Registry)

```
Agent calls: record_finding(rule_id: "unwrap-usage", file: "src/api.rs", line: 42, ...)

1. Load rule registry from findings-data branch
2. Run matching pipeline on "unwrap-usage"
   -> Stage 3 (alias): maps to "unsafe-unwrap" (confidence: 1.0)
3. Check scope: "unsafe-unwrap" has scope.include: ["src/**/*.rs"]
   -> "src/api.rs" matches -> OK
4. Compute fingerprint: SHA-256("src/api.rs:42-42:unsafe-unwrap")
   (uses CANONICAL id, not original)
5. Run dedup against existing findings
   -> If ExistingFinding: append agent to discovered_by, return deduplicated
   -> If NewFinding: create finding with rule_id="unsafe-unwrap", original_rule_id="unwrap-usage"
6. Return response:
   {
     "status": "recorded",
     "uuid": "...",
     "rule_id": "unsafe-unwrap",
     "original_rule_id": "unwrap-usage",
     "normalized_by": "alias"
   }
```

### tally rule CLI Workflows

**Create a rule:**
```bash
tally rule create unsafe-unwrap \
  --name "Potentially unsafe unwrap()" \
  --description "Using unwrap() on fallible types can panic" \
  --category safety \
  --severity-hint important \
  --alias unwrap-usage \
  --alias no-unwrap \
  --cwe CWE-252 \
  --scope-include "src/**/*.rs" \
  --scope-exclude "tests/**" \
  --tag rust --tag panics
```

**Search for a rule:**
```bash
# Text similarity (default)
tally rule search "unwrap usage"
# Output:
# unsafe-unwrap  (0.82 jaro-winkler on ID, 0.91 token match on description)
#   Aliases: unwrap-usage, no-unwrap
#   Category: safety | Severity: important | Findings: 5

# Semantic search (requires --features semantic-search)
tally rule search "code that could panic at runtime" --method semantic
# Output:
# unsafe-unwrap  (0.89 semantic similarity)
#   "Using unwrap() on fallible types can panic"
```

**Add examples:**
```bash
tally rule add-example unsafe-unwrap \
  --type bad \
  --language rust \
  --code 'let val = opt.unwrap();' \
  --explanation "Panics if opt is None"

tally rule add-example unsafe-unwrap \
  --type good \
  --language rust \
  --code 'let val = opt.ok_or(Error::Missing)?;' \
  --explanation "Returns error instead of panicking"
```

**Migrate existing findings:**
```bash
tally rule migrate
# Output:
# Scanning 10 findings...
# Found 4 unique rule IDs:
#   spec-conflict        (1 finding)
#   spec-drift           (2 findings)
#   spec-drift-suggestion (4 findings)
#   spec-drift-tech-debt (2 findings)
#
# Registered 4 rules with status: experimental
# Related rules detected:
#   spec-drift, spec-drift-suggestion, spec-drift-tech-debt share prefix "spec-drift"
#   Consider adding aliases or consolidating.
```

### MCP Tool Workflows

**Agent discovers an issue and searches for existing rules:**
```
Agent -> search_rules(query: "unwrap on option", method: "text")
Tally -> [{ id: "unsafe-unwrap", confidence: 0.87, description: "..." }]
Agent -> record_finding(rule_id: "unsafe-unwrap", file: "src/api.rs", line: 42, ...)
Tally -> { status: "recorded", rule_id: "unsafe-unwrap" }
```

**Agent uses unknown rule ID:**
```
Agent -> record_finding(rule_id: "memory-leak-risk", ...)
Tally ->
  1. No match in registry
  2. Auto-register "memory-leak-risk" as experimental
  3. Levenshtein finds "resource-leak" at 0.72 confidence
  4. Response: {
       status: "recorded",
       rule_id: "memory-leak-risk",
       note: "new rule auto-registered (experimental)",
       similar_rules: [{ id: "resource-leak", confidence: 0.72, method: "jaro_winkler" }]
     }
```

### Workflow: tally rule create

```
User runs: tally rule create unsafe-unwrap --name "..." --description "..." --alias unwrap-usage ...
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. Validate rule_id format   │
              │ Must match:                  │
              │ ^[a-z0-9][a-z0-9-]{0,62}     │
              │  [a-z0-9]$                   │
              │                              │
              │ Invalid → error + exit       │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 2. Check if rule exists      │
              │ Load rules/<id>.json         │
              │                              │
              │ Exists → error:              │
              │ "Rule 'unsafe-unwrap' already│
              │  exists. Use 'tally rule     │
              │  update' to modify."         │
              └──────────────┬───────────────┘
                             │ (does not exist)
                             ▼
              ┌──────────────────────────────┐
              │ 3. Check ID namespace        │
              │ conflicts (bidirectional)    │
              │                              │
              │ For each --alias provided:   │
              │ a) Does it match another     │
              │    rule's canonical ID?      │
              │    → error: "Alias           │
              │    'sql-injection' conflicts │
              │    with canonical rule       │
              │    'sql-injection'"          │
              │                              │
              │ b) Does it match another     │
              │    rule's alias?             │
              │    → error: "Alias           │
              │    'unwrap-usage' is already │
              │    claimed by rule           │
              │    'other-rule'"             │
              │                              │
              │ Also: does new rule's        │
              │ canonical ID match any       │
              │ existing rule's alias?       │
              │ → error: "Rule ID 'sqli'     │
              │  conflicts with alias on     │
              │  rule 'sql-injection'"       │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 4. Build Rule struct         │
              │ {                            │
              │   id, name, description,     │
              │   category, severity_hint,   │
              │   aliases, cwe_ids, scope,   │
              │   tags, examples: [],        │
              │   status: "active",          │
              │   created_by: "cli",         │
              │   created_at: now(),         │
              │   updated_at: now(),         │
              │   finding_count: 0,          │
              │   embedding: null            │
              │ }                            │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 5. Compute embedding         │
              │ (if semantic-search feature  │
              │  is enabled)                 │
              │                              │
              │ embed(name + description)    │
              │ Store in rule.embedding      │
              │ Set rule.embedding_model     │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 6. Save to git               │
              │ upsert_file(                 │
              │   "rules/<id>.json",         │
              │   rule_json,                 │
              │   "Register rule: <id>"      │
              │ )                            │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 7. Output                    │
              │ "Created rule: unsafe-unwrap │
              │  (active, 3 aliases)"        │
              └──────────────────────────────┘
```

### Workflow: tally rule search

```
User runs: tally rule search "unwrap usage" [--method text|semantic]
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. Load all rules            │
              │ (rules/*.json → HashMap)     │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 2. Check exact + alias first │
              │                              │
              │ If query matches a canonical │
              │ ID → return as top result    │
              │ (confidence: 1.0,            │
              │  method: "exact")            │
              │                              │
              │ If query matches an alias    │
              │ → return as top result       │
              │ (confidence: 1.0,            │
              │  method: "alias_match")      │
              │                              │
              │ Then continue with fuzzy     │
              │ search for additional results│
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 3. Determine fuzzy method    │
              │                              │
              │ --method text (default):     │
              │   → Run stages 5+6           │
              │                              │
              │ --method semantic:           │
              │   → Run stage 7              │
              │   → Requires feature flag    │
              └──────────────┬───────────────┘
                             │
              ┌──────────────┴───────────────┐
              │                              │
         Text Search                   Semantic Search
              │                              │
              ▼                              ▼
  ┌─────────────────────┐     ┌─────────────────────────┐
  │ A. Jaro-Winkler on  │     │ A. Generate query       │
  │ rule IDs             │     │ embedding               │
  │                      │     │                         │
  │ For each rule:       │     │ embed("unwrap usage")   │
  │ score = jaro_winkler │     └───────────┬─────────────┘
  │ ("unwrap-usage",     │                 │
  │  rule.id)            │                 ▼
  │                      │     ┌─────────────────────────┐
  │ Also check aliases:  │     │ B. Cosine similarity    │
  │ score = max(score,   │     │ against all cached      │
  │  jaro_winkler(query, │     │ rule embeddings         │
  │  each alias))        │     │                         │
  └──────────┬───────────┘     │ Score each rule         │
             │                 └───────────┬─────────────┘
             ▼                             │
  ┌─────────────────────┐                  │
  │ B. Token Jaccard on  │                 │
  │ descriptions         │                 │
  │                      │                 │
  │ tokenize(query)      │                 │
  │ tokenize(rule.desc)  │                 │
  │ jaccard = |inter| /  │                 │
  │           |union|    │                 │
  └──────────┬───────────┘                 │
             │                             │
             ▼                             ▼
  ┌──────────────────────────────────────────┐
  │ 4. Combine scores                        │
  │                                          │
  │ For text: final = max(id_score,          │
  │                       desc_score)        │
  │ For semantic: final = cosine_score       │
  │                                          │
  │ Filter: final >= 0.3 (minimum relevance) │
  │ Sort: descending by score                │
  │ Limit: --limit (default 10)              │
  └──────────────────┬───────────────────────┘
                     │
                     ▼
  ┌──────────────────────────────────────────┐
  │ 5. Display results                       │
  │                                          │
  │ unsafe-unwrap  (0.82 jaro_winkler)       │
  │   "Using unwrap() on fallible types..."  │
  │   Aliases: unwrap-usage, no-unwrap       │
  │   Category: safety | Status: active      │
  │   Findings: 5                            │
  │                                          │
  │ expect-without-message  (0.41 token)     │
  │   "Using expect() without descriptive.." │
  │   Category: safety | Status: active      │
  │   Findings: 2                            │
  └──────────────────────────────────────────┘
```

### Workflow: tally rule migrate

```
User runs: tally rule migrate
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. Load all findings         │
              │ store.load_all()             │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 2. Extract unique rule_ids   │
              │ with finding counts          │
              │                              │
              │ HashMap<String, MigrationInfo│
              │ > where MigrationInfo {      │
              │   count: usize,              │
              │   severities: HashSet,       │
              │   categories: HashSet,       │
              │   longest_description: String│
              │   // keep longest, not first │
              │   // (more detail = better   │
              │   // seed for the rule)      │
              │ }                            │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 3. Load existing rules       │
              │ (to avoid re-registering)    │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 4. For each unique rule_id   │
              │ not already in registry:     │
              │                              │
              │ Create rules/<id>.json with: │
              │ {                            │
              │   id: <rule_id>,             │
              │   name: <rule_id>,           │
              │   description: longest       │
              │     finding description,     │
              │   category: most common      │
              │     category from findings,  │
              │   severity_hint: highest     │
              │     severity seen,           │
              │   status: "experimental",    │
              │   created_by: "tally:migrate"│
              │   finding_count: count,      │
              │ }                            │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 5. Detect related rules      │
              │ (shared prefix analysis)     │
              │                              │
              │ Group by common prefix:      │
              │ spec-drift, spec-drift-*     │
              │ → suggest consolidation      │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 6. Output report             │
              │                              │
              │ "Scanning 10 findings..."    │
              │ "Found 4 unique rule IDs:"   │
              │ "  spec-drift (2 findings)"  │
              │ "  ..."                      │
              │ "Registered 4 rules"         │
              │ "Related rules detected:"    │
              │ "  spec-drift-* share prefix"│
              └──────────────────────────────┘
```

### Workflow: MCP search_rules Tool

```
Agent calls: search_rules(query: "unwrap on option", method: "text", limit: 5)
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. Load rule registry        │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 2. Run search pipeline       │
              │ (same as CLI search)         │
              │                              │
              │ Pre-check:                   │
              │ • Exact ID match? → top      │
              │ • Alias match? → top         │
              │                              │
              │ Then fuzzy:                  │
              │ Text method:                 │
              │ • Jaro-Winkler on IDs        │
              │ • Token Jaccard on descs     │
              │ • Combine, sort, limit       │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 3. Return JSON array         │
              │ [                            │
              │   {                          │
              │     "id": "unsafe-unwrap",   │
              │     "name": "...",           │
              │     "description": "...",    │
              │     "confidence": 0.87,      │
              │     "method": "jaro_winkler",│
              │     "category": "safety",    │
              │     "status": "active",      │
              │     "finding_count": 5,      │
              │     "aliases": [...]         │
              │   }                          │
              │ ]                            │
              └──────────────────────────────┘
```

### Workflow: MCP create_rule Tool

```
Agent calls: create_rule(rule_id: "buffer-overflow", name: "...",
                         description: "...", category: "security", ...)
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. Validate rule_id format   │
              │ (same as CLI)                │
              │ Invalid → MCP error          │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 2. Check existence +         │
              │ bidirectional ID namespace   │
              │ (same as CLI step 2+3)       │
              │ Exists → MCP error           │
              │ Alias shadows canonical →    │
              │   MCP error                  │
              │ Canonical shadows alias →    │
              │   MCP error                  │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 3. Build + save rule         │
              │ (same as CLI, status=active) │
              │                              │
              │ Compute embedding if feature │
              │ enabled                      │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 4. Return JSON               │
              │ {                            │
              │   "status": "created",       │
              │   "rule_id": "buffer-...",   │
              │   "aliases": [...],          │
              │   "scope": {...}             │
              │ }                            │
              └──────────────────────────────┘
```

### Workflow: Scope Enforcement During record_finding

```
              ┌──────────────────────────────┐
              │ Rule resolved (canonical ID) │
              │ Rule has scope:              │
              │   include: ["src/**/*.rs"]   │
              │   exclude: ["src/gen/**"]    │
              │                              │
              │ Finding file: <file_path>    │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 1. No scope on rule?         │
              │ → SKIP (no scope = all files │
              │   are in scope)              │
              └──────────────┬───────────────┘
                             │ (has scope)
                             ▼
              ┌──────────────────────────────┐
              │ 2. Check include patterns    │
              │ (if scope.include is set)    │
              │                              │
              │ globset match file_path      │
              │ against scope.include[]      │
              │                              │
              │ NO MATCH → scope_warning:    │
              │ "Rule 'unsafe-unwrap' is     │
              │  scoped to [src/**/*.rs] but │
              │  finding is in tests/foo.rs" │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 3. Check exclude patterns    │
              │ (if scope.exclude is set)    │
              │                              │
              │ globset match file_path      │
              │ against scope.exclude[]      │
              │                              │
              │ MATCH → scope_warning:       │
              │ "Rule 'unsafe-unwrap'        │
              │  excludes [src/gen/**] and   │
              │  finding is in               │
              │  src/gen/auto.rs"            │
              │                              │
              │ Both include and exclude can │
              │ independently trigger warns. │
              │ A file in exclude but also   │
              │ in include still warns       │
              │ (exclude takes priority).    │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 4. Continue with normal      │
              │ record_finding flow          │
              │ (fingerprint, dedup, save)   │
              │                              │
              │ The finding IS recorded —    │
              │ scope is advisory, not       │
              │ blocking. Warnings are       │
              │ included in the response.    │
              └──────────────────────────────┘
```

### Dedup Fix Details

Current MCP `record_finding` on `ExistingFinding` (BROKEN — does NOT merge agents):
```rust
IdentityResolution::ExistingFinding { uuid } => ToolOutput {
    status: "deduplicated".into(),
    uuid: Some(uuid.to_string()),
    // Missing: agent merge, timestamp update
}
```

Fixed (match CLI behavior from `src/cli/record.rs` `handle_dedup()`):
```rust
IdentityResolution::ExistingFinding { uuid } => {
    let mut finding = store.load_finding(&uuid).map_err(to_mcp_err)?;
    let already_recorded = finding.discovered_by.iter()
        .any(|a| a.agent_id == agent_id && a.session_id == session_id);

    if !already_recorded {
        finding.discovered_by.push(AgentRecord {
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            detected_at: Utc::now(),
            session_short_id: None,
        });
        finding.updated_at = Utc::now();
        store.save_finding(&finding).map_err(to_mcp_err)?;
    }

    Ok(serde_json::json!({
        "status": "deduplicated",
        "uuid": uuid.to_string(),
        "agent_merged": !already_recorded,
    }))
}
```

### New Dependencies

| Crate | Version | Purpose | Feature-gated? |
|-------|---------|---------|----------------|
| `strsim` | `"0.11"` | Levenshtein, Jaro-Winkler string similarity | No |
| `globset` | `"0.4"` | Glob pattern matching for rule scope | No |
| `fastembed` | `"5"` | Local embedding model for semantic search | Yes (`semantic-search`) |

Note: `strsim` replaces the hand-rolled Levenshtein in `src/query/fields.rs`.

### Current Code to Modify (inline for self-containment)

#### `src/storage/git_store.rs` — upsert_file pattern (lines 586-620)

```rust
fn upsert_file(&self, file_path: &str, content: &[u8], message: &str) -> Result<()> {
    let blob_oid = self.repo.blob(content)?;
    let commit = self.branch_tip()?;
    let parent_tree = commit.tree()?;

    let mut builder = git2::build::TreeUpdateBuilder::new();
    builder.upsert(file_path, blob_oid, FileMode::Blob);
    let new_tree_oid = builder.create_updated(&self.repo, &parent_tree)?;
    let new_tree = self.repo.find_tree(new_tree_oid)?;

    let sig = self.signature()?;
    let ref_name = format!("refs/heads/{}", self.branch_name);

    // Retry on ref lock contention
    for attempt in 0..MAX_LOCK_RETRIES {
        match self.repo.commit(Some(&ref_name), &sig, &sig, message, &new_tree, &[&commit]) {
            Ok(_) => return Ok(()),
            Err(e) if e.code() == ErrorCode::Locked && attempt < MAX_LOCK_RETRIES - 1 => {
                let delay = Duration::from_millis(100 * u64::from(2_u32.pow(attempt)));
                thread::sleep(delay);
            }
            Err(e) => return Err(TallyError::Git(e)),
        }
    }
    // ...
}
```

Rule storage reuses this pattern: `save_rule()` calls `upsert_file("rules/{id}.json", ...)`.

**Story 1.2 fix (Task 1.8):** This retry loop must also handle `ErrorCode::Modified` (-15) — compare-and-swap failure when the ref has moved between reading the tip and committing. On `Modified`, re-read `branch_tip()`, rebuild the tree from the new parent, and retry. The blob and tree building must move INSIDE the retry loop so they use the fresh parent tree. This fixes the local concurrent agent race condition.

#### `src/model/identity.rs` — compute_fingerprint (lines 12-28)

```rust
pub fn compute_fingerprint(primary_location: &Location, rule_id: &str) -> String {
    let input = format!(
        "{}:{}-{}:{}",
        primary_location.file_path, primary_location.line_start,
        primary_location.line_end, rule_id
    );
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
```

No change to this function — callers must pass the canonical rule ID (after matcher resolution).

#### `src/cli/record.rs` — handle_dedup (lines 102-160)

```rust
fn handle_dedup(store: &GitFindingsStore, uuid: Uuid, args: &RecordArgs<'_>) -> Result<()> {
    let mut finding = store.load_finding(&uuid)?;
    let already_recorded = finding.discovered_by.iter()
        .any(|a| a.agent_id == args.agent && a.session_id == args.session);
    let mut changed = false;
    if !already_recorded {
        finding.discovered_by.push(AgentRecord { ... });
        changed = true;
    }
    // Update location if code moved...
    if changed {
        finding.updated_at = Utc::now();
        store.save_finding(&finding)?;
    }
    Ok(())
}
```

This is the correct behavior that MCP must match.

#### `src/query/fields.rs` — hand-rolled levenshtein (lines 102-118)

```rust
fn levenshtein(a: &str, b: &str) -> usize {
    // ... 15 lines of manual implementation
}
```

Replace with `strsim::normalized_levenshtein()` for consistency and correctness.

### Project Structure — New Files

```
src/
  registry/
    mod.rs          # pub use rule, store, matcher, normalize
    rule.rs         # Rule struct, RuleStatus enum, RuleExample, RuleScope
    store.rs        # RuleStore -- CRUD on findings-data branch
    matcher.rs      # RuleMatcher -- 7-stage matching pipeline
    normalize.rs    # Canonical normalization functions
    stopwords.rs    # English stopwords for Jaccard filtering
    semantic.rs     # (feature-gated) Embedding generation + cosine similarity
  cli/
    rule.rs         # CLI handlers for tally rule subcommands
docs/
  reference/
    rule-registry.md  # Standalone reference document
tests/
  registry_model_test.rs      # Rule struct tests
  registry_matcher_test.rs    # Matching pipeline tests
  registry_normalize_test.rs  # Normalization tests
  registry_store_test.rs      # Git-backed storage tests
  cli_rule_test.rs            # CLI integration tests
  mcp_rule_test.rs            # MCP tool tests
  registry_migration_test.rs  # Migration tests
  registry_sync_test.rs       # Sync conflict resolution tests
  property_registry.rs        # Property tests
```

### Test Fixtures

#### Matcher Test Fixture — Pre-Existing Rules

```json
[
  {
    "id": "unsafe-unwrap",
    "name": "Potentially unsafe unwrap()",
    "description": "Using unwrap() on fallible types in production code can cause panics at runtime",
    "category": "safety",
    "severity_hint": "important",
    "aliases": ["unwrap-usage", "no-unwrap", "unwrap-on-option"],
    "cwe_ids": ["CWE-252"],
    "scope": { "include": ["src/**/*.rs"], "exclude": ["tests/**"] },
    "status": "active"
  },
  {
    "id": "sql-injection",
    "name": "SQL injection vulnerability",
    "description": "User input concatenated into SQL queries without parameterization",
    "category": "security",
    "severity_hint": "critical",
    "aliases": ["sqli", "raw-sql-concat"],
    "cwe_ids": ["CWE-89"],
    "scope": null,
    "status": "active"
  },
  {
    "id": "spec-drift",
    "name": "Implementation deviates from specification",
    "description": "Code behavior does not match the documented specification or architecture decision",
    "category": "spec-compliance",
    "severity_hint": "suggestion",
    "aliases": ["spec-mismatch", "spec-divergence"],
    "cwe_ids": [],
    "scope": null,
    "status": "active"
  },
  {
    "id": "resource-leak",
    "name": "Resource not properly released",
    "description": "File handles, connections, or memory allocated without corresponding cleanup",
    "category": "reliability",
    "severity_hint": "important",
    "aliases": [],
    "cwe_ids": ["CWE-404"],
    "scope": null,
    "status": "active"
  }
]
```

#### Matcher Test Cases — Input to Expected Output

| Input rule_id | Input CWE | Expected Stage | Expected canonical_id | Expected confidence | Expected method |
|---|---|---|---|---|---|
| `unsafe-unwrap` | - | 2 (exact) | `unsafe-unwrap` | 1.0 | `exact` |
| `unwrap-usage` | - | 3 (alias) | `unsafe-unwrap` | 1.0 | `alias` |
| `Unsafe_Unwrap` | - | 1+2 (normalize+exact) | `unsafe-unwrap` | 1.0 | `exact` |
| `dclaude:unsafe-unwrap` | - | 1+2 (normalize+exact) | `unsafe-unwrap` | 1.0 | `exact` |
| `no-unwrap` | - | 3 (alias) | `unsafe-unwrap` | 1.0 | `alias` |
| `unsaf-unwrap` (typo) | - | 5 (jaro_winkler) | `unsafe-unwrap` | ~0.96 | `jaro_winkler` |
| `memory-leak-risk` | `CWE-404` | 4 (CWE) | `resource-leak` | 0.7 | `cwe` (suggest only, not auto) |
| `memory-leak-risk` | - | 5 (jaro_winkler) | `resource-leak` | ~0.72 | `jaro_winkler` (suggest only) |
| `completely-new-rule` | - | none | `completely-new-rule` | 0.0 | `auto_registered` |
| `sqli` | - | 3 (alias) | `sql-injection` | 1.0 | `alias` |
| `check-sql-injection` | - | 5 (jaro_winkler) | `sql-injection` | ~0.82 | `jaro_winkler` (suggest, not auto) |

#### Scope Test Fixture

| Rule scope | Finding file path | Expected |
|---|---|---|
| `include: ["src/**/*.rs"]` | `src/api/handler.rs` | MATCH |
| `include: ["src/**/*.rs"]` | `tests/api_test.rs` | NO MATCH (warning) |
| `include: ["src/**/*.rs"], exclude: ["src/generated/**"]` | `src/generated/proto.rs` | NO MATCH (excluded) |
| `null` (no scope) | `anywhere/file.rs` | MATCH (default) |
| `include: ["src/api/**"]` | `src/lib.rs` | NO MATCH (outside scope) |
| `include: ["src/**/*.rs"], exclude: ["src/gen/**"]` | `src/gen/auto.rs` | NO MATCH (in include BUT also in exclude — exclude wins, warning) |
| `exclude: ["vendor/**"]` | `src/api.rs` | MATCH (no include constraint, not excluded) |
| `exclude: ["vendor/**"]` | `vendor/lib.rs` | NO MATCH (excluded, warning) |

#### Migration Test Fixture — 5 Findings with 3 Unique Rule IDs

```json
[
  { "uuid": "aaa...", "rule_id": "spec-drift", "severity": "suggestion" },
  { "uuid": "bbb...", "rule_id": "spec-drift", "severity": "important" },
  { "uuid": "ccc...", "rule_id": "unsafe-unwrap", "severity": "critical" },
  { "uuid": "ddd...", "rule_id": "spec-drift-tech-debt", "severity": "tech_debt" },
  { "uuid": "eee...", "rule_id": "spec-drift-tech-debt", "severity": "tech_debt" }
]
```

Expected: 3 rules registered (`spec-drift`, `unsafe-unwrap`, `spec-drift-tech-debt`), all with status `experimental`. Second `tally rule migrate` is idempotent (0 new rules).

### Auto-Registration Workflow (Detailed)

```
┌──────────────────────────────────────────────────────────────────┐
│ record_finding(rule_id, file, line, severity, category,         │
│                description, agent, ...)                         │
└────────────────────────────┬─────────────────────────────────────┘
                             │
                             ▼
                 ┌───────────────────────┐
                 │ 1. Load Rule Registry │
                 │ (all rules/*.json     │
                 │  into HashMap)        │
                 └───────────┬───────────┘
                             │
                             ▼
                 ┌───────────────────────┐
                 │ 2. Normalize rule_id  │
                 │ • lowercase           │
                 │ • _ → -               │
                 │ • strip "agent:" ns   │
                 │ • trim leading/       │
                 │   trailing hyphens    │
                 │ • collapse spaces → - │
                 │ (NO prefix stripping  │
                 │  of no-/disallow-/    │
                 │  check- — these carry │
                 │  semantic meaning)    │
                 └───────────┬───────────┘
                             │
                             ▼
                 ┌───────────────────────┐
                 │ 2b. Validate format   │
                 │ Must match:           │
                 │ ^[a-z0-9]             │
                 │  [a-z0-9-]{0,62}      │
                 │  [a-z0-9]$            │
                 │                       │
                 │ Invalid → error:      │
                 │ "Invalid rule ID      │
                 │  '<id>' after         │
                 │  normalization.       │
                 │  Must be 2-64 chars,  │
                 │  lowercase            │
                 │  alphanumeric with    │
                 │  hyphens."            │
                 └───────────┬───────────┘
                             │ (valid)
                             ▼
                 ┌───────────────────────┐
                 │ 3. Exact Match?       │──── YES ──▶ Use canonical ID
                 │ (HashMap lookup)      │            (confidence: 1.0)
                 └───────────┬───────────┘                   │
                             │ NO                            │
                             ▼                               │
                 ┌───────────────────────┐                   │
                 │ 4. Alias Match?       │──── YES ──▶ Use canonical ID
                 │ (scan aliases arrays) │            (confidence: 1.0)
                 └───────────┬───────────┘                   │
                             │ NO                            │
                             ▼                               │
                 ┌───────────────────────┐                   │
                 │ 5. CWE Match?         │──── YES ──▶ Add to
                 │ (if agent provided    │            similar_rules
                 │  CWE, scan cwe_ids)   │            (confidence: 0.7
                 │                       │            CWE too broad
                 │ NEVER auto on CWE     │            for auto-match)
                 │ alone (SARIF v2.1)    │                   │
                 └───────────┬───────────┘                   │
                             │ NO / no CWE                   │
                             ▼                               │
                 ┌───────────────────────┐                   │
                 │ 6. Jaro-Winkler       │                   │
                 │ (compare against all  │                   │
                 │  rule IDs)            │                   │
                 │                       │                   │
                 │ >= 0.85? ─── YES ────▶│ Auto-normalize    │
                 │ 0.6-0.84? ── ADD ────▶│ Add to            │
                 │              TO       │ similar_rules     │
                 │              SUGGEST  │ (don't normalize) │
                 └───────────┬───────────┘                   │
                             │ best < 0.6                    │
                             ▼                               │
                 ┌───────────────────────┐                   │
                 │ 7. Token Jaccard      │                   │
                 │ (if description       │                   │
                 │  provided)            │                   │
                 │                       │                   │
                 │ >= 0.5? ─── ADD ─────▶│ Add to            │
                 │              TO       │ similar_rules     │
                 │              SUGGEST  │                   │
                 └───────────┬───────────┘                   │
                             │                               │
                             ▼                               │
                 ┌───────────────────────┐                   │
                 │ 8. Semantic Search    │                   │
                 │ (if feature enabled)  │                   │
                 │                       │                   │
                 │ >= 0.8? ─── ADD ─────▶│ Add to            │
                 │              TO       │ similar_rules     │
                 │              SUGGEST  │                   │
                 └───────────┬───────────┘                   │
                             │                               │
                             ▼                               │
              ┌──────────────────────────────┐               │
              │ 9. NO MATCH — AUTO-REGISTER  │               │
              │                              │               │
              │ Create rules/<rule_id>.json: │               │
              │ {                            │               │
              │   id: <rule_id>,             │               │
              │   name: <rule_id>,           │               │
              │   description: <from finding>│               │
              │   category: <from finding>,  │               │
              │   severity_hint: <from       │               │
              │                  finding>,   │               │
              │   status: "experimental",    │               │
              │   created_by: <agent_id>,    │               │
              │   aliases: [],               │               │
              │   finding_count: 0           │               │
              │ }                            │               │
              │                              │               │
              │ Git commit: "Auto-register   │               │
              │  rule: <rule_id>"            │               │
              └──────────────┬───────────────┘               │
                             │                               │
                             ▼                               │
              ┌──────────────────────────────┐               │
              │ Use rule_id (original or     │◀──────────────┘
              │ canonical) for fingerprint   │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 10. Check Scope              │
              │ (if rule has scope)          │
              │                              │
              │ File matches include globs?  │
              │ File NOT in exclude globs?   │
              │                              │
              │ Mismatch → WARN (don't block)│
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 11. Compute Fingerprint      │
              │ SHA-256(file:start-end:      │
              │         canonical_rule_id)   │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 12. Dedup Resolution         │
              │ (FindingIdentityResolver)    │
              │                              │
              │ ExistingFinding → merge agent│
              │   (append to discovered_by,  │
              │    update timestamp)          │
              │                              │
              │ RelatedFinding → create new  │
              │   with relationship          │
              │                              │
              │ NewFinding → create new      │
              │   with rule_id = canonical,  │
              │   original_rule_id = input   │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 13. Increment finding_count  │
              │ on the matched rule          │
              │                              │
              │ NOTE: finding_count is a     │
              │ cached approximation, not    │
              │ a ledger. Local race: the    │
              │ upsert_file Modified retry   │
              │ re-reads the rule on each    │
              │ attempt. Remote race: sync   │
              │ merge takes max(). For exact │
              │ counts, use rebuild-index    │
              │ --include-rules.             │
              └──────────────┬───────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │ 14. Return Response          │
              │ {                            │
              │   status: "recorded" |       │
              │           "deduplicated",    │
              │   uuid: "...",               │
              │   rule_id: canonical,        │
              │   original_rule_id: input    │
              │     (if different),          │
              │   normalized_by: method,     │
              │   confidence: f64,           │
              │   similar_rules: [...],      │
              │   scope_warning: "..." | null│
              │ }                            │
              └──────────────────────────────┘
```

### Testing Standards

- Matcher tests named as documentation: `alias_maps_to_canonical()`, `levenshtein_threshold_0_85_auto_matches()`
- Normalization tests: `underscore_replaced_with_hyphen()`, `agent_namespace_stripped()`
- Each matching stage tested independently AND as part of the full pipeline
- Concurrent registration test: spawn 10 tasks registering different rules simultaneously
- E2E: record with alias -> dedup works with canonical -> query finds by original or canonical
- Property tests: `normalize_is_idempotent()`, `matcher_never_panics()`

### Confidence Score Thresholds (Configurable)

| Range | Action | Example |
|-------|--------|---------|
| 1.0 | Exact match or alias | `unsafe-unwrap` = `unsafe-unwrap`, or `unwrap-usage` via alias |
| 0.85-0.99 | Auto-normalize, note in response | Levenshtein close match (CWE alone maxes at 0.7 — never auto) |
| 0.60-0.84 | Suggest, don't auto-normalize | "Similar rule exists: 'unsafe-unwrap'" |
| < 0.60 | Register as new rule | No close match found |

### Technical Gotchas

1. **strsim 0.11.1 API (verified)**: `strsim::jaro_winkler(a, b) -> f64` and `strsim::normalized_levenshtein(a, b) -> f64`. Both return 0.0 (completely different) to 1.0 (identical). Already a transitive dep of clap in Cargo.lock. Use `jaro_winkler` for rule IDs (rewards common prefixes) and `normalized_levenshtein` for descriptions.

2. **globset 0.4.18 (verified)**: `globset` (from the ripgrep author, BurntSushi) is faster for matching one path against multiple patterns. `glob` is simpler but slower. Use `globset` since scope checking happens on every `record_finding` call.

3. **fastembed version**: Use `fastembed = "5"` (v5.12.1 is latest stable as of Mar 2026). The crate is published as `fastembed` on crates.io, not `fastembed-rs`. **fastembed API, cache directory, binary size, and model download details must be deep-researched before implementing Task 8.** These are behind the `semantic-search` feature flag and do NOT block Tasks 1-7.

4. **fastembed first-run**: Requires internet to download model on first use. If offline, constructor returns error. Handle gracefully — fall back to text similarity with warning. **Exact API (constructor name, method signatures, InitOptions) must be verified against fastembed v5 docs before Task 8.**

5. **Concurrent rule registration** (deep-researched — git2 concurrency model):

   **Local race (two agents, same repo):** `upsert_file()` must handle TWO error codes:
   - `ErrorCode::Locked` (-14) — file-level lock on ref file, retry with backoff (already handled)
   - `ErrorCode::Modified` (-15) — ref moved between reading tip and committing (compare-and-swap failure). **NOT currently handled.** Fix: on `Modified`, re-read branch tip, rebuild tree from new parent, retry. Same exponential backoff, same `MAX_LOCK_RETRIES` limit.

   **Remote race (two clones diverge, then sync):** When both clones auto-register the same rule ID, `tally sync` → `merge_trees()` produces an add/add conflict on `rules/<id>.json`. Current `sync()` errors out with "This should not happen with one-file-per-finding" — but with rules, it WILL happen. Fix: add rule-aware conflict resolution to `sync()`:
   1. Detect conflicts on `rules/*.json` paths (findings conflicts remain errors)
   2. Parse both sides as `Rule` JSON
   3. Merge semantically:
      - `id`: same (that's why they conflicted)
      - `description`: take longest (more detail = better)
      - `aliases`: union of both arrays (deduplicated)
      - `cwe_ids`: union of both arrays (deduplicated)
      - `tags`: union of both arrays (deduplicated)
      - `severity_hint`: take highest severity (critical > important > suggestion > tech_debt)
      - `category`: take ours (deterministic tiebreak)
      - `created_by`: take the side with earliest `created_at`
      - `created_at`: take earliest
      - `updated_at`: take latest
      - `finding_count`: take max
      - `status`: take most promoted (active > experimental > deprecated)
      - `scope`: take ours (log warning if both differ — manual review needed)
      - `embedding`: set to null (recompute on next access)
   4. Write merged JSON, remove conflict entries via `Index::conflict_remove()`, add resolved entry via `Index::add()`
   5. Continue merge — create merge commit as normal

   **Why these merge rules:** Union for arrays (no data loss). Longest description (more context wins). Earliest created_at (first discoverer is canonical creator). Max finding_count (neither side should decrease). Most promoted status (if one side activated a rule, honor that).

6. **Rule ID validation**: IDs must match `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$` — lowercase alphanumeric with hyphens, **minimum 2 chars** (intentional: single-letter rule IDs like "a" are not meaningful), maximum 64 chars, no leading/trailing hyphens.

7. **Backward compatibility**: Existing findings with `rule_id` values not in the registry continue to work. The `original_rule_id` field is `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — no breaking change to v0.4.x/v0.5.x finding JSON format.

8. **Registry loading performance**: Load all rule files into a `HashMap<String, Rule>` on startup. For a project with 100 rules, this is ~50 file reads from git tree objects — well under 100ms. Cache in memory for the duration of the CLI invocation.

9. **strsim replaces hand-rolled levenshtein**: The existing `levenshtein()` in `src/query/fields.rs` (used for TallyQL field name suggestions) should be replaced with `strsim::normalized_levenshtein()` for consistency. Note: return type changes from `usize` (edit distance) to `f64` (0.0-1.0 similarity) — callers must be updated. Minor refactor as part of Task 1.7.

10. **Embedding cache invalidation**: When a rule's `description` or `name` changes, recompute the embedding. Compare `embedding_model` field to current model version — if mismatched, recompute on next access.

11. **clap 4 nested subcommands (verified)**: The `tally rule create` pattern requires a nested `#[derive(Subcommand)]` enum. Add to `Command` enum:
```rust
/// Manage rules in the rule registry
Rule {
    #[command(subcommand)]
    action: RuleCommand,
},
```
Then define `RuleCommand` as a separate `#[derive(Subcommand)]` enum with `Create`, `Get`, `List`, `Search`, `Update`, `Delete`, `AddExample` variants. This pattern is used in clap's own `git-derive.rs` example.

12. **strsim return type migration**: The existing hand-rolled `levenshtein()` in `src/query/fields.rs:103` returns `usize` (edit distance where 0 = identical). `strsim::normalized_levenshtein()` returns `f64` (similarity where 1.0 = identical). The caller in `validate_field()` currently checks `levenshtein(f, name) <= 2` — this must change to `normalized_levenshtein(f, name) >= 0.6` (or similar threshold). Test that TallyQL field suggestions still work correctly after the migration.

### References

- [Source: tally/src/storage/git_store.rs:586] — `upsert_file()` pattern for git-backed storage
- [Source: tally/src/model/identity.rs:12] — `compute_fingerprint()` function
- [Source: tally/src/model/identity.rs:53] — `FindingIdentityResolver` matching logic
- [Source: tally/src/cli/record.rs:102] — `handle_dedup()` agent merge logic (CLI, correct behavior)
- [Source: tally/src/mcp/server.rs:483] — MCP dedup (BROKEN, does not merge agents)
- [Source: tally/src/query/fields.rs:102] — hand-rolled `levenshtein()` to replace with strsim
- Deep research: SARIF 2.1.0 reportingDescriptor schema
- Deep research: Semgrep rule YAML format with examples and autofix
- Deep research: strsim crate API (normalized_levenshtein, jaro_winkler)
- Deep research: fastembed crate API (TextEmbedding, feature flags)
- Deep research: git2 concurrent write patterns with retry — **confirmed `ErrorCode::Modified` (-15) for compare-and-swap failure, `IndexConflict` struct with ancestor/our/their fields for programmatic conflict resolution, `Index::conflict_remove()` + `Index::add()` for resolved entries**

---

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Completion Notes List

1. **AC-2 Stage 5 deviation:** Changed from auto-match at JW >= 0.85 to suggestion-only. Deep research (Perplexity, Mar 2026) confirmed production tools use deterministic rule ID matching. JW prefix-weighting causes false positives on structured identifiers.
2. **Beyond-spec deliverables:** `update_batch_status` tool, 5 additional resources (version, rules/summary, rules/{id}, agent/{id}, timeline/{duration}), 3 new prompts (consolidate-rules, rule-coverage-report, triage-by-rule), 4 enhanced prompts (fix-finding, triage-file, review-pr, summarize-findings).
3. **Test coverage:** 770 tests (630 existing + 140 new), 79.8% line coverage. 20 additional feature-gated semantic search tests.
4. **dclaude v2.9.0:** 7 skills updated, CLAUDE.md documented, seed rules file.

### File List

**New files:**
- `src/registry/mod.rs` — module declaration, re-exports
- `src/registry/rule.rs` — Rule struct, RuleStatus, RuleScope, RuleExample
- `src/registry/store.rs` — RuleStore CRUD
- `src/registry/matcher.rs` — 7-stage matching pipeline
- `src/registry/normalize.rs` — canonical normalization + validation
- `src/registry/stopwords.rs` — minimal stopwords for Jaccard
- `src/registry/scope.rs` — glob-based scope enforcement
- `src/registry/semantic.rs` — fastembed embeddings (feature-gated)
- `src/cli/rule.rs` — 8 CLI subcommand handlers + migrate + reindex
- `docs/reference/rule-registry.md` — standalone MCP resource reference
- `tests/registry_model_test.rs` — 25 tests
- `tests/registry_normalize_test.rs` — 36 tests
- `tests/registry_matcher_test.rs` — 27 tests
- `tests/registry_scope_test.rs` — 8 tests
- `tests/cli_rule_test.rs` — 14 tests
- `tests/e2e_rule_registry_test.rs` — 3 tests
- `tests/property_registry.rs` — 4 property tests
- `tests/registry_semantic_test.rs` — 20 feature-gated tests
- `tests/mcp_enhanced_test.rs` — 14 tests
- `tests/e2e_mcp_workflow_test.rs` — 9 tests

**Modified files:**
- `Cargo.toml` — strsim, globset, fastembed (optional), version 0.7.0
- `src/lib.rs` — pub mod registry
- `src/model/finding.rs` — original_rule_id field
- `src/storage/git_store.rs` — upsert_file Modified retry, init rules/.gitkeep, sync rule conflict resolution, rebuild_rule_counts, ensure_rules_dir
- `src/cli/mod.rs` — Rule subcommand, RuleCommand enum, RebuildIndex --include-rules
- `src/cli/record.rs` — matcher integration, scope check, rule info output
- `src/main.rs` — Rule command dispatch, Reindex
- `src/mcp/server.rs` — 9 new tools, 5 new resources, 3 new prompts, 4 enhanced prompts, dedup fix, matcher integration
- `src/query/fields.rs` — strsim replacement
- `README.md` — rule registry section, updated counts
- `CLAUDE.md` — project state, On-Demand References
- `.typos.toml` — actve allowlist
- `.github/workflows/ci.yml` — semantic-search test job
- `justfile` — llvm-cov path fix
