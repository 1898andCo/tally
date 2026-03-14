//! Tests for error type Display implementations and error messages.

use tally_ng::error::TallyError;
use tally_ng::model::LifecycleState;

#[test]
fn not_found_display() {
    let err = TallyError::NotFound {
        uuid: "abc".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("abc"),
        "NotFound error should contain the uuid: {msg}"
    );
}

#[test]
fn invalid_transition_display() {
    let err = TallyError::InvalidTransition {
        from: LifecycleState::Open,
        to: LifecycleState::Closed,
        valid: vec![LifecycleState::Acknowledged, LifecycleState::InProgress],
    };
    let msg = err.to_string();
    assert!(msg.contains("open"), "should contain from state: {msg}");
    assert!(msg.contains("closed"), "should contain to state: {msg}");
    assert!(
        msg.contains("acknowledged"),
        "should list valid targets: {msg}"
    );
}

#[test]
fn branch_not_found_display() {
    let err = TallyError::BranchNotFound {
        branch: "findings-data".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("run `tally init`"),
        "BranchNotFound should suggest running tally init: {msg}"
    );
}

#[test]
fn invalid_severity_display() {
    let err = TallyError::InvalidSeverity("ultra-critical".to_string());
    let msg = err.to_string();
    assert!(
        msg.contains("ultra-critical"),
        "InvalidSeverity should contain the invalid value: {msg}"
    );
}

#[test]
fn no_location_display() {
    let err = TallyError::NoLocation;
    let msg = err.to_string();
    assert!(
        msg.contains("at least one location"),
        "NoLocation should mention at least one location: {msg}"
    );
}
