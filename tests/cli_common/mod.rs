#![allow(dead_code)]

use assert_cmd::Command;

/// Create a temporary git repo for CLI tests.
pub fn setup_cli_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let repo_path = tmp.path();

    // git init + initial commit
    let repo = git2::Repository::init(repo_path).expect("init");
    let sig = git2::Signature::now("test", "test@test.com").expect("sig");
    let blob = repo.blob(b"# test").expect("blob");
    let mut builder = repo.treebuilder(None).expect("tb");
    builder
        .insert("README.md", blob, 0o100_644)
        .expect("insert");
    let tree_oid = builder.write().expect("write");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .expect("commit");

    tmp
}

pub fn tally() -> Command {
    Command::cargo_bin("tally").expect("tally binary")
}

/// Helper: init repo, record a finding, return `(TempDir, uuid)` pair.
pub fn setup_with_finding() -> (tempfile::TempDir, String) {
    let tmp = setup_cli_repo();
    tally()
        .arg("init")
        .current_dir(tmp.path())
        .assert()
        .success();

    let output = tally()
        .args([
            "record",
            "--file",
            "src/lib.rs",
            "--line",
            "10",
            "--severity",
            "important",
            "--title",
            "test finding",
            "--rule",
            "test-rule",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("record");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse");
    let uuid = json["uuid"].as_str().expect("uuid").to_string();
    (tmp, uuid)
}
