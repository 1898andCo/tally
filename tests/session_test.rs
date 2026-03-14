//! Tests for session-scoped short ID mapping (Task 6).

use tally::model::Severity;
use tally::session::SessionIdMapper;
use uuid::Uuid;

// --- Positive tests ---

#[test]
fn assign_critical_gets_c_prefix() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let short = mapper.assign(uuid, Severity::Critical);
    assert_eq!(short, "C1");
}

#[test]
fn assign_important_gets_i_prefix() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let short = mapper.assign(uuid, Severity::Important);
    assert_eq!(short, "I1");
}

#[test]
fn assign_suggestion_gets_s_prefix() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let short = mapper.assign(uuid, Severity::Suggestion);
    assert_eq!(short, "S1");
}

#[test]
fn assign_tech_debt_gets_td_prefix() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let short = mapper.assign(uuid, Severity::TechDebt);
    assert_eq!(short, "TD1");
}

#[test]
fn sequential_same_severity_increments() {
    let mut mapper = SessionIdMapper::new();
    let short_1 = mapper
        .assign(Uuid::now_v7(), Severity::Critical)
        .to_string();
    let short_2 = mapper
        .assign(Uuid::now_v7(), Severity::Critical)
        .to_string();
    let short_3 = mapper
        .assign(Uuid::now_v7(), Severity::Critical)
        .to_string();
    assert_eq!(short_1, "C1");
    assert_eq!(short_2, "C2");
    assert_eq!(short_3, "C3");
}

#[test]
fn different_severities_have_independent_counters() {
    let mut mapper = SessionIdMapper::new();
    let c = mapper
        .assign(Uuid::now_v7(), Severity::Critical)
        .to_string();
    let i = mapper
        .assign(Uuid::now_v7(), Severity::Important)
        .to_string();
    let s = mapper
        .assign(Uuid::now_v7(), Severity::Suggestion)
        .to_string();
    assert_eq!(c, "C1");
    assert_eq!(i, "I1");
    assert_eq!(s, "S1");
}

#[test]
fn same_uuid_returns_same_short_id() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let first = mapper.assign(uuid, Severity::Critical).to_string();
    let second = mapper.assign(uuid, Severity::Critical).to_string();
    assert_eq!(first, second, "same UUID should always get same short ID");
}

#[test]
fn resolve_by_short_id() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    mapper.assign(uuid, Severity::Important);
    assert_eq!(mapper.resolve("I1"), Some(uuid));
}

#[test]
fn resolve_case_insensitive() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    mapper.assign(uuid, Severity::Critical);
    assert_eq!(mapper.resolve("c1"), Some(uuid));
    assert_eq!(mapper.resolve("C1"), Some(uuid));
}

#[test]
fn resolve_id_accepts_uuid_string() {
    let mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    assert_eq!(mapper.resolve_id(&uuid.to_string()), Some(uuid));
}

#[test]
fn resolve_id_accepts_short_id() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    mapper.assign(uuid, Severity::Suggestion);
    assert_eq!(mapper.resolve_id("S1"), Some(uuid));
}

#[test]
fn short_id_lookup_by_uuid() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    mapper.assign(uuid, Severity::TechDebt);
    assert_eq!(mapper.short_id(&uuid), Some("TD1"));
}

#[test]
fn len_and_is_empty() {
    let mut mapper = SessionIdMapper::new();
    assert!(mapper.is_empty());
    assert_eq!(mapper.len(), 0);

    mapper.assign(Uuid::now_v7(), Severity::Critical);
    assert!(!mapper.is_empty());
    assert_eq!(mapper.len(), 1);
}

// --- Default trait ---

#[test]
fn mapper_default_is_new() {
    let mapper = SessionIdMapper::default();
    assert!(mapper.is_empty());
    assert_eq!(mapper.len(), 0);
}

// --- Negative tests ---

#[test]
fn resolve_nonexistent_short_id_returns_none() {
    let mapper = SessionIdMapper::new();
    assert_eq!(mapper.resolve("C1"), None);
    assert_eq!(mapper.resolve("X99"), None);
}

#[test]
fn resolve_id_invalid_input_returns_none() {
    let mapper = SessionIdMapper::new();
    assert_eq!(mapper.resolve_id("not-a-uuid-or-short-id"), None);
}

#[test]
fn short_id_unknown_uuid_returns_none() {
    let mapper = SessionIdMapper::new();
    assert_eq!(mapper.short_id(&Uuid::now_v7()), None);
}

// --- Independence and case-preservation tests ---

#[test]
fn mapper_separate_instances_independent() {
    let mut mapper_a = SessionIdMapper::new();
    let mut mapper_b = SessionIdMapper::new();

    let uuid_a = Uuid::now_v7();
    let uuid_b = Uuid::now_v7();

    let short_a = mapper_a.assign(uuid_a, Severity::Critical).to_string();
    let short_b = mapper_b.assign(uuid_b, Severity::Critical).to_string();

    // Both should get C1 since they are independent mappers
    assert_eq!(short_a, "C1");
    assert_eq!(short_b, "C1");

    // mapper_a should not resolve mapper_b's UUID
    assert_eq!(mapper_a.resolve("C1"), Some(uuid_a));
    assert_eq!(mapper_b.resolve("C1"), Some(uuid_b));
    assert_ne!(uuid_a, uuid_b, "UUIDs should differ");
}

#[test]
fn mapper_case_preserved_in_stored_format() {
    let mut mapper = SessionIdMapper::new();
    let uuid = Uuid::now_v7();
    let short = mapper.assign(uuid, Severity::Critical).to_string();

    // short_id() returns uppercase "C1"
    assert_eq!(short, "C1");
    assert_eq!(mapper.short_id(&uuid), Some("C1"));

    // resolve is case-insensitive — "c1" also works
    assert_eq!(mapper.resolve("c1"), Some(uuid));
    assert_eq!(mapper.resolve("C1"), Some(uuid));
}
