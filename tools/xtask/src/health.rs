//! Repository code-health report: formatting drift, line length, and clone habits.
use crate::cargo::{self, Member, Workspace};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub const USAGE: &str = "usage: cargo xtask health [--table | --json]";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageHealth {
    pub package: String,
    pub is_leptos: bool,
    pub rustfmt: usize,
    pub rustfmt_files: Vec<String>,
    pub leptosfmt: Option<usize>,
    pub leptosfmt_files: Vec<String>,
    pub lines_over_160: usize,
    pub redundant_clone: usize,
    pub needless_pass_by_value: usize,
    pub clone_on_copy: usize,
    pub unnecessary_to_owned: usize,
}

impl PackageHealth {
    pub fn to_json(&self) -> Value {
        json!({
            "package": self.package,
            "is_leptos": self.is_leptos,
            "rustfmt": self.rustfmt,
            "rustfmt_files": self.rustfmt_files,
            "leptosfmt": self.leptosfmt,
            "leptosfmt_files": self.leptosfmt_files,
            "lines_over_160": self.lines_over_160,
            "redundant_clone": self.redundant_clone,
            "needless_pass_by_value": self.needless_pass_by_value,
            "clone_on_copy": self.clone_on_copy,
            "unnecessary_to_owned": self.unnecessary_to_owned,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HealthTotals {
    pub rustfmt: usize,
    pub leptosfmt: usize,
    pub lines_over_160: usize,
    pub redundant_clone: usize,
    pub needless_pass_by_value: usize,
    pub clone_on_copy: usize,
    pub unnecessary_to_owned: usize,
}

impl HealthTotals {
    pub fn to_json(&self) -> Value {
        json!({
            "rustfmt": self.rustfmt,
            "leptosfmt": self.leptosfmt,
            "lines_over_160": self.lines_over_160,
            "redundant_clone": self.redundant_clone,
            "needless_pass_by_value": self.needless_pass_by_value,
            "clone_on_copy": self.clone_on_copy,
            "unnecessary_to_owned": self.unnecessary_to_owned,
        })
    }
}

pub struct HealthReport {
    pub leptosfmt_version: Option<String>,
    pub packages: Vec<PackageHealth>,
    pub totals: HealthTotals,
    pub table: String,
}

impl HealthReport {
    pub fn to_json(&self) -> Value {
        json!({
            "leptosfmt_version": self.leptosfmt_version,
            "packages": self.packages.iter().map(PackageHealth::to_json).collect::<Vec<_>>(),
            "totals": self.totals.to_json(),
            "table": self.table,
        })
    }
}

pub fn is_leptos_package(member: &Member) -> bool {
    let manifest = member.dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(&manifest) {
        content.contains("leptos =") || content.contains("leptos=")
    } else {
        false
    }
}

fn ensure_leptosfmt() -> Result<String, String> {
    let mut check_cmd = Command::new("leptosfmt");
    check_cmd.arg("--version");
    if let Ok(out) = cargo::output(&mut check_cmd)
        && out.status.success()
    {
        return Ok(cargo::text(&out.stdout).trim().to_owned());
    }

    eprintln!("leptosfmt absent; installing with cargo install leptosfmt --locked...");
    let mut install = Command::new(cargo::cargo_program());
    install.args(["install", "leptosfmt", "--locked"]);
    let out = cargo::output(&mut install)?;
    if !out.status.success() {
        return Err(format!(
            "failed to install leptosfmt: {}",
            cargo::text(&out.stderr)
        ));
    }

    let mut check_cmd = Command::new("leptosfmt");
    check_cmd.arg("--version");
    let out = cargo::output(&mut check_cmd)?;
    Ok(cargo::text(&out.stdout).trim().to_owned())
}

fn find_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == ".git" {
                continue;
            }
            find_rs_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn count_lines_over_160(path: &Path) -> usize {
    let Ok(content) = fs::read_to_string(path) else {
        return 0;
    };
    content.lines().filter(|l| l.chars().count() > 160).count()
}

fn normalize_rel_path(ws: &Workspace, path_str: &str) -> String {
    let clean = path_str.strip_prefix(r"\\?\").unwrap_or(path_str);
    let path = Path::new(clean);
    ws.rel(path)
}

fn find_package_for_file<'a>(ws: &Workspace, rel: &str, members: &'a [Member]) -> Option<&'a Member> {
    let mut best: Option<&'a Member> = None;
    let mut best_len = 0;
    for member in members {
        let member_rel = ws.rel(&member.dir);
        let prefix = format!("{member_rel}/");
        if (rel.starts_with(&prefix) || rel == member_rel) && member_rel.len() > best_len {
            best_len = member_rel.len();
            best = Some(member);
        }
    }
    best
}

pub fn collect_health(ws: &Workspace) -> Result<HealthReport, String> {
    let leptosfmt_ver = ensure_leptosfmt().ok();

    // 1. Initialise map per package
    let mut stats: BTreeMap<String, PackageHealth> = BTreeMap::new();
    for member in &ws.members {
        stats.insert(
            member.name.clone(),
            PackageHealth {
                package: member.name.clone(),
                is_leptos: is_leptos_package(member),
                rustfmt: 0,
                rustfmt_files: Vec::new(),
                leptosfmt: if is_leptos_package(member) {
                    Some(0)
                } else {
                    None
                },
                leptosfmt_files: Vec::new(),
                lines_over_160: 0,
                redundant_clone: 0,
                needless_pass_by_value: 0,
                clone_on_copy: 0,
                unnecessary_to_owned: 0,
            },
        );
    }

    // 2. Count lines over 160 characters
    for member in &ws.members {
        let mut rs_files = Vec::new();
        find_rs_files(&member.dir, &mut rs_files);
        let mut over_160 = 0;
        for file in rs_files {
            over_160 += count_lines_over_160(&file);
        }
        if let Some(entry) = stats.get_mut(&member.name) {
            entry.lines_over_160 = over_160;
        }
    }

    // 3. Rustfmt check across workspace
    let mut fmt_cmd = Command::new(cargo::cargo_program());
    fmt_cmd.args(["fmt", "--check", "--all"]).current_dir(&ws.root);
    let fmt_out = cargo::output(&mut fmt_cmd)?;
    let fmt_text = format!(
        "{}\n{}",
        cargo::text(&fmt_out.stdout),
        cargo::text(&fmt_out.stderr)
    );
    let mut rustfmt_by_pkg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in fmt_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Diff in ") {
            let rest = rest.trim_end_matches(':');
            let path_str = if let Some((p, _)) = rest.rsplit_once(':') {
                p
            } else {
                rest
            };
            let rel = normalize_rel_path(ws, path_str);
            if let Some(member) = find_package_for_file(ws, &rel, &ws.members) {
                rustfmt_by_pkg
                    .entry(member.name.clone())
                    .or_default()
                    .insert(rel);
            }
        }
    }
    for (pkg_name, files) in rustfmt_by_pkg {
        if let Some(entry) = stats.get_mut(&pkg_name) {
            entry.rustfmt = files.len();
            entry.rustfmt_files = files.into_iter().collect();
        }
    }

    // 4. Leptosfmt check for Leptos packages
    let leptos_members: Vec<&Member> = ws.members.iter().filter(|m| is_leptos_package(m)).collect();
    if !leptos_members.is_empty() {
        let mut leptos_cmd = Command::new("leptosfmt");
        leptos_cmd.arg("--check").current_dir(&ws.root);
        for m in &leptos_members {
            leptos_cmd.arg(ws.rel(&m.dir));
        }
        if let Ok(out) = cargo::output(&mut leptos_cmd) {
            let out_text = format!("{}\n{}", cargo::text(&out.stdout), cargo::text(&out.stderr));
            let mut leptos_by_pkg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for line in out_text.lines() {
                let line = line.trim();
                // Format: ❌ <path> is not correctly formatted. See the difference below:
                if let Some(rest) = line.strip_prefix("❌ ") {
                    if let Some(path_str) = rest.strip_suffix(" is not correctly formatted. See the difference below:")
                        .or_else(|| rest.strip_suffix(" is not correctly formatted."))
                    {
                        let rel = normalize_rel_path(ws, path_str);
                        if let Some(member) = find_package_for_file(ws, &rel, &ws.members) {
                            leptos_by_pkg
                                .entry(member.name.clone())
                                .or_default()
                                .insert(rel);
                        }
                    }
                }
            }
            for (pkg_name, files) in leptos_by_pkg {
                if let Some(entry) = stats.get_mut(&pkg_name) {
                    entry.leptosfmt = Some(files.len());
                    entry.leptosfmt_files = files.into_iter().collect();
                }
            }
        }
    }

    // 5. Clippy checks
    let mut clippy_cmd = Command::new(cargo::cargo_program());
    clippy_cmd
        .args([
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--message-format=json",
            "--",
            "-W",
            "clippy::redundant_clone",
            "-W",
            "clippy::needless_pass_by_value",
            "-W",
            "clippy::clone_on_copy",
            "-W",
            "clippy::unnecessary_to_owned",
        ])
        .current_dir(&ws.root);
    let clippy_out = cargo::output(&mut clippy_cmd)?;
    let clippy_text = cargo::text(&clippy_out.stdout);
    let mut seen_findings: BTreeSet<(String, String, u64)> = BTreeSet::new();

    for line in clippy_text.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if val["reason"] != "compiler-message" {
            continue;
        }
        let Some(code) = val["message"]["code"]["code"].as_str() else {
            continue;
        };
        match code {
            "clippy::redundant_clone"
            | "clippy::needless_pass_by_value"
            | "clippy::clone_on_copy"
            | "clippy::unnecessary_to_owned" => {
                let spans = val["message"]["spans"].as_array();
                let span = spans.and_then(|arr| {
                    arr.iter()
                        .find(|s| s["is_primary"] == true)
                        .or_else(|| arr.first())
                });
                if let Some(span) = span {
                    let file = span["file_name"].as_str().unwrap_or_default();
                    let line_no = span["line_start"].as_u64().unwrap_or(0);
                    let rel = normalize_rel_path(ws, file);
                    let key = (code.to_owned(), rel.clone(), line_no);
                    if seen_findings.insert(key) {
                        if let Some(member) = find_package_for_file(ws, &rel, &ws.members) {
                            if let Some(entry) = stats.get_mut(&member.name) {
                                match code {
                                    "clippy::redundant_clone" => entry.redundant_clone += 1,
                                    "clippy::needless_pass_by_value" => {
                                        entry.needless_pass_by_value += 1
                                    }
                                    "clippy::clone_on_copy" => entry.clone_on_copy += 1,
                                    "clippy::unnecessary_to_owned" => {
                                        entry.unnecessary_to_owned += 1
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 6. Compute totals and build table
    let packages: Vec<PackageHealth> = stats.into_values().collect();
    let mut totals = HealthTotals::default();
    for p in &packages {
        totals.rustfmt += p.rustfmt;
        totals.leptosfmt += p.leptosfmt.unwrap_or(0);
        totals.lines_over_160 += p.lines_over_160;
        totals.redundant_clone += p.redundant_clone;
        totals.needless_pass_by_value += p.needless_pass_by_value;
        totals.clone_on_copy += p.clone_on_copy;
        totals.unnecessary_to_owned += p.unnecessary_to_owned;
    }

    let table = format_table(&packages, &totals);

    Ok(HealthReport {
        leptosfmt_version: leptosfmt_ver,
        packages,
        totals,
        table,
    })
}

fn format_table(packages: &[PackageHealth], totals: &HealthTotals) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<24} {:>7} {:>9} {:>10} {:>15} {:>20} {:>13} {:>20}\n",
        "Package",
        "rustfmt",
        "leptosfmt",
        ">160 chars",
        "redundant_clone",
        "needless_pass_by_val",
        "clone_on_copy",
        "unnecessary_to_owned",
    ));
    out.push_str(&format!("{}\n", "-".repeat(125)));

    for p in packages {
        let leptos_str = match p.leptosfmt {
            Some(count) => count.to_string(),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "{:<24} {:>7} {:>9} {:>10} {:>15} {:>20} {:>13} {:>20}\n",
            p.package,
            p.rustfmt,
            leptos_str,
            p.lines_over_160,
            p.redundant_clone,
            p.needless_pass_by_value,
            p.clone_on_copy,
            p.unnecessary_to_owned,
        ));
    }
    out.push_str(&format!("{}\n", "-".repeat(125)));
    out.push_str(&format!(
        "{:<24} {:>7} {:>9} {:>10} {:>15} {:>20} {:>13} {:>20}\n",
        "Total",
        totals.rustfmt,
        totals.leptosfmt,
        totals.lines_over_160,
        totals.redundant_clone,
        totals.needless_pass_by_value,
        totals.clone_on_copy,
        totals.unnecessary_to_owned,
    ));

    out
}

pub fn run(raw: &[String]) -> ExitCode {
    let mut show_table = true;
    let mut show_json = true;
    for arg in raw {
        match arg.as_str() {
            "--table" => {
                show_table = true;
                show_json = false;
            }
            "--json" => {
                show_table = false;
                show_json = true;
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("health: cannot determine current directory: {e}");
            return ExitCode::SUCCESS;
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("health: cannot load workspace: {e}");
            return ExitCode::SUCCESS;
        }
    };

    let report = match collect_health(&ws) {
        Ok(rep) => rep,
        Err(e) => {
            eprintln!("health error: {e}");
            return ExitCode::SUCCESS;
        }
    };

    let rep_json = report.to_json();
    let target_dir = ws.root.join("target");
    let _ = fs::create_dir_all(&target_dir);
    let _ = fs::write(
        target_dir.join("health.json"),
        serde_json::to_string_pretty(&rep_json).unwrap_or_default(),
    );

    if show_table {
        print!("{}", report.table);
    }
    if show_json {
        if show_table {
            println!("\nJSON Report:\n{}", serde_json::to_string_pretty(&rep_json).unwrap_or_default());
        } else {
            println!("{}", serde_json::to_string_pretty(&rep_json).unwrap_or_default());
        }
    }

    ExitCode::SUCCESS
}
