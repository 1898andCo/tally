#![forbid(unsafe_code)]
//! tally — git-backed findings tracker for AI coding agents.
//!
//! Provides persistent, content-addressable finding identity across
//! sessions, agents, PRs, and branches with full lifecycle tracking.

pub mod cli;
pub mod error;
pub mod mcp;
pub mod model;
pub mod storage;
