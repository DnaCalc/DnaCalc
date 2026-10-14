//! `cargo xtask pin-sources [--tracker <issues.jsonl>] [--out <file>]` writes the migration record
//! `migration/source-manifest.json`: every legacy repository under the legacy root pinned at its
//! HEAD commit, and every port input the beads' Port from sections cite, pinned at a committed
//! blob of that commit.
//!
//! `cargo xtask pin-sources --lookup <Repo>/<path> [--manifest <file>]` prints the pinned entry of
//! one input (or of every input under a directory). It exits 0 for a port or reference input, 1
//! for an unavailable one, 3 for a quarantined or quarantine_derived one (do not copy), 4 for a
//! bulk-evidence path that is never pinned, and 2 for a usage or manifest error. Port beads use it
//! before copying anything.
//!
//! The legacy root is the workspace root's parent directory, or `DNACALC_LEGACY_ROOT` when set.
//! The legacy repositories are only read (`pin_git.rs`). The tracker's Port from sections are
//! authoritative for which beads cite which paths (`pin_cite.rs`); a cited directory expands to its
//! committed files. Three resolutions go further because the citing text or the code asks for
//! them (`pin_rust.rs`): the function catalogue entries of OxFunc's `surface_dispatch.rs`
//! (implementation files, the files they use and declare, their tests and the files those include),
//! the test files of a cited source directory, and the declared modules (`mod x;`, with or without
//! `#[path]`) of every pinned Rust input, which join it with its beads and dispositions. A file a
//! resolution cannot read is counted by reason in the manifest's notes, never dropped silently.
//! The rules are restated in the manifest's `rules`.
//!
//! Nothing here compiles, audits or license-clears an input: every input's `verification` is
//! `provisional`, and bead dc-4r4l's compile audition owns any change of that state.
//!
//! The manifest records one pin date and no other time, and every list is sorted, so a second run
//! over unchanged repositories and an unchanged tracker reproduces it apart from that date.

use crate::cargo::Workspace;
use crate::pin_cite::{self, Cited, Disposition, REPOSITORIES};
use crate::pin_git::{BlobReader, Legacy};
use crate::pin_rust;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const USAGE: &str = "usage: cargo xtask pin-sources [--tracker <issues.jsonl>] [--out <file>]\n       cargo xtask pin-sources --lookup <Repo>/<path> [--manifest <file>]\n       (--lookup exits 0 for a pinned port or reference input, 1 unavailable, 3 quarantined or\n        quarantine_derived, 4 never-pinned bulk evidence, 2 a usage or manifest error)";

/// The migration record, relative to the workspace root.
pub const MANIFEST: &str = "migration/source-manifest.json";
pub const FORMAT: &str = "dnacalc-source-manifest/2";
pub const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";
const TRACKER: &str = ".beads/issues.jsonl";
/// The plan's source audit (2026-10-12), whose blobs the manifest compares with the pinned ones.
const PLAN_AUDIT: &str = "docs/plan/source-audit.json";

/// `--lookup` exit codes; 0 is a pinned port or reference input.
const EXIT_UNAVAILABLE: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_QUARANTINED: u8 = 3;
const EXIT_BULK_EVIDENCE: u8 = 4;

/// A path that is never pinned, committed or not.
struct NotPinned {
    repository: &'static str,
    prefix: &'static str,
    reason: &'static str,
    owner: &'static str,
    archive: &'static str,
}

const NOT_PINNED: [NotPinned; 2] = [
    NotPinned {
        repository: "OxFunc",
        prefix: "smart-fuzzer/cache/",
        reason: "bulk evidence",
        owner: "infra-m0.6",
        archive: "DnaCalc/OxFunc-Evidence",
    },
    NotPinned {
        repository: "OxFunc",
        prefix: "smart-fuzzer/runs/",
        reason: "bulk evidence",
        owner: "infra-m0.6",
        archive: "DnaCalc/OxFunc-Evidence",
    },
];

/// Paths quarantined whatever the citing text says: the numeric code whose provenance infra-m0.5
/// traces (x87.rs, mod.rs, tables.rs, gen_tables.py, gammaln.rs, research.rs and
/// x87_excel_ground_truth.tsv), pending that bead.
const QUARANTINED: [(&str, &str); 1] = [("OxFunc", "crates/oxfunc_core/src/excel_numeric/")];

/// The feature of `RESEARCH_CRATE` that compiles the x87 research surface (`RESEARCH_SURFACE`),
/// the only route by which code outside that crate reaches the quarantined x87 entry points.
const RESEARCH_CRATE: &str = "oxfunc_core";
const RESEARCH_FEATURE: &str = "research-x87";
const RESEARCH_SURFACE: [&str; 2] = ["excel_numeric", "research"];

/// Text that marks a file under a source-discovery path as driving Excel through COM or
/// automation.
const RUNNER_MARKERS: [&str; 5] = [
    "Excel.Application",
    "Workbooks.Open",
    "Marshal",
    "Interop",
    "ComObject",
];
const RUNNER_SELECTION: &str = "infra-m1.2 selects among the runner candidates when it ports the capture adapter; this pin selects none";

/// Source directories whose files form the parser list.
const PARSER_DIRS: [(&str, &str); 2] = [
    ("OxFml", "crates/oxfml_core/src/syntax/"),
    ("OxFml", "crates/oxfml_core/src/language_service/"),
];

/// Directories whose files form the function-family list, besides the files resolved from the
/// catalogue.
const FUNCTION_FAMILY_DIRS: [(&str, &str); 1] = [("OxFunc", "crates/oxfunc_core/src/functions/")];

/// Directories whose files form the oracle-runner list (the capture and verdict lineages), besides
/// the files reached through a source-discovery path.
const ORACLE_RUNNER_DIRS: [(&str, &str); 3] = [
    ("OxFunc", "smart-fuzzer/"),
    ("OxFunc", "tools/xll-addin/"),
    ("DnaTreeCalc", "src/dnacalc-conform/"),
];

/// Path components and extensions that put a file in the corpus list.
const CORPUS_COMPONENTS: [&str; 5] = ["fixtures", "corpus", "testdata", "golden", "evidence"];
const CORPUS_EXTENSIONS: [&str; 6] = ["tsv", "csv", "xlsx", "xlsm", "xlsb", "xls"];

/// Root file names (lower case) that are license files.
const LICENSE_PREFIXES: [&str; 4] = ["license", "licence", "copying", "unlicense"];

/// Why a resolution could not read a file; each is counted in the manifest's notes and summary.
const SKIP_NO_CRATE: &str =
    "helper references not read: the file is in no library crate of its repository";
const SKIP_NOT_IN_TREE: &str =
    "helper references not read: the file is not in its crate's module tree from src/lib.rs";
const SKIP_REFS_PARSE: &str = "helper references not read: the file does not parse";
const SKIP_DECLARATIONS_PARSE: &str = "module declarations not read: the file does not parse";
const SKIP_NON_LITERAL_INCLUDE: &str =
    "an include macro whose argument is not a string literal is not resolved";
const SKIP_MODULE_ABSENT: &str = "a declared module's file is absent at the pinned commit";
const SKIP_MODULE_AMBIGUOUS: &str = "a declared module has both <name>.rs and <name>/mod.rs";
const SKIP_MODULE_OUTSIDE: &str = "a declared module's #[path] leaves the repository";
const SKIP_MODULE_CFG_ATTR: &str =
    "a module declared through cfg_attr(..., path = ...) is not resolved";
const SKIP_CHILD_EXCLUDED: &str =
    "a declared module that the citing bead's Port from text excludes is not pinned for that bead";
const SKIP_CHILD_NOT_PINNED: &str = "a declared module under a never-pinned path is not pinned";
const SKIP_RESEARCH_PARSE: &str =
    "the x87 research-surface check read the file as text: it does not parse";
/// A counted fallback: a declared module found only by reading its parent as a `#[path]` file.
const FALLBACK_PATH_READING: &str = "a declared module was found only by reading its parent's children as those of a #[path] file (the parent is cited directly, so how it is loaded is not known)";

pub struct LegacyRoot {
    pub path: PathBuf,
    pub from_variable: bool,
}

/// The legacy root: `DNACALC_LEGACY_ROOT` when set and not empty, else the workspace root's parent.
pub fn legacy_root(workspace_root: &Path) -> LegacyRoot {
    match std::env::var_os(LEGACY_ROOT_VARIABLE).filter(|value| !value.is_empty()) {
        Some(value) => LegacyRoot {
            path: PathBuf::from(value),
            from_variable: true,
        },
        None => LegacyRoot {
            path: workspace_root
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            from_variable: false,
        },
    }
}

/// The legacy root as a receipt shows it, never as an absolute host path: `..` when it is the
/// workspace root's parent, a workspace-relative path when it lies inside the workspace, and
/// `<legacy-root>` anywhere else (the record's `from` then names `DNACALC_LEGACY_ROOT`).
pub fn shown_legacy_root(workspace_root: &Path, path: &Path) -> String {
    let (root, legacy) = match (fs::canonicalize(workspace_root), fs::canonicalize(path)) {
        (Ok(root), Ok(legacy)) => (root, legacy),
        _ => (workspace_root.to_path_buf(), path.to_path_buf()),
    };
    if root.parent() == Some(legacy.as_path()) {
        return "..".to_owned();
    }
    match legacy.strip_prefix(&root) {
        Ok(inside) if inside.as_os_str().is_empty() => ".".to_owned(),
        Ok(inside) => inside.to_string_lossy().replace('\\', "/"),
        Err(_) => "<legacy-root>".to_owned(),
    }
}

/// The legacy root as an acceptance receipt records it.
pub fn legacy_root_record(workspace_root: &Path) -> Value {
    let root = legacy_root(workspace_root);
    json!({
        "path": shown_legacy_root(workspace_root, &root.path),
        "from": if root.from_variable {
            LEGACY_ROOT_VARIABLE
        } else {
            "default: the workspace root's parent directory"
        },
        "exists": root.path.is_dir(),
    })
}

struct Args {
    tracker: Option<PathBuf>,
    out: Option<PathBuf>,
    lookup: Option<String>,
    manifest: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        tracker: None,
        out: None,
        lookup: None,
        manifest: None,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |name: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--tracker" => parsed.tracker = Some(PathBuf::from(value("--tracker")?)),
            "--out" => parsed.out = Some(PathBuf::from(value("--out")?)),
            "--lookup" => parsed.lookup = Some(value("--lookup")?),
            "--manifest" => parsed.manifest = Some(PathBuf::from(value("--manifest")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if parsed.lookup.is_some() && (parsed.tracker.is_some() || parsed.out.is_some()) {
        return Err("--lookup takes only --manifest".to_owned());
    }
    if parsed.lookup.is_none() && parsed.manifest.is_some() {
        return Err("--manifest is for --lookup".to_owned());
    }
    Ok(parsed)
}

pub fn run(raw: &[String]) -> ExitCode {
    let args = match parse_args(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask pin-sources: {e}\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("xtask pin-sources: no current directory: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask pin-sources: cannot read the workspace: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let at_root = |path: Option<PathBuf>, default: &str| match path {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => ws.root.join(default),
    };
    if let Some(wanted) = args.lookup {
        let manifest = at_root(args.manifest, MANIFEST);
        return lookup(&ws, &manifest, &wanted);
    }
    let tracker = at_root(args.tracker, TRACKER);
    let out = at_root(args.out, MANIFEST);
    match pin(&ws, &tracker) {
        Err(e) => {
            eprintln!("xtask pin-sources: {e}");
            ExitCode::from(1)
        }
        Ok(mut manifest) => {
            manifest["tracker"] = json!(ws.rel(&tracker));
            // The pin date is the day this pin content was first written: a run that reproduces
            // the existing manifest apart from its date keeps that date.
            let previous: Option<Value> = fs::read_to_string(&out)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
            if let Some(previous) = previous {
                let mut earlier = previous.clone();
                earlier["pin_date"] = manifest["pin_date"].clone();
                if earlier == manifest {
                    manifest["pin_date"] = previous["pin_date"].clone();
                    println!(
                        "PIN {} already holds this pin (pin date {}); its pin date is kept",
                        ws.rel(&out),
                        previous["pin_date"].as_str().unwrap_or("?")
                    );
                }
            }
            let text = manifest_text(&manifest);
            let written = out
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::write(&out, &text));
            if let Err(e) = written {
                eprintln!("xtask pin-sources: cannot write {}: {e}", out.display());
                return ExitCode::from(1);
            }
            print_summary(&manifest, &ws.rel(&out));
            ExitCode::SUCCESS
        }
    }
}

fn print_summary(manifest: &Value, out: &str) {
    let summary = &manifest["summary"];
    for repository in manifest["repositories"].as_array().into_iter().flatten() {
        println!(
            "PIN {} {} on_origin={} stash_count={} working_tree={} license={} inputs={}",
            repository["name"].as_str().unwrap_or_default(),
            repository["commit"].as_str().unwrap_or("absent"),
            repository["on_origin"],
            repository["stash_count"],
            repository["working_tree"]["status"],
            repository["license"]["status"].as_str().unwrap_or_default(),
            repository["inputs"],
        );
    }
    for item in manifest["unavailable"].as_array().into_iter().flatten() {
        println!(
            "PIN unavailable {}/{} cited by {}: {}",
            item["repository"].as_str().unwrap_or("?"),
            item["path"].as_str().unwrap_or_default(),
            item["beads"],
            item["reason"].as_str().unwrap_or_default()
        );
    }
    for item in manifest["changed_since_plan_audit"]
        .as_array()
        .into_iter()
        .flatten()
    {
        println!(
            "PIN changed since the plan audit {}: {} -> {}",
            item["path"].as_str().unwrap_or_default(),
            item["audit_blob"].as_str().unwrap_or("?"),
            item["pinned_blob"].as_str().unwrap_or("absent")
        );
    }
    for note in manifest["notes"].as_array().into_iter().flatten() {
        let note = note.as_str().unwrap_or_default();
        let shown: String = note.chars().take(240).collect();
        println!(
            "PIN note: {shown}{}",
            if shown.len() < note.len() { " ..." } else { "" }
        );
    }
    let by = &summary["by_disposition"];
    println!(
        "PIN wrote {out}: {} inputs, all provisional ({} port, {} reference, {} quarantined, {} quarantine_derived), {} with license none-found, {} runner candidates, {} unavailable, {} excluded; by category {}",
        summary["inputs"],
        by["port"],
        by["reference"],
        by["quarantined"],
        by["quarantine_derived"],
        summary["license_none_found"],
        summary["runner_candidates"],
        summary["unavailable"],
        summary["excluded"],
        summary["by_category"]
    );
}

/// One bead of the tracker.
struct Bead {
    id: String,
    description: String,
}

fn read_tracker(path: &Path) -> Result<Vec<Bead>, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read the tracker {}: {e}", path.display()))?;
    let mut beads = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(line)
            .map_err(|e| format!("{} line {} is not JSON: {e}", path.display(), index + 1))?;
        let (Some(id), Some(description)) = (record["id"].as_str(), record["description"].as_str())
        else {
            return Err(format!(
                "{} line {} has no id or description",
                path.display(),
                index + 1
            ));
        };
        beads.push(Bead {
            id: id.to_owned(),
            description: description.to_owned(),
        });
    }
    beads.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(beads)
}

/// One blob the plan's source audit inspected.
struct AuditEntry {
    /// `<Repo>/<path>` as the audit writes it.
    path: String,
    blob: String,
}

fn read_plan_audit(path: &Path) -> Result<Vec<AuditEntry>, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read the plan audit {}: {e}", path.display()))?;
    let audit: Value = serde_json::from_str(&text)
        .map_err(|e| format!("the plan audit {} is not JSON: {e}", path.display()))?;
    let sources = audit["sources"]
        .as_array()
        .ok_or_else(|| format!("the plan audit {} has no sources list", path.display()))?;
    let mut entries: Vec<AuditEntry> = sources
        .iter()
        .filter_map(|source| {
            Some(AuditEntry {
                path: source["path"].as_str()?.to_owned(),
                blob: source["blob_sha"].as_str()?.to_owned(),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Where a file's `mod` declarations look, as rustc keeps it: the directory of the declaring
/// scope, and for a file loaded as `<name>.rs` the extra `<name>` component its children add.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ModState {
    dir: String,
    relative: Option<String>,
}

/// One declared module of a file.
#[derive(Clone, Debug)]
struct Child {
    file: String,
    state: ModState,
    /// The module path below the declaring file's module: enclosing inline modules, then the name.
    module: Vec<String>,
    test_only: bool,
}

/// What one legacy repository looks like at pin time.
struct Repository {
    name: String,
    legacy: Legacy,
    record: Map<String, Value>,
    commit: Option<String>,
    tree: BTreeMap<String, String>,
    license_status: &'static str,
    reader: Option<BlobReader>,
    contents: BTreeMap<String, String>,
    children: BTreeMap<(String, ModState), Vec<Child>>,
}

impl Repository {
    fn open(root: &Path, name: &str) -> Result<Repository, String> {
        let legacy = Legacy::new(root.join(name));
        let mut record = Map::new();
        record.insert("name".to_owned(), json!(name));
        let mut repository = Repository {
            name: name.to_owned(),
            legacy,
            record,
            commit: None,
            tree: BTreeMap::new(),
            license_status: "none-found",
            reader: None,
            contents: BTreeMap::new(),
            children: BTreeMap::new(),
        };
        if !repository.legacy.dir.is_dir() {
            repository.record.insert("present".to_owned(), json!(false));
            repository.record.insert(
                "absent_reason".to_owned(),
                json!(format!("no directory {name} under the legacy root")),
            );
            return Ok(repository);
        }
        let legacy = &repository.legacy;
        let commit = legacy.text(&["rev-parse", "--verify", "HEAD^{commit}"])?;
        let kind = legacy.text(&["cat-file", "-t", &commit])?;
        if kind != "commit" {
            return Err(format!("{name}: HEAD {commit} is a {kind}, not a commit"));
        }
        let branch = legacy.text(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        let origin = legacy.text(&["config", "--get", "remote.origin.url"]).ok();
        let containing = legacy.text(&["branch", "-r", "--contains", &commit])?;
        let origin_branches: Vec<String> = containing
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("origin/") && !line.contains(" -> "))
            .map(str::to_owned)
            .collect();
        let status = legacy.status()?;
        let stash_count = if legacy.succeeds(&["rev-parse", "-q", "--verify", "refs/stash"]) {
            legacy
                .text(&["rev-list", "--walk-reflogs", "--count", "refs/stash"])?
                .parse::<u64>()
                .map_err(|e| format!("{name}: the stash count is not a number: {e}"))?
        } else {
            0
        };
        let tree = legacy.tree(&commit)?;
        let tracked = status.iter().filter(|line| !line.starts_with("??")).count();
        let untracked = status.len() - tracked;
        repository.record.insert("present".to_owned(), json!(true));
        repository.record.insert("commit".to_owned(), json!(commit));
        repository.record.insert("branch".to_owned(), json!(branch));
        repository.record.insert("origin".to_owned(), json!(origin));
        repository
            .record
            .insert("on_origin".to_owned(), json!(!origin_branches.is_empty()));
        repository
            .record
            .insert("origin_branches".to_owned(), json!(origin_branches));
        repository.record.insert(
            "working_tree".to_owned(),
            json!({
                "tracked_changes": tracked,
                "untracked": untracked,
                "status": status,
            }),
        );
        repository
            .record
            .insert("stash_count".to_owned(), json!(stash_count));
        repository
            .record
            .insert("files".to_owned(), json!(tree.len()));
        repository.commit = Some(commit);
        repository.tree = tree;
        repository.read_license()?;
        Ok(repository)
    }

    /// Records the root license files of the pinned tree and the license status they give.
    fn read_license(&mut self) -> Result<(), String> {
        let licenses: Vec<(String, String)> = self
            .tree
            .iter()
            .filter(|(path, _)| {
                let lower = path.to_ascii_lowercase();
                !path.contains('/') && LICENSE_PREFIXES.iter().any(|p| lower.starts_with(p))
            })
            .map(|(path, blob)| (path.clone(), blob.clone()))
            .collect();
        let mut license_files = Vec::new();
        for (path, blob) in licenses {
            let first_line = self
                .content(&path)?
                .lines()
                .map(|line| line.trim().trim_start_matches('#').trim())
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .chars()
                .take(120)
                .collect::<String>();
            license_files.push(json!({"path": path, "blob": blob, "first_line": first_line}));
        }
        self.license_status = if license_files.is_empty() {
            "none-found"
        } else {
            "found"
        };
        self.record.insert(
            "license".to_owned(),
            json!({"status": self.license_status, "files": license_files}),
        );
        Ok(())
    }

    /// The committed text of `path` at the pinned commit (lossy UTF-8), read once.
    fn content(&mut self, path: &str) -> Result<String, String> {
        if let Some(text) = self.contents.get(path) {
            return Ok(text.clone());
        }
        let blob = self
            .tree
            .get(path)
            .ok_or_else(|| format!("{}: {path} is not in the pinned tree", self.name))?
            .clone();
        if self.reader.is_none() {
            self.reader = Some(self.legacy.blobs()?);
        }
        let reader = self.reader.as_mut().ok_or("no blob reader")?;
        let text = String::from_utf8_lossy(&reader.read(&blob)?).into_owned();
        self.contents.insert(path.to_owned(), text.clone());
        Ok(text)
    }

    fn is_dir(&self, dir: &str) -> bool {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.tree
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(path, _)| path.starts_with(&prefix))
    }

    fn files_under(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.tree
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
            .map(|(path, _)| path.clone())
            .filter(|path| !not_pinned(&self.name, path))
            .collect()
    }
}

fn not_pinned(repository: &str, path: &str) -> bool {
    NOT_PINNED
        .iter()
        .any(|np| np.repository == repository && path.starts_with(np.prefix))
}

fn quarantined(repository: &str, path: &str) -> bool {
    under(&QUARANTINED, repository, path)
}

fn under(table: &[(&str, &str)], repository: &str, path: &str) -> bool {
    table
        .iter()
        .any(|(r, prefix)| *r == repository && path.starts_with(prefix))
}

fn repository<'a>(
    repositories: &'a mut [Repository],
    name: &str,
) -> Result<&'a mut Repository, String> {
    repositories
        .iter_mut()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("{name} is not a legacy repository"))
}

/// The reasons a resolution skipped a file, and the counted fallbacks, each with what it touched.
#[derive(Default)]
struct Skips {
    skipped: BTreeMap<&'static str, BTreeSet<String>>,
    fallbacks: BTreeMap<&'static str, BTreeSet<String>>,
}

impl Skips {
    fn skip(&mut self, reason: &'static str, what: String) {
        self.skipped.entry(reason).or_default().insert(what);
    }

    fn fallback(&mut self, reason: &'static str, what: String) {
        self.fallbacks.entry(reason).or_default().insert(what);
    }

    fn notes(&self) -> Vec<String> {
        let line = |kind: &str, reason: &str, items: &BTreeSet<String>| {
            format!(
                "{kind}: {reason}: {}: {}",
                items.len(),
                items.iter().cloned().collect::<Vec<_>>().join("; ")
            )
        };
        self.skipped
            .iter()
            .map(|(reason, items)| line("resolution skipped", reason, items))
            .chain(
                self.fallbacks
                    .iter()
                    .map(|(reason, items)| line("counted fallback", reason, items)),
            )
            .collect()
    }

    fn counts(map: &BTreeMap<&'static str, BTreeSet<String>>) -> Value {
        Value::Object(
            map.iter()
                .map(|(reason, items)| ((*reason).to_owned(), json!(items.len())))
                .collect(),
        )
    }
}

/// How an input came to be selected.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Via {
    /// Cited as written, or as a file of a cited directory.
    Cited(String),
    /// A file of a cited source-discovery path.
    Discovered(String),
    /// An implementation file of a catalogue entry.
    Catalogue,
    /// A file a catalogue implementation file uses or declares, directly or through other files.
    Helper,
    /// A test that names a catalogue entry, or any other test of the catalogue's crate.
    CatalogueTest,
    /// A test of a cited source directory.
    DirectoryTest(String),
    /// A file a selected source or test includes, or a fixture a selected test names.
    Included,
    /// A module that a pinned Rust input declares (`mod x;`, with or without `#[path]`).
    Declared(String),
}

impl Via {
    fn text(&self) -> String {
        match self {
            Via::Cited(token) => format!("cited {token}"),
            Via::Discovered(token) => format!("discovery {token}"),
            Via::Catalogue => "catalogue implementation".to_owned(),
            Via::Helper => "catalogue helper".to_owned(),
            Via::CatalogueTest => "catalogue test".to_owned(),
            Via::DirectoryTest(dir) => format!("test of {dir}"),
            Via::Included => "included or named by a selected file".to_owned(),
            Via::Declared(parent) => format!("declared module of {parent}"),
        }
    }
}

#[derive(Default)]
struct Input {
    /// Each citing bead with its reading of the input and how it reached the input.
    beads: BTreeMap<String, (Disposition, BTreeSet<Via>)>,
    via: BTreeSet<Via>,
    named: BTreeSet<String>,
    symbols: BTreeSet<String>,
    wants_exported: bool,
    exported: Option<Result<Vec<String>, String>>,
    uses: Option<Vec<String>>,
    /// The runner markers a file under a source-discovery path holds.
    runner_markers: Vec<String>,
    /// Why the input is quarantine_derived.
    derived: Option<String>,
}

type Key = (String, String);

#[derive(Default)]
struct Selection {
    inputs: BTreeMap<Key, Input>,
    unavailable: BTreeMap<Key, (BTreeSet<String>, String)>,
    excluded: BTreeMap<Key, BTreeSet<String>>,
}

impl Selection {
    fn add(
        &mut self,
        repository: &str,
        path: &str,
        bead: &str,
        disposition: Disposition,
        via: Via,
    ) -> &mut Input {
        let disposition = if quarantined(repository, path) {
            Disposition::Quarantined
        } else {
            disposition
        };
        let input = self
            .inputs
            .entry((repository.to_owned(), path.to_owned()))
            .or_default();
        let entry = input
            .beads
            .entry(bead.to_owned())
            .or_insert((disposition, BTreeSet::new()));
        // A bead reaching a path twice keeps the more restrictive reading.
        entry.0 = entry.0.max(disposition);
        entry.1.insert(via.clone());
        input.via.insert(via);
        input
    }

    fn unavailable(&mut self, repository: &str, path: &str, bead: &str, reason: String) {
        let entry = self
            .unavailable
            .entry((repository.to_owned(), path.to_owned()))
            .or_insert_with(|| (BTreeSet::new(), reason));
        entry.0.insert(bead.to_owned());
    }
}

/// One Port from text and the beads that carry it.
struct Citation {
    beads: BTreeSet<String>,
    port_from: pin_cite::PortFrom,
    resolved: BTreeSet<Key>,
}

fn pin(ws: &Workspace, tracker: &Path) -> Result<Value, String> {
    let root = legacy_root(&ws.root);
    if !root.path.is_dir() {
        return Err(format!(
            "the legacy root {} does not exist ({}); set {LEGACY_ROOT_VARIABLE} to the directory that holds the legacy repositories",
            root.path.display(),
            if root.from_variable {
                LEGACY_ROOT_VARIABLE
            } else {
                "the workspace root's parent"
            }
        ));
    }
    let beads = read_tracker(tracker)?;
    let audit = read_plan_audit(&ws.root.join(PLAN_AUDIT))?;
    let mut repositories: Vec<Repository> = REPOSITORIES
        .iter()
        .map(|name| Repository::open(&root.path, name))
        .collect::<Result<_, _>>()?;
    build(&beads, &mut repositories, &audit)
}

/// The manifest for `beads` over `repositories`; everything after reading the tracker, the plan
/// audit and the repositories' state.
fn build(
    beads: &[Bead],
    repositories: &mut [Repository],
    audit: &[AuditEntry],
) -> Result<Value, String> {
    let mut citations: BTreeMap<String, Citation> = BTreeMap::new();
    let mut family_do: BTreeMap<String, String> = BTreeMap::new();
    for bead in beads {
        let Some(text) = pin_cite::section(&bead.description, "Port from") else {
            continue;
        };
        let port_from = pin_cite::parse(text);
        if port_from.cited.is_empty() {
            continue;
        }
        if port_from.cited.iter().any(|c| c.catalogue)
            && let Some(do_text) = pin_cite::section(&bead.description, "Do")
        {
            family_do.insert(bead.id.clone(), do_text.to_owned());
        }
        citations
            .entry(text.to_owned())
            .or_insert_with(|| Citation {
                beads: BTreeSet::new(),
                port_from,
                resolved: BTreeSet::new(),
            })
            .beads
            .insert(bead.id.clone());
    }

    let mut selection = Selection::default();
    let mut skips = Skips::default();
    let mut catalogue_citations: BTreeMap<Key, BTreeMap<String, Disposition>> = BTreeMap::new();
    let mut test_dirs: BTreeMap<Key, BTreeMap<String, Disposition>> = BTreeMap::new();
    for citation in citations.values_mut() {
        let mut anchor: Option<(String, String, bool)> = None;
        for cited in &citation.port_from.cited {
            let resolved = resolve_cited(cited, &anchor, repositories);
            if let Some(repository) = &cited.repository {
                anchor = Some((repository.clone(), cited.path.clone(), cited.dir));
            }
            let (repository, path) = match resolved {
                Err((repository, path, reason)) => {
                    for bead in &citation.beads {
                        selection.unavailable(&repository, &path, bead, reason.clone());
                    }
                    continue;
                }
                Ok(found) => found,
            };
            if cited.excluded {
                for bead in &citation.beads {
                    selection
                        .excluded
                        .entry((repository.clone(), path.clone()))
                        .or_default()
                        .insert(bead.clone());
                }
                continue;
            }
            let state = repositories
                .iter()
                .find(|r| r.name == repository)
                .ok_or_else(|| format!("{repository} is not a legacy repository"))?;
            let files = if cited.dir {
                state.files_under(&path)
            } else {
                vec![path.clone()]
            };
            let via = if cited.discovery {
                Via::Discovered(path.clone())
            } else {
                Via::Cited(path.clone())
            };
            for file in &files {
                for bead in &citation.beads {
                    let input =
                        selection.add(&repository, file, bead, cited.disposition, via.clone());
                    input.named.extend(cited.symbols.iter().cloned());
                    input.wants_exported |=
                        citation.port_from.wants_symbols && file.ends_with(".rs");
                }
                citation.resolved.insert((repository.clone(), file.clone()));
            }
            if cited.catalogue {
                let beads = catalogue_citations
                    .entry((repository.clone(), path.clone()))
                    .or_default();
                for bead in &citation.beads {
                    beads.insert(bead.clone(), cited.disposition);
                }
            }
            if cited.dir && citation.port_from.wants_tests {
                let beads = test_dirs
                    .entry((repository.clone(), path.clone()))
                    .or_default();
                for bead in &citation.beads {
                    beads.insert(bead.clone(), cited.disposition);
                }
            }
        }
    }

    let mut trees = Trees::new();
    let mut functions = Vec::new();
    let mut families = Vec::new();
    let mut notes = Vec::new();
    for ((name, path), beads) in &catalogue_citations {
        let state = repository(repositories, name)?;
        let resolved =
            resolve_catalogue(state, path, beads, &mut selection, &mut trees, &mut skips)?;
        notes.extend(resolved.notes);
        for (bead, do_text) in &family_do {
            if beads.contains_key(bead) {
                families.push(family_record(bead, do_text, &resolved.functions));
            }
        }
        functions.extend(resolved.functions);
    }
    for ((name, dir), beads) in &test_dirs {
        let state = repository(repositories, name)?;
        resolve_directory_tests(state, dir, beads, &mut selection, &mut skips)?;
    }
    let declared = expand_declared(repositories, &trees, &mut selection, &mut skips)?;
    for citation in citations.values_mut() {
        let mut pending: Vec<Key> = citation.resolved.iter().cloned().collect();
        while let Some(key) = pending.pop() {
            for child in declared.get(&key).into_iter().flatten() {
                if citation.resolved.insert(child.clone()) {
                    pending.push(child.clone());
                }
            }
        }
    }
    mark_quarantine_derived(repositories, &mut selection, &mut skips)?;
    mark_runner_candidates(repositories, &mut selection)?;

    for ((name, path), input) in selection.inputs.iter_mut() {
        let state = repository(repositories, name)?;
        if !input.named.is_empty() {
            let text = state.content(path)?;
            input.symbols = input
                .named
                .iter()
                .filter(|symbol| contains_symbol(&text, symbol))
                .cloned()
                .collect();
        }
        if input.wants_exported && input.exported.is_none() {
            let text = state.content(path)?;
            input.exported = Some(pin_rust::parse(&text).map(|file| pin_rust::exported(&file)));
        }
    }
    notes.extend(skips.notes());
    let (plan_audit, changed) = plan_audit_drift(repositories, audit, &selection);

    Ok(assemble(Parts {
        repositories,
        citations: &citations,
        selection: &selection,
        functions,
        families,
        notes,
        skips: &skips,
        plan_audit,
        changed,
    }))
}

/// Resolves one cited token to `(repository, path)`, or says why it is unavailable. A relative
/// token is looked up from the directory of the previous anchored token upwards to the repository
/// root; the deepest existing candidate wins.
fn resolve_cited(
    cited: &Cited,
    anchor: &Option<(String, String, bool)>,
    repositories: &[Repository],
) -> Result<(String, String), (String, String, String)> {
    let (repository, candidates) = match (&cited.repository, anchor) {
        (Some(repository), _) => (repository.clone(), vec![cited.path.clone()]),
        (None, Some((repository, anchor_path, anchor_dir))) => {
            let mut base: Vec<&str> = anchor_path
                .trim_end_matches('/')
                .split('/')
                .filter(|p| !p.is_empty())
                .collect();
            if !anchor_dir {
                base.pop();
            }
            let mut candidates = Vec::new();
            loop {
                let mut joined = base.join("/");
                if !joined.is_empty() {
                    joined.push('/');
                }
                joined.push_str(&cited.path);
                candidates.push(joined);
                if base.pop().is_none() {
                    break;
                }
            }
            (repository.clone(), candidates)
        }
        (None, None) => {
            return Err((
                String::new(),
                cited.path.clone(),
                "a relative path with no repository path before it in the Port from text"
                    .to_owned(),
            ));
        }
    };
    let Some(state) = repositories.iter().find(|r| r.name == repository) else {
        return Err((
            repository.clone(),
            candidates[0].clone(),
            format!("{repository} is not a legacy repository"),
        ));
    };
    let Some(commit) = &state.commit else {
        return Err((
            repository.clone(),
            candidates[0].clone(),
            format!("the repository {repository} is not present under the legacy root"),
        ));
    };
    for candidate in &candidates {
        let exists = if cited.dir {
            state.is_dir(candidate)
        } else {
            state.tree.contains_key(candidate)
        };
        if exists {
            if not_pinned(&repository, candidate) {
                return Err((
                    repository.clone(),
                    candidate.clone(),
                    "a path that is never pinned".to_owned(),
                ));
            }
            return Ok((repository.clone(), candidate.clone()));
        }
    }
    Err((
        repository,
        candidates[0].clone(),
        format!("absent at the pinned commit {commit}; route its tasks to fresh implementation"),
    ))
}

/// Whether `text` holds `symbol` as a whole word; a qualified `A::B` also counts when `A` and `B`
/// each stand whole in the text.
fn contains_symbol(text: &str, symbol: &str) -> bool {
    let whole = |word: &str| {
        let bytes = text.as_bytes();
        let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        text.match_indices(word).any(|(at, _)| {
            let end = at + word.len();
            (at == 0 || !ident(bytes[at - 1])) && (end == bytes.len() || !ident(bytes[end]))
        })
    };
    whole(symbol) || (symbol.contains("::") && symbol.split("::").all(whole))
}

/// The library crates of a repository: crate name (directory name, `-` read as `_`) to its
/// `src` directory.
fn library_crates(tree: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut crates = BTreeMap::new();
    for path in tree.keys() {
        if let Some(dir) = path.strip_suffix("/src/lib.rs")
            && tree.contains_key(&format!("{dir}/Cargo.toml"))
        {
            let name = dir.rsplit('/').next().unwrap_or(dir).replace('-', "_");
            crates.insert(name, format!("{dir}/src"));
        }
    }
    crates
}

/// The library crate whose `src` directory holds `path` (the deepest one), as (name, src).
fn owning_crate(crates: &BTreeMap<String, String>, path: &str) -> Option<(String, String)> {
    crates
        .iter()
        .filter(|(_, src)| path.starts_with(&format!("{src}/")))
        .max_by_key(|(_, src)| src.len())
        .map(|(name, src)| (name.clone(), src.clone()))
}

/// `name` appended to the directory `dir` of a repository path.
fn push(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_owned())
}

/// How a file not reached through a declaration reads its children: a crate root or `mod.rs`
/// (`lib.rs`, `main.rs`, `mod.rs`, `build.rs`, or a file directly in `bin`, `tests`, `examples` or
/// `benches`) from its own directory, any other `<name>.rs` from `<name>/` beside it.
fn inferred_state(path: &str) -> ModState {
    let dir = parent_dir(path);
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    let parent_name = dir.rsplit('/').next().unwrap_or(&dir);
    let root_like = matches!(file, "lib.rs" | "main.rs" | "mod.rs" | "build.rs")
        || matches!(parent_name, "bin" | "tests" | "examples" | "benches");
    ModState {
        relative: (!root_like).then(|| stem.to_owned()),
        dir,
    }
}

/// The file a declaration names, with the state its own children read, following rustc: a
/// `#[path]` on an out-of-line module is relative to the declaring scope's directory and its file
/// reads children as a `mod.rs` does; without `#[path]` the file is `<name>.rs` or
/// `<name>/mod.rs` under the scope's directory (with a non-mod-rs file's own name added); an
/// enclosing inline module adds its name (or replaces the directory with its own `#[path]`). The
/// boolean is true when the file was found only by reading the parent as a `#[path]` file.
fn resolve_declaration(
    tree: &BTreeMap<String, String>,
    state: &ModState,
    decl: &pin_rust::ModDecl,
) -> Result<(String, ModState, bool), &'static str> {
    let mut dir = state.dir.clone();
    let mut relative = state.relative.clone();
    for inline in &decl.inline {
        match &inline.path {
            Some(path) => {
                dir = pin_rust::join(&dir, &path.replace('\\', "/")).ok_or(SKIP_MODULE_OUTSIDE)?;
            }
            None => {
                if let Some(name) = relative.take() {
                    dir = push(&dir, &name);
                }
                dir = push(&dir, &inline.name);
            }
        }
        relative = None;
    }
    if let Some(path) = &decl.path {
        let file = pin_rust::join(&dir, &path.replace('\\', "/")).ok_or(SKIP_MODULE_OUTSIDE)?;
        if !tree.contains_key(&file) {
            return Err(SKIP_MODULE_ABSENT);
        }
        let state = ModState {
            dir: parent_dir(&file),
            relative: None,
        };
        return Ok((file, state, false));
    }
    if decl.cfg_attr_path {
        return Err(SKIP_MODULE_CFG_ATTR);
    }
    let lookup = |base: &str| {
        let flat = push(base, &format!("{}.rs", decl.name));
        let nested = push(&push(base, &decl.name), "mod.rs");
        match (tree.contains_key(&flat), tree.contains_key(&nested)) {
            (true, false) => Ok((
                flat,
                ModState {
                    dir: base.to_owned(),
                    relative: Some(decl.name.clone()),
                },
            )),
            (false, true) => Ok((
                nested,
                ModState {
                    dir: push(base, &decl.name),
                    relative: None,
                },
            )),
            (true, true) => Err(SKIP_MODULE_AMBIGUOUS),
            (false, false) => Err(SKIP_MODULE_ABSENT),
        }
    };
    match &relative {
        None => lookup(&dir).map(|(file, state)| (file, state, false)),
        Some(name) => match lookup(&push(&dir, name)) {
            Err(SKIP_MODULE_ABSENT) => lookup(&dir).map(|(file, state)| (file, state, true)),
            other => other.map(|(file, state)| (file, state, false)),
        },
    }
}

/// Whether `text` may hold an out-of-line declaration: some line has `mod <name>` followed by `;`.
fn may_declare_modules(text: &str) -> bool {
    text.lines().any(|line| {
        line.match_indices("mod ").any(|(at, _)| {
            let rest = line[at + 4..].trim_start();
            let name_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '#'))
                .unwrap_or(rest.len());
            name_end > 0 && rest[name_end..].trim_start().starts_with(';')
        })
    })
}

/// The declared modules of `path` read with `state`, cached per repository.
fn children_of(
    repo: &mut Repository,
    path: &str,
    state: &ModState,
    skips: &mut Skips,
) -> Result<Vec<Child>, String> {
    let key = (path.to_owned(), state.clone());
    if let Some(found) = repo.children.get(&key) {
        return Ok(found.clone());
    }
    let text = repo.content(path)?;
    let mut children = Vec::new();
    if may_declare_modules(&text) {
        match pin_rust::parse_full(&text) {
            Err(e) => skips.skip(
                SKIP_DECLARATIONS_PARSE,
                format!("{}/{path}: {e}", repo.name),
            ),
            Ok(file) => {
                for decl in pin_rust::declared_modules(&file) {
                    match resolve_declaration(&repo.tree, state, &decl) {
                        Ok((file, child_state, fallback)) => {
                            if fallback {
                                skips.fallback(
                                    FALLBACK_PATH_READING,
                                    format!("{}/{path}: mod {} -> {file}", repo.name, decl.name),
                                );
                            }
                            let mut module: Vec<String> =
                                decl.inline.iter().map(|m| m.name.clone()).collect();
                            module.push(decl.name.clone());
                            children.push(Child {
                                file,
                                state: child_state,
                                module,
                                test_only: decl.test_only,
                            });
                        }
                        Err(reason) => {
                            skips.skip(reason, format!("{}/{path}: mod {}", repo.name, decl.name))
                        }
                    }
                }
            }
        }
    }
    repo.children.insert(key, children.clone());
    Ok(children)
}

/// The module tree of one library crate, walked from `src/lib.rs` through its declarations.
#[derive(Default)]
struct CrateTree {
    /// Module path to the files that define it (more than one when `cfg` picks between them).
    modules: BTreeMap<Vec<String>, BTreeSet<String>>,
    /// File to its module path and the state its children read.
    files: BTreeMap<String, (Vec<String>, ModState)>,
}

/// Crate trees by (repository, src directory).
type Trees = BTreeMap<(String, String), CrateTree>;

fn ensure_tree(
    state: &mut Repository,
    src: &str,
    trees: &mut Trees,
    skips: &mut Skips,
) -> Result<(), String> {
    let key = (state.name.clone(), src.to_owned());
    if trees.contains_key(&key) {
        return Ok(());
    }
    let mut tree = CrateTree::default();
    let mut pending = vec![(
        format!("{src}/lib.rs"),
        Vec::<String>::new(),
        ModState {
            dir: src.to_owned(),
            relative: None,
        },
    )];
    while let Some((path, module, mod_state)) = pending.pop() {
        if tree.files.contains_key(&path) {
            continue;
        }
        tree.modules
            .entry(module.clone())
            .or_default()
            .insert(path.clone());
        tree.files
            .insert(path.clone(), (module.clone(), mod_state.clone()));
        for child in children_of(state, &path, &mod_state, skips)? {
            let mut child_module = module.clone();
            child_module.extend(child.module.iter().cloned());
            pending.push((child.file, child_module, child.state));
        }
    }
    trees.insert(key, tree);
    Ok(())
}

/// The files that define the longest prefix of `path` (at least `min` segments) in `tree`.
fn module_files(tree: &CrateTree, path: &[String], min: usize) -> BTreeSet<String> {
    for length in (min..=path.len()).rev() {
        if let Some(files) = tree.modules.get(&path[..length]) {
            return files.clone();
        }
    }
    BTreeSet::new()
}

/// What one file of a catalogue closure brings in.
struct Uses {
    /// The files its non-test code refers to, within the repository's library crates.
    refs: Vec<String>,
    /// The files of its non-test declared modules.
    children: Vec<String>,
}

fn uses_of(
    state: &mut Repository,
    crates: &BTreeMap<String, String>,
    trees: &mut Trees,
    path: &str,
    skips: &mut Skips,
) -> Result<Uses, String> {
    let shown = format!("{}/{path}", state.name);
    let non_test = |children: Vec<Child>| -> Vec<String> {
        children
            .into_iter()
            .filter(|c| !c.test_only)
            .map(|c| c.file)
            .collect()
    };
    let Some((own, src)) = owning_crate(crates, path) else {
        skips.skip(SKIP_NO_CRATE, shown);
        let children = children_of(state, path, &inferred_state(path), skips)?;
        return Ok(Uses {
            refs: Vec::new(),
            children: non_test(children),
        });
    };
    ensure_tree(state, &src, trees, skips)?;
    let own_key = (state.name.clone(), src.clone());
    let Some((module, mod_state)) = trees[&own_key].files.get(path).cloned() else {
        skips.skip(SKIP_NOT_IN_TREE, shown);
        let children = children_of(state, path, &inferred_state(path), skips)?;
        return Ok(Uses {
            refs: Vec::new(),
            children: non_test(children),
        });
    };
    let children = non_test(children_of(state, path, &mod_state, skips)?);
    let text = state.content(path)?;
    let file = match pin_rust::parse(&text) {
        Ok(file) => file,
        Err(e) => {
            skips.skip(SKIP_REFS_PARSE, format!("{shown}: {e}"));
            return Ok(Uses {
                refs: Vec::new(),
                children,
            });
        }
    };
    let others: BTreeSet<String> = crates.keys().filter(|n| **n != own).cloned().collect();
    let mut used = BTreeSet::new();
    for reference in pin_rust::module_refs(&file, &module, &others) {
        let (crate_src, min) = match &reference.krate {
            Some(name) => (crates[name].clone(), 0),
            None if reference.relative => (src.clone(), module.len() + 1),
            None => (src.clone(), 0),
        };
        ensure_tree(state, &crate_src, trees, skips)?;
        let key = (state.name.clone(), crate_src);
        for found in module_files(&trees[&key], &reference.path, min) {
            if found != path {
                used.insert(found);
            }
        }
    }
    Ok(Uses {
        refs: used.into_iter().collect(),
        children,
    })
}

struct CatalogueResolution {
    functions: Vec<Value>,
    notes: Vec<String>,
}

/// Resolves the catalogue entries of `dispatch` (OxFunc's `surface_dispatch.rs`) to files: each
/// entry's implementation modules, the files those use and declare (transitively, without passing
/// through the dispatch file itself), every test of the crate (each attributed to the entries it
/// names) and the files those sources include or name. Every such file becomes an input of the
/// citing beads.
fn resolve_catalogue(
    state: &mut Repository,
    dispatch: &str,
    beads: &BTreeMap<String, Disposition>,
    selection: &mut Selection,
    trees: &mut Trees,
    skips: &mut Skips,
) -> Result<CatalogueResolution, String> {
    let text = state.content(dispatch)?;
    let file = pin_rust::parse(&text).map_err(|e| format!("{}/{dispatch} {e}", state.name))?;
    let entries = pin_rust::catalogue(&file);
    let crates = library_crates(&state.tree);
    let (_, src) = owning_crate(&crates, dispatch)
        .ok_or_else(|| format!("{}/{dispatch} is in no library crate", state.name))?;
    ensure_tree(state, &src, trees, skips)?;
    let tree_key = (state.name.clone(), src.clone());
    let dispatch_module = trees[&tree_key]
        .files
        .get(dispatch)
        .map(|(module, _)| module.clone())
        .ok_or_else(|| {
            format!(
                "{}/{dispatch} is not in its crate's module tree",
                state.name
            )
        })?;
    let functions_module = &dispatch_module[..dispatch_module.len().saturating_sub(1)];
    let crate_dir = src.strip_suffix("/src").unwrap_or(&src).to_owned();
    let tests_dir = format!("{crate_dir}/tests");
    let constants: BTreeMap<String, String> = entries
        .iter()
        .map(|e| (e.constant.clone(), e.id.clone()))
        .collect();
    let ids: BTreeSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    let mut notes = Vec::new();
    let name = state.name.clone();
    let add = |selection: &mut Selection, path: &str, via: Via| {
        for (bead, disposition) in beads {
            selection.add(&name, path, bead, *disposition, via.clone());
        }
    };

    // Implementation files and the closure of the files they use or declare.
    let mut implementation: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in &entries {
        let mut files = Vec::new();
        for module in &entry.modules {
            let mut path = functions_module.to_vec();
            path.push(module.clone());
            match trees[&tree_key].modules.get(&path) {
                Some(found) => files.extend(found.iter().cloned()),
                None => notes.push(format!(
                    "{}: module {} has no file in the module tree of {src}",
                    entry.id,
                    path.join("::")
                )),
            }
        }
        implementation.insert(entry.id.clone(), files);
    }
    let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut declared_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut pending: Vec<String> = implementation.values().flatten().cloned().collect();
    while let Some(path) = pending.pop() {
        if uses.contains_key(&path) || path == dispatch {
            continue;
        }
        let found = uses_of(state, &crates, trees, &path, skips)?;
        for child in &found.children {
            declared_by
                .entry(child.clone())
                .or_default()
                .insert(path.clone());
        }
        let used: BTreeSet<String> = found.refs.into_iter().chain(found.children).collect();
        pending.extend(used.iter().cloned());
        uses.insert(path, used.into_iter().collect());
    }
    let implementation_files: BTreeSet<String> =
        implementation.values().flatten().cloned().collect();
    for (path, used) in &uses {
        let via = if implementation_files.contains(path) {
            Via::Catalogue
        } else {
            Via::Helper
        };
        add(selection, path, via);
        for parent in declared_by.get(path).into_iter().flatten() {
            add(selection, path, Via::Declared(parent.clone()));
        }
        if let Some(input) = selection.inputs.get_mut(&(name.clone(), path.clone())) {
            input.uses = Some(used.clone());
        }
    }

    // Tests, attributed to the entries they name, and what they include or name.
    let test_files = state.files_under(&tests_dir);
    let mut tests_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unattributed = Vec::new();
    let mut sources: Vec<String> = uses.keys().cloned().collect();
    for test in test_files.iter().filter(|p| p.ends_with(".rs")) {
        let text = state.content(test)?;
        let named = pin_rust::mentioned_ids(&text, &constants, &ids);
        if named.is_empty() {
            unattributed.push(test.clone());
        }
        for id in named {
            tests_of.entry(id).or_default().push(test.clone());
        }
        add(selection, test, Via::CatalogueTest);
        sources.push(test.clone());
    }
    let fixtures: Vec<String> = test_files
        .iter()
        .filter(|p| !p.ends_with(".rs"))
        .cloned()
        .collect();
    for source in &sources {
        let text = state.content(source)?;
        for included in included_files(state, source, &text, &fixtures, skips) {
            add(selection, &included, Via::Included);
        }
    }

    let mut functions = Vec::new();
    for entry in &entries {
        let files = implementation.get(&entry.id).cloned().unwrap_or_default();
        let mut reached = BTreeSet::new();
        let mut pending = files.clone();
        while let Some(path) = pending.pop() {
            if reached.insert(path.clone())
                && let Some(next) = uses.get(&path)
            {
                pending.extend(next.iter().cloned());
            }
        }
        let quarantined_helpers: Vec<&String> = reached
            .iter()
            .filter(|path| quarantined(&state.name, path))
            .collect();
        let mut record = json!({
            "id": entry.id,
            "name": entry.id.strip_prefix("FUNC.").unwrap_or(&entry.id),
            "modules": entry.modules,
            "files": files,
            "tests": tests_of.get(&entry.id).cloned().unwrap_or_default(),
        });
        if !quarantined_helpers.is_empty() {
            record["quarantined_helpers"] = json!(quarantined_helpers);
        }
        functions.push(record);
    }
    notes.push(format!(
        "{} tests under {}/{tests_dir} name no catalogue entry and are pinned for every citing bead: {}",
        unattributed.len(),
        state.name,
        unattributed.join(", ")
    ));
    Ok(CatalogueResolution { functions, notes })
}

/// The files a selected source includes literally (`include!`, `include_str!`, `include_bytes!`)
/// and the fixtures it names by file name; a non-literal include is counted, not followed.
fn included_files(
    state: &Repository,
    source: &str,
    text: &str,
    fixtures: &[String],
    skips: &mut Skips,
) -> BTreeSet<String> {
    let dir = parent_dir(source);
    let includes = pin_rust::include_paths(text);
    for call in &includes.non_literal {
        skips.skip(
            SKIP_NON_LITERAL_INCLUDE,
            format!("{}/{source}: {call}", state.name),
        );
    }
    let mut found: BTreeSet<String> = includes
        .literal
        .iter()
        .filter_map(|included| pin_rust::join(&dir, included))
        .filter(|path| state.tree.contains_key(path) && !not_pinned(&state.name, path))
        .collect();
    for fixture in fixtures {
        let name = fixture.rsplit('/').next().unwrap_or(fixture);
        if text.contains(name) {
            found.insert(fixture.clone());
        }
    }
    found
}

/// The tests of the crate that holds `dir` that name its module (`<module>::`), with the files
/// they include or name.
fn resolve_directory_tests(
    state: &mut Repository,
    dir: &str,
    beads: &BTreeMap<String, Disposition>,
    selection: &mut Selection,
    skips: &mut Skips,
) -> Result<(), String> {
    let trimmed = dir.trim_end_matches('/');
    let Some((src_and_crate, module)) = trimmed.rsplit_once('/') else {
        return Ok(());
    };
    let Some(crate_dir) = src_and_crate.strip_suffix("/src") else {
        return Ok(());
    };
    let tests_dir = format!("{crate_dir}/tests");
    let test_files = state.files_under(&tests_dir);
    let fixtures: Vec<String> = test_files
        .iter()
        .filter(|p| !p.ends_with(".rs"))
        .cloned()
        .collect();
    let needle = format!("{module}::");
    for test in test_files.iter().filter(|p| p.ends_with(".rs")) {
        let text = state.content(test)?;
        let names_module = text.match_indices(&needle).any(|(at, _)| {
            at == 0 || {
                let b = text.as_bytes()[at - 1];
                !(b.is_ascii_alphanumeric() || b == b'_')
            }
        });
        if !names_module {
            continue;
        }
        let included = included_files(state, test, &text, &fixtures, skips);
        for (bead, disposition) in beads {
            selection.add(
                &state.name,
                test,
                bead,
                *disposition,
                Via::DirectoryTest(dir.to_owned()),
            );
            for path in &included {
                selection.add(&state.name, path, bead, *disposition, Via::Included);
            }
        }
    }
    Ok(())
}

/// Pins the declared modules of every pinned Rust input, transitively, each for the parent's
/// beads with the parent's disposition for that bead, and returns every parent's children. A
/// child that a bead's own Port from text excludes, or that lies under a never-pinned path, is
/// not pinned for it and is counted.
fn expand_declared(
    repositories: &mut [Repository],
    trees: &Trees,
    selection: &mut Selection,
    skips: &mut Skips,
) -> Result<BTreeMap<Key, BTreeSet<Key>>, String> {
    let mut reached: BTreeMap<Key, ModState> = BTreeMap::new();
    let mut declared: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    loop {
        let mut changed = false;
        let parents: Vec<Key> = selection
            .inputs
            .keys()
            .filter(|(_, path)| path.ends_with(".rs"))
            .cloned()
            .collect();
        for key in parents {
            let (name, parent) = &key;
            let mod_state = trees
                .iter()
                .filter(|((r, _), _)| r == name)
                .find_map(|(_, tree)| tree.files.get(parent).map(|(_, s)| s.clone()))
                .or_else(|| reached.get(&key).cloned())
                .unwrap_or_else(|| inferred_state(parent));
            let state = repository(repositories, name)?;
            let children = children_of(state, parent, &mod_state, skips)?;
            if children.is_empty() {
                continue;
            }
            let (beads, named, wants) = {
                let input = &selection.inputs[&key];
                let beads: Vec<(String, Disposition)> = input
                    .beads
                    .iter()
                    .map(|(bead, (d, _))| (bead.clone(), *d))
                    .collect();
                (beads, input.named.clone(), input.wants_exported)
            };
            for child in children {
                let child_key = (name.clone(), child.file.clone());
                reached
                    .entry(child_key.clone())
                    .or_insert_with(|| child.state.clone());
                if not_pinned(name, &child.file) {
                    skips.skip(
                        SKIP_CHILD_NOT_PINNED,
                        format!("{name}/{} (declared by {parent})", child.file),
                    );
                    continue;
                }
                let excluding = selection
                    .excluded
                    .get(&child_key)
                    .cloned()
                    .unwrap_or_default();
                for (bead, disposition) in &beads {
                    if excluding.contains(bead) {
                        skips.skip(
                            SKIP_CHILD_EXCLUDED,
                            format!("{name}/{} (declared by {parent}) for {bead}", child.file),
                        );
                        continue;
                    }
                    let before = selection.inputs.get(&child_key).map(|input| {
                        (
                            input.beads.get(bead).cloned(),
                            input.named.len(),
                            input.wants_exported,
                        )
                    });
                    let input = selection.add(
                        name,
                        &child.file,
                        bead,
                        *disposition,
                        Via::Declared(parent.clone()),
                    );
                    input.named.extend(named.iter().cloned());
                    input.wants_exported |= wants && child.file.ends_with(".rs");
                    let after = Some((
                        input.beads.get(bead).cloned(),
                        input.named.len(),
                        input.wants_exported,
                    ));
                    if before != after {
                        changed = true;
                    }
                    declared
                        .entry(key.clone())
                        .or_default()
                        .insert(child_key.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(declared)
}

/// Whether a `Cargo.toml` text declares a package (not only a workspace).
fn is_package_manifest(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "[package]")
}

/// Whether a package manifest enables `RESEARCH_CRATE`'s `RESEARCH_FEATURE`: on the dependency
/// itself (`oxfunc_core = { ..., features = ["research-x87"] }` or a `[dependencies.oxfunc_core]`
/// table), or through a feature of its own (`x = ["oxfunc_core/research-x87"]`).
fn enables_research(text: &str) -> bool {
    let quoted = format!("\"{RESEARCH_FEATURE}\"");
    let forwarded = format!("{RESEARCH_CRATE}/{RESEARCH_FEATURE}");
    let dependency_table = format!("dependencies.{RESEARCH_CRATE}");
    let mut table = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            table = line.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        if line.contains(&forwarded) {
            return true;
        }
        let names_crate = line
            .strip_prefix(RESEARCH_CRATE)
            .is_some_and(|rest| rest.starts_with([' ', '=', '.']));
        let on_dependency =
            table.ends_with(&dependency_table) || (table.ends_with("dependencies") && names_crate);
        if on_dependency && line.contains(&quoted) {
            return true;
        }
    }
    false
}

/// Marks quarantine_derived (pending infra-m0.5) every input in a package that enables
/// `RESEARCH_CRATE`'s `RESEARCH_FEATURE`, and every Rust input that names the x87 research surface
/// (`excel_numeric::research`), outside the quarantined directory itself.
fn mark_quarantine_derived(
    repositories: &mut [Repository],
    selection: &mut Selection,
    skips: &mut Skips,
) -> Result<(), String> {
    let mut packages: BTreeMap<String, Vec<(String, String, bool)>> = BTreeMap::new();
    for state in repositories.iter_mut().filter(|r| r.commit.is_some()) {
        let manifests: Vec<String> = state
            .tree
            .keys()
            .filter(|path| *path == "Cargo.toml" || path.ends_with("/Cargo.toml"))
            .filter(|path| !not_pinned(&state.name, path))
            .cloned()
            .collect();
        let mut found = Vec::new();
        for manifest in manifests {
            let text = state.content(&manifest)?;
            if is_package_manifest(&text) {
                let dir = manifest
                    .strip_suffix("Cargo.toml")
                    .unwrap_or_default()
                    .to_owned();
                found.push((dir, manifest, enables_research(&text)));
            }
        }
        packages.insert(state.name.clone(), found);
    }
    let surface = RESEARCH_SURFACE.join("::");
    let keys: Vec<Key> = selection.inputs.keys().cloned().collect();
    for key in keys {
        let (name, path) = &key;
        if quarantined(name, path) {
            continue;
        }
        let package = packages.get(name).and_then(|list| {
            list.iter()
                .filter(|(dir, _, _)| path.starts_with(dir.as_str()))
                .max_by_key(|(dir, _, _)| dir.len())
        });
        let reason = match package {
            Some((_, manifest, true)) => Some(format!(
                "a file of the package {manifest}, which enables {RESEARCH_CRATE}'s {RESEARCH_FEATURE} feature"
            )),
            _ if path.ends_with(".rs") => {
                let text = repository(repositories, name)?.content(path)?;
                let calls = format!(
                    "names {surface}, the x87 entry points compiled only with {RESEARCH_CRATE}'s {RESEARCH_FEATURE}"
                );
                if !text.contains(RESEARCH_SURFACE[0]) {
                    None
                } else {
                    match pin_rust::parse_full(&text) {
                        Ok(file) => pin_rust::names_module_path(
                            &file,
                            RESEARCH_SURFACE[0],
                            RESEARCH_SURFACE[1],
                        )
                        .then_some(calls),
                        Err(e) => {
                            skips.skip(SKIP_RESEARCH_PARSE, format!("{name}/{path}: {e}"));
                            text.contains(&surface).then_some(calls)
                        }
                    }
                }
            }
            _ => None,
        };
        if let Some(reason) = reason
            && let Some(input) = selection.inputs.get_mut(&key)
        {
            input.derived = Some(reason);
            for (disposition, _) in input.beads.values_mut() {
                *disposition = (*disposition).max(Disposition::QuarantineDerived);
            }
        }
    }
    Ok(())
}

/// Records the runner markers of every input reached through a source-discovery path, markdown
/// files aside.
fn mark_runner_candidates(
    repositories: &mut [Repository],
    selection: &mut Selection,
) -> Result<(), String> {
    for ((name, path), input) in selection.inputs.iter_mut() {
        let discovered = input.via.iter().any(|v| matches!(v, Via::Discovered(_)));
        if !discovered || path.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        let text = repository(repositories, name)?.content(path)?;
        input.runner_markers = RUNNER_MARKERS
            .iter()
            .filter(|marker| text.contains(**marker))
            .map(|marker| (*marker).to_owned())
            .collect();
    }
    Ok(())
}

/// The plan audit compared with the pin: a summary, and every audited blob of a legacy repository
/// that differs from the blob of the same path at the pinned commit.
fn plan_audit_drift(
    repositories: &[Repository],
    audit: &[AuditEntry],
    selection: &Selection,
) -> (Value, Vec<Value>) {
    let mut changed = Vec::new();
    let mut unchanged = 0;
    let mut not_legacy = Vec::new();
    for entry in audit {
        let (name, path) = entry.path.split_once('/').unwrap_or((&entry.path, ""));
        let Some(state) = repositories.iter().find(|r| r.name == name) else {
            not_legacy.push(entry.path.clone());
            continue;
        };
        let pinned = state.tree.get(path);
        if pinned == Some(&entry.blob) {
            unchanged += 1;
        } else {
            changed.push(json!({
                "path": entry.path,
                "audit_blob": entry.blob,
                "pinned_blob": pinned,
                "commit": state.commit,
                "input": selection.inputs.contains_key(&(name.to_owned(), path.to_owned())),
            }));
        }
    }
    let summary = json!({
        "file": PLAN_AUDIT,
        "compared": audit.len() - not_legacy.len(),
        "unchanged": unchanged,
        "changed": changed.len(),
        "not_a_legacy_repository": not_legacy,
    });
    (summary, changed)
}

/// One family bead's list: the catalogue entries it names exactly, and the terms that are not a
/// catalogue name (a descriptive family such as `IS families`), which this pin does not resolve.
fn family_record(bead: &str, do_text: &str, functions: &[Value]) -> Value {
    let names: BTreeMap<&str, &str> = functions
        .iter()
        .filter_map(|f| Some((f["name"].as_str()?, f["id"].as_str()?)))
        .collect();
    let Some(items) = pin_cite::family_items(do_text) else {
        return json!({
            "bead": bead,
            "functions": [],
            "unresolved_terms": ["the Do names no family list"],
        });
    };
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for item in items {
        match names.get(item.as_str()) {
            Some(id) => resolved.push((*id).to_owned()),
            None => unresolved.push(item),
        }
    }
    json!({"bead": bead, "functions": resolved, "unresolved_terms": unresolved})
}

fn category(repository: &str, path: &str, via: &BTreeSet<Via>) -> &'static str {
    let file = path.rsplit('/').next().unwrap_or_default();
    let extension = file
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    let in_corpus_dir = path
        .split('/')
        .rev()
        .skip(1)
        .any(|part| CORPUS_COMPONENTS.contains(&part));
    if in_corpus_dir || CORPUS_EXTENSIONS.contains(&extension.as_str()) {
        return "corpus";
    }
    if via.iter().any(|v| matches!(v, Via::Discovered(_))) {
        return "oracle-runner";
    }
    if extension == "md" {
        return "other";
    }
    let parser_test = via
        .iter()
        .any(|v| matches!(v, Via::DirectoryTest(dir) if under(&PARSER_DIRS, repository, dir)));
    if parser_test || under(&PARSER_DIRS, repository, path) {
        return "parser";
    }
    let catalogue = via
        .iter()
        .any(|v| matches!(v, Via::Catalogue | Via::Helper | Via::CatalogueTest));
    if catalogue || under(&FUNCTION_FAMILY_DIRS, repository, path) {
        return "function-family";
    }
    if under(&ORACLE_RUNNER_DIRS, repository, path) {
        return "oracle-runner";
    }
    "other"
}

fn rules() -> Value {
    json!({
        "source": "the Port from section of every bead in the tracker; a cited directory expands to its committed files",
        "pin_date": "the UTC date on which this pin content was first written; a run that reproduces the content keeps it, and no other time is recorded",
        "pinned_commit": "each repository's HEAD at pin time, checked to be a commit object; every blob is read from that commit's tree, never from a working tree",
        "working_tree": format!("git status --porcelain with -c {}: only the repository's own ignore rules apply, line endings compare as the Windows checkout wrote them, and unchanged size and time skip a re-read. Reported, not pinned: a blob comes from the commit object", crate::pin_git::STATUS_CONFIG.join(" -c ")),
        "license_status": "found when the pinned tree has a root file whose name starts with license, licence, copying or unlicense (case-insensitive); otherwise none-found; never inferred. summary.license_none_found counts the inputs of repositories with none-found",
        "verification": "provisional for every input: the blob is pinned at a committed object, and nothing here compiled, audited or license-cleared it; bead dc-4r4l's compile audition owns any change of that state",
        "dispositions": {
            "port": "may be copied after review",
            "reference": format!("read for behaviour only: the citing clause or the text after the last path holds one of {:?}, or the token's own sentence names it a {:?}", pin_cite::REFERENCE_MARKERS, pin_cite::DISCOVERY_MARKER),
            "quarantined": format!("not a port source: the citing clause holds {}, or the path is under {:?} (pending infra-m0.5)", pin_cite::QUARANTINE_MARKER, QUARANTINED),
            "quarantine_derived": format!("not a port source and out of every port count (pending infra-m0.5): a file of a package whose Cargo.toml enables {RESEARCH_CRATE}'s {RESEARCH_FEATURE} feature (on the dependency or through a feature of its own), or a Rust file whose code names {}, the route to the quarantined x87 entry points; catalogue files that call the crate-internal excel_numeric dispatcher keep their disposition and list it under quarantined_helpers", RESEARCH_SURFACE.join("::")),
            "input": "quarantined when any citing bead reads it so, then quarantine_derived, then port when any citing bead may port it, otherwise reference. When the citing beads differ in disposition or in how they reached the input, routes lists each group of beads with its own disposition and via",
        },
        "discovery": format!("a token whose own sentence names it a {:?} is read for behaviour only: its files are inputs with disposition reference (unless quarantined or quarantine_derived) and category oracle-runner (corpus files stay corpus)", pin_cite::DISCOVERY_MARKER),
        "runner_candidate": format!("a non-markdown input reached through a source-discovery path whose blob holds one of {RUNNER_MARKERS:?}; {RUNNER_SELECTION}"),
        "unavailable": "cited but absent at the pinned commit, or not resolvable to a repository; never an input and never counted among port inputs",
        "excluded": format!("cited in a sentence that says {:?} before the path; recorded with its blob, never an input, and never pinned as a declared module for the excluding beads", pin_cite::EXCLUDE_MARKER),
        "not_pinned": "the not_pinned list: paths under these prefixes are never inputs, committed or not; --lookup reports them as bulk evidence with their owner and archive",
        "symbols": "code-like words (holding :: or _, or camel-case) of a path's clause and of the text after the last path, plus capitalised single words listed after <path>:; an input (and its declared modules) lists those its blob holds as whole words",
        "exported": "for the .rs inputs of a Port from text that asks for symbols, and their declared modules: the pub items of the blob (and of its pub inline modules), test-only items removed",
        "catalogue": "a clause that holds 'catalogue entries' for OxFunc's surface_dispatch.rs: each FUNC_ID constant's modules from its arg_preparation_profile arm (or its imported eval_<name>_surface), their files in the crate's module tree, the files those use through crate, self, super, sibling-crate and child-module paths and the files they declare (not through the dispatch file), every test of the crate (attributed to the IDs it names) and the files sources include literally or name; all pinned for every citing bead",
        "module_tree": "a library crate's modules are walked from src/lib.rs through its mod declarations as rustc reads them: #[path] relative to the declaring scope's directory (a #[path] file reads its own children as a mod.rs does), otherwise <name>.rs or <name>/mod.rs beside a mod.rs or crate root, or under <name>/ beside a non-mod-rs file; inline modules add their name",
        "declared_modules": "every module a pinned Rust input declares (mod x;, with or without #[path], test-only included) is pinned for the parent's beads with the parent's disposition for each, transitively; via names the parent",
        "resolution_skips": "a file a resolution cannot read (a parse failure, a file in no crate or outside its crate's module tree, a non-literal include macro, an absent, ambiguous or cfg_attr module file) is counted per reason in notes and summary.resolution_skipped; counted fallbacks are in summary.resolution_fallbacks",
        "families": "the Do list after 'Port the named family only:'; an item that equals a catalogue name resolves to its ID, any other item stays an unresolved term (bead dc-xz2t owns them)",
        "directory_tests": "for a Port from text that asks for test files: the tests of the cited directory's crate that name its module (<module>::), with the files they include or name",
        "plan_audit": format!("every blob of {PLAN_AUDIT} sources compared with the blob of the same path at the pinned commit; changed_since_plan_audit lists each that differs"),
        "categories": {
            "corpus": format!("a path component in {CORPUS_COMPONENTS:?} or an extension in {CORPUS_EXTENSIONS:?}"),
            "oracle-runner": format!("files reached through a source-discovery path, and files under {ORACLE_RUNNER_DIRS:?}"),
            "other": "markdown files outside the corpus and the discovery paths, and every file no other category names",
            "order": "corpus first, then source-discovery files as oracle-runner, then markdown files as other, then parser, function-family and oracle-runner",
            "parser": format!("files under {PARSER_DIRS:?} and tests resolved for them"),
            "function-family": format!("files resolved from the catalogue, and files under {FUNCTION_FAMILY_DIRS:?}"),
        },
    })
}

/// What `assemble` turns into the manifest.
struct Parts<'a> {
    repositories: &'a [Repository],
    citations: &'a BTreeMap<String, Citation>,
    selection: &'a Selection,
    functions: Vec<Value>,
    families: Vec<Value>,
    notes: Vec<String>,
    skips: &'a Skips,
    plan_audit: Value,
    changed: Vec<Value>,
}

fn input_disposition(input: &Input) -> Disposition {
    let any = |wanted: Disposition| input.beads.values().any(|(d, _)| *d == wanted);
    if any(Disposition::Quarantined) {
        Disposition::Quarantined
    } else if any(Disposition::QuarantineDerived) {
        Disposition::QuarantineDerived
    } else if any(Disposition::Port) {
        Disposition::Port
    } else {
        Disposition::Reference
    }
}

fn assemble(parts: Parts<'_>) -> Value {
    let Parts {
        repositories,
        citations,
        selection,
        functions,
        families,
        notes,
        skips,
        plan_audit,
        changed,
    } = parts;
    let repository_of = |name: &str| repositories.iter().find(|r| r.name == name);
    let mut by_disposition: BTreeMap<&str, usize> =
        ["port", "reference", "quarantine_derived", "quarantined"]
            .into_iter()
            .map(|d| (d, 0))
            .collect();
    let mut by_category: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_repository: BTreeMap<&str, usize> = BTreeMap::new();
    let mut license_none_found = 0;
    let mut port_license_none_found = 0;
    let mut runner_candidates = 0;
    let mut declared_inputs = 0;
    let mut inputs = Vec::new();
    for ((repository, path), input) in &selection.inputs {
        let state = repository_of(repository);
        let disposition = input_disposition(input);
        let category = category(repository, path, &input.via);
        let license = state.map_or("none-found", |s| s.license_status);
        *by_disposition.entry(disposition.as_str()).or_default() += 1;
        *by_category.entry(category).or_default() += 1;
        *per_repository.entry(repository).or_default() += 1;
        if license == "none-found" {
            license_none_found += 1;
            if disposition == Disposition::Port {
                port_license_none_found += 1;
            }
        }
        if input.via.iter().any(|v| matches!(v, Via::Declared(_))) {
            declared_inputs += 1;
        }
        let mut record = json!({
            "repository": repository,
            "path": path,
            "commit": state.and_then(|s| s.commit.clone()),
            "blob": state.and_then(|s| s.tree.get(path).cloned()),
            "license_status": license,
            "disposition": disposition.as_str(),
            "verification": "provisional",
            "category": category,
            "beads": input.beads.keys().collect::<Vec<_>>(),
            "via": input.via.iter().map(Via::text).collect::<Vec<_>>(),
        });
        let mut routes: BTreeMap<(Disposition, &BTreeSet<Via>), Vec<&String>> = BTreeMap::new();
        for (bead, (d, via)) in &input.beads {
            routes.entry((*d, via)).or_default().push(bead);
        }
        if routes.len() > 1 {
            record["routes"] = Value::Array(
                routes
                    .iter()
                    .map(|((d, via), beads)| {
                        json!({
                            "beads": beads,
                            "disposition": d.as_str(),
                            "via": via.iter().map(Via::text).collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            );
        }
        if let Some(reason) = &input.derived {
            record["quarantine_derived"] = json!(reason);
        }
        if !input.runner_markers.is_empty() {
            runner_candidates += 1;
            record["runner_candidate"] = json!({
                "markers": input.runner_markers,
                "selection": RUNNER_SELECTION,
            });
        }
        if !input.symbols.is_empty() {
            record["symbols"] = json!(input.symbols);
        }
        match &input.exported {
            Some(Ok(names)) => record["exported"] = json!(names),
            Some(Err(e)) => record["exported_error"] = json!(e),
            None => {}
        }
        if let Some(uses) = &input.uses {
            record["uses"] = json!(uses);
        }
        inputs.push(record);
    }
    let unavailable: Vec<Value> = selection
        .unavailable
        .iter()
        .map(|((repository, path), (beads, reason))| {
            json!({
                "repository": if repository.is_empty() { Value::Null } else { json!(repository) },
                "path": path,
                "beads": beads,
                "reason": reason,
            })
        })
        .collect();
    let excluded: Vec<Value> = selection
        .excluded
        .iter()
        .map(|((repository, path), beads)| {
            let state = repository_of(repository);
            json!({
                "repository": repository,
                "path": path,
                "commit": state.and_then(|s| s.commit.clone()),
                "blob": state.and_then(|s| s.tree.get(path).cloned()),
                "beads": beads,
                "reason": "the Port from text says not to port it",
            })
        })
        .collect();
    let citation_records: Vec<Value> = citations
        .values()
        .map(|citation| {
            let named: BTreeSet<&String> = citation
                .port_from
                .cited
                .iter()
                .flat_map(|c| c.symbols.iter())
                .collect();
            let found: BTreeSet<&String> = citation
                .resolved
                .iter()
                .filter_map(|key| selection.inputs.get(key))
                .flat_map(|input| input.symbols.iter())
                .collect();
            json!({
                "beads": citation.beads,
                "paths": citation.port_from.cited.iter().map(|c| c.token.clone()).collect::<Vec<_>>(),
                "files": citation.resolved.len(),
                "symbols_named": named,
                "symbols_not_found": named.difference(&found).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut citing_beads = BTreeSet::new();
    for citation in citations.values() {
        citing_beads.extend(citation.beads.iter().cloned());
    }
    let repository_records: Vec<Value> = repositories
        .iter()
        .map(|state| {
            let mut record = state.record.clone();
            record.insert(
                "inputs".to_owned(),
                json!(
                    per_repository
                        .get(state.name.as_str())
                        .copied()
                        .unwrap_or(0)
                ),
            );
            Value::Object(record)
        })
        .collect();
    let not_pinned: Vec<Value> = NOT_PINNED
        .iter()
        .map(|np| {
            json!({
                "repository": np.repository,
                "path": np.prefix,
                "reason": np.reason,
                "owner": np.owner,
                "archive": np.archive,
            })
        })
        .collect();
    let present = repositories.iter().filter(|r| r.commit.is_some()).count();
    let with = |field: &str| {
        repositories
            .iter()
            .filter(|r| {
                r.record
                    .get("working_tree")
                    .and_then(|tree| tree[field].as_u64())
                    .unwrap_or(0)
                    > 0
            })
            .count()
    };
    let unresolved_terms: usize = families
        .iter()
        .map(|f| f["unresolved_terms"].as_array().map_or(0, Vec::len))
        .sum();
    let resolved_names: usize = families
        .iter()
        .map(|f| f["functions"].as_array().map_or(0, Vec::len))
        .sum();
    let without_module = functions
        .iter()
        .filter(|f| f["files"].as_array().is_none_or(Vec::is_empty))
        .count();
    json!({
        "format": FORMAT,
        "written_by": "cargo xtask pin-sources",
        "pin_date": crate::accept::utc_now()[..10].to_owned(),
        "tracker": Value::Null,
        "rules": rules(),
        "summary": {
            "repositories": repositories.len(),
            "repositories_present": present,
            "repositories_with_tracked_changes": with("tracked_changes"),
            "repositories_with_untracked_entries": with("untracked"),
            "repositories_not_on_origin": repositories.iter().filter(|r| r.record.get("on_origin") == Some(&json!(false))).map(|r| r.name.clone()).collect::<Vec<_>>(),
            "citing_beads": citing_beads.len(),
            "inputs": selection.inputs.len(),
            "verification": {"provisional": selection.inputs.len()},
            "by_disposition": by_disposition,
            "port_provisional": by_disposition["port"],
            "license_none_found": license_none_found,
            "port_provisional_license_none_found": port_license_none_found,
            "by_category": by_category,
            "runner_candidates": runner_candidates,
            "declared_module_inputs": declared_inputs,
            "unavailable": selection.unavailable.len(),
            "excluded": selection.excluded.len(),
            "catalogue_functions": functions.len(),
            "catalogue_functions_without_file": without_module,
            "family_beads": families.len(),
            "family_names_resolved": resolved_names,
            "family_terms_unresolved": unresolved_terms,
            "resolution_skipped": Skips::counts(&skips.skipped),
            "resolution_fallbacks": Skips::counts(&skips.fallbacks),
            "changed_since_plan_audit": changed.len(),
        },
        "notes": notes,
        "repositories": repository_records,
        "not_pinned": not_pinned,
        "plan_audit": plan_audit,
        "changed_since_plan_audit": changed,
        "citations": citation_records,
        "families": families,
        "functions": functions,
        "unavailable": unavailable,
        "excluded": excluded,
        "inputs": inputs,
    })
}

/// The arrays written one element per line; every other value is written indented.
const LINE_ARRAYS: [&str; 7] = [
    "changed_since_plan_audit",
    "citations",
    "families",
    "functions",
    "unavailable",
    "excluded",
    "inputs",
];

/// The manifest as text: the top-level keys in a fixed order, the large arrays one compact
/// element per line so that a re-pin shows as a line diff.
fn manifest_text(manifest: &Value) -> String {
    const ORDER: [&str; 17] = [
        "format",
        "written_by",
        "pin_date",
        "tracker",
        "rules",
        "summary",
        "notes",
        "repositories",
        "not_pinned",
        "plan_audit",
        "changed_since_plan_audit",
        "citations",
        "families",
        "functions",
        "unavailable",
        "excluded",
        "inputs",
    ];
    let object = manifest.as_object().cloned().unwrap_or_default();
    let mut out = String::from("{\n");
    for (index, key) in ORDER.iter().enumerate() {
        let value = object.get(*key).cloned().unwrap_or(Value::Null);
        out.push_str(&format!("  {}: ", json!(key)));
        match (&value, LINE_ARRAYS.contains(key)) {
            (Value::Array(items), true) if !items.is_empty() => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    out.push_str("    ");
                    out.push_str(&item.to_string());
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                out.push_str("  ]");
            }
            _ => {
                let pretty = serde_json::to_string_pretty(&value).unwrap_or_default();
                out.push_str(&pretty.replace('\n', "\n  "));
            }
        }
        out.push_str(if index + 1 < ORDER.len() { ",\n" } else { "\n" });
    }
    out.push_str("}\n");
    out
}

/// `--lookup`: prints the pinned entry for `<Repo>/<path>`, every pinned entry under a directory,
/// or why there is none, and exits with the code `USAGE` lists.
fn lookup(ws: &Workspace, manifest_path: &Path, wanted: &str) -> ExitCode {
    let shown = ws.rel(manifest_path);
    let manifest: Value = match fs::read_to_string(manifest_path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("xtask pin-sources: cannot read the manifest {shown}: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let (text, code) = lookup_text(&manifest, &shown, wanted);
    print!("{text}");
    ExitCode::from(code)
}

/// The lookup's output and exit code for `wanted` in `manifest` (shown as `shown`).
fn lookup_text(manifest: &Value, shown: &str, wanted: &str) -> (String, u8) {
    let wanted = wanted.replace('\\', "/");
    let (repository, path) = wanted.split_once('/').unwrap_or((wanted.as_str(), ""));
    let mut out = String::new();
    for np in manifest["not_pinned"].as_array().into_iter().flatten() {
        let prefix = np["path"].as_str().unwrap_or_default();
        let matches = np["repository"] == json!(repository)
            && !prefix.is_empty()
            && (path == prefix.trim_end_matches('/') || path.starts_with(prefix));
        if matches {
            out.push_str(&format!(
                "not pinned: {} ({}, {}): {wanted} is under {repository}/{prefix}, which no pin records\n",
                np["reason"].as_str().unwrap_or("?"),
                np["owner"].as_str().unwrap_or("?"),
                np["archive"].as_str().unwrap_or("?"),
            ));
            return (out, EXIT_BULK_EVIDENCE);
        }
    }
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let exact: Vec<&Value> = inputs
        .iter()
        .filter(|e| e["repository"] == json!(repository) && e["path"] == json!(path))
        .collect();
    let found: Vec<&Value> = if exact.is_empty() && !path.is_empty() {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        inputs
            .iter()
            .filter(|e| {
                e["repository"] == json!(repository)
                    && e["path"].as_str().is_some_and(|p| p.starts_with(&prefix))
            })
            .collect()
    } else {
        exact
    };
    if !found.is_empty() {
        let mut code = 0;
        for entry in found {
            out.push_str(&entry_text(entry, shown, &manifest["pin_date"]));
            let disposition = entry["disposition"].as_str().unwrap_or_default();
            if matches!(disposition, "quarantined" | "quarantine_derived") {
                out.push_str(&format!("{disposition}: do not copy (infra-m0.5)\n"));
                code = EXIT_QUARANTINED;
            }
        }
        return (out, code);
    }
    let listed = |section: &str| {
        manifest[section]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["repository"] == json!(repository) && e["path"] == json!(path))
            .cloned()
    };
    if let Some(entry) = listed("unavailable") {
        out.push_str(&format!(
            "unavailable {wanted}: cited by {} but {}\n",
            list_text(&entry["beads"]),
            entry["reason"].as_str().unwrap_or_default()
        ));
    } else if let Some(entry) = listed("excluded") {
        out.push_str(&format!(
            "unavailable {wanted}: excluded at commit {} (blob {}); the Port from text of {} says not to port it\n",
            entry["commit"].as_str().unwrap_or("?"),
            entry["blob"].as_str().unwrap_or("absent"),
            list_text(&entry["beads"])
        ));
    } else {
        out.push_str(&format!(
            "unavailable {wanted}: not a pinned input of {shown}; implement it fresh and report it\n"
        ));
    }
    (out, EXIT_UNAVAILABLE)
}

fn list_text(value: &Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn entry_text(entry: &Value, manifest: &str, pin_date: &Value) -> String {
    let text = |field: &str| entry[field].as_str().unwrap_or("?").to_owned();
    let mut out = format!("pinned {}/{}\n", text("repository"), text("path"));
    out.push_str(&format!("  commit {}\n", text("commit")));
    out.push_str(&format!("  blob {}\n", text("blob")));
    out.push_str(&format!("  disposition {}\n", text("disposition")));
    out.push_str(&format!("  verification {}\n", text("verification")));
    for route in entry["routes"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "  route {} for {} via {}\n",
            route["disposition"].as_str().unwrap_or("?"),
            list_text(&route["beads"]),
            list_text(&route["via"])
        ));
    }
    if let Some(reason) = entry["quarantine_derived"].as_str() {
        out.push_str(&format!("  quarantine_derived because it is {reason}\n"));
    }
    if entry["runner_candidate"].is_object() {
        out.push_str(&format!(
            "  runner candidate ({}): {}\n",
            list_text(&entry["runner_candidate"]["markers"]),
            entry["runner_candidate"]["selection"]
                .as_str()
                .unwrap_or_default()
        ));
    }
    out.push_str(&format!("  category {}\n", text("category")));
    out.push_str(&format!("  license {}\n", text("license_status")));
    out.push_str(&format!("  beads {}\n", list_text(&entry["beads"])));
    out.push_str(&format!("  via {}\n", list_text(&entry["via"])));
    if entry["symbols"].is_array() {
        out.push_str(&format!("  symbols {}\n", list_text(&entry["symbols"])));
    }
    out.push_str(&format!(
        "  manifest {manifest} (pin date {})\n",
        pin_date.as_str().unwrap_or("?")
    ));
    out
}

#[cfg(test)]
impl Repository {
    /// An in-memory repository for the unit tests: each file's blob ID is `blob:<path>`.
    fn fixture(name: &str, files: &[(&str, &str)]) -> Repository {
        let tree = files
            .iter()
            .map(|(path, _)| ((*path).to_owned(), format!("blob:{path}")))
            .collect();
        let contents = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();
        let mut record = Map::new();
        record.insert("name".to_owned(), json!(name));
        record.insert("present".to_owned(), json!(true));
        record.insert("commit".to_owned(), json!("fixture-commit"));
        record.insert(
            "license".to_owned(),
            json!({"status": "found", "files": []}),
        );
        Repository {
            name: name.to_owned(),
            legacy: Legacy::new(PathBuf::new()),
            record,
            commit: Some("fixture-commit".to_owned()),
            tree,
            license_status: "found",
            reader: None,
            contents,
            children: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_count_only_as_whole_words() {
        assert!(contains_symbol(
            "pub struct GridAxisValueDependency {",
            "GridAxisValueDependency"
        ));
        assert!(!contains_symbol(
            "GridAxisValueDependencyKind",
            "GridAxisValueDependency"
        ));
        assert!(contains_symbol(
            "enum WriteMode { FreshExport }",
            "WriteMode::FreshExport"
        ));
        assert!(!contains_symbol(
            "enum WriteMode { Other }",
            "WriteMode::FreshExport"
        ));
    }

    #[test]
    fn categories_follow_the_listed_rules() {
        let none = BTreeSet::new();
        let catalogue = BTreeSet::from([Via::Catalogue]);
        let discovered = BTreeSet::from([Via::Discovered("smart-fuzzer/".to_owned())]);
        assert_eq!(
            category("OxFml", "crates/oxfml_core/src/syntax/parser.rs", &none),
            "parser"
        );
        assert_eq!(
            category(
                "OxFunc",
                "crates/oxfunc_core/tests/fixtures/a.json",
                &catalogue
            ),
            "corpus"
        );
        assert_eq!(
            category("OxFunc", "crates/oxfunc_core/src/value.rs", &catalogue),
            "function-family"
        );
        assert_eq!(
            category("OxFunc", "smart-fuzzer/engine/src/judge.rs", &none),
            "oracle-runner"
        );
        assert_eq!(category("OxFunc", "smart-fuzzer/README.md", &none), "other");
        assert_eq!(
            category("OxFunc", "smart-fuzzer/README.md", &discovered),
            "oracle-runner"
        );
        assert_eq!(
            category("OxFunc", "smart-fuzzer/corpus/a.tsv", &discovered),
            "corpus"
        );
        assert_eq!(
            category(
                "OxFunc",
                "crates/oxfunc_core/src/excel_numeric/x.tsv",
                &none
            ),
            "corpus"
        );
        assert!(quarantined(
            "OxFunc",
            "crates/oxfunc_core/src/excel_numeric/x87.rs"
        ));
        assert!(not_pinned("OxFunc", "smart-fuzzer/runs/a.json"));
    }

    #[test]
    fn manifest_text_is_stable_and_parses_back() {
        let manifest = json!({
            "format": FORMAT,
            "summary": {"inputs": 1},
            "inputs": [{"repository": "OxFml", "path": "a.rs"}, {"repository": "OxFml", "path": "b.rs"}],
            "excluded": [],
        });
        let text = manifest_text(&manifest);
        assert!(text.contains("\n    {\"path\":\"a.rs\",\"repository\":\"OxFml\"},\n"));
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["inputs"], manifest["inputs"]);
        assert_eq!(back["excluded"], json!([]));
        assert_eq!(text, manifest_text(&back));
    }

    #[test]
    fn arguments_are_checked() {
        let args =
            |list: &[&str]| parse_args(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert!(args(&[]).is_ok());
        assert!(args(&["--lookup", "OxFml/a.rs", "--manifest", "m.json"]).is_ok());
        assert!(args(&["--lookup", "OxFml/a.rs", "--out", "m.json"]).is_err());
        assert!(args(&["--manifest", "m.json"]).is_err());
        assert!(args(&["--tracker"]).is_err());
        assert!(args(&["--frobnicate"]).is_err());
    }

    #[test]
    fn the_legacy_root_is_never_shown_as_an_absolute_path() {
        let workspace = std::env::current_dir().unwrap();
        let parent = workspace.parent().unwrap();
        assert_eq!(shown_legacy_root(&workspace, parent), "..");
        assert_eq!(
            shown_legacy_root(&workspace, &workspace.join("legacy")),
            "legacy"
        );
        let elsewhere = Path::new("/no/such/legacy/root/anywhere");
        assert_eq!(shown_legacy_root(&workspace, elsewhere), "<legacy-root>");
    }

    #[test]
    fn research_manifests_are_recognised() {
        assert!(enables_research(
            "[package]\nname = \"racer\"\n[dependencies]\noxfunc_core = { path = \"../x\", features = [\"research-x87\"] }\n"
        ));
        assert!(enables_research(
            "[package]\nname = \"p\"\n[features]\ndepreciation-research = [\"oxfunc_core/research-x87\"]\n"
        ));
        assert!(enables_research(
            "[package]\nname = \"p\"\n[dependencies.oxfunc_core]\npath = \"../x\"\nfeatures = [\n  \"research-x87\",\n]\n"
        ));
        // The feature's own definition, a plain dependency and a comment do not enable it.
        assert!(!enables_research(
            "[package]\nname = \"oxfunc_core\"\n[features]\nresearch-x87 = []\n"
        ));
        assert!(!enables_research(
            "[package]\nname = \"engine\"\n[dependencies]\noxfunc_core = { path = \"../x\" }\n# features = [\"research-x87\"]\n"
        ));
    }

    // Fixture-backed tests of the generator: a fixture tracker and fixture repositories run
    // through `build`, the route `cargo xtask pin-sources` takes after reading the real ones.

    fn bead(id: &str, port_from: &str) -> Bead {
        bead_with_do(id, "Nothing.", port_from)
    }

    fn bead_with_do(id: &str, do_text: &str, port_from: &str) -> Bead {
        Bead {
            id: id.to_owned(),
            description: format!(
                "## Outcome\nA fixture.\n\n## Do\n{do_text}\n\n## Port from\n{port_from}\n\n## Acceptance\nNone.\n"
            ),
        }
    }

    const OXFUNC: [(&str, &str); 22] = [
        (
            "crates/oxfunc_core/Cargo.toml",
            "[package]\nname = \"oxfunc_core\"\n\n[features]\nresearch-x87 = []\n",
        ),
        (
            "crates/oxfunc_core/src/lib.rs",
            "pub mod coercion;\npub mod functions;\nmod excel_numeric;\n",
        ),
        (
            "crates/oxfunc_core/src/coercion.rs",
            "#[path = \"coercion_decimal.rs\"]\nmod decimal;\npub fn coerce() -> u8 { decimal::parse() }\n",
        ),
        (
            "crates/oxfunc_core/src/coercion_decimal.rs",
            "#[path = \"coercion_decimal_powers.rs\"]\nmod powers;\nmod sibling;\npub fn parse() -> u8 { powers::p() }\n",
        ),
        (
            "crates/oxfunc_core/src/coercion_decimal_powers.rs",
            "pub fn p() -> u8 { 1 }\n",
        ),
        ("crates/oxfunc_core/src/sibling.rs", "pub fn s() {}\n"),
        (
            "crates/oxfunc_core/src/coercion/decimal.rs",
            "// A decoy: no declaration names this file.\n",
        ),
        (
            "crates/oxfunc_core/src/functions/mod.rs",
            "pub mod abs;\npub mod surface_dispatch;\n",
        ),
        (
            "crates/oxfunc_core/src/functions/abs.rs",
            "use crate::coercion::coerce;\npub struct Meta { pub arg_preparation_profile: u8 }\npub const ABS_META: Meta = Meta { arg_preparation_profile: 0 };\npub fn abs() -> u8 { coerce() }\n",
        ),
        (
            "crates/oxfunc_core/src/functions/surface_dispatch.rs",
            "pub const FUNC_ID_ABS: &str = \"FUNC.ABS\";\npub fn arg_preparation_profile(function_id: &str) -> Option<u8> {\n    match function_id {\n        FUNC_ID_ABS => Some(crate::functions::abs::ABS_META.arg_preparation_profile),\n        _ => None,\n    }\n}\n",
        ),
        (
            "crates/oxfunc_core/src/excel_numeric/mod.rs",
            "#[cfg(target_arch = \"x86_64\")]\nmod x87;\n",
        ),
        (
            "crates/oxfunc_core/src/excel_numeric/x87.rs",
            "pub fn exp() {}\n",
        ),
        (
            "smart-fuzzer/README.md",
            "The runners drive Excel.Application through COM.\n",
        ),
        (
            "smart-fuzzer/tools/Run-Capture.ps1",
            "$excel = New-Object -ComObject Excel.Application\n[System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)\n",
        ),
        ("smart-fuzzer/tools/Build-Plan.ps1", "Write-Output 'plan'\n"),
        (
            "smart-fuzzer/tools/racer/Cargo.toml",
            "[package]\nname = \"racer\"\n\n[dependencies]\noxfunc_core = { path = \"../../../crates/oxfunc_core\", features = [\"research-x87\"] }\n",
        ),
        ("smart-fuzzer/tools/racer/src/main.rs", "fn main() {}\n"),
        (
            "smart-fuzzer/tools/probe/Cargo.toml",
            "[package]\nname = \"probe\"\n\n[dependencies]\noxfunc_core = { path = \"../../../crates/oxfunc_core\" }\n",
        ),
        (
            "smart-fuzzer/tools/probe/src/main.rs",
            "use oxfunc_core::excel_numeric::research as rx;\nfn main() { rx::excel_exp(1.0); }\n",
        ),
        (
            "smart-fuzzer/tools/probe/src/bin/grouped.rs",
            "use oxfunc_core::excel_numeric::{research, Other};\nfn main() { research::excel_ln(1.0); }\n",
        ),
        (
            "smart-fuzzer/tools/probe/src/notes.rs",
            "/// Mentions excel_numeric::research only in a comment.\npub fn f() -> &'static str { \"excel_numeric::research\" }\n",
        ),
        ("smart-fuzzer/runs/run-1.json", "{}\n"),
    ];

    const OXCALC: [(&str, &str); 8] = [
        (
            "src/oxcalc-core/src/grid/reference_engine.rs",
            "mod parse;\nmod transform;\npub struct StrictExcelGridReferenceProfile;\n",
        ),
        (
            "src/oxcalc-core/src/grid/reference_engine/parse.rs",
            "pub fn parse() {}\n",
        ),
        (
            "src/oxcalc-core/src/grid/reference_engine/transform.rs",
            "#[cfg(test)]\nmod tests;\n",
        ),
        (
            "src/oxcalc-core/src/grid/reference_engine/transform/tests.rs",
            "#[test]\nfn t() {}\n",
        ),
        (
            "src/oxcalc-core/src/grid/machine.rs",
            "mod r1c1_plan;\nmod differential;\n",
        ),
        (
            "src/oxcalc-core/src/grid/machine/r1c1_plan.rs",
            "pub fn evaluate() {}\n",
        ),
        (
            "src/oxcalc-core/src/grid/machine/differential.rs",
            "pub struct GridSeededLaneOutcome;\n",
        ),
        ("src/oxcalc-core/src/consumer.rs", "pub struct Consumer;\n"),
    ];

    fn fixture_manifest(beads: &[Bead]) -> Value {
        let mut repositories = vec![
            Repository::fixture("OxFunc", &OXFUNC),
            Repository::fixture("OxCalc", &OXCALC),
        ];
        let audit = [
            AuditEntry {
                path: "OxCalc/src/oxcalc-core/src/consumer.rs".to_owned(),
                blob: "blob:src/oxcalc-core/src/consumer.rs".to_owned(),
            },
            AuditEntry {
                path: "OxCalc/src/oxcalc-core/src/grid/reference_engine.rs".to_owned(),
                blob: "an-older-blob".to_owned(),
            },
            AuditEntry {
                path: "Elsewhere/a.rs".to_owned(),
                blob: "x".to_owned(),
            },
        ];
        build(beads, &mut repositories, &audit).unwrap()
    }

    /// Each input as `Repo/path` with its disposition.
    fn dispositions(manifest: &Value) -> BTreeMap<String, String> {
        manifest["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    format!(
                        "{}/{}",
                        e["repository"].as_str().unwrap(),
                        e["path"].as_str().unwrap()
                    ),
                    e["disposition"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    fn input<'a>(manifest: &'a Value, key: &str) -> &'a Value {
        manifest["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| {
                format!(
                    "{}/{}",
                    e["repository"].as_str().unwrap(),
                    e["path"].as_str().unwrap()
                ) == key
            })
            .unwrap_or_else(|| panic!("{key} is not an input"))
    }

    fn owned(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// A plain citation is a port input, provisional, and counted only as port_provisional; the
    /// manifest calls no input verified.
    #[test]
    fn fixture_classifier_port() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.1",
            "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome.",
        )]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[(
                "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs",
                "port"
            )])
        );
        let entry = input(
            &manifest,
            "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs",
        );
        assert_eq!(entry["verification"], "provisional");
        assert_eq!(entry["symbols"], json!(["GridSeededLaneOutcome"]));
        assert_eq!(manifest["summary"]["port_provisional"], 1);
        assert_eq!(manifest["summary"]["verification"]["provisional"], 1);
        assert!(
            !manifest_text(&manifest)
                .to_ascii_lowercase()
                .contains("verified")
        );
    }

    /// A reference marker makes the input a reference, out of the port count.
    #[test]
    fn fixture_classifier_reference() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.2",
            "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome as the \
             behavioural reference.",
        )]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[(
                "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs",
                "reference"
            )])
        );
        assert_eq!(manifest["summary"]["port_provisional"], 0);
    }

    /// A source-discovery path pins its files as references in the oracle-runner category (never
    /// port), marks the files that drive Excel as runner candidates, and leaves the never-pinned
    /// bulk evidence out.
    #[test]
    fn fixture_classifier_discovery_reference() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.3",
            "OxFunc/smart-fuzzer/ is a source-discovery path for capture. Resolve and record exact \
             files in infra-m0.4.",
        )]);
        let found = dispositions(&manifest);
        assert!(found.keys().all(|k| k.starts_with("OxFunc/smart-fuzzer/")));
        assert!(!found.contains_key("OxFunc/smart-fuzzer/runs/run-1.json"));
        assert!(found.values().all(|d| d != "port"), "{found:?}");
        for key in [
            "OxFunc/smart-fuzzer/README.md",
            "OxFunc/smart-fuzzer/tools/Run-Capture.ps1",
            "OxFunc/smart-fuzzer/tools/Build-Plan.ps1",
            "OxFunc/smart-fuzzer/tools/probe/src/notes.rs",
        ] {
            assert_eq!(found[key], "reference", "{key}");
            assert_eq!(input(&manifest, key)["category"], "oracle-runner", "{key}");
        }
        let runner = input(&manifest, "OxFunc/smart-fuzzer/tools/Run-Capture.ps1");
        assert_eq!(
            runner["runner_candidate"]["markers"],
            json!(["Excel.Application", "Marshal", "Interop", "ComObject"])
        );
        assert!(
            runner["runner_candidate"]["selection"]
                .as_str()
                .unwrap()
                .contains("infra-m1.2")
        );
        assert!(input(&manifest, "OxFunc/smart-fuzzer/README.md")["runner_candidate"].is_null());
        assert!(
            input(&manifest, "OxFunc/smart-fuzzer/tools/Build-Plan.ps1")["runner_candidate"]
                .is_null()
        );
        assert_eq!(manifest["summary"]["runner_candidates"], 1);
        assert_eq!(manifest["summary"]["port_provisional"], 0);
    }

    /// The quarantined directory is quarantined whatever the citing text says, and so is a path
    /// whose clause says QUARANTINED; neither is counted as port.
    #[test]
    fn fixture_classifier_quarantined() {
        let manifest = fixture_manifest(&[
            bead(
                "dc-f.4",
                "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: the exp sequence.",
            ),
            bead(
                "dc-f.5",
                "OxCalc/src/oxcalc-core/src/consumer.rs: Consumer is QUARANTINED here.",
            ),
        ]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[
                ("OxCalc/src/oxcalc-core/src/consumer.rs", "quarantined"),
                (
                    "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
                    "quarantined"
                ),
            ])
        );
        assert_eq!(manifest["summary"]["by_disposition"]["quarantined"], 2);
        assert_eq!(manifest["summary"]["port_provisional"], 0);
    }

    /// Every file of a package that enables research-x87, and every Rust file whose code names
    /// excel_numeric::research (directly or through a use group), is quarantine_derived even when
    /// cited for porting; a comment or string naming it is not enough.
    #[test]
    fn fixture_classifier_quarantine_derived() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.6",
            "OxFunc/smart-fuzzer/tools/: the tool sources.",
        )]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[
                ("OxFunc/smart-fuzzer/tools/Build-Plan.ps1", "port"),
                ("OxFunc/smart-fuzzer/tools/Run-Capture.ps1", "port"),
                ("OxFunc/smart-fuzzer/tools/probe/Cargo.toml", "port"),
                (
                    "OxFunc/smart-fuzzer/tools/probe/src/bin/grouped.rs",
                    "quarantine_derived"
                ),
                (
                    "OxFunc/smart-fuzzer/tools/probe/src/main.rs",
                    "quarantine_derived"
                ),
                ("OxFunc/smart-fuzzer/tools/probe/src/notes.rs", "port"),
                (
                    "OxFunc/smart-fuzzer/tools/racer/Cargo.toml",
                    "quarantine_derived"
                ),
                (
                    "OxFunc/smart-fuzzer/tools/racer/src/main.rs",
                    "quarantine_derived"
                ),
            ])
        );
        assert!(
            input(&manifest, "OxFunc/smart-fuzzer/tools/racer/src/main.rs")["quarantine_derived"]
                .as_str()
                .unwrap()
                .contains("smart-fuzzer/tools/racer/Cargo.toml")
        );
        assert_eq!(
            manifest["summary"]["by_disposition"]["quarantine_derived"],
            4
        );
        assert_eq!(manifest["summary"]["port_provisional"], 4);
    }

    /// A path its own sentence says not to port is excluded, never an input, and is not pinned
    /// as a declared module of a pinned input for that bead (counted instead).
    #[test]
    fn fixture_classifier_excluded() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.7",
            "OxCalc/src/oxcalc-core/src/grid/machine.rs: the machine. Do NOT port consumer.rs or \
             grid/machine/r1c1_plan.rs.",
        )]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[
                ("OxCalc/src/oxcalc-core/src/grid/machine.rs", "port"),
                (
                    "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs",
                    "port"
                ),
            ])
        );
        let excluded: Vec<&str> = manifest["excluded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            excluded,
            [
                "src/oxcalc-core/src/consumer.rs",
                "src/oxcalc-core/src/grid/machine/r1c1_plan.rs"
            ]
        );
        assert_eq!(
            manifest["summary"]["resolution_skipped"][SKIP_CHILD_EXCLUDED],
            1
        );
    }

    /// The declared modules of a cited file are pinned with its bead and disposition: plain
    /// `mod x;` under `<name>/` beside a non-mod-rs file, transitively, test-only modules
    /// included. The plan audit's changed blobs are listed.
    #[test]
    fn fixture_declared_modules_of_a_cited_file() {
        let manifest = fixture_manifest(&[bead(
            "dc-f.8",
            "OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile \
             as the behavioural reference.",
        )]);
        assert_eq!(
            dispositions(&manifest),
            owned(&[
                (
                    "OxCalc/src/oxcalc-core/src/grid/reference_engine.rs",
                    "reference"
                ),
                (
                    "OxCalc/src/oxcalc-core/src/grid/reference_engine/parse.rs",
                    "reference"
                ),
                (
                    "OxCalc/src/oxcalc-core/src/grid/reference_engine/transform.rs",
                    "reference"
                ),
                (
                    "OxCalc/src/oxcalc-core/src/grid/reference_engine/transform/tests.rs",
                    "reference"
                ),
            ])
        );
        let child = input(
            &manifest,
            "OxCalc/src/oxcalc-core/src/grid/reference_engine/parse.rs",
        );
        assert_eq!(child["beads"], json!(["dc-f.8"]));
        assert_eq!(
            child["via"],
            json!(["declared module of src/oxcalc-core/src/grid/reference_engine.rs"])
        );
        assert_eq!(manifest["summary"]["declared_module_inputs"], 3);
        assert_eq!(
            manifest["changed_since_plan_audit"],
            json!([{
                "path": "OxCalc/src/oxcalc-core/src/grid/reference_engine.rs",
                "audit_blob": "an-older-blob",
                "pinned_blob": "blob:src/oxcalc-core/src/grid/reference_engine.rs",
                "commit": "fixture-commit",
                "input": true,
            }])
        );
        assert_eq!(manifest["plan_audit"]["unchanged"], 1);
        assert_eq!(
            manifest["plan_audit"]["not_a_legacy_repository"],
            json!(["Elsewhere/a.rs"])
        );
    }

    /// The catalogue closure follows a `#[path]` module and a plain module: coercion.rs declares
    /// `#[path = "coercion_decimal.rs"] mod decimal;`, which declares a `#[path]` module and a
    /// plain `mod sibling;` that resolves beside it (a `#[path]` file reads children as a mod.rs
    /// does). The decoy coercion/decimal.rs, which no declaration names, stays out.
    #[test]
    fn fixture_catalogue_closure_follows_path_and_plain_modules() {
        let manifest = fixture_manifest(&[bead_with_do(
            "dc-f.9",
            "Port the named family only: ABS.",
            "OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for \
             the function IDs named in Do.",
        )]);
        let found = dispositions(&manifest);
        let expected = owned(&[
            ("OxFunc/crates/oxfunc_core/src/coercion.rs", "port"),
            ("OxFunc/crates/oxfunc_core/src/coercion_decimal.rs", "port"),
            (
                "OxFunc/crates/oxfunc_core/src/coercion_decimal_powers.rs",
                "port",
            ),
            ("OxFunc/crates/oxfunc_core/src/functions/abs.rs", "port"),
            (
                "OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs",
                "port",
            ),
            ("OxFunc/crates/oxfunc_core/src/sibling.rs", "port"),
        ]);
        assert_eq!(found, expected);
        let decimal = input(
            &manifest,
            "OxFunc/crates/oxfunc_core/src/coercion_decimal.rs",
        );
        assert_eq!(
            decimal["via"],
            json!([
                "catalogue helper",
                "declared module of crates/oxfunc_core/src/coercion.rs"
            ])
        );
        assert_eq!(decimal["category"], "function-family");
        assert_eq!(
            manifest["functions"][0]["files"],
            json!(["crates/oxfunc_core/src/functions/abs.rs"])
        );
        assert_eq!(manifest["families"][0]["functions"], json!(["FUNC.ABS"]));
    }

    /// The lookup's exit codes: pinned port 0, quarantined 3 with its do-not-copy line, never-
    /// pinned bulk evidence 4, unavailable 1; no output says verified.
    #[test]
    fn lookups_exit_by_disposition() {
        let manifest = fixture_manifest(&[
            bead(
                "dc-f.1",
                "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome.",
            ),
            bead(
                "dc-f.4",
                "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: the exp sequence.",
            ),
        ]);
        let look = |wanted: &str| lookup_text(&manifest, "m.json", wanted);
        let (text, code) = look("OxCalc/src/oxcalc-core/src/grid/machine/differential.rs");
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("  blob blob:src/oxcalc-core/src/grid/machine/differential.rs\n"));
        assert!(text.contains("  verification provisional\n"));
        let (text, code) = look("OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs");
        assert_eq!(code, EXIT_QUARANTINED);
        assert!(
            text.contains("quarantined: do not copy (infra-m0.5)\n"),
            "{text}"
        );
        for bulk in [
            "OxFunc/smart-fuzzer/cache",
            "OxFunc/smart-fuzzer/runs/a/b.json",
        ] {
            let (text, code) = look(bulk);
            assert_eq!(code, EXIT_BULK_EVIDENCE);
            assert!(
                text.starts_with("not pinned: bulk evidence (infra-m0.6, DnaCalc/OxFunc-Evidence)"),
                "{text}"
            );
        }
        let (text, code) = look("OxFml/no/such/file.rs");
        assert_eq!(code, EXIT_UNAVAILABLE);
        assert!(text.starts_with("unavailable OxFml/no/such/file.rs"));
        for wanted in [
            "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs",
            "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
        ] {
            assert!(!look(wanted).0.to_ascii_lowercase().contains("verified"));
        }
    }

    #[test]
    fn declarations_resolve_as_rustc_reads_them() {
        let tree: BTreeMap<String, String> = [
            "c/src/a.rs",
            "c/src/a/b.rs",
            "c/src/a/inner/d.rs",
            "c/src/m/mod.rs",
            "c/src/m/n.rs",
            "c/src/x/y.rs",
            "c/src/both.rs",
            "c/src/both/mod.rs",
        ]
        .into_iter()
        .map(|p| (p.to_owned(), String::new()))
        .collect();
        let decl = |name: &str, path: Option<&str>, inline: &[&str]| pin_rust::ModDecl {
            inline: inline
                .iter()
                .map(|n| pin_rust::InlineModule {
                    name: (*n).to_owned(),
                    path: None,
                })
                .collect(),
            name: name.to_owned(),
            path: path.map(str::to_owned),
            test_only: false,
            cfg_attr_path: false,
        };
        let root = ModState {
            dir: "c/src".to_owned(),
            relative: None,
        };
        let a = ModState {
            dir: "c/src".to_owned(),
            relative: Some("a".to_owned()),
        };
        let file = |state: &ModState, d: pin_rust::ModDecl| {
            resolve_declaration(&tree, state, &d).map(|(f, _, fallback)| (f, fallback))
        };
        assert_eq!(
            file(&root, decl("a", None, &[])),
            Ok(("c/src/a.rs".to_owned(), false))
        );
        assert_eq!(
            file(&a, decl("b", None, &[])),
            Ok(("c/src/a/b.rs".to_owned(), false))
        );
        assert_eq!(
            file(&a, decl("d", None, &["inner"])),
            Ok(("c/src/a/inner/d.rs".to_owned(), false))
        );
        assert_eq!(
            file(&root, decl("m", None, &[])),
            Ok(("c/src/m/mod.rs".to_owned(), false))
        );
        assert_eq!(
            file(&a, decl("q", Some("x/y.rs"), &[])),
            Ok(("c/src/x/y.rs".to_owned(), false))
        );
        assert_eq!(
            file(&root, decl("both", None, &[])),
            Err(SKIP_MODULE_AMBIGUOUS)
        );
        assert_eq!(
            file(&root, decl("none", None, &[])),
            Err(SKIP_MODULE_ABSENT)
        );
        assert_eq!(
            file(&root, decl("out", Some("../../../z.rs"), &[])),
            Err(SKIP_MODULE_OUTSIDE)
        );
        // A file loaded through #[path] but cited directly reads as `<name>/` first; the
        // sibling reading is a counted fallback.
        assert_eq!(
            file(&a, decl("m", None, &[])),
            Ok(("c/src/m/mod.rs".to_owned(), true))
        );
        assert_eq!(inferred_state("c/src/a.rs"), a);
        assert_eq!(inferred_state("c/src/lib.rs"), root);
        assert!(may_declare_modules("pub(crate) mod decimal ;\n"));
        assert!(!may_declare_modules("mod tests {\n}\n"));
    }
}
