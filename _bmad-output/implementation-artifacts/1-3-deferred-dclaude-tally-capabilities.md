# Story 1.3: Deferred dclaude–tally Capabilities (Placeholder)

Status: backlog
Repository: `1898andCo/tally` + `1898andCo/dclaude`
Language: Rust + Markdown
Date: 2026-03-15
Epic: Query & Search Enhancements
Depends-on: Story 1.2 (Rule Registry)

---

## Problem Statement

Several tally capabilities exist or are planned that dclaude could leverage but are not ready for integration in Story 1.2. This story tracks them for future implementation.

---

## Deferred Capabilities

### 1. MCP Prompts Integration

Tally exposes 5 MCP prompts (triage-file, fix-finding, explain-finding, summarize-findings, review-pr) that provide structured reasoning about findings. dclaude doesn't use any of them. These prompts are not production-ready yet but could significantly enhance dclaude's fix workflow, sprint reviews, and finding triage.

**Scope (when ready):**
1. **Audit MCP prompts** — review each prompt for quality, accuracy, and usefulness in dclaude workflows
2. **Improve prompts** — update prompt templates based on dclaude's actual usage patterns
3. **Integrate into dclaude** — wire prompts into relevant workflow steps:
   - `pr-fix-verify` Step 9 (present finding) → `explain-finding`
   - `pr-fix-verify` Step 11 (verify fix) → `fix-finding`
   - `pr-fix-verify` Step 8 (dashboard) → `triage-file` for prioritization
   - `sprint-review` → `summarize-findings` for stakeholder briefs
   - `sweep-only` Step 5 (summary) → `review-pr` for PR comment generation
4. **Test prompt quality** — validate that prompts produce useful output across different finding types and severities

**Why deferred:** Prompts are not ready for prime time. They need quality review, testing with real findings across project types, refinement for dclaude's context needs, and graceful degradation when output is poor.

### 2. SARIF Export → GitHub Code Scanning Integration

Tally can export findings as SARIF 2.1.0 (GitHub Code Scanning compatible). dclaude could:
- After the gauntlet check in `pr-fix-verify`, export findings as SARIF via `export_findings(format: "sarif")`
- Upload SARIF to GitHub Code Scanning via `gh api` — makes tally findings visible directly in GitHub's Security tab and as PR annotations
- Enables GitHub's built-in finding dismissal/triage UI alongside tally's lifecycle management

**Scope (when ready):**
1. **Validate SARIF output** — ensure tally's SARIF export meets GitHub Code Scanning upload requirements (schema version, tool info, result locations)
2. **Add upload step to pr-fix-verify** — after gauntlet (Step 17.5), export + upload if configured
3. **Handle dedup between GitHub and tally** — GitHub Code Scanning has its own dedup; findings may appear in both places
4. **Configuration** — opt-in via dclaude config or environment variable (not all repos want Code Scanning uploads)
5. **Permissions** — requires `security_events: write` permission on the GitHub token

**Why deferred:** Requires validation of SARIF output quality, permissions setup, and design decisions around dual-tracking (tally + GitHub Code Scanning).

---

## References

- tally MCP prompts: `src/mcp/server.rs` (prompt handlers)
- tally SARIF export: `src/cli/export.rs` (SARIF 2.1.0 format)
- dclaude workflows: `skills/pr-fix-verify/SKILL.md`, `skills/sweep-only/SKILL.md`, `skills/sprint-review/SKILL.md`
- GitHub Code Scanning SARIF upload: `POST /repos/{owner}/{repo}/code-scanning/sarifs`
- Story 1.2: Rule Registry (prerequisite — prompts and exports should use canonical rule IDs)
