//! `TallyQL` error types.
//!
//! **SECURITY:** Display implementations are for internal logging only.
//! Sanitize before sending error details to clients — do not expose span
//! offsets or internal parser state.

use std::ops::Range;

/// Structured `TallyQL` error with semantic variants.
///
/// Spans use byte offsets matching Chumsky 0.10's `&str` input mode.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TallyQLError {
    /// Parse error with location and context.
    #[error(
        "parse error at {span:?}: {expected}, {}",
        found.as_deref().unwrap_or("end of input")
    )]
    Parse {
        span: Range<usize>,
        expected: String,
        /// `None` = end of input (EOF), `Some(token)` = unexpected token.
        found: Option<String>,
        hint: Option<String>,
    },
}

/// Crate-level Result alias.
pub type Result<T> = std::result::Result<T, TallyQLError>;

impl TallyQLError {
    /// Construct a parse error for an unexpected token.
    #[must_use]
    pub fn unexpected_token(
        span: Range<usize>,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self::Parse {
            span,
            expected: expected.into(),
            found: Some(found.into()),
            hint: None,
        }
    }

    /// Construct a parse error for unexpected end of input.
    #[must_use]
    pub fn unexpected_eof(span: Range<usize>, expected: impl Into<String>) -> Self {
        Self::Parse {
            span,
            expected: expected.into(),
            found: None,
            hint: None,
        }
    }

    /// Get the error span.
    #[must_use]
    pub fn span(&self) -> &Range<usize> {
        let Self::Parse { span, .. } = self;
        span
    }

    /// Get the hint, if any.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        let Self::Parse { hint, .. } = self;
        hint.as_deref()
    }

    /// Add a hint to this error.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        let Self::Parse {
            hint: ref mut h, ..
        } = self;
        *h = Some(hint.into());
        self
    }
}
