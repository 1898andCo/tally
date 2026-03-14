# CLAUDE.md

@SOUL.md
@.claude/rules/_index.md

## Project State (Mar 2026)

Single binary crate (`tally-ng` on crates.io, binary name `tally`). v0.4.0.
255 tests, 90.6% coverage. Dual interface: CLI (clap) + MCP server (rmcp).
Git-backed storage on orphan `findings-data` branch via `git2`.

## Build & Test

```bash
cargo build                    # Build
cargo test                     # Run all tests
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo +nightly fmt --all       # Format (requires nightly)
just check                     # Quick pre-commit (fmt + clippy + deny)
just ci                        # Full CI mirror
```

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
