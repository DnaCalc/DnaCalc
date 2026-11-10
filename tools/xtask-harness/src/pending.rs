//! `cargo xtask capture --pending [--list] [--timeout <s>] [--attempts <n>]`: every committed
//! oracle capture that is missing or no longer of its workbook, taken with Excel; and `cargo xtask
//! capture --split --fixture <oracle.xlsx> --capture <capture.json> [--saved <package>]
//! [--keep-saved <sheet>]... [--pending-only] [--only <sheet>]...`, a capture of a whole sectioned oracle workbook written as its
//! section captures, with no Excel run.
//!
//! Slow validation is run on purpose, never as a condition of building, testing or committing: no
//! test fails because a capture is missing or stale (such cells are awaiting capture in every
//! report), and this is the one command that takes what is pending.
//!
//! **What is pending** (`oracle_cases.rs`): every oracle workbook
//! `crates/<crate>/tests/fixtures/<case>/oracle.xlsx` whose committed capture is missing or is of
//! other bytes, and, for a workbook captured per section (`tests/captures/<case>/sections/`), each
//! sheet with formulas whose section capture is missing or of another section key. `--list` prints
//! that and stops.
//!
//! **How it is taken.** Through calc-oracle-excel, the one capture route, as `cargo xtask capture`
//! takes one: an isolated Excel instance per workbook, which the worker's message filter keeps
//! retrying while it rejects calls (RPC_E_CALL_REJECTED) and which is the only Excel the capture
//! stops. Each pending section is taken in its own Excel session and written as soon as it is
//! taken, so a section whose capture fails costs only itself, and the next run takes up only what
//! is still pending. Excel opens a copy of the workbook in which every other section's cells are
//! taken out (its sheets, names, order and formats, and the sheets the section reads, stay as they
//! are, so the section's key in the copy is its key in the workbook, which is checked before Excel
//! starts). A capture that fails, times out or is refused (an isolation failure, a file-cache
//! replay the verdict refuses) is tried again, up to `--attempts` times (default 3), in a new work
//! directory under `target/c/`; one that still fails is reported and the others go on. The timeout of one capture
//! is `--timeout` seconds, or by default 120 s and half a second per formula cell.
//!
//! **What it writes.** A whole workbook's record to `oracle.capture.json` (and the package Excel
//! saved to `oracle.saved.xlsx` when that file is kept there); a section's record to
//! `sections/<sheet>.capture.json`, its source the section key (and the worksheet part Excel saved
//! to `sections/<sheet>.saved.xml` when that file is kept there). It changes no setting and creates
//! no scheduled task; it can be run unattended, from a scheduler or by hand, from the repository
//! root of a clone.
//!
//! Exit codes: 0 when nothing was pending or everything pending was captured (or listed); 1 when
//! a capture still failed after its attempts, or a file cannot be read or written; 2 on a usage
//! error; 4 when this host cannot run a capture (not run).
//!
//! `--split` writes the section captures of a sectioned oracle workbook from a capture of the whole
//! workbook, whose source must be the workbook's content hash as committed: the cells of each
//! section map one to one, so no Excel run is needed. With `--saved`, the package Excel saved
//! during that capture, it also writes the saved worksheet part of each section that keeps one
//! (an existing `sections/<sheet>.saved.xml`, or a sheet named by `--keep-saved`).
//! With `--pending-only`, current section captures remain untouched; only missing or stale
//! sections are written. With `--only <sheet>` (repeatable), only the named sections are written, and
//! only their cells must map: a capture of those sheets alone (the answers of a legacy-evidence import)
//! is enough. The whole capture and saved package still undergo the same hash checks.

use crate::capture::{accepted, default_work_dir_in, repository_revision, with_workbook_iteration};
use crate::oracle_cases::{
    OracleCase, SectionState, emptied_sheet, keyed_saved_sheet, oracle_cases, package_parts,
    replace_package_parts, section_states, whole_state,
};
use calc_oracle_excel::{CalculationPolicy, CaptureError, CaptureOutcome, CaptureRequest, capture};
use calc_verdict::{CaptureRecord, section_record, sha256_hex, workbook_sections};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const USAGE: &str = "usage: cargo xtask capture --pending [--list] [--timeout <s>] [--attempts <n>]\n       cargo xtask capture --split --fixture <oracle.xlsx> --capture <capture.json> [--saved <package>] [--keep-saved <sheet>]... [--pending-only] [--only <sheet>]...";

const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_UNAVAILABLE: u8 = 4;

#[derive(Debug, PartialEq)]
struct PendingArgs {
    list: bool,
    timeout: Option<Duration>,
    attempts: u32,
}

fn parse_pending(args: &[String]) -> Result<PendingArgs, String> {
    let mut parsed = PendingArgs {
        list: false,
        timeout: None,
        attempts: 3,
    };
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut value = || {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--list" => parsed.list = true,
            "--timeout" => {
                let text = value()?;
                let seconds = text.parse::<u64>().ok().filter(|s| *s > 0).ok_or_else(|| {
                    format!("--timeout {text:?} is not a positive number of seconds")
                })?;
                parsed.timeout = Some(Duration::from_secs(seconds));
            }
            "--attempts" => {
                let text = value()?;
                parsed.attempts = text
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("--attempts {text:?} is not a positive count"))?;
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(parsed)
}

/// One workbook with something pending.
enum Pending {
    /// The whole workbook: its committed capture is missing or of other bytes.
    Whole {
        case: OracleCase,
        /// The case the record names: the committed record's, or the case folder's name.
        case_id: String,
    },
    /// Sections of a sectioned workbook.
    Sections {
        case: OracleCase,
        bytes: Vec<u8>,
        pending: Vec<SectionState>,
    },
}

impl Pending {
    fn case(&self) -> &OracleCase {
        match self {
            Pending::Whole { case, .. } | Pending::Sections { case, .. } => case,
        }
    }
}

/// The case id a whole-workbook record names: the committed record's, when it decodes, or the case
/// folder's name.
fn whole_case_id(case: &OracleCase) -> String {
    fs::read(case.whole_capture())
        .ok()
        .and_then(|bytes| CaptureRecord::decode(&bytes).ok())
        .map_or_else(|| case.case.clone(), |record| record.case)
}

/// What is pending in the repository at `root`, with what it prints.
fn pending(root: &Path) -> Result<(Vec<Pending>, Vec<String>), String> {
    let mut found = Vec::new();
    let mut lines = Vec::new();
    for case in oracle_cases(root)? {
        let bytes = fs::read(&case.fixture)
            .map_err(|e| format!("cannot read {}: {e}", case.fixture.display()))?;
        if case.sectioned() {
            let workbook_hash = sha256_hex(&bytes);
            if calc_oracle_excel::capture_source_sha256(&case.fixture, &workbook_hash)?
                != workbook_hash
            {
                return Err("authored-entry fixtures require whole-workbook captures".to_owned());
            }
            let (states, orphans) = section_states(&case, &bytes)?;
            let captured = states.iter().filter(|s| s.capture.is_some()).count();
            let waiting: Vec<SectionState> =
                states.into_iter().filter(|s| s.capture.is_none()).collect();
            let cells: u64 = waiting.iter().map(|s| s.section.formula_cells).sum();
            lines.push(format!(
                "{}: {} sections captured, {} awaiting capture ({cells} formula cells){}",
                case.name,
                captured,
                waiting.len(),
                if waiting.is_empty() { "" } else { ":" }
            ));
            for state in &waiting {
                lines.push(format!(
                    "  {} ({} formula cells): {}",
                    state.section.sheet,
                    state.section.formula_cells,
                    state.awaiting.as_deref().unwrap_or("")
                ));
            }
            for orphan in orphans {
                lines.push(format!(
                    "  {} names no sheet with formulas in the workbook; it is not read",
                    orphan.display()
                ));
            }
            if !waiting.is_empty() {
                found.push(Pending::Sections {
                    case,
                    bytes,
                    pending: waiting,
                });
            }
        } else if let Err(reason) = whole_state(&case, &sha256_hex(&bytes)) {
            lines.push(format!("{}: awaiting capture: {reason}", case.name));
            found.push(Pending::Whole {
                case_id: whole_case_id(&case),
                case,
            });
        }
    }
    Ok((found, lines))
}

/// The timeout of a capture of `cells` formula cells.
fn timeout_for(args: &PendingArgs, cells: u64) -> Duration {
    args.timeout
        .unwrap_or_else(|| Duration::from_secs(120 + cells / 2))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A capture of `fixture` (a file named as the workbook it stands for), tried up to `attempts`
/// times; `Err(None)` when this host cannot run a capture at all.
fn capture_with_attempts(
    root: &Path,
    case_id: &str,
    fixture: &Path,
    policy: CalculationPolicy,
    timeout: Duration,
    attempts: u32,
) -> Result<CaptureOutcome, Option<String>> {
    let mut last = String::new();
    for attempt in 1..=attempts {
        let work_dir = default_work_dir_in(&root.join("target").join("c"));
        println!(
            "xtask capture: [{attempt}/{attempts}] capturing {} as case {case_id} in {} (timeout {}s)",
            fixture.display(),
            work_dir.display(),
            timeout.as_secs()
        );
        let request = CaptureRequest {
            case: case_id.to_owned(),
            fixture: fixture.to_owned(),
            inventory: None,
            companions: Vec::new(),
            work_dir: work_dir.clone(),
            timeout,
            policy,
            revision: 0,
            operations: Vec::new(),
            repository: repository_revision(),
        };
        match capture(&request) {
            Ok(outcome) => match accepted(&outcome) {
                Ok(_) => return Ok(outcome),
                Err(e) => {
                    let _ = write(&work_dir.join("capture.json"), outcome.encoded.as_bytes());
                    last = format!(
                        "refused: {e} (the record is kept in {})",
                        work_dir.display()
                    );
                }
            },
            Err(CaptureError::Unavailable { reason }) => {
                eprintln!("xtask capture (not run): capture unavailable: {reason}");
                return Err(None);
            }
            Err(error) => last = error.to_string(),
        }
        eprintln!("xtask capture: attempt {attempt} of {attempts} failed: {last}");
        if attempt < attempts {
            // Excel is shared on this machine: give a busy one time before the next attempt.
            std::thread::sleep(Duration::from_secs(30));
        }
    }
    Err(Some(last))
}

/// The sheets of `sections` a capture of `pending` keeps whole: the pending ones, those they read,
/// and those with no formula; every other sheet's cells are taken out.
fn kept_sheets(
    sections: &[calc_verdict::WorkbookSection],
    pending: &[SectionState],
) -> BTreeSet<String> {
    let mut kept: BTreeSet<String> = sections
        .iter()
        .filter(|s| s.formula_cells == 0)
        .map(|s| s.sheet.clone())
        .collect();
    for state in pending {
        kept.insert(state.section.sheet.clone());
        kept.extend(state.section.reads.iter().cloned());
    }
    kept
}

/// The copy of a sectioned workbook a capture of its pending sections opens, with each pending
/// section's key checked to be its key in the workbook.
fn pending_copy(bytes: &[u8], pending: &[SectionState]) -> Result<Vec<u8>, String> {
    let parts = calc_verdict::stored_package_parts(bytes)?;
    let sections = workbook_sections(&parts)?;
    let kept = kept_sheets(&sections, pending);
    let mut replace = BTreeMap::new();
    for section in sections.iter().filter(|s| !kept.contains(&s.sheet)) {
        let xml = std::str::from_utf8(&parts[&section.part])
            .map_err(|e| format!("{}: {e}", section.part))?;
        replace.insert(section.part.clone(), emptied_sheet(xml).into_bytes());
    }
    let copy = replace_package_parts(bytes, &replace)?;
    let copied = workbook_sections(&calc_verdict::stored_package_parts(&copy)?)?;
    for state in pending {
        let key = copied
            .iter()
            .find(|s| s.sheet == state.section.sheet)
            .map(|s| s.key.as_str());
        if key != Some(state.section.key.as_str()) {
            return Err(format!(
                "the section {} has another key in the copy of the workbook ({key:?}, not {})",
                state.section.sheet, state.section.key
            ));
        }
    }
    Ok(copy)
}

/// Writes the section captures of `sections` (each a section with formulas) from `record`, a
/// capture of a package that holds them, and the saved worksheet part of each that keeps one from
/// `saved` (the package Excel saved), when given. Each section's inventory in the record must hold
/// as many cells as its worksheet part.
fn write_sections(
    case: &OracleCase,
    record: &CaptureRecord,
    sections: &[&calc_verdict::WorkbookSection],
    saved: Option<&[u8]>,
    keep_saved: &[String],
) -> Result<Vec<String>, String> {
    let saved_parts = saved.map(package_parts).transpose()?;
    let saved_sections = saved_parts.as_ref().map(workbook_sections).transpose()?;
    let mut written = Vec::new();
    for section in sections {
        let split = section_record(
            record,
            &section.sheet,
            &case.section_case(&section.sheet),
            &section.key,
        );
        if split.formulas.len() as u64 != section.formula_cells {
            return Err(format!(
                "the capture lists {} formula cells on {}, whose worksheet part holds {}: the cells do not map one to one",
                split.formulas.len(),
                section.sheet,
                section.formula_cells
            ));
        }
        let encoded = split.encode().map_err(|e| {
            format!(
                "the section capture of {} cannot be written: {e}",
                section.sheet
            )
        })?;
        let path = case.section_capture(&section.sheet);
        write(&path, encoded.as_bytes())?;
        written.push(path.display().to_string());
        let saved_path = case.section_saved(&section.sheet);
        if saved_path.is_file() || keep_saved.contains(&section.sheet) {
            let (Some(parts), Some(saved_sections)) = (&saved_parts, &saved_sections) else {
                return Err(format!(
                    "{} keeps the worksheet Excel saved, and no saved package was given",
                    section.sheet
                ));
            };
            let part = saved_sections
                .iter()
                .find(|s| s.sheet == section.sheet)
                .map(|s| s.part.clone())
                .ok_or_else(|| format!("the saved package has no sheet {}", section.sheet))?;
            let xml = std::str::from_utf8(&parts[&part]).map_err(|e| format!("{part}: {e}"))?;
            write(&saved_path, keyed_saved_sheet(xml, &section.key).as_bytes())?;
            written.push(saved_path.display().to_string());
        }
    }
    Ok(written)
}

fn run_pending(args: &PendingArgs) -> u8 {
    let root = crate::corpus::repository_root();
    let (found, lines) = match pending(&root) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("xtask capture: {e}");
            return EXIT_FAILED;
        }
    };
    for line in &lines {
        println!("xtask capture: {line}");
    }
    if found.is_empty() {
        println!("xtask capture: nothing is pending");
        return 0;
    }
    if args.list {
        return 0;
    }
    let mut failed = Vec::new();
    for item in &found {
        let case = item.case();
        let result = match item {
            Pending::Whole { case, case_id } => capture_whole(&root, case, case_id, args),
            Pending::Sections {
                case,
                bytes,
                pending,
            } => capture_sections(&root, case, bytes, pending, args),
        };
        match result {
            Ok(written) => {
                for path in written {
                    println!("xtask capture: wrote {path}");
                }
            }
            Err(None) => return EXIT_UNAVAILABLE,
            Err(Some(e)) => {
                eprintln!(
                    "xtask capture: {} is still awaiting capture: {e}",
                    case.name
                );
                failed.push(case.name.clone());
            }
        }
    }
    if failed.is_empty() {
        println!("xtask capture: everything pending was captured");
        0
    } else {
        eprintln!(
            "xtask capture: {} of {} workbooks are still awaiting capture: {}",
            failed.len(),
            found.len(),
            failed.join(", ")
        );
        EXIT_FAILED
    }
}

fn capture_whole(
    root: &Path,
    case: &OracleCase,
    case_id: &str,
    args: &PendingArgs,
) -> Result<Vec<String>, Option<String>> {
    let bytes = fs::read(&case.fixture)
        .map_err(|e| Some(format!("cannot read {}: {e}", case.fixture.display())))?;
    let cells = package_parts(&bytes)
        .and_then(|parts| workbook_sections(&parts))
        .map_or(0, |sections| sections.iter().map(|s| s.formula_cells).sum());
    let policy = with_workbook_iteration(CalculationPolicy::default(), &bytes);
    let outcome = capture_with_attempts(
        root,
        case_id,
        &case.fixture,
        policy,
        timeout_for(args, cells),
        args.attempts,
    )?;
    let path = case.whole_capture();
    write(&path, outcome.encoded.as_bytes()).map_err(Some)?;
    let mut written = vec![path.display().to_string()];
    let saved = case.captures.join("oracle.saved.xlsx");
    if saved.is_file() {
        let package = fs::read(&outcome.output_package).map_err(|e| {
            Some(format!(
                "cannot read the package Excel saved, {}: {e}",
                outcome.output_package.display()
            ))
        })?;
        write(&saved, &package).map_err(Some)?;
        written.push(saved.display().to_string());
    }
    Ok(written)
}

fn capture_sections(
    root: &Path,
    case: &OracleCase,
    bytes: &[u8],
    pending: &[SectionState],
    args: &PendingArgs,
) -> Result<Vec<String>, Option<String>> {
    let mut failed = Vec::new();
    for (n, state) in pending.iter().enumerate() {
        let section = &state.section;
        println!(
            "xtask capture: {}: section {} of {}: {} ({} formula cells)",
            case.name,
            n + 1,
            pending.len(),
            section.sheet,
            section.formula_cells
        );
        match capture_section(root, case, bytes, state, args) {
            Ok(written) => {
                for path in written {
                    println!("xtask capture: wrote {path}");
                }
            }
            Err(None) => return Err(None),
            Err(Some(e)) => {
                eprintln!(
                    "xtask capture: {}: the section {} is still awaiting capture: {e}",
                    case.name, section.sheet
                );
                failed.push(section.sheet.clone());
            }
        }
    }
    if failed.is_empty() {
        Ok(Vec::new())
    } else {
        Err(Some(format!(
            "{} of {} sections are still awaiting capture: {}",
            failed.len(),
            pending.len(),
            failed.join(", ")
        )))
    }
}

/// One pending section, captured in its own Excel session and written as soon as it is taken, so
/// a section that fails costs only itself and the next run takes up only what is still pending.
fn capture_section(
    root: &Path,
    case: &OracleCase,
    bytes: &[u8],
    state: &SectionState,
    args: &PendingArgs,
) -> Result<Vec<String>, Option<String>> {
    let copy = pending_copy(bytes, std::slice::from_ref(state)).map_err(Some)?;
    // The copy is named as the workbook, so a formula that reads its own file's name reads the
    // same name as in the workbook.
    let input = default_work_dir_in(&root.join("target").join("c")).join("input");
    let fixture = input.join(case.fixture.file_name().unwrap_or_default());
    write(&fixture, &copy).map_err(Some)?;
    let policy = with_workbook_iteration(CalculationPolicy::default(), &copy);
    let outcome = capture_with_attempts(
        root,
        &case.section_case(&state.section.sheet),
        &fixture,
        policy,
        timeout_for(args, state.section.formula_cells),
        args.attempts,
    )?;
    let saved = fs::read(&outcome.output_package).ok();
    let written = write_sections(
        case,
        &outcome.record,
        &[&state.section],
        saved.as_deref(),
        &[],
    )
    .map_err(Some)?;
    Ok(written)
}

#[derive(Debug, PartialEq)]
struct SplitArgs {
    fixture: PathBuf,
    capture: PathBuf,
    saved: Option<PathBuf>,
    keep_saved: Vec<String>,
    pending_only: bool,
    only: Vec<String>,
}

fn parse_split(args: &[String]) -> Result<SplitArgs, String> {
    let (mut fixture, mut capture, mut saved) = (None, None, None);
    let mut keep_saved = Vec::new();
    let mut pending_only = false;
    let mut only = Vec::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        if flag == "--pending-only" {
            pending_only = true;
            continue;
        }
        let value = rest
            .next()
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--fixture" => fixture = Some(PathBuf::from(value)),
            "--capture" => capture = Some(PathBuf::from(value)),
            "--saved" => saved = Some(PathBuf::from(value)),
            "--keep-saved" => keep_saved.push(value),
            "--only" => only.push(value),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(SplitArgs {
        fixture: fixture.ok_or("--fixture is required")?,
        capture: capture.ok_or("--capture is required")?,
        saved,
        keep_saved,
        pending_only,
        only,
    })
}

fn run_split(args: &SplitArgs) -> Result<Vec<String>, String> {
    let root = crate::corpus::repository_root();
    let fixture = fs::canonicalize(&args.fixture)
        .map_err(|e| format!("cannot find {}: {e}", args.fixture.display()))?;
    let case = oracle_cases(&root)?
        .into_iter()
        .find(|case| fs::canonicalize(&case.fixture).is_ok_and(|path| path == fixture))
        .ok_or_else(|| {
            format!(
                "{} is not an oracle workbook of this repository",
                args.fixture.display()
            )
        })?;
    let bytes = fs::read(&case.fixture)
        .map_err(|e| format!("cannot read {}: {e}", case.fixture.display()))?;
    let workbook_hash = sha256_hex(&bytes);
    let input_hash = calc_oracle_excel::capture_source_sha256(&case.fixture, &workbook_hash)?;
    if input_hash != workbook_hash {
        return Err(
            "authored-entry captures cannot be split into ordinary section captures".to_owned(),
        );
    }
    if !case.sectioned() {
        fs::create_dir_all(case.captures.join(crate::oracle_cases::SECTIONS_DIR))
            .map_err(|e| format!("cannot create the sections folder of {}: {e}", case.name))?;
    }
    let record = fs::read(&args.capture)
        .map_err(|e| format!("cannot read {}: {e}", args.capture.display()))
        .and_then(|bytes| {
            CaptureRecord::decode(&bytes)
                .map_err(|e| format!("{} does not decode: {e}", args.capture.display()))
        })?;
    let digest = format!("sha256:{input_hash}");
    if record.source != digest {
        return Err(format!(
            "the capture is of {}, and {} is {digest}: its cells do not map one to one onto the workbook's sections",
            record.source,
            case.fixture.display()
        ));
    }
    let saved = args
        .saved
        .as_ref()
        .map(|path| fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display())))
        .transpose()?;
    if let (Some(saved), Some(session)) = (&saved, &record.session) {
        let hash = format!("sha256:{}", sha256_hex(saved));
        if session.output_package.as_deref() != Some(hash.as_str()) {
            return Err(format!(
                "the saved package is {hash}, and the capture names {:?} as the package Excel saved",
                session.output_package
            ));
        }
    }
    let sections = workbook_sections(&package_parts(&bytes)?)?;
    let current: BTreeSet<String> = if args.pending_only {
        section_states(&case, &bytes)?
            .0
            .into_iter()
            .filter(|s| s.capture.is_some())
            .map(|s| s.section.sheet)
            .collect()
    } else {
        BTreeSet::new()
    };
    for name in &args.only {
        if !sections
            .iter()
            .any(|s| s.formula_cells > 0 && &s.sheet == name)
        {
            return Err(format!(
                "--only {name}: the workbook has no sheet {name} with formulas"
            ));
        }
    }
    let with_formulas: Vec<&calc_verdict::WorkbookSection> = sections
        .iter()
        .filter(|s| s.formula_cells > 0 && !current.contains(&s.sheet))
        .filter(|s| args.only.is_empty() || args.only.contains(&s.sheet))
        .collect();
    let written = write_sections(
        &case,
        &record,
        &with_formulas,
        saved.as_deref(),
        &args.keep_saved,
    )?;
    Ok(written)
}

/// `cargo xtask capture --pending ...` (`args` after `--pending`).
pub fn run_pending_command(args: &[String]) -> u8 {
    match parse_pending(args) {
        Ok(args) => run_pending(&args),
        Err(e) => {
            eprintln!("xtask capture: {e}\n{USAGE}");
            EXIT_USAGE
        }
    }
}

/// `cargo xtask capture --split ...` (`args` after `--split`).
pub fn run_split_command(args: &[String]) -> u8 {
    let parsed = match parse_split(args) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("xtask capture: {e}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    match run_split(&parsed) {
        Ok(written) => {
            for path in written {
                println!("xtask capture: wrote {path}");
            }
            0
        }
        Err(e) => {
            eprintln!("xtask capture: {e}");
            EXIT_FAILED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn pending_arguments_are_read_with_their_defaults() {
        assert_eq!(
            parse_pending(&[]).unwrap(),
            PendingArgs {
                list: false,
                timeout: None,
                attempts: 3
            }
        );
        assert_eq!(
            parse_pending(&args("--list --timeout 60 --attempts 1")).unwrap(),
            PendingArgs {
                list: true,
                timeout: Some(Duration::from_secs(60)),
                attempts: 1
            }
        );
        for bad in ["--attempts 0", "--timeout x", "--timeout", "--all"] {
            assert!(parse_pending(&args(bad)).is_err(), "{bad}");
        }
        assert!(parse_split(&args("--fixture f.xlsx")).is_err());
        assert_eq!(
            parse_split(&args("--fixture f --capture c --keep-saved s")).unwrap(),
            SplitArgs {
                fixture: PathBuf::from("f"),
                capture: PathBuf::from("c"),
                saved: None,
                keep_saved: vec!["s".to_owned()],
                pending_only: false,
                only: Vec::new(),
            }
        );
        assert!(
            parse_split(&args("--fixture f --capture c --pending-only"))
                .unwrap()
                .pending_only
        );
        assert_eq!(
            parse_split(&args("--fixture f --capture c --only a --only b"))
                .unwrap()
                .only,
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    /// The copy a capture of some of the catalogue's sections opens keeps every pending section's
    /// key and what it reads, and takes out the cells of the other sections only.
    #[test]
    fn a_pending_copy_keeps_the_pending_sections_keys() {
        let catalogue =
            include_bytes!("../../../crates/calc-func/tests/fixtures/catalogue/oracle.xlsx");
        let parts = calc_verdict::stored_package_parts(catalogue).unwrap();
        let sections = workbook_sections(&parts).unwrap();
        let pending: Vec<SectionState> = sections
            .iter()
            .filter(|s| s.sheet == "prefix_metadata" || s.sheet == "f1_power")
            .map(|section| SectionState {
                section: section.clone(),
                capture: None,
                awaiting: Some("test".to_owned()),
            })
            .collect();
        assert_eq!(pending.len(), 2);
        let copy = pending_copy(catalogue, &pending).unwrap();
        let copied =
            workbook_sections(&calc_verdict::stored_package_parts(&copy).unwrap()).unwrap();
        assert_eq!(copied.len(), sections.len());
        for (before, after) in sections.iter().zip(&copied) {
            assert_eq!(before.sheet, after.sheet);
            let kept = before.formula_cells == 0
                || pending.iter().any(|p| p.section.sheet == before.sheet)
                || pending
                    .iter()
                    .any(|p| p.section.reads.contains(&before.sheet));
            if kept {
                assert_eq!(before.key, after.key, "{}", before.sheet);
                assert_eq!(before.formula_cells, after.formula_cells);
            } else {
                assert_eq!(after.formula_cells, 0, "{}", before.sheet);
            }
        }
    }
}
