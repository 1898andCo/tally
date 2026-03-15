# Kickstart: Story 1.2 Rule Registry — Session Resume

## What We're Doing

Building Story 1.2: Rule Registry for tally (git-backed findings tracker). Story file is at:
`/Users/jmagady/Dev/tally/_bmad-output/implementation-artifacts/1-2-rule-registry.md`

## Current State

- Story 1.1 (TallyQL) is DONE and released as tally v0.6.1
- Story 1.2 story spec is written and **adversarial review COMPLETE**
- 14-point audit: PASSED (after fixes)
- Uncertainty scan: PASSED (fastembed version fixed to v5, strsim/globset/clap verified)
- 14 adversarial gotchas: ALL REMEDIATED (13 fixed, 1 moot)
- Final consistency pass: DONE — added release tasks (Task 11), MCP feature parity (8 tools, not 6), normalization alignment across AC-2/Task 2.1/workflow diagrams
- dclaude cross-repo scan: DONE — 17 findings identified, all remediated. Added AC-12 + Task 12 (23 subtasks) for dclaude integration. Story 1.3 placeholder created for deferred capabilities (MCP Prompts, SARIF export).
- **Story is ready for implementation planning**
- 13 task groups, 112 subtasks total

## Gotcha Progress (14 total)

| # | Gotcha | Status | Resolution |
|---|--------|--------|------------|
| 1 | Prefix stripping is destructive | **FIXED** | Removed prefix stripping entirely. Only normalize: lowercase, `_`→`-`, strip agent namespace. Per ESLint/Semgrep/SARIF — prefixes carry semantic meaning. |
| 2 | CWE cross-ref false matches | **FIXED** | Changed CWE confidence from 0.9 to 0.7 (suggest only). Updated: AC-2, matching pipeline walkthrough, auto-registration workflow diagram, matcher test fixture table, confidence thresholds table. CWE alone is never sufficient for auto-match per SARIF v2.1 and CWE mapping analysis. |
| 3 | Auto-registration race condition | **FIXED** | Deep-researched git2 concurrency. TWO fixes: (1) Local race: `upsert_file()` must handle `ErrorCode::Modified` (-15) with re-read + rebuild + retry, not just `Locked`. (2) Remote race: `sync()` must detect `rules/*.json` conflicts, parse both sides, merge semantically (union arrays, longest desc, earliest created_at, max finding_count, most promoted status). Added Task 1.8, Task 7B (6 subtasks), and test tasks 9.11b/9.11c. |
| 4 | Alias shadows canonical ID | **FIXED** | Expanded step 3 in `rule create` workflow to bidirectional ID namespace check: aliases can't match canonical IDs, canonical IDs can't match existing aliases. Same check on `rule update --add-alias`. Auto-registration safe because alias lookup (step 4) already runs before auto-register (step 9). Added test 9.3b. No deep research needed — straightforward validation. |
| 5 | Scope exclude not checked | **FIXED** | Expanded scope workflow to 4 steps: no-scope check, include check, exclude check (exclude takes priority over include), continue. Added 3 new scope test fixtures (include+exclude overlap, exclude-only). No deep research needed — straightforward missing branch. |
| 6 | finding_count not atomic | **FIXED** | `finding_count` is a cached approximation, not a ledger. Local race handled by `ErrorCode::Modified` retry (re-reads rule). Remote race handled by sync merge (take max). Exact counts recoverable via `rebuild-index --include-rules`. Documented in workflow step 13. No deep research needed — pragmatic accept. |
| 7 | Migration uses first description | **FIXED** | Changed `first_description` → `longest_description` in MigrationInfo struct and workflow step 4. Longest description has more context for seeding the rule. Consistent with sync merge rule (longest description wins). No deep research needed. |
| 8 | No update workflow | **FIXED** | Clarified: (1) Deprecated rules still resolve — new findings warn but are recorded, existing findings unaffected. (2) Rule IDs are immutable (file names). "Rename" = create new rule + add old ID as alias on new rule — alias lookup handles resolution, no bulk rewrite needed. No deep research needed. |
| 9 | Double-resolve ambiguity | **MOOT** | With prefix stripping removed (Gotcha #1), normalization only does lowercase + `_`→`-` + strip `agent:` namespace — non-destructive. Exact match runs before alias lookup (sequential pipeline). Gotcha #4 prevents aliases from shadowing canonical IDs. No ambiguity possible. No changes needed. |
| 10 | Search doesn't report alias match method | **FIXED** | Added step 2 to search workflow: check exact + alias match BEFORE fuzzy search. Exact matches return `method: "exact"`, alias matches return `method: "alias_match"`, both at confidence 1.0. Fuzzy results follow as additional suggestions. No deep research needed. |
| 11 | Auto-registration skips ID validation | **FIXED** | Added step 2b (format validation) to auto-registration workflow, between normalization and matching. Invalid IDs after normalization return an error. Also added space→hyphen and trim leading/trailing hyphens to normalization (step 2). Updated Task 2.1 and normalization test 9.3. No deep research needed. |
| 12 | Semantic search misses unembedded rules | **FIXED** | Added lazy embedding computation: on semantic search, compute and cache embeddings for rules with `embedding: null` on-the-fly (never skip). If >50 rules need embedding, warn and suggest `tally rule reindex --embeddings`. Added Tasks 8.8 and 8.9. No deep research needed. |
| 13 | tally init with existing rules | **FIXED** | Updated Task 7.4: `tally init` adds `rules/.gitkeep` to initial tree. For existing repos (pre-1.2), if `rules/` dir missing, adds it via `upsert_file()`. Remains idempotent. No deep research needed. |
| 14 | Stopwords list too aggressive | **FIXED** | Updated Task 2.7: use minimal ~20 word list (articles, basic prepositions, basic conjunctions). Explicitly EXCLUDE negation words (not, no, without, never) and temporal/comparative words (before, after, more, less) — these carry semantic meaning in technical descriptions. Not the 150+ word NLTK/spaCy lists. No deep research needed. |

## Files to Re-Read After Compaction

### Critical — Story spec being reviewed:
1. `/Users/jmagady/Dev/tally/_bmad-output/implementation-artifacts/1-2-rule-registry.md` — THE story file (1,339 lines, read in full)

### Context — Tally project state:
2. `/Users/jmagady/Dev/tally/CLAUDE.md` — project conventions
3. `/Users/jmagady/Dev/tally/src/storage/git_store.rs` lines 586-633 — `upsert_file()` pattern
4. `/Users/jmagady/Dev/tally/src/model/identity.rs` lines 12-28 — `compute_fingerprint()`
5. `/Users/jmagady/Dev/tally/src/model/identity.rs` lines 53-124 — `FindingIdentityResolver`
6. `/Users/jmagady/Dev/tally/src/cli/record.rs` lines 102-160 — `handle_dedup()` (correct behavior)
7. `/Users/jmagady/Dev/tally/src/mcp/server.rs` lines 483-490 — MCP dedup (BROKEN, needs fix)
8. `/Users/jmagady/Dev/tally/src/query/fields.rs` lines 95-120 — hand-rolled `levenshtein()` to replace

### Context — What was already implemented in Story 1.1:
9. `/Users/jmagady/Dev/tally/src/query/parser.rs` — TallyQL Chumsky parser (reference for code patterns)
10. `/Users/jmagady/Dev/tally/src/query/eval.rs` — evaluator with `apply_filters()` / `apply_sort()`

### Memory:
11. `/Users/jmagady/.claude/projects/-Users-jmagady-Dev-axiathon/memory/MEMORY.md` — session memory

## Key Decisions Already Made

1. **File-per-rule storage** on existing `findings-data` orphan branch (not monolithic JSON)
2. **7-stage matching pipeline**: normalize → exact → alias → CWE → jaro_winkler → token jaccard → semantic
3. **No prefix stripping** (Gotcha #1 resolved — ESLint/Semgrep/SARIF all say no)
4. **CWE match = suggestion only** (Gotcha #2 researched — confidence 0.7, not 0.9)
5. **strsim 0.11** for fuzzy matching (verified, already transitive dep of clap)
6. **globset 0.4** for scope glob matching (verified 0.4.18 latest)
7. **fastembed 5** for semantic search (optional, behind `--features semantic-search`)
8. **clap 4 nested subcommands** for `tally rule create/get/list/search/update/delete` (verified pattern)
9. **Auto-registration** creates rules with status `experimental` (not `active`)
10. **MCP dedup fix**: append agent to `discovered_by` on ExistingFinding (match CLI behavior)
11. **dclaude integration** (cross-repo): dclaude skills must search_rules before recording, handle new response fields, query existing findings before recording, use add_note/add_tag during workflows, enrich sprint-review/session-summary with tally data
12. **Deferred to Story 1.3**: MCP Prompts integration (not production-ready), SARIF export → GitHub Code Scanning (needs validation)

## Deep Research Results Summary

### Gotcha #1 Research (prefix stripping):
- ESLint: does NOT strip prefixes, treats each rule as opaque
- Semgrep: does NOT normalize, uses fully qualified IDs
- SARIF v2.1: explicitly says rule IDs are tool-specific, no normalization recommended
- False positive rate from naive prefix stripping: 15-40%
- `no-X` (disallow) vs `check-X` (verify) vs `require-X` (mandate) are semantic opposites

### Gotcha #2 Research (CWE specificity):
- CWE-252 alone covers: unchecked malloc, setuid, fork, pthread, file ops — all different remediation
- CWE-79 covers both stored AND reflected XSS — different exploitation vectors
- SARIF says "Tags SHOULD NOT be used to label a result as belonging to CWE"
- Variant-level CWE: 0.80-0.88 confidence. Base-level: 0.65-0.75. Class-level: 0.45-0.60
- Recommendation: CWE should be ONE signal, never auto-match alone

## What to Do Next

1. ~~Apply Gotcha #2 fix to story~~ DONE
2. ~~Continue adversarial review: Gotchas 3-14~~ ALL DONE
3. ~~Final consistency pass on the story~~ DONE
4. ~~dclaude cross-repo scan~~ DONE (17 findings, AC-12 + Task 12 added)
5. **BUILD IMPLEMENTATION PLAN** (like we did for Story 1.1)
6. Create feature branch `feature/story-1.2-rule-registry` from develop
7. Implement Tasks 1-11 in tally repo (commit after each task group)
8. After tally PR merged + released as v0.7.0: implement Task 12 in dclaude repo
9. Story 1.3 (deferred capabilities) goes to backlog

### Gotcha #3 Research (git2 concurrency):
- `repo.commit(Some(ref), ...)` does compare-and-swap — returns `ErrorCode::Modified` (-15) if ref moved since parent was read
- `ErrorCode::Locked` (-14) is file-level lock — different from logical conflict
- Must handle BOTH in retry loop — `Modified` requires re-reading tip + rebuilding tree
- `merge_trees()` returns `IndexConflict` with ancestor/our/their `Option<IndexEntry>` fields
- Resolve conflicts via `Index::conflict_remove()` + `Index::add()` with merged content
- For add/add conflicts (both sides create same path), ancestor is `None`

## Working Directories
- **axiathon repo**: `/Users/jmagady/Dev/axiathon` (current working dir in Claude Code)
- **tally repo**: `/Users/jmagady/Dev/tally` (where the story and code live)
- **tally branch**: `develop` (story 1.2 not yet on a feature branch)
- **tally version**: v0.6.1 (released with TallyQL + MCP resource)
