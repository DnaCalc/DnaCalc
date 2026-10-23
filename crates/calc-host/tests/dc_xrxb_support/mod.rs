//! The case dc_xrxb__unprefixed_file_spelling_gives_name: a package whose formulas call functions
//! a file stores with the prefix `_xlfn.`, written with and without it, opened through
//! `HostState::open`, recalculated through `calc_host::step` and projected through
//! `calc_host::project`, every formula cell judged through `calc_verdict::judge` against Excel's
//! capture of the same package.
//!
//! The package is calc-func's dc-xrxb oracle workbook (`crates/calc-func/tests/fixtures/dc_xrxb/
//! oracle.xlsx`, from calc-func's committed generator), and the capture is the one Excel took of
//! it (`crates/calc-func/tests/captures/dc_xrxb/oracle.capture.json`). Its sheet file_spelling
//! holds the calls this case names, and its sheet prefix_metadata calls every catalogued named
//! function with and without the prefix. No expected value is written here: each is Excel's raw
//! value in the capture, except the typed entry at the end, whose value is derived by hand.

use calc_engine::ObservedEvaluation;
use calc_fml::reference::{FormulaAnchor, ReferenceNotation, ReferenceReader};
use calc_fml::{EvalFrame, EvalScratch, EvaluationServices, PlanCache, evaluate, instantiate};
use calc_host::value::{
    CalcValue, CellKey, CoreValue, GridArea, IdAllocator, NumericMode, SheetId, WorksheetErrorCode,
};
use calc_host::{
    CellInput, CellRun, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy,
    PackageRead, ProjectedValue, ProjectionRequest, ReadObserver, WorkBudget, project, step,
};
use calc_verdict::{
    Acquisition, CalculationContext, CaptureRecord, CellAddress, CellResult, Evaluation,
    InstrumentRecord, InstrumentedEvaluation, Observation, Provenance, ResultSet, RunIdentity,
    Verdict, VerdictPolicy, judge, sha256_hex,
};
use std::collections::{BTreeMap, HashMap};
use std::ops::ControlFlow;

/// The prefix_metadata cells the host refuses, each with the reason and the bead that owns it:
/// every other formula cell of the workbook must match Excel, the INDIRECT cells A26 and B26
/// included since the engine binds INDIRECT's text against the workbook's sheets (dc-0k9).
/// ISFORMULA asks whether a cell holds a formula, a host fact the cell reader does not supply
/// (dc-ip6). Unprefixed, ISFORMULA is #NAME? on load and computes.
const REFUSED: [(&str, &str); 1] = [("A32", "ISFORMULA, a host fact (dc-ip6)")];
use std::sync::Arc;

const CASE: &str = "dc_xrxb__unprefixed_file_spelling_gives_name";
/// calc-func's dc-xrxb oracle workbook, as committed.
const ORACLE: &[u8] = include_bytes!("../../../calc-func/tests/fixtures/dc_xrxb/oracle.xlsx");
/// Excel's capture of it.
const CAPTURE: &[u8] =
    include_bytes!("../../../calc-func/tests/captures/dc_xrxb/oracle.capture.json");

/// The evaluations the evaluator reported during the recalculation: the cell, revision and tick of
/// each, and what it returned.
#[derive(Default)]
struct Evaluations {
    seen: Vec<(CellKey, u64, u64, Result<CalcValue, String>)>,
}

impl ReadObserver for Evaluations {
    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
        let calculation = evaluation.calculation();
        self.seen.push((
            evaluation.formula(),
            calculation.revision().get(),
            calculation.tick().get(),
            evaluation
                .outcome()
                .cloned()
                .map_err(|fault| format!("CalcFml refused: {fault:?}")),
        ));
    }
}

/// A value as the notes print it.
fn shown(value: &CalcValue) -> String {
    match value.core() {
        CoreValue::Number(number) => format!("{number}"),
        CoreValue::Text(text) => format!("{:?}", text.to_string_lossy()),
        CoreValue::Logical(logical) => logical.to_string().to_uppercase(),
        CoreValue::Error(code) => format!("{code:?}"),
        other => format!("{other:?}"),
    }
}

fn name_error() -> CalcValue {
    CalcValue::error(WorksheetErrorCode::Name)
}

/// The cells of the sheet Data a typed formula reads, held as the oracle workbook holds them.
struct DataReader {
    cells: HashMap<CellKey, CalcValue>,
}

impl ReferenceReader for DataReader {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let (first, last) = (area.first(), area.last());
        for row in first.row()..=last.row() {
            for column in first.column()..=last.column() {
                let at = CellKey::new(area.sheet(), row, column).expect("a cell of the area");
                let run = match self.cells.get(&at) {
                    Some(value) => CellRun::Values(std::slice::from_ref(value)),
                    None => CellRun::Blank(1),
                };
                if sink(run).is_break() {
                    return;
                }
            }
        }
    }
}

struct DataCatalog(SheetId);

impl calc_fml::ReferenceCatalog for DataCatalog {
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        name.eq_ignore_ascii_case("Data").then_some(self.0)
    }
}

/// A compile entry of calc_fml: calc_fml::compile or calc_fml::compile_loaded.
type Compile = fn(
    &calc_fml::syntax::SyntaxTree,
    ReferenceNotation,
    FormulaAnchor,
    &dyn calc_fml::ReferenceCatalog,
    &mut PlanCache,
) -> Result<Arc<calc_fml::CompiledPlan>, calc_fml::CompileError>;

/// `text` compiled by `compile` at Calc!A1 and evaluated through calc_fml::evaluate over Data's
/// cells.
fn typed_value(text: &str, compile: Compile) -> CalcValue {
    let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (
        sheets.issue().expect("a sheet id"),
        sheets.issue().expect("a sheet id"),
    );
    let cell = |row, column| CellKey::new(data, row, column).expect("a cell of Data");
    let text_value =
        |text: &str| CalcValue::text(calc_host::value::ExcelText::from_interop_assignment(text));
    let mut reader = DataReader {
        cells: HashMap::from([
            (cell(1, 1), CalcValue::number(1.0)),
            (cell(2, 1), CalcValue::number(5.0)),
            (cell(3, 1), CalcValue::number(3.0)),
            (cell(1, 2), text_value("x")),
            (cell(2, 2), text_value("y")),
            (cell(3, 2), text_value("x")),
        ]),
    };
    let anchor = FormulaAnchor::new(CellKey::new(calc, 1, 1).expect("Calc!A1"));
    let plan = compile(
        &calc_fml::parse_formula(text),
        ReferenceNotation::A1,
        anchor,
        &DataCatalog(data),
        &mut PlanCache::new(),
    )
    .unwrap_or_else(|refused| panic!("{text}: {refused:?}"));
    let instance = instantiate(&plan, anchor);
    let mut scratch = EvalScratch::new();
    evaluate(
        EvalFrame::new(&instance, &mut scratch),
        &mut EvaluationServices::new(&NumericMode::Fast, &mut reader),
    )
    .unwrap_or_else(|fault| panic!("{text}: {fault:?}"))
}

/// Runs the case and returns its notes.
pub fn unprefixed_file_spelling_gives_name() -> Vec<String> {
    let mut notes = Vec::new();
    let capture = CaptureRecord::decode(CAPTURE).expect("the capture record decodes");
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    assert_eq!(
        capture.source,
        format!("sha256:{}", sha256_hex(ORACLE)),
        "the capture is of the committed oracle workbook"
    );

    // Open, recalculate to the end and project every formula cell of the capture's inventory.
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(ORACLE.to_vec()));
    let mut host = HostState::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("the oracle workbook opens");
    let applied = host.recalc_context();
    assert!(!applied.iteration);
    let context =
        CalculationContext::without_iteration(applied.date1904, applied.precision_as_displayed)
            .fingerprint();
    let mut evaluations = Evaluations::default();
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut evaluations) {
            HostProgress::Calculating { .. } => {}
            HostProgress::Published(_) | HostProgress::Current => break,
            HostProgress::Failed(failure) => panic!("the recalculation failed: {failure}"),
        }
    }
    let ids: HashMap<String, SheetId> = host
        .sheets()
        .iter()
        .map(|sheet| (sheet.name().text().to_string_lossy(), sheet.id()))
        .collect();
    let names: HashMap<SheetId, &str> = ids.iter().map(|(name, id)| (*id, name.as_str())).collect();
    let key_of = |cell: &CellAddress| {
        CellKey::new(ids[cell.sheet()], cell.row(), cell.column()).expect("a cell of the grid")
    };
    let request = ProjectionRequest {
        cells: capture
            .formulas
            .iter()
            .map(|formula| key_of(&formula.cell))
            .collect(),
    };
    let page = project(&host, &request).expect("the inventory projects");
    let mut results = Vec::new();
    let mut calculated: BTreeMap<(String, u32, u32), CalcValue> = BTreeMap::new();
    let mut loaded_text: BTreeMap<(String, u32, u32), String> = BTreeMap::new();
    for (formula, projected) in capture.formulas.iter().zip(page.cells()) {
        let cell = formula.cell.clone();
        match projected.input() {
            Some(CellInput::Formula(text)) => loaded_text.insert(
                (cell.sheet().to_owned(), cell.row(), cell.column()),
                text.text().to_string_lossy(),
            ),
            other => panic!(
                "{}!{}: the host holds {other:?}, not the file's formula",
                cell.sheet(),
                cell.a1()
            ),
        };
        let (provenance, value) = match projected.value() {
            ProjectedValue::Calculated { value, calculation } => {
                calculated.insert(
                    (cell.sheet().to_owned(), cell.row(), cell.column()),
                    value.clone(),
                );
                (
                    Provenance::Calculated {
                        tick: calculation.tick().get(),
                        revision: calculation.revision().get(),
                        context: context.clone(),
                    },
                    Some(value.clone()),
                )
            }
            ProjectedValue::NotModeled { refusal, .. } => (
                Provenance::NotModeled {
                    feature: format!("{refusal:?}"),
                    diagnostic: refusal.to_string(),
                },
                None,
            ),
            other => panic!(
                "{}!{}: the host projects {other:?}, neither calculated nor refused",
                cell.sheet(),
                cell.a1()
            ),
        };
        results.push(CellResult {
            cell,
            provenance,
            value,
        });
    }
    let record = InstrumentRecord {
        case: capture.case.clone(),
        source: capture.source.clone(),
        evaluations: evaluations
            .seen
            .iter()
            .map(|(at, revision, tick, returned)| {
                InstrumentedEvaluation::new(
                    Evaluation {
                        cell: CellAddress::new(names[&at.sheet()], at.row(), at.column())
                            .expect("a cell of the grid"),
                        tick: *tick,
                        revision: *revision,
                        context: context.clone(),
                    },
                    returned.clone(),
                )
            })
            .collect(),
    };
    let results = ResultSet {
        case: capture.case.clone(),
        source: capture.source.clone(),
        run: RunIdentity {
            revision: host.revision().get(),
            context: context.clone(),
        },
        results,
    };
    let report = judge(
        &capture,
        &results,
        &record,
        &VerdictPolicy {
            numeric_mode: NumericMode::Fast,
            evidence: None,
        },
    )
    .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    let refused: Vec<String> = report
        .cells
        .iter()
        .filter(|cell| matches!(cell.verdict, Verdict::NotModeled { .. }))
        .filter(|cell| {
            cell.formula.cell.sheet() == "prefix_metadata"
                && REFUSED.iter().any(|(a1, _)| *a1 == cell.formula.cell.a1())
        })
        .map(|cell| cell.formula.cell.a1())
        .collect();
    assert_eq!(
        refused,
        REFUSED.iter().map(|(a1, _)| *a1).collect::<Vec<_>>(),
        "the prefix_metadata cells the host refuses"
    );
    let unmatched: Vec<String> = report
        .cells
        .iter()
        .filter(|cell| !matches!(cell.verdict, Verdict::Matched { .. }))
        .filter(|cell| {
            !(cell.formula.cell.sheet() == "prefix_metadata"
                && refused.contains(&cell.formula.cell.a1()))
        })
        .map(|cell| {
            format!(
                "{}!{} {:?}: {:?}",
                cell.formula.cell.sheet(),
                cell.formula.cell.a1(),
                cell.formula.formula,
                cell.verdict
            )
        })
        .collect();
    assert!(
        unmatched.is_empty() && report.outside_inventory.is_empty(),
        "{} of {} formula cells not matched with Excel:\n{}",
        unmatched.len(),
        report.cells.len(),
        unmatched.join("\n")
    );
    notes.push(format!(
        "ACCEPT-NOTE: {CASE}: calc-func's dc-xrxb oracle workbook (sha256 {}) opened through HostState::open, recalculated through calc_host::step and projected through calc_host::project; calc_verdict::judge against Excel's capture of it: {}; refused, each by its owner: {}",
        sha256_hex(ORACLE),
        report.summary(),
        REFUSED
            .iter()
            .map(|(a1, why)| format!("prefix_metadata!{a1} {why}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // What the bead names, read from the run and from Excel's raw values: the unprefixed MAXIFS
    // and IFNA of a file give #NAME?, their prefixed spellings compute.
    let excel = |sheet: &str, row: u32, column: u32| -> &CalcValue {
        capture
            .observations
            .iter()
            .find(|observed| {
                observed.cell.sheet() == sheet
                    && observed.cell.row() == row
                    && observed.cell.column() == column
            })
            .and_then(|observed| match &observed.observation {
                Observation::ExcelRaw(value) => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{sheet} row {row}: no raw observation"))
    };
    let ours = |sheet: &str, row: u32, column: u32| -> &CalcValue {
        calculated
            .get(&(sheet.to_owned(), row, column))
            .unwrap_or_else(|| panic!("{sheet} row {row}: not calculated"))
    };
    let named = [
        (1, "MAXIFS(Data!A1:A3,Data!B1:B3,\"x\")", name_error()),
        (2, "IFNA(Data!C1,7)", name_error()),
        (
            3,
            "_xlfn.MAXIFS(Data!A1:A3,Data!B1:B3,\"x\")",
            CalcValue::number(3.0),
        ),
        (4, "_xlfn.IFNA(Data!C1,7)", CalcValue::number(7.0)),
    ];
    for (row, text, value) in &named {
        assert_eq!(excel("file_spelling", *row, 4), value, "{text}: Excel");
        assert_eq!(ours("file_spelling", *row, 4), value, "{text}: the host");
    }
    let mut rows = Vec::new();
    for formula in &capture.formulas {
        if formula.cell.sheet() == "file_spelling" {
            let (row, column) = (formula.cell.row(), formula.cell.column());
            rows.push(format!(
                "D{row} ={} gives {}",
                loaded_text[&("file_spelling".to_owned(), row, column)],
                shown(ours("file_spelling", row, column))
            ));
        }
    }
    notes.push(format!(
        "ACCEPT-NOTE: {CASE}: sheet file_spelling, each formula as the file and the host hold it, calculated by the host and matched with Excel: {}",
        rows.join("; ")
    ));
    let mut unknown_on_load = Vec::new();
    let mut by_plain_name = 0;
    for formula in &capture.formulas {
        if formula.cell.sheet() == "prefix_metadata"
            && formula.cell.column() == 2
            && !refused.contains(&formula.cell.a1())
        {
            let row = formula.cell.row();
            if ours("prefix_metadata", row, 2) == &name_error() {
                unknown_on_load.push(format!(
                    "={}",
                    loaded_text[&("prefix_metadata".to_owned(), row, 2)]
                ));
            } else {
                by_plain_name += 1;
            }
        }
    }
    notes.push(format!(
        "ACCEPT-NOTE: {CASE}: sheet prefix_metadata: every _xlfn.-prefixed call computed and matched but the refused ones above; of the other unprefixed calls, {by_plain_name} computed and matched, and {} gave #NAME? as in Excel: {}",
        unknown_on_load.len(),
        unknown_on_load.join(" ")
    ));

    // Typed entry: the same text, compiled as a user enters it (calc_fml::compile, the compile
    // DNA OneCalc's session runs), computes; compiled as a file stores it, it gives #NAME?.
    // MAXIFS of A1:A3 = 1, 5, 3 where B1:B3 = "x", "y", "x" is max(1, 3) = 3.
    let typed = "=MAXIFS(Data!A1:A3,Data!B1:B3,\"x\")";
    assert_eq!(
        typed_value(typed, calc_fml::compile),
        CalcValue::number(3.0)
    );
    assert_eq!(typed_value(typed, calc_fml::compile_loaded), name_error());
    notes.push(format!(
        "ACCEPT-NOTE: {CASE}: typed {typed} through calc_fml::compile and calc_fml::evaluate gives 3 (derived by hand); the same text through calc_fml::compile_loaded gives #NAME?"
    ));
    notes
}
