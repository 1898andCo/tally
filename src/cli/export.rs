//! Handler for `tally export`.

use crate::error::{Result, TallyError};
use crate::model::Finding;
use crate::storage::GitFindingsStore;

use super::ExportFormat;

/// Handle `tally export`.
///
/// # Errors
///
/// Returns error if storage or serialization fails.
#[tracing::instrument(skip_all, fields(format = ?format))]
pub fn handle_export(
    store: &GitFindingsStore,
    format: ExportFormat,
    output_path: Option<&str>,
) -> Result<()> {
    let findings = store.load_all()?;

    let content = match format {
        ExportFormat::Json => {
            serde_json::to_string_pretty(&findings).map_err(TallyError::Serialization)?
        }
        ExportFormat::Csv => export_csv(&findings),
        ExportFormat::Sarif => export_sarif(&findings),
    };

    match output_path {
        Some(path) => {
            std::fs::write(path, &content).map_err(TallyError::Io)?;
            tracing::info!(count = findings.len(), path, "Exported findings");
        }
        None => println!("{content}"),
    }

    Ok(())
}

#[must_use]
pub fn export_csv(findings: &[Finding]) -> String {
    use std::fmt::Write;
    let mut out = String::from(
        "uuid,severity,status,rule_id,file_path,line_start,line_end,title,created_at\n",
    );
    for f in findings {
        let (file, ls, le) = f.locations.first().map_or(("", 0, 0), |l| {
            (l.file_path.as_str(), l.line_start, l.line_end)
        });
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{}",
            f.uuid,
            f.severity,
            f.status,
            f.rule_id,
            file,
            ls,
            le,
            f.title.replace(',', ";"),
            f.created_at.to_rfc3339(),
        );
    }
    out
}

/// # Panics
///
/// Panics if `serde_json::json!` macro produces a non-object value (impossible by construction).
#[must_use]
#[allow(clippy::too_many_lines)] // SARIF construction requires inline property bag logic
pub fn export_sarif(findings: &[Finding]) -> String {
    let rules: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| &f.rule_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|rule| {
            serde_json::json!({
                "id": rule,
                "shortDescription": {"text": rule},
            })
        })
        .collect();

    let rule_ids: Vec<&str> = findings
        .iter()
        .map(|f| f.rule_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let results: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            let locations: Vec<serde_json::Value> = f
                .locations
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "physicalLocation": {
                            "artifactLocation": {"uri": l.file_path},
                            "region": {
                                "startLine": l.line_start,
                                "endLine": l.line_end,
                            }
                        }
                    })
                })
                .collect();

            let mut result = serde_json::json!({
                "ruleId": f.rule_id,
                "level": f.severity.to_sarif_level(),
                "message": {"text": f.title},
                "locations": locations,
                "ruleIndex": rule_ids.iter().position(|r| *r == f.rule_id).unwrap_or(0),
                "resultProvenance": {
                    "firstDetectionTimeUtc": f.created_at.to_rfc3339(),
                },
            });

            // Add tally-specific properties (SARIF extension via property bags)
            let props = result
                .as_object_mut()
                .expect("object")
                .entry("properties")
                .or_insert_with(|| serde_json::json!({}));
            if !f.notes.is_empty() {
                props["tally_notes"] = serde_json::json!(
                    f.notes
                        .iter()
                        .map(|n| serde_json::json!({
                            "text": n.text,
                            "timestamp": n.timestamp.to_rfc3339(),
                            "agent_id": n.agent_id,
                        }))
                        .collect::<Vec<_>>()
                );
            }
            if !f.edit_history.is_empty() {
                props["tally_editHistory"] = serde_json::json!(
                    f.edit_history
                        .iter()
                        .map(|e| serde_json::json!({
                            "field": e.field,
                            "oldValue": e.old_value,
                            "newValue": e.new_value,
                            "timestamp": e.timestamp.to_rfc3339(),
                            "agent_id": e.agent_id,
                        }))
                        .collect::<Vec<_>>()
                );
            }
            if !f.tags.is_empty() {
                props["tally_tags"] = serde_json::json!(f.tags);
            }
            // Remove empty properties object
            if props.as_object().is_some_and(serde_json::Map::is_empty) {
                result.as_object_mut().expect("object").remove("properties");
            }

            result
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "tally",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/1898andCo/tally",
                    "rules": rules,
                }
            },
            "results": results,
        }]
    });

    serde_json::to_string_pretty(&sarif).unwrap_or_default()
}
