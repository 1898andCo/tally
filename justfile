# tally Development Commands

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments := true

# List available recipes
default:
    @just --list

# ---------------------------------------------------------------------------
# Private guards
# ---------------------------------------------------------------------------

[private]
_require cmd install_hint:
    @command -v {{ cmd }} &>/dev/null || { echo "{{ cmd }} not installed — run: {{ install_hint }}"; exit 1; }

[private]
_require-nightly:
    @rustup run nightly rustfmt --version &>/dev/null || { echo "nightly toolchain required — run: rustup toolchain install nightly --component rustfmt"; exit 1; }

# ---------------------------------------------------------------------------
# Build & Test
# ---------------------------------------------------------------------------

[group('build')]
build:
    cargo build --all-targets

[group('test')]
test:
    cargo test --all-targets

[group('test')]
test-doc:
    cargo test --doc --all-features

# ---------------------------------------------------------------------------
# Quality Checks
# ---------------------------------------------------------------------------

[group('check')]
check: check-fmt check-clippy check-deny

[group('check')]
check-fmt: _require-nightly
    cargo +nightly fmt --all -- --check

[group('check')]
check-clippy:
    cargo clippy --all-targets --all-features -- -D warnings

[group('check')]
check-deny: (_require "cargo-deny" "cargo install cargo-deny --locked")
    cargo deny check advisories licenses bans

[group('check')]
check-toml: (_require "taplo" "cargo install taplo-cli --locked")
    taplo fmt --check

[group('check')]
lint: check

# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

[group('format')]
fmt: _require-nightly
    cargo +nightly fmt --all

[group('format')]
fmt-toml: (_require "taplo" "cargo install taplo-cli --locked")
    taplo fmt

[group('format')]
fmt-all: fmt fmt-toml

# ---------------------------------------------------------------------------
# Development
# ---------------------------------------------------------------------------

[group('dev')]
dev: (_require "cargo-watch" "cargo install cargo-watch --locked")
    cargo watch -x check -x 'test --lib'

[group('dev')]
install-hooks: (_require "lefthook" "brew install lefthook")
    lefthook install

[group('dev')]
clean:
    cargo clean

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

[group('test')]
coverage: (_require "cargo-llvm-cov" "cargo install cargo-llvm-cov --locked")
    cargo llvm-cov --all-targets --summary-only

[group('test')]
coverage-html: (_require "cargo-llvm-cov" "cargo install cargo-llvm-cov --locked")
    cargo llvm-cov --all-targets --html --open

[group('test')]
coverage-json: (_require "cargo-llvm-cov" "cargo install cargo-llvm-cov --locked")
    cargo llvm-cov --all-targets --codecov --output-path codecov.json

# ---------------------------------------------------------------------------
# CI
# ---------------------------------------------------------------------------

[group('ci')]
ci: check-fmt check-clippy build check-deny test test-doc
    @echo ""
    @echo "All CI jobs passed."

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

[group('setup')]
setup:
    #!/usr/bin/env bash
    echo "Installing development tools..."
    rustup component add clippy
    rustup toolchain install nightly --component rustfmt
    cargo install cargo-watch cargo-deny cargo-nextest cargo-llvm-cov --locked
    rustup component add llvm-tools-preview
    if command -v brew &>/dev/null; then
        brew install taplo lefthook
    else
        cargo install taplo-cli --locked
    fi
    echo "Done. Run 'just check' to verify."
