//! Property tests for finding edit operations.

use proptest::prelude::*;
use tally_ng::model::*;

/// Create a minimal finding for property tests.
fn make_finding() -> Finding {
    Finding {
        schema_version: "1.0.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:prop".to_string(),
        rule_id: "test-rule".to_string(),
        locations: vec![Location {
            file_path: "src/lib.rs".to_string(),
            line_start: 1,
            line_end: 1,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Suggestion,
        category: String::new(),
        tags: vec![],
        title: "prop test".to_string(),
        description: "desc".to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        repo_id: String::new(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
        notes: vec![],
        edit_history: vec![],
    }
}

proptest! {
    #[test]
    fn arbitrary_note_text_roundtrips(text in ".*") {
        let mut finding = make_finding();
        finding.add_note(&text, "test");

        let json = serde_json::to_string(&finding).expect("serialize");
        let deser: Finding = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(&deser.notes[0].text, &text);
    }

    #[test]
    fn arbitrary_field_edit_roundtrips(
        field in prop::sample::select(vec!["title", "description", "category"]),
        value in "[a-zA-Z0-9 ]{0,100}",
    ) {
        let mut finding = make_finding();
        finding
            .edit_field(field, serde_json::json!(value), "test")
            .expect("edit");

        let json = serde_json::to_string(&finding).expect("serialize");
        let deser: Finding = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(deser.edit_history.len(), 1);
        prop_assert_eq!(&deser.edit_history[0].field, field);
    }

    #[test]
    fn edit_field_never_modifies_uuid(
        field in prop::sample::select(vec!["title", "description", "suggested_fix", "evidence", "category"]),
    ) {
        let mut finding = make_finding();
        let uuid_before = finding.uuid;
        finding
            .edit_field(field, serde_json::json!("new value"), "test")
            .expect("edit");
        prop_assert_eq!(finding.uuid, uuid_before);
    }

    #[test]
    fn edit_field_always_increments_history(
        field in prop::sample::select(vec!["title", "description", "category"]),
    ) {
        let mut finding = make_finding();
        let len_before = finding.edit_history.len();
        finding
            .edit_field(field, serde_json::json!("new"), "test")
            .expect("edit");
        prop_assert_eq!(finding.edit_history.len(), len_before + 1);
    }
}
