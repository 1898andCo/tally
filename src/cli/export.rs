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

#[must_use]
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

            serde_json::json!({
                "ruleId": f.rule_id,
                "level": f.severity.to_sarif_level(),
                "message": {"text": f.title},
                "locations": locations,
                "ruleIndex": rule_ids.iter().position(|r| *r == f.rule_id).unwrap_or(0),
            })
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
