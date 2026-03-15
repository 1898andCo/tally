//! Tests for semantic search functionality.
//!
//! Cosine similarity tests run always (pure math, no model needed).
//! Embedding and search tests require `--features semantic-search` and
//! download the model (~90MB) on first run.

// --- Cosine similarity tests (always run, no feature gate) ---

#[cfg(feature = "semantic-search")]
use tally_ng::registry::semantic::cosine_similarity;

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_identical_vectors_returns_1() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        (sim - 1.0).abs() < 1e-6,
        "identical vectors should be 1.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_opposite_vectors_returns_negative_1() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![-1.0, 0.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        (sim - (-1.0)).abs() < 1e-6,
        "opposite vectors should be -1.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_orthogonal_vectors_returns_0() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim.abs() < 1e-6,
        "orthogonal vectors should be 0.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_empty_vectors_returns_0() {
    let a: Vec<f32> = vec![];
    let b: Vec<f32> = vec![];
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim.abs() < 1e-6,
        "empty vectors should return 0.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_mismatched_lengths_returns_0() {
    let a = vec![1.0, 2.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim.abs() < 1e-6,
        "mismatched lengths should return 0.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_zero_vector_returns_0() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!(sim.abs() < 1e-6, "zero vector should return 0.0, got {sim}");
}

#[cfg(feature = "semantic-search")]
#[test]
fn cosine_scaled_vectors_returns_1() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![2.0, 4.0, 6.0]; // 2x scale of a
    let sim = cosine_similarity(&a, &b);
    assert!(
        (sim - 1.0).abs() < 1e-6,
        "scaled vectors should be 1.0, got {sim}"
    );
}

#[cfg(feature = "semantic-search")]
#[test]
#[allow(clippy::cast_precision_loss)]
fn cosine_384_dim_vectors() {
    // Simulate real embedding dimensions
    let a: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
    let b: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();
    let sim = cosine_similarity(&a, &b);
    // sin and cos are orthogonal-ish over many periods, should be near 0
    assert!(
        sim.abs() < 0.2,
        "sin/cos over 384 dims should be near-orthogonal, got {sim}"
    );
}

// --- Model-dependent tests (only run with --features semantic-search) ---

#[cfg(feature = "semantic-search")]
mod model_tests {
    use tally_ng::registry::rule::Rule;
    use tally_ng::registry::semantic::{
        EMBEDDING_DIM, EMBEDDING_MODEL_NAME, compute_embedding, compute_query_embedding,
        cosine_similarity, init_model,
    };

    #[test]
    fn init_model_succeeds() {
        let model = init_model();
        assert!(
            model.is_ok(),
            "model init should succeed: {:?}",
            model.err()
        );
    }

    #[test]
    fn compute_embedding_returns_correct_dimension() {
        let mut model = init_model().expect("model init");
        let rule = Rule::new(
            "test-rule".to_string(),
            "Test Rule".to_string(),
            "A rule for testing embedding generation".to_string(),
        );
        let embedding = compute_embedding(&mut model, &rule).expect("embedding");
        assert_eq!(
            embedding.len(),
            EMBEDDING_DIM,
            "embedding should have {EMBEDDING_DIM} dimensions"
        );
    }

    #[test]
    fn compute_query_embedding_returns_correct_dimension() {
        let mut model = init_model().expect("model init");
        let embedding =
            compute_query_embedding(&mut model, "unsafe unwrap usage").expect("query embedding");
        assert_eq!(
            embedding.len(),
            EMBEDDING_DIM,
            "query embedding should have {EMBEDDING_DIM} dimensions"
        );
    }

    #[test]
    fn similar_descriptions_have_high_similarity() {
        let mut model = init_model().expect("model init");

        let rule_a = Rule::new(
            "unsafe-unwrap".to_string(),
            "Unsafe unwrap usage".to_string(),
            "Using unwrap() on Result or Option types can cause panics at runtime".to_string(),
        );
        let rule_b = Rule::new(
            "unwrap-usage".to_string(),
            "Unwrap on fallible types".to_string(),
            "Calling unwrap() on a Result or Option may panic if the value is Err/None".to_string(),
        );

        let emb_a = compute_embedding(&mut model, &rule_a).expect("embedding a");
        let emb_b = compute_embedding(&mut model, &rule_b).expect("embedding b");

        let sim = cosine_similarity(&emb_a, &emb_b);
        assert!(
            sim > 0.7,
            "semantically similar rules should have high similarity, got {sim}"
        );
    }

    #[test]
    fn different_descriptions_have_low_similarity() {
        let mut model = init_model().expect("model init");

        let rule_a = Rule::new(
            "unsafe-unwrap".to_string(),
            "Unsafe unwrap usage".to_string(),
            "Using unwrap() on Result or Option types can cause panics at runtime".to_string(),
        );
        let rule_b = Rule::new(
            "sql-injection".to_string(),
            "SQL injection vulnerability".to_string(),
            "User input concatenated into SQL queries without parameterization".to_string(),
        );

        let emb_a = compute_embedding(&mut model, &rule_a).expect("embedding a");
        let emb_b = compute_embedding(&mut model, &rule_b).expect("embedding b");

        let sim = cosine_similarity(&emb_a, &emb_b);
        assert!(
            sim < 0.7,
            "semantically different rules should have lower similarity, got {sim}"
        );
    }

    #[test]
    fn query_matches_relevant_rule() {
        let mut model = init_model().expect("model init");

        let rule = Rule::new(
            "missing-error-handling".to_string(),
            "Missing error handling".to_string(),
            "Function returns Result but caller ignores the error case".to_string(),
        );
        let emb = compute_embedding(&mut model, &rule).expect("rule embedding");

        let query_relevant =
            compute_query_embedding(&mut model, "error not handled properly").expect("query");
        let query_irrelevant =
            compute_query_embedding(&mut model, "CSS styling for buttons").expect("query");

        let sim_relevant = cosine_similarity(&query_relevant, &emb);
        let sim_irrelevant = cosine_similarity(&query_irrelevant, &emb);

        assert!(
            sim_relevant > sim_irrelevant,
            "relevant query ({sim_relevant}) should score higher than irrelevant ({sim_irrelevant})"
        );
    }

    #[test]
    fn embedding_model_name_is_set() {
        assert_eq!(EMBEDDING_MODEL_NAME, "all-MiniLM-L6-v2");
    }

    #[test]
    fn embedding_is_deterministic() {
        let mut model = init_model().expect("model init");
        let rule = Rule::new(
            "test-rule".to_string(),
            "Test".to_string(),
            "Same input should produce same output".to_string(),
        );
        let emb1 = compute_embedding(&mut model, &rule).expect("embedding 1");
        let emb2 = compute_embedding(&mut model, &rule).expect("embedding 2");

        let sim = cosine_similarity(&emb1, &emb2);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "same input should produce identical embeddings, similarity: {sim}"
        );
    }
}

// --- Semantic search integration tests (need model + git store) ---

#[cfg(feature = "semantic-search")]
mod search_integration {
    use tally_ng::registry::rule::Rule;
    use tally_ng::registry::semantic::semantic_search;
    use tally_ng::storage::GitFindingsStore;

    fn setup_store() -> (tempfile::TempDir, GitFindingsStore) {
        let tmp = tempfile::tempdir().expect("tempdir");
        {
            let repo = git2::Repository::init(tmp.path()).expect("init");
            let sig = git2::Signature::now("test", "test@test.com").expect("sig");
            let blob = repo.blob(b"# test").expect("blob");
            let tree_oid = {
                let mut builder = repo.treebuilder(None).expect("tb");
                builder
                    .insert("README.md", blob, 0o100_644)
                    .expect("insert");
                builder.write().expect("write")
            };
            let tree = repo.find_tree(tree_oid).expect("tree");
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .expect("commit");
        }

        let store = GitFindingsStore::open(tmp.path().to_str().expect("path")).expect("open store");
        store.init().expect("init store");
        (tmp, store)
    }

    fn make_rules() -> Vec<Rule> {
        vec![
            {
                let mut r = Rule::new(
                    "unsafe-unwrap".to_string(),
                    "Unsafe unwrap usage".to_string(),
                    "Using unwrap() on Result or Option can cause panics".to_string(),
                );
                r.category = "safety".to_string();
                r
            },
            {
                let mut r = Rule::new(
                    "sql-injection".to_string(),
                    "SQL injection vulnerability".to_string(),
                    "User input concatenated into SQL queries without parameterization".to_string(),
                );
                r.category = "security".to_string();
                r
            },
            {
                let mut r = Rule::new(
                    "missing-test".to_string(),
                    "Missing test coverage".to_string(),
                    "New code path has no corresponding unit or integration test".to_string(),
                );
                r.category = "testing".to_string();
                r
            },
        ]
    }

    #[test]
    fn semantic_search_ranks_relevant_results_first() {
        let (_tmp, store) = setup_store();
        let mut rules = make_rules();

        // Save rules to store first
        for rule in &rules {
            tally_ng::registry::store::RuleStore::save_rule(&store, rule).expect("save rule");
        }

        let results = semantic_search(&store, &mut rules, "code that can panic", 10)
            .expect("semantic search");

        assert!(!results.is_empty(), "should return results");

        // The "unsafe-unwrap" rule about panics should rank higher than SQL injection
        let unwrap_pos = results.iter().position(|(id, _)| id == "unsafe-unwrap");
        let sql_pos = results.iter().position(|(id, _)| id == "sql-injection");

        if let (Some(u), Some(s)) = (unwrap_pos, sql_pos) {
            assert!(
                u < s,
                "unsafe-unwrap (pos {u}) should rank higher than sql-injection (pos {s}) for 'code that can panic'"
            );
        }
    }

    #[test]
    fn semantic_search_caches_embeddings_in_rules() {
        let (_tmp, store) = setup_store();
        let mut rules = make_rules();

        for rule in &rules {
            tally_ng::registry::store::RuleStore::save_rule(&store, rule).expect("save rule");
        }

        // Before search, embeddings should be None
        assert!(
            rules[0].embedding.is_none(),
            "embedding should start as None"
        );

        let _results =
            semantic_search(&store, &mut rules, "test query", 10).expect("semantic search");

        // After search, embeddings should be cached
        assert!(
            rules[0].embedding.is_some(),
            "embedding should be cached after search"
        );
        assert_eq!(
            rules[0].embedding_model.as_deref(),
            Some("all-MiniLM-L6-v2"),
            "embedding model should be recorded"
        );
    }

    #[test]
    fn semantic_search_empty_rules_returns_empty() {
        let (_tmp, store) = setup_store();
        let mut rules: Vec<Rule> = vec![];

        let results =
            semantic_search(&store, &mut rules, "anything", 10).expect("semantic search on empty");

        assert!(
            results.is_empty(),
            "empty rules should return empty results"
        );
    }

    #[test]
    fn semantic_search_respects_limit() {
        let (_tmp, store) = setup_store();
        let mut rules = make_rules();

        for rule in &rules {
            tally_ng::registry::store::RuleStore::save_rule(&store, rule).expect("save rule");
        }

        let results = semantic_search(&store, &mut rules, "code issue", 1)
            .expect("semantic search with limit");

        assert!(
            results.len() <= 1,
            "should respect limit=1, got {} results",
            results.len()
        );
    }
}
