//! `cargo xtask bench <dir> [--runs <n>] [--out <dir>] [--cache <dir>] [--edit
//! <workbook>=<sheet>!<cell>]... [--excel] [--no-dna] [--timeout <s>]`: the time and memory DNA
//! Calc and Excel take to open and calculate each workbook of a folder (bead dc-10d9), built on
//! `cargo xtask corpus`: the same listing of workbooks, the same cached captures, and the same
//! perf-profile xtask, in which each DNA Calc run is its own process.
//!
//! **DNA Calc**, `--runs` times per workbook (default 5), each run in a new perf-profile xtask process
//! (`bench --one`), through calc-host's product calls with no read observer
//! ([`calc_host::Unobserved`]), recalculating on `--threads` threads (default
//! `calc_platform_desktop::default_threads`: the physical cores, at most eight); with
//! `--verify-threads` a second host on one thread takes every step and every formula cell is
//! compared after each recalculation:
//!
//! 1. open: [`HostState::open`] of the workbook's bytes, read before the clock starts, with the
//!    options `cargo xtask compare` opens it with (the capture's locale, automatic calculation).
//!    The parent decodes the capture once and hands each run its locale and formula inventory in
//!    a small file, so that no run's peak memory holds a decoded capture;
//! 2. cold build (`rebuild_s` in JSON): the first recalculation, which compiles every formula,
//!    builds the graph, orders and evaluates every formula, timed as its first [`step`] with a
//!    budget of no evaluations (binding and ordering) and the step that evaluates and publishes;
//! 3. project: [`project`] of every formula cell of the capture's inventory, counting the cells
//!    projected as calculated by the engine. A run in which any is not counts as failed: a file
//!    cache, a stale value or a refusal is never timed as a calculation;
//! 4. full: [`calculate_full`] and the recalculation of every formula with the plans and the graph
//!    kept (Excel's `CalculateFull`), timed the same way, and projected and counted again, done
//!    `--repeat` times (default 1) in the one process;
//! 5. edit, when `--edit` names a cell of the workbook holding a number: the cell set to that
//!    number times 1.01 (1 when it holds 0) by [`dispatch`], then the recalculation of the dirty
//!    closure, then the cell set back and recalculated, each timed (commit and recalculation) with
//!    the number of formulas the recalculation published; the pair is done `--repeat` times;
//! 6. warm Rebuild: the existing HostSession Rebuild command discards the bound graph and
//!    recalculates every formula after the preceding phases; both thread lanes run it and
//!    every formula is projected and checked again. Its clock includes dependency cleanup.
//! 7. memory: on Windows, peak working set/private bytes (`GetProcessMemoryInfo`);
//!    on Linux, resident bytes by phase and peak RSS (`/proc/self/status` VmRSS/VmHWM).
//!    RSS is not private/committed memory; unavailable counters remain null.
//!
//! Each phase is timed in wall seconds and, beside them, in the cycles the measuring thread ran
//! (`QueryThreadCycleTime`), which other processes' load changes far less: on a shared machine,
//! two builds are compared by their cycles, and Excel by wall time.
//!
//! **Excel**, with `--excel`: one new Excel instance per workbook, started and stopped by
//! calc-oracle-excel's timing worker under the capture isolation ([`time_workbook`]), which opens
//! the workbook `--runs` times without calculating (manual calculation), and times
//! `CalculateFullRebuild`, `CalculateFull` and the same one-cell edits (assignment and
//! `Calculate`), and reads the instance's peak working set and peak private bytes.
//!
//! With `--excel-check` (bead dc-dxm) the timing worker also shows what CalculateFull calculates:
//! on a copy of the workbook whose formula cells all cache the sentinel -987654321, it counts the
//! cells showing the sentinel after opening, after Calculate and after CalculateFull, with
//! multi-threaded calculation as Excel has it and switched off (and set back before Excel quits).
//!
//! **Load.** The machine is shared, so the command reads, before and after each workbook's runs,
//! the processor load (`Win32_Processor.LoadPercentage`) and how many cargo, rustc and Excel
//! processes are running, and records them beside the times.
//!
//! It writes `bench.json` (every sample) and `bench.md` (per workbook and metric: the median, the
//! lowest and highest sample, and the number of samples) to `--out`, by default `target/bench`.
//! Exit codes: 0 when both were written; 1 when they cannot be, or the perf build fails; 2 on
//! a usage error.

use crate::corpus::{Cache, DEFAULT_CACHE, Entry, compare_program, excel_build, folder_workbooks};
use calc_host::value::{CellKey, SheetId};
use calc_host::{
    CalcMode, CellEdit, CellInput, FiniteNumber, HostCommand, HostOptions, HostProgress, HostState,
    HostSession, SkinIntent,
    MemorySource, OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest, Unobserved,
    WorkBudget, calculate_full, dispatch, project, step,
};
use calc_oracle_excel::{TimedEdit, TimingRequest, time_workbook, windows_powershell};
use calc_verdict::{CaptureRecord, sha256_hex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask bench <dir> [--runs <n>] [--repeat <k>] [--threads <n>] [--verify-threads] [--out <dir>] [--cache <dir>] [--edit <workbook>=<sheet>!<cell>]... [--excel] [--excel-check] [--no-dna] [--timeout <s>]";

const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const DEFAULT_RUNS: u32 = 5;
const DEFAULT_OUT: &str = "target/bench";
/// The timeout of one workbook's Excel timing.
const DEFAULT_TIMEOUT_S: u64 = 3600;
/// The prefix of the line a `bench --one` process prints its run on.
const LINE: &str = "dnacalc-bench/1 ";

// Share the host thread tests' strict comparison: independent opens have distinct document
// namespaces, while values, incarnations, ticks, qualifications and certificates must agree.
#[path = "../../../crates/calc-host/tests/contract_support/mod.rs"]
mod independent_workbooks;

#[derive(Debug, PartialEq)]
struct Args {
    dir: PathBuf,
    runs: u32,
    repeat: u32,
    threads: usize,
    verify_threads: bool,
    out: PathBuf,
    cache: PathBuf,
    edits: Vec<(String, String)>,
    excel: bool,
    excel_check: bool,
    dna: bool,
    timeout: Duration,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut dir, mut runs, mut out, mut cache, mut timeout) = (None, None, None, None, None);
    let mut repeat = None;
    let mut threads = None;
    let mut parsed = Args {
        dir: PathBuf::new(),
        runs: DEFAULT_RUNS,
        repeat: 1,
        threads: calc_platform_desktop::default_threads(),
        verify_threads: false,
        out: PathBuf::from(DEFAULT_OUT),
        cache: PathBuf::from(DEFAULT_CACHE),
        edits: Vec::new(),
        excel: false,
        excel_check: false,
        dna: true,
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_S),
    };
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
            "--excel" => parsed.excel = true,
            "--excel-check" => {
                parsed.excel = true;
                parsed.excel_check = true;
            }
            "--no-dna" => parsed.dna = false,
            "--runs" => once(&mut runs, "--runs", value("--runs")?)?,
            "--repeat" => once(&mut repeat, "--repeat", value("--repeat")?)?,
            "--threads" => once(&mut threads, "--threads", value("--threads")?)?,
            "--verify-threads" => parsed.verify_threads = true,
            "--out" => once(&mut out, "--out", value("--out")?)?,
            "--cache" => once(&mut cache, "--cache", value("--cache")?)?,
            "--timeout" => once(&mut timeout, "--timeout", value("--timeout")?)?,
            "--edit" => {
                let text = value("--edit")?;
                let (workbook, cell) = text
                    .split_once('=')
                    .ok_or_else(|| format!("--edit {text:?} is not <workbook>=<sheet>!<cell>"))?;
                split_cell(cell)?;
                parsed.edits.push((workbook.to_owned(), cell.to_owned()));
            }
            flag if flag.starts_with("--") => return Err(format!("unknown argument {flag:?}")),
            folder => once(&mut dir, "the workbook folder", folder.to_owned())?,
        }
    }
    parsed.dir = PathBuf::from(dir.ok_or("give a workbook folder")?);
    if let Some(text) = runs {
        parsed.runs = text
            .parse::<u32>()
            .ok()
            .filter(|runs| *runs > 0)
            .ok_or_else(|| format!("--runs {text:?} is not a positive number"))?;
    }
    if let Some(text) = threads {
        parsed.threads = text
            .parse::<usize>()
            .ok()
            .filter(|threads| *threads > 0)
            .ok_or_else(|| format!("--threads {text:?} is not a positive number"))?;
    }
    if let Some(text) = repeat {
        parsed.repeat = text
            .parse::<u32>()
            .ok()
            .filter(|repeat| *repeat > 0)
            .ok_or_else(|| format!("--repeat {text:?} is not a positive number"))?;
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
    if !parsed.dna && !parsed.excel {
        return Err("--no-dna without --excel times nothing".to_owned());
    }
    Ok(parsed)
}

/// `<sheet>!<cell>` as the sheet's name and the cell's row and column (1-based); the sheet is
/// everything before the last `!`.
fn split_cell(text: &str) -> Result<(String, u32, u32), String> {
    let (sheet, cell) = text
        .rsplit_once('!')
        .ok_or_else(|| format!("{text:?} is not <sheet>!<cell>"))?;
    let digits = cell
        .find(|c: char| c.is_ascii_digit())
        .ok_or_else(|| format!("{cell:?} is not an A1 cell"))?;
    let (letters, number) = cell.split_at(digits);
    if letters.is_empty() || letters.len() > 3 || !letters.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(format!("{cell:?} is not an A1 cell"));
    }
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    let row = number
        .parse::<u32>()
        .ok()
        .filter(|row| *row > 0)
        .ok_or_else(|| format!("{cell:?} is not an A1 cell"))?;
    Ok((sheet.to_owned(), row, column))
}

/// The number an edit sets a cell holding `held` to.
fn edited(held: f64) -> f64 {
    if held != 0.0 { held * 1.01 } else { 1.0 }
}

/// The peak working set and the peak private bytes of this process, in bytes.
#[cfg(windows)]
fn peak_memory() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    counters.cb = size;
    // SAFETY: the pseudo-handle of this process needs no closing, and the counters are a
    // structure of the size passed.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    (ok != 0).then(|| {
        (
            counters.PeakWorkingSetSize as u64,
            counters.PeakPagefileUsage as u64,
        )
    })
}

#[cfg(not(windows))]
fn peak_memory() -> Option<(u64, u64)> {
    None
}

/// The private bytes this process holds now (`PagefileUsage`), to show which phase holds the
/// memory (dc-dxm).
#[cfg(windows)]
fn private_now() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    counters.cb = size;
    // SAFETY: as in peak_memory.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    (ok != 0).then_some(counters.PagefileUsage as u64)
}

#[cfg(not(windows))]
fn private_now() -> Option<u64> {
    None
}

/// Linux reports these fields in KiB. Missing, malformed or overflowing counters stay unknown.
#[cfg(any(target_os = "linux", test))]
fn status_bytes(status: &str, field: &str) -> Option<u64> {
    let mut fields = status
        .lines()
        .find_map(|line| line.strip_prefix(field))?
        .split_whitespace();
    let kib = fields.next()?.parse::<u64>().ok()?;
    (fields.next()? == "kB" && fields.next().is_none()).then_some(())?;
    kib.checked_mul(1024)
}

#[cfg(target_os = "linux")]
fn resident(field: &str) -> Option<u64> {
    status_bytes(&fs::read_to_string("/proc/self/status").ok()?, field)
}
#[cfg(not(target_os = "linux"))]
fn resident(_: &str) -> Option<u64> {
    None
}

/// The processor cycles this thread has run (`QueryThreadCycleTime`), which other processes'
/// load changes far less than wall time: the measure two builds are compared by on a shared
/// machine.
#[cfg(windows)]
fn thread_cycles() -> u64 {
    use windows_sys::Win32::System::Threading::GetCurrentThread;
    use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;
    let mut cycles = 0u64;
    // SAFETY: the pseudo-handle of this thread needs no closing; the count is a u64.
    unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    cycles
}

#[cfg(not(windows))]
fn thread_cycles() -> u64 {
    0
}

/// Unsupported platform counters are unknown, rather than measured zero cycles.
fn available_cycles(value: f64) -> Option<f64> {
    cfg!(windows).then_some(value)
}

/// A phase's wall time and thread cycles.
struct Clock {
    wall: Instant,
    cycles: u64,
}

impl Clock {
    fn start() -> Clock {
        Clock {
            wall: Instant::now(),
            cycles: thread_cycles(),
        }
    }

    /// Seconds, and billions of this thread's cycles, since the start.
    fn stop(&self) -> (f64, f64) {
        (
            self.wall.elapsed().as_secs_f64(),
            (thread_cycles() - self.cycles) as f64 / 1e9,
        )
    }
}

/// One recalculation's two phases: binding and ordering (the first step, of no evaluations),
/// then evaluating and publishing.
struct Phases {
    order: (f64, f64),
    evaluate: (f64, f64),
    published: usize,
}

/// The formulas a published recalculation calculated, or why it did not publish. `ask` (a full
/// recalculation's [`calculate_full`], say) runs inside the first phase's clock.
fn recalculate(host: &mut HostState, ask: impl FnOnce(&mut HostState)) -> Result<Phases, String> {
    let clock = Clock::start();
    ask(host);
    let mut progress = step(host, WorkBudget::evaluations(0), &mut Unobserved);
    let order = clock.stop();
    let clock = Clock::start();
    while let HostProgress::Calculating { .. } = progress {
        progress = step(host, WorkBudget::UNLIMITED, &mut Unobserved);
    }
    let evaluate = clock.stop();
    match progress {
        HostProgress::Published(delta) => Ok(Phases {
            order,
            evaluate,
            published: delta.changes().len(),
        }),
        other => Err(format!("the recalculation did not publish: {other:?}")),
    }
}

/// Exercise the application's actual Rebuild admission on an already bound graph.
fn warm_rebuild(session: &mut HostSession) -> Result<Phases, String> {
    session.set_calculation_budget(WorkBudget::evaluations(0));
    let clock = Clock::start();
    let receipt = session.dispatch(SkinIntent::Rebuild);
    let order = clock.stop();
    if !receipt.accepted() {
        return Err(format!("warm Rebuild was refused: {:?}", receipt.error));
    }
    let mut progress = receipt.recalc.ok_or("warm Rebuild supplied no progress")?;
    session.set_calculation_budget(WorkBudget::UNLIMITED);
    let clock = Clock::start();
    while matches!(progress, HostProgress::Calculating { .. }) {
        progress = session.advance_calculation();
    }
    let evaluate = clock.stop();
    match progress {
        HostProgress::Published(delta) => Ok(Phases { order, evaluate, published: delta.changes().len() }),
        other => Err(format!("warm Rebuild did not publish: {other:?}")),
    }
}

/// Projects `cells` and counts those the engine calculated; the seconds it took.
fn calculated(host: &HostState, request: &ProjectionRequest) -> Result<(f64, usize), String> {
    let started = Instant::now();
    let page = project(host, request).map_err(|e| format!("the projection was refused: {e}"))?;
    let seconds = started.elapsed().as_secs_f64();
    let count = page
        .cells()
        .iter()
        .filter(|cell| {
            matches!(cell.value(), ProjectedValue::Calculated { calculation, .. }
            if !calculation.held_by_circular_rule() && !calculation.reads_external_links())
        })
        .count();
    Ok((seconds, count))
}

/// What a run needs of a workbook's capture, which the parent writes once per workbook so that
/// no run decodes the capture (whose decoding would be part of the run's peak memory): the
/// workbook's hash, the capture's locale settings and its formula inventory. Lines separated by
/// tabs: `source\tsha256:<hex>`, `locale\t<culture>\t<decimal>\t<group>` (or `locale` alone),
/// then one `<sheet>\t<row>\t<column>` per formula cell.
fn write_inventory(capture: &Path, out: &Path) -> Result<(), String> {
    let record = fs::read(capture)
        .map_err(|e| format!("cannot read {}: {e}", capture.display()))
        .and_then(|bytes| {
            CaptureRecord::decode(&bytes).map_err(|e| format!("the capture is refused: {e}"))
        })?;
    let mut text = format!("source\t{}\n", record.source);
    match crate::compare::capture_locale_settings(&record) {
        Some([culture, decimal, thousands]) => {
            text.push_str(&format!("locale\t{culture}\t{decimal}\t{thousands}\n"))
        }
        None => text.push_str("locale\n"),
    }
    for formula in &record.formulas {
        let cell = &formula.cell;
        if cell.sheet().contains(['\t', '\n']) {
            return Err(format!(
                "the sheet name {:?} holds a tab or a line break",
                cell.sheet()
            ));
        }
        text.push_str(&format!(
            "{}\t{}\t{}\n",
            cell.sheet(),
            cell.row(),
            cell.column()
        ));
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(out, text).map_err(|e| format!("cannot write {}: {e}", out.display()))
}

/// How many cells project otherwise, retaining values and qualified provenance while
/// normalizing only the independent opens' owning document namespaces.
fn differences(a: &HostState, b: &HostState, request: &ProjectionRequest) -> Result<usize, String> {
    let left = project(a, request).map_err(|e| format!("the projection was refused: {e}"))?;
    let right = project(b, request).map_err(|e| format!("the projection was refused: {e}"))?;
    Ok(left
        .cells()
        .iter()
        .zip(right.cells())
        .filter(|(x, y)| {
            independent_workbooks::comparable(x.value())
                != independent_workbooks::comparable(y.value())
        })
        .count())
}

/// One DNA Calc run of `fixture` (the module documentation, steps 1 to 6), with the inventory
/// [`write_inventory`] wrote.
fn run_one(
    fixture: &Path,
    inventory: &Path,
    edit: Option<&str>,
    repeat: u32,
    threads: usize,
    verify_threads: bool,
) -> Result<Value, String> {
    let bytes = fs::read(fixture).map_err(|e| format!("cannot read {}: {e}", fixture.display()))?;
    let listing = fs::read_to_string(inventory)
        .map_err(|e| format!("cannot read {}: {e}", inventory.display()))?;
    let mut lines = listing.lines();
    let malformed = || format!("{} is not an inventory", inventory.display());
    if lines.next().and_then(|line| line.strip_prefix("source\t"))
        != Some(&format!("sha256:{}", sha256_hex(&bytes)))
    {
        return Err("the inventory is not of this workbook".to_owned());
    }
    let locale = match lines
        .next()
        .ok_or_else(malformed)?
        .split('\t')
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["locale"] => None,
        ["locale", culture, decimal, thousands] => {
            crate::compare::locale_profile(culture, decimal, thousands)
        }
        _ => return Err(malformed()),
    };
    let mut formulas: Vec<(String, u32, u32)> = Vec::new();
    for line in lines {
        let parse = || -> Option<(String, u32, u32)> {
            let mut fields = line.split('\t');
            let sheet = fields.next()?.to_owned();
            let row = fields.next()?.parse().ok()?;
            let column = fields.next()?.parse().ok()?;
            Some((sheet, row, column))
        };
        formulas.push(parse().ok_or_else(malformed)?);
    }
    let opened = |bytes: Vec<u8>, locale: Option<calc_host::FormatProfile>| {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        HostState::open(
            source,
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                locale,
                ..HostOptions::default()
            },
        )
        .map_err(|e| format!("HostState::open refused the package: {e}"))
    };
    // With --verify-threads, a second host on one thread takes every step the measured one takes,
    // and every formula cell is compared after each recalculation (dc-2m0).
    let mut twin = if verify_threads {
        Some(opened(bytes.clone(), locale.clone())?)
    } else {
        None
    };
    let private_before_open = private_now();
    let rss_before_open = resident("VmRSS:");
    let clock = Clock::start();
    let mut host = opened(bytes, locale)?;
    let open = clock.stop();
    host.set_threads(threads);
    let private_after_open = private_now();
    let rss_after_open = resident("VmRSS:");

    let ids: HashMap<String, SheetId> = host
        .sheets()
        .iter()
        .map(|sheet| (sheet.name().text().to_string_lossy(), sheet.id()))
        .collect();
    let key = |sheet: &str, row: u32, column: u32| -> Option<CellKey> {
        CellKey::new(*ids.get(sheet)?, row, column).ok()
    };
    let cells: Vec<CellKey> = formulas
        .iter()
        .map(|(sheet, row, column)| {
            key(sheet, *row, *column)
                .ok_or_else(|| format!("the workbook holds no cell R{row}C{column} on {sheet}"))
        })
        .collect::<Result<_, _>>()?;
    let inventory = ProjectionRequest { cells };
    let total = inventory.cells.len();
    let all_calculated = |count: usize, what: &str| -> Result<(), String> {
        if count == total {
            Ok(())
        } else {
            Err(format!(
                "after the {what}, {count} of the {total} formula cells were evaluated from live inputs (circular held values and external caches excluded)"
            ))
        }
    };

    let mut compared = 0usize;
    let mut differing = 0usize;
    let mut verify = |host: &HostState, twin: &mut Option<HostState>, ask: fn(&mut HostState)| {
        if let Some(twin) = twin.as_mut() {
            recalculate(twin, ask)?;
            differing += differences(host, twin, &inventory)?;
            compared += 1;
        }
        Ok::<(), String>(())
    };
    let rebuild = recalculate(&mut host, |_| {})?;
    let private_after_rebuild = private_now();
    let rss_after_rebuild = resident("VmRSS:");
    verify(&host, &mut twin, |_| {})?;
    let (project_s, count) = calculated(&host, &inventory)?;
    all_calculated(count, "rebuild")?;

    let mut fulls = Vec::new();
    let mut full_project = 0.0;
    for _ in 0..repeat {
        let full = recalculate(&mut host, calculate_full)?;
        verify(&host, &mut twin, calculate_full)?;
        let (seconds, count) = calculated(&host, &inventory)?;
        all_calculated(count, "full recalculation")?;
        full_project = seconds;
        fulls.push(json!({
            "order_s": full.order.0,
            "evaluate_s": full.evaluate.0,
            "full_s": full.order.0 + full.evaluate.0,
            "order_gcycles": available_cycles(full.order.1),
            "evaluate_gcycles": available_cycles(full.evaluate.1),
            "full_gcycles": available_cycles(full.order.1 + full.evaluate.1),
            "published": full.published,
        }));
    }

    let rss_after_full = resident("VmRSS:");
    let mut edits = Vec::new();
    if let Some(text) = edit {
        let (sheet, row, column) = split_cell(text)?;
        let at = key(&sheet, row, column).ok_or_else(|| format!("the workbook holds no {text}"))?;
        let page = project(&host, &ProjectionRequest { cells: vec![at] })
            .map_err(|e| format!("the projection was refused: {e}"))?;
        let held = match page.cells()[0].input() {
            Some(CellInput::Number(number)) => number.get(),
            other => return Err(format!("{text} holds {other:?}, not a number")),
        };
        let targets = [edited(held), held];
        let mut from = held;
        for to in (0..repeat).flat_map(|_| targets) {
            let number = FiniteNumber::new(to).ok_or("the edited number is not finite")?;
            if let Some(twin) = twin.as_mut() {
                let base = twin.revision();
                dispatch(
                    twin,
                    HostCommand {
                        base,
                        edits: vec![CellEdit::Set {
                            at,
                            input: CellInput::Number(number),
                        }],
                    },
                )
                .map_err(|e| format!("the edit of {text} was refused: {e}"))?;
            }
            let base = host.revision();
            let clock = Clock::start();
            dispatch(
                &mut host,
                HostCommand {
                    base,
                    edits: vec![CellEdit::Set {
                        at,
                        input: CellInput::Number(number),
                    }],
                },
            )
            .map_err(|e| format!("the edit of {text} was refused: {e}"))?;
            let commit = clock.stop();
            let phases = recalculate(&mut host, |_| {})?;
            verify(&host, &mut twin, |_| {})?;
            let (_, count) = calculated(&host, &inventory)?;
            all_calculated(count, "edit")?;
            edits.push(json!({
                "from_bits": format!("{:016x}", from.to_bits()),
                "to_bits": format!("{:016x}", to.to_bits()),
                "commit_s": commit.0,
                "order_s": phases.order.0,
                "evaluate_s": phases.evaluate.0,
                "edit_s": commit.0 + phases.order.0 + phases.evaluate.0,
                "order_gcycles": available_cycles(phases.order.1),
                "evaluate_gcycles": available_cycles(phases.evaluate.1),
                "edit_gcycles": available_cycles(commit.1 + phases.order.1 + phases.evaluate.1),
                "published": phases.published,
            }));
            from = to;
        }
    }
    let mut session = HostSession::new(host);
    let warm = warm_rebuild(&mut session)?;
    if let Some(twin) = twin.take() {
        let mut twin = HostSession::new(twin);
        warm_rebuild(&mut twin)?;
        differing += differences(session.host(), twin.host(), &inventory)?;
        compared += 1;
    }
    let (warm_project_s, warm_count) = calculated(session.host(), &inventory)?;
    all_calculated(warm_count, "warm Rebuild")?;
    let rss_after_warm_rebuild = resident("VmRSS:");
    let memory = peak_memory();
    Ok(json!({
        "threads": threads,
        "thread_compares": compared,
        "thread_differences": differing,
        "formula_cells": total,
        "open_s": open.0,
        "open_gcycles": available_cycles(open.1),
        "rebuild_order_s": rebuild.order.0,
        "rebuild_evaluate_s": rebuild.evaluate.0,
        "rebuild_s": rebuild.order.0 + rebuild.evaluate.0,
        "rebuild_order_gcycles": available_cycles(rebuild.order.1),
        "rebuild_evaluate_gcycles": available_cycles(rebuild.evaluate.1),
        "rebuild_gcycles": available_cycles(rebuild.order.1 + rebuild.evaluate.1),
        "rebuild_published": rebuild.published,
        "warm_rebuild_order_s": warm.order.0,
        "warm_rebuild_evaluate_s": warm.evaluate.0,
        "warm_rebuild_s": warm.order.0 + warm.evaluate.0,
        "warm_rebuild_published": warm.published,
        "warm_project_s": warm_project_s,
        "rss_after_warm_rebuild": rss_after_warm_rebuild,
        "project_s": project_s,
        "fulls": fulls,
        "full_project_s": full_project,
        "edits": edits,
        "peak_working_set": memory.map(|m| m.0),
        "peak_private": memory.map(|m| m.1),
        "private_before_open": private_before_open,
        "private_after_open": private_after_open,
        "private_after_rebuild": private_after_rebuild,
        "private_at_end": private_now(),
        "rss_source": if cfg!(target_os = "linux") { Some("/proc/self/status; bytes; includes shared resident pages") } else { None },
        "rss_before_open": rss_before_open,
        "rss_after_open": rss_after_open,
        "rss_after_rebuild": rss_after_rebuild,
        "rss_after_full": rss_after_full,
        "rss_at_end": resident("VmRSS:"),
        "peak_rss": resident("VmHWM:"),
        "thread_cycles_available": cfg!(windows),
    }))
}

/// The processor load and the cargo, rustc and Excel processes running now.
fn machine_load() -> Value {
    let script = "$l = (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average; $p = Get-Process; $c = @($p | Where-Object { $_.ProcessName -in @('cargo','rustc') }).Count; $e = @($p | Where-Object { $_.ProcessName -eq 'EXCEL' }).Count; Write-Output \"$l $c $e\"";
    let output = Command::new(windows_powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output();
    let text = output
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    let parts: Vec<&str> = text.split_whitespace().collect();
    match parts.as_slice() {
        [load, cargo, excel] => json!({
            "cpu_load_percent": load.parse::<f64>().ok(),
            "cargo_rustc_processes": cargo.parse::<u64>().ok(),
            "excel_processes": excel.parse::<u64>().ok(),
        }),
        _ => json!({ "unread": text }),
    }
}

/// The median, lowest and highest of `samples`, and how many there are.
fn stats(samples: &[f64]) -> Option<Value> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    Some(json!({ "median": median, "min": sorted[0], "max": sorted[n - 1], "n": n }))
}

/// The samples of `field` over `runs`, and over each of their edits when `edits` is set.
fn samples(runs: &[Value], field: &str, list: Option<&str>) -> Vec<f64> {
    match list {
        Some(list) => runs
            .iter()
            .flat_map(|run| run[list].as_array().cloned().unwrap_or_default())
            .filter_map(|sample| sample[field].as_f64())
            .collect(),
        None => runs.iter().filter_map(|run| run[field].as_f64()).collect(),
    }
}

/// One workbook's DNA Calc runs, each in its own process.
fn dna_runs(
    program: &Path,
    entry: &Entry,
    inventory: &Path,
    edit: Option<&str>,
    runs: u32,
    repeat: u32,
    threads: usize,
    verify_threads: bool,
) -> Vec<Value> {
    (0..runs)
        .map(|_| {
            let mut command = Command::new(program);
            command
                .args(["bench", "--one", "--fixture"])
                .arg(&entry.path)
                .arg("--inventory")
                .arg(inventory);
            if let Some(edit) = edit {
                command.args(["--edit", edit]);
            }
            command.args(["--repeat", &repeat.to_string()]);
            command.args(["--threads", &threads.to_string()]);
            command.args(["--verify-threads", if verify_threads { "1" } else { "0" }]);
            match command.output() {
                Ok(output) => String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find_map(|line| line.strip_prefix(LINE))
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .unwrap_or_else(|| {
                        json!({ "failed": format!(
                            "the run ended with {} and printed no result: {}",
                            output.status,
                            crate::cargo::tail(&output.stderr, 3).trim()
                        ) })
                    }),
                Err(e) => json!({ "failed": format!("cannot start {}: {e}", program.display()) }),
            }
        })
        .collect()
}

/// One workbook's Excel timing.
fn excel_timing(args: &Args, entry: &Entry, edit: Option<&str>) -> Value {
    let edit = match edit.map(split_cell).transpose() {
        Ok(edit) => edit.map(|(sheet, row, column)| TimedEdit { sheet, row, column }),
        Err(e) => return json!({ "failed": e }),
    };
    let work_dir = args.out.join("excel-work").join(format!(
        "{}-{}",
        crate::corpus::file_safe(&entry.name),
        crate::accept::utc_now().replace(':', "")
    ));
    match time_workbook(&TimingRequest {
        fixture: entry.path.clone(),
        work_dir,
        runs: args.runs,
        edit,
        check_full: args.excel_check,
        timeout: args.timeout,
    }) {
        Ok(outcome) => {
            let mut result = outcome.result;
            result["excel"] = json!(outcome.run.excel.describe(outcome.run.excel_pid));
            for run in result["runs"].as_array_mut().into_iter().flatten() {
                for edit in run["edits"].as_array_mut().into_iter().flatten() {
                    let total = edit["assign_s"].as_f64().unwrap_or(0.0)
                        + edit["calculate_s"].as_f64().unwrap_or(0.0);
                    edit["edit_s"] = json!(total);
                }
            }
            result
        }
        Err(e) => json!({ "failed": e.to_string() }),
    }
}

const MB: f64 = 1024.0 * 1024.0;

/// A table cell for `stats` of seconds or bytes.
fn cell(stats: &Option<Value>, scale: f64, digits: usize) -> String {
    match stats {
        None => "-".to_owned(),
        Some(s) => {
            let get = |k: &str| s[k].as_f64().unwrap_or(f64::NAN) / scale;
            format!(
                "{:.digits$} ({:.digits$}–{:.digits$}, n={})",
                get("median"),
                get("min"),
                get("max"),
                s["n"]
            )
        }
    }
}

/// The per-workbook statistics of both systems.
fn workbook_stats(dna: &[Value], excel: &Value) -> Value {
    let ok: Vec<Value> = dna
        .iter()
        .filter(|run| run.get("failed").is_none())
        .cloned()
        .collect();
    let excel_runs: Vec<Value> = excel["runs"].as_array().cloned().unwrap_or_default();
    let one = |field: &str| {
        excel[field]
            .as_f64()
            .map(|v| json!({ "median": v, "min": v, "max": v, "n": 1 }))
    };
    json!({
        "dna": {
            "runs": dna.len(),
            "failed_runs": dna.len() - ok.len(),
            "open_s": stats(&samples(&ok, "open_s", None)),
            "rebuild_s": stats(&samples(&ok, "rebuild_s", None)),
            "rebuild_order_s": stats(&samples(&ok, "rebuild_order_s", None)),
            "rebuild_evaluate_s": stats(&samples(&ok, "rebuild_evaluate_s", None)),
            "warm_rebuild_s": stats(&samples(&ok, "warm_rebuild_s", None)),
            "warm_rebuild_order_s": stats(&samples(&ok, "warm_rebuild_order_s", None)),
            "warm_rebuild_evaluate_s": stats(&samples(&ok, "warm_rebuild_evaluate_s", None)),
            "warm_project_s": stats(&samples(&ok, "warm_project_s", None)),
            "full_s": stats(&samples(&ok, "full_s", Some("fulls"))),
            "full_order_s": stats(&samples(&ok, "order_s", Some("fulls"))),
            "full_evaluate_s": stats(&samples(&ok, "evaluate_s", Some("fulls"))),
            "project_s": stats(&samples(&ok, "project_s", None)),
            "edit_s": stats(&samples(&ok, "edit_s", Some("edits"))),
            "edit_published": stats(&samples(&ok, "published", Some("edits"))),
            "open_gcycles": stats(&samples(&ok, "open_gcycles", None)),
            "rebuild_gcycles": stats(&samples(&ok, "rebuild_gcycles", None)),
            "full_gcycles": stats(&samples(&ok, "full_gcycles", Some("fulls"))),
            "full_order_gcycles": stats(&samples(&ok, "order_gcycles", Some("fulls"))),
            "full_evaluate_gcycles": stats(&samples(&ok, "evaluate_gcycles", Some("fulls"))),
            "edit_gcycles": stats(&samples(&ok, "edit_gcycles", Some("edits"))),
            "peak_working_set": stats(&samples(&ok, "peak_working_set", None)),
            "peak_private": stats(&samples(&ok, "peak_private", None)),
            "peak_rss": stats(&samples(&ok, "peak_rss", None)),
        },
        "excel": {
            "open_s": stats(&samples(&excel_runs, "open_s", None)),
            "rebuild_s": stats(&samples(&excel_runs, "rebuild_s", None)),
            "full_s": stats(&samples(&excel_runs, "full_s", None)),
            "noop_s": stats(&samples(&excel_runs, "noop_s", None)),
            "edit_s": stats(&samples(&excel_runs, "edit_s", Some("edits"))),
            "peak_working_set": one("peak_working_set"),
            "peak_private": one("peak_private"),
            "empty_working_set": one("empty_working_set"),
        },
    })
}

fn markdown(run: &Value, workbooks: &[Value]) -> String {
    let mut out = String::from("# DNA Calc and Excel: open, calculation and memory\n\n");
    out.push_str("Each cell: median (lowest–highest, n = samples). Seconds, or MB of 2^20 bytes. The method is in `tools/xtask-harness/src/bench.rs` and `platform/calc-oracle-excel/worker/timing-worker.ps1`. Linux RSS includes shared resident pages and is separate from Windows private bytes; unavailable counters stay blank. Thread cycles are available only on Windows.\n\n");
    for (key, value) in run.as_object().into_iter().flatten() {
        out.push_str(&format!("- {key}: {value}\n"));
    }
    out.push('\n');
    out.push_str("The one-cell edit is the assignment of the cell's new value and the recalculation that follows, for both systems: Excel's Range.Value2 assignment plus Application.Calculate, DNA Calc's dispatch plus the recalculation of the dirty cells. Excel's Calculate takes time even when nothing is dirty (the no-op Calculate column, timed right after CalculateFull), so on the small workbooks Excel's edit figure is an upper bound of the work the edit itself causes.\n\n");
    out.push_str("| Workbook | System | Open s | Full rebuild s | Full recalculation s | One-cell edit s | No-op Calculate s | Formulas the edit recalculated | Peak working set MB | Peak private MB | Linux peak RSS MB |\n|---|---|---|---|---|---|---|---|---|---|---|\n");
    for workbook in workbooks {
        let name = workbook["name"].as_str().unwrap_or("?");
        let s = &workbook["stats"];
        for system in ["dna", "excel"] {
            let m = &s[system];
            let get = |k: &str| -> Option<Value> { m.get(k).cloned().filter(|v| !v.is_null()) };
            out.push_str(&format!(
                "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                if system == "dna" { "DNA Calc" } else { "Excel" },
                cell(&get("open_s"), 1.0, 3),
                cell(&get("rebuild_s"), 1.0, 3),
                cell(&get("full_s"), 1.0, 3),
                cell(&get("edit_s"), 1.0, 4),
                cell(&get("noop_s"), 1.0, 4),
                cell(&get("edit_published"), 1.0, 0),
                cell(&get("peak_working_set"), MB, 0),
                cell(&get("peak_private"), MB, 0),
                cell(&get("peak_rss"), MB, 0),
            ));
        }
    }
    out.push_str("\n## DNA Calc phases\n\n| Workbook | Rebuild: bind and order s | Rebuild: evaluate and publish s | Full: order s | Full: evaluate and publish s | Project inventory s |\n|---|---|---|---|---|---|\n");
    for workbook in workbooks {
        let m = &workbook["stats"]["dna"];
        let get = |k: &str| -> Option<Value> { m.get(k).cloned().filter(|v| !v.is_null()) };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            workbook["name"].as_str().unwrap_or("?"),
            cell(&get("rebuild_order_s"), 1.0, 3),
            cell(&get("rebuild_evaluate_s"), 1.0, 3),
            cell(&get("full_order_s"), 1.0, 3),
            cell(&get("full_evaluate_s"), 1.0, 3),
            cell(&get("project_s"), 1.0, 3),
        ));
    }
    out.push_str("\n## DNA Calc warm Rebuild\n\nThe existing Rebuild columns above time a newly opened, unbound graph. This separate command discards an already bound graph through HostSession and checks every formula again. No Excel warm-Rebuild timing is supplied.\n\n| Workbook | Clear, bind and order s | Evaluate and publish s | Total s | Project inventory s |\n|---|---|---|---|---|\n");
    for workbook in workbooks {
        let m = &workbook["stats"]["dna"];
        let get = |k: &str| -> Option<Value> { m.get(k).cloned().filter(|v| !v.is_null()) };
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n",
            workbook["name"].as_str().unwrap_or("?"),
            cell(&get("warm_rebuild_order_s"), 1.0, 3),
            cell(&get("warm_rebuild_evaluate_s"), 1.0, 3),
            cell(&get("warm_rebuild_s"), 1.0, 3),
            cell(&get("warm_project_s"), 1.0, 3)));
    }
    out.push_str("\n## DNA Calc thread cycles (billions; QueryThreadCycleTime, less sensitive to other load than wall time)\n\n| Workbook | Open | Rebuild | Full | Full: order | Full: evaluate | One-cell edit |\n|---|---|---|---|---|---|---|\n");
    for workbook in workbooks {
        let m = &workbook["stats"]["dna"];
        let get = |k: &str| -> Option<Value> { m.get(k).cloned().filter(|v| !v.is_null()) };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            workbook["name"].as_str().unwrap_or("?"),
            cell(&get("open_gcycles"), 1.0, 3),
            cell(&get("rebuild_gcycles"), 1.0, 3),
            cell(&get("full_gcycles"), 1.0, 3),
            cell(&get("full_order_gcycles"), 1.0, 3),
            cell(&get("full_evaluate_gcycles"), 1.0, 3),
            cell(&get("edit_gcycles"), 1.0, 4),
        ));
    }
    out.push_str("\n## Excel: what CalculateFull calculates (--excel-check)\n\nA copy of the workbook with every formula cell's cached value set to -987654321 is opened; the cells showing it are counted after opening, after Calculate and after CalculateFull. 0 after CalculateFull: every formula cell was calculated in the time measured.\n\n| Workbook | Multi-threaded | Threads | Showing after open | Calculate s | After Calculate | CalculateFull s | After CalculateFull | CalculateFull again s |\n|---|---|---|---|---|---|---|---|---|\n");
    for workbook in workbooks {
        for check in workbook["excel"]["check"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.3} | {} | {:.3} | {} | {:.3} |\n",
                workbook["name"].as_str().unwrap_or("?"),
                check["multithreaded"],
                check["thread_count"],
                check["sentinels_opened"],
                check["calculate_s"].as_f64().unwrap_or(f64::NAN),
                check["sentinels_after_calculate"],
                check["full_s"].as_f64().unwrap_or(f64::NAN),
                check["sentinels_after_full"],
                check["full_again_s"].as_f64().unwrap_or(f64::NAN),
            ));
        }
    }
    out.push_str("\n## Load and failures\n\n");
    for workbook in workbooks {
        out.push_str(&format!(
            "- {}: load {}; edit {}; Excel {}; DNA Calc runs failed: {}\n",
            workbook["name"].as_str().unwrap_or("?"),
            workbook["load"],
            workbook["edit"],
            workbook["excel"].get("failed").map_or_else(
                || format!(
                    "build {}, multi-threaded {} with {} threads; {}",
                    workbook["excel"]["excel_build"],
                    workbook["excel"]["multithreaded"],
                    workbook["excel"]["thread_count"],
                    workbook["excel"]["excel"]
                ),
                |failed| format!("failed: {failed}")
            ),
            workbook["dna"]
                .as_array()
                .map(|runs| runs
                    .iter()
                    .filter_map(|run| run.get("failed"))
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join("; "))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "none".to_owned()),
        ));
    }
    out
}

fn run_bench(args: &Args) -> u8 {
    let started = Instant::now();
    let entries = match folder_workbooks(&args.dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("xtask bench: {e}");
            return EXIT_FAILED;
        }
    };
    if let Err(e) = fs::create_dir_all(&args.out) {
        eprintln!("xtask bench: cannot create {}: {e}", args.out.display());
        return EXIT_FAILED;
    }
    let (program, program_note) = if args.dna {
        match compare_program(Some("perf")) {
            Ok(program) => program,
            Err(e) => {
                eprintln!("xtask bench: {e}");
                return EXIT_FAILED;
            }
        }
    } else {
        (PathBuf::new(), "none run".to_owned())
    };
    let _copy = args.dna.then(|| xtask::harness::CopyCleanup(program.clone()));
    let cache = Cache {
        root: args.cache.clone(),
    };
    let build = excel_build().ok();
    let mut workbooks = Vec::new();
    for entry in &entries {
        let edit = args
            .edits
            .iter()
            .find(|(workbook, _)| *workbook == entry.name)
            .map(|(_, cell)| cell.as_str());
        println!("xtask bench: {}", entry.name);
        let mut load = vec![machine_load()];
        let dna = if args.dna {
            let sha256 = match fs::read(&entry.path) {
                Ok(bytes) => sha256_hex(&bytes),
                Err(e) => {
                    eprintln!("xtask bench: cannot read {}: {e}", entry.path.display());
                    return EXIT_FAILED;
                }
            };
            match cache.find(&sha256, build.as_deref()) {
                Some((capture, _)) => {
                    let inventory = args
                        .out
                        .join("inventory")
                        .join(format!("{}.tsv", crate::corpus::file_safe(&entry.name)));
                    match write_inventory(&capture, &inventory) {
                        Ok(()) => dna_runs(
                            &program,
                            entry,
                            &inventory,
                            edit,
                            args.runs,
                            args.repeat,
                            args.threads,
                            args.verify_threads,
                        ),
                        Err(e) => vec![json!({ "failed": e })],
                    }
                }
                None => vec![
                    json!({ "failed": "no capture is cached for this workbook; run cargo xtask corpus first" }),
                ],
            }
        } else {
            Vec::new()
        };
        load.push(machine_load());
        let excel = if args.excel {
            let excel = excel_timing(args, entry, edit);
            load.push(machine_load());
            excel
        } else {
            Value::Null
        };
        let stats = workbook_stats(&dna, &excel);
        println!(
            "xtask bench:   DNA Calc open {} rebuild {} full {} edit {}; Excel open {} rebuild {} full {} edit {}",
            cell(
                &Some(stats["dna"]["open_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["dna"]["rebuild_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["dna"]["full_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["dna"]["edit_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                4
            ),
            cell(
                &Some(stats["excel"]["open_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["excel"]["rebuild_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["excel"]["full_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                3
            ),
            cell(
                &Some(stats["excel"]["edit_s"].clone()).filter(|v| !v.is_null()),
                1.0,
                4
            ),
        );
        workbooks.push(json!({
            "name": entry.name,
            "edit": edit,
            "load": load,
            "dna": dna,
            "excel": excel,
            "stats": stats,
        }));
    }
    let mut command = vec!["cargo xtask bench".to_owned()];
    command.extend(std::env::args().skip(2));
    let run = json!({
        "command": command.join(" "),
        "finished_utc": crate::accept::utc_now(),
        "repository": crate::capture::repository_revision(),
        "dna_process": program_note,
        "excel_build": build,
        "wall_time_s": started.elapsed().as_secs_f64(),
    });
    let document =
        json!({ "format": "dnacalc-bench", "version": 1, "run": run, "workbooks": workbooks });
    let text = serde_json::to_string_pretty(&document).unwrap_or_default() + "\n";
    for (name, text) in [
        ("bench.json", text),
        ("bench.md", markdown(&run, &workbooks)),
    ] {
        let path = args.out.join(name);
        if let Err(e) = fs::write(&path, text) {
            eprintln!("xtask bench: cannot write {}: {e}", path.display());
            return EXIT_FAILED;
        }
    }
    println!(
        "xtask bench: wrote bench.md and bench.json to {}",
        args.out.display()
    );
    0
}

/// `bench --one --fixture <xlsx> --inventory <file> [--edit <sheet>!<cell>]`: one DNA Calc
/// run, printed on one line.
fn one(args: &[String]) -> u8 {
    let (mut fixture, mut inventory, mut edit) = (None, None, None);
    let mut repeat = 1;
    let mut threads = 1;
    let mut verify_threads = false;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let Some(value) = rest.next() else {
            eprintln!("xtask bench: {flag} needs a value");
            return EXIT_USAGE;
        };
        match flag.as_str() {
            "--fixture" => fixture = Some(PathBuf::from(value)),
            "--inventory" => inventory = Some(PathBuf::from(value)),
            "--edit" => edit = Some(value.clone()),
            "--threads" => match value.parse::<usize>() {
                Ok(count) if count > 0 => threads = count,
                _ => {
                    eprintln!("xtask bench: --threads {value:?} is not a positive number");
                    return EXIT_USAGE;
                }
            },
            "--verify-threads" => verify_threads = value == "1",
            "--repeat" => match value.parse::<u32>() {
                Ok(count) if count > 0 => repeat = count,
                _ => {
                    eprintln!("xtask bench: --repeat {value:?} is not a positive number");
                    return EXIT_USAGE;
                }
            },
            other => {
                eprintln!("xtask bench: unknown argument {other:?}");
                return EXIT_USAGE;
            }
        }
    }
    let (Some(fixture), Some(inventory)) = (fixture, inventory) else {
        eprintln!("xtask bench: --one needs --fixture and --inventory");
        return EXIT_USAGE;
    };
    let result = run_one(
        &fixture,
        &inventory,
        edit.as_deref(),
        repeat,
        threads,
        verify_threads,
    )
    .unwrap_or_else(|failed| json!({ "failed": failed }));
    println!("{LINE}{result}");
    0
}

pub fn run(raw: &[String]) -> ExitCode {
    if let Some(("--one", rest)) = raw
        .split_first()
        .map(|(first, rest)| (first.as_str(), rest))
    {
        return ExitCode::from(one(rest));
    }
    match parse_args(raw) {
        Ok(args) => ExitCode::from(run_bench(&args)),
        Err(e) => {
            eprintln!("xtask bench: {e}\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
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
    fn arguments_and_cells_parse() {
        let parsed =
            parse_args(&args("corpus --runs 3 --edit a.xlsx=Inputs!AB12 --excel")).unwrap();
        assert_eq!(parsed.runs, 3);
        assert!(parsed.excel && parsed.dna);
        assert_eq!(
            parsed.edits,
            vec![("a.xlsx".to_owned(), "Inputs!AB12".to_owned())]
        );
        assert_eq!(
            split_cell("3g CPIH!B7").unwrap(),
            ("3g CPIH".to_owned(), 7, 2)
        );
        assert_eq!(split_cell("A!B!AB12").unwrap(), ("A!B".to_owned(), 12, 28));
        assert!(split_cell("Inputs!12").is_err());
        assert!(split_cell("Inputs!B0").is_err());
        assert!(parse_args(&args("corpus --runs 0")).is_err());
        assert!(parse_args(&args("corpus --no-dna")).is_err());
    }

    #[test]
    fn linux_resident_counters_preserve_units_and_unknown_values() {
        let status = "VmPeak:\t999 kB\nVmHWM:\t12345 kB\nVmRSS:\t4096 kB\n";
        assert_eq!(status_bytes(status, "VmHWM:"), Some(12345 * 1024));
        assert_eq!(status_bytes(status, "VmRSS:"), Some(4096 * 1024));
        assert_eq!(status_bytes(status, "VmSize:"), None);
        for invalid in [
            "VmRSS: 12 B",
            "VmRSS: nope kB",
            "VmRSS: 18446744073709551615 kB",
        ] {
            assert_eq!(status_bytes(invalid, "VmRSS:"), None);
        }
    }

    /// Native-only: independent host opens read the committed workbook and capture.
    #[test]
    fn thread_comparison_keeps_values_and_provenance_without_equating_workbook_ids() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/calc-func/tests");
        let bytes = fs::read(root.join("fixtures/cdoc_jpha/oracle.xlsx")).unwrap();
        let capture = CaptureRecord::decode(
            &fs::read(root.join("captures/cdoc_jpha/oracle.capture.json")).unwrap(),
        )
        .unwrap();
        let open = || {
            HostState::open(
                Arc::new(MemorySource::new(bytes.clone())),
                &OpenPolicy::default(),
                HostOptions {
                    calc_mode: Some(CalcMode::Automatic),
                    ..HostOptions::default()
                },
            )
            .unwrap()
        };
        let (mut left, mut right) = (open(), open());
        left.set_threads(2);
        right.set_threads(1);
        recalculate(&mut left, |_| {}).unwrap();
        recalculate(&mut right, |_| {}).unwrap();
        let cell = &capture.formulas[0].cell;
        let sheet = left
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == cell.sheet())
            .unwrap()
            .id();
        let at = CellKey::new(sheet, cell.row(), cell.column()).unwrap();
        let request = ProjectionRequest { cells: vec![at] };
        let page = |host: &HostState| project(host, &request).unwrap();
        let identity = |host: &HostState| match page(host).cells()[0].value() {
            ProjectedValue::Calculated { calculation, .. } => {
                calculation.stamp().unwrap().target.workbook.document
            }
            other => panic!("expected a certified calculated value: {other:?}"),
        };
        assert_ne!(identity(&left), identity(&right));
        assert_eq!(differences(&left, &right, &request).unwrap(), 0);
        recalculate(&mut left, calculate_full).unwrap();
        let raw_value = |host: &HostState| match page(host).cells()[0].value() {
            ProjectedValue::Calculated { value, .. } => format!("{value:?}"),
            other => panic!("expected a calculated value: {other:?}"),
        };
        assert_eq!(raw_value(&left), raw_value(&right));
        assert_eq!(
            differences(&left, &right, &request).unwrap(),
            1,
            "a changed tick/certificate still differs"
        );
        let literal = |value| ProjectedValue::Literal(calc_host::value::CalcValue::number(value));
        assert_ne!(
            independent_workbooks::comparable(&literal(0.0)),
            independent_workbooks::comparable(&literal(-0.0)),
            "signed zero remains bit-sensitive"
        );
        assert_ne!(
            independent_workbooks::comparable(&literal(1.0)),
            independent_workbooks::comparable(&literal(f64::from_bits(1.0f64.to_bits() + 1))),
            "adjacent finite values remain different"
        );
    }

    /// Native-only: the command reads committed workbook/capture files and writes its inventory.
    #[test]
    fn held_circular_values_and_external_file_caches_cannot_qualify_for_timing() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (package, case, expected) in [
            ("calc-engine", "dc_y2o", "3 of the 17"),
            ("calc-host", "dc_ylr", "1 of the 74"),
        ] {
            let inventory = std::env::temp_dir()
                .join(format!("dnacalc-bench-{}-{case}.tsv", std::process::id()));
            let package = root.join("crates").join(package).join("tests");
            write_inventory(
                &package
                    .join("captures")
                    .join(case)
                    .join("oracle.capture.json"),
                &inventory,
            )
            .unwrap();
            let outcome = run_one(
                &package.join("fixtures").join(case).join("oracle.xlsx"),
                &inventory,
                None,
                1,
                1,
                false,
            );
            fs::remove_file(inventory).unwrap();
            let refusal = outcome.unwrap_err();
            assert!(refusal.contains(expected), "{case}: {refusal}");
        }
    }

    #[test]
    fn statistics_are_the_median_and_the_extremes() {
        assert_eq!(stats(&[]), None);
        let s = stats(&[3.0, 1.0, 2.0]).unwrap();
        assert_eq!(
            (s["median"].as_f64(), s["min"].as_f64(), s["max"].as_f64()),
            (Some(2.0), Some(1.0), Some(3.0))
        );
        let s = stats(&[4.0, 1.0, 2.0, 3.0]).unwrap();
        assert_eq!(s["median"].as_f64(), Some(2.5));
        assert_eq!(edited(0.0), 1.0);
        assert_eq!(edited(2.0), 2.0 * 1.01);
    }
}
