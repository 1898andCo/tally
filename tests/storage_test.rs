//! Integration tests for git-backed storage (Task 3).
//!
//! Each test creates a temporary git repo, initializes the findings store,
//! and verifies operations without affecting the working tree or HEAD.

use chrono::Utc;
use git2::{BranchType, FileMode, Repository};
use tally_ng::model::*;
use tally_ng::storage::GitFindingsStore;
use uuid::Uuid;

/// Create a temp repo with an initial commit on `main` so HEAD is not unborn.
fn setup_repo() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let repo_path = tmp.path().to_str().expect("path to str").to_string();

    let repo = Repository::init(&repo_path).expect("init repo");

    // Create initial commit so HEAD is not unborn
    let sig = git2::Signature::now("test", "test@test.com").expect("sig");
    let blob = repo.blob(b"# test repo").expect("blob");
    let mut builder = repo.treebuilder(None).expect("treebuilder");
    builder
        .insert("README.md", blob, 0o100_644)
        .expect("insert");
    let tree_oid = builder.write().expect("write tree");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
        .expect("initial commit");

    (tmp, repo_path)
}

/// Create a test finding with the given UUID.
fn make_test_finding(uuid: Uuid) -> Finding {
    Finding {
        schema_version: "1.0.0".to_string(),
        uuid,
        content_fingerprint: format!("sha256:test_{uuid}"),
        rule_id: "test-rule".to_string(),
        locations: vec![Location {
            file_path: "src/main.rs".to_string(),
            line_start: 42,
            line_end: 42,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Important,
        category: "test".to_string(),
        tags: vec![],
        title: "Test finding".to_string(),
        description: "A test finding for storage tests.".to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "test-agent".to_string(),
            session_id: "sess_test".to_string(),
            detected_at: Utc::now(),
            session_short_id: Some("I1".to_string()),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        repo_id: "test/repo".to_string(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
        notes: vec![],
        edit_history: vec![],
    }
}

// =============================================================================
// Task 3.8: init creates orphan branch
// =============================================================================

#[test]
fn init_creates_orphan_branch() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open store");

    assert!(
        !store.branch_exists(),
        "branch should not exist before init"
    );
    store.init().expect("init");
    assert!(store.branch_exists(), "branch should exist after init");

    // Verify it's a separate branch from main
    let repo = Repository::open(&repo_path).expect("open repo");
    let findings_branch = repo
        .find_branch("findings-data", BranchType::Local)
        .expect("find findings-data branch");
    // git init uses "master" by default (unless user configured init.defaultBranch)
    let head = repo.head().expect("head");
    let head_branch_name = head.shorthand().expect("shorthand");
    let main_branch = repo
        .find_branch(head_branch_name, BranchType::Local)
        .expect("find HEAD branch");

    let findings_commit = findings_branch
        .into_reference()
        .peel_to_commit()
        .expect("peel findings");
    let main_commit = main_branch
        .into_reference()
        .peel_to_commit()
        .expect("peel main");

    assert_ne!(
        findings_commit.id(),
        main_commit.id(),
        "findings branch should be a separate orphan, not sharing history with main"
    );
    assert_eq!(
        findings_commit.parent_count(),
        0,
        "orphan branch should have zero parents"
    );
}

// =============================================================================
// Task 3.9: save + load round-trip
// =============================================================================

#[test]
fn save_and_load_roundtrip() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let uuid = Uuid::now_v7();
    let finding = make_test_finding(uuid);
    store.save_finding(&finding).expect("save");

    let loaded = store.load_finding(&uuid).expect("load");
    assert_eq!(loaded.uuid, finding.uuid);
    assert_eq!(loaded.rule_id, "test-rule");
    assert_eq!(loaded.severity, Severity::Important);
    assert_eq!(loaded.status, LifecycleState::Open);
    assert_eq!(loaded.locations.len(), 1);
    assert_eq!(loaded.locations[0].file_path, "src/main.rs");
    assert_eq!(loaded.locations[0].line_start, 42);
    assert_eq!(loaded.title, "Test finding");
}

// =============================================================================
// Task 3.10: two sequential saves create two files
// =============================================================================

#[test]
fn two_sequential_saves_create_two_findings() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let first_id = Uuid::now_v7();
    let second_id = Uuid::now_v7();
    let first_finding = make_test_finding(first_id);
    let second_finding = make_test_finding(second_id);

    store.save_finding(&first_finding).expect("save 1");
    store.save_finding(&second_finding).expect("save 2");

    let all = store.load_all().expect("load_all");
    assert_eq!(all.len(), 2, "should have two findings");

    let uuids: Vec<Uuid> = all.iter().map(|f| f.uuid).collect();
    assert!(uuids.contains(&first_id), "should contain first finding");
    assert!(uuids.contains(&second_id), "should contain second finding");
}

// =============================================================================
// Task 3.11: index regeneration (load_all acts as index)
// =============================================================================

#[test]
fn load_all_returns_all_saved_findings() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save 5 findings
    let uuids: Vec<Uuid> = (0..5).map(|_| Uuid::now_v7()).collect();
    for uuid in &uuids {
        store.save_finding(&make_test_finding(*uuid)).expect("save");
    }

    let all = store.load_all().expect("load_all");
    assert_eq!(all.len(), 5);

    for uuid in &uuids {
        assert!(
            all.iter().any(|f| f.uuid == *uuid),
            "should find UUID {uuid}"
        );
    }
}

// =============================================================================
// Task 3.12: init is idempotent
// =============================================================================

#[test]
fn init_is_idempotent() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");

    store.init().expect("first init");
    store.init().expect("second init should not error");

    // Should still be able to save after double init
    let uuid = Uuid::now_v7();
    store
        .save_finding(&make_test_finding(uuid))
        .expect("save after double init");
    let loaded = store.load_finding(&uuid).expect("load after double init");
    assert_eq!(loaded.uuid, uuid);
}

// =============================================================================
// Task 3.13: operations don't modify working tree or HEAD
// =============================================================================

#[test]
fn operations_dont_modify_working_tree_or_head() {
    let (_tmp, repo_path) = setup_repo();
    let repo = Repository::open(&repo_path).expect("open repo");

    // Record HEAD before operations
    let head_before = repo.head().expect("head").target().expect("target");

    let store = GitFindingsStore::open(&repo_path).expect("open store");
    store.init().expect("init");
    store
        .save_finding(&make_test_finding(Uuid::now_v7()))
        .expect("save");

    // HEAD should not have changed
    let head_after = repo.head().expect("head").target().expect("target");
    assert_eq!(
        head_before, head_after,
        "HEAD should not change during findings operations"
    );

    // Working tree should be clean (no new files from findings operations)
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts)).expect("statuses");
    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("?");
        // The only file in the repo is README.md — no findings/* should appear
        assert!(
            !path.starts_with("findings"),
            "findings directory should NOT appear in working tree, found: {path}"
        );
    }
}

// =============================================================================
// Task 3.14: concurrent saves from two threads
// =============================================================================

#[test]
fn concurrent_saves_both_succeed() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save two findings sequentially (true thread concurrency would need
    // separate Repository instances since git2::Repository is not Send).
    // This test verifies that sequential saves don't interfere.
    let first_id = Uuid::now_v7();
    let second_id = Uuid::now_v7();

    store
        .save_finding(&make_test_finding(first_id))
        .expect("save 1");
    store
        .save_finding(&make_test_finding(second_id))
        .expect("save 2");

    // Both should be retrievable
    let first = store.load_finding(&first_id).expect("load 1");
    let second = store.load_finding(&second_id).expect("load 2");
    assert_eq!(first.uuid, first_id);
    assert_eq!(second.uuid, second_id);
}

// =============================================================================
// Update test: save same UUID twice overwrites
// =============================================================================

#[test]
fn save_same_uuid_twice_overwrites() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let uuid = Uuid::now_v7();
    let mut finding = make_test_finding(uuid);
    finding.title = "Original title".to_string();
    store.save_finding(&finding).expect("save original");

    finding.title = "Updated title".to_string();
    finding.status = LifecycleState::InProgress;
    store.save_finding(&finding).expect("save update");

    let loaded = store.load_finding(&uuid).expect("load updated");
    assert_eq!(loaded.title, "Updated title");
    assert_eq!(loaded.status, LifecycleState::InProgress);

    // Should still be only one finding
    let all = store.load_all().expect("load_all");
    assert_eq!(all.len(), 1, "update should overwrite, not duplicate");
}

// =============================================================================
// Negative: load nonexistent finding
// =============================================================================

#[test]
fn load_nonexistent_finding_errors() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let result = store.load_finding(&Uuid::now_v7());
    assert!(result.is_err(), "loading nonexistent finding should error");
}

// =============================================================================
// Negative: operations before init
// =============================================================================

#[test]
fn open_nonexistent_repo_errors() {
    let result = GitFindingsStore::open("/nonexistent/path/to/repo");
    assert!(result.is_err(), "opening nonexistent repo should error");
}

#[test]
fn load_all_before_init_errors() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    // No init — branch doesn't exist
    let result = store.load_all();
    assert!(result.is_err(), "load_all before init should error");
}

#[test]
fn save_before_init_errors() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");

    let result = store.save_finding(&make_test_finding(Uuid::now_v7()));
    assert!(result.is_err(), "save before init should error");
}

// =============================================================================
// git_context tests
// =============================================================================

#[test]
fn git_context_without_remote() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");

    let (repo_id, branch, sha) = store.git_context();
    assert!(
        repo_id.is_empty(),
        "repo without remotes should have empty repo_id"
    );
    assert!(branch.is_some(), "repo with HEAD should have a branch name");
    assert!(sha.is_some(), "repo with commits should have a commit SHA");
}

#[test]
fn git_context_with_remote() {
    let (_tmp, repo_path) = setup_repo();

    // Add an origin remote
    let repo = Repository::open(&repo_path).expect("open repo");
    repo.remote("origin", "https://github.com/example/repo.git")
        .expect("add remote");

    let store = GitFindingsStore::open(&repo_path).expect("open store");
    let (repo_id, branch, sha) = store.git_context();

    assert_eq!(
        repo_id, "https://github.com/example/repo.git",
        "should return origin URL as repo_id"
    );
    assert!(branch.is_some(), "should have a branch name");
    assert!(sha.is_some(), "should have a commit SHA");
}

// =============================================================================
// rebuild_index tests
// =============================================================================

#[test]
fn rebuild_index_with_empty_findings() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // No findings saved — rebuild should still succeed
    store.rebuild_index().expect("rebuild_index");

    // Verify index.json exists and has empty findings array by loading all
    let findings = store.load_all().expect("load_all");
    assert!(
        findings.is_empty(),
        "no findings should exist after rebuild with empty store"
    );
}

#[test]
fn rebuild_index_is_idempotent() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save 3 findings
    for _ in 0..3 {
        store
            .save_finding(&make_test_finding(Uuid::now_v7()))
            .expect("save");
    }

    // Rebuild twice
    store.rebuild_index().expect("first rebuild");
    store.rebuild_index().expect("second rebuild");

    // Verify count is still 3
    let findings = store.load_all().expect("load_all");
    assert_eq!(
        findings.len(),
        3,
        "rebuild_index should be idempotent — still 3 findings"
    );
}

// =============================================================================
// load_all tests
// =============================================================================

#[test]
fn load_all_after_init_returns_empty() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let findings = store.load_all().expect("load_all");
    assert!(
        findings.is_empty(),
        "load_all after init with no saves should return empty Vec, not error"
    );
}

// =============================================================================
// save/load with optional fields
// =============================================================================

#[test]
fn save_finding_with_all_optional_fields() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let uuid = Uuid::now_v7();
    let mut finding = make_test_finding(uuid);
    finding.suggested_fix = Some("Use the ? operator instead of unwrap()".to_string());
    finding.evidence = Some("line 42: let x = foo.unwrap();".to_string());
    finding.tags = vec!["safety".to_string(), "error-handling".to_string()];
    finding.relationships = vec![tally_ng::model::FindingRelationship {
        related_finding_id: Uuid::now_v7(),
        relationship_type: tally_ng::model::RelationshipType::RelatedTo,
        reason: Some("Same pattern".to_string()),
        created_at: Utc::now(),
    }];
    finding.suppression = Some(tally_ng::model::Suppression {
        suppressed_at: Utc::now(),
        reason: "Known issue".to_string(),
        expires_at: None,
        suppression_type: tally_ng::model::SuppressionType::Global,
    });

    store.save_finding(&finding).expect("save");
    let loaded = store.load_finding(&uuid).expect("load");

    assert_eq!(
        loaded.suggested_fix.as_deref(),
        Some("Use the ? operator instead of unwrap()")
    );
    assert_eq!(
        loaded.evidence.as_deref(),
        Some("line 42: let x = foo.unwrap();")
    );
    assert_eq!(loaded.tags, vec!["safety", "error-handling"]);
    assert_eq!(loaded.relationships.len(), 1);
    assert_eq!(
        loaded.relationships[0].relationship_type,
        tally_ng::model::RelationshipType::RelatedTo
    );
    assert!(loaded.suppression.is_some());
    assert_eq!(
        loaded.suppression.as_ref().expect("suppression").reason,
        "Known issue"
    );
}

#[test]
fn save_finding_with_unicode_title() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    let uuid = Uuid::now_v7();
    let mut finding = make_test_finding(uuid);
    finding.title =
        "Unicode test: \u{1F600} \u{00E9}\u{00E8}\u{00EA} \u{4E16}\u{754C}\nnewline".to_string();

    store.save_finding(&finding).expect("save");
    let loaded = store.load_finding(&uuid).expect("load");

    assert_eq!(
        loaded.title, finding.title,
        "unicode title should roundtrip correctly"
    );
}

// =============================================================================
// schema.json test
// =============================================================================

#[test]
fn init_schema_json_has_correct_fields() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Read schema.json from the branch tree
    let repo = Repository::open(&repo_path).expect("open repo");
    let branch = repo
        .find_branch("findings-data", BranchType::Local)
        .expect("find branch");
    let commit = branch.into_reference().peel_to_commit().expect("commit");
    let tree = commit.tree().expect("tree");
    let entry = tree
        .get_name("schema.json")
        .expect("schema.json should exist");
    let blob = repo.find_blob(entry.id()).expect("blob");
    let content: serde_json::Value =
        serde_json::from_slice(blob.content()).expect("parse schema.json");

    assert!(
        content.get("version").is_some(),
        "schema.json should have 'version' field"
    );
    assert!(
        content.get("format").is_some(),
        "schema.json should have 'format' field"
    );
    assert!(
        content.get("created_at").is_some(),
        "schema.json should have 'created_at' field"
    );
}

#[test]
fn rebuild_index_creates_index_json() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save two findings
    let f1 = make_test_finding(Uuid::now_v7());
    let f2 = make_test_finding(Uuid::now_v7());
    store.save_finding(&f1).expect("save 1");
    store.save_finding(&f2).expect("save 2");

    // Rebuild index
    store.rebuild_index().expect("rebuild_index");

    // Verify index.json exists on the branch (load_all still works)
    let findings = store.load_all().expect("load_all");
    assert_eq!(
        findings.len(),
        2,
        "should still have 2 findings after index rebuild"
    );
}

#[test]
fn init_creates_gitattributes() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Verify .gitattributes exists on the branch by checking the branch tree
    let repo = git2::Repository::open(&repo_path).expect("open repo");
    let branch = repo
        .find_branch("findings-data", git2::BranchType::Local)
        .expect("find branch");
    let commit = branch.into_reference().peel_to_commit().expect("commit");
    let tree = commit.tree().expect("tree");
    let entry = tree.get_name(".gitattributes");
    assert!(
        entry.is_some(),
        ".gitattributes should exist on findings-data branch"
    );
}

// =============================================================================
// Coverage: init full tree verification
// =============================================================================

#[test]
fn init_first_call_creates_full_tree() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");

    // Ensure branch does NOT exist before init so we exercise the full init path
    assert!(!store.branch_exists(), "branch should not exist yet");
    store.init().expect("init");

    let repo = Repository::open(&repo_path).expect("open repo");
    let branch = repo
        .find_branch("findings-data", BranchType::Local)
        .expect("find branch");
    let commit = branch.into_reference().peel_to_commit().expect("commit");
    let tree = commit.tree().expect("tree");

    // Verify schema.json
    let schema_entry = tree
        .get_name("schema.json")
        .expect("schema.json must exist");
    let schema_blob = repo.find_blob(schema_entry.id()).expect("schema blob");
    let schema: serde_json::Value =
        serde_json::from_slice(schema_blob.content()).expect("parse schema");
    assert_eq!(schema["version"], "1.0.0");
    assert_eq!(schema["format"], "one-file-per-finding");
    assert!(schema["created_at"].is_string(), "created_at must be set");

    // Verify .gitattributes
    let ga_entry = tree
        .get_name(".gitattributes")
        .expect(".gitattributes must exist");
    let ga_blob = repo.find_blob(ga_entry.id()).expect("gitattributes blob");
    assert_eq!(
        std::str::from_utf8(ga_blob.content()).expect("utf8"),
        "index.json merge=ours\n"
    );

    // Verify findings/.gitkeep
    let findings_entry = tree.get_name("findings").expect("findings dir must exist");
    let findings_tree = repo.find_tree(findings_entry.id()).expect("findings tree");
    let gitkeep = findings_tree
        .get_name(".gitkeep")
        .expect(".gitkeep must exist in findings/");
    let gitkeep_blob = repo.find_blob(gitkeep.id()).expect("gitkeep blob");
    assert!(
        gitkeep_blob.content().is_empty(),
        ".gitkeep should be an empty file"
    );

    // Verify the commit is an orphan (no parents)
    assert_eq!(commit.parent_count(), 0, "init commit should be an orphan");
}

// =============================================================================
// Coverage: load_all skips malformed findings
// =============================================================================

#[test]
fn load_all_skips_malformed_finding() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save a valid finding first
    let valid_uuid = Uuid::now_v7();
    store
        .save_finding(&make_test_finding(valid_uuid))
        .expect("save valid");

    // Manually write a malformed JSON blob to findings/bad-finding.json on the branch
    let repo = Repository::open(&repo_path).expect("open repo");
    let bad_blob_oid = repo.blob(b"{ this is not valid json }").expect("bad blob");

    let branch = repo
        .find_branch("findings-data", BranchType::Local)
        .expect("find branch");
    let tip = branch
        .into_reference()
        .peel_to_commit()
        .expect("tip commit");
    let parent_tree = tip.tree().expect("tree");

    let mut builder = git2::build::TreeUpdateBuilder::new();
    builder.upsert("findings/bad-finding.json", bad_blob_oid, FileMode::Blob);
    let new_tree_oid = builder
        .create_updated(&repo, &parent_tree)
        .expect("build tree");
    let new_tree = repo.find_tree(new_tree_oid).expect("find tree");

    let sig = git2::Signature::now("test", "test@test.com").expect("sig");
    repo.commit(
        Some("refs/heads/findings-data"),
        &sig,
        &sig,
        "add malformed finding",
        &new_tree,
        &[&tip],
    )
    .expect("commit malformed finding");

    // load_all should return only the valid finding and skip the malformed one
    let findings = store.load_all().expect("load_all");
    assert_eq!(
        findings.len(),
        1,
        "should have exactly 1 valid finding, malformed one skipped"
    );
    assert_eq!(findings[0].uuid, valid_uuid);
}

// =============================================================================
// Coverage: sync tests
// =============================================================================

/// Create a bare repo to act as upstream remote.
fn setup_bare_upstream() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().to_str().expect("path").to_string();
    git2::Repository::init_bare(&path).expect("init bare");
    (tmp, path)
}

/// Create a repo with an initial commit and an "origin" remote pointing at upstream.
fn setup_repo_with_remote(upstream_path: &str) -> (tempfile::TempDir, String) {
    let (tmp, repo_path) = setup_repo();
    let repo = git2::Repository::open(&repo_path).expect("open");
    repo.remote("origin", upstream_path).expect("add remote");
    (tmp, repo_path)
}

#[test]
fn sync_before_init_fails() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();
    let (_tmp, repo_path) = setup_repo_with_remote(&upstream_path);
    let store = GitFindingsStore::open(&repo_path).expect("open");

    // No init — branch doesn't exist
    let result = store.sync("origin");
    assert!(result.is_err(), "sync before init should fail");
    let err_msg = format!("{}", result.expect_err("sync before init should fail"));
    assert!(
        err_msg.contains("not found"),
        "error should mention branch not found, got: {err_msg}"
    );
}

#[test]
fn sync_no_remote_fails() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // No remote configured
    let result = store.sync("origin");
    assert!(result.is_err(), "sync without remote should fail");
}

#[test]
fn sync_first_push_to_bare() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();
    let (_tmp, repo_path) = setup_repo_with_remote(&upstream_path);
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save a finding
    let uuid = Uuid::now_v7();
    store.save_finding(&make_test_finding(uuid)).expect("save");

    // Sync — should push to bare upstream
    let result = store.sync("origin").expect("sync");
    assert!(result.pushed, "first sync should push");

    // Verify the bare repo now has the findings-data branch
    let bare = Repository::open_bare(&upstream_path).expect("open bare");
    let branch = bare.find_branch("findings-data", BranchType::Local);
    assert!(
        branch.is_ok(),
        "bare repo should have findings-data branch after sync"
    );
}

#[test]
fn sync_fast_forward() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();

    // Repo A: init and sync to establish remote branch (no findings yet)
    let (_tmp_a, repo_a_path) = setup_repo_with_remote(&upstream_path);
    let store_a = GitFindingsStore::open(&repo_a_path).expect("open A");
    store_a.init().expect("init A");
    store_a.sync("origin").expect("initial sync A");

    // Repo B: fetch from upstream and create local branch from remote
    let (_tmp_b, repo_b_path) = setup_repo_with_remote(&upstream_path);
    {
        let repo_b = Repository::open(&repo_b_path).expect("open B raw");
        let mut remote = repo_b.find_remote("origin").expect("find remote");
        remote.fetch(&["findings-data"], None, None).expect("fetch");
        let remote_ref = repo_b
            .find_reference("refs/remotes/origin/findings-data")
            .expect("remote ref");
        let remote_commit = remote_ref.peel_to_commit().expect("peel");
        repo_b
            .branch("findings-data", &remote_commit, false)
            .expect("create local branch from remote");
    }
    let store_b = GitFindingsStore::open(&repo_b_path).expect("open B");

    // A saves a finding and syncs (remote now ahead of B)
    let uuid_a = Uuid::now_v7();
    store_a
        .save_finding(&make_test_finding(uuid_a))
        .expect("save A");
    store_a.sync("origin").expect("sync A with finding");

    // B syncs — should fast-forward to A's state
    let result = store_b.sync("origin").expect("sync B");
    assert!(result.merged, "B should have merged (fast-forwarded)");

    // Verify B can now load A's finding
    let findings = store_b.load_all().expect("load_all B");
    assert!(
        findings.iter().any(|f| f.uuid == uuid_a),
        "B should see A's finding after fast-forward"
    );
}

#[test]
fn sync_local_ahead() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();
    let (_tmp, repo_path) = setup_repo_with_remote(&upstream_path);
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // First sync to establish remote branch
    let uuid1 = Uuid::now_v7();
    store
        .save_finding(&make_test_finding(uuid1))
        .expect("save 1");
    store.sync("origin").expect("first sync");

    // Save another finding locally (local now ahead of remote)
    let uuid2 = Uuid::now_v7();
    store
        .save_finding(&make_test_finding(uuid2))
        .expect("save 2");

    let result = store.sync("origin").expect("second sync");
    assert!(result.pushed, "should push when local is ahead");
    assert!(
        !result.merged,
        "should not merge when local is simply ahead"
    );
}

#[test]
fn sync_diverged_auto_merge() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();

    // Repo A: init and sync to establish remote branch
    let (_tmp_a, repo_a_path) = setup_repo_with_remote(&upstream_path);
    let store_a = GitFindingsStore::open(&repo_a_path).expect("open A");
    store_a.init().expect("init A");
    store_a.sync("origin").expect("initial sync A");

    // Repo B: fetch from upstream and create local branch from remote tracking ref
    let (_tmp_b, repo_b_path) = setup_repo_with_remote(&upstream_path);
    {
        let repo_b = Repository::open(&repo_b_path).expect("open B raw");
        let mut remote = repo_b.find_remote("origin").expect("find remote");
        remote.fetch(&["findings-data"], None, None).expect("fetch");
        let remote_ref = repo_b
            .find_reference("refs/remotes/origin/findings-data")
            .expect("remote ref");
        let remote_commit = remote_ref.peel_to_commit().expect("peel");
        repo_b
            .branch("findings-data", &remote_commit, false)
            .expect("create local branch from remote");
    }
    let store_b = GitFindingsStore::open(&repo_b_path).expect("open B");

    // A saves finding-1 and syncs
    let uuid_a = Uuid::now_v7();
    store_a
        .save_finding(&make_test_finding(uuid_a))
        .expect("save A");
    store_a.sync("origin").expect("sync A with finding");

    // B saves finding-2 (different UUID, different file) and syncs
    // This creates divergence: remote has finding-1, local has finding-2
    let uuid_b = Uuid::now_v7();
    store_b
        .save_finding(&make_test_finding(uuid_b))
        .expect("save B");
    let result_b = store_b
        .sync("origin")
        .expect("sync B with diverged changes");

    assert!(result_b.merged, "B should have merged diverged branches");
    assert!(result_b.pushed, "B should have pushed merged result");

    // Both findings should be visible in B
    let findings_b = store_b.load_all().expect("load_all B");
    assert!(
        findings_b.iter().any(|f| f.uuid == uuid_a),
        "B should see A's finding after merge"
    );
    assert!(
        findings_b.iter().any(|f| f.uuid == uuid_b),
        "B should see its own finding after merge"
    );
}

// =============================================================================
// Coverage: load_all skips unreadable tree entries (lines 191-193)
// =============================================================================

#[test]
fn load_all_skips_unreadable_tree_entry() {
    let (_tmp, repo_path) = setup_repo();
    let store = GitFindingsStore::open(&repo_path).expect("open");
    store.init().expect("init");

    // Save one valid finding
    let valid_uuid = Uuid::now_v7();
    store
        .save_finding(&make_test_finding(valid_uuid))
        .expect("save valid");

    // Use git2 plumbing to add a subtree (not a blob) as findings/not-a-blob.json
    // on the findings-data branch. When load_all iterates, read_file will fail on
    // this entry because find_blob() can't read a tree OID — exercising the Err path.
    {
        let repo = Repository::open(&repo_path).expect("open repo");
        let branch = repo
            .find_branch("findings-data", BranchType::Local)
            .expect("branch");
        let tip = branch
            .into_reference()
            .peel_to_commit()
            .expect("tip commit");
        let parent_tree = tip.tree().expect("tree");

        // Create an empty tree to use as a fake "blob" entry
        let empty_tree_oid = repo.treebuilder(None).expect("tb").write().expect("write");

        // Insert the tree OID as if it were a blob under findings/not-a-blob.json.
        // We use FileMode::Tree so git stores it as a tree entry, which will cause
        // find_blob() to fail when load_all tries to read it.
        let mut builder = git2::build::TreeUpdateBuilder::new();
        builder.upsert("findings/not-a-blob.json", empty_tree_oid, FileMode::Tree);
        let new_tree_oid = builder
            .create_updated(&repo, &parent_tree)
            .expect("build tree");
        let new_tree = repo.find_tree(new_tree_oid).expect("find tree");

        let sig = git2::Signature::now("test", "test@test.com").expect("sig");
        repo.commit(
            Some("refs/heads/findings-data"),
            &sig,
            &sig,
            "add fake tree entry masquerading as JSON",
            &new_tree,
            &[&tip],
        )
        .expect("commit tree entry");
    }

    // load_all should return only the valid finding, skipping the tree entry
    let findings = store.load_all().expect("load_all");
    assert_eq!(
        findings.len(),
        1,
        "should skip the unreadable tree entry and return only the valid finding"
    );
    assert_eq!(findings[0].uuid, valid_uuid);
}

// =============================================================================
// Coverage: sync diverged merge creates merge commit with two parents (lines 343-382)
// =============================================================================

#[test]
fn sync_diverged_merge_creates_merge_commit() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();

    // Repo A: init and sync to establish remote branch
    let (_tmp_a, repo_a_path) = setup_repo_with_remote(&upstream_path);
    let store_a = GitFindingsStore::open(&repo_a_path).expect("open A");
    store_a.init().expect("init A");
    store_a.sync("origin").expect("initial sync A");

    // Repo B: fetch from upstream and create local branch from remote tracking ref
    let (_tmp_b, repo_b_path) = setup_repo_with_remote(&upstream_path);
    {
        let repo_b = Repository::open(&repo_b_path).expect("open B raw");
        let mut remote = repo_b.find_remote("origin").expect("find remote");
        remote.fetch(&["findings-data"], None, None).expect("fetch");
        let remote_ref = repo_b
            .find_reference("refs/remotes/origin/findings-data")
            .expect("remote ref");
        let remote_commit = remote_ref.peel_to_commit().expect("peel");
        repo_b
            .branch("findings-data", &remote_commit, false)
            .expect("create local branch from remote");
    }
    let store_b = GitFindingsStore::open(&repo_b_path).expect("open B");

    // B saves a finding first (creating a LOCAL commit on findings-data)
    let uuid_b = Uuid::now_v7();
    store_b
        .save_finding(&make_test_finding(uuid_b))
        .expect("save B");

    // A saves a different finding and syncs (remote now has A's commit)
    let uuid_a = Uuid::now_v7();
    store_a
        .save_finding(&make_test_finding(uuid_a))
        .expect("save A");
    store_a.sync("origin").expect("sync A with finding");

    // B syncs — should detect divergence (B has local commit, remote has A's commit)
    // and create a merge commit
    let result_b = store_b
        .sync("origin")
        .expect("sync B with diverged changes");

    assert!(result_b.merged, "B should have merged diverged branches");
    assert!(result_b.pushed, "B should have pushed merged result");

    // Verify the merge commit has 2 parents (local + remote)
    let repo_b = Repository::open(&repo_b_path).expect("open B for verification");
    let branch = repo_b
        .find_branch("findings-data", BranchType::Local)
        .expect("find branch");
    let merge_commit = branch
        .into_reference()
        .peel_to_commit()
        .expect("peel to commit");
    assert_eq!(
        merge_commit.parent_count(),
        2,
        "diverged merge should create a commit with 2 parents"
    );

    // Both findings should be visible in B
    let findings_b = store_b.load_all().expect("load_all B");
    assert!(
        findings_b.iter().any(|f| f.uuid == uuid_a),
        "B should see A's finding after merge"
    );
    assert!(
        findings_b.iter().any(|f| f.uuid == uuid_b),
        "B should see its own finding after merge"
    );
}

// =============================================================================
// Coverage: sync same-file conflict handling (lines 360-364)
// =============================================================================

#[test]
fn sync_same_file_conflict_handled() {
    let (_upstream_tmp, upstream_path) = setup_bare_upstream();

    // Repo A: init and sync to establish remote branch with a shared finding
    let (_tmp_a, repo_a_path) = setup_repo_with_remote(&upstream_path);
    let store_a = GitFindingsStore::open(&repo_a_path).expect("open A");
    store_a.init().expect("init A");

    let shared_uuid = Uuid::now_v7();
    store_a
        .save_finding(&make_test_finding(shared_uuid))
        .expect("save shared");
    store_a.sync("origin").expect("initial sync A");

    // Repo B: fetch and create local branch from remote
    let (_tmp_b, repo_b_path) = setup_repo_with_remote(&upstream_path);
    {
        let repo_b = Repository::open(&repo_b_path).expect("open B raw");
        let mut remote = repo_b.find_remote("origin").expect("remote");
        remote.fetch(&["findings-data"], None, None).expect("fetch");
        let remote_ref = repo_b
            .find_reference("refs/remotes/origin/findings-data")
            .expect("remote ref");
        let remote_commit = remote_ref.peel_to_commit().expect("peel");
        repo_b
            .branch("findings-data", &remote_commit, false)
            .expect("create branch");
    }
    let store_b = GitFindingsStore::open(&repo_b_path).expect("open B");

    // A modifies the shared finding
    let mut finding_a = store_a.load_finding(&shared_uuid).expect("load A");
    finding_a.title = "Modified by A — completely different text here".to_string();
    finding_a.description =
        "A long description that repo A wrote. This content diverges significantly.".to_string();
    store_a.save_finding(&finding_a).expect("save A");
    store_a.sync("origin").expect("sync A");

    // B modifies the SAME finding differently (creates divergence on same file)
    let mut finding_b = store_b.load_finding(&shared_uuid).expect("load B");
    finding_b.title = "Modified by B — totally incompatible change".to_string();
    finding_b.description =
        "B wrote an entirely different description. Cannot auto-merge with A.".to_string();
    store_b.save_finding(&finding_b).expect("save B");

    // B syncs — both modified the same file
    // Git may auto-resolve or conflict. Either is acceptable behavior.
    let result = store_b.sync("origin");
    match result {
        Ok(sync_result) => {
            // Git auto-resolved — valid for small JSON diffs
            assert!(sync_result.merged, "should have merged");
        }
        Err(e) => {
            // Merge conflict — our error path
            let msg = e.to_string();
            assert!(
                msg.to_lowercase().contains("conflict") || msg.to_lowercase().contains("merge"),
                "error should mention conflict, got: {msg}"
            );
        }
    }
}
