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

## 9. Observability Is Infrastructure

Structured tracing goes in from the start, not when debugging. Every handler is instrumented with `#[tracing::instrument]`. Use `tracing::info!` for successful operations, `tracing::warn!` for recoverable issues, and structured fields (not interpolated strings) so logs are machine-parseable.

- CLI verbosity controls tracing level (`-v` info, `-vv` debug, `-vvv` trace)
- MCP operations log finding UUIDs and operation types
- Git storage operations log branch, commit counts, and sync results

---

## 10. Layered Architecture, Strictly Acyclic

Even within a single crate, the dependency graph flows one direction. Modules depend downward, never upward or circularly.

```
main.rs / mcp/server.rs   (entry points)
    ↓
cli/                       (command dispatch + handlers)
    ↓
model/                     (data types, state machine, identity)
    ↓
storage/                   (git-backed persistence)
    ↓
error.rs                   (error types — depended on by all)
```

If a lower layer needs something from a higher layer, the design is wrong. `model/` never imports from `cli/`. `storage/` never imports from `mcp/`.

---

## 11. Behavior Is Configuration; Policy Is Types

Tunable behavior belongs in configuration or CLI flags, not hardcoded in logic. Proximity thresholds for deduplication, default output formats, remote names, batch sizes — these are operational decisions that shouldn't require code changes.

But not everything is configuration. **State machine transitions, identity rules, and domain invariants belong in the type system** (Principle #2).

| Configuration (CLI flags, defaults) | Code (Rust types, compile-time) |
|---|---|
| Dedup proximity threshold (5 lines) | State machine transition rules |
| Default output format (json) | Severity enum variants |
| Remote name (origin) | UUID v7 identity generation |
| Suppression expiry dates | Fingerprint computation |
| Agent ID defaults (cli) | Relationship type validation |

When adding a new parameter, ask: **"Will an operator need to change this without a code release?"** If yes, it's a flag or default. If changing it incorrectly could violate a domain invariant, it's a type.

---

## Current State (Mar 2026)

Single binary crate, v0.4.0. 255 tests, 90.6% coverage. Dual interface (CLI + MCP).
All 11 principles are actively applied. Update this section during releases.
