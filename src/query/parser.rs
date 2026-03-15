//! Chumsky 0.10 parser for `TallyQL` filter expressions.
//!
//! Entry point: [`parse_tallyql()`] with length/depth guards.
//!
//! # Case-Insensitive Keywords
//!
//! All keywords are case-insensitive. This is implemented via a `kw()` helper
//! that uses `text::ident()` with case-insensitive comparison — the idiomatic
//! Chumsky 0.10 pattern (zesterer/chumsky#699).
//!
//! # Thread Safety: `Rc<Cell>` vs `Arc<AtomicUsize>`
//!
//! The nesting depth counter uses `Rc<Cell<usize>>` (not `Arc<AtomicUsize>`).
//! `parse_tallyql()` is synchronous and stack-scoped — never held across
//! `.await` points. `Rc<Cell>` is faster because the compiler can coalesce
//! adjacent inc/dec operations.

use crate::query::ast::FilterExpr;
use crate::query::error::TallyQLError;

/// Parse a `TallyQL` filter expression string into an AST.
///
/// Returns `Ok(FilterExpr)` on success, or a list of parse errors.
///
/// # Errors
///
/// Returns errors for:
/// - Empty input
/// - Input exceeding 8 KB (CWE-400)
/// - Nesting exceeding depth 64 (CWE-674)
/// - Syntax errors with span, expected, found, and optional hint
pub fn parse_tallyql(_input: &str) -> std::result::Result<FilterExpr, Vec<TallyQLError>> {
    // Stub — returns error until commit 2 implements the full parser
    Err(vec![TallyQLError::unexpected_eof(
        0..0,
        "TallyQL parser not yet implemented",
    )])
}
