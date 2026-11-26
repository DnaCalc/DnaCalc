//! Repository-wide check that every `not_modelled` and refusal owner bead cited in production Rust
//! source files exists in `.beads/issues.jsonl` and is `open` or `in_progress` (never `closed` or
//! missing).
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const USAGE: &str = "usage: cargo xtask refusal-owners [--list-open]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueInfo {
    pub status: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citation {
    pub path: String,
    pub line: usize,
    pub bead: String,
    pub status: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Clone, Debug, Default)]
pub struct AuditReport {
    pub files_scanned: usize,
    pub citations_checked: usize,
    pub open_beads: BTreeMap<String, usize>,
    pub violations: Vec<Citation>,
}

pub fn run(args: &[String]) -> ExitCode {
    let mut list_open = false;
    for arg in args {
        match arg.as_str() {
            "--list-open" => list_open = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown refusal-owners option: {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("refusal-owners: {err}");
            return ExitCode::FAILURE;
        }
    };
    let root = match crate::cargo::Workspace::load(&cwd) {
        Ok(ws) => ws.root,
        Err(err) => {
            eprintln!("refusal-owners: {err}");
            return ExitCode::FAILURE;
        }
    };
    let issues = match load_tracker(&root.join(".beads/issues.jsonl")) {
        Ok(issues) => issues,
        Err(err) => {
            eprintln!("refusal-owners: {err}");
            return ExitCode::FAILURE;
        }
    };
    let report = match audit_workspace(&root, &issues) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("refusal-owners: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "refusal-owners: scanned {} prod files, checked {} bead citations ({} distinct open/in_progress beads), {} violations",
        report.files_scanned,
        report.citations_checked,
        report.open_beads.len(),
        report.violations.len(),
    );
    if list_open {
        for (bead, count) in &report.open_beads {
            let info = &issues[bead];
            println!("  OPEN  {bead:14} [{:11}] ({count:3} sites): {}", info.status, info.title);
        }
    }
    for v in &report.violations {
        eprintln!(
            "  FAIL  {}:{} {} [{}] ({}) | {}",
            v.path, v.line, v.bead, v.status, v.title, v.snippet
        );
    }
    if report.violations.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub fn load_tracker(path: &Path) -> Result<BTreeMap<String, IssueInfo>, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let mut issues = BTreeMap::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| format!("invalid JSONL in {}:{}: {e}", path.display(), idx + 1))?;
        let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let status = value
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let title = value
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        issues.insert(id.to_owned(), IssueInfo { status, title });
    }
    Ok(issues)
}

pub fn audit_workspace(
    root: &Path,
    issues: &BTreeMap<String, IssueInfo>,
) -> Result<AuditReport, String> {
    let mut files = Vec::new();
    for dir in ["crates", "apps", "platform", "tools"] {
        let path = root.join(dir);
        if path.is_dir() {
            collect_prod_rs(&path, root, &mut files)?;
        }
    }
    files.sort();
    let mut report = AuditReport::default();
    for abs in files {
        let rel = abs
            .strip_prefix(root)
            .unwrap_or(&abs)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&abs)
            .map_err(|e| format!("failed to read {}: {e}", abs.display()))?;
        report.files_scanned += 1;
        audit_source(&rel, &text, issues, &mut report);
    }
    Ok(report)
}

fn collect_prod_rs(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("failed to read directory {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "tests" || name == "target" {
                continue;
            }
            collect_prod_rs(&path, root, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "tests.rs"
                || name.ends_with("_test.rs")
                || name.ends_with("_tests.rs")
                || path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .contains("/tests/")
            {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

/// Non-refusal string patterns (evidence paths, CSS comments, policy keys, generator descriptions).
fn is_non_refusal_string(s: &str, line: &str) -> bool {
    s.contains("migration/evidence/")
        || s.starts_with("/*")
        || s.starts_with("TrustedCodePolicy:")
        || s.starts_with("VbaPolicy:")
        || s.starts_with("VbaProject:")
        || line.contains("class=\"")
        || line.contains("class = \"")
        || (s.starts_with("dc-dfl.2.6 ") && line.contains("description:"))
}

pub fn audit_source(
    rel_path: &str,
    source: &str,
    issues: &BTreeMap<String, IssueInfo>,
    report: &mut AuditReport,
) {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            i += 1;
            let mut depth = 0i32;
            let mut started = false;
            while i < lines.len() {
                let l = lines[i];
                if !started && l.trim_end().ends_with(';') && !l.contains('{') {
                    i += 1;
                    break;
                }
                for ch in l.chars() {
                    if ch == '{' {
                        depth += 1;
                        started = true;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                i += 1;
                if started && depth <= 0 {
                    break;
                }
            }
            continue;
        }
        let line_no = i + 1;
        let code = strip_line_comment(lines[i]);
        let mut seen_on_line = BTreeSet::new();
        for lit in extract_string_literals(code) {
            if is_non_refusal_string(&lit, code) {
                continue;
            }
            for bead in extract_bead_ids(&lit) {
                if !issues.contains_key(&bead) && !bead.chars().any(|c| c.is_ascii_digit()) {
                    continue;
                }
                if !seen_on_line.insert(bead.clone()) {
                    continue;
                }
                report.citations_checked += 1;
                match issues.get(&bead) {
                    Some(info) if info.status == "open" || info.status == "in_progress" => {
                        *report.open_beads.entry(bead).or_insert(0) += 1;
                    }
                    Some(info) => {
                        report.violations.push(Citation {
                            path: rel_path.to_owned(),
                            line: line_no,
                            bead,
                            status: info.status.clone(),
                            title: info.title.clone(),
                            snippet: code.trim().chars().take(110).collect(),
                        });
                    }
                    None => {
                        report.violations.push(Citation {
                            path: rel_path.to_owned(),
                            line: line_no,
                            bead,
                            status: "MISSING".to_owned(),
                            title: String::new(),
                            snippet: code.trim().chars().take(110).collect(),
                        });
                    }
                }
            }
        }
        i += 1;
    }
}

fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
        } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn extract_string_literals(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            let mut escaped = false;
            while i < bytes.len() {
                if escaped {
                    escaped = false;
                } else if bytes[i] == b'\\' {
                    escaped = true;
                } else if bytes[i] == b'"' {
                    out.push(code[start..i].to_owned());
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    out
}

fn extract_bead_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let at_boundary = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if at_boundary && &bytes[i..i + 3] == b"dc-" {
            let start = i;
            i += 3;
            while i < bytes.len()
                && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'.')
            {
                i += 1;
            }
            let candidate = text[start..i].trim_end_matches('.');
            if candidate.len() > 3 && !candidate.ends_with('-') {
                out.push(candidate.to_owned());
            }
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_issues() -> BTreeMap<String, IssueInfo> {
        BTreeMap::from([
            (
                "dc-oxh8w".to_owned(),
                IssueInfo {
                    status: "open".to_owned(),
                    title: "Automatable macro host".to_owned(),
                },
            ),
            (
                "dc-oxh8w.43".to_owned(),
                IssueInfo {
                    status: "in_progress".to_owned(),
                    title: "AutoFilter remainder".to_owned(),
                },
            ),
            (
                "dc-0f1k.24".to_owned(),
                IssueInfo {
                    status: "closed".to_owned(),
                    title: "Closed COM bead".to_owned(),
                },
            ),
        ])
    }

    #[test]
    fn audit_flags_closed_and_missing_owners_and_ignores_cfg_test() {
        let src = r#"
const OWNER: &str = "dc-0f1k.24";
fn ok() -> &'static str { "dc-oxh8w" }
fn in_prog() -> &'static str { "dc-oxh8w.43" }
fn missing() -> &'static str { "dc-x704" }
// comment mentioning "dc-0f1k.24" is ignored
fn css() -> &'static str { "<p class=\"dc-muted\">hi</p>" }
#[cfg(test)]
mod tests {
    fn test_only() {
        let _ = "dc-0f1k.24";
    }
}
"#;
        let mut report = AuditReport::default();
        audit_source("crates/example/src/lib.rs", src, &sample_issues(), &mut report);
        assert_eq!(report.open_beads.get("dc-oxh8w"), Some(&1));
        assert_eq!(report.open_beads.get("dc-oxh8w.43"), Some(&1));
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.violations[0].bead, "dc-0f1k.24");
        assert_eq!(report.violations[0].status, "closed");
        assert_eq!(report.violations[1].bead, "dc-x704");
        assert_eq!(report.violations[1].status, "MISSING");
    }
}
