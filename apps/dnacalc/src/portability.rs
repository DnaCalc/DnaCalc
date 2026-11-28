//! Static workbook portability through CalcVBA's source and bound procedure analysis.

use crate::{Cli, Failure, fail, session};
use calc_host::portability::HostKind;
use serde_json::{Value, json};

pub(crate) fn inspect(cli: &Cli) -> Result<(Value, u8), Failure> {
    let kinds = match cli.portability_host.as_deref() {
        None => vec![HostKind::Server],
        Some("all") => HostKind::ALL.to_vec(),
        Some(name) => vec![
            HostKind::ALL
                .into_iter()
                .find(|kind| kind.key() == name)
                .ok_or_else(|| fail(format!("unknown portability host {name:?}")))?,
        ],
    };
    let session = session::open_session(cli)?;
    let reports = calc_host_vba::file_portability(session.host(), &kinds)
        .map_err(|error| fail(error.to_string()))?;
    let profiles: Vec<_> = kinds
        .into_iter()
        .zip(reports)
        .map(|(kind, report)| {
            let vba = json!(report);
            let unknown_references: Vec<_> = vba["references"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|reference| reference["state"] == "unknown")
                .collect();
            let review = if unknown_references.is_empty() {
                "partial_report"
            } else {
                "unknown_references"
            };
            json!({
                "host": kind.key(),
                "review": review,
                "unknown_references": unknown_references,
                "vba": vba,
            })
        })
        .collect();

    Ok((
        json!({
            "command": "portability",
            "workbook": cli.book,
            "macro_level": cli.level.key(),
            "formula_needs": "pending (dc-xy16l)",
            "object_model_needs": "pending (dc-4hzu2)",
            "profiles": profiles,
        }),
        0,
    ))
}

pub(crate) fn text(report: &Value) -> String {
    let mut lines = vec![report["workbook"].as_str().unwrap_or_default().to_owned()];
    lines.push("VBA source analysis. Formula and object-model needs are pending.".into());
    for profile in report["profiles"].as_array().into_iter().flatten() {
        let host = profile["host"].as_str().unwrap_or_default();
        lines.push(format!("Host {host}"));
        let vba = &profile["vba"];
        if vba.is_null() {
            lines.push("  No VBA project".into());
        } else if vba["effects_complete"] == false {
            lines.push("  Procedure effects are incomplete".into());
        }
        for reference in profile["unknown_references"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = reference["name"].as_str().unwrap_or_default();
            let guid = reference["guid"].as_str().unwrap_or_default();
            lines.push(
                format!("  Unknown reference {name} {guid}")
                    .trim_end()
                    .to_owned(),
            );
        }
        for entry in vba["procedures"].as_array().into_iter().flatten() {
            let module = entry["module"].as_str().unwrap_or_default();
            let procedure = entry["procedure"].as_str().unwrap_or_default();
            let status = entry["status"]
                .as_str()
                .unwrap_or_default()
                .replace('_', " ");
            lines.push(format!("  {module}.{procedure}  {status}"));
        }
    }
    lines.join("\n")
}
