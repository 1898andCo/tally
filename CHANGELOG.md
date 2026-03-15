# Changelog

All notable changes to this project will be documented in this file.

## [0.7.0] - 2026-03-15

### Bug Fixes

- *(build)* Fix cargo-llvm-cov path detection for Homebrew Rust
- *(build)* Allow permissive licenses from fastembed transitive deps

### Documentation

- *(config)* Add comment explaining typos allowlist entry
- *(release)* Update docs for v0.7.0, add rule registry section
- *(specs)* Update story 1.2 with deviation and completion record

### Features

- *(registry)* Add rule data model, storage, and upsert_file fix
- *(registry)* Integrate matcher into record flow, fix MCP dedup
- *(registry)* Add scope enforcement with glob matching
- *(cli)* Add tally rule subcommands
- *(mcp)* Add 8 rule registry tools and docs resource
- *(registry)* Add migration, init upgrade, and rebuild-index rules
- *(sync)* Add rule conflict resolution via semantic merge
- *(registry)* Add semantic search via fastembed (feature-gated)
- *(mcp)* Add 5 resources, batch tool, 3 prompts, enhance 4 prompts

### Refactor

- *(query)* Replace hand-rolled levenshtein with strsim

### Testing

- *(registry)* Add comprehensive tests for rule registry
- *(registry)* Add semantic search tests (feature-gated)
- *(mcp)* Add comprehensive tests for enhanced resources, tool, prompts

## [0.6.1] - 2026-03-15

### Features

- *(mcp)* Add TallyQL syntax reference as MCP resource

## [0.6.0] - 2026-03-15

### Bug Fixes

- *(ci)* Use RELEASE_PAT for sync-main (scoped to tally repo)

### Documentation

- Add TallyQL syntax reference and update query documentation

### Features

- *(query)* Add TallyQL module foundation — AST, error types, field registry
- *(query)* Implement TallyQL parser with Chumsky 0.10
- *(query)* Implement TallyQL AST evaluator with property tests
- *(query)* Wire TallyQL + enhanced filters into CLI
- *(query)* Add TallyQL and enhanced filters to MCP query_findings tool

### Testing

- *(query)* Add 48 foundation tests for field registry, filters, and sorting
- *(query)* Add 26 parser tests + fix IN list minimum
- *(query)* Add 27 evaluator tests + 9 E2E parse-to-eval pipeline tests
- *(query)* Add 15 CLI tests + 2 E2E lifecycle tests for full coverage

## [0.5.1] - 2026-03-15

### Bug Fixes

- *(ci)* Use HOMEBREW_TAP_TOKEN for sync-main and fix merge ref
- *(ci)* Add livecheck block to Homebrew formula template
- *(ci)* Add Homebrew livecheck and fix cargo +nightly compatibility

### Documentation

- Add story for renaming HOMEBREW_TAP_TOKEN to RELEASE_PAT

## [0.5.0] - 2026-03-14

### Bug Fixes

- *(storage)* Add git2 credential callbacks to sync operations
- *(storage)* Make auth cross-platform with SSH key fallback

### Documentation

- Add project foundation files and BMAD framework
- Remediate all story spec gaps
- Add upgrade instructions and v0.4.0→v0.5.0 migration note

### Features

- *(model)* Add Note, FieldEdit structs and edit_field/add_note methods
- *(state)* Add deferred/suppressed to reopened transitions
- *(mcp)* Add update_finding, add_note, add_tag, remove_tag tools
- *(cli)* Add update-fields, note, tag commands and query --tag filter
- *(export)* Add SARIF property bags, stats enhancements, schema v1.1.0

### Refactor

- *(cli)* Split handlers.rs into per-handler modules
- *(test)* Split cli_test.rs into 6 focused test files

### Testing

- Add 22 tests covering all v0.5.0 gaps

## [0.4.0] - 2026-03-14

### Documentation

- *(mcp)* Improve CLI help, tool descriptions, prompts, add PR resource (#6)

## [0.3.0] - 2026-03-14

### CI/CD

- *(release)* Sync main branch on non-prerelease tags

### Features

- *(mcp)* Expose all Finding fields in MCP tools (#5)
- *(mcp)* Expose all Finding fields in MCP tools (#5)

## [0.2.0] - 2026-03-14

### Bug Fixes

- *(release)* Compute homebrew sha256 directly instead of bump-action

### Features

- *(mcp)* Full CLI parity — 5 new MCP tools (#4)

## [0.1.0] - 2026-03-14

### Bug Fixes

- *(release)* Drop x86_64-apple-darwin target, macos-13 deprecated
- *(release)* Rename crate to tally-ng, binary stays tally

### Documentation

- *(readme)* Update for v0.1.0 with all features and Windows build

### Features

- Scaffold tally project with full config and story spec
- *(model)* Finding data model, state machine, identity resolution (#1)
- *(release)* Add release pipeline with changelog, binaries, brew, crates.io (#3)


