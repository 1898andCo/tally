# Story: Rename HOMEBREW_TAP_TOKEN to RELEASE_PAT

Status: backlog
Repository: `1898andCo/tally`
Date: 2026-03-14

---

## Problem Statement

The `HOMEBREW_TAP_TOKEN` GitHub secret is used for two distinct purposes:
1. Pushing the Homebrew formula to `1898andCo/homebrew-tap` (original purpose)
2. Syncing the `main` branch during releases (added in v0.5.0 to bypass branch protection rulesets)

The name is misleading — it implies the secret is only for Homebrew, but it's now a general-purpose release PAT. This creates confusion for future maintainers and violates the principle of least surprise.

---

## Solution

Rename `HOMEBREW_TAP_TOKEN` to `RELEASE_PAT` across all references, create the new secret with the same token value, and delete the old secret.

---

## Story

As a **maintainer** reading the release workflow,
I want **secrets to be named for their actual purpose**,
So that **I understand what each secret does without reading the workflow history**.

---

## Acceptance Criteria

### AC-1: Workflow references updated
- `.github/workflows/release.yml` uses `secrets.RELEASE_PAT` in both the Homebrew update and sync-main steps

### AC-2: Documentation updated
- `docs/story-finding-mutability.md` required secrets table lists `RELEASE_PAT` with description "PAT with write access to tally repo and homebrew-tap (used for Homebrew formula push and main branch sync)"
- `_bmad-output/implementation-artifacts/story-finding-mutability.md` matches

### AC-3: Secret rotated
- New secret `RELEASE_PAT` created with same token value
- Old secret `HOMEBREW_TAP_TOKEN` deleted
- Release workflow tested (trigger a pre-release tag to verify)

---

## Tasks

- [ ] 1. Create branch `chore/rename-release-pat` from `develop`
- [ ] 2. Update `.github/workflows/release.yml`: replace `HOMEBREW_TAP_TOKEN` → `RELEASE_PAT` (2 occurrences)
- [ ] 3. Update `docs/story-finding-mutability.md`: update secrets table (2 occurrences)
- [ ] 4. Update `_bmad-output/implementation-artifacts/story-finding-mutability.md`: same changes
- [ ] 5. PR, merge to develop
- [ ] 6. Create secret: `gh secret set RELEASE_PAT --repo 1898andCo/tally` (paste same token value)
- [ ] 7. Verify: `gh secret list --repo 1898andCo/tally` shows both secrets
- [ ] 8. Test: push a pre-release tag (e.g., `v0.5.1-rc1`) to verify workflow uses new secret
- [ ] 9. Delete old secret: `gh secret delete HOMEBREW_TAP_TOKEN --repo 1898andCo/tally`

---

## Estimated Scope

- 4 files changed, ~6 lines each
- 1 secret created, 1 secret deleted
- Total: < 15 minutes
