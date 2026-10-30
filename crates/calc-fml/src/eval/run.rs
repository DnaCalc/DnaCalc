//! Running a [`FormulaInstance`] in an [`EvalFrame`].
//!
//! A block's instructions run in order on the frame's operand stack. An eager call takes its
//! arguments from the stack, each already evaluated; a lazy call hands calc-func its arguments
//! as blocks that run only when the kernel asks for them. Before a call, each argument is
//! admitted to its parameter: a reference to one cell passed to a parameter that takes a value is
//! replaced by its cell's value, and a range of more than one cell or an array there is lifted
//! over, intersected or refused as the call's [`Admission`] says; a reference passed to a
//! parameter that admits one reaches the kernel as a reference, which the kernel reads through
//! the shared reader. A call lifted over arrays runs the kernel once per element of its result
//! (the `lift` module) and gives the array of the results.
//!
//! LET names, LAMBDA parameters and the locals a LAMBDA captured live in frames of their own
//! above the operand stack ([`EvalScratch`]'s locals): a LAMBDA call and a defined name's formula
//! each run in a new frame. A LAMBDA value is a [`Closure`] over the plan's block.
//!
//! Operator chains and eager calls, however long or deep, run in one block's loop over the
//! operand stack. Evaluation recurses on the native stack in three places: a lazy call's
//! arguments run from inside calc-func's kernel (`Run::block`, `Run::lazy_call`,
//! `calc_func::invoke`, the kernel, `Blocks::argument`, then `Run::block` for the argument's
//! block), once for each lazy call nested inside another's argument, which the parser limits to
//! 64 levels (Excel's documented limit of nested levels of functions); a LAMBDA call runs its body
//! from `Run::invoke`; and a defined name's formula runs from `Run::name`. LAMBDA calls and names
//! nest at most [`MAX_CALL_DEPTH`] deep; a deeper evaluation is refused. The explicit evaluation
//! stack that removes the recursion is bead dc-m3u's.

use super::cells::{ReaderCells, SharedReader};
use super::closure::Closure;
use super::{
    Admission, EvalFrame, EvaluationObserver, EvaluationReport, EvaluationServices, ExecutedCall,
    FormulaForm, FormulaInstance, Instruction, ResolvedReference, Target, lift, refops,
};
use crate::reference::{
    FormulaAnchor, ReadContractError, ReferenceCatalog, ReferenceNotation,
    ReferenceOperand, SheetTarget, SpillExtent, bind_text,
};
use calc_func::value::{
    CalcArray, CalcValue, CellKey, CoreRef, GridArea, NumericMode, ReferenceLike,
    WorksheetErrorCode,
};
use calc_func::{
    CallAbandoned, CallableInvoker, EvalServices, FunctionCallScratch, FunctionFault,
    ParameterClass, PreparedArguments, RandomSource, ReferenceText, TextReference, invoke,
};
use std::cell::RefCell;

/// The caller's random source, shared by every kernel call of one evaluation. A lazy call's
/// arguments run while its kernel holds the services, so each call reaches the source through
/// this shared cell, borrowed only for one draw.
struct SharedRandom<'r> {
    source: RefCell<&'r mut dyn RandomSource>,
}

/// One call's view of the shared random source.
struct RandomDraws<'a, 'r>(&'a SharedRandom<'r>);

impl RandomSource for RandomDraws<'_, '_> {
    fn next_unit(&mut self) -> f64 {
        self.0.source.borrow_mut().next_unit()
    }
}

/// Services for one kernel call: the shared reader as its cells and, when the caller gave one,
/// the shared random source.
fn call_services<'s>(
    numeric: &'s NumericMode,
    cells: &'s mut ReaderCells<'_, '_>,
    random: Option<&'s mut RandomDraws<'_, '_>>,
) -> EvalServices<'s> {
    let services = EvalServices::with_cells(numeric, cells);
    match random {
        Some(random) => services.with_random(random),
        None => services,
    }
}

/// Why an evaluation produced no value. Each is a refusal to calculate, never a worksheet value:
/// a caller reports the formula as not calculated.
#[derive(Clone, Debug, PartialEq)]
pub enum EvalFault {
    /// A call ended in calc-func's fault (an argument it does not admit, a result it does not
    /// model, an arithmetic site under Exact).
    Function(FunctionFault),
    /// The formula reads `cell`, which the reader reported as missing: an input it cannot supply.
    MissingInput { cell: CellKey },
    /// The reader's runs did not cover an area exactly.
    ReadContract(ReadContractError),
    /// A reference to more than one cell where a legacy formula takes one value at a place whose
    /// evaluation in Excel this package does not model (see [`super::FormulaForm::Legacy`]); work
    /// package dc-m3u owns it (L1, dc-mzk, left it open).
    AreaAsValue { area: GridArea },
    /// The formula's final operation is an infix `+` or `-` (seen through parentheses, a prefix
    /// `+` or `-` and the arguments IF may return) whose result is a non-zero number of at most
    /// 1e-13 times the magnitude of its larger operand; `result` is the binary64 result.
    ///
    /// The result of the formula's final addition or subtraction, which Excel may adjust to 0
    /// ([`NEAR_ZERO_EXPONENTS`]), when its left operand does not read as a number, so the rule
    /// cannot be applied; refused rather than published.
    NearZeroFinalSum { result: f64 },
    /// The evaluation reached a construct whose result is not modelled: `construct` says what it
    /// is and `owner` names the work package that owns it.
    NotModelled {
        construct: &'static str,
        owner: &'static str,
    },
}

/// Excel's adjustment of a formula's final `+` or `-` (Microsoft KB 78113), measured in Excel
/// 16.0.20430 through COM on 2026-10-25 (dc-b1l, `Range.Formula2`): a non-zero result is 0 when
/// the biased binary64 exponent of the operation's left operand exceeds the result's by this
/// many or more. `(1+k*2^-52)-1` is 0 for k up to 7 and k*2^-52 from 8, the same for 1.5, 1.99 and
/// 1024 in place of 1; `(2+k*2^-51)-(2-2^-52)` is 0 for k up to 7 and `(2-2^-52)-(2+k*2^-51)` for
/// k up to 3, so the left operand decides, not the larger; `(-1-k*2^-52)+1` is as the first;
/// `=1E15+1-1E15` is 1 (exponents 49 apart); a result below the normal range, whose biased
/// exponent is 0, is kept when the left operand is below 2^-973
/// (`=POWER(2,-1000)*(1+k*2^-52)-POWER(2,-1000)` is k*2^-1052). Only the formula's root
/// operation is adjusted (see compile's `carries_final_sum`).
const NEAR_ZERO_EXPONENTS: i64 = 50;

/// The biased exponent field of `x`.
fn biased_exponent(x: f64) -> i64 {
    ((x.to_bits() >> 52) & 0x7ff) as i64
}

/// The argument a lazy kernel receives after one of its arguments faulted. The call's result is
/// discarded and the fault reported, so this value is never published. An error makes IF stop
/// without asking for another argument; IFERROR asks for its fallback, which receives this value
/// too without its block running.
const ABANDONED: WorksheetErrorCode = WorksheetErrorCode::Calc;

/// How deep LAMBDA calls and defined names' formulas may nest in one evaluation. Each level
/// recurses on the native stack (see the module documentation), and a deeper evaluation is
/// refused as not modelled, not computed: Excel's own limit is deeper (OxFml measured
/// `=LET(F,LAMBDA(self,n,IF(n<=0,0,1+self(self,n-1))),F(F,4094))` as 4094 and `F(F,4095)` as
/// #NUM!, which [`RECURSION_BUDGET`] models).
pub const MAX_CALL_DEPTH: usize = 64;

/// Excel's budget for nested LAMBDA calls, as OxFml measured it (OxFml `eval/mod.rs`,
/// `LOCAL_CALLABLE_RECURSION_BUDGET_UNITS`): each call in progress costs [`CALL_COST`] units plus
/// [`CALLABLE_ARGUMENT_COST`] for each argument that is a LAMBDA value, and a call that would take
/// the calls in progress past the budget is #NUM!. A named LAMBDA calling itself reaches 5,460
/// levels and a LET-bound one passed itself 4,094.
const RECURSION_BUDGET: usize = 16_383;
const CALL_COST: usize = 3;
const CALLABLE_ARGUMENT_COST: usize = 1;

/// Runs `frame` to its value: the formula's result, or the fault that stopped it.
///
/// A legacy formula's result is one value: a reference to one cell is the cell's value, a larger
/// range gives the cell implicit intersection picks (#VALUE! when there is none), and an array its
/// first element. A dynamic-array formula's result is a reference to one cell as its value, a
/// range of one area as the array of its values, and an array as itself (a spill, which the
/// caller publishes or refuses). A result that is the empty value (a reference to a blank cell, or
/// a blank value a function returned, as IFERROR returns a blank `value`) is the number 0, the
/// value Excel shows for `=A1` when A1 is blank (not measured by this package). The rule applies
/// to every formula, not only to IFERROR and IFNA. Microsoft's IFERROR and IFNA pages say an empty
/// cell is treated as an empty string, yet Excel 16.0.20430.20182 shows 0 for each of bead
/// dc-ibp2's oracle rows whose result is a blank or omitted value or fallback, as this rule gives
/// (its capture of `tests/fixtures/dc_ibp2/oracle.xlsx`).
///
/// When the services carry an [`EvaluationObserver`], it is shown each call the evaluation runs
/// and then this outcome, before it is returned.
pub fn evaluate(
    frame: EvalFrame<'_>,
    services: &mut EvaluationServices<'_>,
) -> Result<CalcValue, EvalFault> {
    let EvalFrame { instance, scratch } = frame;
    let reader = SharedReader::new(&mut *services.reader);
    let random = match services.random.as_mut() {
        Some(source) => Some(SharedRandom {
            source: RefCell::new(&mut **source),
        }),
        None => None,
    };
    let mut run = Run {
        random: random.as_ref(),
        instance,
        stack: &mut scratch.stack,
        calls: &mut scratch.calls,
        locals: &mut scratch.locals,
        base: 0,
        depth: 0,
        budget: 0,
        arguments: &mut scratch.arguments,
        numeric: services.numeric,
        reader: &reader,
        catalog: services.catalog,
        // Reborrowed for this evaluation only: the coercion shortens the observer's lifetime to
        // the run's, which `Option::as_deref_mut` cannot.
        observer: match services.observer.as_mut() {
            Some(observer) => Some(&mut **observer),
            None => None,
        },
    };
    let at = instance.anchor.cell();
    let outcome = run.block(0).and_then(|value| {
        let value = match (instance.plan.form, value.core_ref()) {
            (FormulaForm::Legacy, CoreRef::Reference(reference)) => {
                match refops::intersect_cell(reference, at) {
                    Some(cell) => reader.value_of(&ReferenceLike::cell(cell))?,
                    None => CalcValue::error(WorksheetErrorCode::Value),
                }
            }
            (FormulaForm::Legacy, CoreRef::Array(_)) => refops::implicit(&value, at),
            (FormulaForm::Dynamic, CoreRef::Reference(reference)) => {
                reader.values_of(reference)?
            }
            _ => value,
        };
        Ok(if value.is_empty() {
            CalcValue::number(0.0)
        } else {
            value
        })
    });
    if let Some(observer) = run.observer {
        observer.returned(EvaluationReport {
            anchor: instance.anchor,
            numeric: services.numeric,
            outcome: outcome.as_ref(),
        });
    }
    outcome
}

struct Run<'a, 'r> {
    instance: &'a FormulaInstance,
    stack: &'a mut Vec<CalcValue>,
    /// Call scratch not in use; a lazy call's arguments take their own while it holds one.
    calls: &'a mut Vec<FunctionCallScratch>,
    /// The locals of every frame, the current frame's from `base` on.
    locals: &'a mut Vec<CalcValue>,
    base: usize,
    /// LAMBDA calls and defined names in progress.
    depth: usize,
    /// The recursion budget the LAMBDA calls in progress use.
    budget: usize,
    /// Argument storage reused by eager calls.
    arguments: &'a mut Vec<CalcValue>,
    numeric: &'a NumericMode,
    reader: &'a SharedReader<'r>,
    /// The source of RAND, RANDBETWEEN and RANDARRAY, when the caller gave one.
    random: Option<&'a SharedRandom<'r>>,
    /// The workbook context INDIRECT's text is bound in, when the caller gave one.
    catalog: Option<&'a dyn ReferenceCatalog>,
    observer: Option<&'a mut dyn EvaluationObserver>,
}

fn not_modelled(construct: &'static str) -> EvalFault {
    not_modelled_by(construct, "dc-10y")
}

fn not_modelled_by(construct: &'static str, owner: &'static str) -> EvalFault {
    EvalFault::NotModelled { construct, owner }
}

impl Run<'_, '_> {
    /// Pushes the plan's external reference `index` (dc-ylr): a reference to its cells on the
    /// sheet the catalogue issued for the other book's sheet, which the reader of the workbook's
    /// cells reads from the link's cache, so a function given it takes it as Excel takes a
    /// reference to a closed workbook (Excel 16.0.20527, 2026-10-30: ISREF TRUE, AREAS, ROW,
    /// INDEX and unions as for any reference). #REF! when the reference leaves the grid; a book or
    /// sheet no link of the workbook caches, or no catalogue, is refused.
    fn external(&mut self, index: usize) -> Result<(), EvalFault> {
        let refused = || EvalFault::NotModelled {
            construct: "a reference to another workbook whose sheet no link of this workbook caches",
            owner: "dc-ylr",
        };
        let instance = self.instance;
        let operand = instance.external(index);
        let catalog = self.catalog.ok_or_else(refused)?;
        let sheet = catalog
            .external_sheet(&operand.book, &operand.sheet)
            .ok_or_else(refused)?;
        let placed = ReferenceOperand {
            sheet: SheetTarget::Sheet(sheet),
            shape: operand.shape,
        };
        let area = if operand.wrapped {
            placed.instantiate_wrapped(instance.anchor)
        } else {
            placed.instantiate(instance.anchor)
        };
        self.stack.push(match area {
            Ok(area) => CalcValue::reference(ReferenceLike::area(area)),
            Err(error) => CalcValue::error(error.worksheet_error()),
        });
        Ok(())
    }

    /// Runs block `index` and gives the one value it leaves.
    fn block(&mut self, index: usize) -> Result<CalcValue, EvalFault> {
        let base = self.stack.len();
        let instance = self.instance;
        let block = &instance.plan.blocks[index];
        let mut at = 0;
        while let Some(instruction) = block.get(at) {
            at += 1;
            let step = match instruction {
                Instruction::Literal(literal) => {
                    self.stack.push(literal.value());
                    Ok(())
                }
                Instruction::Omitted => {
                    self.stack.push(CalcValue::missing());
                    Ok(())
                }
                Instruction::Reference(reference) => {
                    self.stack.push(instance.reference(*reference));
                    Ok(())
                }
                Instruction::Reference3D { first, count } => {
                    self.stack.push(instance.reference_3d(*first, *count));
                    Ok(())
                }
                Instruction::Call {
                    target,
                    argc,
                    retain,
                    synthetic,
                    admission,
                } => self.call(*target, *argc, *retain, *synthetic, *admission),
                Instruction::LazyCall {
                    target,
                    first_block,
                    argc,
                    admission,
                } => self.lazy_call(*target, *first_block, *argc, *admission),
                Instruction::Copy(depth) => {
                    let value = self.stack[self.stack.len() - 1 - depth].clone();
                    self.stack.push(value);
                    Ok(())
                }
                Instruction::NearZeroGuard { skip } => {
                    // Stack: left, right, result.
                    let top = self.stack.len() - 1;
                    if !matches!(self.stack[top].as_number(), Some(n) if n != 0.0) {
                        let result = self.stack.pop().expect("the operation's result");
                        self.stack.truncate(top - 2);
                        self.stack.push(result);
                        at += skip;
                    }
                    Ok(())
                }
                Instruction::NearZeroCheck => self.near_zero_check(),
                Instruction::Range => {
                    let right = self.stack.pop().expect("an operand");
                    let left = self.stack.pop().expect("an operand");
                    self.stack.push(refops::range(&left, &right));
                    Ok(())
                }
                Instruction::Intersect => {
                    let right = self.stack.pop().expect("an operand");
                    let left = self.stack.pop().expect("an operand");
                    self.stack.push(refops::intersect(&left, &right));
                    Ok(())
                }
                Instruction::Union(count) => {
                    let start = self.stack.len() - count;
                    let value = refops::union(&self.stack[start..]);
                    self.stack.truncate(start);
                    self.stack.push(value);
                    Ok(())
                }
                Instruction::Implicit => {
                    let value = self.stack.pop().expect("an operand");
                    self.stack
                        .push(refops::implicit(&value, instance.anchor.cell()));
                    Ok(())
                }
                Instruction::ImplicitReference => {
                    let value = self.stack.pop().expect("an operand");
                    let value = match value.core_ref() {
                        CoreRef::Reference(reference) if reference.areas().len() == 1 => {
                            refops::implicit(&value, instance.anchor.cell())
                        }
                        _ => value,
                    };
                    self.stack.push(value);
                    Ok(())
                }
                Instruction::Spill => self.spill(),
                Instruction::Local(slot) => {
                    let value = self.locals[self.base + *slot as usize].clone();
                    self.stack.push(value);
                    Ok(())
                }
                Instruction::Store(slot) => {
                    let value = self.stack.pop().expect("a value to store");
                    self.locals.truncate(self.base + *slot as usize);
                    self.locals.push(value);
                    Ok(())
                }
                Instruction::Lambda {
                    block,
                    params,
                    required,
                    captured,
                } => {
                    let from = self.base;
                    let closure = Closure {
                        plan: plan_address(instance),
                        block: *block,
                        params: *params as usize,
                        required: *required as usize,
                        captured: self.locals[from..from + *captured as usize].into(),
                    };
                    self.stack.push(closure.into_value());
                    Ok(())
                }
                Instruction::Invoke { argc } => self.invoke(*argc),
                Instruction::Function(target) => {
                    self.stack
                        .push(super::closure::FunctionValue { target: *target }.into_value());
                    Ok(())
                }
                Instruction::Name(block) => self.name(*block),
                Instruction::External(index) => self.external(*index),
            };
            if let Err(fault) = step {
                self.stack.truncate(base);
                return Err(fault);
            }
        }
        // Compile lowers every expression to instructions that leave exactly one value.
        let value = self.stack.pop().expect("a block leaves one value");
        debug_assert_eq!(self.stack.len(), base);
        Ok(value)
    }

    /// The argument `value` as parameter `index` of `target` receives it. A parameter that takes
    /// a value receives a reference to one cell as the cell's value, and a larger range or an
    /// array as `admission` says: lifted (a range as the array of its values; a range of more
    /// than one area as #VALUE!), intersected, or refused. Any other parameter receives the
    /// argument as it is.
    fn admit(
        &self,
        target: Target,
        index: usize,
        value: CalcValue,
        admission: Admission,
    ) -> Result<CalcValue, EvalFault> {
        if target.0.parameter_class(index) != Some(ParameterClass::Value) {
            return Ok(value);
        }
        self.admit_value(value, admission)
    }

    /// `value` admitted where one value is taken; see [`Run::admit`].
    fn admit_value(&self, value: CalcValue, admission: Admission) -> Result<CalcValue, EvalFault> {
        // A boxed array (dc-h9u) gives its first element where one value is taken.
        if let CoreRef::Array(array) = value.core_ref()
            && array.is_boxed()
            && value.callable_value().is_none()
        {
            return Ok(first_element(array));
        }
        match value.core_ref() {
            CoreRef::Reference(reference) => {
                if let [area] = reference.areas()
                    && area.cell_count() == 1
                {
                    return self.reader.value_of(reference);
                }
                match admission {
                    Admission::Lift => self.reader.values_of(reference),
                    Admission::IntersectAll | Admission::IntersectReferences => {
                        match refops::intersect_cell(reference, self.instance.anchor.cell()) {
                            Some(cell) => self.reader.value_of(&ReferenceLike::cell(cell)),
                            None => Ok(CalcValue::error(WorksheetErrorCode::Value)),
                        }
                    }
                    Admission::Refuse => Err(EvalFault::AreaAsValue {
                        area: reference.areas()[0],
                    }),
                }
            }
            CoreRef::Array(_) if value.callable_value().is_none() => match admission {
                Admission::Lift | Admission::IntersectReferences => Ok(value),
                Admission::IntersectAll => {
                    Ok(refops::implicit(&value, self.instance.anchor.cell()))
                }
                Admission::Refuse => Err(not_modelled(
                    "an array where a legacy formula takes one value",
                )),
            },
            _ => Ok(value),
        }
    }

    fn call(
        &mut self,
        target: Target,
        argc: usize,
        retain: bool,
        synthetic: bool,
        admission: Admission,
    ) -> Result<(), EvalFault> {
        let start = self.stack.len() - argc;
        let mut lifted = false;
        let form = self.instance.plan.form;
        for index in 0..argc {
            let mut value = std::mem::replace(&mut self.stack[start + index], CalcValue::missing());
            // dc-b1l: a legacy formula gives a function that does not run once per reference of
            // an array of references its first reference.
            if form == FormulaForm::Legacy
                && reference_array(&value)
                && !lifts_over_references(target, form)
            {
                value = value
                    .as_array()
                    .and_then(|array| array.get(0, 0))
                    .cloned()
                    .expect("a non-empty array");
            }
            let admitted = self.admit(target, index, value, admission)?;
            lifted |= is_lifted(target, self.instance.plan.form, index, &admitted);
            self.stack[start + index] = admitted;
        }
        for index in 0..argc {
            let value = &self.stack[start + index];
            if reference_array(value)
                && target.0.parameter_class(index) != Some(ParameterClass::Value)
                && !lifts_over_references(target, form)
                && !TAKES_REFERENCE_ARRAYS.contains(&target.0.spelling())
            {
                return Err(not_modelled_by(
                    "an array of references given to a function whose handling of one is not measured",
                    "dc-b1l",
                ));
            }
        }
        let mut arguments = std::mem::take(&mut *self.arguments);
        arguments.clear();
        // A call that does not keep its arguments on the stack moves them out instead of copying
        // them (dc-dxm); a failed call's arguments are gone either way, as `block` truncates the
        // stack on a fault.
        if retain {
            arguments.extend_from_slice(&self.stack[start..]);
        } else {
            arguments.extend(self.stack.drain(start..));
        }
        let value = if lifted {
            self.lifted(target, &arguments, synthetic)
        } else {
            self.eager(target, &arguments, synthetic)
        };
        arguments.clear();
        *self.arguments = arguments;
        let value = value?;
        if !retain {
            self.stack.truncate(start);
        }
        self.stack.push(value);
        Ok(())
    }

    /// Runs the eager entry `target` on `arguments` once.
    fn eager(
        &mut self,
        target: Target,
        arguments: &[CalcValue],
        synthetic: bool,
    ) -> Result<CalcValue, EvalFault> {
        // A LAMBDA or function value, or an array of them (`HSTACK(SUM,MAX)`, dc-h9u).
        let callable = |value: &CalcValue| {
            value.callable_value().is_some()
                || matches!(value.core_ref(), CoreRef::Array(array)
                    if array.iter_row_major().any(|element| element.callable_value().is_some()))
        };
        if arguments.iter().any(callable) {
            return self.eager_with_callables(target, arguments, synthetic);
        }
        let mut scratch = self.calls.pop().unwrap_or_default();
        self.reader.clear_refusal();
        let mut text = self.catalog.map(|catalog| TextBinding {
            catalog,
            anchor: self.instance.anchor,
            resolved: Vec::new(),
        });
        let result = {
            let mut cells = ReaderCells::new(self.reader);
            let mut draws = self.random.map(RandomDraws);
            let mut services = call_services(self.numeric, &mut cells, draws.as_mut());
            if let Some(text) = text.as_mut() {
                services = services.with_reference_text(text);
            }
            invoke(
                target.0,
                &mut Evaluated(arguments),
                &mut services,
                &mut scratch,
            )
        };
        self.calls.push(scratch);
        for area in text.map(|text| text.resolved).unwrap_or_default() {
            self.show_resolved(target, area);
        }
        self.show_call(target, synthetic);
        result.map_err(|fault| self.function_fault(fault))
    }

    /// Runs the eager entry `target` once on `arguments`, of which at least one is a LAMBDA value,
    /// with the services' callables calling LAMBDA values through this evaluation (MAP, REDUCE
    /// and the other helpers call theirs). A LAMBDA call that stops with a fault stops the
    /// entry's call, and the fault is the evaluation's. No reference text is bound for such a
    /// call: no entry that takes a LAMBDA resolves reference text.
    fn eager_with_callables(
        &mut self,
        target: Target,
        arguments: &[CalcValue],
        synthetic: bool,
    ) -> Result<CalcValue, EvalFault> {
        let mut scratch = self.calls.pop().unwrap_or_default();
        let reader = self.reader;
        let numeric = self.numeric;
        let random = self.random;
        reader.clear_refusal();
        let mut callables = Callables {
            run: &mut *self,
            fault: None,
        };
        let result = {
            let mut cells = ReaderCells::new(reader);
            let mut draws = random.map(RandomDraws);
            let mut services =
                call_services(numeric, &mut cells, draws.as_mut()).with_callables(&mut callables);
            invoke(
                target.0,
                &mut Evaluated(arguments),
                &mut services,
                &mut scratch,
            )
        };
        let fault = callables.fault.take();
        self.calls.push(scratch);
        self.show_call(target, synthetic);
        if let Some(fault) = fault {
            return Err(fault);
        }
        result.map_err(|fault| self.function_fault(fault))
    }

    /// Runs `target` on each element of the arrays among `arguments` at its single-value
    /// parameters, the other arguments as they are, and gives the array of the results. An
    /// element no array is large enough for is #N/A. A result that is a reference or an array is
    /// refused: the arrays of references Excel lifts into (`=OFFSET(A1,{0;1},0)` is #VALUE! in a
    /// cell yet `=SUM(OFFSET(A1,{0;1;2},0))` is 6, Excel 16.0, 2026-10-23) and nested arrays are
    /// not modelled.
    fn lifted(
        &mut self,
        target: Target,
        arguments: &[CalcValue],
        synthetic: bool,
    ) -> Result<CalcValue, EvalFault> {
        let lifted: Vec<usize> = (0..arguments.len())
            .filter(|index| is_lifted(target, self.instance.plan.form, *index, &arguments[*index]))
            .collect();
        // dc-b1l: a call run once per reference of an array of references gives values, not
        // references (Excel 16.0.20527: `=INDEX(OFFSET(A1,{0;1},0,2),1,1)` spills 1|2).
        let over_references = lifted.iter().any(|index| reference_array(&arguments[*index]));
        let shape = lift::shape(
            lifted
                .iter()
                .filter_map(|index| arguments[*index].as_array()),
        );
        let mut cells = Vec::with_capacity(shape.rows * shape.cols);
        let mut element: Vec<CalcValue> = arguments.to_vec();
        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let mut short = false;
                let mut held = None;
                for index in &lifted {
                    let array = arguments[*index].as_array().expect("a lifted array");
                    match lift::element(array, row, col) {
                        // dc-b1l: a reference of an array of references where one value is taken
                        // is #VALUE! (`=TEXT(OFFSET(A1,{0;1},0),"0.0")`, `OFFSET(...)*1`), except
                        // for the functions that run once per reference, which read its value
                        // (`=T(OFFSET(C1,{0;1},0))` spills x and empty text).
                        Some(value)
                            if matches!(value.core_ref(), CoreRef::Reference(_))
                                && target.0.parameter_class(*index)
                                    == Some(ParameterClass::Value) =>
                        {
                            if lifts_over_references(target, self.instance.plan.form) {
                                element[*index] =
                                    self.admit_value(value.clone(), Admission::Lift)?;
                            } else {
                                held = held.or(Some(CalcValue::error(WorksheetErrorCode::Value)));
                            }
                        }
                        Some(value) => match held_array_element(target, self.instance.plan.form, value) {
                            None => element[*index] = value.clone(),
                            Some(Ok(first)) => element[*index] = first,
                            Some(Err(error)) => held = held.or(Some(error)),
                        },
                        None => short = true,
                    }
                }
                if short {
                    cells.push(CalcValue::error(WorksheetErrorCode::NA));
                    continue;
                }
                if let Some(error) = held {
                    cells.push(error);
                    continue;
                }
                let value = self.eager(target, &element, synthetic)?;
                let value = match value.core_ref() {
                    CoreRef::Reference(reference) if over_references => match reference.areas() {
                        [area] if area.cell_count() == 1 => self.reader.value_of(reference)?,
                        _ => CalcValue::error(WorksheetErrorCode::Value),
                    },
                    _ => value,
                };
                cells.push(lifted_element(value)?);
            }
        }
        Ok(CalcValue::array(
            CalcArray::new(shape, cells).expect("one result per cell of the shape"),
        ))
    }

    /// Runs the lazy entry `target` once on values already evaluated.
    fn lazy_once(
        &mut self,
        target: Target,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, EvalFault> {
        let mut scratch = self.calls.pop().unwrap_or_default();
        self.reader.clear_refusal();
        let result = {
            let mut cells = ReaderCells::new(self.reader);
            let mut draws = self.random.map(RandomDraws);
            let mut services = call_services(self.numeric, &mut cells, draws.as_mut());
            invoke(
                target.0,
                &mut Evaluated(arguments),
                &mut services,
                &mut scratch,
            )
        };
        self.calls.push(scratch);
        self.show_call(target, false);
        result.map_err(|fault| self.function_fault(fault))
    }

    fn lazy_call(
        &mut self,
        target: Target,
        first_block: usize,
        argc: usize,
        admission: Admission,
    ) -> Result<(), EvalFault> {
        // The first argument decides which others run; it runs first, and when it is an array
        // the call lifts over it with every argument evaluated.
        let first = self
            .block(first_block)
            .and_then(|value| self.admit(target, 0, value, admission));
        if let Ok(value) = &first
            && is_lifted(target, self.instance.plan.form, 0, value)
        {
            let mut arguments = Vec::with_capacity(argc);
            arguments.push(value.clone());
            for index in 1..argc {
                let value = self.block(first_block + index)?;
                let value = self.admit(target, index, value, admission)?;
                arguments.push(self.lift_operand(value, admission)?);
            }
            let value = self.lifted_lazy(target, &arguments)?;
            self.stack.push(value);
            return Ok(());
        }
        let mut scratch = self.calls.pop().unwrap_or_default();
        let reader = self.reader;
        let numeric = self.numeric;
        let random = self.random;
        reader.clear_refusal();
        let mut arguments = Blocks {
            run: &mut *self,
            target,
            first_block,
            argc,
            admission,
            first: Some(first),
            fault: None,
        };
        let result = {
            let mut cells = ReaderCells::new(reader);
            let mut draws = random.map(RandomDraws);
            let mut services = call_services(numeric, &mut cells, draws.as_mut());
            invoke(target.0, &mut arguments, &mut services, &mut scratch)
        };
        let fault = arguments.fault.take();
        self.calls.push(scratch);
        self.show_call(target, false);
        if let Some(fault) = fault {
            return Err(fault);
        }
        let value = result.map_err(|fault| self.function_fault(fault))?;
        self.stack.push(value);
        Ok(())
    }

    /// An argument of a lazy call lifted over an array, as each element's call receives it: a
    /// reference to one cell as its value, a larger range lifted as the array of its values when
    /// the call lifts ranges, and refused otherwise.
    fn lift_operand(&self, value: CalcValue, admission: Admission) -> Result<CalcValue, EvalFault> {
        match value.core_ref() {
            CoreRef::Reference(reference) => {
                if let [area] = reference.areas()
                    && area.cell_count() == 1
                {
                    return self.reader.value_of(reference);
                }
                match admission {
                    Admission::Lift => self.reader.values_of(reference),
                    _ => Err(EvalFault::AreaAsValue {
                        area: reference.areas()[0],
                    }),
                }
            }
            _ => Ok(value),
        }
    }

    /// A lazy entry lifted over the arrays among `arguments`, every argument evaluated.
    fn lifted_lazy(
        &mut self,
        target: Target,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, EvalFault> {
        let shape = lift::shape(arguments.iter().filter_map(lifted_array));
        let mut cells = Vec::with_capacity(shape.rows * shape.cols);
        let mut element: Vec<CalcValue> = arguments.to_vec();
        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let mut short = false;
                let mut held = None;
                for (index, argument) in arguments.iter().enumerate() {
                    if let Some(array) = lifted_array(argument) {
                        match lift::element(array, row, col) {
                            Some(value) => match held_array_element(target, self.instance.plan.form, value) {
                                None => element[index] = value.clone(),
                                Some(Ok(first)) => element[index] = first,
                                Some(Err(error)) => held = held.or(Some(error)),
                            },
                            None => short = true,
                        }
                    }
                }
                if short {
                    cells.push(CalcValue::error(WorksheetErrorCode::NA));
                    continue;
                }
                if let Some(error) = held {
                    cells.push(error);
                    continue;
                }
                let value = self.lazy_once(target, &element)?;
                cells.push(lifted_element(value)?);
            }
        }
        Ok(CalcValue::array(
            CalcArray::new(shape, cells).expect("one result per cell of the shape"),
        ))
    }

    /// Calls the LAMBDA value beneath the top `argc` values with them as its arguments.
    fn invoke(&mut self, argc: usize) -> Result<(), EvalFault> {
        let start = self.stack.len() - argc;
        let callee = self.stack[start - 1].clone();
        let arguments: Vec<CalcValue> = self.stack.drain(start..).collect();
        self.stack.pop();
        let value = self.call_closure(&callee, arguments)?;
        self.stack.push(value);
        Ok(())
    }

    /// The value of calling `callee` with `arguments`: a LAMBDA call's value, or the worksheet
    /// error Excel gives a call of something else or of the wrong number of arguments.
    fn call_closure(&mut self, callee: &CalcValue, arguments: Vec<CalcValue>) -> Result<CalcValue, EvalFault> {
        let argc = arguments.len();
        if let Some(function) = super::closure::FunctionValue::of(callee) {
            // A function value: its entry called with these arguments, which lift over an array
            // given to a single-value parameter as in a dynamic-array formula (dc-h9u).
            let target = function.target;
            if !target.0.arity().accepts(argc) {
                return Ok(CalcValue::error(WorksheetErrorCode::Value));
            }
            self.stack.extend(arguments);
            let depth = self.stack.len() - argc;
            match self.call(target, argc, false, false, Admission::Lift) {
                Ok(()) => return Ok(self.stack.pop().expect("the call's result")),
                Err(fault) => {
                    self.stack.truncate(depth);
                    return Err(fault);
                }
            }
        }
        let Some(closure) = Closure::of(callee) else {
            // Excel 16.0, 2026-10-23: `=(A1)(2)` is #REF! and `=LET(x,1,x(2))` #VALUE!.
            return Ok(match callee.core_ref() {
                CoreRef::Reference(_) => CalcValue::error(WorksheetErrorCode::Ref),
                _ => CalcValue::error(WorksheetErrorCode::Value),
            });
        };
        if closure.plan != plan_address(self.instance) {
            return Err(not_modelled("a LAMBDA value made by another formula"));
        }
        // Excel 16.0, 2026-10-23: `=LAMBDA(x,y,x)(1)` and `=LAMBDA(x,x)(1,2)` are #VALUE!.
        if argc > closure.params || argc < closure.required {
            return Ok(CalcValue::error(WorksheetErrorCode::Value));
        }
        let cost = CALL_COST
            + CALLABLE_ARGUMENT_COST
                * arguments
                    .iter()
                    .filter(|value| value.callable_value().is_some())
                    .count();
        if self.budget + cost > RECURSION_BUDGET {
            return Ok(CalcValue::error(WorksheetErrorCode::Num));
        }
        if self.depth >= MAX_CALL_DEPTH {
            return Err(EvalFault::NotModelled {
                construct: "LAMBDA calls or defined names nested more deeply than this package evaluates",
                owner: "dc-10y",
            });
        }
        let frame = self.locals.len();
        self.locals.extend(closure.captured.iter().cloned());
        self.locals.extend(arguments);
        for _ in argc..closure.params {
            self.locals.push(CalcValue::missing());
        }
        let caller = std::mem::replace(&mut self.base, frame);
        self.depth += 1;
        self.budget += cost;
        let value = self.block(closure.block);
        self.budget -= cost;
        self.depth -= 1;
        self.base = caller;
        self.locals.truncate(frame);
        value
    }

    /// Runs the block of a defined name's formula in a frame of its own and pushes its value.
    fn name(&mut self, block: usize) -> Result<(), EvalFault> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(EvalFault::NotModelled {
                construct: "LAMBDA calls or defined names nested more deeply than this package evaluates",
                owner: "dc-10y",
            });
        }
        let frame = self.locals.len();
        let caller = std::mem::replace(&mut self.base, frame);
        self.depth += 1;
        let value = self.block(block);
        self.depth -= 1;
        self.base = caller;
        self.locals.truncate(frame);
        self.stack.push(value?);
        Ok(())
    }

    /// The spill reference: the range spilled from the one cell the operand names, #REF! when
    /// that cell spills nothing.
    fn spill(&mut self) -> Result<(), EvalFault> {
        let operand = self.stack.pop().expect("an operand");
        let value = match operand.core_ref() {
            CoreRef::Reference(reference) => match reference.areas() {
                [area] if area.cell_count() == 1 => match self.reader.spill_extent(area.first()) {
                    SpillExtent::Area(area) => CalcValue::reference(ReferenceLike::area(area)),
                    SpillExtent::None => CalcValue::error(WorksheetErrorCode::Ref),
                    SpillExtent::NotModelled => {
                        return Err(EvalFault::NotModelled {
                            construct: "a spilled range reference (#) the reader does not know",
                            owner: "dc-10y",
                        });
                    }
                },
                _ => CalcValue::error(WorksheetErrorCode::Ref),
            },
            CoreRef::Error(_) if operand.callable_value().is_none() => operand,
            _ => CalcValue::error(WorksheetErrorCode::Value),
        };
        self.stack.push(value);
        Ok(())
    }

    /// Shows the observer, when there is one, that `target` ran.
    fn show_call(&mut self, target: Target, synthetic: bool) {
        if let Some(observer) = self.observer.as_deref_mut() {
            observer.called(ExecutedCall {
                target: target.0,
                synthetic,
            });
        }
    }

    /// Shows the observer, when there is one, that `target` resolved reference text to `area`.
    fn show_resolved(&mut self, target: Target, area: GridArea) {
        if let Some(observer) = self.observer.as_deref_mut() {
            observer.resolved(ResolvedReference {
                target: target.0,
                area,
            });
        }
    }

    /// A call's fault, with the reader's refusal when the kernel could not read a reference.
    fn function_fault(&self, fault: FunctionFault) -> EvalFault {
        let refusal = self.reader.take_refusal();
        match (&fault, refusal) {
            (FunctionFault::ReferenceUnreadable { .. }, Some(refusal)) => refusal.fault(),
            _ => EvalFault::Function(fault),
        }
    }

    /// The check of a final `+` or `-`: pops the negations of its right and left operands, its
    /// result and its operands, and pushes the result, or 0 where Excel adjusts it
    /// ([`NEAR_ZERO_EXPONENTS`]). A left negation that is not a number leaves the left operand's
    /// exponent unknown, and a non-zero number result is then refused.
    fn near_zero_check(&mut self) -> Result<(), EvalFault> {
        let base = self.stack.len() - 5;
        let left = self.stack[base + 3].as_number();
        let mut value = self.stack[base + 2].clone();
        self.stack.truncate(base);
        if let Some(result) = value.as_number().filter(|result| *result != 0.0) {
            let Some(left) = left else {
                return Err(EvalFault::NearZeroFinalSum { result });
            };
            if biased_exponent(left) - biased_exponent(result) >= NEAR_ZERO_EXPONENTS {
                value = CalcValue::number(0.0);
            }
        }
        self.stack.push(value);
        Ok(())
    }
}

/// The address of an instance's plan, which identifies the closures it makes.
fn plan_address(instance: &FormulaInstance) -> usize {
    std::sync::Arc::as_ptr(&instance.plan) as usize
}

/// Whether argument `index` of `target`, as admitted, is lifted over: an array at a parameter
/// that takes one value, or an array of references given to a function that lifts over one.
fn is_lifted(target: Target, form: FormulaForm, index: usize, value: &CalcValue) -> bool {
    (target.0.parameter_class(index) == Some(ParameterClass::Value) && lifted_array(value).is_some())
        || (reference_array(value) && lifts_over_references(target, form))
}

/// Whether `target` runs once per reference of an array of references in a formula of `form`.
fn lifts_over_references(target: Target, form: FormulaForm) -> bool {
    let spelling = target.0.spelling();
    match form {
        FormulaForm::Legacy => LEGACY_LIFTS_OVER_REFERENCES.contains(&spelling),
        _ => LIFTS_OVER_REFERENCES.contains(&spelling),
    }
}

/// The functions that run once per reference of an array of references in a legacy formula
/// (dc-b1l, the catalogue's dcb1l_language rows, Excel 16.0.20527, 2026-10-30, over 1, 2, 3 in
/// E1:E3: `=SUM(SUBTOTAL(9,OFFSET(E1,{0;1},0,2)))` is 8, `=SUM(N(OFFSET(E1,{0;1;2},0)))` 6,
/// `=SUM(SUMIF(OFFSET(E1,{0;1},0,2),">1"))` 7, `=SUMPRODUCT(AREAS(OFFSET(E1,{0;1},0)))` 2). Any
/// other function given one takes its first reference (`=SUM(OFFSET(E1,{0,1,2},0))` is 1,
/// `=ROWS(...)` 1, `=ISREF(...)` TRUE, `=SUMPRODUCT(ROW(OFFSET(E1,{0;1},0)))` 1).
const LEGACY_LIFTS_OVER_REFERENCES: [&str; 6] = ["SUBTOTAL", "SUMIF", "COUNTIF", "AREAS", "N", "T"];

/// Whether `value` is an array holding a reference: OFFSET or INDIRECT lifted over an array
/// (dc-b1l).
fn reference_array(value: &CalcValue) -> bool {
    lifted_array(value).is_some_and(|array| {
        array
            .iter_row_major()
            .any(|element| matches!(element.core_ref(), CoreRef::Reference(_)))
    })
}

/// The functions that run once per reference of an array of references given where they take a
/// reference (dc-b1l, Excel 16.0.20527 through COM, 2026-10-30: `=SUBTOTAL(9,OFFSET(A1,{0;1;2},0,2))`
/// spills 3|5|7, `=SUMIF(OFFSET(A1,{0;1},0,3),">2")` 3|7, `=N(OFFSET(A1,{0;1;2},0))` 1|2|3, and
/// COUNTIF, AVERAGEIF, SUMIFS, AREAS, ROW, CELL, INDEX and T likewise).
const LIFTS_OVER_REFERENCES: [&str; 11] = [
    "SUBTOTAL", "SUMIF", "COUNTIF", "AVERAGEIF", "SUMIFS", "AREAS", "ROW", "CELL", "INDEX", "N",
    "T",
];

/// The functions measured to take an array of references whole (dc-b1l, 2026-10-30): SUM,
/// AVERAGE and MAX read each reference's first cell (`=SUM(OFFSET(A1,{0;1;2},0,2))` is 6 over
/// 1..6 in A1:A6), COUNT counts none of them and COUNTA each, ROWS counts the array's rows and
/// ISREF is FALSE.
const TAKES_REFERENCE_ARRAYS: [&str; 7] = ["SUM", "AVERAGE", "MAX", "COUNT", "COUNTA", "ROWS", "ISREF"];

/// The first element of `array`, as one value (a blank as Empty).
fn first_element(array: &CalcArray) -> CalcValue {
    let value = array.get(0, 0).expect("a non-empty array");
    match value.core_ref() {
        CoreRef::Missing => CalcValue::empty(),
        _ => value.clone(),
    }
}

/// The element a call lifted over an array is given for an element that is itself an array
/// (a GROUPBY result's cell, dc-h9u): in a dynamic-array formula an operator gives #VALUE! there
/// (Excel 16.0.20527: `=GROUPBY(k,v,LAMBDA(x,{7,8}),0,0)*2` is #VALUE! in each such cell); a
/// function, and an operator in a legacy formula, take the held array's first element
/// (`=ISNUMBER(GROUPBY(...))` is TRUE there, legacy `=INDEX(GROUPBY(k,v,LAMBDA(x,x))*2,1,2)` is 2);
/// `None` for any other element, which is given as it is.
fn held_array_element(
    target: Target,
    form: FormulaForm,
    value: &CalcValue,
) -> Option<Result<CalcValue, CalcValue>> {
    let CoreRef::Array(held) = value.core_ref() else {
        return None;
    };
    if value.callable_value().is_some() {
        return None;
    }
    Some(match (target.0.notation(), form) {
        (calc_func::Notation::Call, _) | (_, FormulaForm::Legacy) => Ok(first_element(held)),
        _ => Err(CalcValue::error(WorksheetErrorCode::Value)),
    })
}

/// The array `value` is, when it is a plain array (not a LAMBDA value).
fn lifted_array(value: &CalcValue) -> Option<&CalcArray> {
    if value.callable_value().is_some() {
        return None;
    }
    value.as_array()
}

/// One element's result of a lifted call, as the array of results holds it.
fn lifted_element(value: CalcValue) -> Result<CalcValue, EvalFault> {
    match value.core_ref() {
        // dc-b1l: OFFSET or INDIRECT lifted over an array gives an array of references.
        CoreRef::Reference(_) => Ok(value),
        CoreRef::Array(_) if value.callable_value().is_none() => Err(not_modelled(
            "a function lifted over an array whose result is an array",
        )),
        CoreRef::Missing => Ok(CalcValue::empty()),
        _ => Ok(value),
    }
}

/// Reference text bound for one eager call, INDIRECT's, in the evaluated formula's workbook
/// context, with every area it resolved kept for the observer. A lazy call is given none: no lazy
/// entry resolves reference text, and its arguments' own calls are given their own.
struct TextBinding<'c> {
    catalog: &'c dyn ReferenceCatalog,
    anchor: FormulaAnchor,
    resolved: Vec<GridArea>,
}

impl ReferenceText for TextBinding<'_> {
    fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference {
        let notation = if a1 {
            ReferenceNotation::A1
        } else {
            ReferenceNotation::R1C1
        };
        let bound = bind_text(text, notation, self.anchor, self.catalog);
        if let TextReference::Area(area) = bound {
            self.resolved.push(area);
        }
        // dc-ylr: text naming another workbook names a closed one, whose cells INDIRECT does not
        // read: Excel 16.0.20527 gives INDIRECT("'C:\dir\[src.xlsx]Sheet1'!A1") #REF! with that
        // workbook closed (2026-10-30).
        if bound == TextReference::NotModelled && names_another_workbook(text) {
            return TextReference::Invalid;
        }
        bound
    }
}

/// Whether reference text (UTF-16 code units) names another workbook: a qualifier holding `[`,
/// quoted (`'[Book2]Sheet1'!A1`) or not (`[Book2]Sheet1!A1`, `[1]!Name`).
fn names_another_workbook(text: &[u16]) -> bool {
    let text = String::from_utf16_lossy(text);
    let text = text.trim_start();
    let qualifier = match text.strip_prefix('\'') {
        Some(quoted) => quoted.split('\'').next().unwrap_or_default(),
        None => text.split('!').next().unwrap_or_default(),
    };
    text.contains('!') && qualifier.contains('[')
}

/// The caller of LAMBDA values for one eager call's kernel ([`calc_func::CallableInvoker`]): each
/// call runs the LAMBDA's body in this evaluation, as a call written in the formula does. The
/// first fault stops every further call; the kernel's call is then discarded and the fault
/// reported.
struct Callables<'b, 'a, 'r> {
    run: &'b mut Run<'a, 'r>,
    fault: Option<EvalFault>,
}

impl CallableInvoker for Callables<'_, '_, '_> {
    fn call(&mut self, callable: &CalcValue, arguments: &[CalcValue]) -> Result<CalcValue, CallAbandoned> {
        if self.fault.is_some() {
            return Err(CallAbandoned);
        }
        let value = self
            .run
            .call_closure(callable, arguments.to_vec())
            .and_then(|value| match value.core_ref() {
                // A LAMBDA's value that is a reference reaches the helper as its value: one
                // cell's value, or the array of a larger range's values.
                CoreRef::Reference(reference) => match reference.areas() {
                    [area] if area.cell_count() == 1 => self.run.reader.value_of(reference),
                    _ => self.run.reader.values_of(reference),
                },
                _ => Ok(value),
            });
        match value {
            Ok(value) => Ok(value),
            Err(fault) => {
                self.fault = Some(fault);
                Err(CallAbandoned)
            }
        }
    }
}

/// The evaluated arguments of an eager call.
struct Evaluated<'v>(&'v [CalcValue]);

impl PreparedArguments for Evaluated<'_> {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

/// The arguments of a lazy call: argument `i` runs block `first_block + i` when the kernel asks
/// for it, except the first, which ran before the call. The first fault stops every further
/// argument; the call's result is then discarded.
struct Blocks<'b, 'a, 'r> {
    run: &'b mut Run<'a, 'r>,
    target: Target,
    first_block: usize,
    argc: usize,
    admission: Admission,
    first: Option<Result<CalcValue, EvalFault>>,
    fault: Option<EvalFault>,
}

impl PreparedArguments for Blocks<'_, '_, '_> {
    fn count(&self) -> usize {
        self.argc
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        if self.fault.is_some() {
            return CalcValue::error(ABANDONED);
        }
        let evaluated = match (index, self.first.take()) {
            (0, Some(first)) => first,
            (_, first) => {
                self.first = first;
                self.run
                    .block(self.first_block + index)
                    .and_then(|value| self.run.admit(self.target, index, value, self.admission))
            }
        };
        match evaluated {
            Ok(value) => value,
            Err(fault) => {
                self.fault = Some(fault);
                CalcValue::error(ABANDONED)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{
        EvalScratch, EvaluationObserver, EvaluationReport, ExecutedCall, PlanCache, compile,
        instantiate,
    };
    use crate::parse_formula;
    use crate::reference::{
        CellRun, FormulaAnchor, ReferenceCatalog, ReferenceNotation, ReferenceReader,
    };
    use calc_func::value::{ExcelText, IdAllocator, SheetId};
    use std::ops::ControlFlow;

    struct NoSheets;

    impl ReferenceCatalog for NoSheets {
        fn sheet_named(&self, _: &str) -> Option<SheetId> {
            None
        }
    }

    /// Every cell holds the same value.
    struct Everywhere(CalcValue);

    impl ReferenceReader for Everywhere {
        fn read_area(
            &mut self,
            area: GridArea,
            sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
        ) {
            for _ in 0..area.cell_count() {
                if sink(CellRun::Values(std::slice::from_ref(&self.0))).is_break() {
                    return;
                }
            }
        }
    }

    fn evaluated(formula: &str, cells: CalcValue) -> Result<CalcValue, EvalFault> {
        evaluated_under(&NumericMode::Fast, formula, cells)
    }

    fn evaluated_under(
        mode: &NumericMode,
        formula: &str,
        cells: CalcValue,
    ) -> Result<CalcValue, EvalFault> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let anchor = FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), 5, 5).unwrap());
        let plan = compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &NoSheets,
            &mut PlanCache::new(),
        )
        .unwrap();
        let instance = instantiate(&plan, anchor);
        let mut reader = Everywhere(cells);
        let mut scratch = EvalScratch::new();
        evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut EvaluationServices::new(mode, &mut reader),
        )
    }

    fn text(s: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_interop_assignment(s))
    }

    /// What an observer is shown of one evaluation: each call, in order, as its spelling and
    /// notation with `*` appended when it is synthetic, and each report: its anchor, numeric mode
    /// and outcome.
    #[derive(Default)]
    struct Shown {
        calls: Vec<String>,
        reports: Vec<(FormulaAnchor, NumericMode)>,
        outcomes: Vec<Result<CalcValue, EvalFault>>,
    }

    impl EvaluationObserver for Shown {
        fn called(&mut self, call: ExecutedCall) {
            let target = call.target();
            self.calls.push(format!(
                "{:?} {}{}",
                target.notation(),
                target.spelling(),
                if call.synthetic() { " *" } else { "" }
            ));
        }

        fn returned(&mut self, report: EvaluationReport<'_>) {
            self.reports
                .push((report.anchor(), report.numeric_mode().clone()));
            self.outcomes
                .push(report.outcome().cloned().map_err(Clone::clone));
        }
    }

    /// The anchor [`observed`] evaluates at.
    fn observed_anchor() -> FormulaAnchor {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), 5, 5).unwrap())
    }

    /// Evaluates `formula` under Fast, every cell holding `cells`, with an observer attached.
    fn observed(formula: &str, cells: CalcValue) -> (Result<CalcValue, EvalFault>, Shown) {
        let anchor = observed_anchor();
        let plan = compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &NoSheets,
            &mut PlanCache::new(),
        )
        .unwrap();
        let instance = instantiate(&plan, anchor);
        let mut reader = Everywhere(cells);
        let mut scratch = EvalScratch::new();
        let mut shown = Shown::default();
        let outcome = evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut EvaluationServices::new(&NumericMode::Fast, &mut reader).observed_by(&mut shown),
        );
        (outcome, shown)
    }

    /// The observer is shown each call the evaluation ran, as it returned, with the near-zero
    /// check's two negations marked synthetic and a negation the formula wrote not, and then the
    /// outcome the evaluation returned, once, the same as `evaluate` returns, with the instance's
    /// anchor and the numeric mode. A formula whose result is a reference to one cell is shown the
    /// cell's value, as `evaluate` returns it, never the reference.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_observer_is_shown_calls_and_the_outcome() {
        let three = CalcValue::number(3.0);
        // Each formula, the value every cell holds, the calls shown and the outcome.
        type Case<'c> = (
            &'c str,
            CalcValue,
            &'c [&'c str],
            Result<CalcValue, EvalFault>,
        );
        let cases: [Case<'_>; 10] = [
            // A bare reference: the cell's value, not the reference.
            ("=A1", three.clone(), &[], Ok(CalcValue::number(3.0))),
            // A blank cell's value is the number 0.
            ("=A1", CalcValue::empty(), &[], Ok(CalcValue::number(0.0))),
            // A lazy call that returns the reference its argument wrote: the cell's value.
            (
                "=IF(TRUE,A1,0)",
                three.clone(),
                &["Call IF"],
                Ok(CalcValue::number(3.0)),
            ),
            // -3 + 10 = 7: the written negation, the final sum, and the check's two negations.
            (
                "=-A1+10",
                three.clone(),
                &["Prefix -", "Infix +", "Prefix - *", "Prefix - *"],
                Ok(CalcValue::number(7.0)),
            ),
            // A product is not checked.
            (
                "=A1*4",
                three.clone(),
                &["Infix *"],
                Ok(CalcValue::number(12.0)),
            ),
            // A result Excel adjusts: the check runs and publishes 0.
            (
                "=0.5-0.4-0.1",
                CalcValue::empty(),
                &["Infix -", "Infix -", "Prefix - *", "Prefix - *"],
                Ok(CalcValue::number(0.0)),
            ),
            // A result that is not a non-zero number skips the check, so no negation runs.
            (
                "=\"a\"-1",
                CalcValue::empty(),
                &["Infix -"],
                Ok(CalcValue::error(WorksheetErrorCode::Value)),
            ),
            // A lazy call is shown after the calls of the argument it asked for; a sum in its
            // branch is not the formula's final operation, so it is not checked.
            (
                "=IF(TRUE,A1+1,A1*2)",
                three.clone(),
                &["Infix +", "Call IF"],
                Ok(CalcValue::number(4.0)),
            ),
            // An argument the evaluation cannot supply stops the call before its function runs: a
            // range at a place of a legacy formula whose evaluation is not modelled (COUNTIFS's
            // range, which takes only a reference: Excel refuses to enter the formula).
            (
                "=COUNTIFS(A1:A2+1,1)",
                three.clone(),
                &[],
                evaluated("=COUNTIFS(A1:A2+1,1)", three.clone()),
            ),
            // A product below the normal range, which Excel gives as 0 (dc-b1l).
            (
                "=1E-300*1E-10",
                CalcValue::empty(),
                &["Infix *"],
                evaluated("=1E-300*1E-10", CalcValue::empty()),
            ),
        ];
        assert!(matches!(cases[8].3, Err(EvalFault::AreaAsValue { .. })));
        // A product below the normal range is 0, as Excel gives it.
        assert_eq!(cases[9].3, Ok(CalcValue::number(0.0)));
        for (formula, cells, calls, expected) in cases {
            let (outcome, shown) = observed(formula, cells.clone());
            assert_eq!(outcome, expected, "{formula}");
            assert_eq!(shown.calls, calls, "{formula}");
            assert_eq!(shown.outcomes, [expected], "{formula}");
            assert_eq!(
                shown.reports,
                [(observed_anchor(), NumericMode::Fast)],
                "{formula}"
            );
            // The observer changes nothing the evaluation returns.
            assert_eq!(evaluated(formula, cells), outcome, "{formula}");
        }
    }

    /// The near-zero rows of cfunc-m1.2's oracle workbook and the measured rule (see
    /// [`NEAR_ZERO_EXPONENTS`]): Excel's values as its capture and the dc-b1l probes give them.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_final_sum_near_zero_is_adjusted_as_excel_adjusts_it() {
        let blank = CalcValue::empty();
        let n = |x: f64| Ok(CalcValue::number(x));
        assert_eq!(evaluated("=0.5-0.4-0.1", blank.clone()), n(0.0));
        assert_eq!(evaluated("=1.333+1.225-1.333-1.225", blank.clone()), n(0.0));
        assert_eq!(evaluated("=0.1+0.2-0.3", blank.clone()), n(0.0));
        assert_eq!(evaluated("=1+1E-15-1", blank.clone()), n(0.0));
        assert_eq!(evaluated("=1E15+1-1E15", blank.clone()), n(1.0));
        // Below the root operation the result stands.
        assert_eq!(
            evaluated("=1*(0.5-0.4-0.1)", blank.clone()),
            n(-2.7755575615628914e-17)
        );
        assert_eq!(
            evaluated("=(0.5-0.4-0.1)", blank.clone()),
            n(-2.7755575615628914e-17)
        );
        assert_eq!(
            evaluated("=-(0.5-0.4-0.1)", blank.clone()),
            n(2.7755575615628914e-17)
        );
        assert_eq!(
            evaluated("=IF(TRUE,0.5-0.4-0.1)", blank.clone()),
            n(-2.7755575615628914e-17)
        );
        // The left operand's exponent decides: 2+7*2^-51 against 2-2^-52 is adjusted (k up to
        // 7), and in the other order only up to k = 3.
        assert_eq!(
            evaluated("=(2+7*POWER(2,-51))-(2-POWER(2,-52))", blank.clone()),
            n(0.0)
        );
        assert_eq!(
            evaluated("=(2-POWER(2,-52))-(2+4*POWER(2,-51))", blank.clone()),
            n(-9.0 * 2f64.powi(-52))
        );
        // Outside the band, and exactly zero, the result is published.
        assert_eq!(
            evaluated("=1-0.9999999", blank.clone()),
            Ok(CalcValue::number(1.0 - 0.9999999))
        );
        assert_eq!(
            evaluated("=0.5-0.5", blank.clone()),
            Ok(CalcValue::number(0.0))
        );
        // A text operand is read as calc-func reads it.
        assert_eq!(evaluated("=0.1+0.2-\"0.3\"", blank.clone()), n(0.0));
        assert_eq!(
            evaluated("=\"3\"-\"1\"", blank.clone()),
            Ok(CalcValue::number(2.0))
        );
        // A result that is not a non-zero number is not checked: the check's negations do not
        // run. Under Exact, negating the text "1" would be refused (reading text as a number
        // rounds), so the #N/A result shows that they did not.
        assert_eq!(
            evaluated("=\"a\"-1", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::Value))
        );
        let exact = NumericMode::Exact(
            calc_func::value::NumericTargetId::new("test", "test", "test", "test")
                .expect("a target"),
        );
        assert_eq!(
            evaluated_under(&exact, "=#N/A+\"1\"", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::NA))
        );
        assert!(matches!(
            evaluated_under(&exact, "=-\"1\"", blank),
            Err(EvalFault::Function(FunctionFault::ExactUnavailable { .. }))
        ));
    }

    /// A reference to one cell is its value where a value is needed and as the result; a blank
    /// cell is 0 as the result. A legacy formula intersects a reference to more cells.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn references_where_values_are_needed() {
        assert_eq!(evaluated("=A1", text("a")), Ok(text("a")));
        assert_eq!(
            evaluated("=A1", CalcValue::empty()),
            Ok(CalcValue::number(0.0))
        );
        assert_eq!(evaluated("=A1&\"x\"", CalcValue::empty()), Ok(text("x")));
        assert_eq!(
            evaluated("=IF(TRUE,A1)", CalcValue::empty()),
            Ok(CalcValue::number(0.0))
        );
        // A legacy formula intersects a larger range with its own row (row 5 here).
        assert_eq!(
            evaluated("=A1:A2+1", CalcValue::number(1.0)),
            Ok(CalcValue::error(WorksheetErrorCode::Value))
        );
        assert_eq!(
            evaluated("=A1:A9+1", CalcValue::number(1.0)),
            Ok(CalcValue::number(2.0))
        );
        assert_eq!(
            evaluated("=A1:A2", CalcValue::number(1.0)),
            Ok(CalcValue::error(WorksheetErrorCode::Value))
        );
        assert_eq!(
            evaluated("=SUM(A1:B2)", CalcValue::number(1.5)),
            Ok(CalcValue::number(6.0))
        );
    }

    /// IFERROR's fallback runs only when its value is an error; a value whose evaluation faults
    /// stops the call without running the fallback; a blank value is the formula's 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn iferror_runs_its_fallback_only_on_an_error() {
        let blank = CalcValue::empty();
        let cases: [(&str, &[&str]); 4] = [
            ("=IFERROR(1/0,5)", &["Infix /", "Call IFERROR"]),
            ("=IFERROR(2,1/0)", &["Call IFERROR"]),
            ("=IFERROR(1E-300*1E-10,1/0)", &["Infix *", "Call IFERROR"]),
            ("=IFNA(1/0,1/0)", &["Infix /", "Call IFNA"]),
        ];
        let outcomes = [
            Ok(CalcValue::number(5.0)),
            Ok(CalcValue::number(2.0)),
            evaluated("=1E-300*1E-10", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::Div0)),
        ];
        for ((formula, calls), expected) in cases.into_iter().zip(outcomes) {
            let (outcome, shown) = observed(formula, blank.clone());
            assert_eq!(outcome, expected, "{formula}");
            assert_eq!(shown.calls, calls, "{formula}");
        }
        assert_eq!(
            evaluated("=IFERROR(A1,\"x\")", blank.clone()),
            Ok(CalcValue::number(0.0))
        );
        assert_eq!(
            evaluated("=IFERROR(A1,1)&\"|\"", blank.clone()),
            Ok(text("|"))
        );
        // A legacy formula intersects the range; row 5 lies outside it: #VALUE!, caught.
        assert_eq!(
            evaluated("=IFERROR(A1:A2,1)", blank),
            Ok(CalcValue::number(1.0))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn function_faults_pass_through() {
        // A difference below the normal range, which calc-func's number rule refuses.
        assert!(matches!(
            evaluated("=POWER(2,-1022)*(1+POWER(2,-52))-POWER(2,-1022)", CalcValue::empty()),
            Err(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        ));
    }

    /// A file's field access `_FV(value, field)` is FIELDVALUE (dc-ifc): #FIELD! for a value that is
    /// no linked data type, as Excel gives it.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_field_access_is_fieldvalue() {
        assert_eq!(
            evaluated("=_FV(1,\"Price\")", CalcValue::empty()),
            Ok(CalcValue::error(WorksheetErrorCode::Field))
        );
    }

    /// ROW and COLUMN of several rows or columns in a legacy formula, with Excel's values (dc-b1l,
    /// Excel 16.0.20430 on 2026-10-25): `@` before the call except where an array is taken.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn row_and_column_are_intersected_in_a_legacy_formula() {
        let blank = CalcValue::empty();
        let n = |x: f64| Ok(CalcValue::number(x));
        assert_eq!(evaluated("=ROW(B3:B5)", blank.clone()), n(3.0));
        assert_eq!(evaluated("=SUM(ROW(B3:B5))", blank.clone()), n(3.0));
        assert_eq!(evaluated("=SUMPRODUCT(ROW(B3:B5))", blank.clone()), n(12.0));
        assert_eq!(evaluated("=ROWS(ROW(B3:B5))", blank.clone()), n(3.0));
        assert_eq!(
            evaluated("=MATCH(4,ROW(B3:B5),0)", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::NA))
        );
        assert_eq!(evaluated("=COLUMN(B3:D3)", blank.clone()), n(2.0));
        assert_eq!(evaluated("=SUM(COLUMN(B3:D3))", blank), n(2.0));
    }
}
