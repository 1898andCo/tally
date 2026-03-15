//! Minimal stopwords list for technical writing.
//!
//! Includes ONLY articles, basic prepositions, and basic conjunctions.
//! EXCLUDES negation words (not, no, without, never) and temporal/comparative
//! words (before, after, more, less) — these carry semantic meaning in
//! technical descriptions.

/// Minimal stopwords for Token Jaccard filtering (~20 words).
pub const STOPWORDS: &[&str] = &[
    // Articles
    "a", "an", "the", // Basic prepositions
    "of", "in", "to", "for", "on", "at", "by", "with", // Basic conjunctions
    "and", "or", "but", // Common filler
    "is", "are", "was", "were", "be", "been", "it", "its",
];

/// Remove stopwords from a token list.
#[must_use]
pub fn remove_stopwords<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    tokens
        .iter()
        .filter(|t| !STOPWORDS.contains(t))
        .copied()
        .collect()
}
