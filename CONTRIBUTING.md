# Contributing to tally

Thank you for your interest in contributing to tally. This guide will help you set up your development environment and understand our workflow.

## Prerequisites

You will need the following tools installed:

| Tool | Version | Install |
|------|---------|---------|
| [Rust](https://rustup.rs/) | 1.85+ (Edition 2024) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Nightly rustfmt | latest | `rustup toolchain install nightly --component rustfmt` |
| [just](https://github.com/casey/just) | latest | `brew install just` or `cargo install just` |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | latest | `cargo install cargo-deny --locked` |
| [cargo-watch](https://github.com/watchexec/cargo-watch) | latest | `cargo install cargo-watch --locked` |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | latest | `cargo install cargo-llvm-cov --locked` |
| [taplo](https://taplo.tamasfe.dev/) | latest | `brew install taplo` or `cargo install taplo-cli --locked` |
| [lefthook](https://github.com/evilmartians/lefthook) | latest | `brew install lefthook` |
| [typos-cli](https://github.com/crate-ci/typos) | latest | `brew install typos-cli` |
| [git-cliff](https://github.com/orhun/git-cliff) | latest | `cargo install git-cliff --locked` |

> **macOS tip:** Using `brew install` for just, lefthook, taplo, and typos-cli is faster than building from source via cargo.

> **Warning: Do NOT install Rust via Homebrew (`brew install rust`).** Homebrew's Rust binaries (`cargo`, `rustfmt`, `clippy`) shadow rustup's proxies and break toolchain directives like `cargo +nightly`. If you have Homebrew Rust installed, remove it: `brew uninstall rust`. Use [rustup](https://rustup.rs/) exclusively for Rust toolchain management.

## First-Time Setup

Complete these steps to go from zero to running tests. Target time: **under 10 minutes**.

### 1. Clone the repository

```bash
git clone git@github.com:1898andCo/tally.git
cd tally
```

### 2. Install prerequisites

```bash
# Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup toolchain install nightly --component rustfmt

# All dev tools at once
just setup
```

Or install individually:

```bash
brew install just lefthook taplo typos-cli
cargo install cargo-deny cargo-watch cargo-llvm-cov git-cliff --locked
```

### 3. Set up Git hooks

```bash
lefthook install
```

This installs pre-commit hooks that run formatting, linting, and test checks before every commit.

### 4. Build and test

```bash
cargo build     # compile
cargo test      # run all tests
cargo clippy    # lint check
```

If all three commands pass, your environment is ready.

## Git Workflow

This project uses **Git Flow**. All work branches from `develop` and merges back via pull request.

### Branch structure

| Branch | Purpose | Branch from |
|--------|---------|-------------|
| `main` | Production releases only | — |
| `develop` | Integration branch (default) | `main` |
| `feature/*` | New features | `develop` |
| `fix/*` | Bug fixes | `develop` |
| `release/*` | Release preparation | `develop` |
| `hotfix/*` | Emergency production fixes | `main` |

### Special branches

| Branch | Purpose | Protected |
|--------|---------|-----------|
| `findings-data` | Orphan branch for finding storage (git2 plumbing only) | Deletion restricted |

> **Do not manually edit or delete the `findings-data` branch.** It is managed exclusively by tally's git2 storage layer. Branch deletion is restricted by GitHub ruleset.

### Branch naming

```
feature/short-description
fix/issue-description
```

### Creating a branch

```bash
git checkout develop
git pull origin develop
git checkout -b feature/my-feature
```

### Commit messages

All commits follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[optional body]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

**Scopes:** `model`, `mcp`, `cli`, `storage`, `state`, `session`

**Rules:**
- Use imperative, present tense ("add" not "added")
- Do not capitalize the first letter of the description
- Do not end the description with a period
- No AI attribution lines (no "Generated with Claude Code", no "Co-Authored-By: Claude")

**Examples:**

```
feat(mcp): add update_finding tool
fix(storage): handle ref lock retry on concurrent writes
docs(cli): update query subcommand help text
test(model): add deferred-to-reopened transition test
```

## Code Quality

### Pre-commit checks

Lefthook runs the following on every commit (in parallel):
- `cargo +nightly fmt --all -- --check` — formatting (nightly rustfmt)
- `cargo clippy --all-targets --all-features -- -D warnings` — lint warnings treated as errors
- `taplo fmt --check` — TOML formatting enforcement
- `cargo test --all-targets` — run tests
- `typos` — spell check

> **Note:** Conventional commit format is a team convention enforced by code review. Branch protection (no direct commits to `develop` or `main`) is enforced by GitHub branch protection rules, not local hooks.

### Manual quality check

```bash
cargo +nightly fmt --all                                              # format code
cargo clippy --all-targets --all-features -- -D warnings              # lint
taplo fmt --check                                                     # TOML formatting
cargo test --all-targets                                              # run tests
cargo test --doc --all-features                                       # doc tests
typos                                                                 # spell check
```

Or use just recipes:

```bash
just check      # quick: fmt + clippy + deny
just ci          # full CI mirror: fmt + clippy + build + deny + test + doc-test
```

## Pull Request Process

1. **Branch from `develop`** — never from `main`
2. **Write tests** for new functionality
3. **Ensure all checks pass:** `just ci`
4. **Use conventional commit messages**
5. **Open a PR targeting `develop`**
6. **Describe your changes** — what and why, not just how
7. **Address review feedback** — push new commits, don't force-push during review

### What reviewers look for

- Tests cover acceptance criteria
- No `unwrap()` in production code (use `?` or `expect()` with actionable message)
- Structured error variants, not string bags
- State machine transitions validated (no bypass of `allowed_transitions()`)
- Finding identity immutability preserved (uuid, fingerprint, rule_id never modified)
- MCP and CLI interfaces stay in sync (same features available in both)
- Conventional commit messages

## Project Structure

```
tally/
├── src/
│   ├── main.rs             # CLI entry point + MCP server mode
│   ├── lib.rs              # Public API re-exports
│   ├── error.rs            # TallyError types
│   ├── session.rs          # SessionIdMapper (UUID <-> short IDs)
│   ├── model/
│   │   ├── finding.rs      # Finding, Severity, AgentRecord, Suppression
│   │   ├── identity.rs     # FindingIdentityResolver, fingerprint computation
│   │   └── state_machine.rs # LifecycleState, StateTransition, validation
│   ├── storage/
│   │   └── git_store.rs    # Git-backed one-file-per-finding on orphan branch
│   ├── cli/
│   │   └── *.rs            # Clap CLI subcommands
│   └── mcp/
│       └── server.rs       # MCP server (rmcp) with tools, resources, prompts
├── tests/
│   ├── model_test.rs       # Data model + state machine
│   ├── storage_test.rs     # Git storage round-trip
│   ├── identity_test.rs    # Fingerprint + dedup
│   ├── cli_test.rs         # CLI integration
│   ├── mcp_test.rs         # MCP tool integration
│   ├── mcp_unit_test.rs    # MCP server unit tests
│   ├── e2e_lifecycle_test.rs # Full user workflows
│   ├── error_test.rs       # Error types
│   ├── session_test.rs     # Session ID mapping
│   └── property_identity.rs # Proptest: fingerprint determinism
├── docs/
│   ├── story.md            # Original implementation story
│   └── story-finding-mutability.md # v0.5.0 enhancement story
├── Cargo.toml
├── Cargo.lock              # Committed (binary crate)
├── justfile                # Development recipes
├── cliff.toml              # git-cliff changelog config
├── deny.toml               # cargo-deny license/advisory config
├── rust-toolchain.toml
├── .typos.toml
├── .lefthook.yml
├── .mcp.json               # Example MCP config for Claude Code
├── CLAUDE.md               # AI assistant guidance
├── CONTRIBUTING.md          # This file
├── LICENSE                  # Apache-2.0
└── README.md
```

## Testing

| Test type | Location | Convention |
|-----------|----------|------------|
| Unit tests | `#[cfg(test)] mod tests {}` in same file | Named after function under test |
| Integration tests | `tests/` directory | Named after feature being tested |
| Property tests | `tests/property_*.rs` | Named after property being verified |
| E2E lifecycle | `tests/e2e_lifecycle_test.rs` | Named after workflow being exercised |
| Snapshot tests | `tests/snapshot_*.rs` | Uses `insta::assert_snapshot!` |

```bash
cargo test                     # all tests
cargo test -- test_name        # specific test
just coverage                  # coverage summary
just coverage-html             # coverage HTML report
```

## Release Process

Releases are tag-driven. See `docs/story-finding-mutability.md` "Release Process" section for the full pipeline, or the quick version:

```bash
just ci                  # verify everything passes
just release 0.5.0       # bump version, changelog, commit, tag
git push origin develop --tags  # triggers release workflow
```

The release workflow builds binaries (Linux, macOS, Windows), publishes to crates.io, updates the Homebrew formula, and syncs `main`.

## Getting Help

- **Story specs:** See `docs/story.md` and `docs/story-finding-mutability.md`
- **Open an issue:** For bugs, questions, or feature requests
- **README:** See `README.md` for user-facing documentation
