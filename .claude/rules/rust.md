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
