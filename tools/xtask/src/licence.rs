//! The provenance scan of `cargo xtask check` (dc-96l.6), run beside the word scan.
//!
//! It reads `migration/licence-ledger.json`, the licence ledger of calc-func and calc-value
//! (dc-96l.1), and fails when
//! - a git-tracked file under the ledger's `scan.ledger_roots` has no entry in its `files`, or an
//!   entry names a file that is not tracked;
//! - a git-tracked file under `scan.scan_roots` holds the markers of a removed or replaced source
//!   (`scan.markers`, by family: the NT CRT file and routine names, the routine names of the
//!   removed reference libraries, the glibc copy's file names, words for reading machine code) more often
//!   than `scan.allowed` lists for that file and family.
//!
//! A marker matches case-sensitively where the characters on either side of it are not ASCII
//! letters or digits (with a marker `abc`, `x_abc` holds one and `xabc` none). The ledger stores each marker as
//! the hex of its ASCII bytes, so neither it nor this package carries the markers as text. The
//! records (`docs/`, `migration/`, `.beads/`, `accept-receipts/`) are outside the scanned roots:
//! they describe the sources and the clean-up by name.

use crate::arch::CheckResult;
use crate::cargo;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const LEDGER_PATH: &str = "migration/licence-ledger.json";
const NAME: &str = "provenance-scan";

/// One family of markers.
pub struct Family {
    pub name: String,
    pub forms: Vec<String>,
}

/// What the scan reads from the ledger.
pub struct Scan {
    pub ledger_roots: Vec<String>,
    pub scan_roots: Vec<String>,
    pub families: Vec<Family>,
    /// Per file, the allowed count per family, and the bead that owns the allowance.
    pub allowed: BTreeMap<String, (BTreeMap<String, usize>, String)>,
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
        let scan = &value["scan"];
        let mut families = Vec::new();
        for family in scan["markers"]
            .as_array()
            .ok_or("scan.markers is not a list")?
        {
            let name = family["family"]
                .as_str()
                .ok_or("a marker family has no name")?
                .to_owned();
            let forms = strings(&family["forms_hex"], "forms_hex")?
                .iter()
                .map(|hex| unhex(hex))
                .collect::<Result<Vec<_>, _>>()?;
            if forms.iter().any(String::is_empty) {
                return Err(format!("family {name} has an empty marker"));
            }
            families.push(Family { name, forms });
        }
        let mut allowed = BTreeMap::new();
        for (path, entry) in scan["allowed"]
            .as_object()
            .ok_or("scan.allowed is not an object")?
        {
            let owner = entry["owner"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("allowance for {path} names no owner bead"))?
                .to_owned();
            let mut counts = BTreeMap::new();
            for (family, count) in entry["counts"]
                .as_object()
                .ok_or_else(|| format!("allowance for {path} has no counts"))?
            {
                if !families.iter().any(|f| &f.name == family) {
                    return Err(format!(
                        "allowance for {path} names unknown family {family}"
                    ));
                }
                let count = count
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| format!("allowance for {path} {family} is not a count"))?;
                counts.insert(family.clone(), count);
            }
            allowed.insert(path.clone(), (counts, owner));
        }
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
            ledger_roots: strings(&scan["ledger_roots"], "scan.ledger_roots")?,
            scan_roots: strings(&scan["scan_roots"], "scan.scan_roots")?,
            families,
            allowed,
            entries,
        })
    }
}

/// How many times `marker` stands in `text` with no ASCII letter or digit on either side.
pub fn count_marker(text: &str, marker: &str) -> usize {
    let bytes = text.as_bytes();
    let boundary = |at: Option<&u8>| at.is_none_or(|b| !b.is_ascii_alphanumeric());
    let mut count = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find(marker) {
        let start = from + found;
        let end = start + marker.len();
        let before = if start == 0 {
            None
        } else {
            bytes.get(start - 1)
        };
        if boundary(before) && boundary(bytes.get(end)) {
            count += 1;
        }
        from = end;
    }
    count
}

/// The marker count per family in `text` (families with none left out).
pub fn family_counts(text: &str, families: &[Family]) -> BTreeMap<String, usize> {
    families
        .iter()
        .filter_map(|family| {
            let n: usize = family
                .forms
                .iter()
                .map(|form| count_marker(text, form))
                .sum();
            (n > 0).then(|| (family.name.clone(), n))
        })
        .collect()
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

/// The whole provenance scan: the ledger covers every file under its roots, and no marker
/// reappears (the sweep).
pub fn check(root: &Path) -> CheckResult {
    check_with(root, true)
}

/// The marker part of the provenance scan alone (the per-commit check); the ledger's completeness
/// is left to the sweep.
pub fn check_markers(root: &Path) -> CheckResult {
    check_with(root, false)
}

fn check_with(root: &Path, ledger: bool) -> CheckResult {
    let scan = match fs::read_to_string(root.join(LEDGER_PATH))
        .map_err(|e| format!("cannot read {LEDGER_PATH}: {e}"))
        .and_then(|text| Scan::parse(&text))
    {
        Ok(scan) => scan,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let (ledger_files, scanned_files) = match tracked(root, &scan.ledger_roots)
        .and_then(|l| Ok((l, tracked(root, &scan.scan_roots)?)))
    {
        Ok(both) => both,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let mut findings = if ledger {
        let all_tracked = match tracked(root, &[]) {
            Ok(files) => files,
            Err(error) => {
                return CheckResult::new(
                    NAME,
                    format!("not checked: {error}"),
                    vec![error],
                    Value::Null,
                );
            }
        };
        coverage_findings(&ledger_files, &all_tracked, &scan.entries)
    } else {
        Vec::new()
    };
    let mut hits = serde_json::Map::new();
    let mut total = 0;
    let mut text_files = 0;
    for path in &scanned_files {
        let Ok(bytes) = fs::read(root.join(path)) else {
            findings.push(format!("{path}: cannot read"));
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        text_files += 1;
        let counts = family_counts(&text, &scan.families);
        let (allowed, owner) = scan
            .allowed
            .get(path)
            .map(|(counts, owner)| (counts.clone(), owner.as_str()))
            .unwrap_or_default();
        for (family, &count) in &counts {
            total += count;
            let limit = allowed.get(family).copied().unwrap_or(0);
            if count > limit {
                findings.push(format!(
                    "{path}: {count} markers of {family}, above its allowance of {limit}{}; remove the material (the ledger's scan.what)",
                    if owner.is_empty() { String::new() } else { format!(" (owned by {owner})") }
                ));
            }
        }
        if !counts.is_empty() || !allowed.is_empty() {
            hits.insert(
                path.clone(),
                json!({ "counts": counts, "allowed": allowed, "owner": owner }),
            );
        }
    }
    let allowed_total: usize = scan.allowed.values().flat_map(|(c, _)| c.values()).sum();
    let summary = format!(
        "{} tracked files under {} against {} ledger entries; {total} markers of {} families in {} of {text_files} text files under {}, allowances {allowed_total} in {} files",
        ledger_files.len(),
        scan.ledger_roots.join(" and "),
        scan.entries.len(),
        scan.families.len(),
        hits.values()
            .filter(|h| h["counts"].as_object().is_some_and(|c| !c.is_empty()))
            .count(),
        scan.scan_roots.join(", "),
        scan.allowed.len(),
    );
    let details = json!({
        "ledger": LEDGER_PATH,
        "ledger_files": ledger_files.len(),
        "entries": scan.entries.len(),
        "scanned_text_files": text_files,
        "files": Value::Object(hits),
    });
    CheckResult::new(NAME, summary, findings, details)
}

/// The ASCII text whose bytes `hex` spells in hexadecimal.
pub fn unhex(hex: &str) -> Result<String, String> {
    let digits = hex.as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return Err(format!("{hex:?} is not an even number of hex digits"));
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let pair = std::str::from_utf8(pair).map_err(|_| format!("{hex:?} is not hex"))?;
        bytes.push(u8::from_str_radix(pair, 16).map_err(|_| format!("{hex:?} is not hex"))?);
    }
    if !bytes.is_ascii() {
        return Err(format!("{hex:?} does not spell ASCII text"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{hex:?} does not spell ASCII text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A family whose one marker is `word`, given as hex the way the ledger stores it.
    fn family(name: &str, hex: &str) -> Family {
        Family {
            name: name.to_owned(),
            forms: vec![unhex(hex).unwrap()],
        }
    }

    #[test]
    fn a_marker_counts_only_where_it_stands_apart_from_letters_and_digits() {
        // "abc0" (61626330): counts beside spaces, underscores and punctuation, not inside a
        // longer run of letters or digits, and only in the same case.
        let marker = unhex("61626330").unwrap();
        assert_eq!(count_marker("abc0 x_abc0 (abc0)", &marker), 3);
        assert_eq!(count_marker("xabc0 abc01 ABC0", &marker), 0);
        assert_eq!(count_marker("abc0", &marker), 1);
        let families = [family("f", "61626330"), family("g", "7a7a")];
        let counts = family_counts("abc0 zz abc0", &families);
        assert_eq!(counts.get("f"), Some(&2));
        assert_eq!(counts.get("g"), Some(&1));
        assert!(family_counts("nothing here", &families).is_empty());
    }

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
    fn the_ledger_parses_and_every_allowance_names_a_bead() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scan = Scan::parse(&fs::read_to_string(root.join(LEDGER_PATH)).unwrap()).unwrap();
        assert!(!scan.families.is_empty() && !scan.entries.is_empty());
        assert!(
            scan.allowed
                .values()
                .all(|(_, owner)| owner.starts_with("dc-"))
        );
    }
}
