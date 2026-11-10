//! The licence ledger's check (the sweep's `licence-ledger` step).
//!
//! `migration/licence-ledger.json` (dc-96l.1) lists the exceptions among the files of calc-func
//! and calc-value: each entry (`path`, `class`, `reason`) records a file that is not plain own
//! work. A tracked file that is not listed counts as own work. The check fails when a listed
//! path is not a git-tracked file, or when a path is listed twice.

use crate::arch::CheckResult;
use crate::cargo;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const LEDGER_PATH: &str = "migration/licence-ledger.json";
const NAME: &str = "licence-ledger";

/// The paths the ledger lists.
pub fn listed_paths(text: &str) -> Result<Vec<String>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("{LEDGER_PATH} is not JSON: {e}"))?;
    value["files"]
        .as_array()
        .ok_or("files is not a list")?
        .iter()
        .map(|f| {
            f["path"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "a files entry has no path".to_owned())
        })
        .collect()
}

/// The findings: entries for files that are not tracked, and entries listed twice.
pub fn coverage_findings(tracked: &BTreeSet<String>, entries: &[String]) -> Vec<String> {
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.clone()) {
            findings.push(format!("{LEDGER_PATH}: {entry} has more than one entry"));
        }
    }
    for path in seen.difference(tracked) {
        findings.push(format!(
            "{LEDGER_PATH} has an entry for {path}, which is not a tracked file"
        ));
    }
    findings
}

fn tracked(root: &Path) -> Result<BTreeSet<String>, String> {
    Ok(cargo::git(root, &["ls-files", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Every listed path is a tracked file (the sweep).
pub fn check(root: &Path) -> CheckResult {
    let entries = match fs::read_to_string(root.join(LEDGER_PATH))
        .map_err(|e| format!("cannot read {LEDGER_PATH}: {e}"))
        .and_then(|text| listed_paths(&text))
    {
        Ok(entries) => entries,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let tracked = match tracked(root) {
        Ok(files) => files,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let findings = coverage_findings(&tracked, &entries);
    let summary = format!("{} ledger entries, each for a tracked file", entries.len());
    let details = json!({ "ledger": LEDGER_PATH, "entries": entries.len() });
    CheckResult::new(NAME, summary, findings, details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_names_untracked_and_repeated_entries() {
        let tracked: BTreeSet<String> = ["a.rs", "b.rs"].map(str::to_owned).into();
        let entries = ["a.rs", "c.rs", "a.rs"].map(str::to_owned);
        let findings = coverage_findings(&tracked, &entries);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].contains("a.rs has more than one entry"));
        assert!(findings[1].contains("entry for c.rs"));
    }

    #[test]
    fn a_tracked_file_without_an_entry_counts_as_own_work() {
        let tracked: BTreeSet<String> = ["a.rs", "b.rs"].map(str::to_owned).into();
        assert!(coverage_findings(&tracked, &["a.rs".to_owned()]).is_empty());
        assert!(coverage_findings(&tracked, &[]).is_empty());
    }

    #[test]
    fn the_ledger_parses_and_every_listed_file_is_an_exception() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = fs::read_to_string(root.join(LEDGER_PATH)).unwrap();
        assert!(!listed_paths(&text).unwrap().is_empty());
        let value: Value = serde_json::from_str(&text).unwrap();
        assert!(
            value["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f["class"] != "own-work")
        );
    }
}
