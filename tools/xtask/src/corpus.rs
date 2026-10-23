//! `cargo xtask corpus [<dir>] [--oracle] [--out <dir>] [--cache <dir>] [--import
//! <capture.json>]... [--no-capture] [--timeout <s>]`: a folder of workbooks, and with `--oracle`
//! the oracle workbooks committed in this repository, each compared with Excel through the
//! product route, with one summary of the whole corpus (bead `dc-vcs`).
//!
//! 1. **Workbooks.** Every `.xlsx` and `.xlsm` file directly in `<dir>` (not in its subfolders,
//!    and not an Excel lock file `~$...`), in name order; with `--oracle`, every
//!    `crates/*/tests/fixtures/*/oracle.xlsx` of the repository, each with its committed capture
//!    `crates/<crate>/tests/captures/<case>/oracle.capture.json` when there is one.
//! 2. **Captures, taken once and cached.** The cache (`--cache`, by default
//!    `C:\Work\DnaCalc\corpus\captures-cache`, outside the repository) holds a capture record at
//!    `<sha256 of the workbook>/<Excel build>.capture.json`. The Excel build is the file version of
//!    the `EXCEL.EXE` that `App Paths` registers, which is what a capture records as its
//!    `excel_build`. A workbook with a committed capture uses it. Otherwise the cached capture of
//!    this Excel build is used; when the build cannot be read, the cached capture of any build is
//!    used and the summary says which. Without one, the workbook is captured with Excel
//!    (`calc_oracle_excel::capture`, as `cargo xtask capture` takes it, accepted by the same rule)
//!    and the record is cached; `--no-capture` never starts Excel and counts such a workbook as not
//!    compared. `--import` adds existing capture records to the cache first, keyed by the
//!    workbook hash their `source` names and the build their environment names, so a capture taken
//!    before never has to be taken again.
//! 3. **Excel processes.** A capture runs in the isolation of calc-oracle-excel: its worker starts
//!    a new Excel through COM and reports that Excel's PID, read from the window handle
//!    `Application.Hwnd`; the adapter terminates only a PID that was not running before the
//!    capture started. The owner's Excel, and any Excel this command did not start, is never
//!    stopped. What became of each capture's Excel is in the summary's capture line.
//! 4. **Comparison through the product route.** Each workbook is compared by a `cargo xtask
//!    compare` process (the one verdict path: calc_host open, recalculate and project, calc-verdict
//!    judge, then the triage), writing `report.json`, `evaluations.json`, `triage.json` and its log
//!    `compare.log` to `<out>/workbooks/<name>/`. That process is a release build of xtask: this
//!    one, when it is a release build, and otherwise one this command builds first with `cargo
//!    build --release -p xtask` in `target/xtask-corpus`, so the recorded wall times are those of
//!    optimised code. A separate process means a workbook that crashes the comparison is a counted
//!    failure, and the run carries on.
//! 5. **The summary.** `<out>/summary.md` and `<out>/summary.json` (`corpus_summary.rs`): per
//!    workbook and in total, pass (matched Excel) and fail (mismatched + refused + failed or
//!    missing) over all formula cells, the pass rate over all formula cells, the wall times of open,
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
use crate::corpus_summary::{NotCompared, Summary, Times, Workbook, read_triage};
use calc_oracle_excel::{CalculationPolicy, CaptureRequest, capture, windows_powershell};
use calc_verdict::{CaptureRecord, sha256_hex};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask corpus [<dir>] [--oracle] [--out <dir>] [--cache <dir>] [--import <capture.json>]... [--no-capture] [--timeout <s>]";

const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// The cache of capture records, outside the repository.
pub(crate) const DEFAULT_CACHE: &str = r"C:\Work\DnaCalc\corpus\captures-cache";
const DEFAULT_OUT: &str = "target/corpus";
/// The timeout of one Excel capture; the largest Ofgem workbook took most of an hour.
const DEFAULT_TIMEOUT_S: u64 = 3 * 3600;
/// The target directory of the release build of the compare process.
const RELEASE_TARGET_DIR: &str = "target/xtask-corpus";

#[derive(Debug, PartialEq)]
pub(crate) struct Args {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) oracle: bool,
    pub(crate) out: PathBuf,
    pub(crate) cache: PathBuf,
    pub(crate) imports: Vec<PathBuf>,
    pub(crate) no_capture: bool,
    pub(crate) timeout: Duration,
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
            "--out" => once(&mut out, "--out", value("--out")?)?,
            "--cache" => once(&mut cache, "--cache", value("--cache")?)?,
            "--timeout" => once(&mut timeout, "--timeout", value("--timeout")?)?,
            "--import" => parsed.imports.push(PathBuf::from(value("--import")?)),
            flag if flag.starts_with("--") => return Err(format!("unknown argument {flag:?}")),
            dir => {
                if parsed.dir.replace(PathBuf::from(dir)).is_some() {
                    return Err("only one workbook folder may be given".to_owned());
                }
            }
        }
    }
    if parsed.dir.is_none() && !parsed.oracle && parsed.imports.is_empty() {
        return Err("give a workbook folder, --oracle or --import".to_owned());
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
    pub(crate) committed: Option<PathBuf>,
}

/// The `.xlsx` and `.xlsm` files directly in `dir`, in name order, leaving out Excel's lock
/// files.
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
            && matches!(extension.as_deref(), Some("xlsx" | "xlsm"))
        {
            entries.push(Entry {
                name,
                path,
                committed: None,
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Every `crates/*/tests/fixtures/*/oracle.xlsx` under `root`, in path order, with its committed
/// capture `crates/<crate>/tests/captures/<case>/oracle.capture.json` when there is one.
pub(crate) fn oracle_workbooks(root: &Path) -> Result<Vec<Entry>, String> {
    let list = |dir: &Path| -> Result<Vec<PathBuf>, String> {
        let mut dirs: Vec<PathBuf> = match fs::read_dir(dir) {
            Ok(listing) => listing
                .filter_map(Result::ok)
                .map(|item| item.path())
                .filter(|path| path.is_dir())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("cannot list {}: {e}", dir.display())),
        };
        dirs.sort();
        Ok(dirs)
    };
    let mut entries = Vec::new();
    for krate in list(&root.join("crates"))? {
        for case in list(&krate.join("tests").join("fixtures"))? {
            let path = case.join("oracle.xlsx");
            if !path.is_file() {
                continue;
            }
            let crate_name = krate.file_name().unwrap_or_default().to_string_lossy();
            let case_name = case.file_name().unwrap_or_default().to_string_lossy();
            let committed = krate
                .join("tests")
                .join("captures")
                .join(case.file_name().unwrap_or_default())
                .join("oracle.capture.json");
            entries.push(Entry {
                name: format!("{crate_name}/{case_name}/oracle.xlsx"),
                path,
                committed: committed.is_file().then_some(committed),
            });
        }
    }
    Ok(entries)
}

/// The cache of capture records: `<root>/<sha256>/<Excel build>.capture.json`.
pub(crate) struct Cache {
    pub(crate) root: PathBuf,
}

impl Cache {
    pub(crate) fn path(&self, sha256: &str, build: &str) -> PathBuf {
        self.root
            .join(file_safe(sha256))
            .join(format!("{}.capture.json", file_safe(build)))
    }

    /// The cached capture of the workbook `sha256` for `build`; when the build is not known, the
    /// cached capture of any build (the last in name order). Its path and build.
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
                Some((item.path(), build))
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

/// The cache key of a capture record: the workbook hash its `source` names and the Excel build
/// its environment names.
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
    Ok((sha256, build))
}

/// The file version of the `EXCEL.EXE` that `App Paths` registers, as a capture records its
/// `excel_build`.
fn excel_build() -> Result<String, String> {
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
fn repository_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    root.canonicalize().unwrap_or(root)
}

/// The release xtask the comparisons run in, and its description.
fn compare_program() -> Result<(PathBuf, String), String> {
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
            "xtask",
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
        .join(format!("xtask{}", std::env::consts::EXE_SUFFIX));
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
}

impl Run<'_> {
    fn build(&mut self) -> Result<String, String> {
        self.build.get_or_insert_with(excel_build).clone()
    }

    /// The capture `entry` is compared with: its committed capture, the cached one, or one taken
    /// now with Excel and cached.
    fn capture_of(&mut self, entry: &Entry, sha256: &str) -> Result<Captured, NotCompared> {
        if let Some(committed) = &entry.committed {
            return Ok(Captured {
                path: committed.clone(),
                source: format!("committed capture {}", committed.display()),
                seconds: None,
            });
        }
        let build = self.build();
        if let Some((path, cached_build)) = self.cache.find(sha256, build.as_deref().ok()) {
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
            companions: Vec::new(),
            work_dir: self.cache.root.join("work").join(format!(
                "{}-{}",
                &sha256[..12.min(sha256.len())],
                crate::accept::utc_now().replace(':', "")
            )),
            timeout: self.args.timeout,
            policy: CalculationPolicy::default(),
            revision: 0,
            operations: Vec::new(),
            repository: repository_revision(),
        };
        println!(
            "xtask corpus: capturing {} with Excel (timeout {}s)",
            entry.name,
            self.args.timeout.as_secs()
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
    capture: &Path,
    out: &Path,
) -> (Result<Value, NotCompared>, f64) {
    let not = |reason: String| NotCompared {
        stage: "compare".to_owned(),
        reason,
    };
    if let Err(e) = fs::create_dir_all(out) {
        return (
            Err(not(format!("cannot create {}: {e}", out.display()))),
            0.0,
        );
    }
    let started = Instant::now();
    let output = Command::new(program)
        .arg("compare")
        .arg("--fixture")
        .arg(&entry.path)
        .arg("--capture")
        .arg(capture)
        .arg("--out")
        .arg(out)
        .output();
    let seconds = started.elapsed().as_secs_f64();
    let output = match output {
        Ok(output) => output,
        Err(e) => {
            return (
                Err(not(format!("cannot start {}: {e}", program.display()))),
                seconds,
            );
        }
    };
    let mut log = output.stdout.clone();
    log.extend_from_slice(&output.stderr);
    let _ = fs::write(out.join("compare.log"), &log);
    if !output.status.success() {
        return (
            Err(not(format!(
                "cargo xtask compare ended with {}: {}",
                output.status,
                crate::cargo::tail(&output.stderr, 3).trim()
            ))),
            seconds,
        );
    }
    let triage = fs::read(out.join("triage.json"))
        .map_err(|e| format!("cannot read triage.json: {e}"))
        .and_then(|bytes| {
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|e| format!("triage.json does not parse: {e}"))
        });
    (triage.map_err(not), seconds)
}

/// Runs the corpus and writes its summary; the exit code.
fn run_corpus(args: &Args) -> u8 {
    let started = Instant::now();
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
        match folder_workbooks(dir) {
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

    let mut run = Run {
        args,
        cache,
        build: None,
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
                    "xtask corpus:   formula cells {}; pass {}; fail {} (mismatched {}, refused {}, failed or missing {}); pass rate {}{}",
                    c.formula_cells,
                    c.pass,
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
            "repository": repository_revision(),
            "compare_process": program_note,
            "excel_build": build,
            "cache": run.cache.root.display().to_string(),
            "listing_failures": listing_failures,
            "wall_time_s": started.elapsed().as_secs_f64(),
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
    let totals = summary.totals();
    let c = &totals.counts;
    println!(
        "xtask corpus: {} workbooks ({} compared, {} not compared); formula cells {}; pass {}; fail {} (mismatched {}, refused {}, failed or missing {}); pass rate over all formula cells {}",
        totals.workbooks,
        totals.compared,
        totals.not_compared,
        c.formula_cells,
        c.pass,
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

/// Reads, captures (or finds the capture of) and compares one workbook.
fn one_workbook(run: &mut Run<'_>, program: &Path, entry: &Entry, out: &Path) -> Workbook {
    let mut workbook = Workbook {
        name: entry.name.clone(),
        sha256: None,
        capture: None,
        outcome: Err(NotCompared {
            stage: "read".to_owned(),
            reason: String::new(),
        }),
        times: Times::default(),
    };
    let bytes = match fs::read(&entry.path) {
        Ok(bytes) => bytes,
        Err(e) => {
            workbook.outcome = Err(NotCompared {
                stage: "read".to_owned(),
                reason: format!("cannot read {}: {e}", entry.path.display()),
            });
            return workbook;
        }
    };
    let sha256 = sha256_hex(&bytes);
    drop(bytes);
    workbook.sha256 = Some(sha256.clone());
    let captured = match run.capture_of(entry, &sha256) {
        Ok(captured) => captured,
        Err(not) => {
            workbook.outcome = Err(not);
            return workbook;
        }
    };
    workbook.capture = Some(captured.source);
    workbook.times.capture = captured.seconds;
    let dir = out.join("workbooks").join(file_safe(&entry.name));
    let (triage, seconds) = compare_one(program, entry, &captured.path, &dir);
    workbook.times.compare_process = Some(seconds);
    workbook.outcome = triage.and_then(|triage| {
        read_triage(&triage).map_err(|reason| NotCompared {
            stage: "summary".to_owned(),
            reason,
        })
    });
    workbook
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
