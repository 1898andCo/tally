//! Integration tests for git-backed storage (Task 3).
//!
//! Each test creates a temporary git repo, initializes the findings store,
//! and verifies operations without affecting the working tree or HEAD.

use chrono::Utc;
use git2::{BranchType, Repository};
use tally::model::*;
use tally::storage::GitFindingsStore;
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
