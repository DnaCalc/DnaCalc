# Question for GPT-6 Pro: design review of DNA Calc's formula evaluator and its contract with the engine

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce
Excel's observable behaviour exactly (bit-identical numbers, the same error values, the same
invocation counts where Excel exposes them through callbacks). The repository is public:
https://github.com/DnaCalc/DnaCalc, MIT. This question is pinned to commit
`95c1b3bf9ff4661f64214be2d353cb549c5cef33`
(https://github.com/DnaCalc/DnaCalc/tree/95c1b3bf9ff4661f64214be2d353cb549c5cef33). Every path
below is relative to the repository root, and every `file:line` refers to that commit. You may
read the repository if you can; everything you need to answer is also in the appendices, so do not
depend on browsing.

The evaluator is `crates/calc-fml` (the Excel formula language: parse, bind, compile, evaluate),
over the value model in `crates/calc-value` and the function catalogue in `crates/calc-func` and
`crates/calc-catalogue`. Its caller is the engine, `crates/calc-engine` (one dependency graph,
dirty closure, ordering, staging, publication). An earlier review round (October 2026) fixed the
architecture around it: the recalculation contract, the separation of execution order, read view
and publication, invocation identity, read certificates. That round did not look inside the
evaluator. This one does.

## What we ask

Review the evaluator's design, not every function. "Not rethink everything, but deep-dive,
brainstorm, review, and rework where we can do better." Three later goals shape what "better"
means:

1. **The engine above the evaluator will be optimised later**: multi-threading across cells (a
   first version exists for one wave, appendix J.6), smart and incremental recalculation (early
   cutoff, an observer-driven necessary set, incremental ordering). The evaluator must give the
   engine what those need and must not hide state the engine would have to rediscover.
2. **The evaluator may be transcoded to another language, or compiled into evaluation traces**
   (straight-line programs per formula or per block of formulas, possibly for a GPU). Its model
   must be small, explicit and free of incidental Rust-isms.
3. **Excel's subtle behaviours must be explicit in the design and the code**, not implicit in
   control flow. The readers of this codebase are agents; clarity of model and names matters more
   than prose for people.

Please produce, in this order:

**A. A design critique** of the evaluator as it stands: the plan and instruction set (appendix B),
the value model (C), the core loop and its suspension and retry model (D), argument admission,
lifting and lazy evaluation (E), the legacy (implicit-intersection) form (F), LET, LAMBDA and
defined names (G), reference binding and INDIRECT (H), reads, effects, user functions, RTD and
random draws (I), and the engine contract including the runtime dependency overlay (J). For each:
what the design encodes well, what is implicit where it should be explicit, what is wrong or
fragile, and what is merely untidy. Separate what you can verify from the excerpts from what you
infer.

**B. Ranked improvement proposals**, each with: the change; the benefit for clarity, for
multithreading, for smart recalculation, for transcoding or trace compilation; the risk to
bit-exact Excel parity; the rough size (hours, days, weeks of one agent); what evidence or
experiment should come first. Keep the list to changes worth doing. Put bold ideas in a separate
section.

**C. The explicit model of Excel behaviours the evaluator must express.** We want a compact,
named list (types and enumerations in Rust-flavoured pseudocode are welcome) of the semantic
dimensions a formula evaluator for Excel has to carry: value kinds and their coercions, blank vs
empty text vs zero, missing argument vs blank cell, reference vs value parameters, implicit
intersection and lifting by formula form, the array-of-references cases, error precedence,
volatility, draw identity, evaluation of names per use, LAMBDA scope and recursion limits, the
near-zero adjustment of a final `+`/`-`, boxed arrays, and whatever we have missed. For each: is it
represented explicitly in our code (name it), implicitly (where), or not at all?

**D. Parallel safety and transcodability.** What in the evaluator is already safe to run on many
threads over independent cells, what is not (shared mutable services, `RefCell`, `Rc`, thread-bound
callables), and what the smallest set of changes is. For trace compilation: which instructions are
pure functions of their operands, which read the workbook, which have effects, which can suspend;
whether the instruction set is a good basis for an IR, and how you would represent lazy arguments,
LAMBDA closures, name bodies and suspension points in a straight-line trace.

**E. The runtime dependency overlay** (appendix J.3 to J.5). The engine's graph is the union of
structural edges (every reference the plan holds, including untaken branches and LAMBDA bodies)
and runtime-observed edges (what the last evaluation read beyond those). Answer: does the evaluator
report enough to keep that overlay exact (cell and area footprints, defined names, spill areas,
reads made by UDF/XLL/VBA callbacks and RTD, reads inside LAMBDA and LET and inside lazy branches
not taken)? Is the overlay's granularity (one edge per distinct area read) right for smart
recalculation and for parallel scheduling? INDIRECT and OFFSET are declared volatile in the
catalogue, as Excel documents them (appendix J.7 has the detail, including a stale comment that
says otherwise); where is that volatility observable (UDF counters in the same formula, dependents'
call counts) and should it be an explicit named policy instead of a flag? How should first-time
discovery of a dynamic edge (a wait, a retry) relate to the steady state (known dynamic edges fed
into the next order)?

**F. Specific questions.** Answer each briefly, with reasoning:

- Q1. The operand stack is cloned before every `Call`, `Spill`, `LazyCall` and `Invoke`
  instruction so that the instruction can be retried after a read of a not-yet-calculated cell
  (D.4). Another agent is measuring Excel's requeue behaviour and benchmarking alternatives
  (`docs/architecture/EVALUATOR_SUSPENSION.md`, in progress). Do not redo that experiment, but
  say what the right *model* is: retry the instruction, suspend inside the read, or make
  admission never mutate the stack so that no copy is needed.
- Q2. The lazy-call protocol (E.3) re-enters a lazy kernel with `#CALC!` standing in for the
  arguments it has not yet asked for, records the first missing index, evaluates that block, and
  re-enters. The kernel may run up to `argc + 1` times per call. Is this a sound model of IF,
  IFS, SWITCH, CHOOSE, IFERROR and IFNA, and is there a cleaner one that stays resumable?
- Q3. Argument admission (reference to one cell becomes its value; a range becomes an array,
  the intersected cell, or a refusal, by `Admission`) is decided at compile time per call site
  from the formula's form and the call's place (F). Is the four-kind `Place` lattice and the
  catalogue's `LegacyPlace` enumeration (twenty-one variants) the right factoring, or should the
  legacy rules be expressed as a small table of (parameter class, place) to admission with
  per-function exceptions?
- Q4. Lifting (E.2) interleaves five measured sub-rules: short arrays give `#N/A`, padding before
  or after the kernel by function, arrays of references give `#VALUE!` except for functions that
  run once per reference, held (boxed) arrays give their first element by notation and form, and
  array-valued kernel results contribute their first cell. Is there an ordering or a
  representation that makes these rules independently testable?
- Q5. Defined names are compiled into the referring formula's plan as extra blocks (G.3), so a
  name change recompiles the plan, and a name is evaluated per use. Is this the right boundary
  between the evaluator and the engine's name table, given the engine wants shared name memos
  later?
- Q6. `CalcValue` is a `CoreValue` enum plus an optional `Arc<ValueMetadata>` for callables,
  images, cube members and the "Excel overflow NaN" qualification (C.2). Equality is overridden
  so that an overflow NaN equals only another overflow NaN with the same bits. Is this a sound
  value model for transcoding, and what would you change first?
- Q7. Effects and evidence (user-call records, name-invocation records, RTD reads, fact reads,
  random draws) are collected through `Option<&mut ...>` sinks in the evaluation context (I.3).
  Is there a cleaner observer or ledger model that keeps the evaluator pure and makes the
  recorded evidence a first-class result?
- Q8. The evaluator recurses on the native stack for lazy arguments, LAMBDA calls and name bodies
  (bounded by `MAX_CALL_DEPTH = 64`), while Excel allows about 4,094 to 5,460 nested LAMBDA calls
  (modelled by a separate `RECURSION_BUDGET`). Should the evaluator become fully explicit-stack
  (heap cursors only), and what does that cost?
- Q9. What should a formula-level "purity and read-set" summary look like, computed once per
  plan at compile time, so the engine can decide early cutoff, parallel eligibility and memo
  eligibility without reading instructions? Today there are ad-hoc predicates
  (`volatile()`, `has_user_calls()`, `parallel_call_limit()`, `name_memo_eligibility()`,
  `provisional_references()`).
- Q10. Anything in the appendices that looks like a behavioural defect (not style), say so
  plainly with the file and line.

Constraints we hold to: one evaluator (no second route for tests or for the engine); typed
refusals (`NotModelled { construct, owner }`) rather than guesses, counted as failures; portable
crates build for `wasm32-unknown-unknown` with no threads, files or network; no process-global
state; no "improved" semantics, Excel's oddities are copied bit for bit. Do not propose rewriting
the engine; the engine contract is the boundary, and changes to it are in scope only where the
evaluator needs them.

---

# Appendix A: architecture summary

## A.1 Crates and layering

| Crate | Role |
|---|---|
| `calc-value` | Value model: `CalcValue`, `CoreValue`, `ReferenceLike`, `CalcArray`, `CallableValue`; grid coordinates `CellKey`, `GridArea`; generational ids `SheetId`, `NameId`, `NodeId`; `NumericMode`. No features, every type `Send + Sync`. |
| `calc-catalogue` | Declarative function catalogue data: `FunctionId`, spelling, arity, `Parameter { class, entry, legacy }`, `ArgumentDemand`, `EntryTraits`, three-D places. Compile-time constant tables. |
| `calc-func` | Kernels attached to the declarations; the call interface (`invoke`, `invoke_slice`, `PreparedArguments`, `EvalServices`, `CellReader`, `HostFacts`, `CallableInvoker`); user-function traits (`UserFunction`, `UserFunctionContext`, `UserFunctionInvocation`); RTD types; coercion and locale. Holds no formula grammar and no reference binding. |
| `calc-fml` | The formula language: lossless lexer and parser (`syntax/`), reference binding (`reference/`), compile and evaluate (`eval/`), callable table (`callable.rs`), RTD ledger (`rtd.rs`), editor assistance. Never depends on the engine. |
| `calc-engine` | One graph for the workbook (`FormulaNode`, `Prerequisite`, reverse indexes), dirty closure and ordering, `begin_recalc` / `step` / `finish`, spills, iteration, data tables, read certificates and the recalculation contract (`contract.rs`). Calls the evaluator through `calc_fml::EvalContinuation::advance` and implements its `ReferenceReader`, `ReferenceCatalog`, `HostFacts`, `RandomSource`. |

Layering is checked mechanically (`tools/xtask/architecture.json`). calc-fml and calc-func are
portable; the engine's parallel wave is `cfg(not(wasm32))`.

## A.2 The route from text to value

1. `parse_formula(text) -> SyntaxTree` (lossless, UTF-16 spans).
2. `bind_references(tree, notation, anchor, catalog) -> ReferenceSite[]`: each grid reference
   becomes an anchor-free `ReferenceOperand { sheet: SheetTarget, shape: ReferenceShape }` whose
   coordinates are `AxisRef::Absolute(n)` or `AxisRef::Relative(offset)`, so a formula and its
   copies bind to equal operands.
3. `compile(tree, ..) -> Arc<CompiledPlan>` through a caller-owned `PlanCache` that interns equal
   plans (`=SUM(A1:A3)*2` in C10 and `=SUM(A2:A4)*2` in C11 share one plan). The plan is the
   formula's normal form: blocks of `Instruction`s over reference operands and catalogue targets;
   block 0 is the formula, further blocks are lazy arguments, LAMBDA bodies and defined names'
   formulas.
4. `instantiate(plan, anchor) -> FormulaInstance`: each operand placed at the anchor as
   `Result<GridArea, PlacementError>` (a `#REF!` where it leaves the grid or names deleted cells).
5. `evaluate(EvalFrame, &mut FormulaEvaluationContext) -> Result<CalcValue, EvalFault>` or, for
   resumable work, `EvalContinuation::new(instance, scratch).advance(ctx, quantum) -> EvalStep`.
   Every operator and function runs through `calc_func::invoke`; every cell read goes through the
   caller's `ReferenceReader`.

The engine binds each formula node once (`begin_recalc`: compile, instantiate, file structural
prerequisites), orders the dirty closure, evaluates nodes in `step` through one continuation per
node, stages results, and publishes a generation in `finish`.

## A.3 Sizes

`crates/calc-fml/src/eval/run.rs` 4,147 lines (about 2,450 before tests), `compile.rs` 2,862,
`mod.rs` 1,034, `cells.rs` 631, `context.rs` 251, `legacy.rs` 243, `refops.rs` 283, `lift.rs` 55,
`closure.rs` 79, `entry.rs` 437, `callable.rs` 662, `rtd.rs` 485; `calc-value/src/value.rs` 1,090;
`calc-func/src/call.rs` 861, `catalogue.rs` 816, `udf.rs` 373, `kernel.rs` 233;
`calc-engine/src/recalc.rs` 6,384, `read.rs` 2,336, `state.rs` 1,728, `contract.rs` 1,454,
`graph.rs` 777.

---

# Appendix B: the plan and the instruction set

Source: `crates/calc-fml/src/eval/mod.rs:156-180, 586-712`.

```rust
/// A compiled formula: immutable instructions over anchor-free reference operands. Two formulas
/// with the same normal form compile to equal plans, which a PlanCache shares as one allocation.
/// Block 0 is the formula. Each further block is one argument of a lazy call, a LAMBDA body or a
/// defined name's formula. A block's instructions run in order on an operand stack, except those
/// a guard skips, and leave exactly one value.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CompiledPlan {
    blocks: Box<[Box<[Instruction]>]>,
    references: Box<[ReferenceOperand]>,
    /// Whether each reference is placed with its relative offsets wrapping around the grid's
    /// edge (instantiate_wrapped): a defined name's references.
    wrapped: Box<[bool]>,
    /// References to other workbooks, read from the link cache (dc-ylr).
    externals: Box<[ExternalOperand]>,
    form: FormulaForm,                               // Legacy | Dynamic (calc-syntax)
    name_bindings: Box<[NameBindingProof]>,
    callable_bindings: Box<[CallableBindingProof]>,
}

/// How a call admits a range of more than one cell, or an array, given to a parameter that takes
/// one value (an operator's operand included). Compile decides it from the formula's form and
/// the argument's place in the formula (the `legacy` module).
enum Admission {
    /// Run on each element (a range as the array of its values); the result is the array of
    /// results: a dynamic-array formula, or an array parameter of a legacy one.
    Lift,
    /// A range gives the cell implicit intersection picks, an array its first element: a legacy
    /// formula's single-value place on the way to its result.
    IntersectAll,
    /// A range gives the intersected cell, an array lifts: a legacy formula inside a parameter
    /// that takes a reference (SUM's).
    IntersectReferences,
    /// A range or array there is refused: a legacy place whose evaluation is not modelled.
    Refuse,
}

/// One step of a plan.
enum Instruction {
    /// An engine-neutral user function, bound once to a checked callable slot.
    UserCall { name: String, binding: Option<CallableHandle>, source: Option<UserFunctionSource>, argc: usize },
    /// Pushes the literal's value.
    Literal(Literal),
    /// Pushes an omitted argument (CalcValue::missing).
    Omitted,
    /// Pushes reference operand `.0` as the instance placed it: a reference to its area, or
    /// `#REF!` where it could not be placed.
    Reference(usize),
    /// Pushes the 3-D reference of `count` operands from `first`, one per sheet of its span.
    Reference3D { first: usize, count: usize },
    /// Pops `argc` values, calls `target` with them and pushes the result. With `retain`, the
    /// arguments as the call received them stay on the stack beneath the result (the near-zero
    /// check of a final `+`/`-`). With `synthetic`, compile inserted the call.
    Call { target: Target, argc: usize, retain: bool, synthetic: bool, admission: Admission },
    /// Calls the lazy entry `target`, whose argument `i` is block `first_block + i`. When its
    /// first argument, which decides, is an array (or a range admitted as one), every argument
    /// runs and the call lifts over the arrays.
    LazyCall { target: Target, first_block: usize, argc: usize, admission: Admission,
               /// Selected range arguments of a lifted call retain the enclosing array context.
               lifted_admission: Admission },
    /// Pops two references, pushes the range from the top-left of the two to the bottom-right
    /// (`:` between references that are not one written range).
    Range,
    /// Pops two references, pushes their intersection (the space operator).
    Intersect,
    /// Pops `.0` references, pushes the multi-area reference of all their areas (`,`).
    Union(usize),
    /// Pops a value, pushes the one value implicit intersection takes of it (`@`).
    Implicit,
    /// Pops a value; for a one-area reference pushes the intersected cell (#VALUE! when none);
    /// any other value as it is: the `@` a legacy formula writes before a reference given to
    /// TYPE or TRANSPOSE.
    ImplicitReference,
    /// Pops a one-cell reference, pushes the reference to the range spilled from it (`#`).
    Spill,
    /// Pushes local `.0` of the current frame: a LET name or a LAMBDA parameter.
    Local(u32),
    /// Pops a value into local `.0` of the current frame, forgetting the locals after it.
    Store(u32),
    /// Pushes a LAMBDA value whose body is block `block`: `params` parameters, the first
    /// `required` required, capturing the current frame's first `captured` locals.
    Lambda { block: usize, params: u32, required: u32, captured: u32 },
    /// Pops `argc` arguments and the value beneath them, a LAMBDA value, pushes the call's result.
    Invoke { argc: usize },
    /// Pushes the function value of catalogue entry `.0` (`GROUPBY(.., SUM)`, file `_xleta.SUM`).
    Function(Target),
    /// Pushes the plan's external reference `.0` (another workbook, link cache).
    External(usize),
    /// Runs block `.0`, a defined name's formula, in a frame of its own and pushes its value.
    Name(NameTarget),
    /// Pushes a copy of the value `.0` places below the top of the stack (0 is the top).
    Copy(usize),
    /// Starts the check of the formula's final infix `+`/`-`, whose call retained its operands.
    /// A result that is not a non-zero number needs no check: operands popped, result stays, the
    /// next `skip` instructions do not run.
    NearZeroGuard { skip: usize },
    /// Ends that check: pops the negations of right and left operands, the result and the two
    /// operands, pushes the result or 0 where Excel adjusts it, or refuses (NearZeroFinalSum).
    NearZeroCheck,
}

struct NameTarget { block: usize, binding: Option<BoundName>, definition_sheet: Option<SheetId> }

/// A literal operand. A number is held as its bits, so a plan is hashed and compared by the exact
/// binary64 it pushes.
enum Literal { Number(u64), Text(ExcelText), Logical(bool), Error(WorksheetErrorCode),
               Array { rows: usize, cols: usize, cells: Box<[Literal]> } }

/// A resolved catalogue entry, compared and hashed by its stable function ID.
struct Target(FunctionCallTarget<'static>);
```

Compile emits the near-zero check as (`compile.rs:2197-2222`): the root infix `+`/`-` is a
`Call { retain: true }`, followed by `NearZeroGuard { skip: 5 }`, `Copy(2)`, synthetic prefix `-`
call, `Copy(2)`, synthetic prefix `-` call, `NearZeroCheck`. The negations exist to coerce the
operands to numbers through the catalogue's own prefix `-` so that the biased exponent of the
left operand can be read.

Plan-level predicates the engine uses (`mod.rs:187-480`): `name_memo_candidates()` and
`name_memo_eligibility(block)` (walks blocks: volatile call, catalogue call without facility
classification, user call, external, callable, locals and recursion each block eligibility),
`has_user_calls()`, `parallel_call_limit(bindings)`, `only_cached_user_calls(bindings)`,
`permits_early_execution(bindings)`, `provisional_references(bindings)`,
`has_provisional_calls(table)`, `references()`, `form()`, `reads_external_links()`, `calls()`
(every target in every block, untaken branches and names included), `volatile()` (any call whose
catalogue traits say volatile), `user_calls()`, `user_call_sources()`.

`PlanCache` (`mod.rs:810-891`): `HashMap<u64, Vec<Weak<CompiledPlan>>>` keyed by the plan's hash,
weak ownership (instances, continuations and closures own their plans), four expired buckets swept
per insertion.

`FormulaInstance` (`mod.rs:893-963`): `plan: Arc<CompiledPlan>`, `anchor: FormulaAnchor`,
`areas: Box<[Result<GridArea, PlacementError>]>`, one per plan reference, placed by
`instantiate` or `instantiate_wrapped` (names wrap relative offsets around the grid edge, as Excel
reads `RC[-1]` stored from A1).

---

# Appendix C: the value model

Source: `crates/calc-value/src/value.rs`.

## C.1 Kinds

```rust
pub enum WorksheetErrorCode { Null, Div0, Value, Ref, Name, Num, NA, Busy, GettingData, Spill,
                              Calc, Field, Blocked, Connect }

/// Excel text: UTF-16 code units, which need not form valid UTF-16 (a truncation can leave a
/// dangling high surrogate, as Excel's can). Shared behind Arc<[u16]>.
pub struct ExcelText { units: Arc<[u16]> }

pub struct ArrayShape { pub rows: usize, pub cols: usize }

/// A non-empty rectangular array in row-major order; cells shared behind an Arc. A cell may
/// itself hold an array: GROUPBY and PIVOTBY keep a LAMBDA's array result in its cell.
/// A boxed array is an array taken out of such a cell (INDEX of the cell): Excel 16.0.20527 shows
/// its first element and spills nothing, takes its first element where one value is taken
/// (`=INDEX(g,1,2)*10` is 10) and the whole array where an array is taken (ROWS, SUM, VSTACK).
pub struct CalcArray { shape: ArrayShape, cells: Arc<[CalcValue]>, boxed: bool }

/// One or more grid areas of one sheet, in order (a union keeps duplicates), or a 3-D
/// reference's areas, one per sheet of its span. One area is held inline and allocates nothing.
/// A structured reference is bound to its areas before it becomes a value; a spill anchor and an
/// external workbook reference are not representable here.
pub struct ReferenceLike { areas: Areas }       // Areas::One(GridArea) | Areas::Many(Arc<[GridArea]>)

/// The worksheet-visible part of a value.
pub enum CoreValue { Number(f64), Text(ExcelText), Logical(bool), Error(WorksheetErrorCode),
                     Empty /* an empty cell */, Missing /* an omitted argument */,
                     Array(CalcArray), Reference(ReferenceLike) }

/// A callable value (a LAMBDA or a function value): arity, a summary for display, and the shared
/// handle of its identity. Equal when arities are equal and they share one handle (Arc::ptr_eq).
pub struct CallableValue { arity: CallableArityShape, summary: Arc<str>, handle: Arc<dyn OpaqueCallable> }
pub trait OpaqueCallable: Debug + Send + Sync + 'static { fn as_any(&self) -> &dyn Any; }
```

## C.2 `CalcValue`

```rust
pub struct CalcValue { core: CoreValue, metadata: Option<Arc<ValueMetadata>> }

// Reuse the optional metadata pointer rather than growing every operand/frame.
enum ValueMetadata { Callable(CallableValue), ExcelOverflow, Image(ImageValue), Cube(CubeValue) }

impl PartialEq for CalcValue {
    fn eq(&self, other: &Self) -> bool {
        if self.is_excel_overflow() || other.is_excel_overflow() {
            self.is_excel_overflow() == other.is_excel_overflow()
                && self.as_number().map(f64::to_bits) == other.as_number().map(f64::to_bits)
        } else {
            self.core == other.core
                && self.callable_value() == other.callable_value()
                && self.image_value() == other.image_value()
        }
    }
}
```

- A callable's core is `Error(Calc)` (`#CALC!`, what Excel shows for a LAMBDA in a cell); an
  image's core is `Error(Value)`; a cube member's core is its display text.
- `excel_overflow_number(nan)`: the measured rounding conversion past DBL_MAX (ROUND, ROUNDDOWN,
  TRUNC produce an exponent field of 2047 with the significand intact; dc-j9k.19). Only that
  conversion may originate the qualification; copying preserves it; an ordinary NaN with the same
  bits never gets it.
- `CoreRef<'a>` is a by-value view (`core_ref()`) that callers match on so the storage can change
  (planned compaction, dc-dxm step 2). Estimated size today: about 56 bytes per value (a 48-byte
  `CoreValue` plus the metadata pointer; estimated from the layout, not measured).
- `Missing` marks an omitted argument and is never a function's result (`admit_return` refuses it:
  `calc-func/src/call.rs:677-683`); `Empty` is a blank cell and may be a result.

## C.3 Where the value kinds meet the formula result

`run.rs:355-370, 498-521` (the end of `evaluate_slice`): a legacy formula's result that is a
reference is intersected at the anchor and the cell read (`#VALUE!` when the anchor's row or
column lies outside), an array gives its first element; a dynamic formula's reference result is
read as the array of its values with blank positions displayed as 0 (`zero_array_blanks`); any
result that is `Empty` becomes the number 0 (Excel shows 0 for `=A1` over a blank A1, and for
IFERROR's blank fallback, measured dc-ibp2). `with_reference_result()` on the context skips all of
this and returns the expression's reference or value (name inspection).

---

# Appendix D: the core loop, suspension and retry

Source: `crates/calc-fml/src/eval/run.rs`.

## D.1 Entry points

```rust
pub fn evaluate(frame: EvalFrame<'_>, services: &mut FormulaEvaluationContext<'_>)
    -> Result<CalcValue, EvalFault>;           // one shot, quantum = usize::MAX (run.rs:374-393)

pub struct EvalContinuation { instance: FormulaInstance, scratch: EvalScratch, cursor: BlockCursor, finished: bool }
impl EvalContinuation {
    pub fn new(instance: &FormulaInstance, scratch: EvalScratch) -> Self;
    pub fn advance(&mut self, services: &mut FormulaEvaluationContext<'_>, quantum: usize) -> EvalStep;
    pub fn waiting_input(&self) -> Option<CellKey>;        // the input a retained read could not consume
    pub fn pending_external(&self) -> Option<(u64, u64)>;  // (invocation id, request) of a parked async call
    pub fn accept_external(&mut self, invocation: u64, request: u64, value: CalcValue) -> bool;
    pub fn progress(&self) -> ContinuationProgress;        // { next_instruction, completed_elements }
    pub fn into_scratch(self) -> EvalScratch;
}
pub enum EvalStep { Suspended(ContinuationProgress), Complete(Result<CalcValue, EvalFault>) }

pub enum EvalFault {
    Function(FunctionFault),                 // calc-func refused: admission, arity, not modelled, Exact
    MissingInput { cell: CellKey },          // the reader reported the cell as missing
    ReadContract(ReadContractError),         // the reader's runs did not cover an area exactly
    AreaAsValue { area: GridArea },          // legacy single-value place whose Excel behaviour is unmeasured
    NearZeroFinalSum { result: f64 },        // final +/- adjustment rule cannot be applied (left operand not a number)
    NotModelled { construct: &'static str, owner: &'static str },
}
```

`EvalScratch` (`mod.rs:1087-1104`) is caller-owned storage reused across evaluations: `stack:
Vec<CalcValue>`, `calls: Vec<FunctionCallScratch>`, `locals: Vec<CalcValue>` (the locals of every
frame, each frame above its caller's), `arguments: Vec<CalcValue>`.

## D.2 Cursors (the retained state of one activation)

```rust
struct BlockCursor {
    at: usize,                                  // next instruction of this block
    base: Option<usize>,                        // operand-stack height when the block started
    pending: Option<Box<PendingCall>>,          // an eager resumable call in progress
    pending_user: Option<Box<PendingUserCall>>, // a user-function activation in progress
    child: Option<Box<PendingBlock>>,           // a LAMBDA body or name body in progress
    lazy: Option<Box<PendingLazy>>,             // a lazy call with its evaluated arguments so far
    result: Option<CalcValue>,                  // the block's value once it completed
    waiting_input: Option<CellKey>,
}
struct PendingBlock { block: usize, frame: usize /* locals base */, cost: usize /* recursion budget */,
                      cursor: BlockCursor, name: Option<Arc<ActiveName>> }
struct PendingLazy { values: Vec<Option<CalcValue>>, wanted: Option<usize>, cursor: BlockCursor }
struct PendingCall { target: Target, arguments: Vec<CalcValue>, scratch: FunctionCallScratch,
                     child: Option<Box<PendingBlock>>, synthetic: bool, retain: bool, start: usize }
struct PendingUserCall { callable: CallableHandle, cached: bool, provisional_arguments: Vec<GridArea>,
                         reevaluation: Option<UserReevaluationReason>,
                         activation: Box<dyn UserFunctionInvocation>, frame: UserInvocationFrame,
                         record: Option<usize>, external: Option<(u64, Option<CalcValue>)> }
```

The run itself (`struct Run`, `run.rs:542-574`) is rebuilt per `advance` from the context: the
instance, the three scratch vectors, `base` (locals base of the current frame), `depth`, `budget`,
the `SharedReader`, the optional `SharedRandom`, the catalogue, the callable table, the
observation reader, the effects sink, diagnostics sink and the observer.

## D.3 `evaluate_slice` (`run.rs:395-540`, trimmed)

```rust
fn evaluate_slice(frame, services, cursor: &mut BlockCursor, quantum) -> EvalStep {
    if cursor.base.is_none() { *effects = UserFunctionEffects::default(); }   // fresh activation
    if services.request.policy is not DependencyWithinCommand | DiagnosticSnapshot {
        return Complete(Err(NotModelled { "an observation or registered callback without an invocation read view" }))
    }
    let reader = SharedReader::new(services.reader, host, anchor cell, environment, services.request);
    let observation_reader = services.observation_reader.map(|r| SharedReader::new(r, .., CachedMacroObservation));
    let mut run = Run { .. };
    let value = match run.block_slice(0, cursor, &mut remaining) {
        Ok(Some(value)) => Ok(value), Err(fault) => Err(fault),
        Ok(None) => return Suspended(progress),
    };
    let outcome = value.and_then(|value| /* cell-result rules of C.3 */);
    if let Err(MissingInput { cell }) = &outcome && reader.pending_producer(*cell) {
        cursor.waiting_input = Some(*cell);
        return Suspended(progress);
    }
    observer.returned(EvaluationReport { anchor, numeric, outcome });
    Complete(outcome)
}
```

## D.4 `block_slice` (`run.rs:902-1185`, trimmed to the control flow)

```rust
fn block_slice(&mut self, index, cursor, remaining) -> Result<Option<CalcValue>, EvalFault> {
    cursor.waiting_input = None;
    if let Some(value) = &cursor.result { return Ok(Some(value.clone())); }
    let base = *cursor.base.get_or_insert(self.stack.len());
    let block = &self.instance.plan.blocks[index];
    let mut at = cursor.at;
    while let Some(instruction) = block.get(at) {
        if *remaining == 0 { cursor.at = at; return Ok(None); }
        let resuming = cursor.pending.is_some() || cursor.pending_user.is_some()
                    || cursor.child.is_some() || cursor.lazy.is_some();
        if !resuming && *remaining != usize::MAX { *remaining -= 1; }
        at += 1;
        // Only a pure read instruction may be retried. Its operands stay intact;
        // completed calls and child frames remain behind this instruction cursor.
        let operands = matches!(instruction, Call{..} | Spill | LazyCall{..} | Invoke{..})
            .then(|| self.stack.clone());                                     // run.rs:931-938
        let effects_before = effects.calls;
        let step = (|| { match instruction {
            Literal(l)        => push(l.value()),
            Omitted           => push(missing),
            Reference(i)      => push(instance.reference(i)),            // reference value or #REF!
            Reference3D{..}   => push(instance.reference_3d(..)),
            UserCall{..}      => if !self.user_call_slice(.., &mut cursor.pending_user, quantum)? { cursor.at = at-1; return Ok(false) }
                                 if *remaining != usize::MAX { *remaining = 0 }            // a user call ends the quantum
            Call{..}          => match self.call_slice(.., &mut cursor.pending, quantum) {
                                   Ok(false) => { cursor.at = at-1; return Ok(false) }     // resumable kernel yielded
                                   Ok(true)  => if resumable && *remaining != MAX { *remaining = 0 }
                                   Err(f)    => { self.stack.truncate(base); return Err(f) } }
            LazyCall{..}      => if !self.lazy_call_slice(.., &mut cursor.lazy, remaining)? { cursor.at = at-1; return Ok(false) }
            Copy(d)           => push(stack[len-1-d].clone()),
            NearZeroGuard{skip} => if top is not a non-zero number { pop result; truncate(top-2); push result; at += skip }
            NearZeroCheck     => self.near_zero_check()?,
            Range | Intersect | Union(n) => refops on popped references,
            Implicit          => push(refops::implicit(pop, anchor)),
            ImplicitReference => one-area reference intersected, else unchanged,
            Spill             => self.spill()?,                            // reader.spill_extent
            Local(slot)       => push(locals[base + slot].clone()),
            Store(slot)       => { locals.truncate(base+slot); locals.push(pop) }
            Lambda{..}        => push(Closure { plan: Arc::clone(plan), block, params, required,
                                                captured: locals[base..base+captured].into() }.into_value()),
            Invoke{argc}      => if !self.invoke_slice(argc, &mut cursor.child, remaining)? { cursor.at = at-1; return Ok(false) }
            Function(t)       => push(FunctionValue{t}.into_value()),
            Name(target)      => { if cursor.child.is_none() { check_depth; start_name; cursor.child = PendingBlock{..} }
                                   if !self.child_slice(&mut cursor.child, remaining, false)? { cursor.at = at-1; return Ok(false) } }
            External(i)       => self.external(i)?,
        }?; Ok(true) })();
        if matches!(step, Ok(false)) { return Ok(None); }
        if let Err(fault) = step {
            if let MissingInput { cell } = fault && self.reader.pending_producer(cell)
               && effects.calls == effects_before && let Some(operands) = operands {
                *self.stack = operands; cursor.waiting_input = Some(cell); cursor.at = at - 1;
                return Ok(None);                                                // run.rs:1160-1174
            }
            self.stack.truncate(base); return Err(fault);
        }
    }
    let value = self.stack.pop().expect("a block leaves one value");
    cursor.at = at; cursor.result = Some(value.clone());
    Ok(Some(value))
}
```

Points to note:

- Suspension has three causes, all surfacing as `EvalStep::Suspended`: the quantum ran out (the
  engine re-queues the node at the front of its ready queue, `recalc.rs:2011`), a read of a cell
  whose producer is scheduled but not yet evaluated (`cursor.waiting_input` set, the engine makes
  the node wait on that producer, `recalc.rs:5905-5920, 2105-2130`), or a user function parked
  on an async request (`PendingUserCall.external`, `recalc.rs:2012-2063`).
- The retry after `MissingInput` is per instruction: everything before the instruction stays
  completed (its calls are not re-run, its draws not redrawn); the instruction's own operands are
  restored from the clone. The clone happens on every `Call`, `Spill`, `LazyCall` and `Invoke`
  even though only reads inside admission or inside a kernel can produce `MissingInput`. Another
  agent is measuring Excel's requeue behaviour and the cost of alternatives
  (`docs/architecture/EVALUATOR_SUSPENSION.md`, in progress). Treat Q1 as a modelling question.
- `effects.calls == effects_before` blocks the retry when a user function ran inside the
  instruction (its effects must not be replayed); the engine then refuses the formula
  ("an effectful callable requiring a dependency continuation", `recalc.rs:5891-5899`).
- A user call or a resumable kernel ends the quantum (`*remaining = 0`), so each such call is the
  last thing in its slice.
- Rust recursion: `block_slice` recurses through `lazy_call_slice`, `child_slice` and
  `invoke_slice` for every nesting level; `MAX_CALL_DEPTH = 64` bounds LAMBDA and name nesting
  (`check_depth`); lazy-argument nesting is bounded by the parser's 64-level limit.

## D.5 Services shared through `RefCell`

`SharedReader` (`cells.rs:47-198`) wraps the caller's `&mut dyn ReferenceReader` and `Option<&mut
dyn HostFacts>` in `RefCell`s, with a `Cell<Option<ReadRefusal>>` that records why a kernel's
reference read failed (missing cell, denial, contract error) so that `function_fault` can turn
calc-func's `ReferenceUnreadable` into the right `EvalFault`. `SharedRandom` (`run.rs:49-60`)
does the same for the random source. The reason: a lazy call's arguments run while its kernel
holds the services, and a kernel calling LAMBDA helpers (MAP, REDUCE) re-enters the evaluator
through `Callables` (`run.rs:2348-2417`) while it holds `EvalServices`. Each kernel call builds a
fresh `EvalServices` over `ReaderCells` and `ReaderHost` adapters (`call_services`, `run.rs:64-80`).

---

# Appendix E: calls, admission, lifting and lazy evaluation

## E.1 The calc-func call contract (`crates/calc-func/src/call.rs`, `catalogue.rs`)

```rust
/// The arguments of one call. The caller produces each value when the call interface asks for
/// it, so a lazy entry's held-back arguments are never evaluated by the interface.
pub trait PreparedArguments { fn count(&self) -> usize; fn argument(&mut self, index: usize) -> CalcValue; }

/// Typed read access to the cells a reference argument names. A kernel reads a reference only
/// through it and never reaches workbook state.
pub trait CellReader {
    /// Visits the cells of `area` that hold a value, row-major, until Break. A cell it does not
    /// visit is blank. `Unreadable` when it cannot supply the values of `area`.
    fn read_area(&mut self, area: GridArea, visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>) -> Result<(), Unreadable>;
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions>;
}
pub trait RandomSource { fn next_unit(&mut self) -> f64; }
pub trait ReferenceText { fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference; }  // INDIRECT
pub enum TextReference { Area(GridArea), Invalid /* #REF! */, NotModelled }
pub trait HostFacts { caller, is_formula, formula_text, sheet_index, sheet_named, sheet_name,
                      sheet_count, workbook_name, workbook_facts, is_external_sheet, pivot_data_cell }
pub trait CallableInvoker {           // MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY call LAMBDAs through it
    fn call(&mut self, callable: &CalcValue, arguments: &[CalcValue]) -> Result<CalcValue, CallAbandoned>;
    fn call_slice(&mut self, callable: &CalcValue, arguments: &[CalcValue]) -> Result<Option<CalcValue>, CallAbandoned>; // None: yielded
}

pub struct EvalServices<'s> { numeric: &'s NumericMode, cells: Option<&'s mut dyn CellReader>,
    random: Option<&'s mut dyn RandomSource>, reference_text: Option<&'s mut dyn ReferenceText>,
    environment: Option<&'s CalcEnvironment>, host: Option<&'s mut dyn HostFacts>,
    callables: Option<&'s mut dyn CallableInvoker> }

pub(crate) enum Kernel {
    Eager(EagerKernel),                              // fn(&[CalcValue], &mut EvalServices) -> Result<CalcValue, FunctionFault>
    Resumable { run: EagerKernel, start: StartKernel }, // start gives a Box<dyn KernelTask> advanced by `elements`
    Lazy(LazyKernel),                                // fn(&mut LazyArguments, &mut EvalServices) -> ...; asks for arguments itself
    Binding,                                         // LET, LAMBDA: lowered by calc-fml
    NotHeld { owner: &'static str },                 // typed refusal naming the work package
    Deferred { class: Deferral },
}

pub enum FunctionFault { NotHeld{..}, Deferred{..}, Binding{..}, ContinuationTarget{..}, ArityMismatch{..},
    ReferenceArgument{ id, index } /* a value parameter received a reference: the caller dereferences */,
    ReferenceRequired{ id, index }, ReferenceUnreadable{ id, index }, NotImplemented{ id, argument },
    ExactUnavailable{ id }, MissingReturned{ id } }

pub fn invoke(target, arguments: &mut dyn PreparedArguments, services, scratch) -> Result<CalcValue, FunctionFault>;
// arity checked before any argument is asked for; eager: arguments asked once each in order and
// admitted against ParameterClass (Value refuses a reference; Reference refuses a non-reference,
// except that #REF! given to a Reference parameter is the call's result: call.rs:641-648);
// lazy: the kernel asks for what it selects, each admitted when asked for.
pub fn invoke_slice(.., elements: usize) -> Result<Option<CalcValue>, FunctionFault>;  // resumable kernels
```

Catalogue declarations (`calc-catalogue/src/declarations.rs`):

```rust
pub enum ParameterClass { Value, ValueOrReference, Reference }
pub enum ArgumentDemand { Eager, BranchOnCondition /* IF, CHOOSE: arg 0 decides */, FallbackOnError /* IFERROR, IFNA */ }
pub struct Parameter { name, class: ParameterClass, entry: EntryClass, legacy: LegacyPlace }
pub struct EntryTraits {
    intersect_legacy_result: bool, reject_multi_cell_reference: bool, padding_precedes_call: bool,
    can_skip_nested_subtotals: bool, is_subtotal: bool, lift_dynamic_references: bool,
    take_reference_array: bool, lift_computed_array: bool, paired_result: PairedResult,
    whole_result_policy: WholeResultPolicy, zero_array_blanks: bool, zero_lifted_blank: bool,
    zero_dynamic_lifted_blank: bool, volatile: bool, flush_subnormal_inputs: bool,
    retain_overflow_number: bool, retain_subnormal_output: bool, strict_number_arguments: bool,
    three_d: ThreeD,
}
pub enum LegacyPlace { Default, IntersectReference, IntersectReferenceAlways, SingleValue, ArrayAlways,
    ArrayAlwaysRepeated, ArrayAlwaysLiftReferences, ArrayAlwaysLiftReferencesRepeated, ArrayAtRoot,
    ArrayAtRootRepeated, AggregateAtRoot, AggregateAtRootRepeated, AggregateUnderArray,
    AggregateUnderArrayRepeated, AggregateAtRootTwice, AggregateRootArrayNested,
    AggregateRootArrayNestedLiftReferences, LiftReferences, AggregateIntersectReference,
    AggregateAlwaysIntersectReference }
```

Kernels stop with `Exit::Error(code)` (a computed worksheet error, which is a value) or
`Exit::Fault(FunctionFault)` (not a value; the caller reports the formula as not calculated).
Every arithmetic site goes through `Site::add/subtract/multiply/divide` (x87 double rounding as
Excel computes the four operations) and `Site::number` (non-finite is `#NUM!`, `-0` is `+0`, a
subnormal result is refused as not modelled). `NumericMode::Exact` is refused at every site today
(`kernel.rs`).

## E.2 Eager calls: `call_slice` and `lifted` (`run.rs:1280-1388, 1562-1664`)

```rust
fn call_slice(&mut self, target, argc, retain, synthetic, admission, pending, quantum) -> Result<bool, EvalFault> {
    if let Some(call) = pending.take() { return self.advance_call(call, pending, quantum); }
    let start = self.stack.len() - argc;
    for index in 0..argc {
        let mut value = take(&mut self.stack[start + index]);
        // dc-b1l: a legacy formula gives a function that does not run once per reference of an
        // array of references its first reference.
        if form == Legacy && reference_array(&value) && !lifts_over_references(target, form) { value = first element }
        // IFS/SWITCH paired results: a computed Empty is told apart from a raw blank reference before admission.
        if paired_result(target, index) && value.is_empty() { computed_blank_results.push(index) }
        let admitted = self.admit(target, index, value, admission)?;           // may read cells
        lifted |= is_lifted(target, form, index, &admitted);
        self.stack[start + index] = admitted;                                 // stack mutated in place
    }
    // an array of references at a non-Value parameter of a function whose handling is unmeasured is refused
    arguments = if retain { copy of stack[start..] } else { drain(stack[start..]) };
    let whole_result = (lifted || computed blanks) && target.selects_whole_result(&arguments, numeric);  // IFS/SWITCH scalar selection
    if whole_result { lifted = false; computed blanks become 0 }
    if !lifted && target.is_resumable() { return self.advance_call(PendingCall{..}, pending, quantum) }
    let value = if lifted { self.lifted(target, &arguments, synthetic) } else { self.eager(target, &arguments, synthetic) }?;
    if !retain { self.stack.truncate(start) } ; self.stack.push(value); Ok(true)
}

/// A parameter that takes a value receives a reference to one cell as the cell's value, and a
/// larger range or an array as `admission` says. Any other parameter receives the argument as it is.
fn admit(&self, target, index, value, admission) -> Result<CalcValue, EvalFault> {
    if target.parameter_class(index) != Some(ParameterClass::Value) { return Ok(value) }
    if target.traits().reject_multi_cell_reference && value is a one-area reference of > 1 cell { return Ok(#VALUE!) } // MROUND, QUOTIENT
    self.admit_value(value, admission)
}
fn admit_value(&self, value, admission) -> Result<CalcValue, EvalFault> {
    if boxed array { return first element }                              // dc-h9u
    match value {
        Reference(one cell)        => self.reader.value_of(reference),   // read; Empty for a blank cell
        Reference(range) => match admission {
            Lift => self.reader.values_of(reference),                    // array of the values; multi-area -> #VALUE!
            IntersectAll | IntersectReferences => intersect at anchor, read the cell, or #VALUE!,
            Refuse => Err(AreaAsValue { area }) },
        Array => match admission { Lift | IntersectReferences => value, IntersectAll => first element,
                                   Refuse => Err(NotModelled("an array where a legacy formula takes one value")) },
        other => other,
    }
}
```

Lifting (`lift.rs`, `run.rs:1562-1664, 2199-2300`): the result shape is the largest row count by
the largest column count over the lifted arrays; a one-row array repeats down, a one-column array
across; any other array too small for a cell leaves `#N/A` there. Per element, the rules are:

1. An element that is a *reference* (an array of references, from OFFSET or INDIRECT lifted over
   an array) at a `Value` parameter is `#VALUE!`, except for functions that run once per
   reference (`lifts_over_references`: catalogue `LegacyPlace::*LiftReferences` in a legacy
   formula, `traits.lift_dynamic_references` in a dynamic one), which read its value.
2. An element that is itself an *array* (a GROUPBY result's cell, "held" or boxed array): a
   function, or an operator in a legacy formula, takes its first element; an operator in a dynamic
   formula gives `#VALUE!` (`held_array_element`).
3. A *missing coordinate* is `#N/A` in that argument. If `padding_precedes_call` (operators, and
   named functions that declare it) the `#N/A` is the cell's result without running the kernel,
   except that for named calls a formed error in an earlier argument takes precedence.
4. Otherwise the kernel runs once on the element arguments (`self.eager`); a reference result over
   references is read as its value (one cell) or `#VALUE!`; an array-valued result contributes its
   first cell; a `Missing` result becomes `Empty` (`lifted_element`).

`is_lifted(target, form, index, value)`: a `Value` parameter (or a `lift_computed_array` entry
such as COUNTBLANK) given a plain array, or an array of references given to a function that
lifts over references.

## E.3 Lazy calls: `lazy_call_slice` (`run.rs:1700-1820`)

```rust
fn lazy_call_slice(&mut self, target, first_block, argc, admission, lifted_admission, pending, remaining) -> Result<bool, EvalFault> {
    let call = pending.get_or_insert(PendingLazy { values: vec![None; argc], wanted: Some(0), cursor: default });
    loop {
        if let Some(index) = call.wanted {                        // evaluate the wanted argument's block
            match self.block_slice(first_block + index, &mut call.cursor, remaining)
                      .and_then(|v| v.map(|v| self.admit(target, index, v, admission)).transpose()) {
                Ok(Some(value)) => { call.values[index] = Some(value); call.wanted = None; call.cursor = default }
                Ok(None) => return Ok(false),                         // suspended inside the argument
                Err(fault) => { show the call unless lifted; return Err(fault) }
            }
        }
        let lifted = is_lifted(target, form, 0, values[0]) || (FallbackOnError && values[1] is lifted);
        if lifted {                                                   // an array decides: every argument runs
            if let Some(index) = first None in values { call.wanted = Some(index); continue }
            let values = values.map(|i, v| if i == 0 { v } else { self.lift_operand(v, lifted_admission) })?;
            push(self.lifted_lazy(target, &values)?); *pending = None; return Ok(true)
        }
        // The catalogue's lazy kernels only select arguments. Re-enter that pure selection with
        // retained values; never re-enter an argument or its effects.
        let mut arguments = PendingArguments { values: &call.values, wanted: None };
        let result = invoke(target, &mut arguments, &mut services, &mut scratch);
        if let Some(index) = arguments.wanted { call.wanted = Some(index); continue }   // result discarded
        self.show_call(target, false);
        let value = zero_if_array_blanks(target, result?);          // IF returning a whole computed array
        push(value); *pending = None; return Ok(true)
    }
}
/// A lazy kernel can select a missing argument without entering its block. The enclosing cursor
/// then runs that block and retains its result across every wait.
struct PendingArguments<'a> { values: &'a [Option<CalcValue>], wanted: Option<usize> }
impl PreparedArguments for PendingArguments<'_> {
    fn argument(&mut self, index) -> CalcValue {
        match &self.values[index] { Some(v) => v.clone(),
            None => { self.wanted.get_or_insert(index); CalcValue::error(ABANDONED /* #CALC! */) } }
    }
}
```

`lifted_lazy` (`run.rs:1845-1891`) runs the lazy kernel once per element with every argument
evaluated (as Excel evaluates both branches of an array IF), pads missing coordinates with `#N/A`
in that argument, and turns a selected blank into 0 for IFERROR/IFNA, `zero_lifted_blank` entries
and, in a dynamic formula, `zero_dynamic_lifted_blank` entries.

## E.4 Kernels calling LAMBDAs (`run.rs:1508-1556, 2348-2417`)

When any argument is a callable (or an array holding one), `eager_with_callables` gives the kernel
a `Callables` invoker whose `call_slice` prepares a closure frame (`prepare_closure`) and runs the
body as a child block of the pending call (`PendingCall.child`), so a MAP over a LAMBDA that reads
a not-yet-calculated cell suspends and resumes at the same item. A kernel that uses the
non-resumable `call()` and meets a wait is refused ("a helper callback without a retained kernel
continuation"). The first fault in a helper stops every further call and becomes the evaluation's
fault. A LAMBDA's reference result reaches the helper as its value (one cell) or array of values.

---

# Appendix F: the legacy form, implicit intersection and the near-zero check

## F.1 Forms and places (`calc-fml/src/eval/legacy.rs`, `compile.rs:569-577`)

`FormulaForm::{Legacy, Dynamic}` is the formula's dialect (a file's plain `<f>` is legacy, an
array or dynamic-array formula and `Range.Formula2` entry are dynamic). In a dynamic formula
every single-value place admits a range as the array of its values (`Admission::Lift`). In a
legacy formula the admission depends on the place the argument holds, computed at compile time by
walking down from the root:

```rust
pub(super) enum Place {
    Value,            // the root and every single-value parameter or operand on the way down: intersect a range, first element of an array
    Aggregate,        // below a reference parameter that does not spill (SUM's, MATCH's lookup array): intersect a range, lift an array
    Array,            // below an array parameter (SUMPRODUCT's, LOOKUP's vectors, INDEX's array, ROWS's): both lift
    ArrayAggregate,   // the arguments of an aggregate function called at an array place (dc-b1l)
    Unknown,          // unmeasured: a range or array meeting a single-value place is refused (AreaAsValue)
}
impl Place {
    fn admission(self) -> Admission { Value => IntersectAll, Aggregate => IntersectReferences, Array|ArrayAggregate => Lift, Unknown => Refuse }
    fn operand(self) -> Place { self }
    fn argument(self, spelling, index, class: Option<ParameterClass>, lazy: bool) -> Place { /* legacy.rs:120-186: a decision table over LegacyPlace */ }
}
```

The measurements behind it (Excel 16.0 through COM, 2026-10-23, `Range.Formula` entry read back
through `Range.Formula2`): `=A1:A3*1` becomes `=@A1:A3*1` (2 in row 2), `=SUM(A1:A3*2)` becomes
`=SUM(@A1:A3*2)` (4), `=SUM({1,2}*2)` stays (6), `=SUMPRODUCT(A1:A3*2)` stays (12),
`=SUM(IF(TRUE,A1:A3*1))` becomes `=SUM(IF(TRUE,@A1:A3*1))` (2) but `=SUM(IF(TRUE,A1:A3))` stays
(6), `=INDEX(A1:A3*2,2)` gets `@` before INDEX at the root only, and so on (the table in
`legacy.rs:1-62`). Compile also emits `Instruction::Implicit` after ROW/COLUMN/FILTERXML-class
calls in a legacy formula outside an array place (`traits.intersect_legacy_result`) and
`Instruction::ImplicitReference` before arguments whose `LegacyPlace` is an intersect-reference
kind (TYPE, TRANSPOSE: `compile.rs:1175-1232`). LET's inner place is `Unknown` ("where a legacy
formula's LET intersects is not measured: refused there", `compile.rs:1540-1541`) and so is a
LAMBDA body's.

## F.2 Implicit intersection (`refops.rs:118-153`)

`@` on a reference gives the cell in the formula's row (when the area spans more than one row) and
column (when it spans more than one column), `#VALUE!` when the formula's row or column lies
outside it or the reference has more than one area; an array gives its first element; any other
value is itself. Measured 2026-10-23: `=@S1!C1:E10` in D5 on another sheet gives S1!D5, in F5 and
D11 `#VALUE!`; `=@{7,8}` is 7.

## F.3 The near-zero adjustment of a final `+`/`-` (`run.rs:113-128, 2179-2194`)

Excel (KB 78113) adjusts the result of a formula's final addition or subtraction to 0 when it is
tiny relative to the operands. Measured in Excel 16.0.20430 through COM on 2026-10-25 (dc-b1l):
a non-zero result is 0 when the biased binary64 exponent of the operation's *left* operand
exceeds the result's by 50 or more; `(1+k*2^-52)-1` is 0 for k up to 7 and `k*2^-52` from 8;
`=1E15+1-1E15` is 1 (49 apart); only the root operation is adjusted (not under parentheses, a
prefix sign, IF's arguments or LET's result). The evaluator reads the left operand's exponent
from its negation (the synthetic prefix `-` call coerces text and logicals as arithmetic would);
when the negation is not a number and the result is non-zero, the formula is refused with
`EvalFault::NearZeroFinalSum` rather than published.

---

# Appendix G: LET, LAMBDA and defined names

## G.1 Lowering (`compile.rs:1527-1631`)

`LET(n1, v1, ..., body)`: each value is lowered into the current block followed by
`Store(slot)` (`Task::Declare`), the name is brought into scope as that slot, the body is lowered,
then the names go out of scope. Locals are per frame: `Local(slot)` reads `locals[base + slot]`.
LET's result is not a final-sum tail.

`LAMBDA(p1, [p2], body)`: emits `Lambda { block, params, required, captured: depth() }` and lowers
the body into a new block with the parameters in scope *after* the captured names (lexical scope:
the body sees the LET names and outer parameters visible where the LAMBDA was written). A call
`Invoke { argc }` pops the arguments and the callee.

## G.2 Closures and calls (`closure.rs`, `run.rs:2030-2119`)

```rust
pub(super) struct Closure { plan: Arc<CompiledPlan>, block: usize, params: usize, required: usize, captured: Box<[CalcValue]> }
// into_value(): CalcValue::callable(CallableValue::new(arity(required..=params), "LAMBDA", Arc::new(self)))
pub(super) struct FunctionValue { target: Target }   // SUM named where a LAMBDA is taken; called eagerly with the given arguments
```

`prepare_closure(callee, arguments)`: a function value calls its target with `Admission::Lift`; a
non-callable callee is `#REF!` (a reference) or `#VALUE!`; a closure of *another plan* is refused
("a LAMBDA value made by another formula"); wrong arity is `#VALUE!`; the recursion budget
(`RECURSION_BUDGET = 16_383` units, `CALL_COST = 3` per call plus `CALLABLE_ARGUMENT_COST = 1`
per LAMBDA-valued argument, measured by OxFml: a named LAMBDA calling itself reaches 5,460 levels,
a LET-bound one passed itself 4,094) gives `#NUM!` when exceeded; `check_depth` refuses beyond
`MAX_CALL_DEPTH = 64` native levels as not modelled. The frame is then `locals.extend(captured);
locals.extend(arguments); missing for the rest`, and the body block runs as a `PendingBlock`
(`child_slice`), which restores `base`, `depth`, `budget` and truncates the locals afterwards.

## G.3 Defined names (`compile.rs:1670-1831`, `run.rs:1135-1152, 1927-2026`)

A defined name used by a formula is compiled into the formula's plan as its own block
(`name_block`): the name's text is parsed, bound at A1 of its definition sheet (or the anchor's),
and lowered with `in_name = true` so its references are marked `wrapped` (relative offsets wrap
around the grid edge when placed at the formula's anchor, as Excel reads a name stored from A1).
The same name and definition under the same parent and sheet compile once per plan
(`Lowering.names`). Nesting is bounded by `MAX_NAME_NESTING = 32`. A plan records
`NameBindingProof { resolved: Option<BoundName>, probes: Vec<NameProbe { key, found }>, parent }`
for each lookup, including misses that allowed a fallback (sheet scope then workbook scope), so
the engine can rebind exactly the formulas a shadowing change affects.

At run time `Instruction::Name` runs the block in a fresh frame (`PendingBlock { frame:
locals.len(), .. }`), once per use (`=Nm+Nm` runs the body twice), and records a
`NameInvocation { ordinal, parent, definition, context, body, result: NameResultKind }` in the
effects when an effects sink is present (bounded by `MAX_RECORDED_NAME_INVOCATIONS = 1024`).
`CompiledPlan::name_memo_eligibility` is the static precondition for a future shared memo: a body
with a volatile call, an unclassified catalogue call, a user call, an external input, a callable
value or locals is ineligible; a body with only reads needs a read certificate.

---

# Appendix H: reference binding and INDIRECT

## H.1 Operands (`calc-fml/src/reference/mod.rs`)

```rust
pub struct FormulaAnchor { cell: CellKey }
pub enum ReferenceNotation { A1, R1C1 }
pub enum AxisRef { Absolute(u32), Relative(i32) }
pub struct CellRef { row: AxisRef, column: AxisRef }
pub enum ReferenceShape { Cell(CellRef), Area { first: CellRef, last: CellRef }, Rows { first, last }, Columns { first, last }, Deleted }
pub enum SheetTarget { Anchor, Sheet(SheetId), Deleted, Span { first: SheetId, last: SheetId } }
pub struct ReferenceOperand { pub sheet: SheetTarget, pub shape: ReferenceShape }
pub enum PlacementError { DeletedSheet, DeletedCells, RowOffGrid { resolved: i64 }, ColumnOffGrid { resolved: i64 }, SheetSpan }
impl ReferenceOperand {
    pub fn instantiate(&self, anchor: FormulaAnchor) -> Result<GridArea, PlacementError>;         // off-grid is #REF!
    pub fn instantiate_wrapped(&self, anchor: FormulaAnchor) -> Result<GridArea, PlacementError>; // names: offsets wrap
}
```

The ends of an area are held in an anchor-independent order (two absolute or two relative by
position, an absolute one first), so `=R[-2]C[-1]:R7C4` is one operand wherever bound; placing
finds the corners. A 3-D span is expanded by compile into one operand per sheet the catalogue
lists (`Reference3D { first, count }`).

## H.2 The catalogue the binder asks (`reference/bind.rs:46-100`)

```rust
pub trait ReferenceCatalog {
    fn sheet_named(&self, name: &str) -> Option<SheetId>;
    fn defined_name(&self, name: &str, scope: NameScope) -> NameDefinition;    // NotModelled refuses, never #NAME?
    fn name_binding(&self, name, scope, parent) -> Option<NameBindingProof> { None }
    fn table(&self, name: &str) -> TableLookup;
    fn table_containing(&self, cell: CellKey) -> TableLookup;
    fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>>;
    fn unknown_calls_are_names(&self) -> bool;        // true: =FOO(1) is #NAME?; false: a VBA/add-in function might hold it
    fn workbook_name(&self) -> Option<&str>;
    fn external_sheet(&self, book, sheet) -> Option<SheetId>;
    fn external_function_target(..);
}
```

## H.3 INDIRECT (`run.rs:2302-2342`, `calc-func/src/catalogue/reference_text.rs`)

The INDIRECT kernel coerces its text and `a1` flag and asks the services' `ReferenceText` to
resolve the text; calc-func parses no reference text. calc-fml supplies `TextBinding { catalog,
anchor, resolved: Vec<GridArea> }` per eager call, which binds the text with the same binder
(`bind_text`, INDIRECT's own grammar) against the caller's `ReferenceCatalog` and the instance's
anchor (relative R1C1 text is offset from the formula's cell), keeps each resolved area, and
shows each to the observer as a `ResolvedReference { target, area }` after the call. Text naming
another workbook resolves to `#REF!` (a closed workbook, measured 2026-10-30). A defined name in
the text is `NotModelled` and the call is refused. Name lookups made while binding the text are
traced by the engine's `SnapshotCatalog::traced` (appendix J.4).

---

# Appendix I: reads, effects, user functions, RTD and random draws

## I.1 The reader the evaluator reads through (`calc-fml/src/reference/read.rs`)

```rust
pub trait ReferenceReader {
    /// True only when this missing read is waiting for an admitted, unfinished producer.
    /// Terminal unavailable inputs and unloaded cells never authorize provisional values.
    fn pending_producer(&self, _at: CellKey) -> bool { false }
    fn provisional_argument_pending(&self, _reference: &ReferenceLike) -> bool { false }
    fn provisional_observation_pending(&self, _reference: &ReferenceLike) -> bool { false }
    fn read_denial(&self) -> Option<ReadDenial>;                       // why a Missing is a typed refusal, not a wait
    fn read_fact(&mut self, fact: FactRead);                            // a host question recorded without reading the value
    /// Reports the cells of `area` as runs in row-major order covering every cell exactly once:
    /// Values(&[CalcValue]) | Blank(n) | Missing(n). Once `sink` returns Break the reader stops.
    fn read_area(&mut self, request: ReadRequest, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>);
    fn read_cell(&mut self, request: ReadRequest, at: CellKey) -> Result<CellRead, ReadContractError>; // default via read_area
    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent;        // Area | None (#REF!) | NotModelled
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions>;
}
pub enum CellRun<'a> { Values(&'a [CalcValue]), Blank(u64), Missing(u64) }
pub enum CellRead { Value(CalcValue), Blank, Missing }
```

`read_cells` checks that the runs account for every cell exactly once (`ReadContractError`
otherwise). `SharedReader::value_of` (one cell) and `values_of` (an area as an array; a
multi-area reference is `#VALUE!`; above `2^24` cells refused) turn `Missing` into
`EvalFault::MissingInput { cell }` unless the reader has a `read_denial`, which becomes
`NotModelled`.

## I.2 The evaluation context (`calc-fml/src/eval/context.rs`)

```rust
pub struct FormulaEvaluationContext<'s> {
    reference_result: bool,                       // name inspection: return the expression's reference or value
    numeric: &'s NumericMode,
    reader: &'s mut dyn ReferenceReader,
    environment: Option<SemanticEnvironment>,     // { date_system, compatibility_version, profile }; the clock is separate
    host: Option<&'s mut dyn HostFacts>,
    pass: PassServices<'s>,                       // { clock: Option<f64>, random: Option<&mut dyn RandomSource> }: fixed for one pass
    request: ReadRequest,                         // { purpose, policy, facets }
    observer: Option<&'s mut dyn EvaluationObserver>,
    catalog: Option<&'s dyn ReferenceCatalog>,    // for INDIRECT's text
    functions: Option<&'s CallableTable>,
    invocation: Option<UserInvocationFrame>,      // { id, parent, read_view: Rc<dyn Any>, formula_entry }
    external_inputs: Option<&'s dyn RtdCut>,      // immutable RTD cut fixed at command admission
    rtd_connector: Option<&'s dyn RtdConnector>,
    user_view: Option<UserView>,                  // ActiveSheet / Selection for VBA
    reference_handles: Option<&'s dyn ReferenceHandles>,
    observation_reader: Option<&'s mut dyn ReferenceReader>,   // cached VBA observations (not edges)
    function_diagnostics: Option<&'s mut Vec<String>>,
    user_function_effects: Option<&'s mut UserFunctionEffects>,
}
pub enum ReadPolicy { DependencyWithinCommand, CachedMacroObservation, RegisteredWorksheetCallback, DiagnosticSnapshot }
pub enum ReadPurpose { FormulaValue, FormulaText, FormulaKind, ReferenceShape, SheetMetadata, Diagnostic }
pub enum ObservableFacets { Value, FormulaText, FormulaKind, Shape, SheetMetadata, Occupancy }
pub enum FactInvalidation { OnChange, OnNextAuthorizedCalculation, ObservationOnly }
pub struct FactRead { request: ReadRequest, subject: FactSubject /* Cell | Sheet | Workbook */, invalidation: FactInvalidation }
```

The evaluator refuses to run under `CachedMacroObservation` or `RegisteredWorksheetCallback` as
the *formula's* policy (`run.rs:408-424`); those policies reach it only as the observation reader
a cached-read UDF uses. Host facts asked by CELL, INFO, ISFORMULA, FORMULATEXT, SHEET(S) go through
`ReaderHost`, which records a `FactRead` with its facet and invalidation class before answering
(`cells.rs:274-425`).

## I.3 Effects and evidence (`mod.rs:490-584`)

```rust
/// Effects observed while evaluating one formula, separate from its immutable plan and result.
pub struct UserFunctionEffects {
    calls: usize, volatile: bool,                      // last executed Application.Volatile request wins
    invocations: Vec<UserCallRecord>,                  // { id, parent, callable, state_lane, call_site, occurrence, completed, non_preemptible }
    reevaluation: Option<UserReevaluationReason>,      // XllUncalcedUnwind | ProvisionalArguments
    callable_reads: Vec<CallableBindingProof>,
    external_inputs: Vec<RtdRead>,                     // { topic, lease, sequence }
    next_name: u64, name_invocations: Vec<NameInvocation>, omitted_name_invocations: u64,
}
pub trait EvaluationObserver {
    fn called(&mut self, call: ExecutedCall);                 // each catalogue call that ran, with `synthetic`
    fn resolved(&mut self, reference: ResolvedReference) {}   // each area INDIRECT resolved from text
    fn returned(&mut self, report: EvaluationReport<'_>);     // the outcome, exactly as returned; only the evaluator can make one
}
```

## I.4 User functions (`calc-func/src/udf.rs`, `run.rs:681-901, 3671-4147`)

```rust
pub trait UserFunction: Debug {                 // no Send/Sync: a desktop add-in may belong to its thread
    fn permits_early_execution(&self) -> bool { false }
    fn max_parallel_calls(&self) -> usize { 1 }
    fn reevaluation_policy(&self) -> Option<UserReevaluationReason> { None }
    fn state_lane(&self) -> Option<u64> { None }   // adapters sharing project state (VBA)
    fn permits_reentry(&self) -> bool { false }
    fn cached_reads(&self) -> bool { false }       // reads outside arguments see cached values, not dependencies
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String>;
    fn begin_call(&self, arguments, frame: UserInvocationFrame, context) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> { Ok(None) }
    fn call_with_context(&self, arguments, context: &mut dyn UserFunctionContext) -> Result<CalcValue, String> { /* dereference references, call */ }
}
pub trait UserFunctionInvocation: Debug { fn resume(&mut self, context, quantum: u64) -> Result<UserFunctionStep, String>; }
pub enum UserFunctionStep { Suspended, Awaiting { request: u64 }, Complete(CalcValue) }
pub trait UserFunctionContext {                 // what a running UDF may ask of the evaluator
    caller, user_view, sheet_name, reference_qualifier, callable_identity, rtd_value, read_topic,
    request_reevaluation, call_user_function, take_async_result, retain_reference, resolve_reference,
    invocation, set_volatile, refuse, refuse_by, values_of, argument_values_of,
    worksheet_function, worksheet_function_for, evaluate_constant_expression
}
```

`user_call_slice` (`run.rs:681-901`): resolves the handle in the `CallableTable` (`#NAME?` when
unbound), refuses reentry into an occupied state lane and nesting beyond 64, allocates a
`UserInvocationFrame` and a `UserCallRecord`, computes the areas of reference arguments
(`provisional_arguments`), and either runs the call synchronously (`call_with_context`) or keeps a
`PendingUserCall` activation that `resume` advances by quantum; `Awaiting { request }` parks it
until the engine delivers `accept_external`. A `DiagnosticSnapshot` never enters a user function.
`UserCallContext` (`run.rs:3671-4147`) implements the context over the run: `values_of` reads
through the observation reader when the function declares `cached_reads`, otherwise through the
dependency reader; `argument_values_of` on a pending producer with `ProvisionalArguments` policy
requests a semantic reevaluation and returns `Empty` (the measured XLL unwind, P03);
`worksheet_function` runs a catalogue entry through the same `call` route;
`evaluate_constant_expression` compiles and runs a constant expression in a private `PlanCache`;
`call_user_function` nests another user call with the frame as parent.

## I.5 RTD and random draws

RTD (`calc-func/src/udf.rs`, `calc-fml/src/rtd.rs`): a topic is `TopicKey { progid (ASCII
case-folded), server, strings }`; an `RtdCut` is an immutable view fixed at command admission;
`read_topic` joins or starts the connection through the owner's `RtdConnector` and takes the
admitted value or the connection's provisional one; each read is recorded as `RtdRead { topic,
lease, sequence }` in the effects and ends in the result's certificate. RTD never completes a
call; an update is an input event for later demand.

Random (`run.rs:46-60`, `calc-engine/src/random.rs`): the caller gives one `RandomSource` per
evaluation; the engine's `RandomStream` is `splitmix64` seeded from `(seed ^ pass ordinal << 32 ^
root invocation, tick, cell)`, kept in `EvalLocal.random` across a suspension so a resumed
continuation continues the stream (it does not redraw; completed calls stay behind the cursor).
The random.rs module comment still describes the earlier discard-and-retry model.

---

# Appendix J: the engine contract and the runtime dependency overlay

## J.1 Nodes and edges (`calc-engine/src/state.rs:20-56`, `graph.rs:100-200`)

```rust
pub struct FormulaNode { id: NodeId, anchor: CellKey,
    prerequisites: Option<Vec<Prerequisite>>,   // bind time: every placed reference of the plan + Name(id) per resolved binding
    dynamic: Vec<Prerequisite> }                 // what the last calculated evaluation read beyond its placed references
pub enum Prerequisite { Name(NameId), Cell(CellKey), Area(GridArea), AxisValue(AxisInterval), SheetSpan(SheetSpan) }
```

`prerequisites(instance)` (`recalc.rs:3052-3070`) files every operand of `plan.references()`
(untaken lazy branches, LAMBDA bodies and name bodies included) as `Cell`, `Area` or `AxisValue`,
and `Name(bound.id)` for every resolved `NameBindingProof`. `set_dynamic(node, dynamic)`
(`state.rs:1329-1345`) replaces the node's dynamic edges and refiles them in the one reverse
index; equal edges leave the index untouched. The doc comment on `dynamic_prerequisites()`:
"the areas a reference computed as it ran (INDIRECT's text, OFFSET's result, a spilled range `#`
names) read, and, for an array formula, the cells beside its anchor its result covers or would
cover. They are edges of the one graph like the placed references, so a change to one of these
cells makes the node stale."

## J.2 Ordering (`recalc.rs:1036-1056`)

The dirty closure is the pending nodes, the nodes whose plans are volatile, and every reader
(through structural and dynamic edges) of their anchors or spill areas. It is ordered by a ready
queue with prerequisite counts; ties by anchor (sheet, row, column). "When the dynamic
prerequisites close a cycle that the placed references do not, the closure is ordered by the
placed references alone, and a formula that then reads a cell evaluated later waits for it. A
closure the placed references alone cannot order has a cycle, and the recalculation is refused"
(unless iteration is on). Reused results carry their previous dynamic edges forward
(`recalc.rs:1801`); staged results replace them at publication (`recalc.rs:2629, 2831`).

## J.3 What one evaluation reports (`recalc.rs:4855-5062, 5694-5946`)

The engine wraps its `WorkbookReader` in `Observed`, which records every area read into
`footprint: Vec<GridArea>` (also fact reads of a cell subject, and the anchor of every
`spill_extent` question), `value_reads`, `blanks` and `facts`; and gives the evaluator an
`Instrument` observer whose `resolved()` pushes INDIRECT's resolved areas into the same footprint.
Name lookups made while binding INDIRECT's text are traced by `SnapshotCatalog::traced` into
`name_reads`, which become `Prerequisite::Name` dynamic edges (`recalc.rs:5355-5367`). After a
completed evaluation:

```rust
/// The areas an evaluation read that none of the formula's placed references holds, each once.
fn dynamic_prerequisites(state, node, instance, footprint: &[GridArea]) -> Vec<Prerequisite> {
    for &area in footprint {
        let held = instance.areas().contains(Ok(area))
            || placed.iter().any(|p| p.contains_area(area))
            || instance.areas().iter().any(|placed| placed contains area.first() and area.last());
        if !held && !out.contains(&prerequisite) { out.push(area_prerequisite(area)) }   // Cell or Area
    }
}
```

Reads a UDF makes through `values_of` / `argument_values_of` go through the same `SharedReader`
and so into the footprint; reads a `cached_reads` UDF makes go to the observation reader, whose
footprint is kept apart (`observation_footprint`) and is *not* filed as an edge (cached VBA
observations are evidence, not dependencies: `RECALC_CONTRACT.md` 4, RT 1). RTD reads are not
cell reads; they are `RtdRead`s in the certificate and invalidate by topic.

## J.4 Discovery: waits and retries (`recalc.rs:5862-5946, 2005-2130`; `read.rs:1624-1634`)

```rust
impl ReferenceReader for WorkbookReader<'_> {
    fn pending_producer(&self, at: CellKey) -> bool {
        self.uncomputed.get().is_some_and(|(cell, node)| cell == at
            && binding is Placed && progress.is_scheduled(node) && !progress.is_evaluated(node))
    }
}
```

The reader notes the first formula cell it could not supply in `uncomputed: Cell<Option<(CellKey,
NodeId)>>` and reports `Missing`. The evaluator then either retries the instruction later
(appendix D.4) or completes with `MissingInput`. The engine's `evaluate_with` decides: a missing
cell that a *placed* reference names, that is a scheduled formula and not waiting, is
`RecalcFailure::UnscheduledRead` (the order missed an edge; an invariant violation); a cell reached
through a computed reference becomes `Attempt::Waits { on, cell }`, and `defer` makes the node
wait on the producer (`waits_on`, `waiting`), detecting cycles through the wait chain. A suspended
activation keeps its stack, cursor, random stream and footprint in `EvalLocal`. In the parallel
first wave (J.6) a dynamic prerequisite inside a concurrent scalar callback is a typed refusal
("a dynamic prerequisite inside a concurrent scalar callback", `recalc.rs:2110-2125`); a pure
formula that did not finish is dropped and re-run on the main thread in order.

The acceptance test for L2 (`calc-engine/tests/acceptance.rs:3596-3650`) shows the intended steady
state: `Calc!A1 = INDIRECT("A"&B1)` with `B1 = 5` waits for `Calc!A5` once, then
`dynamic_prerequisites(A1) == [Cell(A5)]`, every formula evaluated exactly once, and after an edit
the dynamic edges order every formula after what it reads.

## J.5 Certificates (`contract.rs:255-304`)

```rust
pub struct ReadCertificate { external_cache: bool, legacy_boundary: bool, uses_boundary_cache: bool,
    uses_imported_cache: bool, dependencies: Vec<ReadSpan>, semantic_environment: u64, bindings: u64,
    volatile_inputs: Option<u64>, details: OptionalReadDetails }
pub struct ReadDetails { linked_inputs, observations: Vec<CachedObservation>, external_inputs: Vec<RtdRead>,
    callables: Vec<CallableBindingProof>, invocations: Vec<CallInvocation>, name_invocations: Vec<NameInvocation>,
    omitted_name_invocations: u64, udf_volatile: Option<bool>, boundary_caches: Vec<BoundaryReadSpan>,
    facts: Vec<FactStamp>, absences: Vec<AbsenceProof>, names: Vec<NameBindingProof> }
```

## J.6 The parallel first wave (`recalc.rs:2173-2215`)

The order is cut into levels by the readers cache (a node's level is one more than the highest
level of what it reads). Each level runs on several threads through the one evaluator
(`evaluate_with`), every evaluation reading only what earlier levels staged (`EvalShared`, read
only while a level runs) and writing only its thread's `EvalLocal`; the main thread stages in
order. Array formulas run on the main thread at their place and cut the levels. Eligibility
(`parallel::eligible`): more than one thread, no interest set, no fences, no suspended activation,
a silent observer, no cycle or iteration, no spill, no data table, at least 1,024 nodes. The
`UserFunction` trait is not `Send`; `UserInvocationFrame.read_view` is an `Rc`;
`CallableTable.slots` hold `Rc<dyn UserFunction>`.

## J.7 Volatility (`calc-catalogue/src/reference_text.rs:24`, `offset.rs:37`; `recalc.rs:2613-2619, 3000-3002`)

The catalogue declares INDIRECT and OFFSET volatile (`EntryTraits::NONE.volatile()`), as it does
RAND, RANDBETWEEN, RANDARRAY, NOW, TODAY, CELL and INFO. `CompiledPlan::volatile()` is true when
any call in any block (untaken branches included, as in Excel) has the trait; the engine puts such
nodes in `state.volatile`, which seeds every recalculation's closure; a UDF's
`Application.Volatile` request is recorded per evaluation (`udf_volatile`) and does the same. The
L2 acceptance test asserts `state.is_volatile(Calc!A1)` for the INDIRECT formula. The module
comment of the INDIRECT kernel in `calc-func/src/catalogue/reference_text.rs:9-11` says the
opposite ("The catalogue declares no volatility, so this entry does not mark INDIRECT volatile");
it predates the declaration and is stale. There is no named policy: `volatile: bool` covers both
"recalculate every pass" (RAND, NOW) and the documented-volatile reference functions, and the
argument-dependent cases (CELL, INFO, SUMIF with mismatched ranges; probe P17) are not expressed.

---

# Appendix K: measured Excel facts the evaluator encodes

Each is in the code with its measurement; the design should keep each one nameable.

| Fact | Where encoded |
|---|---|
| Final `+`/`-` adjusted to 0 when the left operand's biased exponent exceeds the result's by 50 or more; only the root operation; not under parentheses, prefix sign, IF arguments, LET result (dc-b1l, 16.0.20430) | `run.rs:113-128`, `compile.rs:2197-2222`, `NEAR_ZERO_EXPONENTS` |
| Legacy `@` placement table (which parameters intersect a range, which lift) and L3's per-parameter rows | `legacy.rs:1-62`, catalogue `LegacyPlace` |
| Array of references: SUM/AVERAGE/MAX read each reference's first cell, COUNT counts none, COUNTA each, ROWS counts the array, ISREF FALSE; SUBTOTAL, SUMIF, COUNTIF, AVERAGEIF, SUMIFS, AREAS, ROW, CELL, INDEX, T, N run once per reference; TEXT/operators give `#VALUE!` (16.0.20527, 2026-10-30) | `run.rs:2208-2247`, traits `lift_dynamic_references`, `take_reference_array` |
| Boxed array (INDEX of a GROUPBY cell): first element where a value is taken, whole array where an array is taken, spills nothing | `value.rs` `CalcArray::boxed`, `run.rs:1219-1224`, `recalc.rs` `array_outcome` |
| Held array element: operator in a dynamic formula `#VALUE!`, function or legacy operator takes the first element | `run.rs:2264-2279` |
| Lifting shape: max rows by max cols, one-row/one-column broadcast, `#N/A` for short arrays; padding before or after the kernel per function, earlier formed error precedes padding in named calls | `lift.rs`, `run.rs:1618-1645`, `padding_precedes_call` |
| Blank cell result shows 0; IFERROR's blank fallback shows 0; blank positions of a dynamic reference result display 0 (dc-ibp2, W111 cases 129/177) | `run.rs:498-521`, `zero_array_blanks` |
| Lifted IF/IFERROR/IFNA/CHOOSE turn a selected blank into 0; scalar calls keep the kernel's blank policy | `run.rs:1872-1884`, traits `zero_lifted_blank`, `zero_dynamic_lifted_blank` |
| IFS/SWITCH: a scalar selection returns the chosen result whole; a computed Empty in a paired result is told from a raw blank reference | `run.rs:1311-1362`, `PairedResult`, `WholeResultPolicy` |
| `#REF!` given to a reference-only parameter is the call's result (dc-ifc) | `call.rs:641-648` |
| Implicit intersection: row when the area spans rows, column when it spans columns, `#VALUE!` outside or multi-area; `=@{7,8}` is 7 | `refops.rs:118-153` |
| Reference operators: union of two sheets `#VALUE!`, empty intersection `#NULL!`, 3-D meets no operator | `refops.rs:1-26` |
| Names wrap relative offsets around the grid edge (`=LeftCell` for `RC[-1]` in A1 reads XFD1) | `reference/mod.rs` `instantiate_wrapped` |
| LAMBDA recursion: budget 16,383 units, 3 per call plus 1 per LAMBDA argument; `#NUM!` beyond | `run.rs:143-150`, `prepare_closure` |
| INDIRECT to another (closed) workbook is `#REF!` | `run.rs:2322-2327` |
| Excel's x87 double rounding of the four operations; `-0` is `+0`; overflow is `#NUM!`; subnormal results refused | `calc-func/src/kernel.rs` |
| Rounding past DBL_MAX yields a qualified NaN that copies but is never re-created | `value.rs` `excel_overflow_number` |
| Volatility per formula cell for UDFs: last executed request wins, untaken branch requests nothing | `UserFunctionEffects.volatile`, `set_volatile` |
| XLL uncalced unwind: a provisional argument read of a pending producer ends the semantic evaluation (P03) | `run.rs:3904-3920`, `UserReevaluationReason::ProvisionalArguments` |

---

# Appendix L: in-flight work and what not to redo

- `docs/architecture/EVALUATOR_SUSPENSION.md` (another agent, in progress): measures Excel's
  requeue behaviour when a formula reads a not-yet-calculated cell, benchmarks the per-call
  operand-stack clone against alternatives, and proposes the suspension model. Refer to it; do not
  redo the experiment. Q1 asks for the model, not the measurement.
- `RECALC_CONTRACT.md` (phases 1-2) and `SYNTHESIS.md` 1.3-1.4 are the agreed engine contract.
  The evaluator's `ReadRequest`, `ReadPolicy`, `UserInvocationFrame`, `EvalContinuation` and
  `UserFunctionEffects` exist to serve it. Proposals that change the contract must say so.
- Measured-but-unmodelled places are typed refusals (`Admission::Refuse`, `Place::Unknown`,
  `EvalFault::NotModelled { construct, owner }`). A proposal that would turn a refusal into a
  guess is out.
- Open Excel measurements that touch the evaluator (`EXCEL_CALC_PROBES.md`): P12 defined names
  per-use evaluation, P16 dynamic references and whether old targets keep invalidating, P17 the
  volatility matrix (argument-dependent policy, not a flag), P18 branch evaluation and error
  order (defines legal hoisting and common-subexpression elimination), P19 LAMBDA recursion and
  helper witness order, P21 MTC eligibility.
- The owner's standing rules: bit-match Excel always; no calendar estimates (sizes in agent
  effort are fine); one evaluator; portable crates stay free of threads, files and network.
