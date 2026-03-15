//! Git-backed findings store using one-file-per-finding on an orphan branch.
//!
//! All operations use `git2` plumbing — no working tree checkout, no HEAD
//! modification. The `findings-data` branch is an orphan branch that holds
//! `findings/<uuid>.json` files and a regenerable `index.json`.
//!
//! Deep research (Mar 2026) confirmed:
//! - `TreeUpdateBuilder` handles multi-level paths (avoids manual `TreeBuilder` recursion)
//! - One-file-per-finding has zero merge conflicts for concurrent writes
//! - `ErrorCode::Locked` (code -14) requires manual retry (libgit2 doesn't retry)
//! - `repo.head_unborn()` detects repos with no initial commits

use std::path::Path;
use std::thread;
use std::time::Duration;

use git2::{BranchType, ErrorCode, FileMode, Repository, Signature};

use crate::error::{Result, TallyError};
use crate::model::Finding;

/// Default branch name for findings storage.
const FINDINGS_BRANCH: &str = "findings-data";

/// Directory within the findings branch that holds individual finding JSON files.
const FINDINGS_DIR: &str = "findings";

/// Max retry attempts for ref lock contention.
const MAX_LOCK_RETRIES: u32 = 3;

/// Build `RemoteCallbacks` with a credential chain:
/// 1. git credential helper (`~/.gitconfig` — osxkeychain/manager/store, `gh auth setup-git`)
/// 2. `GITHUB_TOKEN` / `GIT_TOKEN` environment variable
/// 3. SSH agent (Unix `ssh-agent`, Windows OpenSSH agent)
/// 4. SSH key from default paths (`~/.ssh/id_ed25519`, `~/.ssh/id_rsa`)
///
/// Tracks attempt count to avoid libgit2's infinite retry loop.
fn build_remote_callbacks() -> git2::RemoteCallbacks<'static> {
    let attempts = std::cell::Cell::new(0u32);
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        let attempt = attempts.get();
        if attempt >= 4 {
            return Err(git2::Error::from_str(
                "Authentication failed: exhausted all credential strategies",
            ));
        }
        attempts.set(attempt + 1);

        let username = username_from_url.unwrap_or("git");

        // Strategy 1: git credential helper (works cross-platform:
        // macOS osxkeychain, Windows GCM, Linux store/cache, gh auth setup-git)
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            if let Ok(config) = git2::Config::open_default() {
                if let Ok(cred) = git2::Cred::credential_helper(&config, url, username_from_url) {
                    return Ok(cred);
                }
            }

            // Strategy 2: environment variable (CI/Actions, headless)
            if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GIT_TOKEN"))
            {
                return git2::Cred::userpass_plaintext("git", &token);
            }
        }

        // Strategy 3: SSH agent (Unix ssh-agent, Windows OpenSSH agent via SSH_AUTH_SOCK)
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                return Ok(cred);
            }

            // Strategy 4: SSH key from default file paths (no agent needed)
            if let Some(home) = home::home_dir() {
                let ssh_dir = home.join(".ssh");
                for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
                    let privkey = ssh_dir.join(key_name);
                    if privkey.exists() {
                        let pubkey = ssh_dir.join(format!("{key_name}.pub"));
                        let pubkey_ref = if pubkey.exists() {
                            Some(pubkey.as_path())
                        } else {
                            None
                        };
                        if let Ok(cred) = git2::Cred::ssh_key(username, pubkey_ref, &privkey, None)
                        {
                            return Ok(cred);
                        }
                    }
                }
            }
        }

        Err(git2::Error::from_str("No suitable credentials found"))
    });
    callbacks
}

/// Build `FetchOptions` with auth callbacks.
fn build_fetch_options() -> git2::FetchOptions<'static> {
    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(build_remote_callbacks());
    opts
}

/// Build `PushOptions` with auth callbacks.
fn build_push_options() -> git2::PushOptions<'static> {
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(build_remote_callbacks());
    opts
}

/// Wrap auth errors with actionable, platform-specific guidance.
fn wrap_auth_error(e: git2::Error, remote_name: &str) -> TallyError {
    if e.code() == git2::ErrorCode::Auth
        || e.message().contains("authentication")
        || e.message().contains("credential")
    {
        let helper_hint = if cfg!(target_os = "macos") {
            "git config --global credential.helper osxkeychain"
        } else if cfg!(target_os = "windows") {
            "git config --global credential.helper manager"
        } else {
            "git config --global credential.helper store"
        };
        TallyError::Git(git2::Error::from_str(&format!(
            "Authentication failed for remote '{remote_name}'. Configure credentials with one of:\n  \
             - gh auth setup-git  (recommended)\n  \
             - {helper_hint}\n  \
             - Set GITHUB_TOKEN environment variable\n  \
             - Add SSH key to ~/.ssh/ (id_ed25519 or id_rsa)"
        )))
    } else {
        TallyError::Git(e)
    }
}

/// Result of a sync operation.
#[derive(Debug)]
pub struct SyncResult {
    /// Whether remote data was fetched.
    pub fetched: bool,
    /// Whether a merge was performed.
    pub merged: bool,
    /// Whether local data was pushed.
    pub pushed: bool,
}

/// Git-backed findings store.
pub struct GitFindingsStore {
    repo: Repository,
    branch_name: String,
}

impl GitFindingsStore {
    /// Open a findings store for the repository at `repo_path`.
    ///
    /// Does NOT create the findings branch — call [`init`](Self::init) first.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the repository cannot be opened.
    pub fn open(repo_path: &str) -> Result<Self> {
        let repo = Repository::open(repo_path)?;
        Ok(Self {
            repo,
            branch_name: FINDINGS_BRANCH.to_string(),
        })
    }

    /// Initialize the findings branch as an orphan branch.
    ///
    /// Creates the branch with a `schema.json` and empty `findings/` directory.
    /// Idempotent — returns `Ok` if the branch already exists.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if branch creation or commit fails.
    #[tracing::instrument(skip_all)]
    pub fn init(&self) -> Result<()> {
        // Check if branch already exists
        if self.branch_exists() {
            return Ok(());
        }

        let schema_content = serde_json::json!({
            "version": "1.1.0",
            "format": "one-file-per-finding",
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let schema_blob = self.repo.blob(
            serde_json::to_string_pretty(&schema_content)
                .map_err(TallyError::Serialization)?
                .as_bytes(),
        )?;

        // Create empty findings directory with a .gitkeep
        let gitkeep_blob = self.repo.blob(b"")?;

        // Create .gitattributes for merge strategy
        let gitattributes_blob = self.repo.blob(b"index.json merge=ours\n")?;

        // Build tree: findings/.gitkeep + schema.json + .gitattributes
        let mut builder = git2::build::TreeUpdateBuilder::new();
        builder.upsert(
            format!("{FINDINGS_DIR}/.gitkeep"),
            gitkeep_blob,
            FileMode::Blob,
        );
        builder.upsert("schema.json", schema_blob, FileMode::Blob);
        builder.upsert(".gitattributes", gitattributes_blob, FileMode::Blob);

        // For TreeUpdateBuilder::create_updated we need a baseline tree.
        // Since this is an orphan branch, create an empty tree first.
        let empty_tree_oid = self.repo.treebuilder(None)?.write()?;
        let empty_tree = self.repo.find_tree(empty_tree_oid)?;

        let new_tree_oid = builder.create_updated(&self.repo, &empty_tree)?;
        let new_tree = self.repo.find_tree(new_tree_oid)?;

        let sig = self.signature()?;

        // Create orphan commit (no parents)
        let commit_oid = self.repo.commit(
            None, // Don't update HEAD
            &sig,
            &sig,
            "Initialize tally findings store",
            &new_tree,
            &[], // Empty parents = orphan
        )?;

        // Create the branch reference
        let commit = self.repo.find_commit(commit_oid)?;
        self.repo.branch(&self.branch_name, &commit, false)?;

        Ok(())
    }

    /// Save a finding as `findings/<uuid>.json` on the findings branch.
    ///
    /// Creates a new commit on the branch with the finding file added.
    /// If a finding with the same UUID already exists, it is overwritten (update).
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Serialization` if the finding cannot be serialized,
    /// or `TallyError::Git`/`TallyError::BranchNotFound` if git operations fail.
    #[tracing::instrument(skip_all, fields(uuid = %finding.uuid))]
    pub fn save_finding(&self, finding: &Finding) -> Result<()> {
        let file_path = format!("{FINDINGS_DIR}/{}.json", finding.uuid);
        let content = serde_json::to_string_pretty(finding).map_err(TallyError::Serialization)?;

        self.upsert_file(
            &file_path,
            content.as_bytes(),
            &format!("Save finding {}", finding.uuid),
        )
    }

    /// Load a single finding by UUID.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the file doesn't exist on the branch,
    /// or `TallyError::Serialization` if the JSON is malformed.
    #[tracing::instrument(skip_all, fields(uuid = %uuid))]
    pub fn load_finding(&self, uuid: &uuid::Uuid) -> Result<Finding> {
        let file_path = format!("{FINDINGS_DIR}/{uuid}.json");
        let content = self.read_file(&file_path)?;
        let finding: Finding =
            serde_json::from_slice(&content).map_err(TallyError::Serialization)?;
        Ok(finding)
    }

    /// Load all findings from the branch.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::BranchNotFound` if the findings branch doesn't exist,
    /// or `TallyError::Git` if git operations fail. Malformed individual findings
    /// are logged to stderr and skipped (not returned as errors).
    #[tracing::instrument(skip_all)]
    pub fn load_all(&self) -> Result<Vec<Finding>> {
        let filenames = self.list_directory(FINDINGS_DIR)?;
        let mut findings = Vec::new();

        for name in &filenames {
            if name == ".gitkeep" || Path::new(name).extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let file_path = format!("{FINDINGS_DIR}/{name}");
            match self.read_file(&file_path) {
                Ok(content) => match serde_json::from_slice::<Finding>(&content) {
                    Ok(finding) => findings.push(finding),
                    Err(e) => {
                        tracing::warn!(name, error = %e, "Skipping malformed finding");
                    }
                },
                Err(e) => {
                    tracing::warn!(name, error = %e, "Failed to read finding");
                }
            }
        }

        Ok(findings)
    }

    /// Rebuild the `index.json` file from all finding files.
    ///
    /// Scans every `findings/<uuid>.json`, extracts metadata (uuid, severity,
    /// status, file, rule, fingerprint), and writes a single `index.json` file
    /// for fast queries. The index is always regenerable from finding files.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if git operations fail or branch doesn't exist.
    #[tracing::instrument(skip_all)]
    pub fn rebuild_index(&self) -> Result<()> {
        let findings = self.load_all()?;

        let index_entries: Vec<serde_json::Value> = findings
            .iter()
            .map(|f| {
                let primary_file = f.locations.first().map_or("", |l| l.file_path.as_str());
                serde_json::json!({
                    "uuid": f.uuid.to_string(),
                    "severity": f.severity,
                    "status": f.status,
                    "rule_id": f.rule_id,
                    "file_path": primary_file,
                    "fingerprint": f.content_fingerprint,
                    "title": f.title,
                    "tags": f.tags,
                    "created_at": f.created_at.to_rfc3339(),
                    "updated_at": f.updated_at.to_rfc3339(),
                })
            })
            .collect();

        let index = serde_json::json!({
            "version": "1.0.0",
            "count": index_entries.len(),
            "findings": index_entries,
        });

        let content = serde_json::to_string_pretty(&index).map_err(TallyError::Serialization)?;

        self.upsert_file("index.json", content.as_bytes(), "Rebuild findings index")
    }

    /// Detect git context (`repo_id`, branch, `commit_sha`) from the current repository.
    ///
    /// Returns (`repo_id`, branch, `commit_sha`) where:
    /// - `repo_id` is derived from the origin remote URL, or empty if no remote
    /// - `branch` is the current HEAD branch name, or `None` if HEAD is detached/unborn
    /// - `commit_sha` is the current HEAD commit SHA, or `None` if HEAD is unborn
    #[must_use]
    pub fn git_context(&self) -> (String, Option<String>, Option<String>) {
        let repo_id = self
            .repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().map(String::from))
            .unwrap_or_default();

        let branch = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from));

        let commit_sha = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| c.id().to_string());

        (repo_id, branch, commit_sha)
    }

    /// Check if the findings branch exists.
    #[must_use]
    pub fn branch_exists(&self) -> bool {
        self.repo
            .find_branch(&self.branch_name, BranchType::Local)
            .is_ok()
    }

    /// Check if any remote has a findings-data branch (i.e., findings have been pushed).
    #[must_use]
    pub fn has_remote_branch(&self) -> bool {
        let remote_ref = format!("refs/remotes/origin/{}", self.branch_name);
        self.repo.find_reference(&remote_ref).is_ok()
    }

    /// Sync the findings branch with the remote (fetch + merge + push).
    ///
    /// Fetches the remote findings-data branch, fast-forward merges if possible,
    /// then pushes local changes. Retries push on non-fast-forward (up to 3 attempts
    /// with fetch+merge between retries).
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if remote operations fail (auth, network, etc.).
    /// Returns `TallyError::BranchNotFound` if the local branch doesn't exist.
    #[tracing::instrument(skip_all, fields(remote = remote_name))]
    #[allow(clippy::too_many_lines)] // sync has inherent complexity: fetch + merge + push + retry
    pub fn sync(&self, remote_name: &str) -> Result<SyncResult> {
        if !self.branch_exists() {
            return Err(TallyError::BranchNotFound {
                branch: self.branch_name.clone(),
            });
        }

        let ref_name = format!("refs/heads/{}", self.branch_name);
        let remote_ref = format!("refs/remotes/{}/{}", remote_name, self.branch_name);

        // Fetch the remote branch
        let mut remote = self.repo.find_remote(remote_name)?;
        let mut fetch_opts = build_fetch_options();
        remote
            .fetch(&[&self.branch_name], Some(&mut fetch_opts), None)
            .map_err(|e| wrap_auth_error(e, remote_name))?;

        // Check if remote branch exists
        let remote_commit = match self.repo.find_reference(&remote_ref) {
            Ok(reference) => Some(reference.peel_to_commit()?),
            Err(_) => None, // Remote branch doesn't exist yet — first push
        };

        let local_commit = self.branch_tip()?;

        // Merge remote into local (fast-forward if possible)
        let mut merged = false;
        if let Some(ref remote_commit) = remote_commit {
            if remote_commit.id() != local_commit.id() {
                // Check if local is ancestor of remote (remote is ahead — fast-forward)
                if self
                    .repo
                    .graph_descendant_of(remote_commit.id(), local_commit.id())?
                {
                    // Fast-forward local to remote
                    self.repo.reference(
                        &ref_name,
                        remote_commit.id(),
                        true,
                        "tally sync: fast-forward to remote",
                    )?;
                    tracing::info!(
                        branch = %self.branch_name,
                        commit = %&remote_commit.id().to_string()[..8],
                        "Fast-forwarded to remote"
                    );
                    merged = true;
                } else if self
                    .repo
                    .graph_descendant_of(local_commit.id(), remote_commit.id())?
                {
                    // Local is ahead of remote — just push
                    tracing::info!("Local ahead of remote, pushing");
                } else {
                    // Diverged — one-file-per-finding means git merge should auto-resolve
                    tracing::info!("Branches diverged, merging");
                    let merge_base = self
                        .repo
                        .merge_base(local_commit.id(), remote_commit.id())?;
                    let base_commit = self.repo.find_commit(merge_base)?;

                    let base_tree = base_commit.tree()?;
                    let local_tree = local_commit.tree()?;
                    let remote_tree = remote_commit.tree()?;

                    let mut merge_index =
                        self.repo
                            .merge_trees(&base_tree, &local_tree, &remote_tree, None)?;

                    if merge_index.has_conflicts() {
                        return Err(TallyError::Git(git2::Error::from_str(
                            "Merge conflict on findings-data branch. \
                             This should not happen with one-file-per-finding. \
                             Resolve manually with: git checkout findings-data && git merge origin/findings-data",
                        )));
                    }

                    let merged_tree_oid = merge_index.write_tree_to(&self.repo)?;
                    let merged_tree = self.repo.find_tree(merged_tree_oid)?;
                    let sig = self.signature()?;

                    let merge_commit = self.repo.commit(
                        Some(&ref_name),
                        &sig,
                        &sig,
                        "tally sync: merge remote findings",
                        &merged_tree,
                        &[&local_commit, remote_commit],
                    )?;
                    tracing::info!(commit = %&merge_commit.to_string()[..8], "Merged remote changes");
                    merged = true;
                }
            }
        }

        // Push with retry
        for attempt in 0..MAX_LOCK_RETRIES {
            let mut push_remote = self.repo.find_remote(remote_name)?;
            let mut push_opts = build_push_options();
            match push_remote.push(&[&format!("{ref_name}:{ref_name}")], Some(&mut push_opts)) {
                Ok(()) => {
                    return Ok(SyncResult {
                        fetched: remote_commit.is_some(),
                        merged,
                        pushed: true,
                    });
                }
                Err(e) if attempt < MAX_LOCK_RETRIES - 1 => {
                    let delay = Duration::from_millis(100 * u64::from(2_u32.pow(attempt)));
                    tracing::warn!(
                        attempt = attempt + 1,
                        max = MAX_LOCK_RETRIES,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Push failed, retrying"
                    );
                    thread::sleep(delay);

                    // Re-fetch and try to merge again before retry
                    let mut fetch_remote = self.repo.find_remote(remote_name)?;
                    let mut retry_fetch_opts = build_fetch_options();
                    fetch_remote.fetch(&[&self.branch_name], Some(&mut retry_fetch_opts), None)?;
                }
                Err(e) => return Err(wrap_auth_error(e, remote_name)),
            }
        }

        unreachable!("retry loop should have returned")
    }

    // --- Private helpers ---

    /// Get the branch tip commit.
    fn branch_tip(&self) -> Result<git2::Commit<'_>> {
        let branch = self
            .repo
            .find_branch(&self.branch_name, BranchType::Local)
            .map_err(|_| TallyError::BranchNotFound {
                branch: self.branch_name.clone(),
            })?;
        let commit = branch.into_reference().peel_to_commit()?;
        Ok(commit)
    }

    /// Read a file from the findings branch without checkout.
    fn read_file(&self, file_path: &str) -> Result<Vec<u8>> {
        let commit = self.branch_tip()?;
        let tree = commit.tree()?;
        let entry = tree.get_path(Path::new(file_path))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
    }

    /// List all entries in a directory on the findings branch.
    fn list_directory(&self, dir_path: &str) -> Result<Vec<String>> {
        let commit = self.branch_tip()?;
        let tree = commit.tree()?;
        let dir_entry = tree.get_path(Path::new(dir_path))?;
        let dir_tree = self.repo.find_tree(dir_entry.id())?;

        let mut entries = Vec::new();
        for entry in &dir_tree {
            if let Some(name) = entry.name() {
                entries.push(name.to_string());
            }
        }
        Ok(entries)
    }

    /// Upsert a file on the findings branch and commit.
    ///
    /// Uses `TreeUpdateBuilder` for multi-level path handling and
    /// retries on ref lock contention or compare-and-swap failure
    /// (libgit2 doesn't retry automatically).
    fn upsert_file(&self, file_path: &str, content: &[u8], message: &str) -> Result<()> {
        let blob_oid = self.repo.blob(content)?;
        let sig = self.signature()?;
        let ref_name = format!("refs/heads/{}", self.branch_name);

        // Retry on ref lock contention or compare-and-swap (Modified) failure.
        // Blob creation + tree building are INSIDE the loop so they use the
        // fresh parent tree on Modified retries.
        for attempt in 0..MAX_LOCK_RETRIES {
            let commit = self.branch_tip()?;
            let parent_tree = commit.tree()?;

            let mut builder = git2::build::TreeUpdateBuilder::new();
            builder.upsert(file_path, blob_oid, FileMode::Blob);
            let new_tree_oid = builder.create_updated(&self.repo, &parent_tree)?;
            let new_tree = self.repo.find_tree(new_tree_oid)?;

            match self
                .repo
                .commit(Some(&ref_name), &sig, &sig, message, &new_tree, &[&commit])
            {
                Ok(_) => return Ok(()),
                Err(e)
                    if (e.code() == ErrorCode::Locked || e.code() == ErrorCode::Modified)
                        && attempt < MAX_LOCK_RETRIES - 1 =>
                {
                    let reason = if e.code() == ErrorCode::Modified {
                        "Ref modified (compare-and-swap), retrying"
                    } else {
                        "Ref lock contention, retrying"
                    };
                    let delay = Duration::from_millis(100 * u64::from(2_u32.pow(attempt)));
                    tracing::warn!(
                        branch = %self.branch_name,
                        attempt = attempt + 1,
                        max = MAX_LOCK_RETRIES,
                        delay_ms = delay.as_millis(),
                        reason,
                    );
                    thread::sleep(delay);
                }
                Err(e) => return Err(e.into()),
            }
        }

        unreachable!("retry loop should have returned")
    }

    /// Remove a file from the findings branch and commit.
    fn remove_file(&self, file_path: &str, message: &str) -> Result<()> {
        let sig = self.signature()?;
        let ref_name = format!("refs/heads/{}", self.branch_name);

        for attempt in 0..MAX_LOCK_RETRIES {
            let commit = self.branch_tip()?;
            let parent_tree = commit.tree()?;

            let mut builder = git2::build::TreeUpdateBuilder::new();
            builder.remove(file_path);
            let new_tree_oid = builder.create_updated(&self.repo, &parent_tree)?;
            let new_tree = self.repo.find_tree(new_tree_oid)?;

            match self
                .repo
                .commit(Some(&ref_name), &sig, &sig, message, &new_tree, &[&commit])
            {
                Ok(_) => return Ok(()),
                Err(e)
                    if (e.code() == ErrorCode::Locked || e.code() == ErrorCode::Modified)
                        && attempt < MAX_LOCK_RETRIES - 1 =>
                {
                    let delay = Duration::from_millis(100 * u64::from(2_u32.pow(attempt)));
                    thread::sleep(delay);
                }
                Err(e) => return Err(e.into()),
            }
        }

        unreachable!("retry loop should have returned")
    }

    // --- Public wrappers for registry/store use ---

    /// Public wrapper for `upsert_file`. Used by the rule registry.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if git operations fail.
    pub fn upsert_file_pub(&self, file_path: &str, content: &[u8], message: &str) -> Result<()> {
        self.upsert_file(file_path, content, message)
    }

    /// Public wrapper for `read_file`. Used by the rule registry.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the file doesn't exist or git fails.
    pub fn read_file_pub(&self, file_path: &str) -> Result<Vec<u8>> {
        self.read_file(file_path)
    }

    /// Public wrapper for `list_directory`. Used by the rule registry.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the directory doesn't exist or git fails.
    pub fn list_directory_pub(&self, dir_path: &str) -> Result<Vec<String>> {
        self.list_directory(dir_path)
    }

    /// Public wrapper for `remove_file`. Used by the rule registry.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if git operations fail.
    pub fn remove_file_pub(&self, file_path: &str, message: &str) -> Result<()> {
        self.remove_file(file_path, message)
    }

    /// Create a git signature for commits.
    fn signature(&self) -> Result<Signature<'_>> {
        // Try to use the repo's configured user, fall back to tally defaults
        self.repo
            .signature()
            .or_else(|_| Signature::now("tally", "tally@localhost"))
            .map_err(Into::into)
    }
}
