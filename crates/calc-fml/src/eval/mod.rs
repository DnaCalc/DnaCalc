//! Compiled plans and their evaluation.
//!
//! [`compile`] lowers a parsed formula into a [`CompiledPlan`]: immutable instructions in which
//! every reference is the anchor-free [`ReferenceOperand`] that [`crate::bind_references`] gives
//! it, and every operator and function is a calc-func catalogue target. A plan is therefore the
//! formula's normal form, the same value for a formula and its copies, and the caller-owned
//! [`PlanCache`] hands out one shared `Arc<CompiledPlan>` per normal form: `=SUM(A1:A3)*2` entered
//! in C10 and `=SUM(A2:A4)*2` entered in C11 share one plan.
//!
//! [`instantiate`] places a plan's references at an anchor, giving a [`FormulaInstance`].
//! [`evaluate`] runs an instance in an [`EvalFrame`], the mutable state of that one evaluation
//! (its operand stack and call scratch, held in the caller's reusable [`EvalScratch`]). Nothing
//! of an evaluation is written into the plan, so any number of instances and frames share it.
//!
//! Every built-in operator and function runs through `calc_func::invoke`. User functions run
//! through `calc_func::UserFunctions`, which the host supplies independently of any VBA engine. Every value a formula reads
//! comes from the caller's [`ReferenceReader`], which is also the `calc_func::CellReader` the
//! kernels read reference arguments through (see the `cells` module). A function whose catalogue
//! entry is lazy (IF, IFERROR, IFNA) asks for its arguments itself, and only the arguments it asks
//! for run.
//!
//! Evaluation does not compile: [`evaluate`] receives no formula text, syntax tree or catalogue.
//!
//! A caller may attach an [`EvaluationObserver`] to the services ([`EvaluationServices::observed_by`]):
//! the evaluator itself shows it each catalogue call it runs ([`ExecutedCall`], marked when compile
//! inserted the call), each reference it resolved from text as it ran ([`ResolvedReference`]) and
//! the outcome it returns ([`EvaluationReport`]), so what the observer records comes from the
//! evaluation, not from a caller's account of it. None has a public constructor: a caller can pass
//! on what it is shown, but cannot make it.
//!
//! INDIRECT's text is bound to a reference as the call runs, in the formula's workbook context:
//! the sheets of the [`ReferenceCatalog`] the caller gives the services
//! ([`EvaluationServices::with_catalog`]) and the instance's anchor. Without a catalogue, calc-func
//! refuses the call as not modelled.

mod builtins;
mod cells;
mod closure;
mod compile;
mod legacy;
mod lift;
mod refops;
mod run;

/// The spelling a named call resolves by in the catalogue; the editor assistance resolves by it too.
pub(crate) use compile::file_unprefixed;
pub use compile::{
    CompileError, compile, compile_loaded, compile_loaded_with, compile_with,
    compile_with_functions,
};
pub use run::{EvalFault, MAX_CALL_DEPTH, evaluate};

use crate::reference::{
    ExternalOperand, FormulaAnchor, PlacementError, ReferenceCatalog, ReferenceOperand,
    ReferenceReader,
};
use calc_func::value::{
    ArrayShape, CalcArray, CalcValue, ExcelText, GridArea, NumericMode, ReferenceLike,
    WorksheetErrorCode,
};
use calc_func::{FunctionCallScratch, FunctionCallTarget, RandomSource};
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A compiled formula: immutable instructions over anchor-free reference operands. Two formulas
/// with the same normal form (the same structure, literals, functions and operands) compile to
/// equal plans, which a [`PlanCache`] shares as one allocation.
///
/// Block 0 is the formula. Each further block is one argument of a lazy call, which runs only
/// when the called function asks for that argument. A block's instructions run in order on an
/// operand stack, except those a guard skips, and leave exactly one value.
//
// The instructions are numbered by their position in their block, so a later map from an
// instruction to its source span can be kept beside the plan without changing it; none exists
// yet. An [`EvaluationObserver`] is shown each executed call, with whether compile inserted it.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CompiledPlan {
    blocks: Box<[Box<[Instruction]>]>,
    references: Box<[ReferenceOperand]>,
    /// Whether each reference is placed with its relative offsets wrapping around the grid's
    /// edge ([`ReferenceOperand::instantiate_wrapped`]): a defined name's references.
    wrapped: Box<[bool]>,
    /// The references to other workbooks, which read the values the workbook's links cache
    /// (dc-ylr).
    externals: Box<[ExternalOperand]>,
    form: FormulaForm,
}

/// How a formula's text is evaluated where a range or an array meets a place that takes one
/// value, and how its result is published.
///
/// Excel keeps two forms of a cell formula. A dynamic-array formula (entered in Excel 365, or set
/// through `Range.Formula2`) lifts: a function of single values given a range or an array runs on
/// each element, and a result of more than one value spills. A legacy formula (every formula a
/// file holds without dynamic-array cell metadata, or one set through `Range.Formula`) takes one
/// value by implicit intersection where Excel 365 shows an `@` in its `Formula2` text: a range at
/// a single-value parameter or an operator's operand gives the cell in the formula's row or
/// column, and the formula's result is one value. Inside an array parameter (SUMPRODUCT's,
/// LOOKUP's vectors, INDEX's array) a legacy formula lifts too. Measured on Excel 16.0 through COM
/// on 2026-10-23 (`Range.Formula` entry, then `Range.Formula2` and the value): `=SUM(A1:A3*2)` in
/// row 2 is `=SUM(@A1:A3*2)`, 4; `=SUMPRODUCT(A1:A3*2)` keeps no `@`, 12; `=SUM(ABS({-1,-2}))`
/// keeps no `@`, 3; `=ABS({-1,-2})` is `=ABS(@{-1,-2})`, 1; `=SUMPRODUCT(IF(A1:A3>1,1,0))` is
/// `=SUMPRODUCT(IF(@A1:A3>1,1,0))`, 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormulaForm {
    /// Implicit intersection where Excel 365 writes `@` for a legacy formula; the `legacy`
    /// module says where it applies and where this package refuses rather than guess.
    Legacy,
    /// Lifting and spilling, as a dynamic-array formula.
    Dynamic,
}

impl CompiledPlan {
    /// The plan's references in the order compile met them, those in arguments a lazy function
    /// may never ask for included.
    pub fn references(&self) -> &[ReferenceOperand] {
        &self.references
    }

    /// The form the plan was compiled in.
    pub fn form(&self) -> FormulaForm {
        self.form
    }

    /// Whether the plan reads another workbook's cells, whose values are those the workbook's
    /// links cache, not live inputs (dc-ylr).
    pub fn reads_external_links(&self) -> bool {
        !self.externals.is_empty()
    }

    /// Every catalogue entry the plan may call, in the order compile emitted the calls, those in
    /// arguments a lazy function may never ask for and in defined names' formulas included, with
    /// repeats.
    pub fn calls(&self) -> impl Iterator<Item = FunctionCallTarget<'static>> + '_ {
        self.blocks
            .iter()
            .flatten()
            .filter_map(|instruction| match instruction {
                Instruction::Call { target, .. } | Instruction::LazyCall { target, .. } => {
                    Some(target.0)
                }
                _ => None,
            })
    }

    /// Whether the plan calls a function Excel recalculates on every recalculation: RAND,
    /// RANDBETWEEN, RANDARRAY, NOW, TODAY, OFFSET, INDIRECT, CELL and INFO (Microsoft's
    /// "Excel Recalculation" page lists these as volatile). A call a lazy function may never ask
    /// for counts too, as it does in Excel.
    pub fn volatile(&self) -> bool {
        self.blocks
            .iter()
            .flatten()
            .any(|i| matches!(i, Instruction::UserCall { .. }))
            || self.calls().any(|target| {
                VOLATILE_FUNCTIONS
                    .iter()
                    .any(|name| strip_future_prefix(target.spelling()).eq_ignore_ascii_case(name))
            })
    }
}

/// The functions [`CompiledPlan::volatile`] counts as volatile.
const VOLATILE_FUNCTIONS: [&str; 9] = [
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "NOW",
    "TODAY",
    "OFFSET",
    "INDIRECT",
    "CELL",
    "INFO",
];

/// A catalogue spelling without the file form's `_XLFN.` prefix.
fn strip_future_prefix(spelling: &str) -> &str {
    let upper = spelling.get(..6).map(str::to_ascii_uppercase);
    match upper.as_deref() {
        Some("_XLFN.") => &spelling[6..],
        _ => spelling,
    }
}

/// How a call admits a range of more than one cell, or an array, given to a parameter that takes
/// one value (an operator's operand included). Compile decides it from the formula's form and
/// the argument's place in the formula (the `legacy` module).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Admission {
    /// The call runs on each element (a range as the array of its values), and its result is the
    /// array of the results: a dynamic-array formula, or an array parameter of a legacy one.
    Lift,
    /// A range gives the cell implicit intersection picks, and an array its first element: a
    /// legacy formula's single-value place on the way to its result.
    IntersectAll,
    /// A range gives the cell implicit intersection picks, and an array lifts: a legacy formula
    /// inside a parameter that takes a reference (SUM's).
    IntersectReferences,
    /// A range or an array there is refused: a legacy formula whose evaluation at this place is
    /// not modelled.
    Refuse,
}

/// One step of a plan.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Instruction {
    /// An engine-neutral eager user function, resolved against the current host registry.
    UserCall { name: String, argc: usize },
    /// Pushes the literal's value.
    Literal(Literal),
    /// Pushes an omitted argument ([`CalcValue::missing`]).
    Omitted,
    /// Pushes reference operand `.0` of the plan as the instance placed it: a reference to its
    /// area, or `#REF!` where it could not be placed.
    Reference(usize),
    /// Pushes the 3-D reference of the `count` reference operands from `first`, one per sheet of
    /// its span in sheet order, as the instance placed them ([`ReferenceLike::three_d`]), or
    /// `#REF!` where one could not be placed.
    Reference3D { first: usize, count: usize },
    /// Pops `argc` values, calls `target` with them and pushes the result. With `retain`, the
    /// arguments as the call received them (each reference to a value parameter replaced by its
    /// cell's value) stay on the stack beneath the result instead of being popped. With
    /// `synthetic`, compile inserted the call (a negation of the near-zero check) and the formula
    /// did not write it.
    ///
    /// `admission` says how a range or an array given to a single-value parameter is admitted.
    Call {
        target: Target,
        argc: usize,
        retain: bool,
        synthetic: bool,
        admission: Admission,
    },
    /// Calls the lazy entry `target`, whose argument `i` is block `first_block + i`; it pushes the
    /// result. When its first argument, which decides, is an array (or a range admitted as one),
    /// every argument runs and the call lifts over the arrays.
    LazyCall {
        target: Target,
        first_block: usize,
        argc: usize,
        admission: Admission,
    },
    /// Pops two references and pushes the reference from the top-left of the two to their
    /// bottom-right (the range operator `:` between references that are not one written range).
    Range,
    /// Pops two references and pushes their intersection (the intersection operator, a space).
    Intersect,
    /// Pops `.0` references and pushes the multi-area reference of all their areas (the union
    /// operator `,`).
    Union(usize),
    /// Pops a value and pushes the one value implicit intersection takes of it (`@`).
    Implicit,
    /// Pops a value and pushes, for a reference of one area, the cell implicit intersection takes
    /// of it (#VALUE! when there is none), and any other value as it is: the `@` a legacy formula
    /// writes before a reference given to TYPE or TRANSPOSE (`legacy::intersects_reference`).
    ImplicitReference,
    /// Pops a reference to one cell and pushes the reference to the range spilled from it (`#`).
    Spill,
    /// Pushes local `.0` of the current frame: a LET name or a LAMBDA parameter.
    Local(u32),
    /// Pops a value into local `.0` of the current frame, forgetting the locals after it.
    Store(u32),
    /// Pushes a LAMBDA value whose body is block `block`: it takes `params` arguments, the first
    /// `required` of them required, and captures the current frame's first `captured` locals.
    Lambda {
        block: usize,
        params: u32,
        required: u32,
        captured: u32,
    },
    /// Pops `argc` arguments and the value beneath them, which must be a LAMBDA value, and pushes
    /// the result of calling it.
    Invoke { argc: usize },
    /// Pushes the function value of the catalogue entry `.0`: a function named where a LAMBDA
    /// value is taken (`GROUPBY(A2:A9,B2:B9,SUM)`, which a file stores as `_xleta.SUM`), which a
    /// call of it calls with the arguments it is given (dc-h9u).
    Function(Target),
    /// Pushes the plan's external reference `.0` (dc-ylr): a reference to the cells of the sheet
    /// the workbook's link to the other book caches.
    External(usize),
    /// Runs block `.0`, a defined name's formula, in a frame of its own and pushes its value.
    Name(usize),
    /// Pushes a copy of the value `.0` places below the top of the stack (0 is the top).
    Copy(usize),
    /// Starts the check of the formula's final infix `+` or `-`, whose call retained its two
    /// operands beneath its result. A result that is not a non-zero number needs no check: the
    /// operands are popped, the result stays, and the next `skip` instructions (the rest of the
    /// check) do not run.
    NearZeroGuard { skip: usize },
    /// Ends the check of the formula's final infix `+` or `-`. It pops, from the top, the
    /// negations of the operation's right and left operands, its result and its two operands, and
    /// pushes the result, or refuses it as [`EvalFault::NearZeroFinalSum`]. Compile emits the two
    /// negations as calls of their own, between the guard and this check.
    NearZeroCheck,
}

/// A literal operand. A number is held as its bits, so that a plan is hashed and compared by the
/// exact binary64 it pushes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Literal {
    Number(u64),
    Text(ExcelText),
    Logical(bool),
    Error(WorksheetErrorCode),
    /// An array constant: `rows` by `cols` scalar literals in row-major order.
    Array {
        rows: usize,
        cols: usize,
        cells: Box<[Literal]>,
    },
}

impl Literal {
    fn value(&self) -> CalcValue {
        match self {
            Literal::Number(bits) => CalcValue::number(f64::from_bits(*bits)),
            Literal::Text(text) => CalcValue::text(text.clone()),
            Literal::Logical(b) => CalcValue::logical(*b),
            Literal::Error(code) => CalcValue::error(*code),
            Literal::Array { rows, cols, cells } => {
                let shape = ArrayShape {
                    rows: *rows,
                    cols: *cols,
                };
                let cells = cells.iter().map(Literal::value).collect();
                CalcValue::array(
                    CalcArray::new(shape, cells)
                        .expect("compile gives an array constant its shape"),
                )
            }
        }
    }
}

/// A resolved catalogue entry, compared and hashed by its stable function ID.
#[derive(Clone, Copy, Debug)]
struct Target(FunctionCallTarget<'static>);

impl PartialEq for Target {
    fn eq(&self, other: &Self) -> bool {
        self.0.id() == other.0.id()
    }
}

impl Eq for Target {}

impl Hash for Target {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.id().hash(state);
    }
}

/// The plans a caller has compiled, one shared allocation per normal form. [`compile`] returns
/// the plan already held when a formula's normal form equals it. The cache never evicts; plan
/// eviction belongs to bead cfml-m2.2.
#[derive(Debug, Default)]
pub struct PlanCache {
    plans: HashSet<Arc<CompiledPlan>>,
}

impl PlanCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The held plan equal to `plan`, or `plan` itself, now held.
    fn share(&mut self, plan: CompiledPlan) -> Arc<CompiledPlan> {
        if let Some(held) = self.plans.get(&plan) {
            return Arc::clone(held);
        }
        let plan = Arc::new(plan);
        self.plans.insert(Arc::clone(&plan));
        plan
    }
}

/// A plan placed at an anchor: each of its reference operands as the grid area it names there,
/// or the [`PlacementError`] (a worksheet `#REF!`) when it names none.
#[derive(Clone, Debug)]
pub struct FormulaInstance {
    plan: Arc<CompiledPlan>,
    anchor: FormulaAnchor,
    areas: Box<[Result<GridArea, PlacementError>]>,
}

impl FormulaInstance {
    pub fn plan(&self) -> &Arc<CompiledPlan> {
        &self.plan
    }

    pub fn anchor(&self) -> FormulaAnchor {
        self.anchor
    }

    /// The placed areas, one per entry of [`CompiledPlan::references`], in its order.
    pub fn areas(&self) -> &[Result<GridArea, PlacementError>] {
        &self.areas
    }

    /// The plan's external reference `index` (dc-ylr).
    pub(crate) fn external(&self, index: usize) -> &ExternalOperand {
        &self.plan.externals[index]
    }

    /// The value reference operand `index` pushes.
    fn reference(&self, index: usize) -> CalcValue {
        match self.areas[index] {
            Ok(area) => CalcValue::reference(ReferenceLike::area(area)),
            Err(placement) => CalcValue::error(placement.worksheet_error()),
        }
    }

    /// The 3-D reference the `count` operands from `first` push.
    fn reference_3d(&self, first: usize, count: usize) -> CalcValue {
        let mut areas = Vec::with_capacity(count);
        for placed in &self.areas[first..first + count] {
            match placed {
                Ok(area) => areas.push(*area),
                Err(placement) => return CalcValue::error(placement.worksheet_error()),
            }
        }
        match ReferenceLike::three_d(areas) {
            Ok(reference) => CalcValue::reference(reference),
            Err(_) => CalcValue::error(WorksheetErrorCode::Ref),
        }
    }
}

/// Places `plan` at `anchor`: the formula the plan was compiled from, copied to `anchor`.
pub fn instantiate(plan: &Arc<CompiledPlan>, anchor: FormulaAnchor) -> FormulaInstance {
    FormulaInstance {
        plan: Arc::clone(plan),
        anchor,
        areas: plan
            .references
            .iter()
            .zip(plan.wrapped.iter())
            .map(|(operand, wrapped)| {
                if *wrapped {
                    operand.instantiate_wrapped(anchor)
                } else {
                    operand.instantiate(anchor)
                }
            })
            .collect(),
    }
}

/// What an evaluation runs with, owned by the caller: the numeric policy, which every arithmetic
/// site reads, the reader that supplies every cell the formula reads, and, when the caller gives
/// them, the catalogue that INDIRECT's text is bound against and the observer the evaluator
/// reports the evaluation to.
pub struct EvaluationServices<'s> {
    numeric: &'s NumericMode,
    reader: &'s mut dyn ReferenceReader,
    observer: Option<&'s mut dyn EvaluationObserver>,
    catalog: Option<&'s dyn ReferenceCatalog>,
    random: Option<&'s mut dyn RandomSource>,
    functions: Option<&'s calc_func::UserFunctions>,
    function_diagnostics: Option<&'s mut Vec<String>>,
}

impl<'s> EvaluationServices<'s> {
    pub fn new(numeric: &'s NumericMode, reader: &'s mut dyn ReferenceReader) -> Self {
        EvaluationServices {
            numeric,
            reader,
            observer: None,
            catalog: None,
            random: None,
            functions: None,
            function_diagnostics: None,
        }
    }

    /// Current user functions and optional execution diagnostics. Runtime failures produce
    /// #VALUE!; a returned worksheet error retains its code.
    pub fn with_functions(mut self, functions: &'s calc_func::UserFunctions) -> Self {
        self.functions = Some(functions);
        self
    }

    pub fn with_function_diagnostics(mut self, diagnostics: &'s mut Vec<String>) -> Self {
        self.function_diagnostics = Some(diagnostics);
        self
    }

    /// These services, with `random` the source of RAND, RANDBETWEEN and RANDARRAY. Without one
    /// those functions are refused as not modelled.
    pub fn with_random(self, random: &'s mut dyn RandomSource) -> Self {
        EvaluationServices {
            random: Some(random),
            ..self
        }
    }

    /// These services, with reference text (INDIRECT's) bound against `catalog`'s sheets, the
    /// formula's workbook context. Without a catalogue such a call is refused as not modelled.
    pub fn with_catalog(self, catalog: &'s dyn ReferenceCatalog) -> Self {
        EvaluationServices {
            catalog: Some(catalog),
            ..self
        }
    }

    /// These services, with `observer` shown each call the evaluation runs and the outcome it
    /// returns (see [`EvaluationObserver`]).
    pub fn observed_by(self, observer: &'s mut dyn EvaluationObserver) -> Self {
        EvaluationServices {
            observer: Some(observer),
            ..self
        }
    }
}

impl fmt::Debug for EvaluationServices<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvaluationServices")
            .field("numeric", self.numeric)
            .field("observed", &self.observer.is_some())
            .field("catalog", &self.catalog.is_some())
            .field("random", &self.random.is_some())
            .finish_non_exhaustive()
    }
}

/// Observes one evaluation from inside the evaluator (plan sections 6.5 and 10.1). [`evaluate`]
/// shows it each catalogue call the evaluation runs, as the call returns, and then the outcome
/// the evaluation returns, before returning it. It is shown these borrowed and is given no reader,
/// stack or plan, so it cannot change what the evaluation reads or returns: what it is shown is
/// the evaluator's own account of the evaluation, made before any caller publishes a result.
pub trait EvaluationObserver {
    /// A catalogue call ran and returned, with a value or a fault. A call is shown only once its
    /// function ran: a call whose arguments the evaluation could not supply is not shown. The
    /// arguments of a lazy call run inside it, so their calls are shown before it.
    fn called(&mut self, call: ExecutedCall);

    /// The call `reference.target()` resolved reference text to an area as it ran (INDIRECT's),
    /// a reference the formula does not write: the evaluation's dynamic read footprint, beside the
    /// references its plan holds ([`CompiledPlan::references`]). Shown as that call returns, before
    /// the call itself is shown to [`EvaluationObserver::called`]; text that names no reference
    /// (#REF!) shows nothing. The default records nothing.
    fn resolved(&mut self, reference: ResolvedReference) {
        let _ = reference;
    }

    /// The evaluation's outcome, exactly as [`evaluate`] returns it, in a report only the
    /// evaluator makes.
    fn returned(&mut self, report: EvaluationReport<'_>);
}

/// What one evaluation returned, as [`EvaluationObserver::returned`] is shown it: the anchor of
/// the instance evaluated, the numeric mode it ran under and its outcome, exactly as [`evaluate`]
/// returns it (a reference to one cell already replaced by the cell's value).
///
/// Only the evaluator makes one: its fields are private and it has no constructor, so code
/// outside this package that is shown a report can read it and pass it on, but cannot make one or
/// change one. It is neither `Clone` nor `Copy`, so it is passed on at most once. Making one
/// outside this package does not compile, because its fields are private (E0451; stable rustdoc
/// does not check the error code, only that compiling fails):
///
/// ```compile_fail
/// use calc_fml::EvaluationReport;
/// use calc_fml::reference::FormulaAnchor;
/// use calc_func::value::{CalcValue, CellKey, IdAllocator, NumericMode, SheetId};
///
/// let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
/// let anchor = FormulaAnchor::new(CellKey::new(sheets.issue().unwrap(), 1, 1).unwrap());
/// let value = CalcValue::number(12.0);
/// let _report = EvaluationReport {
///     anchor,
///     numeric: &NumericMode::Fast,
///     outcome: Ok(&value),
/// };
/// ```
#[derive(Debug)]
pub struct EvaluationReport<'e> {
    anchor: FormulaAnchor,
    numeric: &'e NumericMode,
    outcome: Result<&'e CalcValue, &'e EvalFault>,
}

impl<'e> EvaluationReport<'e> {
    /// The anchor of the instance evaluated: the cell the formula belongs to.
    pub fn anchor(&self) -> FormulaAnchor {
        self.anchor
    }

    /// The numeric mode the evaluation ran under.
    pub fn numeric_mode(&self) -> &'e NumericMode {
        self.numeric
    }

    /// The value the evaluation returned, or the fault that stopped it.
    pub fn outcome(&self) -> Result<&'e CalcValue, &'e EvalFault> {
        self.outcome
    }
}

/// One catalogue call an evaluation ran, as [`EvaluationObserver::called`] is shown it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutedCall {
    target: FunctionCallTarget<'static>,
    synthetic: bool,
}

impl ExecutedCall {
    /// The catalogue entry called: an operator or a function.
    pub fn target(self) -> FunctionCallTarget<'static> {
        self.target
    }

    /// Whether compile inserted the call rather than the formula writing it. The only such calls
    /// are the two negations of the check of a final `+` or `-` ([`EvalFault::NearZeroFinalSum`]):
    /// calls of the catalogue's prefix `-`, as a negation the formula writes (`=-A1`) is too, so
    /// only this flag tells them apart.
    pub fn synthetic(self) -> bool {
        self.synthetic
    }
}

/// An area a call resolved from reference text as an evaluation ran, as
/// [`EvaluationObserver::resolved`] is shown it: the area and the catalogue call (INDIRECT) that
/// resolved it. Only the evaluator makes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedReference {
    target: FunctionCallTarget<'static>,
    area: GridArea,
}

impl ResolvedReference {
    /// The catalogue entry whose call resolved the text.
    pub fn target(self) -> FunctionCallTarget<'static> {
        self.target
    }

    /// The area the text named, on its sheet.
    pub fn area(self) -> GridArea {
        self.area
    }
}

/// Storage the caller owns and reuses across evaluations: the operand stack and the call scratch
/// of each level of lazy calls. It holds nothing from one evaluation to the next.
#[derive(Debug, Default)]
pub struct EvalScratch {
    stack: Vec<CalcValue>,
    calls: Vec<FunctionCallScratch>,
    /// The locals of every frame of the evaluation (LET names, LAMBDA parameters and captures),
    /// each frame above its caller's.
    locals: Vec<CalcValue>,
    /// The argument storage eager calls reuse (dc-dxm: kept between evaluations).
    arguments: Vec<CalcValue>,
}

impl EvalScratch {
    pub fn new() -> Self {
        Self::default()
    }
}

/// One evaluation of an instance: the mutable state [`evaluate`] works in, kept apart from the
/// plan's immutable instructions. A frame is made for one call and consumed by it.
#[derive(Debug)]
pub struct EvalFrame<'a> {
    instance: &'a FormulaInstance,
    scratch: &'a mut EvalScratch,
}

impl<'a> EvalFrame<'a> {
    /// A frame for one evaluation of `instance`, working in `scratch`'s storage.
    pub fn new(instance: &'a FormulaInstance, scratch: &'a mut EvalScratch) -> Self {
        scratch.stack.clear();
        scratch.locals.clear();
        EvalFrame { instance, scratch }
    }
}
