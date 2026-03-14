//! MCP server for tally — exposes finding operations as MCP tools.
//!
//! Uses the official `rmcp` crate (v0.8) with stdio transport.
//! All diagnostic output goes to stderr; stdout is reserved for JSON-RPC.

pub mod server;
