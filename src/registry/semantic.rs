//! Semantic search via local embeddings (feature-gated behind `semantic-search`).
//!
//! Uses fastembed (ONNX Runtime) to generate embeddings for rule names and
//! descriptions, then computes cosine similarity for semantic matching.
//! Model is downloaded on first use and cached locally.
//!
//! Deep research (Mar 2026) confirmed:
//! - `TextEmbedding::try_new(InitOptions::new(model).with_cache_dir(...))`
//! - `model.embed(Vec<&str>, batch_size)` returns `Vec<Vec<f32>>`
//! - Sync-only API — use `spawn_blocking` in async contexts
//! - `all-MiniLM-L6-v2`: 384-dim, 256-token max, good for short text

#[cfg(feature = "semantic-search")]
use std::path::PathBuf;

#[cfg(feature = "semantic-search")]
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

#[cfg(feature = "semantic-search")]
use crate::error::{Result, TallyError};

#[cfg(feature = "semantic-search")]
use super::rule::Rule;

/// The embedding model name stored in rule JSON for cache invalidation.
#[cfg(feature = "semantic-search")]
pub const EMBEDDING_MODEL_NAME: &str = "all-MiniLM-L6-v2";

/// Expected embedding dimension for `all-MiniLM-L6-v2`.
#[cfg(feature = "semantic-search")]
pub const EMBEDDING_DIM: usize = 384;

/// Initialize the embedding model with cache directory from env or default.
///
/// # Errors
///
/// Returns error if model download fails (no internet and no cached model).
#[cfg(feature = "semantic-search")]
pub fn init_model() -> Result<TextEmbedding> {
    let cache_dir = model_cache_dir();

    // Ensure cache directory exists
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        tracing::warn!(
            path = %cache_dir.display(),
            error = %e,
            "Failed to create model cache directory"
        );
    }

    let options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(true);

    TextEmbedding::try_new(options).map_err(|e| {
        TallyError::InvalidInput(format!(
            "Failed to initialize embedding model (all-MiniLM-L6-v2). \
             If offline, the model must be pre-downloaded. Error: {e}"
        ))
    })
}

/// Get the model cache directory from `TALLY_MODEL_CACHE` env var or default.
#[cfg(feature = "semantic-search")]
fn model_cache_dir() -> PathBuf {
    if let Ok(path) = std::env::var("TALLY_MODEL_CACHE") {
        return PathBuf::from(path);
    }

    // Default: ~/.cache/tally/models/
    home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("tally")
        .join("models")
}

/// Compute embedding for a rule's name + description.
///
/// # Errors
///
/// Returns error if embedding generation fails.
#[cfg(feature = "semantic-search")]
pub fn compute_embedding(model: &mut TextEmbedding, rule: &Rule) -> Result<Vec<f32>> {
    let text = format!("{} — {}", rule.name, rule.description);
    let embeddings = model
        .embed(vec![text.as_str()], Some(1))
        .map_err(|e| TallyError::InvalidInput(format!("Embedding generation failed: {e}")))?;

    let emb = embeddings
        .into_iter()
        .next()
        .ok_or_else(|| TallyError::InvalidInput("No embedding returned".to_string()))?;

    if emb.len() != EMBEDDING_DIM {
        return Err(TallyError::InvalidInput(format!(
            "Unexpected embedding dimension: got {}, expected {EMBEDDING_DIM}",
            emb.len()
        )));
    }

    Ok(emb)
}

/// Compute embedding for a query string.
///
/// # Errors
///
/// Returns error if embedding generation fails.
#[cfg(feature = "semantic-search")]
pub fn compute_query_embedding(model: &mut TextEmbedding, query: &str) -> Result<Vec<f32>> {
    let embeddings = model
        .embed(vec![query], Some(1))
        .map_err(|e| TallyError::InvalidInput(format!("Query embedding failed: {e}")))?;

    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| TallyError::InvalidInput("No embedding returned".to_string()))
}

/// Compute cosine similarity between two embedding vectors.
///
/// Returns a value in `[-1.0, 1.0]` where 1.0 = identical, 0.0 = orthogonal.
#[cfg(feature = "semantic-search")]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let x = f64::from(*x);
        let y = f64::from(*y);
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let result = (dot / denom) as f32;
        result
    }
}

/// Search rules by semantic similarity to a query.
///
/// Computes embeddings lazily for rules with `embedding: None`, caching them
/// in the rule JSON. Returns `(rule_id, similarity_score)` pairs sorted descending.
///
/// # Errors
///
/// Returns error if model initialization or embedding fails.
#[cfg(feature = "semantic-search")]
pub fn semantic_search(
    store: &crate::storage::GitFindingsStore,
    rules: &mut [Rule],
    query: &str,
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let mut model = init_model()?;

    let query_embedding = compute_query_embedding(&mut model, query)?;

    // Compute and cache embeddings for rules that don't have them
    let mut lazy_count = 0;
    for rule in rules.iter_mut() {
        if rule.embedding.is_none() {
            match compute_embedding(&mut model, rule) {
                Ok(emb) => {
                    rule.embedding = Some(emb);
                    rule.embedding_model = Some(EMBEDDING_MODEL_NAME.to_string());
                    lazy_count += 1;

                    // Persist the cached embedding
                    if let Err(e) = crate::registry::store::RuleStore::save_rule(store, rule) {
                        tracing::warn!(rule_id = %rule.id, error = %e, "Failed to cache embedding");
                    }
                }
                Err(e) => {
                    tracing::warn!(rule_id = %rule.id, error = %e, "Failed to compute embedding");
                }
            }
        }

        // Invalidate stale embeddings (model changed)
        if let Some(ref model_name) = rule.embedding_model {
            if model_name != EMBEDDING_MODEL_NAME {
                rule.embedding = None;
                rule.embedding_model = None;
            }
        }
    }

    if lazy_count > 50 {
        tracing::warn!(
            count = lazy_count,
            "Computed embeddings for {lazy_count} rules on the fly. \
             Consider running `tally rule reindex --embeddings` to batch-compute."
        );
    }

    // Score all rules with embeddings
    let mut results: Vec<(String, f32)> = rules
        .iter()
        .filter_map(|rule| {
            rule.embedding.as_ref().map(|emb| {
                let sim = cosine_similarity(&query_embedding, emb);
                (rule.id.clone(), sim)
            })
        })
        .filter(|(_, sim)| *sim >= 0.3) // Min relevance threshold
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    Ok(results)
}

/// Batch-compute embeddings for all rules that don't have them cached.
///
/// # Errors
///
/// Returns error if model initialization fails.
#[cfg(feature = "semantic-search")]
pub fn reindex_embeddings(
    store: &crate::storage::GitFindingsStore,
    rules: &mut [Rule],
) -> Result<usize> {
    let mut model = init_model()?;
    let mut count = 0;

    for rule in rules.iter_mut() {
        let needs_embedding = rule.embedding.is_none()
            || rule
                .embedding_model
                .as_deref()
                .is_none_or(|m| m != EMBEDDING_MODEL_NAME);

        if needs_embedding {
            match compute_embedding(&mut model, rule) {
                Ok(emb) => {
                    rule.embedding = Some(emb);
                    rule.embedding_model = Some(EMBEDDING_MODEL_NAME.to_string());
                    if let Err(e) = crate::registry::store::RuleStore::save_rule(store, rule) {
                        tracing::warn!(rule_id = %rule.id, error = %e, "Failed to save embedding");
                    }
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!(rule_id = %rule.id, error = %e, "Failed to compute embedding");
                }
            }
        }
    }

    Ok(count)
}
