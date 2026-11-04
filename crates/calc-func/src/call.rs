//! The call interface: [`invoke`] runs a resolved catalogue entry on the arguments its caller
//! supplies through [`PreparedArguments`], with services ([`EvalServices`]) and scratch storage
//! ([`FunctionCallScratch`]) that the caller owns and reuses.
//!
//! The entry's declarations decide the order of work: the arity is checked before any argument
//! is asked for; an eager entry's arguments are asked for once each, in order, and admitted
//! against their parameter classes before its kernel runs; a lazy entry's kernel asks for the
//! arguments it selects itself, each admitted when it is asked for, so an argument it does not
//! select is never evaluated.

use crate::catalogue::{CatalogueEntry, FunctionCallTarget, FunctionId, Kernel, ParameterClass};
use crate::locale::CalcEnvironment;
use crate::value::{
    CalcValue, CellKey, CoreRef, GridArea, NumericMode, SheetId, WorksheetErrorCode,
};
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

/// The arguments of one call. The caller produces each value when the call interface asks for
/// it, so a lazy entry's held-back arguments are never evaluated by the interface.
pub trait PreparedArguments {
    /// The number of arguments at the call site, omitted ones included.
    fn count(&self) -> usize;

    /// Argument `index`, below [`PreparedArguments::count`]. An omitted argument is
    /// [`CalcValue::missing`].
    fn argument(&mut self, index: usize) -> CalcValue;
}

/// Typed read access to the cells that a reference argument names, supplied by the caller with
/// the services. A kernel reads a reference only through it and never reaches workbook state.
pub trait CellReader {
    /// Calls `visit` for the cells of `area` that hold a value, in row-major order (each row left
    /// to right, rows top to bottom), until `visit` returns [`ControlFlow::Break`]; it then
    /// returns `Ok`. A cell it does not visit is blank, and it may also visit a blank cell as
    /// [`CoreRef::Empty`]. [`Unreadable`] when it cannot supply the values of `area`.
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable>;

    /// What SUBTOTAL and AGGREGATE leave out of `area`: the cells holding a SUBTOTAL or
    /// AGGREGATE formula, and the rows that are hidden or filtered out. An explicit `None` when
    /// the reader cannot say; those functions then refuse a reference argument as not modelled
    /// rather than count cells Excel would leave out.
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions>;
}

/// The cells of an area that SUBTOTAL and AGGREGATE leave out, as a [`CellReader`] reports them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtotalExclusions {
    /// The cells whose formula is a SUBTOTAL or AGGREGATE call.
    pub nested_subtotals: Vec<CellKey>,
    /// The rows hidden by hand (SUBTOTAL's function numbers 101 to 111 leave them out).
    pub hidden_rows: Vec<u32>,
    /// The rows a filter hides (every function number leaves them out).
    pub filtered_rows: Vec<u32>,
}

/// A source of uniform random numbers for RAND, RANDBETWEEN and RANDARRAY, which the caller
/// supplies with the services. Without one those functions are refused as not modelled.
pub trait RandomSource {
    /// A number in [0, 1).
    fn next_unit(&mut self) -> f64;
}

/// A [`CellReader`]'s refusal to supply the values of an area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// Resolves reference text, as INDIRECT reads it, in the calling formula's workbook context: its
/// sheets, and its cell, from which relative R1C1 text is offset. calc-func holds neither, and
/// no reference grammar, so the caller supplies the resolution with the services
/// ([`EvalServices::with_reference_text`]); calc-fml implements it with its reference binder.
pub trait ReferenceText {
    /// The reference `text` (UTF-16 code units) names, read in A1 notation when `a1` is true and
    /// in R1C1 notation otherwise.
    fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference;
}

/// What a [`ReferenceText`] resolution gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextReference {
    /// The text names this area.
    Area(GridArea),
    /// The text names no reference: the worksheet error #REF!.
    Invalid,
    /// What the text names is not modelled (a defined name, another workbook), and the call is
    /// refused.
    NotModelled,
}

/// Facts about the workbook and its cells that only the host holds, which CELL, INFO, ISFORMULA,
/// FORMULATEXT, SHEET and SHEETS read (ported from OxFunc's `host_info.rs` HostInfoProvider, at
/// 6fca3518). Every question is required. An explicit `None` refuses a fact the host cannot supply.
pub trait HostFacts {
    /// Whether `sheet` is a sheet of another workbook, read from the workbook's link to it
    /// (dc-ylr): Excel 16.0.20527 (2026-10-30, the other book closed) gives SUMIF, COUNTIF,
    /// SUMIFS, COUNTIFS, AVERAGEIF and OFFSET #VALUE! for a reference to it, and CELL, ISFORMULA
    /// and FORMULATEXT #N/A. The default says no sheet is.
    fn is_external_sheet(&mut self, sheet: SheetId) -> bool;

    /// The cell whose formula is being evaluated.
    fn caller(&mut self) -> Option<CellKey>;

    /// Whether `cell` holds a formula.
    fn is_formula(&mut self, cell: CellKey) -> Option<bool>;

    /// The formula `cell` holds, as Excel shows it (with its leading `=`), `Some(None)` when it
    /// holds none.
    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>>;

    /// The one-based position of `sheet` among the workbook's sheets.
    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32>;

    /// The sheet named `name` (any case).
    fn sheet_named(&mut self, name: &str) -> Option<SheetId>;

    /// The name of `sheet`.
    fn sheet_name(&mut self, sheet: SheetId) -> Option<String>;

    /// The current workbook identity in the pinned model, including a successful Save As.
    /// This is namespace metadata, independent of the file facts used by CELL and INFO.
    fn workbook_name(&mut self) -> Option<String> {
        None
    }

    /// How many sheets the workbook holds.
    fn sheet_count(&mut self) -> Option<u32>;

    /// The host's facts about the workbook's file, formats and views and about the application
    /// ([`WorkbookFacts`]), which CELL and INFO read (dc-uar).
    fn workbook_facts(&mut self) -> Option<Arc<dyn WorkbookFacts>>;
}

/// Facts about the workbook that only its host holds, beyond the cells' values: the file it was
/// opened from, each cell's format, the columns' widths, the sheets' views, the calculation mode
/// and the application (dc-uar). CELL reads the first four, INFO the rest. Every question is required; an explicit `None` refuses an unavailable fact. The host gives one snapshot per recalculation, shared by the threads that
/// evaluate it.
pub trait WorkbookFacts: Send + Sync + fmt::Debug {
    /// The format of `cell`: its number format code, horizontal alignment and whether it is
    /// locked.
    fn cell_format(&self, cell: CellKey) -> Option<CellFormatFacts>;

    /// The width of `column` (1-based) of `sheet` as Excel shows it.
    fn column_width(&self, sheet: SheetId, column: u32) -> Option<ColumnWidth>;

    /// The workbook's file: its folder and file name.
    fn file(&self) -> Option<WorkbookFile>;

    /// The active cell of the workbook's active sheet, which CELL without a reference reads.
    fn active_cell(&self) -> Option<CellKey>;

    /// The top left cell of `sheet`'s window, which INFO("origin") reads.
    fn top_left_cell(&self, sheet: SheetId) -> Option<CellKey>;

    /// The calculation mode, which INFO("recalc") reads.
    fn calculation_mode(&self) -> Option<CalculationMode>;

    /// The facts about the application and its platform that INFO reads.
    fn application(&self) -> Option<ApplicationFacts>;
}

/// A cell's format as CELL reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellFormatFacts {
    /// The number format code, as the file writes it (`General`, `0.00`, ...).
    pub number_format: String,
    /// The built-in number format ID the cell's format names, when the workbook does not declare
    /// that ID itself: the locale-dependent ones (14, 22) class by the locale's own code.
    pub builtin: Option<u32>,
    pub horizontal: HorizontalAlignment,
    /// Whether the cell is locked (protection's `locked`, which is true unless a format clears
    /// it).
    pub locked: bool,
}

/// A cell format's horizontal alignment (`<alignment horizontal>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalAlignment {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}

/// A column's width as Excel shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnWidth {
    /// The width in characters of the default font's widest digit, as Excel's column width
    /// shows it (8.43 for the default column of a Calibri or Aptos Narrow 11 workbook); 0 for a
    /// hidden column.
    pub characters: f64,
    /// Whether the column has the sheet's default width (no width of its own).
    pub default: bool,
}

/// The workbook's file as CELL("filename") and a reference to another sheet name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookFile {
    /// The folder the file was opened from, with its trailing separator; `None` for a workbook
    /// never saved, whose file name CELL("filename") gives as empty text.
    pub folder: Option<String>,
    /// The file name (`Book1.xlsx`), or a never-saved workbook's name (`Book1`).
    pub name: String,
}

/// The calculation mode INFO("recalc") reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculationMode {
    Automatic,
    AutomaticExceptTables,
    Manual,
}

/// The facts about the application and its platform that INFO reads; a `None` field refuses its
/// type as not modelled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplicationFacts {
    /// INFO("release"): the application's version (`16.0`).
    pub release: Option<String>,
    /// INFO("osversion"): the operating system (`Windows (64-bit) NT 10.00`).
    pub os_version: Option<String>,
    /// INFO("system"): `pcdos` on Windows, `mac` on a Mac.
    pub system: Option<String>,
    /// INFO("directory"): the current folder, with its trailing separator.
    pub directory: Option<String>,
}

/// Calls LAMBDA values, as MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY call the LAMBDA they
/// are given. calc-func holds no formula evaluator, so the caller supplies one with the services
/// ([`EvalServices::with_callables`]); calc-fml implements it with its own evaluator. Without one
/// those functions are refused as not modelled.
pub trait CallableInvoker {
    /// The value of calling `callable` (a value whose [`CalcValue::callable_value`] is set) with
    /// `arguments`. A call that is not a LAMBDA call's value (a callable of another formula, a
    /// call of the wrong number of arguments) is the worksheet error Excel gives it. When the
    /// evaluation of the call stops with a fault, the invoker keeps the fault for its caller and
    /// returns [`CallAbandoned`]; the kernel then stops, and the caller reports the kept fault.
    fn call(
        &mut self,
        callable: &CalcValue,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, CallAbandoned>;

    /// Advances the same helper invocation. `None` retains its arguments and activation;
    /// the helper must not move to another item until it receives a value.
    fn call_slice(
        &mut self,
        callable: &CalcValue,
        arguments: &[CalcValue],
    ) -> Result<Option<CalcValue>, CallAbandoned> {
        self.call(callable, arguments).map(Some)
    }
}

/// A [`CallableInvoker`]'s report that a call stopped with a fault it keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallAbandoned;

/// The services a call runs with, owned by the caller: the numeric policy, which every
/// arithmetic site reads instead of assuming one, the typed reader of reference arguments, and
/// the resolution of reference text.
pub struct EvalServices<'s> {
    numeric: &'s NumericMode,
    cells: Option<&'s mut dyn CellReader>,
    random: Option<&'s mut dyn RandomSource>,
    reference_text: Option<&'s mut dyn ReferenceText>,
    environment: Option<&'s CalcEnvironment>,
    host: Option<&'s mut dyn HostFacts>,
    callables: Option<&'s mut dyn CallableInvoker>,
}

impl<'s> EvalServices<'s> {
    /// Services without a cell reader: a reference argument that a kernel must read is refused
    /// with [`FunctionFault::ReferenceUnreadable`].
    pub fn new(numeric: &'s NumericMode) -> Self {
        EvalServices {
            numeric,
            cells: None,
            random: None,
            reference_text: None,
            environment: None,
            host: None,
            callables: None,
        }
    }

    /// Services whose kernels read reference arguments through `cells`.
    pub fn with_cells(numeric: &'s NumericMode, cells: &'s mut dyn CellReader) -> Self {
        EvalServices {
            numeric,
            cells: Some(cells),
            random: None,
            reference_text: None,
            environment: None,
            host: None,
            callables: None,
        }
    }

    /// These services, with reference text resolved by `resolution`. Without one, a call that
    /// must resolve reference text (INDIRECT's) is refused as not modelled.
    pub fn with_reference_text(self, resolution: &'s mut dyn ReferenceText) -> Self {
        EvalServices {
            reference_text: Some(resolution),
            ..self
        }
    }

    /// These services with `random` as the source of RAND, RANDBETWEEN and RANDARRAY.
    pub fn with_random(mut self, random: &'s mut dyn RandomSource) -> Self {
        self.random = Some(random);
        self
    }

    /// These services in `environment`: the date system, locale profile and clock that the
    /// date, time and number-format kernels read. Without one those kernels are refused as not
    /// modelled.
    pub fn with_environment(mut self, environment: &'s CalcEnvironment) -> Self {
        self.environment = Some(environment);
        self
    }

    /// These services with `callables` as the caller of LAMBDA values ([`CallableInvoker`]).
    pub fn with_callables(mut self, callables: &'s mut dyn CallableInvoker) -> Self {
        self.callables = Some(callables);
        self
    }

    pub(crate) fn callables(&mut self) -> Option<&mut (dyn CallableInvoker + 's)> {
        self.callables.as_deref_mut()
    }

    /// These services with `host` as the source of the host's facts ([`HostFacts`]).
    pub fn with_host(mut self, host: &'s mut dyn HostFacts) -> Self {
        self.host = Some(host);
        self
    }

    /// The explicit environment for this invocation.
    pub(crate) fn environment(&self) -> Option<CalcEnvironment> {
        self.environment.copied()
    }

    /// The explicit host questions for this invocation.
    pub(crate) fn host(&mut self) -> Option<&mut (dyn HostFacts + 's)> {
        self.host.as_deref_mut()
    }

    pub fn numeric_mode(&self) -> &'s NumericMode {
        self.numeric
    }

    pub(crate) fn cells(&mut self) -> Option<&mut (dyn CellReader + 's)> {
        self.cells.as_deref_mut()
    }

    pub(crate) fn random(&mut self) -> Option<&mut (dyn RandomSource + 's)> {
        self.random.as_deref_mut()
    }

    pub(crate) fn reference_text(&mut self) -> Option<&mut (dyn ReferenceText + 's)> {
        self.reference_text.as_deref_mut()
    }
}

impl fmt::Debug for EvalServices<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalServices")
            .field("numeric", self.numeric)
            .field("cells", &self.cells.is_some())
            .field("random", &self.random.is_some())
            .field("reference_text", &self.reference_text.is_some())
            .field("environment", &self.environment)
            .field("host", &self.host.is_some())
            .field("callables", &self.callables.is_some())
            .finish()
    }
}

/// Argument storage the caller owns and reuses across calls, so a call on warm scratch does not
/// allocate for its arguments.
#[derive(Debug, Default)]
pub struct FunctionCallScratch {
    arguments: Vec<CalcValue>,
    task: Option<Box<dyn KernelTask>>,
    task_target: Option<FunctionId>,
}

impl FunctionCallScratch {
    pub fn new() -> Self {
        Self::default()
    }
}

/// An infrastructure or admission failure of a call, distinct from a worksheet error value that
/// a kernel computes. A caller reports a call that ends in a fault as not calculated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionFault {
    /// Scratch still owns an activation of another catalogue entry. It is preserved for the
    /// original caller rather than silently replaced or executed twice.
    ContinuationTarget {
        expected: FunctionId,
        received: FunctionId,
    },
    /// The call supplies a number of arguments that the entry's arity does not admit.
    ArityMismatch { id: FunctionId, supplied: usize },
    /// A value parameter received a reference; the caller dereferences before the call.
    ReferenceArgument { id: FunctionId, index: usize },
    /// A reference-only parameter received something other than a reference.
    ReferenceRequired { id: FunctionId, index: usize },
    /// The kernel must read the reference at argument `index`, and the services have no
    /// [`CellReader`] or the reader refused an area of it.
    ReferenceUnreadable { id: FunctionId, index: usize },
    /// The kernel does not model Excel's result for this call; `argument` names the argument
    /// responsible when one is.
    NotImplemented {
        id: FunctionId,
        argument: Option<usize>,
    },
    /// The call reaches an arithmetic site under [`NumericMode::Exact`], for which no
    /// implementation exists.
    ExactUnavailable { id: FunctionId },
    /// The kernel returned [`CoreRef::Missing`], which marks an omitted argument and is never a
    /// function's result ([`CoreRef::Empty`] is).
    MissingReturned { id: FunctionId },
}

/// The kernel of an eager entry: its arguments, all evaluated and admitted, and the services.
pub(crate) type EagerKernel =
    fn(&[CalcValue], &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault>;

/// An eager kernel's owned state at a binary64 store boundary. It borrows no caller service.
pub(crate) trait KernelTask: std::fmt::Debug + Send {
    fn advance(
        &mut self,
        elements: usize,
        services: &mut EvalServices<'_>,
    ) -> Result<Option<CalcValue>, FunctionFault>;
    fn completed_elements(&self) -> usize;
}

pub(crate) type StartKernel =
    fn(&[CalcValue], &mut EvalServices<'_>) -> Result<Box<dyn KernelTask>, FunctionFault>;

#[derive(Debug)]
pub(crate) struct ReadyTask(pub Option<CalcValue>);

impl KernelTask for ReadyTask {
    fn advance(
        &mut self,
        _: usize,
        _: &mut EvalServices<'_>,
    ) -> Result<Option<CalcValue>, FunctionFault> {
        Ok(self.0.take())
    }
    fn completed_elements(&self) -> usize {
        0
    }
}

impl FunctionCallScratch {
    pub fn completed_elements(&self) -> usize {
        self.task
            .as_ref()
            .map_or(0, |task| task.completed_elements())
    }
}

/// The kernel of a lazy entry: the arguments it selects from, and the services.
pub(crate) type LazyKernel =
    fn(&mut LazyArguments<'_>, &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault>;

/// The arguments of a lazy entry's call. The caller evaluates each one only when the kernel asks
/// for it, and it is admitted against its parameter class before the kernel sees it.
pub(crate) struct LazyArguments<'a> {
    entry: &'a CatalogueEntry,
    source: &'a mut dyn PreparedArguments,
    count: usize,
}

impl LazyArguments<'_> {
    /// The number of arguments at the call site, omitted ones included.
    pub(crate) fn count(&self) -> usize {
        self.count
    }

    /// Argument `index`, below [`LazyArguments::count`], evaluated now.
    pub(crate) fn argument(&mut self, index: usize) -> Result<CalcValue, FunctionFault> {
        let value = self.source.argument(index);
        admit_argument(self.entry, index, &value)?;
        Ok(value)
    }
}

/// Runs the entry `target` resolved to. See the module documentation for the order of work.
pub fn invoke(
    target: FunctionCallTarget<'_>,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    scratch: &mut FunctionCallScratch,
) -> Result<CalcValue, FunctionFault> {
    if let Some(expected) = scratch.task_target.filter(|id| *id != target.entry.id) {
        return Err(FunctionFault::ContinuationTarget {
            expected,
            received: target.entry.id,
        });
    }
    if target.is_resumable() {
        return invoke_slice(target, arguments, services, scratch, usize::MAX).and_then(|value| {
            value.ok_or(FunctionFault::NotImplemented {
                id: target.entry.id,
                argument: None,
            })
        });
    }
    let entry = target.entry;
    let supplied = arguments.count();
    if !entry.arity.accepts(supplied) {
        return Err(FunctionFault::ArityMismatch {
            id: entry.id,
            supplied,
        });
    }
    match entry.kernel {
        Kernel::Lazy(kernel) => {
            let mut selectable = LazyArguments {
                entry,
                source: arguments,
                count: supplied,
            };
            admit_return(entry.id, kernel(&mut selectable, services)?)
        }
        Kernel::Eager(kernel) | Kernel::Resumable { run: kernel, .. } => {
            scratch.arguments.clear();
            let result = run_eager(entry, kernel, arguments, services, &mut scratch.arguments);
            scratch.arguments.clear();
            result
        }
    }
}

/// Advances an eager kernel, retaining its arguments and owned state between calls. Entries
/// without a continuation run to completion and are explicitly non-preemptible here.
pub fn invoke_slice(
    target: FunctionCallTarget<'_>,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    scratch: &mut FunctionCallScratch,
    elements: usize,
) -> Result<Option<CalcValue>, FunctionFault> {
    let Kernel::Resumable { start, .. } = target.entry.kernel else {
        return invoke(target, arguments, services, scratch).map(Some);
    };
    if let Some(expected) = scratch.task_target.filter(|id| *id != target.entry.id) {
        return Err(FunctionFault::ContinuationTarget {
            expected,
            received: target.entry.id,
        });
    }
    if scratch.task.is_none() {
        if !target.entry.arity.accepts(arguments.count()) {
            return Err(FunctionFault::ArityMismatch {
                id: target.entry.id,
                supplied: arguments.count(),
            });
        }
        scratch.arguments.clear();
        if let Some(value) = prepare_eager(target.entry, arguments, &mut scratch.arguments)? {
            return Ok(Some(value));
        }
        scratch.task = Some(start(&scratch.arguments, services)?);
        scratch.task_target = Some(target.entry.id);
        scratch.arguments.clear();
    }
    let result = scratch
        .task
        .as_mut()
        .expect("started above")
        .advance(elements, services);
    if !matches!(result, Ok(None)) {
        scratch.task = None;
        scratch.task_target = None;
    }
    result?
        .map(|value| admit_return(target.entry.id, value))
        .transpose()
}

fn run_eager(
    entry: &CatalogueEntry,
    kernel: EagerKernel,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    evaluated: &mut Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    if let Some(value) = prepare_eager(entry, arguments, evaluated)? {
        return Ok(value);
    }
    admit_return(entry.id, kernel(evaluated, services)?)
}

fn prepare_eager(
    entry: &CatalogueEntry,
    arguments: &mut dyn PreparedArguments,
    evaluated: &mut Vec<CalcValue>,
) -> Result<Option<CalcValue>, FunctionFault> {
    for index in 0..arguments.count() {
        let value = arguments.argument(index);
        // #REF! given to a parameter that takes only a reference is the call's result (dc-ifc,
        // Excel 16.0.20430 on 2026-10-27: OFFSET(#REF!,0,0), SUM(OFFSET(#REF!,0,0,1,1)),
        // AREAS(#REF!), ISFORMULA(#REF!), FORMULATEXT(#REF!), CELL("row",#REF!), SUMIF(#REF!,1)
        // and ROWS(#REF!) are #REF!; Excel refuses to enter any other value there).
        if entry.parameter_class(index) == ParameterClass::Reference
            && matches!(value.core_ref(), CoreRef::Error(WorksheetErrorCode::Ref))
        {
            return Ok(Some(CalcValue::error(WorksheetErrorCode::Ref)));
        }
        admit_argument(entry, index, &value)?;
        evaluated.push(value);
    }
    Ok(None)
}

fn admit_argument(
    entry: &CatalogueEntry,
    index: usize,
    value: &CalcValue,
) -> Result<(), FunctionFault> {
    let is_reference = matches!(value.core_ref(), CoreRef::Reference(_));
    match entry.parameter_class(index) {
        ParameterClass::Value if is_reference => Err(FunctionFault::ReferenceArgument {
            id: entry.id,
            index,
        }),
        ParameterClass::Reference if !is_reference => Err(FunctionFault::ReferenceRequired {
            id: entry.id,
            index,
        }),
        ParameterClass::Value | ParameterClass::ValueOrReference | ParameterClass::Reference => {
            Ok(())
        }
    }
}

fn admit_return(id: FunctionId, value: CalcValue) -> Result<CalcValue, FunctionFault> {
    if value.is_missing() {
        Err(FunctionFault::MissingReturned { id })
    } else {
        Ok(value)
    }
}

impl FunctionCallTarget<'_> {
    /// Runs this entry through [`invoke`].
    pub fn invoke(
        self,
        arguments: &mut dyn PreparedArguments,
        services: &mut EvalServices<'_>,
        scratch: &mut FunctionCallScratch,
    ) -> Result<CalcValue, FunctionFault> {
        invoke(self, arguments, services, scratch)
    }
}

/// Two boundary rules, on test entries that exist only here: no admitted kernel returns Missing,
/// and a reference-only parameter, like COUNTBLANK's, admits a reference and refuses every other
/// kind of value. Each test runs natively and, as a wasm-bindgen test, in Chrome and Firefox.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{ArgumentDemand, Notation, Parameter};
    use crate::value::{CallableArityShape, CellKey, IdAllocator, ReferenceLike, SheetId};

    struct Given(Vec<CalcValue>);

    impl PreparedArguments for Given {
        fn count(&self) -> usize {
            self.0.len()
        }

        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }

    fn returns_missing(
        _: &[CalcValue],
        _: &mut EvalServices<'_>,
    ) -> Result<CalcValue, FunctionFault> {
        Ok(CalcValue::missing())
    }

    fn returns_empty(
        _: &[CalcValue],
        _: &mut EvalServices<'_>,
    ) -> Result<CalcValue, FunctionFault> {
        Ok(CalcValue::empty())
    }

    const TEST_ID: FunctionId = FunctionId::new(0xFFFF, 1);

    const fn entry(class: ParameterClass, kernel: crate::call::EagerKernel) -> CatalogueEntry {
        CatalogueEntry {
            id: TEST_ID,
            spelling: "TEST.BOUNDARY",
            notation: Notation::Call,
            arity: CallableArityShape::exact(1),
            parameters: match class {
                ParameterClass::Reference => &[Parameter {
                    name: "reference",
                    class: ParameterClass::Reference,
                }],
                _ => &[Parameter {
                    name: "value",
                    class: ParameterClass::ValueOrReference,
                }],
            },
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(kernel),
        }
    }

    fn call(entry: &CatalogueEntry, argument: CalcValue) -> Result<CalcValue, FunctionFault> {
        let mode = NumericMode::Fast;
        invoke(
            FunctionCallTarget { entry },
            &mut Given(vec![argument]),
            &mut EvalServices::new(&mode),
            &mut FunctionCallScratch::new(),
        )
    }

    fn a_reference() -> CalcValue {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        CalcValue::reference(ReferenceLike::cell(CellKey::new(sheet, 1, 1).unwrap()))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_raw_return_may_be_empty_but_never_missing() {
        let missing = entry(ParameterClass::ValueOrReference, returns_missing);
        let empty = entry(ParameterClass::ValueOrReference, returns_empty);
        assert_eq!(
            call(&missing, CalcValue::number(1.0)),
            Err(FunctionFault::MissingReturned { id: TEST_ID })
        );
        assert_eq!(call(&empty, CalcValue::number(1.0)), Ok(CalcValue::empty()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_reference_only_parameter_admits_only_references() {
        let reference_only = entry(ParameterClass::Reference, returns_empty);
        // #REF! there is the call's result (see run_eager).
        assert_eq!(
            call(
                &reference_only,
                CalcValue::error(crate::value::WorksheetErrorCode::Ref)
            ),
            Ok(CalcValue::error(crate::value::WorksheetErrorCode::Ref))
        );
        for refused in [
            CalcValue::number(1.0),
            CalcValue::empty(),
            CalcValue::missing(),
            CalcValue::error(crate::value::WorksheetErrorCode::NA),
        ] {
            assert_eq!(
                call(&reference_only, refused),
                Err(FunctionFault::ReferenceRequired {
                    id: TEST_ID,
                    index: 0
                })
            );
        }
        assert_eq!(call(&reference_only, a_reference()), Ok(CalcValue::empty()));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_kernel_continuation_keeps_its_arguments_and_cannot_be_repurposed() {
        let catalogue = crate::catalogue();
        let sequence = catalogue
            .resolve_surface(Notation::Call, "SEQUENCE")
            .unwrap();
        let sum = catalogue.resolve_surface(Notation::Call, "SUM").unwrap();
        let mut scratch = FunctionCallScratch::new();
        let mode = NumericMode::Fast;
        let mut services = EvalServices::new(&mode);
        assert_eq!(
            invoke_slice(
                sequence,
                &mut Given(vec![CalcValue::number(5.0)]),
                &mut services,
                &mut scratch,
                2
            ),
            Ok(None)
        );
        assert_eq!(scratch.completed_elements(), 2);
        assert_eq!(
            invoke(
                sum,
                &mut Given(vec![CalcValue::number(99.0)]),
                &mut services,
                &mut scratch
            ),
            Err(FunctionFault::ContinuationTarget {
                expected: sequence.id(),
                received: sum.id()
            })
        );
        assert_eq!(scratch.completed_elements(), 2);
        // Resuming uses the admitted arguments, even if a caller's source now has another value.
        let value = invoke(
            sequence,
            &mut Given(vec![CalcValue::number(100.0)]),
            &mut services,
            &mut scratch,
        )
        .unwrap();
        let array = value.as_array().unwrap();
        assert_eq!(array.shape().rows, 5);
        assert_eq!(array.get(4, 0), Some(&CalcValue::number(5.0)));
    }
}
