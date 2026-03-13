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
