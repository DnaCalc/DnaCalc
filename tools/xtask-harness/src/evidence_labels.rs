//! `cargo xtask evidence labels [--write]`: recomputes the acquisition labels of the record
//! `migration/evidence/oxfunc-smart-fuzzer.json` (bead `infra-m0.6`) from the evidence checkout at
//! the record's pinned commit and the local originals, under the rules the record's `labels_rule`
//! states. Without `--write` it prints every way the record's derived sections differ from the
//! recomputation and exits 1 when one does; with `--write` it writes the recomputation into the
//! record. The derived sections are [`DERIVED`]; every other field of the record is written by
//! hand.
//!
//! The inventory is the paths of `manifest.tsv` and `excluded.tsv` at the pinned commit, read with
//! `git cat-file`. A run folder is path-only when its name starts with w108 or w109 (the W108/W109
//! identification runs), or when a `modules` or `classes` entry of
//! `migration/numeric-provenance.json` with disposition tainted or unresolved selects one of its
//! files or names it; no file of a path-only folder is opened (the infra-m0.5 reading rule). Of
//! every other entry, each JSON file at any depth is read for the permitted top-level metadata
//! fields only ([`build_fields`]); labelling reads no capture row, and only the outside-rule search
//! below scans their text for a build key. A JSON file that
//! is empty, holds NUL bytes, is not UTF-8 or (holding the text `build"`) does not parse is listed
//! as unread, with whether a build key appears in its text. An unknown entry without any
//! fingerprint whose JSON or JSONL text holds a build key outside the permitted fields is listed
//! in `builds_outside_rule`: unknown is the conservative label, and the list counts it.

use crate::evidence::{CHECKOUT, real_dir};
use crate::legacy::Legacy;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

/// The record, relative to the workspace root.
pub const RECORD: &str = "migration/evidence/oxfunc-smart-fuzzer.json";

/// The reviewed numeric-provenance dispositions (bead `infra-m0.5`), relative to the workspace root.
const PROVENANCE: &str = "migration/numeric-provenance.json";

/// The directory under the legacy root that holds the originals (`cache/` and `runs/`).
const ORIGINALS: [&str; 2] = ["OxFunc", "smart-fuzzer"];

/// The record's sections this command derives, in the order it reports them.
pub const DERIVED: [&str; 5] = [
    "path_only",
    "counts",
    "unread_metadata",
    "builds_outside_rule",
    "labels",
];

/// Run-folder name prefixes that make a folder path-only: the W108/W109 identification runs.
const NAME_PREFIXES: [&str; 2] = ["w108", "w109"];

/// The top-level objects of a metadata document whose `build` and `excel_build` are read, besides
/// `capture_provenance.environment`, `environment.excel` and each object of the array
/// `environments`.
const HOLDERS: [&str; 4] = [
    "excel_environment",
    "environment",
    "version_axes",
    "profile",
];

/// The record as JSON.
pub fn read_record(workspace_root: &Path) -> Result<Value, String> {
    let path = workspace_root.join(RECORD);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {RECORD}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{RECORD} is not valid JSON: {e}"))
}

/// The entry an inventory path belongs to: the directory directly under cache/ or runs/ (ending in
/// `/`), or the file itself when it lies directly there.
fn entry_of(path: &str) -> String {
    let mut parts = path.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(root), Some(name), Some(_)) => format!("{root}/{name}/"),
        _ => path.to_owned(),
    }
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// Whether a `modules` or `classes` entry selects the OxFunc path `smart-fuzzer/<path>` of one of
/// `paths`: a module by its `path`, a class by its selector's `paths`, or by its `prefixes` and
/// `suffixes` together.
fn selects(item: &Value, paths: &[String]) -> bool {
    let (repository, exact, prefixes, suffixes) = match item.get("selector") {
        Some(selector) => (
            selector["repository"].as_str(),
            strings(&selector["paths"]),
            strings(&selector["prefixes"]),
            strings(&selector["suffixes"]),
        ),
        None => (
            item["repository"].as_str(),
            item["path"].as_str().into_iter().collect(),
            Vec::new(),
            Vec::new(),
        ),
    };
    if repository != Some("OxFunc") {
        return false;
    }
    paths.iter().any(|path| {
        let full = format!("smart-fuzzer/{path}");
        exact.contains(&full.as_str())
            || ((!prefixes.is_empty() || !suffixes.is_empty())
                && (prefixes.is_empty() || prefixes.iter().any(|p| full.starts_with(p)))
                && (suffixes.is_empty() || suffixes.iter().any(|s| full.ends_with(s))))
    })
}

/// The run-folder names that `smart-fuzzer/runs/<name>` mentions in `text` give.
fn folders_named(text: &str) -> Vec<&str> {
    text.match_indices("smart-fuzzer/runs/")
        .map(|(at, marker)| {
            let rest = &text[at + marker.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
                .unwrap_or(rest.len());
            rest[..end].trim_end_matches('.')
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Whether any string inside `item` names the run folder `name`.
fn names_folder(item: &Value, name: &str) -> bool {
    match item {
        Value::String(text) => folders_named(text).contains(&name),
        Value::Array(items) => items.iter().any(|item| names_folder(item, name)),
        Value::Object(map) => map.values().any(|item| names_folder(item, name)),
        _ => false,
    }
}

/// The path-only run folders among `entries`, each with why.
fn path_only_folders(
    entries: &BTreeMap<String, Vec<String>>,
    provenance: &Value,
) -> BTreeMap<String, Vec<String>> {
    let mut folders = BTreeMap::new();
    for (entry, paths) in entries {
        let Some(name) = entry
            .strip_prefix("runs/")
            .and_then(|rest| rest.strip_suffix('/'))
        else {
            continue;
        };
        let mut because = Vec::new();
        let lower = name.to_ascii_lowercase();
        for prefix in NAME_PREFIXES {
            if lower.starts_with(prefix) {
                because.push(format!("its name starts with {prefix}, ignoring case"));
            }
        }
        for list in ["modules", "classes"] {
            for item in provenance[list].as_array().into_iter().flatten() {
                let disposition = item["disposition"].as_str().unwrap_or_default();
                if disposition != "tainted" && disposition != "unresolved" {
                    continue;
                }
                let id = item["id"].as_str().unwrap_or_default();
                if selects(item, paths) {
                    because.push(format!(
                        "{PROVENANCE} {list} entry {id} ({disposition}) selects its files"
                    ));
                } else if names_folder(item, name) {
                    because.push(format!(
                        "{PROVENANCE} {list} entry {id} ({disposition}) names smart-fuzzer/runs/{name}/"
                    ));
                }
            }
        }
        if !because.is_empty() {
            folders.insert(entry.clone(), because);
        }
    }
    folders
}

/// A build value: a string of ASCII digits or a non-negative integer.
fn build_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if is_digits(text) => Some(text.clone()),
        Value::Number(number) => number.as_u64().map(|n| n.to_string()),
        _ => None,
    }
}

/// The permitted metadata fields of one document, in the rule's order: `excel_build` of the
/// document object, then `build` and `excel_build` of each holder ([`HOLDERS`],
/// `capture_provenance.environment`, `environment.excel`, each object of `environments`).
fn build_fields(document: &Value) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Some(object) = document.as_object() else {
        return found;
    };
    if let Some(build) = object.get("excel_build").and_then(build_value) {
        found.push(("excel_build".to_owned(), build));
    }
    let mut holders: Vec<(String, &Map<String, Value>)> = HOLDERS
        .iter()
        .filter_map(|name| {
            object
                .get(*name)
                .and_then(Value::as_object)
                .map(|holder| ((*name).to_owned(), holder))
        })
        .collect();
    for (outer, inner) in [
        ("capture_provenance", "environment"),
        ("environment", "excel"),
    ] {
        if let Some(holder) = object
            .get(outer)
            .and_then(|value| value.get(inner))
            .and_then(Value::as_object)
        {
            holders.push((format!("{outer}.{inner}"), holder));
        }
    }
    for (index, item) in object
        .get("environments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(holder) = item.as_object() {
            holders.push((format!("environments[{index}]"), holder));
        }
    }
    for (name, holder) in holders {
        for field in ["build", "excel_build"] {
            if let Some(build) = holder.get(field).and_then(build_value) {
                found.push((format!("{name}.{field}"), build));
            }
        }
    }
    found
}

/// What one JSON file gives the metadata rule.
#[derive(Debug, PartialEq)]
enum Metadata {
    /// It parsed; these are its permitted fields (possibly none).
    Fields(Vec<(String, String)>),
    /// Its text holds no `build"`, so no permitted field: it is not parsed.
    NoBuildKey,
    /// It cannot be read as JSON text, for this reason.
    Unread(&'static str),
}

fn read_metadata(bytes: &[u8]) -> Metadata {
    if bytes.is_empty() {
        return Metadata::Unread("empty");
    }
    if bytes.contains(&0) {
        return Metadata::Unread("holds NUL bytes");
    }
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let Ok(text) = std::str::from_utf8(body) else {
        return Metadata::Unread("not UTF-8");
    };
    if !text.contains("build\"") {
        return Metadata::NoBuildKey;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(document) => Metadata::Fields(build_fields(&document)),
        Err(_) => Metadata::Unread("not valid JSON"),
    }
}

/// The build values that a key `"build"` or `"excel_build"` takes anywhere in `bytes`, as text: a
/// string of digits or an integer. NUL bytes are dropped first, so UTF-16 text is searched too.
fn build_keys(bytes: &[u8]) -> BTreeSet<String> {
    let bytes: Cow<[u8]> = if bytes.contains(&0) {
        Cow::Owned(bytes.iter().copied().filter(|b| *b != 0).collect())
    } else {
        Cow::Borrowed(bytes)
    };
    let text = String::from_utf8_lossy(&bytes);
    let raw = text.as_bytes();
    let mut builds = BTreeSet::new();
    for (at, needle) in text.match_indices("build\"") {
        let before = &raw[..at];
        if !(before.ends_with(b"\"") || before.ends_with(b"\"excel_")) {
            continue;
        }
        let mut i = at + needle.len();
        let skip = |mut i: usize| {
            while raw.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            i
        };
        i = skip(i);
        if raw.get(i) != Some(&b':') {
            continue;
        }
        i = skip(i + 1);
        let quoted = raw.get(i) == Some(&b'"');
        if quoted {
            i += 1;
        }
        let start = i;
        while raw.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let next = raw.get(i).copied();
        let whole = if quoted {
            next == Some(b'"')
        } else {
            !matches!(next, Some(b'.' | b'e' | b'E'))
        };
        if i > start && whole {
            builds.insert(text[start..i].to_owned());
        }
    }
    builds
}

/// The recomputed derived sections and what the recomputation read.
struct Computed {
    path_only: BTreeMap<String, Vec<String>>,
    labels: Vec<Value>,
    unread: Vec<Value>,
    outside: Vec<Value>,
    counts: Value,
    opened: BTreeSet<String>,
}

impl Computed {
    fn sections(&self) -> Vec<(&'static str, Value)> {
        let path_only: Vec<Value> = self
            .path_only
            .iter()
            .map(|(entry, because)| json!({"entry": entry, "because": because}))
            .collect();
        DERIVED
            .iter()
            .map(|name| {
                let value = match *name {
                    "path_only" => Value::Array(path_only.clone()),
                    "counts" => self.counts.clone(),
                    "unread_metadata" => Value::Array(self.unread.clone()),
                    "builds_outside_rule" => Value::Array(self.outside.clone()),
                    _ => Value::Array(self.labels.clone()),
                };
                (*name, value)
            })
            .collect()
    }
}

fn read_original(
    originals: &Path,
    path: &str,
    opened: &mut BTreeSet<String>,
) -> Result<Vec<u8>, String> {
    opened.insert(path.to_owned());
    std::fs::read(originals.join(path)).map_err(|e| format!("cannot read the original {path}: {e}"))
}

fn compute(
    inventory: &BTreeSet<String>,
    originals: &Path,
    provenance: &Value,
) -> Result<Computed, String> {
    let mut entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in inventory {
        entries
            .entry(entry_of(path))
            .or_default()
            .push(path.clone());
    }
    let path_only = path_only_folders(&entries, provenance);
    let mut labels = Vec::new();
    let mut unread = Vec::new();
    let mut outside = Vec::new();
    let mut opened = BTreeSet::new();
    let (mut json_files, mut parsed, mut without_key) = (0usize, 0usize, 0usize);
    for (entry, paths) in &entries {
        let closed = path_only.contains_key(entry);
        let mut sources: Vec<Value> = Vec::new();
        let mut dirs = BTreeSet::new();
        for path in paths {
            let parts: Vec<&str> = path.split('/').collect();
            for index in 0..parts.len() - 1 {
                if let Some(build) = parts[index].strip_prefix("build-")
                    && is_digits(build)
                {
                    let dir = format!("{}/", parts[..=index].join("/"));
                    if dirs.insert(dir.clone()) {
                        sources.push(json!({"path": dir, "build": build}));
                    }
                }
            }
        }
        if !closed {
            for path in paths.iter().filter(|path| path.ends_with(".json")) {
                let bytes = read_original(originals, path, &mut opened)?;
                json_files += 1;
                match read_metadata(&bytes) {
                    Metadata::Fields(fields) => {
                        parsed += 1;
                        for (field, build) in fields {
                            sources.push(json!({"file": path, "field": field, "build": build}));
                        }
                    }
                    Metadata::NoBuildKey => without_key += 1,
                    Metadata::Unread(reason) => unread.push(json!({
                        "file": path,
                        "reason": reason,
                        "holds_build_key": !build_keys(&bytes).is_empty(),
                    })),
                }
            }
        }
        let builds: BTreeSet<&str> = sources
            .iter()
            .filter_map(|source| source["build"].as_str())
            .collect();
        let build = (builds.len() == 1)
            .then(|| builds.iter().next().map(|b| (*b).to_owned()))
            .flatten();
        let mut label = Map::new();
        label.insert("entry".into(), json!(entry));
        label.insert("files".into(), json!(paths.len()));
        label.insert(
            "reading".into(),
            json!(if closed {
                "path only"
            } else {
                "path and metadata"
            }),
        );
        label.insert(
            "label".into(),
            json!(if build.is_some() { "known" } else { "unknown" }),
        );
        label.insert("excel_build".into(), json!(build));
        if builds.len() > 1 {
            label.insert("conflicting_builds".into(), json!(builds));
        }
        label.insert(
            "certifies_exact_target".into(),
            json!(build.as_ref().map(|b| format!("excel-build-{b}"))),
        );
        if !closed && sources.is_empty() {
            let mut files = Vec::new();
            let mut found = BTreeSet::new();
            for path in paths
                .iter()
                .filter(|path| path.ends_with(".json") || path.ends_with(".jsonl"))
            {
                let keys = build_keys(&read_original(originals, path, &mut opened)?);
                if !keys.is_empty() {
                    files.push(path.clone());
                    found.extend(keys);
                }
            }
            if !files.is_empty() {
                outside.push(json!({"entry": entry, "files": files, "builds": found}));
            }
        }
        label.insert("sources".into(), Value::Array(sources));
        labels.push(Value::Object(label));
    }
    let known: Vec<&Value> = labels.iter().filter(|l| l["label"] == "known").collect();
    let mut by_build: BTreeMap<String, usize> = BTreeMap::new();
    for label in &known {
        *by_build
            .entry(label["excel_build"].as_str().unwrap_or_default().to_owned())
            .or_default() += 1;
    }
    let counts = json!({
        "entries": labels.len(),
        "files": inventory.len(),
        "known": known.len(),
        "unknown": labels.len() - known.len(),
        "path_only_entries": path_only.len(),
        "known_by_build": by_build,
        "unknown_without_fingerprint": labels
            .iter()
            .filter(|l| l["label"] == "unknown" && l["sources"].as_array().is_some_and(Vec::is_empty))
            .count(),
        "unknown_conflicting": labels.iter().filter(|l| l.get("conflicting_builds").is_some()).count(),
        "outside_rule_entries": outside.len(),
        "metadata_files": {
            "json": json_files,
            "parsed": parsed,
            "without_build_key": without_key,
            "unread": unread.len(),
        },
    });
    Ok(Computed {
        path_only,
        labels,
        unread,
        outside,
        counts,
        opened,
    })
}

/// The paths of one table of the evidence repository at `pin`.
fn pinned_paths(git: &Legacy, pin: &str, name: &str) -> Result<Vec<String>, String> {
    let bytes = git.read(&["cat-file", "blob", &format!("{pin}:{name}")])?;
    let text =
        String::from_utf8(bytes).map_err(|e| format!("{name} at {pin} is not UTF-8: {e}"))?;
    Ok(text
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| line.split('\t').next().unwrap_or_default().to_owned())
        .collect())
}

/// The order in which the record's object keys are written; keys not listed follow in name order.
const KEY_ORDER: [&str; 79] = [
    // the record
    "format",
    "bead",
    "subject",
    "evidence_repository",
    "manifest",
    "excluded",
    "removed",
    "layout",
    "labels_rule",
    "path_only",
    "counts",
    "unread_metadata",
    "builds_outside_rule",
    "labels",
    // evidence_repository
    "url",
    "branch",
    "commit",
    "local_checkout",
    "originals",
    "legacy_root",
    "script",
    "commands",
    "backup",
    // labels_rule
    "rule",
    "entries",
    "path_rule",
    "path_only_rule",
    "metadata_rule",
    "unread_rule",
    "outside_rule",
    // labels, sources, tables, unread files, folders
    "entry",
    "path",
    "file",
    "columns",
    "sha256",
    "git_blob",
    "archived_files",
    "cache_files",
    "runs_files",
    "files",
    // counts
    "known",
    "unknown",
    "path_only_entries",
    "known_by_build",
    "unknown_without_fingerprint",
    "unknown_conflicting",
    "outside_rule_entries",
    "metadata_files",
    "json",
    "parsed",
    "without_build_key",
    "unread",
    // labels and sources
    "reading",
    "label",
    "excel_build",
    "conflicting_builds",
    "builds",
    "field",
    "build",
    "sources",
    "certifies_exact_target",
    "certification",
    // the rest
    "raw_bytes",
    "gz_bytes",
    "reason",
    "holds_build_key",
    "because",
    "note",
    "source",
    "folders",
    "measured",
    "measured_by",
    "quoted",
    "quoted_from",
    "why",
    "kept",
    "transcript",
    "command",
    "exit",
];

fn ordered(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let rank = |key: &str| {
        KEY_ORDER
            .iter()
            .position(|k| *k == key)
            .unwrap_or(KEY_ORDER.len())
    };
    let mut items: Vec<(&String, &Value)> = map.iter().collect();
    items.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| a.0.cmp(b.0)));
    items
}

fn compact(value: &Value) -> String {
    match value {
        Value::Object(map) => format!(
            "{{{}}}",
            ordered(map)
                .iter()
                .map(|(key, item)| format!(
                    "{}: {}",
                    compact(&Value::String((*key).clone())),
                    compact(item)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(compact).collect::<Vec<_>>().join(", ")
        ),
        scalar => serde_json::to_string(scalar).expect("a JSON scalar serialises"),
    }
}

fn pretty(value: &Value, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent + 2);
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            let items = ordered(map);
            for (index, (key, item)) in items.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&compact(&Value::String((*key).clone())));
                out.push_str(": ");
                pretty(item, indent + 2, out);
                out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                out.push_str(&pad);
                if item.is_object() {
                    out.push_str(&compact(item));
                } else {
                    pretty(item, indent + 2, out);
                }
                out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
        other => out.push_str(&compact(other)),
    }
}

/// The record as written: objects indented, each object inside an array on one line.
fn render(record: &Value) -> String {
    let mut out = String::new();
    pretty(record, 0, &mut out);
    out.push('\n');
    out
}

pub fn run(workspace_root: &Path, legacy_root: &Path, write: bool) -> ExitCode {
    match compare_or_write(workspace_root, legacy_root, write) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("xtask evidence labels: {e}");
            ExitCode::from(1)
        }
    }
}

fn compare_or_write(
    workspace_root: &Path,
    legacy_root: &Path,
    write: bool,
) -> Result<bool, String> {
    let mut record = read_record(workspace_root)?;
    let pin = record["evidence_repository"]["commit"]
        .as_str()
        .ok_or_else(|| format!("{RECORD} has no evidence_repository.commit"))?
        .to_owned();
    let checkout = legacy_root.join(CHECKOUT);
    let originals: PathBuf = ORIGINALS
        .iter()
        .fold(legacy_root.to_path_buf(), |path, part| path.join(part));
    if !checkout.join(".git").exists() {
        return Err(format!(
            "the evidence checkout <legacy root>/{CHECKOUT} does not exist ({})",
            checkout.display()
        ));
    }
    if !originals.is_dir() {
        return Err(format!(
            "the originals <legacy root>/OxFunc/smart-fuzzer do not exist ({})",
            originals.display()
        ));
    }
    let git = Legacy::new(real_dir(&checkout));
    let mut inventory = BTreeSet::new();
    for name in ["manifest.tsv", "excluded.tsv"] {
        inventory.extend(pinned_paths(&git, &pin, name)?);
    }
    let provenance_text = std::fs::read_to_string(workspace_root.join(PROVENANCE))
        .map_err(|e| format!("cannot read {PROVENANCE}: {e}"))?;
    let provenance: Value = serde_json::from_str(&provenance_text)
        .map_err(|e| format!("{PROVENANCE} is not valid JSON: {e}"))?;
    println!(
        "xtask evidence labels: pinned commit {pin}: {} paths in manifest.tsv and excluded.tsv",
        inventory.len()
    );
    let started = Instant::now();
    let computed = compute(&inventory, &originals, &provenance)?;
    for (entry, because) in &computed.path_only {
        println!("xtask evidence labels: path only, files not opened: {entry} ({because:?})");
    }
    let counts = &computed.counts;
    println!(
        "xtask evidence labels: {} entries: {} known {}, {} unknown ({} without a fingerprint, {} with conflicting builds), {} path only",
        counts["entries"],
        counts["known"],
        counts["known_by_build"],
        counts["unknown"],
        counts["unknown_without_fingerprint"],
        counts["unknown_conflicting"],
        counts["path_only_entries"]
    );
    println!(
        "xtask evidence labels: JSON files of the other entries: {}",
        counts["metadata_files"]
    );
    for file in &computed.unread {
        println!("xtask evidence labels: unread: {}", compact(file));
    }
    for entry in &computed.outside {
        println!(
            "xtask evidence labels: unknown, with a build key outside the permitted fields: {}",
            compact(entry)
        );
    }
    let opened_closed = computed
        .opened
        .iter()
        .filter(|path| computed.path_only.contains_key(&entry_of(path)))
        .count();
    println!(
        "xtask evidence labels: opened {} original files ({opened_closed} of a path-only folder) in {:.1} s",
        computed.opened.len(),
        started.elapsed().as_secs_f64()
    );
    let mut differences = 0usize;
    for (name, value) in computed.sections() {
        let recorded = &record[name];
        if name == "labels" {
            let by_entry = |labels: &Value| -> BTreeMap<String, Value> {
                labels
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|label| {
                        (
                            label["entry"].as_str().unwrap_or_default().to_owned(),
                            label.clone(),
                        )
                    })
                    .collect()
            };
            let ours = by_entry(&value);
            let theirs = by_entry(recorded);
            let all: BTreeSet<&String> = ours.keys().chain(theirs.keys()).collect();
            for entry in all {
                let (a, b) = (theirs.get(entry), ours.get(entry));
                if a != b {
                    differences += 1;
                    println!(
                        "xtask evidence labels: label differs: {entry}: record {} | computed {}",
                        a.map_or("absent".to_owned(), compact),
                        b.map_or("absent".to_owned(), compact)
                    );
                }
            }
            if differences == 0 && recorded != &value {
                differences += 1;
                println!("xtask evidence labels: the record's labels differ in order or count");
            }
        } else if recorded != &value {
            differences += 1;
            println!(
                "xtask evidence labels: {name} differs: record {} | computed {}",
                compact(recorded),
                compact(&value)
            );
        }
    }
    if write {
        let object = record
            .as_object_mut()
            .ok_or_else(|| format!("{RECORD} is not a JSON object"))?;
        for (name, value) in computed.sections() {
            object.insert(name.to_owned(), value);
        }
        std::fs::write(workspace_root.join(RECORD), render(&record))
            .map_err(|e| format!("cannot write {RECORD}: {e}"))?;
        println!(
            "xtask evidence labels: wrote {DERIVED:?} into {RECORD}; {differences} differences were replaced"
        );
        return Ok(true);
    }
    if differences == 0 {
        println!("xtask evidence labels: {RECORD}'s {DERIVED:?} equal the recomputation");
        Ok(true)
    } else {
        println!(
            "xtask evidence labels: {differences} differences; `cargo xtask evidence labels --write` writes the recomputation into {RECORD}"
        );
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_the_folder_under_a_root_or_a_file_directly_in_it() {
        assert_eq!(entry_of("runs/w111-x/answers/a.json"), "runs/w111-x/");
        assert_eq!(entry_of("cache/oracle/env.json"), "cache/oracle/");
        assert_eq!(entry_of("cache/README.md"), "cache/README.md");
    }

    #[test]
    fn build_fields_read_only_the_permitted_holders() {
        let document = json!({
            "excel_build": "20430",
            "excel_environment": {"build": "19929", "version": "16.0"},
            "environment": {"excel_build": 20026, "excel": {"excel_build": "20131"}},
            "version_axes": {"excel_build": "1"},
            "profile": {"excel_build": "2"},
            "capture_provenance": {"environment": {"excel_build": "3"}},
            "environments": [{"build": "4"}, "skip", {"excel_build": "16.0"}],
            "rows": [{"excel_build": "999"}],
            "other": {"build": "998"},
        });
        assert_eq!(
            build_fields(&document),
            vec![
                ("excel_build".to_owned(), "20430".to_owned()),
                ("excel_environment.build".to_owned(), "19929".to_owned()),
                ("environment.excel_build".to_owned(), "20026".to_owned()),
                ("version_axes.excel_build".to_owned(), "1".to_owned()),
                ("profile.excel_build".to_owned(), "2".to_owned()),
                (
                    "capture_provenance.environment.excel_build".to_owned(),
                    "3".to_owned()
                ),
                (
                    "environment.excel.excel_build".to_owned(),
                    "20131".to_owned()
                ),
                ("environments[0].build".to_owned(), "4".to_owned()),
            ]
        );
        assert!(build_fields(&json!([{"excel_build": "1"}])).is_empty());
        assert!(build_fields(&json!({"excel_build": -1})).is_empty());
    }

    #[test]
    fn read_metadata_classifies_every_file() {
        assert_eq!(read_metadata(b""), Metadata::Unread("empty"));
        assert_eq!(read_metadata(&[0; 8]), Metadata::Unread("holds NUL bytes"));
        assert_eq!(
            read_metadata(b"{\"a\": \"\xff\"}"),
            Metadata::Unread("not UTF-8")
        );
        assert_eq!(read_metadata(b"{\"a\": 1}"), Metadata::NoBuildKey);
        assert_eq!(
            read_metadata(b"{\"excel_build\": \"1\""),
            Metadata::Unread("not valid JSON")
        );
        assert_eq!(
            read_metadata(b"\xEF\xBB\xBF{\"excel_build\": \"7\"}"),
            Metadata::Fields(vec![("excel_build".to_owned(), "7".to_owned())])
        );
        assert_eq!(
            read_metadata(b"{\"note\": \"nightly build\"}"),
            Metadata::Fields(Vec::new())
        );
    }

    #[test]
    fn build_keys_find_keys_with_whole_build_values_in_any_encoding() {
        let text = br#"{"build": "20430", "x": {"excel_build":19929}, "rebuild": "5", "build": "16.0", "excel_build": 3.5, "note": "build", "build" : "7"}"#;
        assert_eq!(
            build_keys(text),
            BTreeSet::from(["20430".to_owned(), "19929".to_owned(), "7".to_owned()])
        );
        let utf16: Vec<u8> = "{\"excel_build\": \"20131\"}"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(build_keys(&utf16), BTreeSet::from(["20131".to_owned()]));
        assert!(build_keys(&[0; 16]).is_empty());
    }

    #[test]
    fn path_only_folders_come_from_names_and_numeric_provenance() {
        let mut entries = BTreeMap::new();
        for path in [
            "runs/W108-a/x.json",
            "runs/w109b/y.json",
            "runs/named/z.json",
            "runs/selected/k/fixture.json",
            "runs/suffixed/r-production-replay.json",
            "runs/open/z.json",
            "runs/cleared/z.json",
            "cache/w108-cache/z.json",
        ] {
            entries
                .entry(entry_of(path))
                .or_insert_with(Vec::new)
                .push(path.to_owned());
        }
        let provenance = json!({
            "modules": [
                {"id": "m1", "repository": "OxFunc", "path": "crates/x.py", "disposition": "unresolved",
                 "evidence": [{"clause": "reads smart-fuzzer/runs/named/, given as a path"}]},
                {"id": "m2", "repository": "OxFunc", "path": "smart-fuzzer/runs/selected/k/fixture.json", "disposition": "tainted"},
                {"id": "m3", "repository": "OxFunc", "path": "x", "disposition": "cleared",
                 "reason": "smart-fuzzer/runs/cleared/ is fine"}
            ],
            "classes": [
                {"id": "c1", "selector": {"repository": "OxFunc", "prefixes": ["smart-fuzzer/runs/"], "suffixes": ["-production-replay.json"]}, "disposition": "tainted"},
                {"id": "c2", "selector": {"repository": "OxXml", "prefixes": ["smart-fuzzer/"]}, "disposition": "tainted"}
            ]
        });
        let folders = path_only_folders(&entries, &provenance);
        let names: Vec<&str> = folders.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "runs/W108-a/",
                "runs/named/",
                "runs/selected/",
                "runs/suffixed/",
                "runs/w109b/"
            ]
        );
        assert_eq!(
            folders["runs/named/"],
            [format!(
                "{PROVENANCE} modules entry m1 (unresolved) names smart-fuzzer/runs/named/"
            )]
        );
        assert_eq!(
            folders["runs/selected/"],
            [format!(
                "{PROVENANCE} modules entry m2 (tainted) selects its files"
            )]
        );
        assert_eq!(
            folders["runs/suffixed/"],
            [format!(
                "{PROVENANCE} classes entry c1 (tainted) selects its files"
            )]
        );
        assert_eq!(
            folders["runs/W108-a/"],
            ["its name starts with w108, ignoring case"]
        );
    }

    #[test]
    fn render_round_trips_and_puts_the_entry_first() {
        let record = json!({
            "labels": [{"sources": [{"build": "1", "file": "a.json", "field": "excel_build"}], "entry": "cache/a.json", "files": 1}],
            "format": "f",
            "counts": {"known_by_build": {"20430": 2}, "entries": 1},
            "evidence_repository": {"commands": ["a", "b"], "url": "u"},
            "empty": [],
        });
        let text = render(&record);
        assert_eq!(serde_json::from_str::<Value>(&text).ok(), Some(record));
        assert!(text.starts_with("{\n  \"format\": \"f\",\n"), "{text}");
        assert!(
            text.contains(r#"    {"entry": "cache/a.json", "files": 1, "sources": [{"file": "a.json", "field": "excel_build", "build": "1"}]}"#),
            "{text}"
        );
    }
}
