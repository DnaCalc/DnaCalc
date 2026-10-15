//! Numeric provenance (bead infra-m0.5): the dispositions of `migration/numeric-provenance.json`,
//! the release source allowlist that `cargo xtask pin-sources` writes from them and the source
//! manifest, and the `numeric-provenance` check that every `cargo xtask accept` run (and
//! `cargo xtask ci`) performs among its architecture checks.
//!
//! A disposition is `cleared`, `tainted` or `unresolved`; the file's `dispositions` says what each
//! means. Plan sections 1.1 and 12 quarantine derived material, which is a property of a file's
//! own content, not of its call graph (the orchestrator's ruling of 2026-10-15 on infra-m0.5).
//! [`derive`] gives every manifest input its numeric provenance, the same way in the generator
//! (`pin.rs`) and in the check, from the input's own content as the manifest records it:
//!
//! - a module entry (one path at one reviewed blob; a different blob makes the input at least
//!   unresolved), else the first class whose selector matches, else `unresolved` (`no-review`)
//!   for an input the pin quarantines;
//! - at least `unresolved` when the citation sweep matched a line of its own text
//!   (`content_facts.citations`: the CRT chains, 87tran/87disp, the C-runtime routines; rule
//!   `citation-sweep`), when a commit the dispositions file lists as tainted changed it
//!   (`content_facts.changed_by`; rule `tainted-commit`), or when its code holds an include macro
//!   the pin cannot read (rule `unread-include`);
//! - at least the disposition of every file its code includes literally (rule
//!   `include-of-quarantined`, to a fixed point over includes of includes).
//!
//! A file that only calls (refers to) a tainted or unresolved file keeps its own disposition and
//! carries a `calls_quarantined` list (rule `calls-quarantined`): the files of its own repository
//! it refers to whose provenance is tainted or unresolved, each crate of another repository it
//! refers to that holds such a file (`<Repo> crate <name>`), and `<Repo> run-time loads` when its
//! code loads files or runs programs at run time. The release source allowlist holds exactly the
//! port inputs whose provenance is absent or `cleared`, each with its `calls_quarantined` list
//! when it has one; a port bead ports such a file only against clean replacements of every listed
//! module.
//!
//! The check reads committed files only, never a legacy repository, so it runs on every host and
//! in CI. It recomputes every input's numeric provenance and `calls_quarantined` list from the
//! dispositions file and the facts the manifest records per input (`code_refs`, `content_facts`)
//! and never trusts the manifest's recorded `numeric_provenance`. What it trusts, and cannot
//! re-derive without the legacy repositories, are those recorded facts themselves: the code edges,
//! the citation sweep's matches and the touched-by-commit lists (the infra-m0.5 acceptance case
//! recomputes the sweep and the touched-by-commit lists from git). It fails when the allowlist
//! lists an input the recomputation does not admit, leaves out one it admits, or lists one without
//! its exact `calls_quarantined` list; when the manifest records a provenance less restrictive than
//! the recomputation or another `calls_quarantined` list; when the manifest's facts were computed
//! with other sweep patterns or another tainted-commit list than the dispositions file holds; when
//! the dispositions file has no review record, its record does not match the review file it names,
//! or an entry is cleared without being covered by the recorded hash of the dispositions as
//! reviewed (a tainted or unresolved entry may be added or changed after the review when it is
//! marked so); when a class-wide or deferred entry names no owner bead; and when a file under
//! `migration/witnesses/` is not a retained witness selection, the directory exceeds its size
//! bound, a retained selection's bytes, source, selection rule, embedded capture provenance or
//! generators do not match its acquisition metadata, a generator is quarantined, tainted or
//! unresolved, or a generator builds or runs a package of a tainted class and the witness records
//! no equality of its bits with an earlier independent retention. Its summary counts the inputs of
//! every rule and of every class-wide or deferred entry together with the bead that owns it.

use crate::arch::CheckResult;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The dispositions, relative to the workspace root.
pub const DISPOSITIONS: &str = "migration/numeric-provenance.json";
/// The release source allowlist the generator writes beside the manifest.
pub const ALLOWLIST: &str = "migration/release-source-allowlist.json";
/// The file name of the allowlist, written beside whichever manifest the generator writes.
pub const ALLOWLIST_FILE: &str = "release-source-allowlist.json";
/// The retained witness set.
pub const WITNESSES: &str = "migration/witnesses";
pub const FORMAT: &str = "dnacalc-numeric-provenance/2";
pub const ALLOWLIST_FORMAT: &str = "dnacalc-release-source-allowlist/2";
/// The format of a retained witness selection.
pub const SELECTION_FORMAT: &str = "dnacalc-numeric-witness-selection/1";
/// The name of the architecture check.
pub const CHECK: &str = "numeric-provenance";
/// The entry name of an input the pin quarantines and no entry matches.
pub const NO_REVIEW: &str = "no-review";
const NO_REVIEW_REASON: &str =
    "the pin quarantines it and no entry of the dispositions file matches it";
/// The rules of the content-derivation ruling, by id; the dispositions file defines each one's
/// reason and owner.
pub const RULE_CITATION: &str = "citation-sweep";
pub const RULE_COMMIT: &str = "tainted-commit";
pub const RULE_UNREAD: &str = "unread-include";
pub const RULE_INCLUDE: &str = "include-of-quarantined";
pub const RULE_CALLS: &str = "calls-quarantined";
const RULES: [&str; 5] = [
    RULE_CITATION,
    RULE_COMMIT,
    RULE_UNREAD,
    RULE_INCLUDE,
    RULE_CALLS,
];
/// The `calls_quarantined` entry of an input whose code loads files or runs programs at run time.
pub const RUNTIME_LOADS: &str = "run-time loads";
/// The only method a retained witness may record.
const WITNESS_METHOD: &str = "excel-capture";
/// The cache mode a retained witness's capture must record: every answer computed live.
const WITNESS_CACHE: &str = "no_cache";
/// The file under the witness directory that is not a witness.
const WITNESS_ATTRIBUTES: &str = ".gitattributes";
/// What a generator of a retained witness may have done.
const GENERATOR_ROLES: [&str; 4] = ["expected_bits", "inputs", "driver", "snapshot"];
/// The review verdicts a review file may give.
const VERDICTS: [&str; 3] = ["confirmed", "confirmed_with_changes", "rejected"];
/// The hash of the dispositions as reviewed: the git blob ID of the canonical text.
pub const REVIEW_HASH_ALGORITHM: &str = "git-blob-sha1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Numeric {
    Cleared,
    Unresolved,
    Tainted,
}

impl Numeric {
    pub fn parse(text: &str) -> Option<Numeric> {
        match text {
            "cleared" => Some(Numeric::Cleared),
            "unresolved" => Some(Numeric::Unresolved),
            "tainted" => Some(Numeric::Tainted),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Numeric::Cleared => "cleared",
            Numeric::Unresolved => "unresolved",
            Numeric::Tainted => "tainted",
        }
    }
}

fn non_cleared(disposition: Option<Numeric>) -> bool {
    disposition.is_some_and(|d| d != Numeric::Cleared)
}

/// The numeric provenance of one input: its disposition (none when nothing in its own content
/// gives one), the entries and rules that gave it, why, the facts of its own content that a rule
/// applied to (`citations`, `changed_by`, `includes`, `unread_includes`), and the tainted or
/// unresolved modules it calls (`calls_quarantined`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tag {
    pub disposition: Option<Numeric>,
    pub entries: Vec<String>,
    pub reason: String,
    pub citations: Vec<String>,
    pub changed_by: Vec<String>,
    pub includes: Vec<String>,
    pub unread_includes: Vec<String>,
    pub calls_quarantined: Vec<String>,
}

impl Tag {
    fn entry(disposition: Numeric, id: &str, reason: &str) -> Tag {
        Tag {
            disposition: Some(disposition),
            entries: vec![id.to_owned()],
            reason: reason.to_owned(),
            ..Tag::default()
        }
    }

    pub fn no_review() -> Tag {
        Tag::entry(Numeric::Unresolved, NO_REVIEW, NO_REVIEW_REASON)
    }

    /// At least `disposition`, by the entry or rule `id` with `reason` (added once).
    fn raise(&mut self, disposition: Numeric, id: &str, reason: &str) {
        self.disposition = Some(self.disposition.map_or(disposition, |d| d.max(disposition)));
        if !self.entries.iter().any(|e| e == id) {
            self.entries.push(id.to_owned());
            if self.reason.is_empty() {
                self.reason = reason.to_owned();
            } else {
                self.reason = format!("{}; and {reason}", self.reason);
            }
        }
    }

    pub fn json(&self) -> Value {
        let mut record = serde_json::Map::new();
        if let Some(disposition) = self.disposition {
            record.insert("disposition".to_owned(), json!(disposition.as_str()));
        }
        record.insert("entries".to_owned(), json!(self.entries));
        record.insert("reason".to_owned(), json!(self.reason));
        for (key, list) in [
            ("citations", &self.citations),
            ("changed_by", &self.changed_by),
            ("includes", &self.includes),
            ("unread_includes", &self.unread_includes),
            ("calls_quarantined", &self.calls_quarantined),
        ] {
            if !list.is_empty() {
                record.insert(key.to_owned(), json!(list));
            }
        }
        Value::Object(record)
    }
}

/// The code edges of one Rust input, as the manifest records them under `code_refs`: the files of
/// the same repository its non-test code refers to (`refs`) and includes literally (`includes`),
/// the include macros of its non-test code whose argument is not one string literal
/// (`unread_includes`, which the pin cannot follow), the library crates of other repositories it
/// refers to (`crates`, as `<Repo> crate <name>`), the run-time loads its code makes (`runtime`:
/// the calls that read files or run programs), and what its test code adds (`test_refs`,
/// `test_includes`, `test_unread_includes`, `test_crates`, `test_runtime`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub refs: Vec<String>,
    pub includes: Vec<String>,
    pub unread_includes: Vec<String>,
    pub crates: Vec<String>,
    pub runtime: Vec<String>,
    pub test_refs: Vec<String>,
    pub test_includes: Vec<String>,
    pub test_unread_includes: Vec<String>,
    pub test_crates: Vec<String>,
    pub test_runtime: Vec<String>,
}

impl Edges {
    const KEYS: [&'static str; 10] = [
        "refs",
        "includes",
        "unread_includes",
        "crates",
        "runtime",
        "test_refs",
        "test_includes",
        "test_unread_includes",
        "test_crates",
        "test_runtime",
    ];

    fn lists(&self) -> [&Vec<String>; 10] {
        [
            &self.refs,
            &self.includes,
            &self.unread_includes,
            &self.crates,
            &self.runtime,
            &self.test_refs,
            &self.test_includes,
            &self.test_unread_includes,
            &self.test_crates,
            &self.test_runtime,
        ]
    }

    pub fn is_empty(&self) -> bool {
        self.lists().iter().all(|list| list.is_empty())
    }

    /// The record, holding only the non-empty lists.
    pub fn json(&self) -> Value {
        let mut record = serde_json::Map::new();
        for (key, list) in Edges::KEYS.iter().zip(self.lists()) {
            if !list.is_empty() {
                record.insert((*key).to_owned(), json!(list));
            }
        }
        Value::Object(record)
    }

    pub fn from_json(value: &Value) -> Edges {
        let list = |key: &str| -> Vec<String> { list_of(value, key) };
        Edges {
            refs: list("refs"),
            includes: list("includes"),
            unread_includes: list("unread_includes"),
            crates: list("crates"),
            runtime: list("runtime"),
            test_refs: list("test_refs"),
            test_includes: list("test_includes"),
            test_unread_includes: list("test_unread_includes"),
            test_crates: list("test_crates"),
            test_runtime: list("test_runtime"),
        }
    }
}

fn list_of(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// One line of an input's own text that the citation sweep matched: its number and the pattern.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Citation {
    pub line: u64,
    pub pattern: String,
}

/// What the pin read in one input's own content, as the manifest records it under
/// `content_facts`: the lines the citation sweep matched, and the commits of the dispositions
/// file's tainted list that changed the file (`changed_by`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub citations: Vec<Citation>,
    pub changed_by: Vec<String>,
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.citations.is_empty() && self.changed_by.is_empty()
    }

    pub fn json(&self) -> Value {
        let mut record = serde_json::Map::new();
        if !self.citations.is_empty() {
            record.insert(
                "citations".to_owned(),
                Value::Array(
                    self.citations
                        .iter()
                        .map(|c| json!({"line": c.line, "pattern": c.pattern}))
                        .collect(),
                ),
            );
        }
        if !self.changed_by.is_empty() {
            record.insert("changed_by".to_owned(), json!(self.changed_by));
        }
        Value::Object(record)
    }

    pub fn from_json(value: &Value) -> Facts {
        Facts {
            citations: value["citations"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| Citation {
                    line: c["line"].as_u64().unwrap_or(0),
                    pattern: c["pattern"].as_str().unwrap_or_default().to_owned(),
                })
                .collect(),
            changed_by: list_of(value, "changed_by"),
        }
    }
}

/// The lines of `text` the citation sweep matches: a line holds a pattern when the pattern occurs
/// in it, case-sensitively, not directly after an ASCII letter, digit or underscore. Each line
/// is reported once, with the first pattern (in the given order) it holds.
pub fn sweep(text: &str, patterns: &[String]) -> Vec<Citation> {
    let mut found = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let hit = patterns.iter().find(|pattern| {
            line.match_indices(pattern.as_str()).any(|(at, _)| {
                at == 0
                    || !line[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        });
        if let Some(pattern) = hit {
            found.push(Citation {
                line: index as u64 + 1,
                pattern: pattern.clone(),
            });
        }
    }
    found
}

/// One manifest input as [`derive`] reads it.
pub struct Node {
    pub repository: String,
    pub path: String,
    pub blob: Option<String>,
    /// The pin quarantines it (read disposition quarantined or quarantine_derived).
    pub quarantine: bool,
    pub edges: Edges,
    pub facts: Facts,
}

impl Node {
    /// The node of a manifest input record.
    pub fn of(input: &Value) -> Node {
        Node {
            repository: input["repository"].as_str().unwrap_or_default().to_owned(),
            path: input["path"].as_str().unwrap_or_default().to_owned(),
            blob: input["blob"].as_str().map(str::to_owned),
            quarantine: matches!(
                input["disposition"].as_str(),
                Some("quarantined" | "quarantine_derived")
            ),
            edges: Edges::from_json(&input["code_refs"]),
            facts: Facts::from_json(&input["content_facts"]),
        }
    }
}

pub type Key = (String, String);

/// The library crates of other repositories that inputs refer to: `<Repo> crate <name>` to the
/// repository and its `src` directory.
pub type Crates = BTreeMap<String, (String, String)>;

/// The `calls_quarantined` entry of a library crate of another repository.
pub fn crate_label(repository: &str, name: &str) -> String {
    format!("{repository} crate {name}")
}

/// The `calls_quarantined` entry of an input of `repository` that loads files at run time.
pub fn runtime_label(repository: &str) -> String {
    format!("{repository} {RUNTIME_LOADS}")
}

/// The crates the manifest records under `numeric_content.library_crates`.
pub fn crates_of(manifest: &Value) -> Crates {
    manifest["numeric_content"]["library_crates"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(label, value)| {
            (
                label.clone(),
                (
                    value["repository"].as_str().unwrap_or_default().to_owned(),
                    value["src"].as_str().unwrap_or_default().to_owned(),
                ),
            )
        })
        .collect()
}

/// The bead that owns the replacement of a class-wide or deferred entry, and what replaces it.
#[derive(Clone, Debug)]
pub struct Owner {
    pub bead: String,
    pub replacement: String,
}

fn owner_of(value: &Value, what: &str) -> Result<Option<Owner>, String> {
    let owner = &value["owner"];
    if owner.is_null() {
        return Ok(None);
    }
    Ok(Some(Owner {
        bead: text_field(owner, "bead", &format!("{what} owner"))?.to_owned(),
        replacement: text_field(owner, "replacement", &format!("{what} owner"))?.to_owned(),
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub repository: String,
    pub path: String,
    pub blob: String,
}

#[derive(Clone, Debug)]
pub struct Generator {
    pub role: String,
    pub source: Source,
    /// The commit whose tree holds `source.blob` at `source.path`, when that is not the pinned
    /// blob of the path.
    pub commit: Option<String>,
    /// The package directories (of the generator's repository) it builds or runs.
    pub builds_or_runs: Vec<String>,
    /// The package directories its text names without building or running them (each recorded
    /// with the lines that name it and why); a package of a tainted class named this way needs
    /// the same equality proof as one built or run.
    pub names_packages: Vec<String>,
}

/// An earlier retention of the same capture, made independently of the witness's generator chain,
/// with which the witness's expected bits were compared row by row.
#[derive(Clone, Debug)]
pub struct Retention {
    pub source: Source,
    pub compared_rows: u64,
    pub matched_rows: u64,
}

/// How a retained witness selects rows from its source capture: the first `edge_rows` rows (the
/// input generator's fixed edge rows), then every `stride`-th of the `random_rows` seeded random
/// rows that follow, starting with the first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub edge_rows: u64,
    pub random_rows: u64,
    pub stride: u64,
}

impl Selection {
    /// The row indexes of the source that the rule selects, in order.
    pub fn indexes(&self) -> Vec<u64> {
        let mut out: Vec<u64> = (0..self.edge_rows).collect();
        if self.stride > 0 {
            out.extend(
                (0..self.random_rows)
                    .step_by(self.stride as usize)
                    .map(|k| self.edge_rows + k),
            );
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Witness {
    pub file: String,
    /// The git blob ID of the retained selection file.
    pub blob: String,
    pub function: String,
    pub rows: u64,
    pub selection: Selection,
    pub source: Source,
    pub source_commit: String,
    pub source_sha256: String,
    pub source_rows: u64,
    pub acquisition: Value,
    pub generators: Vec<Generator>,
    pub retention: Option<Retention>,
}

struct Module {
    id: String,
    repository: String,
    path: String,
    blob: String,
    disposition: Numeric,
    reason: String,
    owner: Option<Owner>,
    after_review: bool,
}

struct Selector {
    repository: String,
    paths: BTreeSet<String>,
    prefixes: Vec<String>,
    suffixes: Vec<String>,
}

impl Selector {
    fn matches(&self, repository: &str, path: &str) -> bool {
        if self.repository != repository {
            return false;
        }
        if self.paths.contains(path) {
            return true;
        }
        !self.prefixes.is_empty()
            && self.prefixes.iter().any(|p| path.starts_with(p.as_str()))
            && (self.suffixes.is_empty()
                || self.suffixes.iter().any(|s| path.ends_with(s.as_str())))
    }

    /// Whether it may select a path under `dir` (which ends with `/`).
    fn reaches_under(&self, repository: &str, dir: &str) -> bool {
        self.repository == repository
            && (self.paths.iter().any(|p| p.starts_with(dir))
                || self
                    .prefixes
                    .iter()
                    .any(|p| p.starts_with(dir) || dir.starts_with(p.as_str())))
    }
}

struct Class {
    id: String,
    selector: Selector,
    disposition: Numeric,
    reason: String,
    owner: Option<Owner>,
    after_review: bool,
}

/// A rule of the content-derivation ruling: its reason and owner.
#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub reason: String,
    pub owner: Option<Owner>,
}

/// A commit the dispositions file lists as tainted: a file it changed is at least unresolved.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaintedCommit {
    pub repository: String,
    pub commit: String,
}

/// A gap the dispositions file records with the bead that owns it.
#[derive(Clone, Debug)]
pub struct Gap {
    pub what: String,
    pub owner: String,
}

/// The dispositions file, parsed and checked.
pub struct Reviewed {
    modules: Vec<Module>,
    classes: Vec<Class>,
    rules: BTreeMap<String, Rule>,
    pub tainted_commits: Vec<TaintedCommit>,
    pub sweep_patterns: Vec<String>,
    pub witnesses: Vec<Witness>,
    /// The size bound of the witness directory, in bytes.
    witness_limit: Option<u64>,
    /// Each excluded witness candidate: its repository and the paths it names.
    excluded: Vec<(Option<String>, Vec<String>)>,
    /// The review record (`status.review`), when the file has one.
    review: Value,
    gaps: Vec<Gap>,
    /// Every entry id with its disposition, for comparison with the review file.
    dispositions_by_id: BTreeMap<String, Numeric>,
    /// The hash of the entries as reviewed, recomputed from the file.
    reviewed_hash: Result<String, String>,
}

fn text_field<'a>(value: &'a Value, key: &str, what: &str) -> Result<&'a str, String> {
    match value[key].as_str() {
        Some(text) if !text.trim().is_empty() => Ok(text),
        _ => Err(format!("{what} has no {key}")),
    }
}

fn strings(value: &Value, what: &str) -> Result<Vec<String>, String> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{what} holds a value that is not a string"))
            })
            .collect(),
        _ => Err(format!("{what} is not a list")),
    }
}

fn disposition_of(value: &Value, what: &str) -> Result<Numeric, String> {
    let text = text_field(value, "disposition", what)?;
    Numeric::parse(text).ok_or_else(|| {
        format!("{what} has disposition {text:?}, not one of cleared, tainted or unresolved")
    })
}

fn number(value: &Value, key: &str, what: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("{what} has no {key}"))
}

/// Every evidence item quotes something and says where it is: a header, comment or code line of
/// the entry's own file (or, with `path` and `blob`, of another), a line of a document, a commit
/// message, or a value of a JSON data file at a pointer.
fn check_evidence(value: &Value, what: &str) -> Result<(), String> {
    let items = value["evidence"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| format!("{what} records no evidence"))?;
    for (index, item) in items.iter().enumerate() {
        let shown = format!("{what} evidence {}", index + 1);
        let kind = text_field(item, "kind", &shown)?;
        if kind != "data" {
            text_field(item, "quote", &shown)?;
        }
        match kind {
            "header" | "comment" | "code" => {
                number(item, "line", &shown)?;
                if !item["path"].is_null() {
                    text_field(item, "path", &shown)?;
                    text_field(item, "blob", &shown)?;
                }
            }
            "doc" => {
                text_field(item, "path", &shown)?;
                text_field(item, "blob", &shown)?;
                number(item, "line", &shown)?;
            }
            "commit" => {
                text_field(item, "commit", &shown)?;
            }
            "data" => {
                text_field(item, "path", &shown)?;
                text_field(item, "blob", &shown)?;
                text_field(item, "pointer", &shown)?;
                if item.get("value").is_none() {
                    return Err(format!("{shown} has no value"));
                }
            }
            other => {
                return Err(format!(
                    "{shown} has kind {other:?}, not header, comment, code, doc, commit or data"
                ));
            }
        }
    }
    Ok(())
}

/// Whether an entry is marked as added or changed after the review, and the mark is well formed.
fn after_review_of(entry: &Value, what: &str) -> Result<bool, String> {
    let mark = &entry["after_review"];
    if mark.is_null() {
        return Ok(false);
    }
    let shown = format!("{what} after_review");
    match text_field(mark, "kind", &shown)? {
        "added" => {}
        "changed" => {
            if !mark["reviewed_as"].is_object() {
                return Err(format!("{shown} is a change but records no reviewed_as"));
            }
        }
        other => return Err(format!("{shown} has kind {other:?}, not added or changed")),
    }
    text_field(mark, "date", &shown)?;
    text_field(mark, "reason", &shown)?;
    Ok(true)
}

fn source_of(value: &Value, what: &str) -> Result<Source, String> {
    Ok(Source {
        repository: text_field(value, "repository", what)?.to_owned(),
        path: text_field(value, "path", what)?.to_owned(),
        blob: text_field(value, "blob", what)?.to_owned(),
    })
}

fn is_hex_id(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn witness_of(entry: &Value) -> Result<Witness, String> {
    let file = text_field(entry, "file", "a retained witness")?.to_owned();
    let what = format!("witness {file}");
    if !file.starts_with(&format!("{WITNESSES}/")) {
        return Err(format!("{what} is not under {WITNESSES}/"));
    }
    let acquisition = entry["acquisition"].clone();
    if !acquisition.is_object() {
        return Err(format!("{what} has no acquisition metadata"));
    }
    let generators = entry["generators"]
        .as_array()
        .filter(|g| !g.is_empty())
        .ok_or_else(|| format!("{what} names no generator"))?
        .iter()
        .map(|g| {
            let shown = format!("{what} generator");
            let role = text_field(g, "role", &shown)?.to_owned();
            if !GENERATOR_ROLES.contains(&role.as_str()) {
                return Err(format!(
                    "{shown} has role {role:?}, not one of {GENERATOR_ROLES:?}"
                ));
            }
            let commit = g["commit"].as_str().map(str::to_owned);
            if commit.as_deref().is_some_and(|c| !is_hex_id(c)) {
                return Err(format!("{shown} names a commit that is not a 40-digit ID"));
            }
            let builds_or_runs = strings(&g["builds_or_runs"], &format!("{shown} builds_or_runs"))?;
            if builds_or_runs.iter().any(|dir| !dir.ends_with('/')) {
                return Err(format!(
                    "{shown} builds_or_runs names a package that is not a directory ending with /"
                ));
            }
            let mut names_packages = Vec::new();
            for named in g["names_packages"].as_array().into_iter().flatten() {
                let package = text_field(named, "package", &format!("{shown} names_packages"))?;
                if !package.ends_with('/') {
                    return Err(format!(
                        "{shown} names_packages names a package that is not a directory ending with /"
                    ));
                }
                text_field(named, "why", &format!("{shown} names_packages"))?;
                if named["lines"].as_array().is_none_or(|l| l.is_empty()) {
                    return Err(format!("{shown} names_packages records no lines"));
                }
                names_packages.push(package.to_owned());
            }
            Ok(Generator {
                role,
                source: source_of(g, &shown)?,
                commit,
                builds_or_runs,
                names_packages,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let retention = match &entry["independent_retention"] {
        Value::Null => None,
        record => {
            let shown = format!("{what} independent_retention");
            Some(Retention {
                source: source_of(record, &shown)?,
                compared_rows: number(record, "compared_rows", &shown)?,
                matched_rows: number(record, "matched_rows", &shown)?,
            })
        }
    };
    let source = &entry["source"];
    let shown_source = format!("{what} source");
    let selection = &entry["selection"];
    let shown_selection = format!("{what} selection");
    text_field(selection, "rule", &shown_selection)?;
    Ok(Witness {
        blob: text_field(entry, "blob", &what)?.to_owned(),
        function: text_field(entry, "function", &what)?.to_owned(),
        rows: number(entry, "rows", &what)?,
        selection: Selection {
            edge_rows: number(selection, "edge_rows", &shown_selection)?,
            random_rows: number(selection, "random_rows", &shown_selection)?,
            stride: number(selection, "stride", &shown_selection)?,
        },
        source: source_of(source, &shown_source)?,
        source_commit: text_field(source, "commit", &shown_source)?.to_owned(),
        source_sha256: text_field(source, "sha256", &shown_source)?.to_owned(),
        source_rows: number(source, "rows", &shown_source)?,
        acquisition,
        generators,
        retention,
        file,
    })
}

/// The entries as the review saw them, in a canonical form: every module and class entry without
/// an `after_review` mark as it stands, every one marked as changed as its `reviewed_as` record,
/// and none marked as added. Only the fields that decide a disposition are kept (id, disposition,
/// repository, path, blob, selector, and each symbol group's names and disposition), so editing a
/// reason or an evidence quote does not change it.
pub fn reviewed_view(file: &Value) -> Result<Value, String> {
    let mut view = Vec::new();
    for (kind, key) in [("module", "modules"), ("class", "classes")] {
        for entry in file[key].as_array().into_iter().flatten() {
            let mark = &entry["after_review"];
            let base = match mark["kind"].as_str() {
                None => entry,
                Some("added") => continue,
                Some("changed") => &mark["reviewed_as"],
                Some(other) => {
                    return Err(format!(
                        "{kind} {} has after_review kind {other:?}",
                        entry["id"]
                    ));
                }
            };
            let mut record =
                json!({"kind": kind, "id": entry["id"], "disposition": base["disposition"]});
            for field in ["repository", "path", "blob", "selector"] {
                if !base[field].is_null() {
                    record[field] = base[field].clone();
                }
            }
            if let Some(groups) = base["symbols"].as_array() {
                record["symbols"] = Value::Array(
                    groups
                        .iter()
                        .map(|g| json!({"names": g["names"], "disposition": g["disposition"]}))
                        .collect(),
                );
            }
            view.push(record);
        }
    }
    view.sort_by_key(|r| (r["kind"].to_string(), r["id"].to_string()));
    Ok(Value::Array(view))
}

/// The hash of [`reviewed_view`]: the git blob ID of its compact JSON text.
pub fn reviewed_hash(file: &Value) -> Result<String, String> {
    Ok(blob_id(reviewed_view(file)?.to_string().as_bytes()))
}

impl Reviewed {
    /// The rules of the content-derivation ruling and no entry.
    #[cfg(test)]
    pub fn empty() -> Reviewed {
        Reviewed::parse(&fixture_text(json!([]), json!([]), json!([]))).expect("parses")
    }

    pub fn load(path: &Path) -> Result<Reviewed, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("cannot read the dispositions file {}: {e}", path.display()))?;
        Reviewed::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Reviewed, String> {
        let file: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        if file["format"] != FORMAT {
            return Err(format!("format is {}, not {FORMAT}", file["format"]));
        }
        let mut dispositions_by_id = BTreeMap::new();
        let mut ids: BTreeSet<String> = BTreeSet::new();
        let mut unique = |id: &str| -> Result<(), String> {
            if ids.insert(id.to_owned()) {
                Ok(())
            } else {
                Err(format!("the entry id {id} appears twice"))
            }
        };
        unique(NO_REVIEW)?;

        let mut rules = BTreeMap::new();
        for rule in file["rules"].as_array().ok_or("rules is not a list")? {
            let id = text_field(rule, "id", "a rule")?.to_owned();
            let what = format!("rule {id}");
            if !RULES.contains(&id.as_str()) {
                return Err(format!("{what} is not one of {RULES:?}"));
            }
            unique(&id)?;
            rules.insert(
                id.clone(),
                Rule {
                    reason: text_field(rule, "reason", &what)?.to_owned(),
                    owner: owner_of(rule, &what)?,
                    id,
                },
            );
        }
        if let Some(missing) = RULES.iter().find(|r| !rules.contains_key(**r)) {
            return Err(format!("rules has no rule {missing}"));
        }

        let mut tainted_commits = Vec::new();
        for entry in file["tainted_commits"]
            .as_array()
            .ok_or("tainted_commits is not a list")?
        {
            let commit = text_field(entry, "commit", "a tainted commit")?.to_owned();
            let what = format!("tainted commit {commit}");
            if !is_hex_id(&commit) {
                return Err(format!("{what} is not a 40-digit ID"));
            }
            text_field(entry, "reason", &what)?;
            let record = TaintedCommit {
                repository: text_field(entry, "repository", &what)?.to_owned(),
                commit,
            };
            if tainted_commits.contains(&record) {
                return Err(format!("{what} is listed twice"));
            }
            tainted_commits.push(record);
        }
        let sweep_patterns = strings(
            &file["citation_sweep"]["patterns"],
            "citation_sweep.patterns",
        )?;
        if sweep_patterns.is_empty() || sweep_patterns.iter().any(|p| p.trim().is_empty()) {
            return Err("citation_sweep.patterns is empty or holds an empty pattern".to_owned());
        }

        let mut modules = Vec::new();
        for entry in file["modules"].as_array().ok_or("modules is not a list")? {
            let id = text_field(entry, "id", "a module entry")?.to_owned();
            let what = format!("module {id}");
            check_evidence(entry, &what)?;
            let module = Module {
                repository: text_field(entry, "repository", &what)?.to_owned(),
                path: text_field(entry, "path", &what)?.to_owned(),
                blob: text_field(entry, "blob", &what)?.to_owned(),
                disposition: disposition_of(entry, &what)?,
                reason: text_field(entry, "reason", &what)?.to_owned(),
                owner: owner_of(entry, &what)?,
                after_review: after_review_of(entry, &what)?,
                id,
            };
            unique(&module.id)?;
            dispositions_by_id.insert(module.id.clone(), module.disposition);
            if modules
                .iter()
                .any(|m: &Module| m.repository == module.repository && m.path == module.path)
            {
                return Err(format!("{what}: {} has two module entries", module.path));
            }
            modules.push(module);
        }

        let mut classes = Vec::new();
        for entry in file["classes"].as_array().ok_or("classes is not a list")? {
            let id = text_field(entry, "id", "a class entry")?.to_owned();
            let what = format!("class {id}");
            check_evidence(entry, &what)?;
            let selector = &entry["selector"];
            let selector = Selector {
                repository: text_field(selector, "repository", &format!("{what} selector"))?
                    .to_owned(),
                paths: strings(&selector["paths"], &format!("{what} paths"))?
                    .into_iter()
                    .collect(),
                prefixes: strings(&selector["prefixes"], &format!("{what} prefixes"))?,
                suffixes: strings(&selector["suffixes"], &format!("{what} suffixes"))?,
            };
            if selector.paths.is_empty() && selector.prefixes.is_empty() {
                return Err(format!(
                    "{what} selects nothing: it has no paths and no prefixes"
                ));
            }
            let class = Class {
                selector,
                disposition: disposition_of(entry, &what)?,
                reason: text_field(entry, "reason", &what)?.to_owned(),
                owner: owner_of(entry, &what)?,
                after_review: after_review_of(entry, &what)?,
                id,
            };
            unique(&class.id)?;
            dispositions_by_id.insert(class.id.clone(), class.disposition);
            classes.push(class);
        }

        let mut witnesses = Vec::new();
        let mut excluded = Vec::new();
        let mut witness_limit = None;
        let set = &file["witnesses"];
        if !set.is_null() {
            witness_limit = Some(number(set, "max_total_bytes", "witnesses")?);
            for entry in set["retained"]
                .as_array()
                .ok_or("witnesses.retained is not a list")?
            {
                witnesses.push(witness_of(entry)?);
            }
            for entry in set["excluded"]
                .as_array()
                .ok_or("witnesses.excluded is not a list")?
            {
                text_field(entry, "reason", "an excluded witness")?;
                excluded.push((
                    entry["repository"].as_str().map(str::to_owned),
                    strings(&entry["paths"], "an excluded witness's paths")?,
                ));
            }
        }
        let mut gaps = Vec::new();
        for gap in file["gaps"].as_array().into_iter().flatten() {
            gaps.push(Gap {
                what: text_field(gap, "what", "a gap")?.to_owned(),
                owner: text_field(gap, "owner", "a gap")?.to_owned(),
            });
        }
        Ok(Reviewed {
            modules,
            classes,
            rules,
            tainted_commits,
            sweep_patterns,
            witnesses,
            witness_limit,
            excluded,
            review: file["status"]["review"].clone(),
            gaps,
            dispositions_by_id,
            reviewed_hash: reviewed_hash(&file),
        })
    }

    fn rule(&self, id: &str) -> &Rule {
        &self.rules[id]
    }

    /// The reason of a rule, for a finding or a lookup.
    pub fn rule_reason(&self, id: &str) -> &str {
        &self.rule(id).reason
    }

    /// Whether `commit` of `repository` is listed as tainted.
    pub fn is_tainted_commit(&self, repository: &str, commit: &str) -> bool {
        self.tainted_commits
            .iter()
            .any(|t| t.repository == repository && t.commit == commit)
    }

    /// The disposition of `<repository>/<path>` pinned at `blob`, if any entry applies: a module
    /// entry for the path (at least unresolved when the blob is not the reviewed one), else the
    /// first matching class.
    pub fn tag(&self, repository: &str, path: &str, blob: Option<&str>) -> Option<Tag> {
        if let Some(module) = self
            .modules
            .iter()
            .find(|m| m.repository == repository && m.path == path)
        {
            return Some(if blob == Some(module.blob.as_str()) {
                Tag::entry(module.disposition, &module.id, &module.reason)
            } else {
                Tag::entry(
                    module.disposition.max(Numeric::Unresolved),
                    &module.id,
                    &format!(
                        "the review read blob {} but the pin holds {}; the new content is not reviewed ({})",
                        module.blob,
                        blob.unwrap_or("no blob"),
                        module.reason
                    ),
                )
            });
        }
        self.classes
            .iter()
            .find(|c| c.selector.matches(repository, path))
            .map(|c| Tag::entry(c.disposition, &c.id, &c.reason))
    }

    /// Whether an entry that is not cleared may select a file under `dir` (ending with `/`).
    fn non_cleared_under(&self, repository: &str, dir: &str) -> bool {
        self.modules.iter().any(|m| {
            m.repository == repository
                && m.path.starts_with(dir)
                && m.disposition != Numeric::Cleared
        }) || self
            .classes
            .iter()
            .any(|c| c.disposition != Numeric::Cleared && c.selector.reaches_under(repository, dir))
    }

    /// The module entries whose path is neither among `inputs` (`Repo/path` keys) nor the source
    /// of a retained witness.
    pub fn modules_outside(&self, inputs: &BTreeSet<String>) -> Vec<String> {
        let sources: BTreeSet<String> = self
            .witnesses
            .iter()
            .map(|w| format!("{}/{}", w.source.repository, w.source.path))
            .collect();
        self.modules
            .iter()
            .filter(|m| {
                let key = format!("{}/{}", m.repository, m.path);
                !inputs.contains(&key) && !sources.contains(&key)
            })
            .map(|m| m.id.clone())
            .collect()
    }

    fn excluded_contains(&self, source: &Source) -> bool {
        self.excluded.iter().any(|(repository, paths)| {
            repository.as_deref() == Some(source.repository.as_str())
                && paths.contains(&source.path)
        })
    }

    /// Every entry and rule that names an owner bead: (id, what it is, owner).
    pub fn owned_entries(&self) -> Vec<(String, String, Owner)> {
        let mut out = Vec::new();
        for rule in self.rules.values() {
            if let Some(owner) = &rule.owner {
                out.push((rule.id.clone(), "rule".to_owned(), owner.clone()));
            }
        }
        for module in &self.modules {
            if let Some(owner) = &module.owner {
                out.push((
                    module.id.clone(),
                    module.disposition.as_str().to_owned(),
                    owner.clone(),
                ));
            }
        }
        for class in &self.classes {
            if let Some(owner) = &class.owner {
                out.push((
                    class.id.clone(),
                    class.disposition.as_str().to_owned(),
                    owner.clone(),
                ));
            }
        }
        out
    }

    /// The entries and rules that are class-wide or deferred but name no owner bead: every rule,
    /// every class that selects by prefix, and every entry that is unresolved.
    fn unowned(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .rules
            .values()
            .filter(|r| r.owner.is_none())
            .map(|r| format!("rule {}", r.id))
            .collect();
        out.extend(
            self.modules
                .iter()
                .filter(|m| m.owner.is_none() && m.disposition == Numeric::Unresolved)
                .map(|m| format!("module {} (unresolved)", m.id)),
        );
        out.extend(
            self.classes
                .iter()
                .filter(|c| {
                    c.owner.is_none()
                        && (!c.selector.prefixes.is_empty() || c.disposition == Numeric::Unresolved)
                })
                .map(|c| format!("class {} ({})", c.id, c.disposition.as_str())),
        );
        out
    }

    /// The retained witnesses as the manifest records them, for `--lookup`.
    pub fn retained_json(&self) -> Value {
        Value::Array(
            self.witnesses
                .iter()
                .map(|w| {
                    let entry = self
                        .tag(&w.source.repository, &w.source.path, Some(&w.source.blob))
                        .and_then(|t| {
                            t.disposition
                                .map(|d| (d.as_str(), t.entries.join(", ")))
                        });
                    json!({
                        "file": w.file,
                        "function": w.function,
                        "rows": w.rows,
                        "source": {
                            "repository": w.source.repository,
                            "commit": w.source_commit,
                            "path": w.source.path,
                            "blob": w.source.blob,
                            "sha256": w.source_sha256,
                            "rows": w.source_rows,
                        },
                        "numeric_provenance": entry.map(|(d, e)| json!({"disposition": d, "entries": [e]})),
                    })
                })
                .collect(),
        )
    }
}

/// A dispositions file text with the five rules, the given modules, classes and tainted commits,
/// for the unit tests here and in `pin.rs`.
#[cfg(test)]
pub fn fixture_text(modules: Value, classes: Value, tainted: Value) -> String {
    let rule = |id: &str| json!({"id": id, "reason": format!("fixture {id}"), "owner": {"bead": "dc-fixture", "replacement": "fixture"}});
    json!({
        "format": FORMAT,
        "rules": RULES.iter().map(|id| rule(id)).collect::<Vec<_>>(),
        "tainted_commits": tainted,
        "citation_sweep": {"patterns": ["87tran", "C-runtime"]},
        "modules": modules,
        "classes": classes,
    })
    .to_string()
}

/// The disposition of `target`: its tag when it is an input, else the entry for its path (blob
/// unknown), else unresolved when it lies under the quarantined directory, else none.
fn target_disposition(
    target: &Key,
    tags: &BTreeMap<Key, Tag>,
    inputs: &BTreeSet<Key>,
    reviewed: &Reviewed,
) -> Option<Numeric> {
    if inputs.contains(target) {
        return tags.get(target).and_then(|t| t.disposition);
    }
    reviewed
        .tag(&target.0, &target.1, None)
        .and_then(|t| t.disposition)
        .or_else(|| crate::pin::quarantined(&target.0, &target.1).then_some(Numeric::Unresolved))
}

/// Every input's numeric provenance from its own content (see the module documentation), and its
/// `calls_quarantined` list. An input whose tag would be empty has none.
pub fn derive(nodes: &[Node], reviewed: &Reviewed, crates: &Crates) -> BTreeMap<Key, Tag> {
    let key = |repository: &str, path: &str| (repository.to_owned(), path.to_owned());
    let inputs: BTreeSet<Key> = nodes.iter().map(|n| key(&n.repository, &n.path)).collect();
    let mut tags: BTreeMap<Key, Tag> = BTreeMap::new();
    for node in nodes {
        let mut tag = reviewed
            .tag(&node.repository, &node.path, node.blob.as_deref())
            .or_else(|| node.quarantine.then(Tag::no_review))
            .unwrap_or_default();
        if !node.facts.citations.is_empty() {
            tag.citations = node
                .facts
                .citations
                .iter()
                .map(|c| format!("line {} ({})", c.line, c.pattern))
                .collect();
            tag.raise(
                Numeric::Unresolved,
                RULE_CITATION,
                reviewed.rule_reason(RULE_CITATION),
            );
        }
        let changed: Vec<String> = node
            .facts
            .changed_by
            .iter()
            .filter(|c| reviewed.is_tainted_commit(&node.repository, c))
            .cloned()
            .collect();
        if !changed.is_empty() {
            tag.changed_by = changed;
            tag.raise(
                Numeric::Unresolved,
                RULE_COMMIT,
                reviewed.rule_reason(RULE_COMMIT),
            );
        }
        let unread: Vec<String> = node
            .edges
            .unread_includes
            .iter()
            .chain(&node.edges.test_unread_includes)
            .cloned()
            .collect();
        if !unread.is_empty() {
            tag.unread_includes = unread;
            tag.raise(
                Numeric::Unresolved,
                RULE_UNREAD,
                reviewed.rule_reason(RULE_UNREAD),
            );
        }
        if tag != Tag::default() {
            tags.insert(key(&node.repository, &node.path), tag);
        }
    }
    // An include embeds the included file: at least its disposition, to a fixed point.
    loop {
        let mut moved = false;
        for node in nodes {
            let own = key(&node.repository, &node.path);
            for target in node.edges.includes.iter().chain(&node.edges.test_includes) {
                if target == &node.path {
                    continue;
                }
                let found =
                    target_disposition(&key(&node.repository, target), &tags, &inputs, reviewed);
                let Some(disposition) = found.filter(|d| *d != Numeric::Cleared) else {
                    continue;
                };
                let tag = tags.entry(own.clone()).or_default();
                let before = (tag.disposition, tag.includes.len());
                if !tag.includes.contains(target) {
                    tag.includes.push(target.clone());
                    tag.includes.sort();
                }
                tag.raise(
                    disposition,
                    RULE_INCLUDE,
                    reviewed.rule_reason(RULE_INCLUDE),
                );
                moved |= before != (tag.disposition, tag.includes.len());
            }
        }
        if !moved {
            break;
        }
    }
    // What each input calls: the files of its repository whose provenance is not cleared, the
    // crates of other repositories that hold such a file, and its run-time loads.
    let settled: BTreeMap<Key, Option<Numeric>> = tags
        .iter()
        .map(|(k, t)| (k.clone(), t.disposition))
        .collect();
    let bad = |target: &Key| -> bool {
        if inputs.contains(target) {
            return non_cleared(settled.get(target).copied().flatten());
        }
        non_cleared(target_disposition(target, &tags, &inputs, reviewed))
    };
    let crate_bad = |label: &str| -> bool {
        let Some((repository, src)) = crates.get(label) else {
            return true;
        };
        let dir = format!("{src}/");
        crate::pin::quarantined_under(repository, &dir)
            || reviewed.non_cleared_under(repository, &dir)
            || settled
                .iter()
                .any(|((r, p), d)| r == repository && p.starts_with(&dir) && non_cleared(*d))
    };
    let mut calls: Vec<(Key, Vec<String>)> = Vec::new();
    for node in nodes {
        let mut files: BTreeSet<String> = BTreeSet::new();
        for target in node.edges.refs.iter().chain(&node.edges.test_refs) {
            if target != &node.path && bad(&key(&node.repository, target)) {
                files.insert(target.clone());
            }
        }
        let foreign: BTreeSet<String> = node
            .edges
            .crates
            .iter()
            .chain(&node.edges.test_crates)
            .filter(|label| crate_bad(label))
            .cloned()
            .collect();
        let mut list: Vec<String> = files.into_iter().chain(foreign).collect();
        if !node.edges.runtime.is_empty() || !node.edges.test_runtime.is_empty() {
            list.push(runtime_label(&node.repository));
        }
        if !list.is_empty() {
            calls.push((key(&node.repository, &node.path), list));
        }
    }
    for (own, list) in calls {
        let tag = tags.entry(own).or_default();
        if tag.disposition.is_none() {
            tag.entries.push(RULE_CALLS.to_owned());
            tag.reason = reviewed.rule_reason(RULE_CALLS).to_owned();
        }
        tag.calls_quarantined = list;
    }
    tags
}

/// Whether the release source allowlist admits a manifest input: read disposition port, and a
/// numeric provenance that is absent or cleared.
pub fn admitted(input: &Value) -> bool {
    input["disposition"] == "port"
        && input["numeric_provenance"]["disposition"]
            .as_str()
            .is_none_or(|d| d == Numeric::Cleared.as_str())
}

/// Whether a port input with the numeric provenance `tag` is admitted.
fn admits(input: &Value, tag: Option<&Tag>) -> bool {
    input["disposition"] == "port" && !non_cleared(tag.and_then(|t| t.disposition))
}

/// What a tag says, for a finding.
fn tag_text(tag: &Tag) -> String {
    let mut out = format!(
        "{} ({})",
        tag.disposition.map_or("none", Numeric::as_str),
        tag.entries.join(", ")
    );
    for (shown, list) in [
        ("citation sweep matches", &tag.citations),
        ("changed by tainted commits", &tag.changed_by),
        ("includes", &tag.includes),
        ("include macros it cannot read", &tag.unread_includes),
        ("calls_quarantined", &tag.calls_quarantined),
    ] {
        if !list.is_empty() {
            out.push_str(&format!("; {shown} {}", list.join(", ")));
        }
    }
    out
}

fn key_of(entry: &Value) -> String {
    format!(
        "{}/{}",
        entry["repository"].as_str().unwrap_or_default(),
        entry["path"].as_str().unwrap_or_default()
    )
}

const ALLOWLIST_RULE: &str = "exactly the inputs of the manifest whose read disposition is port and whose numeric_provenance is absent or cleared, each at its pinned commit and blob and each with its calls_quarantined list when it has one (port it only against clean replacements of every listed module); written by cargo xtask pin-sources and never by hand; the numeric-provenance check of cargo xtask accept recomputes every input's numeric provenance and calls_quarantined list from the dispositions file and the manifest's code_refs and content_facts and rejects any other entry or list";

/// The release source allowlist for `manifest` (written at `manifest_rel`).
pub fn allowlist(manifest: &Value, manifest_rel: &str) -> Value {
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let mut entries = Vec::new();
    let mut by_repository: BTreeMap<String, usize> = BTreeMap::new();
    let mut left_out: BTreeMap<&str, usize> = ["tainted", "unresolved"]
        .into_iter()
        .map(|d| (d, 0))
        .collect();
    let mut ports = 0;
    let mut none_found = 0;
    let mut with_list = 0;
    for input in &inputs {
        if input["disposition"] == "port" {
            ports += 1;
        }
        if !admitted(input) {
            if input["disposition"] == "port"
                && let Some(d) = input["numeric_provenance"]["disposition"].as_str()
                && let Some(count) = left_out.get_mut(d)
            {
                *count += 1;
            }
            continue;
        }
        *by_repository
            .entry(input["repository"].as_str().unwrap_or_default().to_owned())
            .or_default() += 1;
        if input["license_status"] == "none-found" {
            none_found += 1;
        }
        let mut entry = json!({
            "repository": input["repository"],
            "path": input["path"],
            "commit": input["commit"],
            "blob": input["blob"],
            "license_status": input["license_status"],
            "category": input["category"],
        });
        let list = &input["numeric_provenance"]["calls_quarantined"];
        if list.as_array().is_some_and(|l| !l.is_empty()) {
            with_list += 1;
            entry["calls_quarantined"] = list.clone();
        }
        entries.push(entry);
    }
    json!({
        "format": ALLOWLIST_FORMAT,
        "written_by": "cargo xtask pin-sources",
        "manifest": manifest_rel,
        "dispositions": DISPOSITIONS,
        "rule": ALLOWLIST_RULE,
        "summary": {
            "entries": entries.len(),
            "entries_with_calls_quarantined": with_list,
            "by_repository": by_repository,
            "license_none_found": none_found,
            "port_inputs": ports,
            "port_inputs_left_out": left_out,
            "verification": "provisional for every entry, as in the manifest",
        },
        "entries": entries,
    })
}

/// The allowlist as text: fixed key order, one entry per line.
pub fn allowlist_text(allowlist: &Value) -> String {
    const ORDER: [&str; 7] = [
        "format",
        "written_by",
        "manifest",
        "dispositions",
        "rule",
        "summary",
        "entries",
    ];
    let mut out = String::from("{\n");
    for (index, key) in ORDER.iter().enumerate() {
        let value = &allowlist[*key];
        out.push_str(&format!("  {}: ", json!(key)));
        match value {
            Value::Array(items) if !items.is_empty() => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    out.push_str("    ");
                    out.push_str(&item.to_string());
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                out.push_str("  ]");
            }
            _ => {
                let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
                out.push_str(&pretty.replace('\n', "\n  "));
            }
        }
        out.push_str(if index + 1 < ORDER.len() { ",\n" } else { "\n" });
    }
    out.push_str("}\n");
    out
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not JSON: {e}", path.display()))
}

/// The git blob ID of some bytes, as `git hash-object --no-filters` computes it: the SHA-1 of
/// `blob <length>\0` followed by the bytes. Computed here, so the check needs no git and no
/// repository ownership settings on any host.
pub fn blob_id(bytes: &[u8]) -> String {
    let mut data = format!("blob {}\0", bytes.len()).into_bytes();
    data.extend_from_slice(bytes);
    sha1(&data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// SHA-1 (FIPS 180-4), used only to name content as git does.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (state, add) in h.iter_mut().zip([a, b, c, d, e]) {
            *state = state.wrapping_add(add);
        }
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Every file under `dir`, relative to `root` with forward slashes, with its size.
fn files_under(root: &Path, dir: &Path, found: &mut Vec<(String, u64)>) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files_under(root, &path, found)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            found.push((rel.to_string_lossy().replace('\\', "/"), size));
        }
    }
    Ok(())
}

/// What the check reads, relative to a workspace root.
pub struct Files {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub dispositions: PathBuf,
    pub allowlist: PathBuf,
}

impl Files {
    pub fn at(root: &Path) -> Files {
        Files {
            root: root.to_path_buf(),
            manifest: root.join(crate::pin::MANIFEST),
            dispositions: root.join(DISPOSITIONS),
            allowlist: root.join(ALLOWLIST),
        }
    }
}

/// The `numeric-provenance` architecture check over the committed files of `root`.
pub fn check(root: &Path) -> CheckResult {
    let (summary, findings, details) = check_files(&Files::at(root));
    CheckResult::new(CHECK, summary, findings, details)
}

/// The manifest's `numeric_content` record of what the content facts were computed with.
pub fn content_record(reviewed: &Reviewed, crates: &Crates) -> Value {
    json!({
        "citation_sweep": {
            "patterns": reviewed.sweep_patterns,
            "rule": "every line of every pinned input's text (lossy UTF-8) that holds a pattern, case-sensitively, not directly after an ASCII letter, digit or underscore; recorded per input under content_facts.citations as line and pattern, never as the line's text",
        },
        "tainted_commits": reviewed.tainted_commits.iter().map(|t| json!({"repository": t.repository, "commit": t.commit})).collect::<Vec<_>>(),
        "changed_by_rule": "the paths each listed commit changed against its first parent (git diff-tree -r --root -M), followed through later renames up to the pinned commit; recorded per input under content_facts.changed_by",
        "library_crates": crates.iter().map(|(label, (repository, src))| (label.clone(), json!({"repository": repository, "src": src}))).collect::<serde_json::Map<String, Value>>(),
    })
}

/// The check's summary, findings and details.
pub fn check_files(files: &Files) -> (String, Vec<String>, Value) {
    let loaded = (|| -> Result<(Value, Reviewed, Value), String> {
        Ok((
            read_json(&files.manifest)?,
            Reviewed::load(&files.dispositions)?,
            read_json(&files.allowlist)?,
        ))
    })();
    let (manifest, reviewed, listed) = match loaded {
        Ok(found) => found,
        Err(e) => return (format!("not checked: {e}"), vec![e], Value::Null),
    };
    let mut findings = Vec::new();
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let nodes: Vec<Node> = inputs.iter().map(Node::of).collect();
    let crates = crates_of(&manifest);
    let derived = derive(&nodes, &reviewed, &crates);
    let tag_of = |input: &Value| {
        derived.get(&(
            input["repository"].as_str().unwrap_or_default().to_owned(),
            input["path"].as_str().unwrap_or_default().to_owned(),
        ))
    };

    // The manifest's content facts were computed with the dispositions file's patterns and
    // tainted commits.
    let recorded = &manifest["numeric_content"];
    let wanted = content_record(&reviewed, &crates);
    for field in ["citation_sweep", "tainted_commits"] {
        let (found, expected) = match field {
            "citation_sweep" => (&recorded[field]["patterns"], &wanted[field]["patterns"]),
            _ => (&recorded[field], &wanted[field]),
        };
        if found != expected {
            findings.push(format!(
                "{}: numeric_content.{field} is {found} but {DISPOSITIONS} holds {expected}, so the content facts are stale (regenerate with cargo xtask pin-sources)",
                crate::pin::MANIFEST
            ));
        }
    }

    // The allowlist against the recomputed provenance of the manifest's inputs.
    if listed["format"] != ALLOWLIST_FORMAT {
        findings.push(format!(
            "{ALLOWLIST}: format is {}, not {ALLOWLIST_FORMAT}",
            listed["format"]
        ));
    }
    let entries = listed["entries"].as_array().cloned().unwrap_or_default();
    let mut seen = BTreeSet::new();
    for entry in &entries {
        let key = key_of(entry);
        if !seen.insert(key.clone()) {
            findings.push(format!("{ALLOWLIST}: {key} is listed twice"));
            continue;
        }
        let repository = entry["repository"].as_str().unwrap_or_default();
        let path = entry["path"].as_str().unwrap_or_default();
        match by_key.get(&key) {
            None => findings.push(format!(
                "{ALLOWLIST}: {key} is listed but is not an input of the manifest"
            )),
            Some(input) => {
                if input["disposition"] != "port" {
                    findings.push(format!(
                        "{ALLOWLIST}: {key} is listed but its read disposition is {}",
                        input["disposition"].as_str().unwrap_or("?")
                    ));
                }
                let tag = tag_of(input);
                if let Some(tag) = tag.filter(|t| non_cleared(t.disposition)) {
                    findings.push(format!(
                        "{ALLOWLIST}: {key} is listed but its numeric provenance, recomputed from {DISPOSITIONS} and the manifest's code_refs and content_facts, is {}",
                        tag_text(tag)
                    ));
                }
                let wanted_list: Vec<String> =
                    tag.map(|t| t.calls_quarantined.clone()).unwrap_or_default();
                let found_list = list_of(entry, "calls_quarantined");
                if found_list != wanted_list {
                    findings.push(format!(
                        "{ALLOWLIST}: {key} is listed with calls_quarantined {found_list:?} but the recomputation gives {wanted_list:?}; an input that calls a tainted or unresolved module is admitted only with its list"
                    ));
                }
                if input["commit"] != entry["commit"] || input["blob"] != entry["blob"] {
                    findings.push(format!(
                        "{ALLOWLIST}: {key} is listed at commit {} blob {} but pinned at commit {} blob {}",
                        entry["commit"], entry["blob"], input["commit"], input["blob"]
                    ));
                }
            }
        }
        if crate::pin::quarantined(repository, path) {
            findings.push(format!(
                "{ALLOWLIST}: {key} is listed but lies under the quarantined numeric directory"
            ));
        }
    }
    let admitted_keys: Vec<String> = inputs
        .iter()
        .filter(|input| admits(input, tag_of(input)))
        .map(key_of)
        .collect();
    let missing: Vec<&String> = admitted_keys
        .iter()
        .filter(|key| !seen.contains(*key))
        .collect();
    for key in &missing {
        findings.push(format!(
            "{ALLOWLIST}: the recomputed provenance admits {key} but it is not listed (regenerate with cargo xtask pin-sources)"
        ));
    }

    // The manifest's recorded provenance is never less restrictive than the recomputation, and
    // records the same calls_quarantined list (the lookup prints it).
    let mut stale = 0;
    for input in &inputs {
        let recorded = &input["numeric_provenance"];
        let recorded_disposition = recorded["disposition"].as_str().and_then(Numeric::parse);
        let tag = tag_of(input);
        let wanted_disposition = tag.and_then(|t| t.disposition);
        let less = match (recorded_disposition, wanted_disposition) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(r), Some(w)) => r < w,
        };
        let wanted_list: Vec<String> = tag.map(|t| t.calls_quarantined.clone()).unwrap_or_default();
        if less || list_of(recorded, "calls_quarantined") != wanted_list {
            stale += 1;
            findings.push(format!(
                "{}: {} records numeric provenance {} but the recomputation from {DISPOSITIONS} and the manifest's code_refs and content_facts gives {} (regenerate with cargo xtask pin-sources)",
                crate::pin::MANIFEST,
                key_of(input),
                recorded,
                tag.map_or("none".to_owned(), tag_text)
            ));
        }
    }

    // The review record, the owners, and the witness set.
    let review_findings = check_review(files, &reviewed);
    let reviewed_ok = review_findings.is_empty();
    findings.extend(review_findings);
    for unowned in reviewed.unowned() {
        findings.push(format!(
            "{DISPOSITIONS}: {unowned} is class-wide or deferred but names no owner bead"
        ));
    }
    let (witness_findings, witness_bytes) = check_witnesses(files, &reviewed, &by_key, &derived);
    let witness_count = reviewed.witnesses.len();
    let witness_failures = witness_findings.len();
    findings.extend(witness_findings);
    for witness in manifest["retained_witnesses"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let facts = Facts::from_json(&witness["source"]["content_facts"]);
        let changed: Vec<&String> = facts
            .changed_by
            .iter()
            .filter(|c| {
                reviewed.is_tainted_commit(
                    witness["source"]["repository"].as_str().unwrap_or_default(),
                    c,
                )
            })
            .collect();
        if !facts.citations.is_empty() || !changed.is_empty() {
            findings.push(format!(
                "{}: the retained witness source {} has a citation sweep match or a change by a tainted commit ({})",
                crate::pin::MANIFEST,
                witness["source"]["path"],
                witness["source"]["content_facts"]
            ));
        }
    }

    // Counts: per entry and rule with its owner, the calls_quarantined edges, and the edges into
    // files that are not pinned inputs.
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut admitted_with_list = 0;
    let mut crate_edges = 0;
    let mut crate_inputs = 0;
    let mut runtime_inputs = 0;
    let mut test_files = 0;
    let keys: BTreeSet<Key> = nodes
        .iter()
        .map(|n| (n.repository.clone(), n.path.clone()))
        .collect();
    for ((_, path), tag) in &derived {
        for entry in &tag.entries {
            *counts.entry(entry.clone()).or_default() += 1;
        }
        if path.contains("/tests/") && non_cleared(tag.disposition) {
            test_files += 1;
        }
    }
    for node in &nodes {
        let foreign = node.edges.crates.len() + node.edges.test_crates.len();
        crate_edges += foreign;
        if foreign > 0 {
            crate_inputs += 1;
        }
        if !node.edges.runtime.is_empty() || !node.edges.test_runtime.is_empty() {
            runtime_inputs += 1;
        }
    }
    for input in &inputs {
        if admits(input, tag_of(input))
            && tag_of(input).is_some_and(|t| !t.calls_quarantined.is_empty())
        {
            admitted_with_list += 1;
        }
    }
    let mut unpinned_oxfunc = 0;
    let mut unpinned_elsewhere = 0;
    for node in &nodes {
        let edges = &node.edges;
        for target in edges
            .refs
            .iter()
            .chain(&edges.includes)
            .chain(&edges.test_refs)
            .chain(&edges.test_includes)
        {
            if !keys.contains(&(node.repository.clone(), target.clone())) {
                if node.repository == "OxFunc" {
                    unpinned_oxfunc += 1;
                } else {
                    unpinned_elsewhere += 1;
                }
            }
        }
    }
    let owned: Vec<Value> = reviewed
        .owned_entries()
        .into_iter()
        .map(|(id, what, owner)| {
            json!({
                "entry": id,
                "disposition": what,
                "inputs": counts.get(&id).copied().unwrap_or(0),
                "owner": owner.bead,
                "replacement": owner.replacement,
            })
        })
        .collect();
    let owned_text: Vec<String> = owned
        .iter()
        .map(|o| {
            format!(
                "{} ({}) on {} inputs, owner {}",
                o["entry"].as_str().unwrap_or_default(),
                o["disposition"].as_str().unwrap_or_default(),
                o["inputs"],
                o["owner"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let gaps: Vec<Value> = reviewed
        .gaps
        .iter()
        .map(|g| json!({"what": g.what, "owner": g.owner}))
        .collect();
    let gap_owners: Vec<&str> = reviewed.gaps.iter().map(|g| g.owner.as_str()).collect();
    let calls_owner = reviewed
        .rule(RULE_CALLS)
        .owner
        .as_ref()
        .map_or("none", |o| o.bead.as_str());

    let summary = format!(
        "release source allowlist {} entries ({} admitted by the provenance recomputed from {DISPOSITIONS} and the manifest's code_refs and content_facts, {admitted_with_list} of them only with a calls_quarantined list, {} missing); {} manifest inputs with stale numeric provenance; review record {}; {witness_count} retained witness selections ({witness_bytes} bytes), {witness_failures} witness findings; entries and rules with their owners: {}; edges counted for the calls_quarantined owner {calls_owner}: {crate_edges} cross-repository crate edges in {crate_inputs} inputs, {runtime_inputs} inputs with run-time loads, and {} edges into files that are not pinned inputs ({unpinned_oxfunc} in OxFunc, {unpinned_elsewhere} elsewhere); {test_files} files under a tests directory are tainted or unresolved; gaps owned by {}",
        entries.len(),
        admitted_keys.len(),
        missing.len(),
        stale,
        if reviewed_ok {
            "present, matching its review file and covering every cleared entry"
        } else {
            "absent, not matching its review file, or not covering every cleared entry"
        },
        owned_text.join("; "),
        unpinned_oxfunc + unpinned_elsewhere,
        gap_owners.join(", ")
    );
    let details = json!({
        "manifest": crate::pin::MANIFEST,
        "dispositions": DISPOSITIONS,
        "allowlist": ALLOWLIST,
        "allowlist_entries": entries.len(),
        "admitted_by_recomputation": admitted_keys.len(),
        "admitted_with_calls_quarantined": admitted_with_list,
        "inputs_by_entry": counts,
        "test_files_tainted_or_unresolved": test_files,
        "cross_repository_crate_edges": crate_edges,
        "inputs_with_cross_repository_crate_edges": crate_inputs,
        "inputs_with_run_time_loads": runtime_inputs,
        "edges_into_files_not_pinned": {"OxFunc": unpinned_oxfunc, "elsewhere": unpinned_elsewhere},
        "edges_owner": calls_owner,
        "owned_entries": owned,
        "gaps": gaps,
        "trusted_facts": "the manifest's code_refs and content_facts (citation sweep matches and touched-by-commit lists) are trusted as recorded, and the calls_quarantined lists and the include rule rest on code_refs; the infra-m0.5 acceptance case recomputes the citation sweep and the touched-by-commit lists from git",
        "witnesses": WITNESSES,
        "retained_witnesses": witness_count,
        "witness_bytes": witness_bytes,
    });
    (summary, findings, details)
}

/// The first word of a review's proposed disposition, when it is a disposition.
fn proposed(text: &str) -> Option<Numeric> {
    let word: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    Numeric::parse(&word.to_ascii_lowercase())
}

/// The review record of the dispositions file against the review file it names: the counts of
/// its verdicts, every entry whose proposed disposition starts with a disposition, the confirmed
/// witness sources, the owner's confirmation (pending, or a confirmer and a date), and the hash of
/// the dispositions as reviewed, which must cover every cleared entry.
fn check_review(files: &Files, reviewed: &Reviewed) -> Vec<String> {
    let mut findings = Vec::new();
    let record = &reviewed.review;
    if !record.is_object() {
        findings.push(format!(
            "{DISPOSITIONS}: status.review is absent, so its dispositions are not reviewed"
        ));
        return findings;
    }
    for key in ["reviewer", "date"] {
        if record[key].as_str().is_none_or(|t| t.trim().is_empty()) {
            findings.push(format!("{DISPOSITIONS}: status.review has no {key}"));
        }
    }
    let confirmation = &record["owner_confirmation_of_cleared_witness_sources"];
    let confirmed_ok = confirmation == "pending"
        || (confirmation["confirmer"]
            .as_str()
            .is_some_and(|t| !t.trim().is_empty())
            && confirmation["date"]
                .as_str()
                .is_some_and(|t| !t.trim().is_empty()));
    if !confirmed_ok {
        findings.push(format!(
            "{DISPOSITIONS}: status.review.owner_confirmation_of_cleared_witness_sources is {confirmation}, neither \"pending\" nor a confirmation with a confirmer and a date"
        ));
    }
    // The hash of the dispositions as reviewed.
    let hash = &record["dispositions_hash"];
    match &reviewed.reviewed_hash {
        Err(e) => findings.push(format!("{DISPOSITIONS}: {e}")),
        Ok(found) => {
            if hash["algorithm"] != REVIEW_HASH_ALGORITHM || hash["value"] != found.as_str() {
                findings.push(format!(
                    "{DISPOSITIONS}: the entries as reviewed hash to {found} but status.review.dispositions_hash records {hash}: an entry was added or changed without a review (a tainted or unresolved one must carry an after_review mark; a cleared one needs a new review)"
                ));
            }
        }
    }
    for module in &reviewed.modules {
        if module.after_review && module.disposition == Numeric::Cleared {
            findings.push(format!(
                "{DISPOSITIONS}: module {} is cleared but marked as added or changed after the review; a clearance needs a new review",
                module.id
            ));
        }
    }
    for class in &reviewed.classes {
        if class.after_review && class.disposition == Numeric::Cleared {
            findings.push(format!(
                "{DISPOSITIONS}: class {} is cleared but marked as added or changed after the review; a clearance needs a new review",
                class.id
            ));
        }
    }
    let outcome = &record["outcome"];
    let Some(review_rel) = outcome["file"].as_str() else {
        findings.push(format!(
            "{DISPOSITIONS}: status.review.outcome names no review file"
        ));
        return findings;
    };
    let review = match read_json(&files.root.join(review_rel)) {
        Ok(review) => review,
        Err(e) => {
            findings.push(format!("{DISPOSITIONS}: status.review.outcome: {e}"));
            return findings;
        }
    };
    let entries = review["entries"].as_array().cloned().unwrap_or_default();
    let mut tally: BTreeMap<&str, u64> = VERDICTS.iter().map(|v| (*v, 0)).collect();
    for entry in &entries {
        match entry["verdict"].as_str().and_then(|v| tally.get_mut(v)) {
            Some(count) => *count += 1,
            None => findings.push(format!(
                "{review_rel}: entry {} has verdict {}, not one of {VERDICTS:?}",
                entry["id"], entry["verdict"]
            )),
        }
    }
    if outcome["entries"] != json!(entries.len()) {
        findings.push(format!(
            "{DISPOSITIONS}: status.review.outcome records {} entries but {review_rel} holds {}",
            outcome["entries"],
            entries.len()
        ));
    }
    for (verdict, count) in &tally {
        if outcome[*verdict] != json!(count) {
            findings.push(format!(
                "{DISPOSITIONS}: status.review.outcome records {} {verdict} but {review_rel} holds {count}",
                outcome[*verdict]
            ));
        }
    }
    for entry in &entries {
        let id = entry["id"].as_str().unwrap_or_default();
        let (Some(wanted), Some(found)) = (
            entry["proposed_disposition"].as_str().and_then(proposed),
            reviewed.dispositions_by_id.get(id),
        ) else {
            continue;
        };
        if wanted != *found {
            findings.push(format!(
                "{DISPOSITIONS}: entry {id} is {} but {review_rel} proposes {}",
                found.as_str(),
                wanted.as_str()
            ));
        }
    }
    let confirmed: BTreeSet<String> = record["witness_sources_confirmed"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let by_review: BTreeSet<String> = review["witness_sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| w["black_box_acquisition_confirmed"] == true)
        .filter_map(|w| w["id"].as_str().map(str::to_owned))
        .collect();
    let used: BTreeSet<String> = reviewed
        .witnesses
        .iter()
        .filter_map(|w| {
            reviewed
                .tag(&w.source.repository, &w.source.path, Some(&w.source.blob))
                .and_then(|t| t.entries.first().cloned())
        })
        .collect();
    if confirmed != used || !confirmed.is_subset(&by_review) {
        findings.push(format!(
            "{DISPOSITIONS}: status.review confirms the witness sources {confirmed:?}; the retained witnesses use {used:?} and {review_rel} confirms black-box acquisition for {by_review:?}"
        ));
    }
    findings
}

/// The fields of a capture's environment that a witness's acquisition must repeat.
const ENVIRONMENT_FIELDS: [&str; 6] = [
    "excel_version",
    "excel_build",
    "excel_bitness",
    "workbook_compatibility",
    "excel_operating_system",
    "excel_input_plumbing",
];

fn check_witnesses(
    files: &Files,
    reviewed: &Reviewed,
    by_key: &BTreeMap<String, &Value>,
    derived: &BTreeMap<Key, Tag>,
) -> (Vec<String>, u64) {
    let mut findings = Vec::new();
    let dir = files.root.join(WITNESSES);
    let mut present = Vec::new();
    if dir.is_dir()
        && let Err(e) = files_under(&files.root, &dir, &mut present)
    {
        findings.push(e);
    }
    let total: u64 = present.iter().map(|(_, size)| size).sum();
    match reviewed.witness_limit {
        Some(limit) if total > limit => findings.push(format!(
            "{WITNESSES}/ holds {total} bytes, more than the {limit} bytes {DISPOSITIONS} allows for retained selections"
        )),
        _ => {}
    }
    let retained: BTreeSet<&str> = reviewed.witnesses.iter().map(|w| w.file.as_str()).collect();
    for (file, _) in &present {
        if file == &format!("{WITNESSES}/{WITNESS_ATTRIBUTES}") {
            continue;
        }
        if !retained.contains(file.as_str()) {
            findings.push(format!(
                "{file} is under {WITNESSES}/ but {DISPOSITIONS} records no acquisition metadata for it as a retained witness"
            ));
        }
    }
    for witness in &reviewed.witnesses {
        let file = &witness.file;
        let mut fail = |what: String| findings.push(format!("witness {file}: {what}"));
        let path = files.root.join(file);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                fail(format!("cannot read it: {e}"));
                continue;
            }
        };
        let found = blob_id(&bytes);
        if found != witness.blob {
            fail(format!(
                "its bytes hash to blob {found}, not the recorded {}",
                witness.blob
            ));
        }
        let source = &witness.source;
        match reviewed.tag(&source.repository, &source.path, Some(&source.blob)) {
            Some(tag) if tag.disposition == Some(Numeric::Cleared) => {}
            other => fail(format!(
                "its source {}/{} is not cleared by a module entry at blob {} ({})",
                source.repository,
                source.path,
                source.blob,
                other.map_or("no entry".to_owned(), |t| tag_text(&t))
            )),
        }
        if reviewed.excluded_contains(source) {
            fail(format!(
                "its source {}/{} is recorded as an excluded witness",
                source.repository, source.path
            ));
        }
        let acquisition = &witness.acquisition;
        if acquisition["method"] != WITNESS_METHOD {
            fail(format!(
                "acquisition method is {}, not {WITNESS_METHOD}",
                acquisition["method"]
            ));
        }
        let content: Value = match serde_json::from_slice(&bytes) {
            Ok(content) => content,
            Err(e) => {
                fail(format!("it is not JSON: {e}"));
                continue;
            }
        };
        // The selection's own record against the metadata.
        if content["format"] != SELECTION_FORMAT {
            fail(format!(
                "format is {}, not {SELECTION_FORMAT}",
                content["format"]
            ));
        }
        if content["function"] != witness.function.as_str() {
            fail(format!(
                "it holds function {}, not {}",
                content["function"], witness.function
            ));
        }
        let wanted_source = json!({
            "repository": source.repository,
            "commit": witness.source_commit,
            "path": source.path,
            "blob": source.blob,
            "sha256": witness.source_sha256,
            "rows": witness.source_rows,
        });
        if content["source"] != wanted_source {
            fail(format!(
                "its source record {} differs from the metadata {wanted_source}",
                content["source"]
            ));
        }
        let rule = &witness.selection;
        let recorded_rule = &content["selection"];
        if recorded_rule["edge_rows"] != json!(rule.edge_rows)
            || recorded_rule["random_rows"] != json!(rule.random_rows)
            || recorded_rule["stride"] != json!(rule.stride)
            || recorded_rule["rule"].as_str().is_none_or(str::is_empty)
        {
            fail(format!(
                "its selection record {recorded_rule} differs from the metadata's selection rule"
            ));
        }
        if rule.edge_rows + rule.random_rows != witness.source_rows || rule.stride == 0 {
            fail(format!(
                "the selection rule ({} edge rows, {} random rows, stride {}) does not cover the source's {} rows",
                rule.edge_rows, rule.random_rows, rule.stride, witness.source_rows
            ));
        }
        let capture = &content["capture_provenance"];
        for field in ENVIRONMENT_FIELDS {
            let embedded = &capture["environment"][field];
            if !embedded.is_string() || embedded != &acquisition["excel"][field] {
                fail(format!(
                    "capture environment {field} is {embedded}, acquisition records {}",
                    acquisition["excel"][field]
                ));
            }
        }
        if capture["captured_utc"] != acquisition["captured_utc"]
            || !capture["captured_utc"].is_string()
        {
            fail(format!(
                "capture time {} differs from the recorded {}",
                capture["captured_utc"], acquisition["captured_utc"]
            ));
        }
        if capture["oracle_cache"]["mode"] != WITNESS_CACHE
            || acquisition["oracle_cache"] != WITNESS_CACHE
        {
            fail(format!(
                "capture cache mode {} (recorded {}) is not {WITNESS_CACHE}: an answer may not have been computed live",
                capture["oracle_cache"]["mode"], acquisition["oracle_cache"]
            ));
        }
        for field in ["name", "version"] {
            if capture["runner"][field] != acquisition["runner"][field]
                || !capture["runner"][field].is_string()
            {
                fail(format!(
                    "capture runner {field} {} differs from the recorded {}",
                    capture["runner"][field], acquisition["runner"][field]
                ));
            }
        }
        let rows = content["witnesses"].as_array().cloned().unwrap_or_default();
        if rows.len() as u64 != witness.rows {
            fail(format!(
                "it holds {} rows, not the recorded {}",
                rows.len(),
                witness.rows
            ));
        }
        let prefix = acquisition["input_id_prefix"].as_str().unwrap_or_default();
        let wanted_ids: Vec<String> = rule
            .indexes()
            .iter()
            .map(|i| format!("{prefix}{i:05}"))
            .collect();
        let ids: Vec<String> = rows
            .iter()
            .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
            .collect();
        if prefix.is_empty() || ids != wanted_ids {
            fail(format!(
                "its row ids are not the {} ids the selection rule picks with the recorded input prefix {prefix:?}",
                wanted_ids.len()
            ));
        }
        let malformed = rows
            .iter()
            .filter(|row| !row["args"].is_array() || !row["expected_bits"].is_string())
            .count();
        if malformed > 0 {
            fail(format!("{malformed} rows lack args or expected_bits"));
        }
        // Generators: pinned, never quarantined, tainted or unresolved; one that builds or runs a
        // package of a tainted class needs the equality with an independent retention.
        let runner_name = acquisition["runner"]["name"].as_str().unwrap_or_default();
        let mut runner = 0;
        let mut tainted_packages: Vec<String> = Vec::new();
        for generator in &witness.generators {
            let g = &generator.source;
            let key = format!("{}/{}", g.repository, g.path);
            if generator.role == "expected_bits"
                && !runner_name.is_empty()
                && g.path.ends_with(&format!("/{runner_name}"))
            {
                runner += 1;
            }
            let built = generator
                .builds_or_runs
                .iter()
                .map(|p| (p, "builds or runs"));
            let named = generator.names_packages.iter().map(|p| (p, "names"));
            for (package, how) in built.chain(named) {
                let manifest = format!("{package}Cargo.toml");
                let class = reviewed
                    .tag(&g.repository, &manifest, None)
                    .and_then(|t| t.disposition);
                if non_cleared(class) {
                    tainted_packages.push(format!("{key} {how} {}/{package}", g.repository));
                }
            }
            match by_key.get(&key) {
                None => fail(format!(
                    "generator {key} is not a pinned input of the manifest"
                )),
                Some(input) => {
                    if input["blob"] != g.blob.as_str() && generator.commit.is_none() {
                        fail(format!(
                            "generator {key} is recorded at blob {} but pinned at {}, and it names no commit whose tree holds that blob",
                            g.blob, input["blob"]
                        ));
                    }
                    let read = input["disposition"].as_str().unwrap_or_default();
                    if matches!(read, "quarantined" | "quarantine_derived") {
                        fail(format!("generator {key} is {read} in the manifest"));
                    }
                    let tag = derived.get(&(g.repository.clone(), g.path.clone()));
                    if let Some(tag) = tag.filter(|t| non_cleared(t.disposition)) {
                        fail(format!(
                            "generator {key} has numeric provenance {}",
                            tag_text(tag)
                        ));
                    }
                }
            }
            if crate::pin::quarantined(&g.repository, &g.path) {
                fail(format!(
                    "generator {key} lies under the quarantined numeric directory"
                ));
            }
            if let Some(tag) = reviewed.tag(&g.repository, &g.path, Some(&g.blob))
                && non_cleared(tag.disposition)
            {
                fail(format!(
                    "generator {key} is {} in {DISPOSITIONS} ({})",
                    tag.disposition.map_or("none", Numeric::as_str),
                    tag.entries.join(", ")
                ));
            }
        }
        if runner == 0 {
            fail(format!(
                "no generator with the role expected_bits is the runner {runner_name:?} the capture names"
            ));
        }
        if !tainted_packages.is_empty() {
            match &witness.retention {
                None => fail(format!(
                    "{} (a package of a tainted class), and the witness records no equality of its bits with an independent retention (independent_retention)",
                    tainted_packages.join("; ")
                )),
                Some(retention) => {
                    if retention.compared_rows != witness.rows
                        || retention.matched_rows != retention.compared_rows
                        || retention.source.path == source.path
                        || retention.source.repository != source.repository
                    {
                        fail(format!(
                            "{}; its independent retention {}/{} records {} of {} rows matched for a selection of {} rows",
                            tainted_packages.join("; "),
                            retention.source.repository,
                            retention.source.path,
                            retention.matched_rows,
                            retention.compared_rows,
                            witness.rows
                        ));
                    }
                }
            }
        }
    }
    (findings, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &str, path: &str, blob: &str, disposition: &str) -> Value {
        json!({
            "id": id, "repository": "R", "path": path, "blob": blob,
            "disposition": disposition, "reason": "fixture",
            "evidence": [{"kind": "header", "line": 1, "quote": "q"}],
        })
    }

    fn file(modules: Vec<Value>, classes: Vec<Value>) -> String {
        fixture_text(json!(modules), json!(classes), json!([]))
    }

    fn class(id: &str, selector: Value, disposition: &str) -> Value {
        json!({
            "id": id, "selector": selector, "disposition": disposition, "reason": "fixture",
            "evidence": [{"kind": "commit", "commit": "c", "quote": "q"}],
        })
    }

    /// Blob IDs as git computes them, each a fixed digest that `git hash-object --no-filters`
    /// gives for the same bytes: the empty blob, a short text, and a text whose `blob <length>\0`
    /// header and bytes span two SHA-1 blocks.
    #[test]
    fn blob_ids_match_git() {
        let cases: [(&[u8], &str); 3] = [
            (b"", "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"),
            (b"hello\n", "ce013625030ba8dba906f756967f9e9ca394464a"),
            (
                b"The quick brown fox jumps over the lazy dog, then over the lazy dog again.\n",
                "a6e6140896dbc0151b20233484b357e9862fa74d",
            ),
        ];
        assert!(cases[2].0.len() + "blob 75\0".len() > 64);
        for (bytes, wanted) in cases {
            assert_eq!(
                blob_id(bytes),
                wanted,
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn modules_apply_before_classes_and_a_new_blob_is_not_cleared() {
        let reviewed = Reviewed::parse(&file(
            vec![
                module("a", "src/a.rs", "b1", "cleared"),
                module("t", "src/t.rs", "b2", "tainted"),
            ],
            vec![
                class(
                    "narrow",
                    json!({"repository": "R", "prefixes": ["src/"], "suffixes": ["-x.json"]}),
                    "tainted",
                ),
                class(
                    "broad",
                    json!({"repository": "R", "prefixes": ["src/"]}),
                    "unresolved",
                ),
            ],
        ))
        .unwrap();
        let d = |path: &str, blob: &str| {
            reviewed
                .tag("R", path, Some(blob))
                .map(|t| (t.disposition.unwrap(), t.entries[0].clone()))
        };
        assert_eq!(
            d("src/a.rs", "b1"),
            Some((Numeric::Cleared, "a".to_owned()))
        );
        assert_eq!(
            d("src/a.rs", "other"),
            Some((Numeric::Unresolved, "a".to_owned()))
        );
        assert_eq!(
            d("src/t.rs", "other"),
            Some((Numeric::Tainted, "t".to_owned()))
        );
        assert_eq!(
            d("src/q-x.json", "b"),
            Some((Numeric::Tainted, "narrow".to_owned()))
        );
        assert_eq!(
            d("src/q.json", "b"),
            Some((Numeric::Unresolved, "broad".to_owned()))
        );
        assert_eq!(d("lib/q.json", "b"), None);
        assert_eq!(reviewed.tag("S", "src/a.rs", Some("b1")), None);
    }

    fn refused(text: &str) -> String {
        match Reviewed::parse(text) {
            Ok(_) => panic!("accepted: {text}"),
            Err(e) => e,
        }
    }

    #[test]
    fn malformed_dispositions_are_refused() {
        let bad_disposition = file(vec![module("a", "p", "b", "fine")], vec![]);
        assert!(refused(&bad_disposition).contains("not one of"));
        let duplicate = file(
            vec![
                module("a", "p", "b", "cleared"),
                module("a", "q", "b", "cleared"),
            ],
            vec![],
        );
        assert!(refused(&duplicate).contains("twice"));
        let mut no_evidence = module("a", "p", "b", "cleared");
        no_evidence["evidence"] = json!([]);
        assert!(refused(&file(vec![no_evidence], vec![])).contains("no evidence"));
        let mut no_value = module("a", "p", "b", "cleared");
        no_value["evidence"] =
            json!([{"kind": "data", "path": "d.json", "blob": "b", "pointer": "/x"}]);
        assert!(refused(&file(vec![no_value], vec![])).contains("no value"));
        let empty_selector = class("c", json!({"repository": "R"}), "tainted");
        assert!(refused(&file(vec![], vec![empty_selector])).contains("selects nothing"));
        let mut no_bead = class(
            "c",
            json!({"repository": "R", "prefixes": ["x/"]}),
            "unresolved",
        );
        no_bead["owner"] = json!({"replacement": "per-file review"});
        assert!(refused(&file(vec![], vec![no_bead])).contains("owner has no bead"));
        let mut changed = module("a", "p", "b", "tainted");
        changed["after_review"] = json!({"kind": "changed", "date": "d", "reason": "r"});
        assert!(refused(&file(vec![changed], vec![])).contains("no reviewed_as"));
        let mut no_rule: Value = serde_json::from_str(&file(vec![], vec![])).unwrap();
        no_rule["rules"].as_array_mut().unwrap().pop();
        assert!(refused(&no_rule.to_string()).contains("has no rule"));
        let short = fixture_text(
            json!([]),
            json!([]),
            json!([{"repository": "R", "commit": "abc", "reason": "r"}]),
        );
        assert!(refused(&short).contains("40-digit"));
    }

    fn node(path: &str, edges: Value) -> Node {
        Node {
            repository: "R".to_owned(),
            path: path.to_owned(),
            blob: Some(format!("blob-{path}")),
            quarantine: false,
            edges: Edges::from_json(&edges),
            facts: Facts::default(),
        }
    }

    const TAINTED_COMMIT: &str = "1111111111111111111111111111111111111111";

    /// The content rule: a citation, a change by a listed tainted commit (an unlisted one does
    /// nothing) and an unread include each make a file unresolved; an include takes the included
    /// file's disposition, through an include of an include; a file that only refers to a tainted
    /// or unresolved file keeps no disposition and lists it, a caller of that caller lists
    /// nothing, a reference to a crate of another repository that holds a non-cleared file is
    /// listed at crate level (a clean crate is not), and run-time loads are listed.
    #[test]
    fn the_content_rule_reads_own_content_and_callers_only_list_what_they_call() {
        let reviewed = Reviewed::parse(&fixture_text(
            json!([module("t", "t.rs", "blob-t.rs", "tainted")]),
            json!([]),
            json!([{"repository": "R", "commit": TAINTED_COMMIT, "reason": "fixture"}]),
        ))
        .unwrap();
        let mut cites = node("cites.rs", json!({}));
        cites.facts.citations = vec![Citation {
            line: 3,
            pattern: "C-runtime".to_owned(),
        }];
        let mut changed = node("changed.json", json!({}));
        changed.facts.changed_by = vec![TAINTED_COMMIT.to_owned()];
        let mut unlisted = node("unlisted.json", json!({}));
        unlisted.facts.changed_by = vec!["2222222222222222222222222222222222222222".to_owned()];
        let nodes = vec![
            node("t.rs", json!({})),
            cites,
            changed,
            unlisted,
            node("caller.rs", json!({"refs": ["t.rs", "plain.rs"]})),
            node("caller-of-caller.rs", json!({"refs": ["caller.rs"]})),
            node("embeds.rs", json!({"includes": ["t.rs"]})),
            node(
                "embeds-embedder.rs",
                json!({"test_includes": ["embeds.rs"]}),
            ),
            node("embeds-changed.rs", json!({"includes": ["changed.json"]})),
            node(
                "unread.rs",
                json!({"unread_includes": ["include_str!(concat!(..))"]}),
            ),
            node(
                "foreign.rs",
                json!({"crates": ["Q crate dirty", "Q crate clean"]}),
            ),
            node("loads.rs", json!({"test_runtime": ["fs::read_to_string"]})),
            node("plain.rs", json!({})),
        ];
        let crates: Crates = [
            (
                "Q crate dirty".to_owned(),
                ("Q".to_owned(), "dirty/src".to_owned()),
            ),
            (
                "Q crate clean".to_owned(),
                ("Q".to_owned(), "clean/src".to_owned()),
            ),
        ]
        .into_iter()
        .collect();
        let mut with_foreign = nodes;
        let mut dirty = node("dirty/src/x.rs", json!({}));
        dirty.repository = "Q".to_owned();
        dirty.facts.citations = vec![Citation {
            line: 1,
            pattern: "87tran".to_owned(),
        }];
        with_foreign.push(dirty);
        let mut clean = node("clean/src/y.rs", json!({}));
        clean.repository = "Q".to_owned();
        with_foreign.push(clean);
        let tags = derive(&with_foreign, &reviewed, &crates);
        let get = |path: &str| tags.get(&("R".to_owned(), path.to_owned()));
        let disposition = |path: &str| get(path).and_then(|t| t.disposition);
        assert_eq!(disposition("t.rs"), Some(Numeric::Tainted));
        assert_eq!(disposition("cites.rs"), Some(Numeric::Unresolved));
        assert_eq!(get("cites.rs").unwrap().entries, vec![RULE_CITATION]);
        assert_eq!(
            get("cites.rs").unwrap().citations,
            vec!["line 3 (C-runtime)"]
        );
        assert_eq!(disposition("changed.json"), Some(Numeric::Unresolved));
        assert_eq!(
            get("changed.json").unwrap().changed_by,
            vec![TAINTED_COMMIT]
        );
        assert_eq!(get("unlisted.json"), None);
        let caller = get("caller.rs").unwrap();
        assert_eq!(caller.disposition, None);
        assert_eq!(caller.entries, vec![RULE_CALLS]);
        assert_eq!(caller.calls_quarantined, vec!["t.rs"]);
        assert_eq!(get("caller-of-caller.rs"), None);
        assert_eq!(disposition("embeds.rs"), Some(Numeric::Tainted));
        assert_eq!(get("embeds.rs").unwrap().includes, vec!["t.rs"]);
        assert_eq!(disposition("embeds-embedder.rs"), Some(Numeric::Tainted));
        assert_eq!(disposition("embeds-changed.rs"), Some(Numeric::Unresolved));
        assert_eq!(disposition("unread.rs"), Some(Numeric::Unresolved));
        let foreign = get("foreign.rs").unwrap();
        assert_eq!(foreign.disposition, None);
        assert_eq!(foreign.calls_quarantined, vec!["Q crate dirty"]);
        assert_eq!(
            get("loads.rs").unwrap().calls_quarantined,
            vec!["R run-time loads"]
        );
        assert_eq!(get("plain.rs"), None);
        let json = caller.json();
        assert!(json.get("disposition").is_none());
        assert_eq!(json["calls_quarantined"], json!(["t.rs"]));
    }

    #[test]
    fn the_sweep_matches_patterns_not_inside_a_word() {
        let patterns = vec![
            "87tran".to_owned(),
            "C-runtime".to_owned(),
            "NT source".to_owned(),
        ];
        let text =
            "a\nfollows fpw32/tran/i386/87tran.asm\nthe C-runtime `pow`\nACCOUNT source\nX87tran\n";
        let found = sweep(text, &patterns);
        assert_eq!(
            found,
            vec![
                Citation {
                    line: 2,
                    pattern: "87tran".to_owned()
                },
                Citation {
                    line: 3,
                    pattern: "C-runtime".to_owned()
                },
            ]
        );
    }

    /// The hash of the dispositions as reviewed: unchanged by a reason edit or by an entry marked
    /// as added; changed by an unmarked new entry and by a change of a reviewed disposition; a
    /// change marked with the reviewed record hashes as before.
    #[test]
    fn the_reviewed_hash_covers_unmarked_entries() {
        let base: Value =
            serde_json::from_str(&file(vec![module("a", "a.rs", "b1", "cleared")], vec![]))
                .unwrap();
        let hash = reviewed_hash(&base).unwrap();
        let mut reason = base.clone();
        reason["modules"][0]["reason"] = json!("another reason");
        assert_eq!(reviewed_hash(&reason).unwrap(), hash);
        let mut added = base.clone();
        let mut new = module("n", "n.rs", "b2", "tainted");
        new["after_review"] = json!({"kind": "added", "date": "d", "reason": "r"});
        added["modules"].as_array_mut().unwrap().push(new);
        assert_eq!(reviewed_hash(&added).unwrap(), hash);
        let mut unmarked = base.clone();
        unmarked["modules"]
            .as_array_mut()
            .unwrap()
            .push(module("n", "n.rs", "b2", "cleared"));
        assert_ne!(reviewed_hash(&unmarked).unwrap(), hash);
        let mut changed = base.clone();
        changed["modules"][0]["disposition"] = json!("tainted");
        assert_ne!(reviewed_hash(&changed).unwrap(), hash);
        changed["modules"][0]["after_review"] = json!({
            "kind": "changed", "date": "d", "reason": "r",
            "reviewed_as": {"repository": "R", "path": "a.rs", "blob": "b1", "disposition": "cleared"},
        });
        assert_eq!(reviewed_hash(&changed).unwrap(), hash);
    }

    #[test]
    fn the_selection_rule_picks_edges_then_every_stride_th_random_row() {
        let rule = Selection {
            edge_rows: 3,
            random_rows: 10,
            stride: 4,
        };
        assert_eq!(rule.indexes(), vec![0, 1, 2, 3, 7, 11]);
    }

    #[test]
    fn the_allowlist_admits_only_port_inputs_without_a_non_cleared_provenance() {
        let manifest = json!({"inputs": [
            {"repository": "R", "path": "a", "commit": "c", "blob": "1", "disposition": "port", "license_status": "found", "category": "other"},
            {"repository": "R", "path": "b", "commit": "c", "blob": "2", "disposition": "port", "license_status": "found", "category": "other",
             "numeric_provenance": {"disposition": "cleared", "entries": ["x"]}},
            {"repository": "R", "path": "c", "commit": "c", "blob": "3", "disposition": "port", "license_status": "found", "category": "other",
             "numeric_provenance": {"disposition": "tainted", "entries": ["x"]}},
            {"repository": "R", "path": "d", "commit": "c", "blob": "4", "disposition": "port", "license_status": "found", "category": "other",
             "numeric_provenance": {"disposition": "unresolved", "entries": ["x"]}},
            {"repository": "R", "path": "e", "commit": "c", "blob": "5", "disposition": "reference", "license_status": "found", "category": "other"},
            {"repository": "R", "path": "f", "commit": "c", "blob": "6", "disposition": "quarantined", "license_status": "found", "category": "other",
             "numeric_provenance": {"disposition": "cleared", "entries": ["x"]}},
            {"repository": "R", "path": "g", "commit": "c", "blob": "7", "disposition": "port", "license_status": "found", "category": "other",
             "numeric_provenance": {"entries": [RULE_CALLS], "calls_quarantined": ["c"]}},
        ]});
        let list = allowlist(&manifest, "m.json");
        let keys: Vec<String> = list["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(key_of)
            .collect();
        assert_eq!(keys, vec!["R/a", "R/b", "R/g"]);
        assert_eq!(list["entries"][2]["calls_quarantined"], json!(["c"]));
        assert!(list["entries"][0].get("calls_quarantined").is_none());
        assert_eq!(list["summary"]["entries_with_calls_quarantined"], 1);
        assert_eq!(
            list["summary"]["port_inputs_left_out"],
            json!({"tainted": 1, "unresolved": 1})
        );
        let text = allowlist_text(&list);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), list);
    }
}
