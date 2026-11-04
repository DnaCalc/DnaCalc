//! `cargo xtask corpus [<dir>] [--oracle] [--out <dir>] [--cache <dir>] [--import
//! <capture.json>]... [--no-capture] [--timeout <s>]`: a folder of workbooks, and with `--oracle`
//! the oracle workbooks committed in this repository, each compared with Excel through the
//! product route, with one summary of the whole corpus (bead `dc-vcs`).
//!
//! 1. **Workbooks.** Every `.xlsx`, `.xlsm` and `.xlsb` file directly in `<dir>` (not in its
//!    subfolders, and not an Excel lock file `~$...`), in name order; an `.xlsb` is captured with
//!    its twin, the `.xlsx` or `.xlsm` of its stem in the same folder that Excel saved from the
//!    same workbook, as the inventory the capture worker reads (bead dc-5sr), and without one it
//!    is not compared; with `--oracle`, every
//!    `crates/*/tests/fixtures/*/oracle.xlsx` of the repository, each with its committed capture
//!    `crates/<crate>/tests/captures/<case>/oracle.capture.json`, or, for a workbook captured per
//!    section, its section captures `.../<case>/sections/<sheet>.capture.json` (`oracle_cases.rs`).
//!    An oracle workbook is compared only with its committed captures, never with a cached one or
//!    one taken now: a workbook whose committed capture is missing or of other bytes, and each
//!    section with no capture of its current key, is counted as awaiting capture (its formula
//!    cells, counted from the package), and `cargo xtask capture --pending` takes it. A sectioned
//!    workbook is compared once per capture session its valid section captures come from, each
//!    time with one record of that session's sections (`calc_verdict::merged_record`, written to
//!    `<out>/workbooks/<name>/session-<n>.capture.json`), and the counts are added.
//! 2. **Captures of a folder's workbooks, taken once and cached.** The cache (`--cache`, by default
//!    `C:\Work\DnaCalc\corpus\captures-cache`, outside the repository) holds a capture record at
//!    `<sha256 of the workbook>/<Excel build>.capture.json`, or, for a workbook captured under
//!    another calculation policy than Excel's default (one whose `calcPr` iterates, since dc-10y:
//!    `capture::with_workbook_iteration`), at `<sha256>/<Excel build>_policy-<policy>.capture.json`
//!    ([`policy_key`]), so a record taken under the default policy is never used for it. The Excel build is the file version of
//!    the `EXCEL.EXE` that `App Paths` registers, which is what a capture records as its
//!    `excel_build`. A workbook with a committed capture uses it. Otherwise the cached capture of
//!    this Excel build is used; when the build cannot be read, the cached capture of any build is
//!    used and the summary says which. Without one, the workbook is captured with Excel
//!    (`calc_oracle_excel::capture`, as `cargo xtask capture` takes it, accepted by the same rule)
//!    and the record is cached; `--no-capture` never starts Excel and counts such a workbook as not
//!    compared. `--import` adds existing capture records to the cache first, keyed by the
//!    workbook hash their `source` names and the build their environment names, so a capture taken
//!    before never has to be taken again.
//! 3. **Excel processes.** calc-oracle-excel submits capture jobs to the lane's owned Excel
//!    session queue. The host resets and verifies its recorded baseline between jobs, retains
//!    launch counts and receipts, and repeats every fiftieth ordinary observation in a fresh
//!    process. A crash, drift or fresh-required job recycles only its owned PID/start-time tuple.
//!    Other Excel processes are never stopped. The receipt is retained with each capture.
//! 4. **Comparison through the product route.** Each workbook is compared by a `cargo xtask
//!    compare` process (the one verdict path: calc_host open, recalculate and project, calc-verdict
//!    judge, then the triage), writing `report.json`, `evaluations.json`, `triage.json` and its log
//!    `compare.log` to `<out>/workbooks/<name>/`. That process is a release build of xtask: this
//!    one, when it is a release build, and otherwise one this command builds first with `cargo
//!    build --release -p xtask` in `target/xtask-corpus`, so the recorded wall times are those of
//!    optimised code. A separate process means a workbook that crashes the comparison is a counted
//!    failure, and the run carries on.
//! 5. **The summary.** `<out>/summary.md` and `<out>/summary.json` (`corpus_summary.rs`): per
//!    workbook and in total, pass (matched Excel), external links (matched from another
//!    workbook's link caches), circular held values and awaiting captures, all inside fail beside
//!    mismatches, refusals and failed/missing results over all formula cells, the pass rate over all formula cells, the wall times of open,
//!    recalculation, projection and judging, and the failures grouped by root cause across the
//!    corpus. A workbook that could not be read, captured or compared is listed as not compared,
//!    with the stage and the reason, and the run carries on.
//!
//! `--out` defaults to `target/corpus`. The summary of a corpus outside the repository belongs
//! outside it too: it names workbooks, and a reason a workbook was not compared may quote a sheet.
//! Paths are relative to the current directory.
//!
//! Exit codes: 0 when the summary was written, whatever it says; 1 when it cannot be written, the
//! cache or the output folder cannot be made, or the release build of the compare process fails;
//! 2 on a usage error.

use crate::capture::{accepted, repository_revision};
use crate::corpus_summary::{Compared, NotCompared, Summary, Times, Workbook, read_triage};
use crate::oracle_cases::{OracleCase, oracle_cases, package_parts, section_states, whole_state};
use calc_oracle_excel::{CalculationPolicy, CaptureRequest, capture, windows_powershell};
use calc_verdict::{CaptureRecord, sha256_hex};
use calc_verdict::{merged_record, workbook_sections};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask corpus [<dir>] [--recursive] [--oracle] [--out <dir>] [--cache <dir>] [--import <capture.json>]... [--no-capture] [--dna-only] [--timeout <s>] [--shard <index/count>] [--resume] [--minimize] | cargo xtask corpus --merge <summary.json>... --out <dir>";

const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// The cache of capture records, outside the repository.
pub(crate) const DEFAULT_CACHE: &str = r"C:\Work\DnaCalc\corpus\captures-cache";
const DEFAULT_OUT: &str = "target/corpus";
/// One workbook's census/capture/comparison budget; the largest Ofgem capture took most of an hour.
const DEFAULT_TIMEOUT_S: u64 = 3 * 3600;
/// The target directory of the release build of the compare process.
const RELEASE_TARGET_DIR: &str = "target/xtask-corpus";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Args {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) oracle: bool,
    pub(crate) out: PathBuf,
    pub(crate) cache: PathBuf,
    pub(crate) imports: Vec<PathBuf>,
    pub(crate) no_capture: bool,
    pub(crate) timeout: Duration,
    pub(crate) recursive: bool,
    pub(crate) shard: Option<(u64, u64)>,
    pub(crate) resume: bool,
    pub(crate) minimize: bool,
    pub(crate) dna_only: bool,
    pub(crate) merges: Vec<PathBuf>,
}

pub(crate) fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        dir: None,
        oracle: false,
        out: PathBuf::from(DEFAULT_OUT),
        cache: PathBuf::from(DEFAULT_CACHE),
        imports: Vec::new(),
        no_capture: false,
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_S),
        recursive: false,
        shard: None,
        resume: false,
        minimize: false,
        dna_only: false,
        merges: Vec::new(),
    };
    let (mut out, mut cache, mut timeout) = (None, None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let once = |slot: &mut Option<String>, flag: &str, value: String| {
            if slot.replace(value).is_some() {
                Err(format!("{flag} is given twice"))
            } else {
                Ok(())
            }
        };
        match arg.as_str() {
            "--oracle" => parsed.oracle = true,
            "--no-capture" => parsed.no_capture = true,
            "--recursive" => parsed.recursive = true,
            "--resume" => parsed.resume = true,
            "--minimize" => parsed.minimize = true,
            "--dna-only" => {
                parsed.dna_only = true;
                parsed.no_capture = true;
            }
            "--shard" => {
                if parsed.shard.is_some() {
                    return Err("--shard is given twice".into());
                }
                let text = value("--shard")?;
                let (index, count) = text
                    .split_once('/')
                    .ok_or("--shard needs index/count (zero based)")?;
                let index: u64 = index.parse().map_err(|_| "invalid shard index")?;
                let count: u64 = count.parse().map_err(|_| "invalid shard count")?;
                if count == 0 || index >= count {
                    return Err("--shard needs 0 <= index < count".into());
                }
                parsed.shard = Some((index, count));
            }
            "--out" => once(&mut out, "--out", value("--out")?)?,
            "--cache" => once(&mut cache, "--cache", value("--cache")?)?,
            "--timeout" => once(&mut timeout, "--timeout", value("--timeout")?)?,
            "--import" => parsed.imports.push(PathBuf::from(value("--import")?)),
            "--merge" => parsed.merges.push(PathBuf::from(value("--merge")?)),
            flag if flag.starts_with("--") => return Err(format!("unknown argument {flag:?}")),
            dir => {
                if parsed.dir.replace(PathBuf::from(dir)).is_some() {
                    return Err("only one workbook folder may be given".to_owned());
                }
            }
        }
    }
    if parsed.dir.is_none()
        && !parsed.oracle
        && parsed.imports.is_empty()
        && parsed.merges.is_empty()
    {
        return Err("give a workbook folder, --oracle or --import".to_owned());
    }
    if !parsed.merges.is_empty()
        && (parsed.dir.is_some()
            || parsed.oracle
            || !parsed.imports.is_empty()
            || parsed.shard.is_some())
    {
        return Err("--merge takes shard summaries without a workbook folder, --oracle, --import or --shard".into());
    }
    if let Some(out) = out {
        parsed.out = PathBuf::from(out);
    }
    if let Some(cache) = cache {
        parsed.cache = PathBuf::from(cache);
    }
    if let Some(text) = timeout {
        let seconds = text
            .parse::<u64>()
            .ok()
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| format!("--timeout {text:?} is not a positive number of seconds"))?;
        parsed.timeout = Duration::from_secs(seconds);
    }
    Ok(parsed)
}

/// `text` with every character other than an ASCII letter, digit, `.`, `-` or `_` written `_`.
pub(crate) fn file_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// One workbook to compare: its name in the summary, its path, and its committed capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    /// Frozen by content selection; later phases refuse a changed source.
    pub(crate) batch_sha256: Option<String>,
    pub(crate) committed: Option<PathBuf>,
    /// For an `.xlsb`, its SpreadsheetML twin in the same folder, when there is one.
    pub(crate) inventory: Option<PathBuf>,
    /// For an oracle workbook of this repository, its case: compared only with its committed
    /// captures.
    pub(crate) oracle: Option<OracleCase>,
}

/// The `.xlsx`, `.xlsm` and `.xlsb` files directly in `dir`, in name order, leaving out Excel's
/// lock files; each `.xlsb` with its twin.
pub(crate) fn folder_workbooks(dir: &Path) -> Result<Vec<Entry>, String> {
    let listing =
        fs::read_dir(dir).map_err(|e| format!("cannot list the folder {}: {e}", dir.display()))?;
    let mut entries = Vec::new();
    for item in listing {
        let item = item.map_err(|e| format!("cannot list the folder {}: {e}", dir.display()))?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        if path.is_file()
            && !name.starts_with("~$")
            && matches!(extension.as_deref(), Some("xlsx" | "xlsm" | "xlsb" | "xls"))
        {
            let inventory = (extension.as_deref() == Some("xlsb"))
                .then(|| {
                    ["xlsx", "xlsm"]
                        .iter()
                        .map(|twin| path.with_extension(twin))
                        .find(|twin| twin.is_file())
                })
                .flatten();
            entries.push(Entry {
                name,
                path: path.clone(),
                batch_sha256: None,
                committed: path
                    .with_extension("capture.json")
                    .is_file()
                    .then(|| path.with_extension("capture.json")),
                inventory,
                oracle: None,
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Extend the ordinary folder inventory; do not follow directory links into
/// another corpus or revisit a directory through a link.
pub(crate) fn folder_tree(root: &Path, recursive: bool) -> Result<Vec<Entry>, String> {
    let mut pending = vec![root.to_owned()];
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        let mut entries = folder_workbooks(&dir)?;
        for entry in &mut entries {
            entry.name = entry
                .path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
        }
        found.extend(entries);
        if recursive {
            for item in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let item = item.map_err(|e| e.to_string())?;
                if item.file_type().map_err(|e| e.to_string())?.is_dir() {
                    pending.push(item.path());
                }
            }
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Every `crates/*/tests/fixtures/*/oracle.xlsx` under `root`, in path order, with its committed
/// capture `crates/<crate>/tests/captures/<case>/oracle.capture.json` when there is one.
pub(crate) fn oracle_workbooks(root: &Path) -> Result<Vec<Entry>, String> {
    Ok(oracle_cases(root)?
        .into_iter()
        .map(|case| {
            let committed = case.whole_capture();
            Entry {
                name: case.name.clone(),
                path: case.fixture.clone(),
                batch_sha256: None,
                committed: committed.is_file().then_some(committed),
                inventory: None,
                oracle: Some(case),
            }
        })
        .collect())
}

/// The cache of capture records: `<root>/<sha256>/<Excel build>.capture.json`.
pub(crate) struct Cache {
    pub(crate) root: PathBuf,
}

impl Cache {
    /// Linux has no local Excel build. Retain the recorded calculation policy
    /// when choosing among existing captures instead of discarding iteration.
    pub(crate) fn find_policy(
        &self,
        sha256: &str,
        policy: &CalculationPolicy,
    ) -> Option<(PathBuf, String)> {
        let mut candidates = Vec::new();
        for item in fs::read_dir(self.root.join(file_safe(sha256))).ok()? {
            let path = item.ok()?.path();
            if !path
                .file_name()?
                .to_string_lossy()
                .ends_with(".capture.json")
            {
                continue;
            }
            let Some(record) = fs::read(&path)
                .ok()
                .and_then(|bytes| CaptureRecord::decode(&bytes).ok())
            else {
                continue;
            };
            if record.source != format!("sha256:{sha256}")
                || policy_key("policy", &record_policy(&record)) != policy_key("policy", policy)
            {
                continue;
            }
            let Some(build) = record
                .environment
                .as_ref()
                .map(|env| env.target.excel_build())
            else {
                continue;
            };
            candidates.push((path, policy_key(build, policy)));
        }
        candidates.sort();
        candidates.pop()
    }
    pub(crate) fn path(&self, sha256: &str, build: &str) -> PathBuf {
        self.root
            .join(file_safe(sha256))
            .join(format!("{}.capture.json", file_safe(build)))
    }

    /// The cached capture of the workbook `sha256` for `build` (a [`policy_key`]); when the build
    /// is not known, the cached capture of any build taken under the default policy (the last in
    /// name order). Its path and build.
    pub(crate) fn find(&self, sha256: &str, build: Option<&str>) -> Option<(PathBuf, String)> {
        if let Some(build) = build {
            let path = self.path(sha256, build);
            return path.is_file().then(|| (path, build.to_owned()));
        }
        let mut cached: Vec<(PathBuf, String)> = fs::read_dir(self.root.join(file_safe(sha256)))
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|item| {
                let name = item.file_name().to_string_lossy().into_owned();
                let build = name.strip_suffix(".capture.json")?.to_owned();
                (!build.contains("_policy-")).then(|| (item.path(), build))
            })
            .collect();
        cached.sort();
        cached.pop()
    }

    /// Stores `bytes`, the capture record of the workbook `sha256` taken by `build`, unless a
    /// record is cached there already; the cached path, and whether it was written now.
    pub(crate) fn store(
        &self,
        sha256: &str,
        build: &str,
        bytes: &[u8],
    ) -> Result<(PathBuf, bool), String> {
        let path = self.path(sha256, build);
        if path.is_file() {
            return Ok((path, false));
        }
        let dir = path.parent().expect("a cache path has a folder");
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let partial = path.with_extension("partial");
        fs::write(&partial, bytes)
            .and_then(|()| fs::rename(&partial, &path))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        Ok((path, true))
    }

    /// Imports the capture record at `record`: keyed by the workbook hash its `source` names and
    /// the Excel build its environment names. Says what became of it.
    pub(crate) fn import(&self, record: &Path) -> Result<String, String> {
        let bytes = fs::read(record)
            .map_err(|e| format!("cannot read the capture record {}: {e}", record.display()))?;
        let decoded = CaptureRecord::decode(&bytes)
            .map_err(|e| format!("the capture record {} is refused: {e}", record.display()))?;
        let (sha256, build) = record_key(&decoded)?;
        let (path, written) = self.store(&sha256, &build, &bytes)?;
        Ok(if written {
            format!("imported {} as {}", record.display(), path.display())
        } else if fs::read(&path).is_ok_and(|cached| cached == bytes) {
            format!(
                "{} is cached already as {}",
                record.display(),
                path.display()
            )
        } else {
            format!(
                "{} is not imported: another record of the same workbook and Excel build is cached as {}, and is kept",
                record.display(),
                path.display()
            )
        })
    }
}

/// The key a capture under `policy` by Excel `build` is cached by: the build for Excel's default
/// policy (automatic, iteration off, 100 iterations, maximum change 0.001), else the build,
/// `_policy-` and the policy (`16.0.20430.20182_policy-manual-iteration-100-3f50624dd2f1a9fc`).
pub(crate) fn policy_key(build: &str, policy: &CalculationPolicy) -> String {
    if *policy == CalculationPolicy::default() {
        return build.to_owned();
    }
    let mode = match policy.mode {
        calc_oracle_excel::CalculationMode::Automatic => "automatic",
        calc_oracle_excel::CalculationMode::Manual => "manual",
        calc_oracle_excel::CalculationMode::SemiAutomatic => "semiautomatic",
    };
    format!(
        "{build}_policy-{mode}-iteration{}-{}-{:016x}",
        if policy.iteration { "" } else { "off" },
        policy.max_iterations,
        policy.max_change.to_bits()
    )
}

/// The calculation policy a capture record was taken under, read from the settings its
/// environment records (`calculation`, `iteration`, `max_iterations`, `max_change`); Excel's
/// default where a setting is missing or not read.
fn record_policy(record: &CaptureRecord) -> CalculationPolicy {
    let mut policy = CalculationPolicy::default();
    let Some(environment) = &record.environment else {
        return policy;
    };
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
    };
    if let Some(mode) = setting("calculation").and_then(calc_oracle_excel::CalculationMode::parse) {
        policy.mode = mode;
    }
    if let Some(iteration) = setting("iteration") {
        policy.iteration = iteration == "true";
    }
    if let Some(count) = setting("max_iterations").and_then(|text| text.parse().ok()) {
        policy.max_iterations = count;
    }
    if let Some(bits) = setting("max_change").and_then(|text| u64::from_str_radix(text, 16).ok()) {
        policy.max_change = f64::from_bits(bits);
    }
    policy
}

/// The cache key of a capture record: the workbook hash its `source` names, and the Excel build
/// its environment names with the calculation policy it was taken under ([`policy_key`]).
pub(crate) fn record_key(record: &CaptureRecord) -> Result<(String, String), String> {
    let sha256 = record
        .source
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| {
            format!(
                "the capture record's source {:?} is not a sha256: workbook hash",
                record.source
            )
        })?
        .to_ascii_lowercase();
    let build = record
        .environment
        .as_ref()
        .map(|environment| environment.target.excel_build().to_owned())
        .filter(|build| !build.is_empty())
        .ok_or("the capture record names no Excel build, so it cannot be keyed by one")?;
    Ok((sha256, policy_key(&build, &record_policy(record))))
}

/// The file version of the `EXCEL.EXE` that `App Paths` registers, as a capture records its
/// `excel_build`.
pub(crate) fn excel_build() -> Result<String, String> {
    if !cfg!(windows) {
        return Err(format!(
            "an Excel build is read on Windows; this host is {}",
            std::env::consts::OS
        ));
    }
    let script = "$p = (Get-ItemProperty -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\excel.exe' -ErrorAction Stop).'(default)'; (Get-Item -LiteralPath $p -ErrorAction Stop).VersionInfo.FileVersion";
    let output = Command::new(windows_powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| format!("cannot start Windows PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success()
        && !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        Ok(text)
    } else {
        Err(format!(
            "the Excel build could not be read ({}): {}",
            output.status,
            crate::cargo::tail(&output.stderr, 3)
        ))
    }
}

/// The repository root: two levels above xtask's manifest.
pub(crate) fn repository_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    match root.canonicalize() {
        // Windows gives a canonical path its verbatim prefix `\\?\`, which the summary need not
        // show; the path without it names the same folder.
        Ok(canonical) => {
            let text = canonical.to_string_lossy();
            match text.strip_prefix(r"\\?\") {
                Some(plain) if !plain.starts_with("UNC") => PathBuf::from(plain),
                _ => canonical,
            }
        }
        Err(_) => root,
    }
}

/// The release xtask the comparisons run in, and its description.
pub(crate) fn compare_program() -> Result<(PathBuf, String), String> {
    if !cfg!(debug_assertions) {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot name this xtask's executable: {e}"))?;
        return Ok((exe, "this xtask, a release build".to_owned()));
    }
    let root = repository_root();
    let target = root.join(RELEASE_TARGET_DIR);
    println!(
        "xtask corpus: building the release xtask the comparisons run in ({})",
        target.display()
    );
    let mut command = Command::new(crate::cargo::cargo_program());
    command
        .current_dir(&root)
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "xtask-harness",
            "--target-dir",
        ])
        .arg(&target);
    let output = crate::cargo::output(&mut command)?;
    if !output.status.success() {
        return Err(format!(
            "the release build of xtask failed ({}):\n{}",
            output.status,
            crate::cargo::tail(&output.stderr, 20)
        ));
    }
    let exe = target
        .join("release")
        .join(format!("xtask-harness{}", std::env::consts::EXE_SUFFIX));
    Ok((
        exe,
        format!("a release build of xtask in {RELEASE_TARGET_DIR}"),
    ))
}

/// The capture of one workbook: where it came from, its path, and the capture time when taken now.
struct Captured {
    path: PathBuf,
    source: String,
    seconds: Option<f64>,
}

/// The run's state: the cache, the Excel build (read once, when first needed) and the options.
struct Run<'a> {
    args: &'a Args,
    cache: Cache,
    build: Option<Result<String, String>>,
    program_digest: String,
}

impl Run<'_> {
    fn build(&mut self) -> Result<String, String> {
        self.build.get_or_insert_with(excel_build).clone()
    }

    /// The capture `entry` is compared with: its committed capture, the cached one, or one taken
    /// now with Excel and cached.
    fn capture_of(
        &mut self,
        entry: &Entry,
        sha256: &str,
        policy: &CalculationPolicy,
        timeout: Duration,
    ) -> Result<Captured, NotCompared> {
        if let Some(committed) = &entry.committed {
            return Ok(Captured {
                path: committed.clone(),
                source: format!("committed capture {}", committed.display()),
                seconds: None,
            });
        }
        let build = self.build();
        let keyed = build.as_ref().ok().map(|build| policy_key(build, policy));
        let lookup = if keyed.is_some() {
            self.cache.find(sha256, keyed.as_deref())
        } else {
            self.cache.find_policy(sha256, policy)
        };
        if let Some((path, cached_build)) = lookup {
            let note = match &build {
                Ok(_) => String::new(),
                Err(e) => format!("; this Excel's build is not known ({e})"),
            };
            return Ok(Captured {
                source: format!(
                    "cached capture of Excel {cached_build} ({}){note}",
                    path.display()
                ),
                path,
                seconds: None,
            });
        }
        let not = |reason: String| NotCompared {
            stage: "capture".to_owned(),
            reason,
        };
        if self.args.no_capture {
            return Err(not(match build {
                Ok(build) => format!(
                    "no capture is cached for Excel {build}, and --no-capture starts no Excel"
                ),
                Err(e) => format!(
                    "no capture is cached, and --no-capture starts no Excel (this Excel's build is not known: {e})"
                ),
            }));
        }
        let stem = entry
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let request = CaptureRequest {
            case: format!("corpus__{}", file_safe(&stem)),
            fixture: entry.path.clone(),
            inventory: entry.inventory.clone(),
            companions: Vec::new(),
            work_dir: self.cache.root.join("work").join(format!(
                "{}-{}",
                &sha256[..12.min(sha256.len())],
                crate::accept::utc_now().replace(':', "")
            )),
            timeout,
            policy: policy.clone(),
            revision: 0,
            operations: Vec::new(),
            repository: repository_revision(),
        };
        println!(
            "xtask corpus: capturing {} with Excel (timeout {}s)",
            entry.name,
            timeout.as_secs()
        );
        let started = Instant::now();
        let outcome = capture(&request).map_err(|e| not(e.to_string()))?;
        let seconds = started.elapsed().as_secs_f64();
        let excel = format!(
            "Excel PID {} ({})",
            outcome
                .run
                .excel_pid
                .map_or_else(|| "?".to_owned(), |pid| pid.to_string()),
            outcome.run.excel.describe(outcome.run.excel_pid)
        );
        accepted(&outcome).map_err(|e| {
            not(format!(
                "the capture is refused: {e}; {excel}; the record is kept in {}",
                request.work_dir.display()
            ))
        })?;
        let (record_sha, record_build) = record_key(&outcome.record).map_err(not)?;
        if record_sha != sha256 {
            return Err(not(format!(
                "the capture's source is sha256:{record_sha}, not the workbook's sha256:{sha256}"
            )));
        }
        let (path, _) = self
            .cache
            .store(sha256, &record_build, outcome.encoded.as_bytes())
            .map_err(not)?;
        Ok(Captured {
            source: format!(
                "captured now with Excel {record_build} in {seconds:.1} s, {excel}, cached as {}",
                path.display()
            ),
            path,
            seconds: Some(seconds),
        })
    }
}

/// Compares `entry` with its capture in a compare process, writing to `out`.
fn compare_one(
    program: &Path,
    entry: &Entry,
    capture: Option<&Path>,
    out: &Path,
    args: &Args,
    program_digest: &str,
    snapshot: &str,
) -> (Result<Value, NotCompared>, f64) {
    let not = |reason: String| NotCompared {
        stage: "compare".to_owned(),
        reason,
    };
    let started = Instant::now();
    let attempt = || -> Result<Value, String> {
        fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
        let key = crate::corpus_batch::key(&entry.path, capture, program_digest, args.minimize)?;
        if key["fixture"].as_str() != Some(snapshot) {
            return Err("workbook bytes changed after inventory admission".into());
        }
        if args.resume
            && let Some(triage) = crate::corpus_batch::completed(out, &key)
        {
            println!(
                "xtask corpus: resumed {} (verified result bytes)",
                entry.name
            );
            return Ok(triage);
        }
        if args.timeout.is_zero() {
            return Err("workbook timeout before calculation".into());
        }
        let marker = out.join("complete.json");
        if marker.exists() {
            fs::remove_file(&marker).map_err(|e| e.to_string())?;
        }
        let log = crate::corpus_batch::attempt_log(out)?;
        let payload = log.with_extension("");
        fs::create_dir(&payload).map_err(|e| e.to_string())?;
        let mut command = Command::new(program); // ubs:ignore -- compare_program supplies the current or Cargo-built executable; corpus data supplies only arguments.
        command
            .arg("compare")
            .arg("--fixture")
            .arg(&entry.path)
            .arg("--out")
            .arg(&payload);
        match capture {
            Some(path) => {
                command.arg("--capture").arg(path);
            }
            None => {
                command.arg("--dna-only");
            }
        }
        if args.minimize {
            command.arg("--minimize");
        }
        let result = crate::corpus_batch::bounded(&mut command, &log, args.timeout);
        let outcome = match &result {
            Ok(status) if status.success() => "completed".to_owned(),
            Ok(status) => format!("comparison ended with {status}"),
            Err(reason) => reason.clone(),
        };
        let attempt_record = json!({"key":key, "outcome":outcome,
            "finished_utc":crate::accept::utc_now(), "wall_time_s":started.elapsed().as_secs_f64(),
            "log":log.file_name().map(|name|name.to_string_lossy())});
        fs::write(
            log.with_extension("json"),
            serde_json::to_vec_pretty(&attempt_record).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        // Retain every numbered attempt; compare.log is the latest convenience copy.
        if log.is_file() {
            fs::copy(&log, out.join("compare.log")).map_err(|e| e.to_string())?;
        }
        let inventory = payload.join("inventory.record.json");
        if let Ok(bytes) = fs::read(&inventory)
            && let Ok(record) = CaptureRecord::decode(&bytes)
            && record.source == format!("sha256:{}", key["fixture"].as_str().unwrap_or(""))
        {
            fs::write(out.join("inventory.record.json"), bytes).map_err(|e| e.to_string())?;
        }
        let status = result?;
        if !status.success() {
            return Err(format!("{outcome}; log {}", log.display()));
        }
        let bytes = fs::read(payload.join("triage.json"))
            .map_err(|e| format!("cannot read new triage.json: {e}"))?;
        let triage: Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("triage.json does not parse: {e}"))?;
        if triage["source"] != format!("sha256:{snapshot}") {
            return Err("comparison source differs from the admitted workbook".into());
        }
        read_triage(&triage)?;
        crate::corpus_batch::publish(&payload, out)?;
        crate::corpus_batch::finish(out, &key)?;
        Ok(triage)
    };
    (attempt().map_err(not), started.elapsed().as_secs_f64())
}

/// Runs the corpus and writes its summary; the exit code.
fn run_corpus(args: &Args) -> u8 {
    if !args.merges.is_empty() {
        return match crate::corpus_batch::merge_summaries(&args.merges, &args.out) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("xtask corpus: {e}");
                EXIT_FAILED
            }
        };
    }
    let started = Instant::now();
    let repository = repository_revision();
    let cache = Cache {
        root: args.cache.clone(),
    };
    if let Err(e) = fs::create_dir_all(&cache.root) {
        eprintln!(
            "xtask corpus: cannot create the cache {}: {e}",
            cache.root.display()
        );
        return EXIT_FAILED;
    }
    for record in &args.imports {
        match cache.import(record) {
            Ok(message) => println!("xtask corpus: {message}"),
            Err(e) => eprintln!("xtask corpus: {e}"),
        }
    }
    let mut entries = Vec::new();
    let mut listing_failures = Vec::new();
    if let Some(dir) = &args.dir {
        match folder_tree(dir, args.recursive) {
            Ok(found) => entries.extend(found),
            Err(e) => listing_failures.push(e),
        }
    }
    if args.oracle {
        match oracle_workbooks(&repository_root()) {
            Ok(found) => entries.extend(found),
            Err(e) => listing_failures.push(e),
        }
    }
    for failure in &listing_failures {
        eprintln!("xtask corpus: {failure}");
    }
    if entries.is_empty() && listing_failures.is_empty() {
        if args.dir.is_none() && !args.oracle {
            return 0;
        }
        eprintln!("xtask corpus: no workbooks to compare");
    }
    if let Err(e) = fs::create_dir_all(args.out.join("workbooks")) {
        eprintln!("xtask corpus: cannot create {}: {e}", args.out.display());
        return EXIT_FAILED;
    }
    let (program, program_note) = if entries.is_empty() {
        (PathBuf::new(), "none run".to_owned())
    } else {
        match compare_program() {
            Ok(program) => program,
            Err(e) => {
                eprintln!("xtask corpus: {e}");
                return EXIT_FAILED;
            }
        }
    };

    let (entries, inventory) = crate::corpus_batch::select(entries, args.shard);
    if let Err(e) = fs::write(
        args.out.join("inventory.json"),
        serde_json::to_vec_pretty(&inventory).expect("inventory serializes"),
    ) {
        eprintln!("xtask corpus: cannot write inventory: {e}");
        return EXIT_FAILED;
    }
    let program_digest = if program.as_os_str().is_empty() {
        String::new()
    } else {
        match crate::corpus_batch::file_digest(&program) {
            Ok(digest) => digest,
            Err(e) => {
                eprintln!("xtask corpus: cannot hash executable: {e}");
                return EXIT_FAILED;
            }
        }
    };
    let mut run = Run {
        args,
        cache,
        build: None,
        program_digest,
    };
    let mut workbooks = Vec::new();
    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        println!("xtask corpus: [{}/{total}] {}", i + 1, entry.name);
        let workbook = one_workbook(&mut run, &program, entry, &args.out);
        match &workbook.outcome {
            Ok(compared) => {
                let c = &compared.counts;
                println!(
                    "xtask corpus:   formula cells {}; pass {}; external links {}; circular rule {}; awaiting capture {}; fail {} (mismatched {}, refused {}, failed or missing {}); pass rate {}{}",
                    c.formula_cells,
                    c.pass,
                    c.external_links,
                    c.circular_rule,
                    c.awaiting_capture,
                    c.fail(),
                    c.mismatched,
                    c.refused,
                    c.failed_or_missing,
                    crate::corpus_summary::rate(c.pass, c.formula_cells),
                    compared
                        .workbook_failure
                        .as_ref()
                        .map(|failure| format!("; the whole run failed: {failure}"))
                        .unwrap_or_default()
                );
            }
            Err(not) => println!(
                "xtask corpus:   not compared: {}: {}",
                not.stage, not.reason
            ),
        }
        workbooks.push(workbook);
    }

    let mut command = vec!["cargo xtask corpus".to_owned()];
    command.extend(std::env::args().skip(2));
    let build = match &run.build {
        Some(Ok(build)) => json!(build),
        Some(Err(e)) => json!(format!("not known: {e}")),
        None => json!("not read (no capture needed it)"),
    };
    let summary = Summary {
        run: json!({
            "command": command.join(" "),
            "finished_utc": crate::accept::utc_now(),
            "repository": repository,
            "compare_process": program_note,
            "excel_build": build,
            "cache": run.cache.root.display().to_string(),
            "listing_failures": listing_failures,
            "wall_time_s": started.elapsed().as_secs_f64(),
            "shard": args.shard,
            "inventory": "inventory.json",
            "corpus_inventory_sha256": inventory["content_key"],
            "resume": args.resume,
            "dna_only_fallback": args.dna_only,
            "comparison_timeout_s": args.timeout.as_secs(),
            "resumed_stage_times": "from the verified original comparison",
            "executable_sha256": run.program_digest,

        }),
        workbooks,
    };
    let json_text = match serde_json::to_string_pretty(&summary.json()) {
        Ok(text) => text + "\n",
        Err(e) => {
            eprintln!("xtask corpus: the summary cannot be encoded: {e}");
            return EXIT_FAILED;
        }
    };
    for (name, text) in [
        ("summary.json", json_text),
        ("summary.md", summary.markdown()),
    ] {
        let path = args.out.join(name);
        if let Err(e) = fs::write(&path, text) {
            eprintln!("xtask corpus: cannot write {}: {e}", path.display());
            return EXIT_FAILED;
        }
    }
    if let Err(e) = crate::corpus_batch::write_problem_functions(&args.out, &summary.json()) {
        eprintln!("xtask corpus: {e}");
        return EXIT_FAILED;
    }
    let totals = summary.totals();
    let c = &totals.counts;
    println!(
        "xtask corpus: {} workbooks ({} compared, {} not compared); formula cells {}; pass {}; external links {}; circular rule {}; awaiting capture {}; fail {} (mismatched {}, refused {}, failed or missing {}); pass rate over all formula cells {}",
        totals.workbooks,
        totals.compared,
        totals.not_compared,
        c.formula_cells,
        c.pass,
        c.external_links,
        c.circular_rule,
        c.awaiting_capture,
        c.fail(),
        c.mismatched,
        c.refused,
        c.failed_or_missing,
        crate::corpus_summary::rate(c.pass, c.formula_cells)
    );
    println!(
        "xtask corpus: wrote summary.md and summary.json to {}",
        args.out.display()
    );
    0
}

fn inventory_policy(record: &CaptureRecord) -> Result<CalculationPolicy, String> {
    let context = &record
        .session
        .as_ref()
        .ok_or("source inventory has no context")?
        .context;
    let mut fields = context.split(';');
    if fields.next() != Some("excel-calc-context/1") {
        return Err("unknown source context format".into());
    }
    let fields: BTreeMap<_, _> = fields.filter_map(|field| field.split_once('=')).collect();
    let read = |name| {
        fields
            .get(name)
            .copied()
            .ok_or_else(|| format!("source context lacks {name}"))
    };
    let iteration = match read("iteration")? {
        "true" => true,
        "false" => false,
        _ => return Err("invalid source iteration setting".into()),
    };
    let mut policy = CalculationPolicy::default();
    policy.iteration = iteration;
    if iteration {
        policy.mode = calc_oracle_excel::CalculationMode::Manual;
        policy.max_iterations = read("max_iterations")?
            .parse()
            .map_err(|_| "invalid source iteration count")?;
        policy.max_change = f64::from_bits(
            u64::from_str_radix(read("max_change")?, 16)
                .map_err(|_| "invalid source maximum change")?,
        );
    }
    Ok(policy)
}

/// Reads, captures (or finds the capture of) and compares one workbook.
fn one_workbook(run: &mut Run<'_>, program: &Path, entry: &Entry, out: &Path) -> Workbook {
    let file_started = Instant::now();
    let mut workbook = Workbook {
        name: entry.name.clone(),
        sha256: None,
        capture: None,
        inventory_formula_cells: None,
        failure_key: None,
        outcome: Err(NotCompared {
            stage: "read".to_owned(),
            reason: String::new(),
        }),
        times: Times::default(),
    };
    // Only committed oracle metadata is inspected in the parent. Folder input
    // inflation and model admission remain inside the bounded census child.
    let bytes = if entry.oracle.is_some() {
        match fs::read(&entry.path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                workbook.outcome = Err(NotCompared {
                    stage: "read".into(),
                    reason: format!("cannot read {}: {e}", entry.path.display()),
                });
                return workbook;
            }
        }
    } else {
        None
    };
    let sha256 = match bytes
        .as_deref()
        .map(sha256_hex)
        .map(Ok)
        .unwrap_or_else(|| crate::corpus_batch::file_digest(&entry.path))
    {
        Ok(digest) => digest,
        Err(e) => {
            workbook.outcome = Err(NotCompared {
                stage: "read".into(),
                reason: format!("cannot read {}: {e}", entry.path.display()),
            });
            return workbook;
        }
    };
    workbook.sha256 = Some(sha256.clone());
    if entry.batch_sha256.as_deref() != Some(sha256.as_str()) {
        workbook.failure_key = Some("SourceChanged".into());
        workbook.outcome = Err(NotCompared {
            stage: "read".into(),
            reason: "workbook no longer matches its batch inventory; resume to take a new snapshot"
                .into(),
        });
        return workbook;
    }
    if let Some(case) = &entry.oracle {
        let bytes = bytes.as_deref().expect("committed oracle bytes read above");
        workbook.inventory_formula_cells = package_parts(bytes)
            .and_then(|parts| workbook_sections(&parts))
            .ok()
            .map(|sections| sections.iter().map(|s| s.formula_cells).sum());
        if case.sectioned() {
            sectioned_workbook(
                &mut workbook,
                program,
                entry,
                case,
                bytes,
                &sha256,
                out,
                run.args,
                &run.program_digest,
            );
            return workbook;
        }
        if let Err(reason) = whole_state(case, &sha256) {
            workbook.capture = Some(format!("awaiting capture: {reason}"));
            workbook.outcome = awaiting(bytes, &reason);
            return workbook;
        }
    }
    drop(bytes);
    let dir = crate::corpus_batch::output_dir(out, &entry.name, &sha256);
    let inventory = match crate::corpus_batch::census(
        program,
        &entry.path,
        &sha256,
        &dir,
        &run.program_digest,
        run.args.resume,
        run.args.timeout.saturating_sub(file_started.elapsed()),
    ) {
        Ok(record) => record,
        Err(reason) => {
            workbook.failure_key = fs::read(dir.join("inventory/inventory-failure.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .filter(|failure| failure["source"] == format!("sha256:{sha256}"))
                .and_then(|failure| failure["key"].as_str().map(str::to_owned));
            workbook.outcome = Err(NotCompared {
                stage: "inventory".into(),
                reason,
            });
            return workbook;
        }
    };
    workbook.inventory_formula_cells = Some(inventory.formulas.len() as u64);
    let policy = match inventory_policy(&inventory) {
        Ok(policy) => policy,
        Err(reason) => {
            workbook.outcome = Err(NotCompared {
                stage: "inventory".into(),
                reason,
            });
            return workbook;
        }
    };
    let captured = match run.capture_of(
        entry,
        &sha256,
        &policy,
        run.args.timeout.saturating_sub(file_started.elapsed()),
    ) {
        Ok(captured) => Some(captured),
        Err(not) if run.args.dna_only => {
            workbook.capture = Some(format!("DNA only; no Excel observations: {}", not.reason));
            None
        }
        Err(not) => {
            workbook.outcome = Err(not);
            return workbook;
        }
    };
    if captured.is_none() && inventory.formulas.is_empty() {
        workbook.outcome = Err(NotCompared {
            stage: "inventory".into(),
            reason: "source contains no formula cells; no comparison required".into(),
        });
        return workbook;
    }
    if let Some(captured) = &captured {
        workbook.capture = Some(captured.source.clone());
        workbook.times.capture = captured.seconds;
    }
    let mut bounded_args = run.args.clone();
    bounded_args.timeout = run.args.timeout.saturating_sub(file_started.elapsed());
    let (triage, seconds) = compare_one(
        program,
        entry,
        captured.as_ref().map(|c| c.path.as_path()),
        &dir,
        &bounded_args,
        &run.program_digest,
        &sha256,
    );
    if workbook.inventory_formula_cells.is_none() {
        workbook.inventory_formula_cells = fs::read(dir.join("inventory.record.json"))
            .ok()
            .and_then(|bytes| CaptureRecord::decode(&bytes).ok())
            .filter(|record| record.source == format!("sha256:{sha256}"))
            .map(|record| record.formulas.len() as u64);
    }
    workbook.times.compare_process = Some(seconds);
    workbook.outcome = triage.and_then(|triage| {
        read_triage(&triage).map_err(|reason| NotCompared {
            stage: "summary".to_owned(),
            reason,
        })
    });
    workbook
}

/// The outcome of a workbook awaiting capture: its formula cells, counted from the package, all
/// awaiting capture.
fn awaiting(bytes: &[u8], reason: &str) -> Result<Compared, NotCompared> {
    let cells = package_parts(bytes)
        .and_then(|parts| workbook_sections(&parts))
        .map_err(|e| NotCompared {
            stage: "read".to_owned(),
            reason: format!(
                "awaiting capture ({reason}), and its formula cells cannot be counted: {e}"
            ),
        })?
        .iter()
        .map(|section| section.formula_cells)
        .sum();
    let mut compared = Compared::default();
    compared.counts.formula_cells = cells;
    compared.counts.awaiting_capture = cells;
    compared.causes.insert(
        crate::corpus_summary::Cause::Other {
            class: "capture".into(),
            key: "awaiting capture".into(),
        },
        crate::corpus_summary::CauseCount {
            whole_workbook: cells,
            ..Default::default()
        },
    );
    Ok(compared)
}

/// Compares a workbook captured per section: each capture session's valid section captures
/// together, against one record of them, and every section with no valid capture counted as
/// awaiting capture.
fn sectioned_workbook(
    workbook: &mut Workbook,
    program: &Path,
    entry: &Entry,
    case: &OracleCase,
    bytes: &[u8],
    sha256: &str,
    out: &Path,
    args: &Args,
    program_digest: &str,
) {
    let file_started = Instant::now();
    let states = match section_states(case, bytes) {
        Ok((states, _)) => states,
        Err(reason) => {
            workbook.outcome = Err(NotCompared {
                stage: "read".to_owned(),
                reason,
            });
            return;
        }
    };
    // The valid captures by the session that took them: the package Excel opened, the capture
    // tool and the workbook's path (which CELL("filename") reads).
    let mut sessions: BTreeMap<(String, String, String), Vec<&CaptureRecord>> = BTreeMap::new();
    let mut compared = Compared::default();
    let mut awaiting_names = Vec::new();
    for state in &states {
        match &state.capture {
            Some(record) => {
                let session = record.session.as_ref();
                let path = record
                    .environment
                    .as_ref()
                    .and_then(|environment| {
                        environment
                            .settings
                            .iter()
                            .find(|(name, _)| name == "workbook_path")
                            .map(|(_, value)| value.clone())
                    })
                    .unwrap_or_default();
                sessions
                    .entry((
                        session.map(|s| s.input_package.clone()).unwrap_or_default(),
                        session.map(|s| s.tool.clone()).unwrap_or_default(),
                        path,
                    ))
                    .or_default()
                    .push(record);
            }
            None => {
                compared.counts.formula_cells += state.section.formula_cells;
                compared.counts.awaiting_capture += state.section.formula_cells;
                awaiting_names.push(format!(
                    "{} ({} cells)",
                    state.section.sheet, state.section.formula_cells
                ));
            }
        }
    }
    let dir = crate::corpus_batch::output_dir(out, &entry.name, sha256);
    if let Err(e) = fs::create_dir_all(&dir) {
        workbook.outcome = Err(NotCompared {
            stage: "compare".to_owned(),
            reason: format!("cannot create {}: {e}", dir.display()),
        });
        return;
    }
    let mut compare_seconds = 0.0;
    for (n, records) in sessions.values().enumerate() {
        let Some(merged) = merged_record(records, &case.case, &format!("sha256:{sha256}")) else {
            continue;
        };
        let path = dir.join(format!("session-{}.capture.json", n + 1));
        let written = merged
            .encode()
            .map_err(|e| e.to_string())
            .and_then(|text| fs::write(&path, text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            workbook.outcome = Err(NotCompared {
                stage: "capture".to_owned(),
                reason: format!(
                    "the record of capture session {} cannot be written: {e}",
                    n + 1
                ),
            });
            return;
        }
        let mut bounded_args = args.clone();
        bounded_args.timeout = args.timeout.saturating_sub(file_started.elapsed());
        let (triage, seconds) = compare_one(
            program,
            entry,
            Some(&path),
            &dir.join(format!("session-{}", n + 1)),
            &bounded_args,
            program_digest,
            sha256,
        );
        compare_seconds += seconds;
        match triage.and_then(|triage| {
            read_triage(&triage).map_err(|reason| NotCompared {
                stage: "summary".to_owned(),
                reason,
            })
        }) {
            Ok(part) => compared.add(&part),
            Err(not) => {
                workbook.outcome = Err(not);
                return;
            }
        }
    }
    workbook.times.compare_process = Some(compare_seconds);
    if compared.counts.awaiting_capture > 0 {
        compared.causes.insert(
            crate::corpus_summary::Cause::Other {
                class: "capture".into(),
                key: "awaiting capture".into(),
            },
            crate::corpus_summary::CauseCount {
                whole_workbook: compared.counts.awaiting_capture,
                ..Default::default()
            },
        );
    }
    let captured = states.iter().filter(|s| s.capture.is_some()).count();
    workbook.capture = Some(format!(
        "{captured} committed section captures from {} capture session(s); {} section(s) awaiting capture{}",
        sessions.len(),
        awaiting_names.len(),
        if awaiting_names.is_empty() {
            String::new()
        } else {
            format!(": {}", awaiting_names.join(", "))
        }
    ));
    workbook.outcome = Ok(compared);
}

pub fn run(raw: &[String]) -> ExitCode {
    let args = match parse_args(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask corpus: {e}\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    ExitCode::from(run_corpus(&args))
}

#[cfg(test)]
#[path = "corpus_tests.rs"]
mod tests;
