//! The licence ledger's completeness check (the sweep's `provenance` step).
//!
//! It reads `migration/licence-ledger.json`, the licence ledger of calc-func and calc-value
//! (dc-96l.1), and fails when a git-tracked file under the ledger's `scan.ledger_roots` has no
//! entry in its `files`, or an entry names a file that is not tracked, or a file has two entries.

use crate::arch::CheckResult;
use crate::cargo;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const LEDGER_PATH: &str = "migration/licence-ledger.json";
const NAME: &str = "provenance";

/// What the check reads from the ledger.
pub struct Scan {
    pub ledger_roots: Vec<String>,
    /// The paths the ledger has an entry for.
    pub entries: Vec<String>,
}

fn strings(value: &Value, what: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{what} is not a list"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{what} holds a non-string"))
        })
        .collect()
}

impl Scan {
    pub fn parse(text: &str) -> Result<Scan, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("{LEDGER_PATH} is not JSON: {e}"))?;
        let entries = value["files"]
            .as_array()
            .ok_or("files is not a list")?
            .iter()
            .map(|f| {
                f["path"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "a files entry has no path".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Scan {
            ledger_roots: strings(&value["scan"]["ledger_roots"], "scan.ledger_roots")?,
            entries,
        })
    }
}

/// The coverage findings: tracked files without an entry, entries for untracked files, and
/// entries listed twice.
pub fn coverage_findings(
    required: &BTreeSet<String>,
    tracked: &BTreeSet<String>,
    entries: &[String],
) -> Vec<String> {
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.clone()) {
            findings.push(format!("{LEDGER_PATH}: {entry} has more than one entry"));
        }
    }
    for path in required.difference(&seen) {
        findings.push(format!(
            "{path} has no entry in {LEDGER_PATH}: add {{\"path\", \"class\", \"reason\"}} saying where its content comes from"
        ));
    }
    for path in seen.difference(tracked) {
        findings.push(format!(
            "{LEDGER_PATH} has an entry for {path}, which is not a tracked file"
        ));
    }
    findings
}

fn tracked(root: &Path, roots: &[String]) -> Result<BTreeSet<String>, String> {
    let mut args = vec!["ls-files", "-z", "--"];
    args.extend(roots.iter().map(String::as_str));
    Ok(cargo::git(root, &args)?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The ledger's completeness: it covers every file under its roots (the sweep).
pub fn check(root: &Path) -> CheckResult {
    let scan = match fs::read_to_string(root.join(LEDGER_PATH))
        .map_err(|e| format!("cannot read {LEDGER_PATH}: {e}"))
        .and_then(|text| Scan::parse(&text))
    {
        Ok(scan) => scan,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let (ledger_files, all_tracked) = match tracked(root, &scan.ledger_roots)
        .and_then(|l| Ok((l, tracked(root, &[])?)))
    {
        Ok(both) => both,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let findings = coverage_findings(&ledger_files, &all_tracked, &scan.entries);
    let summary = format!(
        "{} tracked files under {} against {} ledger entries",
        ledger_files.len(),
        scan.ledger_roots.join(" and "),
        scan.entries.len(),
    );
    let details = json!({
        "ledger": LEDGER_PATH,
        "ledger_files": ledger_files.len(),
        "entries": scan.entries.len(),
    });
    CheckResult::new(NAME, summary, findings, details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_names_missing_untracked_and_repeated_entries() {
        let tracked: BTreeSet<String> = ["a.rs", "b.rs"].map(str::to_owned).into();
        let entries = ["a.rs", "c.rs", "a.rs"].map(str::to_owned);
        let findings = coverage_findings(&tracked, &tracked, &entries);
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(findings[0].contains("a.rs has more than one entry"));
        assert!(findings[1].starts_with("b.rs has no entry"));
        assert!(findings[2].contains("entry for c.rs"));
        assert!(
            coverage_findings(&tracked, &tracked, &["a.rs".to_owned(), "b.rs".to_owned()])
                .is_empty()
        );
    }

    #[test]
    fn optional_provenance_entries_outside_required_roots_still_name_tracked_files() {
        let required = BTreeSet::from(["crates/calc-func/a.rs".to_owned()]);
        let tracked = BTreeSet::from([
            "crates/calc-func/a.rs".to_owned(),
            "tools/probe.rs".to_owned(),
        ]);
        let entries = vec![
            "crates/calc-func/a.rs".to_owned(),
            "tools/probe.rs".to_owned(),
        ];
        assert!(coverage_findings(&required, &tracked, &entries).is_empty());
        assert!(coverage_findings(&required, &tracked, &entries[1..])[0].contains("has no entry"));
        let mut bad = entries;
        bad.push("tools/untracked.rs".into());
        assert!(coverage_findings(&required, &tracked, &bad)[0].contains("not a tracked file"));
    }

    #[test]
    fn the_ledger_parses_and_lists_its_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scan = Scan::parse(&fs::read_to_string(root.join(LEDGER_PATH)).unwrap()).unwrap();
        assert!(!scan.ledger_roots.is_empty() && !scan.entries.is_empty());
    }
}
