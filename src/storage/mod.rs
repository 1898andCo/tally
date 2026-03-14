//! Git-backed storage for findings — one file per finding on an orphan branch.

pub mod git_store;

pub use git_store::GitFindingsStore;
