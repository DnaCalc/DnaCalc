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
//! Every operator and function runs through `calc_func::invoke`. Every value a formula reads
//! comes from the caller's [`ReferenceReader`], which is also the `calc_func::CellReader` the
//! kernels read reference arguments through (see the `cells` module). A function whose catalogue
//! entry is lazy (IF) asks for its arguments itself, and only the arguments it asks for run.
//!
//! Evaluation does not compile: [`evaluate`] receives no formula text, syntax tree or catalogue.

mod cells;
mod compile;
mod run;

pub use compile::{CompileError, compile};
pub use run::{EvalFault, evaluate};

use crate::reference::{FormulaAnchor, PlacementError, ReferenceOperand, ReferenceReader};
use calc_func::value::{
    CalcValue, ExcelText, GridArea, NumericMode, ReferenceLike, WorksheetErrorCode,
};
use calc_func::{FunctionCallScratch, FunctionCallTarget};
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
// instruction to its source span, or an observer called once per executed instruction, can be
// kept beside the plan without changing it. Neither exists yet.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CompiledPlan {
    blocks: Box<[Box<[Instruction]>]>,
    references: Box<[ReferenceOperand]>,
}

impl CompiledPlan {
    /// The plan's references in the order compile met them, those in arguments a lazy function
    /// may never ask for included.
    pub fn references(&self) -> &[ReferenceOperand] {
        &self.references
    }
}

/// One step of a plan.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Instruction {
    /// Pushes the literal's value.
    Literal(Literal),
    /// Pushes an omitted argument ([`CalcValue::missing`]).
    Omitted,
    /// Pushes reference operand `.0` of the plan as the instance placed it: a reference to its
    /// area, or `#REF!` where it could not be placed.
    Reference(usize),
    /// Pops `argc` values, calls `target` with them and pushes the result. With `retain`, the
    /// arguments as the call received them (each reference to a value parameter replaced by its
    /// cell's value) stay on the stack beneath the result instead of being popped.
    Call {
        target: Target,
        argc: usize,
        retain: bool,
    },
    /// Calls the lazy entry `target`, whose argument `i` is block `first_block + i`; it pushes the
    /// result.
    LazyCall {
        target: Target,
        first_block: usize,
        argc: usize,
    },
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
}

impl Literal {
    fn value(&self) -> CalcValue {
        match self {
            Literal::Number(bits) => CalcValue::number(f64::from_bits(*bits)),
            Literal::Text(text) => CalcValue::text(text.clone()),
            Literal::Logical(b) => CalcValue::logical(*b),
            Literal::Error(code) => CalcValue::error(*code),
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

    /// The value reference operand `index` pushes.
    fn reference(&self, index: usize) -> CalcValue {
        match self.areas[index] {
            Ok(area) => CalcValue::reference(ReferenceLike::area(area)),
            Err(placement) => CalcValue::error(placement.worksheet_error()),
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
            .map(|operand| operand.instantiate(anchor))
            .collect(),
    }
}

/// What an evaluation runs with, owned by the caller: the numeric policy, which every arithmetic
/// site reads, and the reader that supplies every cell the formula reads.
pub struct EvaluationServices<'s> {
    numeric: &'s NumericMode,
    reader: &'s mut dyn ReferenceReader,
}

impl<'s> EvaluationServices<'s> {
    pub fn new(numeric: &'s NumericMode, reader: &'s mut dyn ReferenceReader) -> Self {
        EvaluationServices { numeric, reader }
    }
}

impl fmt::Debug for EvaluationServices<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvaluationServices")
            .field("numeric", self.numeric)
            .finish_non_exhaustive()
    }
}

/// Storage the caller owns and reuses across evaluations: the operand stack and the call scratch
/// of each level of lazy calls. It holds nothing from one evaluation to the next.
#[derive(Debug, Default)]
pub struct EvalScratch {
    stack: Vec<CalcValue>,
    calls: Vec<FunctionCallScratch>,
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
        EvalFrame { instance, scratch }
    }
}
