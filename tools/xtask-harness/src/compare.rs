//! `cargo xtask compare --fixture <xlsx> --capture <capture.json> --out <dir> [--exact-target
//! <excel build>,<channel>,<compatibility>,<numeric environment>]`: one workbook's DNA Calc
//! calculation compared with its Excel capture, cell by cell, with every difference classified
//! (bead `dc-kt3l`).
//!
//! 1. It refuses (exit 1, nothing written) unless the capture record's `source` is `sha256:` and
//!    the SHA-256 of the fixture's bytes, and refuses a capture record `calc_verdict::judge`
//!    refuses on its own (an inventory failure, say) before opening the workbook.
//! 2. It opens the fixture's bytes with `calc_host::HostState::open` under calc-doc's default open
//!    policy. When the host refuses the package (`OpenFailure`), every inventory cell is missing,
//!    with the typed error as the reason, and the triage classes every cell `doc` under the
//!    error's variant.
//! 3. It recalculates with `calc_host::step` and an unlimited budget, showing a read observer every
//!    evaluation CalcFml's evaluator reports (its cell, revision, tick and what it returned), each
//!    catalogue call it ran and each run of cells it read. When the recalculation fails or is
//!    refused, the observer's evaluations are dropped, so nothing it saw proves a calculation, and
//!    the triage classes every cell `engine` under the failure's variant.
//! 4. It projects exactly the inventory's cells with `calc_host::project` and maps each projected
//!    value onto calc-verdict's provenance: calculated (tick, revision, and the context
//!    fingerprint below), file-cached (with the package's hash and the part), stale (with its
//!    cause and the last value or cache), not modelled (the typed refusal with what it concerns
//!    as the feature, such as `Evaluation:MissingInput (reads Calc!B1)`, and the refusal's text
//!    as the diagnostic), and an empty or literal cell, or a sheet the loaded workbook does not
//!    hold, as missing.
//! 5. It judges the inventory in-process with `calc_verdict::judge` against the version 2
//!    evaluation record built from the observer's evaluations (`InstrumentRecord`; this command is
//!    its only product writer), and writes `report.json` (the CellVerdict report) and
//!    `evaluations.json` (the evaluation record) to `--out`.
//! 6. It triages every inventory cell (`compare_triage.rs`) and writes `triage.json`, then prints a
//!    summary: the inventory and what was calculated, matched and mismatched, the classes, the
//!    mismatched cells by class, the not-modelled cells by the key of their root with the cells
//!    each root blocks, and the wall time of open, step, project and judge.
//!
//! The run's context fingerprint is the one `calc_verdict::CalculationContext::without_iteration`
//! gives for the date system and precision `HostState::recalc_context` reports the host applies,
//! never the capture's, with the iteration settings it applies (count and maximum change) when it
//! iterates (L3, dc-m3u). A version 1 capture names no context, so when the package
//! declares a date system the host does not apply (`HostState::declared_date1904`), the triage
//! classes every calculated cell `context` under the setting that differs. Under
//! `--exact-target` the host recalculates under that Exact numeric mode and the verdict certifies
//! against that target, so a strict-bit comparison is never of a value calculated under Fast; with
//! no Exact arithmetic in calc-func yet, such a run refuses every arithmetic site
//! (`ExactUnavailable`).
//!
//! Only the evaluations whose value differs from the capture's observation (by
//! `calc_verdict::compare`, the comparison `judge` uses) keep their calls and reads, so the memory
//! the observer holds grows with the differing cells, not with the workbook's reads; a read keeps
//! at most `compare_triage::READ_ENTRIES` runs per evaluation and counts the cells of the rest.
//! Paths are relative to the current directory.
//!
//! Exit codes: 0 when the three files were written, whatever they say; 1 when an input cannot be
//! read or is refused (the fixture's hash, the capture's consistency) or a file cannot be written;
//! 2 on a usage error; 3 when the capture's formula inventory is empty; 4 when the capture path
//! holds the absent-capture marker (the capture has not been taken).

use crate::compare_triage::{
    self, Call, Class, Dna, Read, Retained, RunEntry, WorkbookFailure, address_of, refusal_of,
    variant,
};
use crate::verdict::parse_target;
use calc_engine::ObservedEvaluation;
use calc_fml::ExecutedCall;
use calc_host::value::{
    ArrayShape, CalcArray, CalcValue, CellKey, GridArea, NumericMode, NumericTargetId, SheetId,
};
use calc_host::{
    CalcMode, CellRun, HostOptions, HostProgress, HostState, LoadError, MemorySource, OpenFailure,
    OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest, ReadObserver, RecalcContext,
    RecalcFailed, WorkBudget, project, step,
};
use calc_verdict::{
    CalculationContext, CaptureRecord, CellAddress, CellResult, Evaluation, EvaluationRecord,
    FormulaKind, InstrumentRecord, InstrumentedEvaluation, JudgeError, MissingCapture, Observation,
    Provenance, ResultSet, RunIdentity, VerdictPolicy, judge, sha256_hex,
};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask compare --fixture <xlsx> --capture <capture.json> --out <dir> [--exact-target <excel build>,<channel>,<compatibility>,<numeric environment>]";

const EXIT_REFUSED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_EMPTY_INVENTORY: u8 = 3;
const EXIT_MISSING_CAPTURE: u8 = 4;

#[derive(Debug, PartialEq, Eq)]
struct Request {
    fixture: PathBuf,
    capture: PathBuf,
    out: PathBuf,
    target: Option<NumericTargetId>,
}

/// Why nothing was written, and the exit code that says so.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    code: u8,
    message: String,
}

fn refused(message: String) -> Failure {
    Failure {
        code: EXIT_REFUSED,
        message,
    }
}

fn parse_args(args: &[String]) -> Result<Request, String> {
    let (mut fixture, mut capture, mut out, mut target) = (None, None, None, None);
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let slot = match flag.as_str() {
            "--fixture" => &mut fixture,
            "--capture" => &mut capture,
            "--out" => &mut out,
            "--exact-target" => &mut target,
            other => return Err(format!("unknown argument {other:?}")),
        };
        if slot.replace(value.clone()).is_some() {
            return Err(format!("{flag} is given twice"));
        }
    }
    Ok(Request {
        fixture: PathBuf::from(fixture.ok_or("--fixture is required")?),
        capture: PathBuf::from(capture.ok_or("--capture is required")?),
        out: PathBuf::from(out.ok_or("--out is required")?),
        target: target.as_deref().map(parse_target).transpose()?,
    })
}

fn read(path: &Path, what: &str) -> Result<Vec<u8>, Failure> {
    fs::read(path).map_err(|e| refused(format!("cannot read the {what} {}: {e}", path.display())))
}

fn judge_failure(error: JudgeError) -> Failure {
    let code = match &error {
        JudgeError::EmptyInventory { .. } => EXIT_EMPTY_INVENTORY,
        JudgeError::Inconsistent { .. } => EXIT_REFUSED,
    };
    Failure {
        code,
        message: format!("the inputs are refused: {error}"),
    }
}

/// One evaluation the read observer was shown: the formula cell, the calculation's revision and
/// tick, and what CalcFml's evaluator returned (`Err` with its fault).
struct Seen {
    at: CellKey,
    revision: u64,
    tick: u64,
    returned: Result<CalcValue, String>,
}

/// One area an evaluation read, as the observer is shown it: the area, the cells its runs
/// covered, the runs kept and the cells of the runs not kept.
struct ReadBuffer {
    area: GridArea,
    cells: u64,
    runs: Vec<RunEntry>,
    omitted: u64,
}

/// The calls and reads of one evaluation.
type Kept = (Vec<ExecutedCall>, Vec<ReadBuffer>);

/// The read observer `step` is given: it records every evaluation the evaluator reports, and keeps
/// the calls and reads of an evaluation whose value differs from the capture's observation of its
/// cell.
struct Instrument<'c> {
    /// The capture's observation of each inventory cell on a sheet the workbook holds.
    expected: HashMap<CellKey, &'c CalcValue>,
    evaluations: Vec<Seen>,
    /// The index in `evaluations` of each cell's evaluation at a revision and tick. An iterating
    /// recalculation evaluates a formula on a circular reference once per pass, all at one tick;
    /// the record keeps the last, whose value is the one published (dc-10y).
    latest: HashMap<(CellKey, u64, u64), usize>,
    /// The formula whose evaluation the calls and reads below belong to.
    current: Option<CellKey>,
    calls: Vec<ExecutedCall>,
    reads: Vec<ReadBuffer>,
    /// Runs kept for the current evaluation, at most `compare_triage::READ_ENTRIES`.
    kept: usize,
    retained: HashMap<CellKey, Kept>,
}

impl<'c> Instrument<'c> {
    fn new(expected: HashMap<CellKey, &'c CalcValue>) -> Self {
        Instrument {
            expected,
            evaluations: Vec::new(),
            latest: HashMap::new(),
            current: None,
            calls: Vec::new(),
            reads: Vec::new(),
            kept: 0,
            retained: HashMap::new(),
        }
    }

    /// Makes `formula` the evaluation the next calls and reads belong to.
    fn begin(&mut self, formula: CellKey) {
        if self.current != Some(formula) {
            self.clear();
            self.current = Some(formula);
        }
    }

    fn clear(&mut self) {
        self.current = None;
        self.calls.clear();
        self.reads.clear();
        self.kept = 0;
    }

    /// Drops everything recorded: the recalculation published nothing, so no evaluation it saw
    /// proves a calculated value.
    fn drop_all(&mut self) {
        self.clear();
        self.evaluations.clear();
        self.latest.clear();
        self.retained.clear();
    }

    fn keep(&mut self, entry: RunEntry, cells: u64) {
        let Some(read) = self.reads.last_mut() else {
            return;
        };
        if self.kept < compare_triage::READ_ENTRIES {
            read.runs.push(entry);
            self.kept += 1;
        } else {
            read.omitted += cells;
        }
    }
}

impl ReadObserver for Instrument<'_> {
    fn observe(
        &mut self,
        formula: CellKey,
        area: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()> {
        self.begin(formula);
        if offset == 0 || self.reads.last().is_none_or(|read| read.area != area) {
            self.reads.push(ReadBuffer {
                area,
                cells: 0,
                runs: Vec::new(),
                omitted: 0,
            });
        }
        if let Some(read) = self.reads.last_mut() {
            read.cells += run.len();
        }
        match run {
            CellRun::Values(values) => {
                for value in values {
                    self.keep(RunEntry::Value(value.clone()), 1);
                }
            }
            CellRun::Blank(count) => self.keep(RunEntry::Blank(count), count),
            CellRun::Missing(count) => self.keep(RunEntry::Missing(count), count),
        }
        ControlFlow::Continue(())
    }

    fn called(&mut self, formula: CellKey, call: ExecutedCall) {
        self.begin(formula);
        self.calls.push(call);
    }

    /// The evaluation was discarded to wait for a cell: what it called and read is forgotten, so
    /// a retained difference shows only the evaluation that was published.
    fn deferred(&mut self, formula: CellKey, _cell: CellKey) {
        if self.current == Some(formula) {
            self.clear();
        }
    }

    fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
        let at = evaluation.formula();
        let calculation = evaluation.calculation();
        let returned = match evaluation.outcome() {
            Ok(value) => Ok(value.clone()),
            Err(fault) => Err(format!("CalcFml refused: {fault:?}")),
        };
        let differs = match (&returned, self.expected.get(&at)) {
            (Ok(value), Some(expected)) => calc_verdict::compare(expected, value).is_some(),
            _ => false,
        };
        if differs {
            let kept = if self.current == Some(at) {
                (
                    std::mem::take(&mut self.calls),
                    std::mem::take(&mut self.reads),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            self.retained.insert(at, kept);
        } else {
            self.retained.remove(&at);
        }
        let seen = Seen {
            at,
            revision: calculation.revision().get(),
            tick: calculation.tick().get(),
            returned,
        };
        match self.latest.get(&(at, seen.revision, seen.tick)) {
            Some(index) => self.evaluations[*index] = seen,
            None => {
                self.latest
                    .insert((at, seen.revision, seen.tick), self.evaluations.len());
                self.evaluations.push(seen);
            }
        }
        self.clear();
    }
}

/// A read observer that can drop everything it recorded, which [`recalculate`] does when the
/// recalculation publishes nothing.
trait Recorder: ReadObserver {
    fn drop_all(&mut self);
}

impl Recorder for Instrument<'_> {
    fn drop_all(&mut self) {
        Instrument::drop_all(self);
    }
}

/// Recalculates `host` to the end with an unlimited budget, showing `recorder` every evaluation.
/// When the recalculation is refused or fails, `recorder` drops everything it recorded, since no
/// evaluation it saw was published, and the failure is the run's: every cell is classed `engine`
/// under its variant.
fn recalculate(host: &mut HostState, recorder: &mut impl Recorder) -> Option<WorkbookFailure> {
    let progress = loop {
        match step(host, WorkBudget::UNLIMITED, recorder) {
            HostProgress::Calculating { .. } => {}
            done => break done,
        }
    };
    match progress {
        HostProgress::Failed(failure) => {
            recorder.drop_all();
            Some(WorkbookFailure {
                class: Class::Engine,
                key: step_key(&failure),
                message: format!("the recalculation published nothing: {failure}"),
            })
        }
        HostProgress::Current
        | HostProgress::Manual
        | HostProgress::Cancelled
        | HostProgress::Published(_)
        | HostProgress::Calculating { .. } => None,
    }
}

/// The fingerprint of the context `applied` names: its date system, precision and, when it
/// iterates, its iteration count and maximum change.
fn run_context(applied: &RecalcContext) -> String {
    if applied.iteration {
        return CalculationContext {
            date1904: applied.date1904,
            iteration: true,
            max_iterations: applied.max_iterations,
            max_change: f64::from_bits(applied.max_change_bits),
            precision_as_displayed: applied.precision_as_displayed,
        }
        .fingerprint();
    }
    CalculationContext::without_iteration(applied.date1904, applied.precision_as_displayed)
        .fingerprint()
}

/// For a version 1 capture, which names no context, the key of the context settings the package
/// declares (`declared_date1904`) and `applied` does not apply; `None` when they agree or the
/// capture names its context (judge then compares the two).
fn declared_key(
    capture: &CaptureRecord,
    declared_date1904: bool,
    applied: &RecalcContext,
) -> Option<String> {
    (capture.session.is_none() && declared_date1904 != applied.date1904).then(|| {
        format!(
            "date1904: package declares {declared_date1904}, run applies {} (the version 1 capture names no context)",
            applied.date1904
        )
    })
}

/// Wall times of the run's stages; a stage that did not run has none.
#[derive(Debug, Default)]
struct Times {
    open: Duration,
    step: Option<Duration>,
    project: Option<Duration>,
    judge: Duration,
}

fn seconds(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "not run".to_owned(),
        |duration| format!("{:.3} s", duration.as_secs_f64()),
    )
}

/// What the run produced for the verdict and the triage.
struct Run {
    results: ResultSet,
    record: InstrumentRecord,
    dna: Vec<Dna>,
    retained: HashMap<CellAddress, Retained>,
    workbook: Option<WorkbookFailure>,
    /// [`declared_key`]'s key, when the package declares a context the run does not apply.
    declared: Option<String>,
    context: String,
    revision: u64,
    times: Times,
}

/// The key of a refused package: the failure's variant, and for a refused workbook the load
/// error's (and a refused cell's) variant.
fn open_key(failure: &OpenFailure) -> String {
    match failure {
        OpenFailure::Package(error) => format!("Package:{}", variant(error)),
        OpenFailure::Workbook(LoadError::Cell { error, .. }) => {
            format!("Workbook:Cell:{}", variant(error))
        }
        OpenFailure::Workbook(error) => format!("Workbook:{}", variant(error)),
        OpenFailure::Caches(_) => "Caches".to_owned(),
    }
}

/// The key of a failed recalculation: whether it was refused or failed, and the variant.
fn step_key(failure: &RecalcFailed) -> String {
    match failure {
        RecalcFailed::Refused(error) => format!("Refused:{}", variant(error)),
        RecalcFailed::Failed(failure) => format!("Failed:{}", variant(failure)),
    }
}

/// The locale profile of the Excel that took `capture`: its user culture with the decimal and
/// group separators it recorded, which the host's TEXT, VALUE and number text read with.
/// `None` when the capture records no culture or separators the profile table cannot hold.
pub(crate) fn capture_locale(capture: &CaptureRecord) -> Option<calc_host::FormatProfile> {
    let [culture, decimal, thousands] = capture_locale_settings(capture)?;
    locale_profile(culture, decimal, thousands)
}

/// The user culture and the decimal and group separators `capture` recorded.
pub(crate) fn capture_locale_settings(capture: &CaptureRecord) -> Option<[&str; 3]> {
    let environment = capture.environment.as_ref()?;
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    Some([
        setting("user_culture")?,
        setting("decimal_separator")?,
        setting("thousands_separator")?,
    ])
}

/// The clock Excel's calculation read for `capture`, as a serial date and time in the workbook's
/// date system. Where the inventory holds a cell whose formula is `=NOW()` alone, Excel's raw
/// value of that cell is the instant its calculation read (dc-jv0: an input Excel reports, as the
/// capture's own clock reading cannot be, since the worker reads the local clock before Excel
/// opens the workbook, seconds earlier); otherwise the setting calculation_clock (the local clock
/// when the capture's worker started, a 1900-system serial as binary64 bits in hex, which
/// calc-oracle-excel records when the run did not cross midnight), less 1462 days when the
/// setting date1904 is true. `None` when the capture records neither.
pub(crate) fn capture_clock(capture: &CaptureRecord) -> Option<f64> {
    let now_cell = capture.formulas.iter().find_map(|formula| {
        let text = formula.formula.as_deref()?;
        if !text.trim().eq_ignore_ascii_case("=NOW()") {
            return None;
        }
        capture
            .observations
            .iter()
            .find(|observed| observed.cell == formula.cell)
            .and_then(|observed| match &observed.observation {
                Observation::ExcelRaw(value) => value.as_number(),
                _ => None,
            })
    });
    if let Some(now) = now_cell.filter(|now| now.is_finite()) {
        return Some(now);
    }
    let environment = capture.environment.as_ref()?;
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let now = f64::from_bits(u64::from_str_radix(setting("calculation_clock")?, 16).ok()?);
    let now = if setting("date1904") == Some("true") {
        now - 1462.0
    } else {
        now
    };
    now.is_finite().then_some(now)
}

/// A capture setting of `capture`'s environment.
fn capture_setting<'c>(capture: &'c CaptureRecord, name: &str) -> Option<&'c str> {
    capture
        .environment
        .as_ref()?
        .settings
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The file Excel opened for `capture` (the setting workbook_path, its full name, dc-uar): its
/// folder with the trailing separator and its file name. `None` when the capture records none.
pub(crate) fn capture_file(capture: &CaptureRecord) -> Option<calc_fml::WorkbookFile> {
    let path = capture_setting(capture, "workbook_path")?;
    let cut = path.rfind('\\')?;
    Some(calc_fml::WorkbookFile {
        folder: Some(path[..=cut].to_owned()),
        name: path[cut + 1..].to_owned(),
    })
}

/// Excel's facts INFO reads, from `capture`'s environment (dc-uar): `release` the setting
/// excel_version, `osversion` operating_system, and `system` `pcdos` when that names Windows and
/// `mac` otherwise. The current folder is not recorded, so INFO("directory") stays refused.
pub(crate) fn capture_application(capture: &CaptureRecord) -> Option<calc_fml::ApplicationFacts> {
    let release = capture_setting(capture, "excel_version")?.to_owned();
    let os = capture_setting(capture, "operating_system")?.to_owned();
    let system = if os.starts_with("Windows") {
        "pcdos"
    } else {
        "mac"
    };
    Some(calc_fml::ApplicationFacts {
        release: Some(release),
        os_version: Some(os),
        system: Some(system.to_owned()),
        directory: None,
    })
}

/// The locale profile of a user culture with these separators ([`capture_locale`]).
pub(crate) fn locale_profile(
    culture: &str,
    decimal: &str,
    thousands: &str,
) -> Option<calc_host::FormatProfile> {
    calc_host::FormatProfile::for_culture(culture)?.with_separators(decimal, thousands)
}

/// The run of the workbook in `fixture` for `capture`: open, step, project. `name` is the
/// workbook's file name, by which INDIRECT's text may name it, as Excel opened it from the
/// capture's copy of the same name.
/// The array the published spill of the array formula anchored at `at` shows, its cells in
/// row-major order, when it fills more than its anchor and every cell of it projects as
/// calculated; `None` otherwise.
fn spilled_array(host: &HostState, at: CellKey) -> Option<CalcValue> {
    let area = host.spill_area(at)?;
    if area.cell_count() < 2 {
        return None;
    }
    let (first, last) = (area.first(), area.last());
    let mut cells = Vec::with_capacity(area.cell_count() as usize);
    for row in first.row()..=last.row() {
        for column in first.column()..=last.column() {
            cells.push(CellKey::new(at.sheet(), row, column).ok()?);
        }
    }
    let page = project(host, &ProjectionRequest { cells }).ok()?;
    let values = page
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::Calculated { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect::<Option<Vec<CalcValue>>>()?;
    let shape = ArrayShape {
        rows: (last.row() - first.row() + 1) as usize,
        cols: (last.column() - first.column() + 1) as usize,
    };
    CalcArray::new(shape, values).map(CalcValue::array)
}

fn run_workbook(
    fixture: &[u8],
    name: Option<&str>,
    capture: &CaptureRecord,
    numeric: NumericMode,
) -> Result<Run, Failure> {
    let empty_record = || InstrumentRecord {
        case: capture.case.clone(),
        source: capture.source.clone(),
        evaluations: Vec::new(),
    };
    let started = Instant::now();
    let mut memory = MemorySource::new(fixture.to_vec());
    if let Some(name) = name {
        memory = memory.named(name);
    }
    let source: Arc<dyn PackageRead> = Arc::new(memory);
    let opened = HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            numeric: numeric.clone(),
            formula_cell_budget: None,
            // The capture's Excel recalculates every formula (CalculateFullRebuild) whatever mode
            // the package declares, so the comparison calculates under the automatic mode too.
            calc_mode: Some(CalcMode::Automatic),
            locale: capture_locale(capture),
        },
    );
    let open_time = started.elapsed();
    let mut host = match opened {
        Ok(host) => host,
        Err(failure) => {
            let reason = format!("HostState::open refused the package: {failure}");
            let results = capture
                .formulas
                .iter()
                .map(|formula| CellResult {
                    cell: formula.cell.clone(),
                    provenance: Provenance::Missing {
                        reason: reason.clone(),
                    },
                    value: None,
                })
                .collect();
            // No host was made, so no context was applied: the run names none, and no result
            // claims one.
            let context = "no-host: HostState::open refused the package".to_owned();
            return Ok(Run {
                results: ResultSet {
                    case: capture.case.clone(),
                    source: capture.source.clone(),
                    run: RunIdentity {
                        revision: 0,
                        context: context.clone(),
                    },
                    results,
                },
                record: empty_record(),
                dna: vec![Dna::OpenRefused; capture.formulas.len()],
                retained: HashMap::new(),
                workbook: Some(WorkbookFailure {
                    class: Class::Doc,
                    key: open_key(&failure),
                    message: reason,
                }),
                declared: None,
                context,
                revision: 0,
                times: Times {
                    open: open_time,
                    ..Times::default()
                },
            });
        }
    };

    // The clock NOW, TODAY and a date read without a year use: the one Excel's calculation read
    // (the capture's setting calculation_clock); a capture without one leaves those refused.
    host.set_clock(capture_clock(capture));
    // dc-uar: the file Excel opened and Excel's own facts, which CELL and INFO read; a capture
    // without them leaves those refused.
    host.set_file(capture_file(capture));
    host.set_application(capture_application(capture));
    let applied = host.recalc_context();
    let context = run_context(&applied);
    let declared = declared_key(capture, host.declared_date1904(), &applied);
    let names: HashMap<SheetId, String> = host
        .sheets()
        .iter()
        .map(|sheet| (sheet.id(), sheet.name().text().to_string_lossy()))
        .collect();
    let ids: HashMap<&str, SheetId> = names
        .iter()
        .map(|(id, name)| (name.as_str(), *id))
        .collect();
    let key_of = |cell: &CellAddress| -> Option<CellKey> {
        let sheet = ids.get(cell.sheet())?;
        CellKey::new(*sheet, cell.row(), cell.column()).ok()
    };
    let expected: HashMap<CellKey, &CalcValue> = capture
        .observations
        .iter()
        .filter_map(|observed| Some((key_of(&observed.cell)?, observed.observation.expected())))
        .collect();

    let mut instrument = Instrument::new(expected);
    let started = Instant::now();
    let workbook = recalculate(&mut host, &mut instrument);
    let step_time = started.elapsed();

    let keys: Vec<Option<CellKey>> = capture
        .formulas
        .iter()
        .map(|formula| key_of(&formula.cell))
        .collect();
    let request = ProjectionRequest {
        cells: keys.iter().flatten().copied().collect(),
    };
    let started = Instant::now();
    let page = project(&host, &request)
        .map_err(|e| refused(format!("the projection of the inventory was refused: {e}")))?;
    let project_time = started.elapsed();

    let address = |at: CellKey| address_of(&names, at);
    let mut projected = page.cells().iter();
    let mut results = Vec::with_capacity(capture.formulas.len());
    let mut dna = Vec::with_capacity(capture.formulas.len());
    for (formula, key) in capture.formulas.iter().zip(&keys) {
        let cell = formula.cell.clone();
        if key.is_none() {
            results.push(CellResult {
                cell,
                provenance: Provenance::Missing {
                    reason: format!(
                        "the workbook as calc-doc loaded it holds no sheet named {:?}",
                        formula.cell.sheet()
                    ),
                },
                value: None,
            });
            dna.push(Dna::SheetNotLoaded);
            continue;
        }
        let projected = projected
            .next()
            .expect("the projection returns one cell per requested cell");
        let (provenance, value, fact) = match projected.value() {
            ProjectedValue::Empty => (
                Provenance::Missing {
                    reason: "the workbook as calc-doc loaded it holds no input at this cell"
                        .to_owned(),
                },
                None,
                Dna::Empty,
            ),
            ProjectedValue::Literal(_) => (
                Provenance::Missing {
                    reason: "the workbook as calc-doc loaded it holds a literal, not a formula, at this cell".to_owned(),
                },
                None,
                Dna::Literal,
            ),
            ProjectedValue::FileCached(cache) => (
                Provenance::FileCached {
                    source: format!("{} {}", capture.source, cache.source().name()),
                },
                Some(cache.value().to_value()),
                Dna::FileCached,
            ),
            ProjectedValue::Calculated { value, calculation } => (
                // The value the circular-reference rule held with iteration off was not
                // evaluated (dc-y2o.1): the judge counts it apart, with no evaluation asked for.
                if calculation.held_by_circular_rule() {
                    Provenance::CircularRule {
                        tick: calculation.tick().get(),
                        revision: calculation.revision().get(),
                        context: context.clone(),
                        external_links: calculation.reads_external_links(),
                    }
                } else {
                    Provenance::Calculated {
                        tick: calculation.tick().get(),
                        revision: calculation.revision().get(),
                        context: context.clone(),
                        external_links: calculation.reads_external_links(),
                    }
                },
                // A dynamic array's anchor is compared by the whole array its spill shows, as the
                // capture records Excel's (dc-677); a legacy array's cells each by its own value.
                Some(match formula.kind {
                    FormulaKind::Ordinary | FormulaKind::SharedMaster { .. } | FormulaKind::SharedChild { .. } => {
                        key.and_then(|at| spilled_array(&host, at))
                            .unwrap_or_else(|| value.clone())
                    }
                    _ => value.clone(),
                }),
                Dna::Calculated,
            ),
            ProjectedValue::NotModeled { refusal, .. } => {
                let triaged = refusal_of(refusal, &address);
                (
                    Provenance::NotModeled {
                        feature: triaged.feature.clone(),
                        diagnostic: refusal.to_string(),
                    },
                    None,
                    Dna::Refused(triaged),
                )
            }
            ProjectedValue::Stale(stale) => (
                Provenance::Stale {
                    last_calculated: stale.last_calculation().map(|c| c.tick().get()),
                    cause: format!("{:?}", stale.cause()),
                },
                stale
                    .last_value()
                    .cloned()
                    .or_else(|| stale.file_cache().map(|cache| cache.value().to_value())),
                Dna::Stale,
            ),
        };
        results.push(CellResult {
            cell,
            provenance,
            value,
        });
        dna.push(fact);
    }

    let record = InstrumentRecord {
        evaluations: instrument
            .evaluations
            .iter()
            .filter_map(|seen| {
                Some(InstrumentedEvaluation::new(
                    Evaluation {
                        cell: address(seen.at)?,
                        tick: seen.tick,
                        revision: seen.revision,
                        context: context.clone(),
                    },
                    seen.returned.clone(),
                ))
            })
            .collect(),
        ..empty_record()
    };
    let retained = instrument
        .retained
        .into_iter()
        .filter_map(|(at, (calls, reads))| {
            let kept = Retained {
                calls: calls
                    .into_iter()
                    .map(|call| {
                        let target = call.target();
                        Call {
                            id: target.id(),
                            spelling: target.spelling().to_owned(),
                            notation: format!("{:?}", target.notation()),
                            synthetic: call.synthetic(),
                        }
                    })
                    .collect(),
                reads: reads
                    .into_iter()
                    .filter_map(|read| {
                        let (first, last) = (read.area.first(), read.area.last());
                        Some(Read {
                            sheet: names.get(&read.area.sheet())?.clone(),
                            top: first.row(),
                            left: first.column(),
                            bottom: last.row(),
                            right: last.column(),
                            cells: read.cells,
                            runs: read.runs,
                            omitted: read.omitted,
                        })
                    })
                    .collect(),
            };
            Some((address(at)?, kept))
        })
        .collect();
    let revision = host.revision().get();
    Ok(Run {
        results: ResultSet {
            case: capture.case.clone(),
            source: capture.source.clone(),
            run: RunIdentity {
                revision,
                context: context.clone(),
            },
            results,
        },
        record,
        dna,
        retained,
        workbook,
        declared,
        context,
        revision,
        times: Times {
            open: open_time,
            step: Some(step_time),
            project: Some(project_time),
            judge: Duration::ZERO,
        },
    })
}

fn write(path: &Path, text: &str) -> Result<(), Failure> {
    fs::write(path, text).map_err(|e| refused(format!("cannot write {}: {e}", path.display())))
}

fn numeric_name(numeric: &NumericMode) -> String {
    match numeric.target() {
        None => "fast".to_owned(),
        Some(target) => format!(
            "exact {},{},{},{}",
            target.excel_build(),
            target.channel(),
            target.compatibility(),
            target.environment()
        ),
    }
}

/// Reads the request's files, runs the workbook, judges and triages it, writes the three files and
/// prints the summary.
fn compare(request: &Request) -> Result<(), Failure> {
    let fixture = read(&request.fixture, "fixture")?;
    let capture_bytes = read(&request.capture, "capture record")?;
    if let Some(missing) = MissingCapture::recognize(&capture_bytes) {
        return Err(Failure {
            code: EXIT_MISSING_CAPTURE,
            message: format!("{}: {missing}", request.capture.display()),
        });
    }
    let capture = CaptureRecord::decode(&capture_bytes)
        .map_err(|e| refused(format!("the capture record is refused: {e}")))?;
    let digest = format!("sha256:{}", sha256_hex(&fixture));
    if capture.source != digest {
        return Err(refused(format!(
            "the fixture {} has the content hash {digest} and the capture record's source is {}: the capture is not of these bytes",
            request.fixture.display(),
            capture.source
        )));
    }
    let numeric = request
        .target
        .clone()
        .map_or(NumericMode::Fast, NumericMode::Exact);
    let policy = VerdictPolicy {
        numeric_mode: numeric.clone(),
        evidence: None,
    };
    // The capture record's own consistency, before the workbook is opened: judged against no
    // results, every cell is missing, and whatever judge refuses in the record is refused here.
    judge(
        &capture,
        &ResultSet {
            case: capture.case.clone(),
            source: capture.source.clone(),
            run: RunIdentity {
                revision: 0,
                context: "capture-check".to_owned(),
            },
            results: Vec::new(),
        },
        &EvaluationRecord {
            case: capture.case.clone(),
            source: capture.source.clone(),
            evaluations: Vec::new(),
        },
        &policy,
    )
    .map_err(judge_failure)?;

    let name = request.fixture.file_name().and_then(|name| name.to_str());
    let mut run = run_workbook(&fixture, name, &capture, numeric.clone())?;
    let started = Instant::now();
    let report = judge(&capture, &run.results, &run.record, &policy).map_err(judge_failure)?;
    run.times.judge = started.elapsed();
    let triage = compare_triage::triage(
        &capture,
        &report,
        &run.dna,
        &run.retained,
        run.workbook.as_ref(),
        run.declared.as_deref(),
    );

    let evaluations = run
        .record
        .encode()
        .map_err(|e| refused(format!("the evaluation record cannot be written: {e}")))?;
    let run_member = json!({
        "revision": run.revision,
        "context": run.context,
        "numeric_mode": numeric_name(&numeric),
        "workbook_failure": run.workbook.as_ref().map(|failure| json!({
            "class": failure.class.name(),
            "key": failure.key,
            "message": failure.message,
        })),
        "declared_context_not_applied": run.declared,
        "wall_time_s": {
            "open": run.times.open.as_secs_f64(),
            "step": run.times.step.map(|d| d.as_secs_f64()),
            "project": run.times.project.map(|d| d.as_secs_f64()),
            "judge": run.times.judge.as_secs_f64(),
        },
        "evaluator_calls": run.record.evaluations.len(),
        "evaluations_kept_with_reads": run.retained.len(),
    });
    fs::create_dir_all(&request.out)
        .map_err(|e| refused(format!("cannot create {}: {e}", request.out.display())))?;
    write(&request.out.join("report.json"), &report.encode())?;
    write(&request.out.join("evaluations.json"), &evaluations)?;
    write(
        &request.out.join("triage.json"),
        &triage.encode(&report, run_member),
    )?;

    let c = &report.counts;
    println!(
        "xtask compare: {}: inventory {} formula cells; calculated {}; matched {}; mismatched {}; not run {}; not modelled {}; unproven {}, stale {}, file-cached {}, failed {}, missing {}",
        report.case,
        c.denominator,
        c.calculated,
        c.matched,
        c.mismatched,
        c.not_run,
        c.not_modeled,
        c.unproven,
        c.stale,
        c.file_cached,
        c.failed,
        c.missing
    );
    if c.external_links > 0 {
        println!(
            "xtask compare: {} calculated cells read another workbook's link caches ({} of them matched); the corpus summary counts those matches apart from passes",
            c.external_links, c.matched_external_links
        );
    }
    if c.circular_rule > 0 {
        println!(
            "xtask compare: {} cells hold the circular-reference rule's value with iteration off ({} of them matched); the corpus summary counts those matches apart from passes",
            c.circular_rule, c.matched_circular_rule
        );
    }
    if let Some(failure) = &run.workbook {
        println!(
            "xtask compare: {}; every inventory cell is classed {} with key {}",
            failure.message,
            failure.class.name(),
            failure.key
        );
    }
    if let Some(declared) = &run.declared {
        println!(
            "xtask compare: the package declares a context the run does not apply ({declared}); every calculated cell is classed context"
        );
    }
    for line in triage.summary(&report) {
        println!("xtask compare: {line}");
    }
    println!(
        "xtask compare: wall time: open {}, step {}, project {}, judge {}",
        seconds(Some(run.times.open)),
        seconds(run.times.step),
        seconds(run.times.project),
        seconds(Some(run.times.judge))
    );
    println!(
        "xtask compare: run at revision {} under {} with context {}; {} evaluator calls recorded",
        run.revision,
        numeric_name(&numeric),
        run.context,
        run.record.evaluations.len()
    );
    println!("xtask compare: verdict: {}", report.summary());
    println!(
        "xtask compare: wrote report.json, evaluations.json and triage.json to {}",
        request.out.display()
    );
    Ok(())
}

/// The command's exit code for `args`, after printing what it did or why it did not.
fn run_code(args: &[String]) -> u8 {
    let request = match parse_args(args) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("xtask compare: {e}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    match compare(&request) {
        Ok(_) => 0,
        Err(failure) => {
            eprintln!("xtask compare: {}", failure.message);
            failure.code
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    ExitCode::from(run_code(args))
}

/// The dc-kt3l cases' workbook generator, shared with the unit tests below.
#[cfg(test)]
#[path = "../../xtask/tests/acceptance/dc_kt3l_package.rs"]
mod test_package;

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    /// A host opened on the dc-kt3l cases' workbook, and the key of its cell `Calc!<row,column>`.
    fn opened() -> (HostState, impl Fn(u32, u32) -> CellKey) {
        let source: Arc<dyn PackageRead> =
            Arc::new(MemorySource::new(super::test_package::workbook()));
        let host = HostState::open(source, &OpenPolicy::default(), HostOptions::default())
            .expect("the dc-kt3l workbook opens");
        let calc = host
            .sheets()
            .iter()
            .find(|sheet| sheet.name().text().to_string_lossy() == "Calc")
            .expect("the workbook has a sheet Calc")
            .id();
        (host, move |row, column| {
            CellKey::new(calc, row, column).unwrap()
        })
    }

    /// The instrument, stopping the recalculation at the first read after an evaluation's calls
    /// and reads were retained, and saying what it held then.
    struct Stopping<'c> {
        inner: Instrument<'c>,
        held_when_stopped: Option<(usize, usize)>,
    }

    impl ReadObserver for Stopping<'_> {
        fn observe(
            &mut self,
            formula: CellKey,
            area: GridArea,
            offset: u64,
            run: CellRun<'_>,
        ) -> ControlFlow<()> {
            if !self.inner.retained.is_empty() {
                self.held_when_stopped =
                    Some((self.inner.evaluations.len(), self.inner.retained.len()));
                return ControlFlow::Break(());
            }
            self.inner.observe(formula, area, offset, run)
        }

        fn called(&mut self, formula: CellKey, call: ExecutedCall) {
            self.inner.called(formula, call);
        }

        fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
            self.inner.evaluated(evaluation);
        }
    }

    impl Recorder for Stopping<'_> {
        fn drop_all(&mut self) {
            self.inner.drop_all();
        }
    }

    /// A recalculation that fails after evaluations were recorded (the observer stops it at a
    /// read, `RecalcFailure::ReadFailed`, through the public `calc_host::step`) drops every
    /// evaluation and every retained call and read, and is the run's engine failure.
    #[test]
    fn a_recalculation_failing_after_evaluations_drops_them() {
        let (mut host, at) = opened();
        // Calc!A1's expectation differs from the 7.75 it calculates, so its calls and reads are
        // retained before the stop.
        let wrong = CalcValue::number(7.0);
        let mut expected = HashMap::new();
        expected.insert(at(1, 1), &wrong);
        let mut recorder = Stopping {
            inner: Instrument::new(expected),
            held_when_stopped: None,
        };
        let failure = recalculate(&mut host, &mut recorder);
        let (evaluations, retained) = recorder
            .held_when_stopped
            .expect("the observer stopped the recalculation");
        assert!(
            evaluations >= 1 && retained == 1,
            "held {evaluations} evaluations and {retained} retained when stopped"
        );
        let failure = failure.expect("the stopped recalculation is the run's failure");
        assert_eq!(
            (failure.class, failure.key.as_str()),
            (Class::Engine, "Failed:ReadFailed")
        );
        assert!(
            failure
                .message
                .starts_with("the recalculation published nothing: the recalculation failed"),
            "{}",
            failure.message
        );
        assert!(recorder.inner.evaluations.is_empty());
        assert!(recorder.inner.retained.is_empty());
        // The same workbook recalculated without the stop publishes and keeps its evaluations.
        let (mut host, at) = opened();
        let mut expected = HashMap::new();
        expected.insert(at(1, 1), &wrong);
        let mut instrument = Instrument::new(expected);
        assert_eq!(recalculate(&mut host, &mut instrument), None);
        // The workbook is macro-enabled, so Calc!B1's call of a name no function holds is refused
        // and never compiled; the other four formulas evaluate once, B2 stopping at B1.
        let mut cells: Vec<CellKey> = instrument.evaluations.iter().map(|seen| seen.at).collect();
        cells.sort();
        assert_eq!(cells, [at(1, 1), at(2, 1), at(2, 2), at(3, 1)]);
        assert_eq!(instrument.retained.len(), 1);
    }

    /// A host that applies iterative calculation is named with its iteration count and maximum
    /// change (L3); one that does not is named by the fingerprint without iteration.
    #[test]
    fn an_iterating_context_is_named_with_its_limits() {
        let mut applied = RecalcContext {
            date1904: false,
            iteration: true,
            max_iterations: 50,
            max_change_bits: 0.01f64.to_bits(),
            precision_as_displayed: false,
            numeric: NumericMode::Fast,
        };
        assert_eq!(
            run_context(&applied),
            format!(
                "excel-calc-context/1;date1904=false;iteration=true;max_iterations=50;max_change={:016x};precision_as_displayed=false",
                0.01f64.to_bits()
            )
        );
        applied.iteration = false;
        applied.date1904 = true;
        assert_eq!(
            run_context(&applied),
            "excel-calc-context/1;date1904=true;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false"
        );
    }

    /// The package's declared date system is a context key only for a version 1 capture and only
    /// when the run does not apply it.
    #[test]
    fn a_declared_date_system_is_keyed_for_a_version_1_capture() {
        let applied = RecalcContext {
            date1904: false,
            iteration: false,
            max_iterations: 100,
            max_change_bits: 0.001f64.to_bits(),
            precision_as_displayed: false,
            numeric: NumericMode::Fast,
        };
        let mut capture = CaptureRecord::decode(
            br#"{"format": "dnacalc-capture-record", "version": 1, "case": "k", "source": "sha256:00", "acquisition": {"kind": "file_cache_replay"}, "environment": null, "sheets": [{"name": "S", "visibility": "visible"}], "formulas": [{"sheet": "S", "row": 1, "column": 1, "kind": "ordinary"}], "observations": []}"#,
        )
        .unwrap();
        assert_eq!(
            declared_key(&capture, true, &applied).as_deref(),
            Some(
                "date1904: package declares true, run applies false (the version 1 capture names no context)"
            )
        );
        assert_eq!(declared_key(&capture, false, &applied), None);
        capture.session = Some(calc_verdict::CaptureSession {
            revision: 0,
            operations: Vec::new(),
            context: "c".to_owned(),
            tool: "t".to_owned(),
            input_package: "sha256:00".to_owned(),
            output_package: None,
            failures: Vec::new(),
            shared_ranges: Vec::new(),
            saved: Vec::new(),
            reopened: Vec::new(),
        });
        assert_eq!(declared_key(&capture, true, &applied), None);
    }

    #[test]
    fn arguments_name_each_input_once() {
        let request = parse_args(&args(
            "--fixture f.xlsx --capture c.json --out dir --exact-target 20430,ch,co,x86-64",
        ))
        .unwrap();
        assert_eq!(request.fixture, PathBuf::from("f.xlsx"));
        assert_eq!(request.capture, PathBuf::from("c.json"));
        assert_eq!(request.out, PathBuf::from("dir"));
        assert_eq!(
            request.target,
            NumericTargetId::new("20430", "ch", "co", "x86-64")
        );
        for (bad, refusal) in [
            ("--capture c.json --out d", "--fixture is required"),
            ("--fixture f.xlsx --out d", "--capture is required"),
            ("--fixture f.xlsx --capture c.json", "--out is required"),
            (
                "--fixture f --fixture g --capture c --out d",
                "--fixture is given twice",
            ),
            (
                "--fixture f --capture c --out d --results r",
                "unknown argument",
            ),
            ("--fixture f --capture c --out", "needs a value"),
            (
                "--fixture f --capture c --out d --exact-target 20430,ch",
                "is not <excel build>",
            ),
        ] {
            let error = parse_args(&args(bad)).unwrap_err();
            assert!(error.contains(refusal), "{bad}: {error}");
        }
        assert_eq!(run_code(&args("--fixture f.xlsx")), EXIT_USAGE);
    }

    /// A capture path holding the absent-capture marker exits 4; unreadable inputs exit 1; and
    /// nothing is written in either case.
    #[test]
    fn a_capture_not_taken_or_unreadable_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("xtask-compare-{}-absent", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("capture.json");
        fs::write(
            &marker,
            r#"{"format": "dnacalc-capture-absent", "version": 1, "case": "k", "artifact": "capture record", "fixture": "f.xlsx", "command": "cargo xtask capture --fixture f.xlsx --out capture.json"}"#,
        )
        .unwrap();
        let fixture = dir.join("f.xlsx");
        fs::write(&fixture, b"PK").unwrap();
        let out = dir.join("out");
        let failure = compare(&Request {
            fixture: fixture.clone(),
            capture: marker,
            out: out.clone(),
            target: None,
        })
        .unwrap_err();
        assert_eq!(failure.code, EXIT_MISSING_CAPTURE, "{}", failure.message);
        let failure = compare(&Request {
            fixture: dir.join("absent.xlsx"),
            capture: dir.join("absent.json"),
            out: out.clone(),
            target: None,
        })
        .unwrap_err();
        assert_eq!(failure.code, EXIT_REFUSED);
        assert!(
            failure.message.contains("cannot read the fixture"),
            "{}",
            failure.message
        );
        assert!(!out.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// INFO("system") through the product route: the catalogue holds INFO and its kernel refuses
    /// the host facts it does not model, so the cell is not modelled under its owner, not a
    /// missing function.
    #[test]
    fn a_kernel_refusal_is_not_modelled_under_its_owner() {
        let calc = super::test_package::calc_sheet(
            r#"<c r="C1" t="e"><f>INFO("system")</f><v>#VALUE!</v></c>"#,
        );
        let fixture = super::test_package::package(&super::test_package::inputs_sheet(""), &calc);
        let capture = CaptureRecord::decode(
            br#"{"format": "dnacalc-capture-record", "version": 1, "case": "k", "source": "sha256:00", "acquisition": {"kind": "file_cache_replay"}, "environment": null, "sheets": [{"name": "Inputs", "visibility": "visible"}, {"name": "Calc", "visibility": "visible"}], "formulas": [{"sheet": "Calc", "row": 1, "column": 3, "kind": "ordinary"}], "observations": []}"#,
        )
        .unwrap();
        let run = run_workbook(&fixture, None, &capture, NumericMode::Fast).unwrap();
        let Dna::Refused(refusal) = &run.dna[0] else {
            panic!("Calc!C1 is {:?}", run.dna[0]);
        };
        assert_eq!(
            (
                refusal.class,
                refusal.key.as_str(),
                refusal.owner.as_str(),
                refusal.refusal.as_str()
            ),
            (
                Class::NotModelled,
                "INFO",
                "exactness dc-uar",
                "Evaluation:Function:NotImplemented"
            )
        );
    }

    /// The keys of a refused package and of a failed recalculation name the typed error's variant.
    #[test]
    fn workbook_failures_are_keyed_by_their_variant() {
        use calc_host::CellError;
        let failure = OpenFailure::Workbook(LoadError::Cell {
            part: "xl/worksheets/sheet1.xml".to_owned(),
            row: 1,
            column: 1,
            error: CellError::UnsupportedType {
                kind: "d".to_owned(),
            },
        });
        assert_eq!(open_key(&failure), "Workbook:Cell:UnsupportedType");
        assert_eq!(
            open_key(&OpenFailure::Workbook(LoadError::MissingPart {
                part: "p".to_owned()
            })),
            "Workbook:MissingPart"
        );
        assert_eq!(
            step_key(&RecalcFailed::Refused(calc_host::StartError::Cycle {
                cells: Vec::new()
            })),
            "Refused:Cycle"
        );
    }
}
