//! Shared helpers of the F1 test files: a call through `calc_func::invoke` by spelling, a sheet
//! of cells that reference arguments read, and the value builders.

#![allow(dead_code)]

use calc_func::value::{
    CalcArray, CalcValue, CellKey, CoreValue, ExcelText, GridArea, IdAllocator, NumericMode,
    ReferenceLike, SheetId, WorksheetErrorCode,
};
use calc_func::{
    CellReader, EvalServices, FunctionCallScratch, FunctionFault, Notation, PreparedArguments,
    RandomSource, SubtotalExclusions, Unreadable, catalogue, invoke,
};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

pub struct Given(pub Vec<CalcValue>);

impl PreparedArguments for Given {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

/// A sheet of cells, keyed by (row, column), 1-based.
pub struct Sheet {
    pub id: SheetId,
    pub cells: BTreeMap<(u32, u32), CalcValue>,
    /// What the sheet reports to SUBTOTAL and AGGREGATE, or `None` when it cannot say.
    pub exclusions: Option<SubtotalExclusions>,
}

impl Sheet {
    pub fn new() -> Self {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        Sheet {
            id: ids.issue().expect("a sheet id"),
            cells: BTreeMap::new(),
            exclusions: None,
        }
    }

    /// Sets the cells of column `column` from row `row` down.
    pub fn column(&mut self, row: u32, column: u32, values: Vec<CalcValue>) -> &mut Self {
        for (i, value) in values.into_iter().enumerate() {
            self.cells.insert((row + i as u32, column), value);
        }
        self
    }

    pub fn key(&self, row: u32, column: u32) -> CellKey {
        CellKey::new(self.id, row, column).expect("a cell")
    }

    /// The reference to rows `r1..=r2` and columns `c1..=c2`.
    pub fn area(&self, r1: u32, c1: u32, r2: u32, c2: u32) -> CalcValue {
        let area = GridArea::new(self.key(r1, c1), self.key(r2, c2)).expect("an area");
        CalcValue::reference(ReferenceLike::area(area))
    }
}

impl CellReader for Sheet {
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        for (&(row, column), value) in &self.cells {
            let key = CellKey::new(self.id, row, column).expect("a held cell");
            if area.contains(key) && visit(key, value).is_break() {
                break;
            }
        }
        Ok(())
    }

    fn subtotal_exclusions(&mut self, _area: GridArea) -> Option<SubtotalExclusions> {
        self.exclusions.clone()
    }
}

/// A fixed sequence of random numbers.
pub struct Fixed(pub Vec<f64>);

impl RandomSource for Fixed {
    fn next_unit(&mut self) -> f64 {
        self.0.remove(0)
    }
}

/// `spelling` called on `args` with no cell reader.
pub fn call(spelling: &str, args: Vec<CalcValue>) -> Result<CalcValue, FunctionFault> {
    let target = catalogue()
        .resolve_surface(Notation::Call, spelling)
        .unwrap_or_else(|_| panic!("{spelling} is catalogued"));
    let mode = NumericMode::Fast;
    invoke(
        target,
        &mut Given(args),
        &mut EvalServices::new(&mode),
        &mut FunctionCallScratch::new(),
    )
}

/// `spelling` called on `args`, reading references from `sheet`.
pub fn call_on(
    sheet: &mut Sheet,
    spelling: &str,
    args: Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    let target = catalogue()
        .resolve_surface(Notation::Call, spelling)
        .unwrap_or_else(|_| panic!("{spelling} is catalogued"));
    let mode = NumericMode::Fast;
    invoke(
        target,
        &mut Given(args),
        &mut EvalServices::with_cells(&mode, sheet),
        &mut FunctionCallScratch::new(),
    )
}

/// `spelling` called on `args` with `random` as the random source.
pub fn call_random(
    random: &mut Fixed,
    spelling: &str,
    args: Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    let target = catalogue()
        .resolve_surface(Notation::Call, spelling)
        .unwrap_or_else(|_| panic!("{spelling} is catalogued"));
    let mode = NumericMode::Fast;
    invoke(
        target,
        &mut Given(args),
        &mut EvalServices::new(&mode).with_random(random),
        &mut FunctionCallScratch::new(),
    )
}

/// `spelling` called on `args` in `environment` (F2: dates, locale), reading references from
/// `sheet` when one is given and asking `host` for host facts when one is given.
pub fn call_in(
    environment: &calc_func::locale::CalcEnvironment,
    sheet: Option<&mut Sheet>,
    host: Option<&mut dyn calc_func::HostFacts>,
    spelling: &str,
    args: Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    let target = catalogue()
        .resolve_surface(Notation::Call, spelling)
        .unwrap_or_else(|_| panic!("{spelling} is catalogued"));
    let mode = NumericMode::Fast;
    let services = match sheet {
        Some(sheet) => EvalServices::with_cells(&mode, sheet),
        None => EvalServices::new(&mode),
    };
    let mut services = services.with_environment(environment);
    if let Some(host) = host {
        services = services.with_host(host);
    }
    invoke(
        target,
        &mut Given(args),
        &mut services,
        &mut FunctionCallScratch::new(),
    )
}

pub fn n(x: f64) -> CalcValue {
    CalcValue::number(x)
}

pub fn t(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

pub fn b(x: bool) -> CalcValue {
    CalcValue::logical(x)
}

pub fn e(code: WorksheetErrorCode) -> CalcValue {
    CalcValue::error(code)
}

pub fn missing() -> CalcValue {
    CalcValue::missing()
}

/// A row-major array value.
pub fn arr(rows: Vec<Vec<f64>>) -> CalcValue {
    CalcValue::array(
        CalcArray::from_rows(
            rows.into_iter()
                .map(|row| row.into_iter().map(CalcValue::number).collect())
                .collect(),
        )
        .expect("a rectangular array"),
    )
}

/// A column array of numbers.
pub fn col(values: &[f64]) -> CalcValue {
    arr(values.iter().map(|&v| vec![v]).collect())
}

/// A row-major array of any values.
pub fn arr_values(rows: Vec<Vec<CalcValue>>) -> CalcValue {
    CalcValue::array(CalcArray::from_rows(rows).expect("a rectangular array"))
}

/// The number a call computed.
pub fn number(result: Result<CalcValue, FunctionFault>) -> f64 {
    match result {
        Ok(value) => match value.core() {
            CoreValue::Number(n) => *n,
            other => panic!("expected a number, got {other:?}"),
        },
        Err(fault) => panic!("expected a number, got {fault:?}"),
    }
}

/// Asserts the call computed a number within `rel` relative (or absolute, near 0) distance.
#[track_caller]
pub fn close(result: Result<CalcValue, FunctionFault>, expected: f64, rel: f64) {
    let got = number(result);
    let tolerance = rel * expected.abs().max(1e-300);
    assert!(
        (got - expected).abs() <= tolerance || got == expected,
        "got {got:e}, expected {expected:e} (rel {rel:e})"
    );
}

/// Asserts the call computed exactly this number.
#[track_caller]
pub fn exact(result: Result<CalcValue, FunctionFault>, expected: f64) {
    let got = number(result);
    assert_eq!(
        got.to_bits(),
        expected.to_bits(),
        "got {got:e}, expected {expected:e}"
    );
}

/// Pin a calculation while retaining the recorded Excel answer as a separate fact.
/// A known mismatch remains a failure in the witness and corpus summaries.
#[track_caller]
pub fn recorded_calculation(result: Result<CalcValue, FunctionFault>, excel: f64, calculated: f64) {
    let got = number(result);
    assert_eq!(
        got.to_bits(),
        calculated.to_bits(),
        "calculated {got:e}; pinned calculation {calculated:e}; recorded Excel {excel:e} ({:#018x})",
        excel.to_bits()
    );
}

/// Asserts the call computed this worksheet error.
#[track_caller]
pub fn error(result: Result<CalcValue, FunctionFault>, code: WorksheetErrorCode) {
    match result {
        Ok(value) => assert_eq!(value.as_error(), Some(code), "got {value:?}"),
        Err(fault) => panic!("expected {code:?}, got {fault:?}"),
    }
}

/// Asserts the call computed this text.
#[track_caller]
pub fn text(result: Result<CalcValue, FunctionFault>, expected: &str) {
    match result {
        Ok(value) => match value.core() {
            CoreValue::Text(text) => assert_eq!(text.to_string_lossy(), expected),
            other => panic!("expected text {expected:?}, got {other:?}"),
        },
        Err(fault) => panic!("expected text {expected:?}, got {fault:?}"),
    }
}

/// Asserts the call was refused as not modelled.
#[track_caller]
pub fn refused(result: Result<CalcValue, FunctionFault>) {
    match result {
        Err(FunctionFault::NotImplemented { .. }) => {}
        other => panic!("expected a not-modelled refusal, got {other:?}"),
    }
}

/// The numbers of an array result, row-major, with its shape.
pub fn array(result: Result<CalcValue, FunctionFault>) -> (usize, usize, Vec<f64>) {
    let value = result.expect("an array result");
    let CoreValue::Array(array) = value.core() else {
        panic!("expected an array, got {value:?}");
    };
    let shape = array.shape();
    let cells = array
        .iter_row_major()
        .map(|cell| match cell.core() {
            CoreValue::Number(n) => *n,
            other => panic!("expected numbers, got {other:?}"),
        })
        .collect();
    (shape.rows, shape.cols, cells)
}
