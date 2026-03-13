# Rust Coding Rules

## Safety

- `#![forbid(unsafe_code)]` in main crate
- No `unwrap()` in production code — use `?` or `expect()` with message
- No blocking the async runtime — use `spawn_blocking` for CPU work

## Error Handling

- Use `thiserror` for error enums
- `pub type Result<T> = std::result::Result<T, FindingsError>`
- Structured error variants, not string bags

## Module Structure

- `lib.rs`: public API re-exports only
- `error.rs`: crate error types
- Domain modules in subdirectories

## Testing

- Unit tests: `#[cfg(test)] mod tests {}` for private internals
- Integration tests: `tests/` directory
- Property tests: `tests/property_*.rs` with `proptest`
- Test names as documentation: `fingerprint_deterministic_for_same_input()`
