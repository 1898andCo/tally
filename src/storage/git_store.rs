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

use crate::error::{TallyError, Result};
use crate::model::Finding;

/// Default branch name for findings storage.
const FINDINGS_BRANCH: &str = "findings-data";

/// Directory within the findings branch that holds individual finding JSON files.
const FINDINGS_DIR: &str = "findings";

/// Max retry attempts for ref lock contention.
const MAX_LOCK_RETRIES: u32 = 3;

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
    pub fn init(&self) -> Result<()> {
        // Check if branch already exists
        if self.branch_exists() {
            return Ok(());
        }

        let schema_content = serde_json::json!({
            "version": "1.0.0",
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

        // Build tree: findings/.gitkeep + schema.json
        let mut builder = git2::build::TreeUpdateBuilder::new();
        builder.upsert(
            format!("{FINDINGS_DIR}/.gitkeep"),
            gitkeep_blob,
            FileMode::Blob,
        );
        builder.upsert("schema.json", schema_blob, FileMode::Blob);

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
    pub fn save_finding(&self, finding: &Finding) -> Result<()> {
        let file_path = format!("{FINDINGS_DIR}/{}.json", finding.uuid);
        let content = serde_json::to_string_pretty(finding)
            .map_err(TallyError::Serialization)?;

        self.upsert_file(&file_path, content.as_bytes(), &format!("Save finding {}", finding.uuid))
    }

    /// Load a single finding by UUID.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the file doesn't exist on the branch,
    /// or `TallyError::Serialization` if the JSON is malformed.
    pub fn load_finding(&self, uuid: &uuid::Uuid) -> Result<Finding> {
        let file_path = format!("{FINDINGS_DIR}/{uuid}.json");
        let content = self.read_file(&file_path)?;
        let finding: Finding = serde_json::from_slice(&content)
            .map_err(TallyError::Serialization)?;
        Ok(finding)
    }

    /// Load all findings from the branch.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::BranchNotFound` if the findings branch doesn't exist,
    /// or `TallyError::Git` if git operations fail. Malformed individual findings
    /// are logged to stderr and skipped (not returned as errors).
    pub fn load_all(&self) -> Result<Vec<Finding>> {
        let filenames = self.list_directory(FINDINGS_DIR)?;
        let mut findings = Vec::new();

        for name in &filenames {
            if name == ".gitkeep"
                || Path::new(name).extension().is_none_or(|ext| ext != "json")
            {
                continue;
            }

            let file_path = format!("{FINDINGS_DIR}/{name}");
            match self.read_file(&file_path) {
                Ok(content) => {
                    match serde_json::from_slice::<Finding>(&content) {
                        Ok(finding) => findings.push(finding),
                        Err(e) => {
                            eprintln!("WARNING: skipping malformed finding {name}: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("WARNING: failed to read {name}: {e}");
                }
            }
        }

        Ok(findings)
    }

    /// Check if the findings branch exists.
    #[must_use]
    pub fn branch_exists(&self) -> bool {
        self.repo
            .find_branch(&self.branch_name, BranchType::Local)
            .is_ok()
    }

    // --- Private helpers ---

    /// Get the branch tip commit.
    fn branch_tip(&self) -> Result<git2::Commit<'_>> {
        let branch = self.repo
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
    /// retries on ref lock contention (libgit2 doesn't retry automatically).
    fn upsert_file(&self, file_path: &str, content: &[u8], message: &str) -> Result<()> {
        let blob_oid = self.repo.blob(content)?;

        let commit = self.branch_tip()?;
        let parent_tree = commit.tree()?;

        let mut builder = git2::build::TreeUpdateBuilder::new();
        builder.upsert(file_path, blob_oid, FileMode::Blob);
        let new_tree_oid = builder.create_updated(&self.repo, &parent_tree)?;
        let new_tree = self.repo.find_tree(new_tree_oid)?;

        let sig = self.signature()?;
        let ref_name = format!("refs/heads/{}", self.branch_name);

        // Retry on ref lock contention
        for attempt in 0..MAX_LOCK_RETRIES {
            match self.repo.commit(
                Some(&ref_name),
                &sig,
                &sig,
                message,
                &new_tree,
                &[&commit],
            ) {
                Ok(_) => return Ok(()),
                Err(e) if e.code() == ErrorCode::Locked && attempt < MAX_LOCK_RETRIES - 1 => {
                    let delay = Duration::from_millis(100 * u64::from(2_u32.pow(attempt)));
                    eprintln!(
                        "Ref lock contention on {}, retry {}/{} after {}ms",
                        self.branch_name,
                        attempt + 1,
                        MAX_LOCK_RETRIES,
                        delay.as_millis()
                    );
                    thread::sleep(delay);
                }
                Err(e) => return Err(e.into()),
            }
        }

        unreachable!("retry loop should have returned")
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
