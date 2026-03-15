//! CLI integration tests — core commands: init, help, capabilities, sync.

mod cli_common;
use cli_common::*;

use predicates::prelude::*;

#[test]
fn cli_help_shows_subcommands() {
    tally()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("record"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("stats"));
}

#[test]
fn cli_mcp_capabilities_lists_all() {
    // No repo needed — capabilities are reflected from the server struct
    tally()
        .arg("mcp-capabilities")
        .assert()
        .success()
        // Tools — dynamically reflected
        .stdout(predicate::str::contains("Tools (23)"))
        .stdout(predicate::str::contains("record_finding"))
        .stdout(predicate::str::contains("query_findings"))
        .stdout(predicate::str::contains("update_finding_status"))
        .stdout(predicate::str::contains("suppress_finding"))
        .stdout(predicate::str::contains("record_batch"))
        .stdout(predicate::str::contains("get_finding_context"))
        .stdout(predicate::str::contains("initialize_store"))
        .stdout(predicate::str::contains("export_findings"))
        .stdout(predicate::str::contains("sync_findings"))
        .stdout(predicate::str::contains("rebuild_index"))
        .stdout(predicate::str::contains("import_findings"))
        .stdout(predicate::str::contains("update_finding"))
        .stdout(predicate::str::contains("add_note"))
        .stdout(predicate::str::contains("add_tag"))
        .stdout(predicate::str::contains("remove_tag"))
        // Prompts — dynamically reflected
        .stdout(predicate::str::contains("Prompts (5)"))
        .stdout(predicate::str::contains("triage-file"))
        .stdout(predicate::str::contains("fix-finding"))
        .stdout(predicate::str::contains("summarize-findings"))
        .stdout(predicate::str::contains("review-pr"))
        .stdout(predicate::str::contains("explain-finding"))
        // Resources
        .stdout(predicate::str::contains("Resources (8)"))
        .stdout(predicate::str::contains("findings://docs/tallyql-syntax"))
        .stdout(predicate::str::contains("findings://summary"))
        // Config example
        .stdout(predicate::str::contains("mcp-server"));
}

#[test]
fn cli_init_succeeds() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Initialized"))
        .stderr(predicate::str::contains("protect the findings-data branch"));
}

#[test]
fn cli_init_is_idempotent() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn cli_init_prints_branch_protection_tip() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("protect the findings-data branch"));
}

#[test]
fn cli_outside_git_repo_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // No git init — just an empty directory

    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_sync_before_init_fails() {
    let tmp = setup_cli_repo();
    // No init
    tally()
        .args(["sync"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn cli_sync_auth_failure_shows_guidance() {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Add a dummy remote that will fail auth
    let repo = git2::Repository::open(tmp.path()).expect("open");
    repo.remote("test-remote", "https://github.com/nonexistent/repo.git")
        .expect("add remote");
    drop(repo);

    // Sync should fail with actionable error
    tally()
        .args(["sync", "--remote", "test-remote"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication failed").or(
            // On some systems it may fail with a different git error before auth
            predicate::str::contains("error"),
        ));
}
