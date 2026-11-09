# Evaluator design review: calc-fml, calc-value and the engine contract

Date 2026-11-09. Code read at DnaCalc-win HEAD `95c1b3bf`; the evaluator, value, catalogue and
engine files are byte-identical to `6f0a01fb` (`git diff --stat`). Read-only; no cargo, no Excel.
Companion: `8-evaluator-design-prompt.md` in this folder (the GPT-6 Pro question, with the code
excerpts). The per-call operand-stack copy is another agent's experiment
(`docs/architecture/EVALUATOR_SUSPENSION.md`, in progress); here it is one item among others.
"Verified" means read at the cited lines; "inferred" means concluded without a run (section 5).

## 1. Area reviews

### 1.1 The plan and the instruction set (`calc-fml/src/eval/mod.rs:156-712`)

Design: an immutable `CompiledPlan` of 24 `Instruction` kinds over anchor-free operands and
catalogue targets; block 0 the formula, further blocks lazy arguments, LAMBDA bodies and names;
interned by hash in a weak `PlanCache`; placed at an anchor by `FormulaInstance`. Compile runs on
its own task stack. Excel behaviour explicit and well named: `FormulaForm`, `Admission`, the
near-zero check as instructions (`NearZeroGuard`, `NearZeroCheck`, synthetic negations), name
wrapping (`wrapped`), the two `@`s, `#`, function values. Implicit: the *dereference of a cell*
is not an instruction; it happens inside `Call`'s admission (`run.rs:1218-1257`), so the IR shows
calls but not reads, and lifting is a run-time property of `Call`.

Weak: (a) reads invisible in the IR, the one thing a trace compiler and an early-cutoff
scheduler both want; (b) nine engine-facing predicates (`mod.rs:187-480`) each walk the blocks
with their own rules; (c) `Target` holds a `FunctionCallTarget<'static>` pointer, so a plan is not
data; (d) `UserCall { name: String }` is the only heap string in the IR. Options: R1, R2, R6.

### 1.2 The value model (`calc-value/src/value.rs`)

Design: `CalcValue { core: CoreValue (8 kinds), metadata: Option<Arc<ValueMetadata>> }` for
callables, images, cube members and the "Excel overflow NaN"; `CoreRef` as the matching view.
Explicit: `Empty` vs `Missing`, ill-formed UTF-16, `#CALC!` and `#VALUE!` faces, `CalcArray::boxed`
(dc-h9u), the overflow NaN that copies but is never re-created, unions on one sheet, 3-D areas.
Implicit: which kinds a *cell* may hold (not a reference, `Missing`, a callable, an unqualified
non-finite) lives in the engine (`result.rs:116-140`) and in `evaluate_slice`, not in a type.

Weak: (a) `PartialEq` compares numbers by IEEE equality except for the overflow case, which uses
bits; plans hash literals by bits, values compare by `==`; (b) about 56 bytes per value
(estimated from the layout), large for a columnar store or a GPU trace (dc-dxm step 2 planned);
(c) `Missing` is a value kind, so an omitted argument can enter the stack and arrays
(`CalcArray::from_scalar(missing)`) and must be refused at return (`MissingReturned`).
Options: R8; keep `Missing` out of arrays by construction; define number equality by bits.

### 1.3 The core loop, suspension and retry (`run.rs:395-1185`)

Design: `Run` rebuilt per `advance`; `block_slice` runs a block on the shared operand stack with a
heap `BlockCursor` per block holding what is in progress (`pending`, `pending_user`, `child`,
`lazy`). Three suspension causes share `EvalStep::Suspended`: quantum out, a read of a scheduled
but unevaluated producer (`waiting_input`), an async user call. A `MissingInput` with a pending
producer is retried at the instruction with the stack restored from a clone taken before every
`Call`, `Spill`, `LazyCall`, `Invoke` (`run.rs:931-938, 1160-1174`); `effects_before`
(`run.rs:939-942, 1163-1167`) blocks the retry when a user function ran, so effects never replay
(contract 3.3). Good: completed instructions never re-run (no redraws, no repeated calls);
`cursor.result` makes a finished block idempotent; the quantum ends at every user call and
resumable kernel.

Weak: (a) `Spill` cannot produce `MissingInput` (`spill_extent` answers `NotModelled`) and
`Invoke`'s body catches its own waits, so only `Call` and `LazyCall` need the clone; (b)
`call_slice` admits *in place* (`stack[start+index] = admitted`) then drains, so the clone is the
only way back; (c) Rust recursion for lazy arguments, LAMBDA bodies and names
(`MAX_CALL_DEPTH = 64`) although every cursor is on the heap; (d) two entry points (`evaluate`,
used by `detached.rs:474` and `data_table.rs:281`, and `EvalContinuation::advance`) over one loop;
(e) `Run` has 24 fields, ten optional services, valid combinations unstated. Options: R1, R5, R13.

### 1.4 Eager calls, admission and lifting (`run.rs:1280-1664, 2199-2300`; `lift.rs`)

Design: `call_slice` admits each argument (`admit`: a one-cell reference becomes its value, a range
by `Admission`, a boxed array its first element), decides `lifted` per argument, moves the
arguments out, applies the IFS/SWITCH whole-result rule, runs the kernel once or per element.
Explicit in the catalogue: `ParameterClass`, `reject_multi_cell_reference`,
`padding_precedes_call`, `lift_dynamic_references`, `take_reference_array`,
`lift_computed_array`, `PairedResult`, `WholeResultPolicy`. Explicit in code with measurements:
short arrays `#N/A`, broadcast, arrays of references (`reference_array`,
`lifts_over_references`), held arrays (`held_array_element`), first cell of an array-valued
result (`lifted_element`), formed error before padding in named calls. Every rule carries its
Excel build, date and formula; unmeasured combinations are typed refusals.

Weak: (a) the *order* of the five element sub-rules is only the control flow of `lifted()`, with
no per-rule unit test, only end-to-end captures; (b) three legacy special cases run before
admission in `call_slice` (first reference of a reference array, paired blanks, the
post-admission refusal) and none is visible in the instruction; (c) `is_lifted` runs twice per
argument. Option: R7.

### 1.5 Lazy calls (`run.rs:1700-1891, 2432-2451`)

Design: `PendingLazy` holds the evaluated arguments; the kernel is re-entered with
`PendingArguments`, which answers `#CALC!` for an argument not yet evaluated and records the first
such index (`wanted`); the result is discarded, the wanted block runs, the kernel re-enters, up to
`argc + 1` times. An array in argument 0 (IFERROR: argument 1) makes every argument run and the
kernel lift per element. Explicit: `ArgumentDemand::{BranchOnCondition, FallbackOnError}`; array
IF evaluating both branches; selected blanks becoming 0 in lifted IF-family calls. Implicit: that
a lazy kernel's selection is a pure function of the arguments it has seen (the protocol depends on
it; the catalogue cannot say it); that `#CALC!` never leaks (it does not: a set `wanted` discards
the result, `run.rs:1806-1811`). Good: resumable without re-running an argument; no callbacks
across the calc-func boundary; IF stays a plain function.

Weak: a sentinel standing in for "not yet evaluated" is a smell for a transcoder, which wants a
branch; worst case `argc + 1` kernel runs (IFS with many pairs). Option: R9.

### 1.6 The legacy form and the near-zero check (`legacy.rs`; `compile.rs:569-577, 2081-2222`; `run.rs:113-128, 2179-2194`)

Design: compile walks the tree with a `Place` (Value, Aggregate, Array, ArrayAggregate, Unknown);
admission is `Place::admission()`; `Place::argument()` is a cascade over the catalogue's 21
`LegacyPlace` variants (`legacy.rs:120-186`). The root `+`/`-` is checked by five emitted
instructions. Explicit and richly measured (`legacy.rs:1-62`, the L3 per-parameter rows, dc-b1l's
exponent rule and its exclusions); `Place::Unknown` keeps unmeasured places honest refusals; the
`@` Excel writes back is the oracle.

Weak: (a) the 21 variants are combinations of about four orthogonal facts (at the root, under an
array place, lifts references, repeats) re-derived by matching variant names; (b)
`carries_final_sum` returns `false` unconditionally (`compile.rs:2081-2084`) while `Task::Lower`'s
doc still describes IF arguments ending the tail; (c) LET and LAMBDA bodies are `Place::Unknown` in
a legacy formula, so a range meeting a single-value place there is refused; worth a probe, LET
is common in Excel 365 files. Options: R10, R13.

### 1.7 LET, LAMBDA and defined names (`compile.rs:1527-1831`; `closure.rs`; `run.rs:1927-2119`)

Design: frame-local slots (`Local`, `Store`); a `Closure { plan, block, params, required,
captured }` behind `CallableValue`; a call is a `PendingBlock` with budget accounting; a defined
name is compiled into the referring plan as a block and run per use in a fresh frame. Explicit:
lexical capture, optional parameters, the measured recursion budget (`RECURSION_BUDGET`,
`CALL_COST`, `CALLABLE_ARGUMENT_COST`), name wrapping, per-use evaluation, scope fallback as
`NameBindingProof` probes. Implicit: a LAMBDA is callable only by the plan that made it
(`Arc::ptr_eq`, `run.rs:2086-2088`), right for Excel but a refusal rather than a modelled error.
Good: the closure owns its plan; name bodies compile once per plan; `name_memo_eligibility` says
what it does not prove.

Weak: (a) a name used by 10,000 formulas is lowered once per distinct referrer normal form and a
name edit recompiles them all; the engine's name table and the plan's name blocks are two
representations of one definition; (b) `MAX_CALL_DEPTH = 64` is a native-stack limit presented as
"not modelled" while Excel's own limit (the budget) is already encoded; (c) a LAMBDA created
inside MAP over a large array allocates a captured slice per element. Options: R4, R5.

### 1.8 Reference binding and INDIRECT (`reference/mod.rs`, `bind.rs:46-100, 276-288`; `run.rs:2302-2342`)

Design: anchor-free operands with `AxisRef::{Absolute, Relative}`, anchor-independent corner
order, `SheetTarget::{Anchor, Sheet, Deleted, Span}`, typed `PlacementError`; INDIRECT's text
bound by the same binder through a per-call `TextBinding` that records resolved areas for the
observer. Explicit: `#REF!` qualifiers, off-grid placement, wrapping, 3-D expansion, closed
workbook `#REF!`, A1 vs R1C1. The cleanest layer: small `Copy` types, one trait, the measurement
beside each rule. Weak: (a) name lookups made while binding INDIRECT's text are traced by the
*engine's* catalogue wrapper (`SnapshotCatalog::traced`), not reported by the evaluator, whose
`resolved()` covers areas only; (b) a `TextBinding` is built for every eager call
(`run.rs:1464-1468`), because "may resolve text" is not a catalogue fact.

### 1.9 Reads, effects, user functions, RTD, random (`cells.rs`; `context.rs`; `run.rs:681-901, 3671-4147`; `udf.rs`)

Design: one `ReferenceReader` seam for every cell read (kernels through `ReaderCells`, the
evaluator through `SharedReader`, both over `RefCell`s because kernels and nested evaluation
interleave); effects and evidence to `Option<&mut>` sinks and the `EvaluationObserver`; user
functions through `UserFunction` / `UserFunctionInvocation` with a `UserFunctionContext` the run
implements; RTD through an immutable cut and the owner's connector; one `RandomSource` per
evaluation. Explicit: `cached_reads` (VBA reads outside arguments see cached values and make no
edge), per-cell UDF volatility with last request wins, the XLL uncalced unwind as
`UserReevaluationReason::ProvisionalArguments`, measured RTD topic identity, `DiagnosticSnapshot`
never entering user code, reentry refused unless declared. Good: every read, UDF and helper-LAMBDA
reads included, funnels through one seam, so the engine's footprint is complete without the
evaluator's help; effects are separate from plan and result.

Weak: (a) eight optional services and two sinks on the context, validity in prose only; (b)
`UserInvocationFrame.read_view: Rc<dyn Any>` and `CallableTable.slots: Vec<Option<Rc<dyn
UserFunction>>>` make the whole UDF route thread-bound by type, even for a thread-safe XLL, so
parallel eligibility is a run-time check; (c) `random.rs:4-8` documents the old
discard-and-retry model ("draws the same values") while the stream is now kept across a
suspension (`recalc.rs:5705-5713, 5923`); (d) `UserCallContext::refuse` turns a typed fault into
`format!("{fault:?}")` text and re-derives it from `context.fault`. Options: R3, R11, R13.

### 1.10 Engine contract and the runtime dependency overlay (`state.rs:20-56, 1329-1345`; `recalc.rs:1036-1056, 1801, 2005-2130, 4855-5062, 5355-5367, 5694-5946, 6103-6130`; `read.rs:1624-1634`)

**Does the evaluator report enough to keep the overlay exact?** Yes for cells and areas, by
construction rather than by report: every value read passes the one reader, which the engine wraps
(`Observed`) to collect the footprint; INDIRECT's resolved areas come through the `Instrument`
observer; `spill_extent` adds the anchor asked; fact reads add their cell subject. Verified: UDF
reads through `values_of` / `argument_values_of` use `run.reader` (`run.rs:3904-3930`), helper
LAMBDAs and kernels read the same `SharedReader`. Names: structural `Prerequisite::Name` per
binding proof in the plan (`recalc.rs:3052-3060`); dynamic `Name` edges for lookups while binding
INDIRECT's text (`recalc.rs:5355-5367`). Not reported, by design: reads of a `cached_reads` UDF
(`observation_footprint`, evidence, never an edge: RT 1) and RTD reads (per-topic invalidation).
Spill output areas are the producer's dynamic prerequisites (`FormulaNode::dynamic_prerequisites`
doc). Untaken lazy branches, LAMBDA and LET bodies: covered structurally, since `plan.references()`
lists every operand of every block (`mod.rs:409-413`), as Excel's static tree does. Not exact: the
footprint is per area *as read*; `=SUM(INDIRECT("A1:A1000"))` files one `Area`, a UDF reading
A1..A1000 cell by cell files 1,000 `Cell`s, deduped by `Vec::contains` (quadratic). Inferred: no
footprint bound exists.

**Granularity.** One edge per distinct area read is right for invalidation (the reverse index
answers per cell of an area). For scheduling the overlay is only as good as the last evaluation: a
new INDIRECT target is discovered by a wait, and previous targets stay edges until the node is
published again (`set_dynamic` replaces the vector), the conservative side of P16. The parallel
wave's `levels()` uses the readers cache including dynamic edges; a discovery inside the wave
falls back to the main thread (`recalc.rs:2185-2191`), except inside a concurrent scalar callback,
where it is a typed refusal (`recalc.rs:2110-2125`): the one place thread count changes a result
from value to refused.

**INDIRECT and OFFSET volatility.** The premise "deliberately not volatile here" is not what the
code does. `calc-catalogue/src/reference_text.rs:24` and `offset.rs:37` declare
`EntryTraits::NONE.volatile()`; `CompiledPlan::volatile()` reads it; the engine puts the node in
`state.volatile` at bind time (`recalc.rs:3000-3002`) and after publication (`recalc.rs:2613-2619`);
the L2 test asserts `state.is_volatile(Calc!A1)` for `=INDIRECT("A"&B1)`
(`calc-engine/tests/acceptance.rs:3644`). The INDIRECT kernel's module comment
(`calc-func/src/catalogue/reference_text.rs:9-11`) says the opposite and is stale (from dc-51ka;
the trait came with dc-mx3x and moved in dc-zsn.5). So values are kept right by dynamic edges and
invocation counts by volatility; both are needed. Observable: a UDF counter in the same formula
(`=Count(INDIRECT("A1"))` runs every F9), dependents' UDF counters (a dirty upstream formula
re-invokes a nonvolatile VBA UDF even with an unchanged result: `EXCEL_CALC_PROBES.md` section 1),
`Application.Volatile` interplay. It should be a named policy: `volatile: bool` cannot say *why*
(draws, documented reference policy, host facts) nor express CELL, INFO and SUMIF's
argument-dependent volatility (P17). Proposal R12.

**Discovery vs steady state.** Discovery is `Attempt::Waits` and `defer`, the activation kept with
stack, cursor, footprint and stream; steady state is `previous.dynamic` carried into the next
closure (`recalc.rs:1801`) and `set_dynamic` into the reverse index, so the next order places the
reader after its producers (the L2 test: every formula evaluated once). Sound. Implicit: (a) the
engine, not the evaluator, decides whether a `MissingInput` is a wait, an `UnscheduledRead`
invariant violation or a terminal refusal (`recalc.rs:5862-5901`), using `pending_producer`, which
the evaluator also consults for the retry: one question asked in two layers; (b) a wait in the
final dereference of the result (`run.rs:523-531`) and one at an instruction look the same to the
engine, though the former has a completed block behind it. Fine today; both matter when a wait
must be a point in a trace.

## 2. Ranked reworks

Sizes are agent effort; parity risk is the risk to bit-exact Excel results.

**R1. Cell dereference as an instruction; admission never touches the operand stack.** Compile
emits `Admit { target, index, admission }` before a `Call` for each `Value` parameter; `Call` runs
the kernel on admitted values, copies its arguments out and truncates only after success. A
`MissingInput` then arises at an `Admit` (one pop, one push: nothing to restore) or inside a
kernel's own range read (arguments still on the stack: nothing to restore); the per-call clone
goes without a per-call copy. Benefit: the IR shows every read (trace compilation, early cutoff);
retry becomes a property of two instruction kinds; `call_slice` loses its in-place mutation.
Parity risk: low (same reads, same left-to-right order). Size: 3-5 days. Evidence first: the
suspension document's benchmark, decided together; calc-fml suite and corpus before and after.

**R2. One `PlanSummary` at compile time.** `{ volatility, user_calls, externals, callables,
names, lazy_blocks, may_spill, reads_host_facts, reads_reference_text, facilities }` stored in the
plan, replacing the nine block-walking predicates (`mod.rs:187-480`). Benefit: cutoff, memo and
parallel eligibility read one struct; transcoding carries it. Parity risk: none. Size: 2-3 days.
Evidence: an equivalence test against the old predicates over the corpus plans.

**R3. Evidence returned with the step.** `EvalStep::Complete { outcome, evidence }` with
`EvaluationEvidence { effects, footprint: Vec<ReadFootprintEntry>, resolved, name_invocations,
draws }` owned by the caller; the observer stays for streaming. Removes the `Option<&mut>` sinks
and makes the footprint the evaluator's statement, not the engine's reconstruction. Benefit:
clarity, no shared sinks for parallel, `Observed`/`Instrument` shrink to value capture. Parity
risk: none. Size: 3-4 days (`recalc.rs`, `detached_work.rs` call sites).

**R4. Shared name bodies.** A defined name compiles once per workbook into a `NameBody` plan the
engine owns; `Instruction::Name` carries a `NameBodyId` and the expected binding proof;
instantiation links it. Benefit: a name edit rebinds instead of recompiling; the shared memo the
synthesis wants has one body to memo; memory for large name tables. Parity risk: low (per-use
evaluation and wrapping stay). Size: 1-2 weeks across calc-fml and `name_change.rs`. Evidence
first: P12.

**R5. Explicit-stack evaluation.** Replace the `block_slice` → `lazy_call_slice` / `child_slice`
/ `invoke_slice` → `block_slice` recursion by a cursor stack in a loop (cursors are already heap
boxes). Benefit: `MAX_CALL_DEPTH = 64` goes; Excel's budget is the only limit; a flat machine for a
transcoder; a quantum can stop anywhere. Parity risk: low (depth refusals become computed, `#NUM!`
by the measured budget). Size: 1-2 weeks. Evidence first: P19; a depth test on the 1 MB wasm stack.

**R6. `FunctionId` in the IR.** `Target` becomes `FunctionId`, resolved to the kernel at
instantiation or through a per-plan table. Benefit: a plan is data (serialisable, hashable without
a pointer, transcodable). Parity risk: none. Size: 1 day.

**R7. Lifting as a named element pipeline.** Split `lifted()` into `ElementRule` stages
(`ReferenceElement`, `HeldArray`, `ShortArray`, `KernelResult`) applied in one declared order, with
per-stage tests citing the W111, dc-b1l and dc-h9u rows. Parity risk: low with the captures as the
check. Size: 2-3 days.

**R8. A `CellValue` type for published results.** The subset a cell may hold (number, text,
logical, error, with the overflow and image qualifications), produced once by the cell-result
rules at the end of `evaluate_slice`, consumed by publication; `CalcValue` stays the expression
type. Benefit: "what a cell may hold" becomes a type; a small fixed-size value for columnar and
GPU storage. Parity risk: none. Size: 3-5 days (engine results store).

**R9. Lazy kernels that say what they want.** `LazyKernel` returns `Want(index) | Done(value)`;
the `#CALC!` stand-in and the re-entry loop go. Benefit: one kernel run per decision, no sentinel
in kernel code, a direct lowering to branches. Parity risk: none (selection is pure). Size: 2-4
days for IF, IFS, SWITCH, CHOOSE, IFERROR, IFNA and the call interface.

**R10. `LegacyPlace` as orthogonal fields.** `{ root: Intersect|Lift|Aggregate, under_array:
Same|Lift|Aggregate|Unknown, lift_references: bool, repeats: No|All|NextOnly }` and
`Place::argument` as a table. Parity risk: medium (the 21 variants must reproduce exactly; the
catalogue oracle sheets and `ported-parameters-before.tsv` check it). Size: 3-5 days.

**R11. Thread-bound vs shareable callables by type.** Split `CallableTable` into a `Send + Sync`
table (thread-safe XLLs, pure natives) and a thread-bound one (VBA, COM); `read_view` an `Arc`.
Benefit: parallel eligibility is a type fact; the parallel wave can run XLL formulas. Parity
risk: none. Size: 1 week with the host crates. Evidence first: P21.

**R12. A named volatility policy.** `Volatility::{Never, EveryRecalculation { why: Draws |
DocumentedReference | HostFacts }, ByArgument(fn)}` replacing `volatile: bool`; `CompiledPlan::
volatile` and `state.volatile` read it; delete the stale comment. Parity risk: none for the first
two; `ByArgument` waits for P17. Size: 1 day plus P17.

**R13. Small cleanups (half a day).** Delete `carries_final_sum` (`compile.rs:2081`); take
`Spill` and `Invoke` out of the retry set (or let R1 remove it); fix `random.rs:4-8`; make the
valid service combinations of `FormulaEvaluationContext` a builder rule.

## 3. Bold ideas (not ranked)

- **A read-set IR pass.** With R1, a compile pass computes per block "reads before the first
  effect" and "reads that depend on a value" (INDIRECT text, OFFSET result); the engine prefetches
  the certain reads of the next level and treats the dependent ones as the only discovery points.
- **Trace recording as a run mode.** A `TraceSink` records each instruction's operand kinds and
  shapes; the result is a straight-line program for (plan, shapes) with guards on conditions and
  shapes; replay checks the guards. This is where a GPU path starts.
- **Shape-specialised plans.** Lifting decisions depend on input shapes; an "all scalar" plan has
  no lifting code and is what most of a workbook runs.
- **Kernels over a value arena.** Per-recalc arena with 16-byte handles; `CoreRef` already hides
  the storage, so the clone cost and the GPU representation shrink together.
- **An IR-level conformance harness.** Replay recorded (plan, inputs, outcome) triples against a
  transcoded evaluator, keyed by plan hash; the corpus and catalogue captures give the oracle.

## 4. Defects found

Behaviour or evidence defects, not style. None changes a published number that I could verify
without running; each falls back to a typed refusal.

- **D1 (evidence).** `calc-func/src/catalogue/reference_text.rs:9-11` says INDIRECT is not
  volatile; the declaration, `CompiledPlan::volatile` and the L2 test say it is. It misled this
  review's own brief. Fix the comment; encode the reason as policy (R12).
- **D2 (refusal where a wait is possible).** `grouping.rs` (GROUPBY, PIVOTBY) calls LAMBDAs via
  `CallableInvoker::call`, not `call_slice`; a body there reading a pending producer through a
  computed reference ends in "a helper callback without a retained kernel continuation"
  (`run.rs:2044-2047`) instead of waiting; `lambda_helpers.rs` uses `call_slice` and waits.
- **D3 (stale invariant).** `random.rs:4-8` says a formula evaluated again after a wait redraws
  the same values; the stream is now kept across the suspension (`recalc.rs:5705-5713, 5923`) and a
  formula never redraws. The contract's draw-ledger text (3.3) also describes the old model.
- **D4 (dead rule).** `compile.rs:2081-2084` `carries_final_sum` always returns `false`; the
  `Task::Lower` doc still describes IF arguments ending the tail.
- **D5 (parallel-only refusal).** A dynamic prerequisite discovered inside a concurrent scalar
  callback is refused (`recalc.rs:2110-2125`) where one thread would wait and calculate; a result
  should not depend on the thread count.
- **D6 (unbounded footprint).** Cell-by-cell UDF or helper reads file one edge per cell;
  `dynamic_prerequisites` dedups with `Vec::contains` (quadratic); no bound, no coalescing.
  Inferred from `recalc.rs:6103-6130` and `Observed::read_cell`.

## 5. Verified versus inferred

Verified: everything cited with `file:line`; the volatility declaration and its test; the retry
set and `effects_before`; the lazy protocol discarding results when `wanted` is set; the footprint
route for UDF, helper and kernel reads; `call` vs `call_slice` in the two kernel families; the
stream kept across suspension; `carries_final_sum`. Inferred: the 56-byte value size; no
footprint bound; D5 being the only thread-count-dependent outcome; that R1 removes every copy
(kernels keep their arguments on the stack until success, the `retain` path's shape); the sizes
in section 2. Not reviewed: `syntax/`, `reference/structured.rs`, `reference/transform.rs`,
`assist.rs`, `entry.rs` beyond its role, the engine's iteration, data tables, spills and pivots
beyond the evaluator contract.
