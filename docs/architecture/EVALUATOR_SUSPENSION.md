# Evaluator suspension: a formula that reads a cell not yet calculated

Date 2026-11-09. Owner bead: dc-hsa5.18. Code: `crates/calc-fml/src/eval/run.rs` (`ResumePoint`,
`waits_for_inputs`, `Run::call_slice`, `Run::invoke_slice`). Contract context:
`RECALC_CONTRACT.md` 0.3 and 3.3 (physical retry keeps the invocation; effects at most once).

## 1. When it happens

The engine evaluates formulas in an order made from the graph's edges. An evaluation can read a
cell no edge named: INDIRECT's text, OFFSET's and INDEX's reference results, a reference a
LAMBDA or name gives back, a spilled range. When that cell holds a formula this recalculation has
admitted and not yet evaluated, the reader reports `EvalFault::MissingInput { cell }` and says the
cell's producer is pending (`ReferenceReader::pending_producer`). The engine then evaluates the
producer and comes back (`calc-engine/src/recalc.rs`, `Attempt::Waits` / `Suspended`). Every area
an evaluation read is filed as a dynamic prerequisite when the recalculation publishes
(`state.rs` `set_dynamic`), and the next order puts the formula after it, so a wait happens when
a dynamic target is first discovered or changes (acceptance `dc_0k9__indirect_through_the_engine`:
two waits on the first recalculation, none on the next). INDIRECT and OFFSET are volatile, so such
formulas are evaluated on every recalculation, but in the learned order.

Static references never wait: the order already puts their producers first.

## 2. What Excel does (measured)

Excel 16.0.20527 x64 (CalculationVersion 191029, multi-threaded calculation on, 16 threads),
owned hidden instances on a private desktop, 2026-11-09. A VBA UDF `Tk(tag, v)` records each call
in order and returns `v`. Each trial: a new workbook, Manual, formulas entered with `Formula2` in
the listed order, then C1 := 6 and `Application.Calculate` (or `CalculateFull`), then C1 := 7 and
Calculate, then a clean Calculate. Ten trials per case under Calculate and five under CalculateFull,
41 cases, 615 workbooks. Probe, method and raw results:
`crates/calc-engine/tests/fixtures/excel-calc-probes/suspension/`.

With B1 `=C1*2`, B2 `=B1+1`, B3 `=B2+1` entered first and A1 last:

| A1 | Calls in the first calculation (all trials) |
|---|---|
| `=Tk("pre",1)+INDIRECT("B3")+Tk("post",2)` | pre, pre, post |
| same with `B3` (static), or `OFFSET(B1,2,0)` | pre, pre, post |
| `=Tk("outer",Tk("pre",1)+INDIRECT("B3"))+Tk("post",2)` | pre, pre, outer, post |
| `=SUM(Tk("pre",1),INDIRECT("B3"),Tk("post",2))` | pre, post, pre, post |
| `=IF(Tk("cond",1)>0,INDIRECT("B3"),0)+Tk("post",2)` | cond, cond, post |
| `=LET(x,Tk("pre",1),x+INDIRECT("B3")+Tk("post",2))` | pre, pre, post |
| `=Tk("pre",1)+Tk("arg",INDIRECT("B3")+0)` | pre, pre, arg |
| `...INDIRECT("B3")+Tk("mid",2)+INDIRECT("E3")...` (two chains) | pre, pre, mid, post |
| B1..B8 chain, `INDIRECT("B8")` | pre, pre, post |
| `=Tk("pre",RAND())+INDIRECT("B3")+Tk("post",RAND())` | pre(r1), pre(r2), post(r3); A1 = B3 + r2 + r3 |
| `NOW()` in place of RAND | NOW read again; equal or one tick later |

So Excel abandons the formula at the read, keeps nothing of the attempt (the second attempt draws
RAND again and the value uses the second draws), calculates the formula again from its start once
the chain is done, and the second attempt does not abandon again. A UDF before the read is entered
twice, its effects stay. The read happens where the reference becomes a value: at `+`, at SUM's
own read of its reference argument (after all its arguments ran), at IF's result. The later
calculations ran each UDF once; a clean Calculate entered the UDFs again only in formulas with
INDIRECT or OFFSET (volatile). The abandonment depends on Excel's calc chain, not on the reference
kind: the static `B3` form abandons too, while with the producer one level from C1 (B1 `=C1*2`,
26 cases in either entry order, through INDIRECT, OFFSET, RAND, NOW, SUM, IF, LET) every UDF was
entered once.

## 3. The model DNA Calc keeps: wait at the reading instruction

An instruction that may read a cell (`waits_for_inputs`: an eager call, a lazy call, a call of a
function value) and faults with `MissingInput` for a pending producer, with no user-function call
since it started, **waits**: the cursor stays on it, the evaluation returns
`EvalStep::Suspended` with `EvalContinuation::waiting_input()` naming the cell, and after the
producer is done the same instruction runs again from its start. Nothing else in the formula runs
again.

Guarantees to the engine above:

- **What a suspended formula leaves behind:** an `EvalContinuation` holding the plan, the cursor
  (instruction position, retained child cursors of lazy arguments, LAMBDA bodies and names), the
  operand stack below the waiting instruction exactly as it was when the instruction started, its
  locals, and the user-function effects and random stream position so far. No result, no
  observer outcome, no published value.
- **What is re-executed:** only the waiting instruction: its argument admission and its kernel,
  on the same operands. A lazy call keeps the arguments it already evaluated. Earlier
  instructions, their calls, draws and user functions are never re-entered.
- **Effect accounting:** user-function calls and RAND/NOW reads happen once per occurrence, as
  without the wait; the evaluation keeps its `InvocationId` (contract 0.3, a physical retry). A
  wait after a user-function call inside the same instruction is not taken: the fault ends the
  evaluation and the engine either refuses it (`an effectful callable requiring a dependency
  continuation`, dc-hsa5.4) or takes a typed `ReevaluationReason` (P03).
- **Fallback:** a pending read that cannot wait in place (an instruction that is not a waiting
  one, or operands not where the instruction found them) ends the evaluation with
  `MissingInput`; with no user-function call the engine evaluates the formula again from its
  start once the producer is done (`Attempt::Waits`), which is Excel's shape and still pure.
- **Cost:** a wait costs the cursor position and two counts. No operand is copied when nothing
  waits.

How the code makes it explicit: `ResumePoint` (stack depth and user-call count when an instruction
starts) is the whole rewind state, and the waiting instructions are **operand-transactional**:
they read their operands where they lie and pop them only when they complete. An eager call admits
its arguments into its own argument list (`Run::admit_arguments`) and replaces its operands with
its result in `Run::complete_call`; a function value is called in place under its callee; a lazy
call consumes nothing from the stack. A debug assertion checks the depth on every wait.

### Where DNA differs from Excel, and why

Under the same order DNA would enter a UDF before the read once where Excel enters it twice, and
draw RAND once where Excel draws twice and discards one. DNA also reads IF's reference result at
the operator that uses it, after the operator's other operand ran (`=IF(..,B3,0)+Tk()`: DNA has
run both before it waits, Excel only the IF). These differences are only visible through UDF call
counts and side effects, never in a pure formula's value. Matching them would also need Excel's
calc chain: DNA never waits on a static reference, where Excel abandons formulas on its first
calculation after entry. The contract (effects at most once per invocation, physical retries
invent no effects) keeps the DNA model; an Excel-count mode would be a typed reevaluation reason,
not a change of the evaluator.

## 4. Alternatives considered

Measured costs are in section 5. All except (d) give the same observable behaviour.

- **(a) Snapshot (until 2026-11-09).** Clone the whole operand stack before every call, restore
  it on a wait. Same semantics as the model above, but it copies every pending operand of every
  enclosing expression on every call (quadratic in nesting depth) and hides the rewind rule in a
  clone. Rejected.
- **(b) Peek, do not pop (chosen).** Calls read their arguments in place and pop on completion.
  The per-call cost is one clone per argument into the reused argument list (a number copy or a
  reference-count increment), no allocation. The wait state is the cursor. Simplest to reason
  about and to transcode: each instruction is a function from its operands to one result, with a
  separate commit.
- **(c) Stack mark and operand log.** Save only the operands the instruction may consume, restore
  them on a wait. Same cost as (b) plus a restore path and a log to keep consistent; (b) is (c)
  with the log made unnecessary.
- **(d) Restart the formula, memoising pure sub-results by instruction position.** Excel's shape.
  It re-enters prefix UDFs (matching Excel's counts only under Excel's chain order, which DNA does
  not reproduce) and must decide which results to memoise: re-drawing RAND as Excel does means
  RAND is not memoised, so the restart changes draws and needs a new evaluation identity
  (contract 3.3); memoising UDF results breaks Excel's counts anyway. A restart also throws away
  lazy-argument and LAMBDA progress that (b) keeps. Its appeal is a stateless evaluator; for
  multi-threading the retained continuation is already per cell and owned by the engine.
- **(e) Dependency pre-pass or purity tags.** A pre-pass cannot find INDIRECT/OFFSET targets
  without evaluating the formula (their text and offsets are computed values, often under IF).
  Tagging operands pure or impure only decides where a copy is needed, and with (b) none is.
- **(f) Admission as its own instruction ("Admit").** Compile each cell dereference of a call's
  argument into an explicit instruction, so reads are visible in the plan and a wait always lands
  on a one-operand instruction. It fits trace compilation (reads become explicit trace nodes),
  but admission depends on the whole argument list (lifting, IFS/SWITCH whole-result selection,
  reference-array rules) and kernels such as SUM still read references themselves, so calls stay
  waiting instructions. With (b) in place it is a later refactor of the compiler, not a
  precondition; kept as the direction for the trace and GPU work.

For parallel recalculation all of (a)-(c) and (f) keep the wait inside one cell's continuation,
owned by one worker; (d) trades that state for repeated work. For transcoding and trace
compilation (b) and (f) are the clean ones: no hidden copy, and the rewind is "run this
instruction again".

## 5. Measurements

Perf profile, host dna-koderbot (16 Haswell cores, shared with another lane; 1-minute load 1.2-3.9
during the runs), `xtask-harness bench --one` (the measured process of `cargo xtask bench`) with
`--threads 1 --repeat 3`, before (6f0a01fb) and after (the prototype of this change) alternating
with the order flipped each round: 5 rounds for the corpus workbooks, 7 for the micro workbooks.
Evaluation-and-publish time in ms, median (min-max) over rounds; Full is the median of 3 per run.

| Workbook (formula cells) | Phase | Before | After | Change |
|---|---|---|---|---|
| Ofgem Annex 4 (10,582) | Full | 278 (268-318) | 276 (270-288) | -1.0% |
| Ofgem Annex 9 (32,880) | Full | 309 (303-338) | 299 (287-301) | -3.1% |
| Ofgem default tariff cap (165,810) | first recalculation | 3561 (3416-3603) | 3335 (3151-3531) | -6.3% |
| Ofgem default tariff cap | Full | 2780 (2772-2854) | 2645 (2572-2747) | -4.9% |
| Ofgem default tariff cap | one-cell edit | 2214 (2076-2359) | 2084 (2040-2133) | -5.9% |
| `dc_51ka` oracle, INDIRECT (183) | first recalculation | 29.6 (27.5-45.5) | 31.8 (30.1-39.6) | noise |
| shallow: 8 calls, depth 2 (4,000) | Full | 58.9 (50.6-68.1) | 54.3 (49.7-64.5) | noise |
| deep: 30 nested SUM / `+` (3,000) | Full | 64.2 (61.1-71.5) | 41.4 (38.8-45.4) | -35.6% |
| arrays: SEQUENCE and range arrays (4,000) | Full | 671 (639-817) | 680 (630-706) | noise |
| dynamic: INDIRECT/OFFSET (4,000) | first recalculation | 148 (126-230) | 144 (132-153) | noise |

The copy's share of evaluation time today is therefore about 1-6% on the corpus workbooks, about a
third on deeply nested formulas (it grows with nesting depth), and below the noise for shallow
calls and array arguments. Instrumented counts (a measurement build, not committed): the
snapshot held 1.6-1.9 values on average on the Ofgem workbooks, 16 on the deep one; the default
tariff cap took 444,585 snapshots per recalculation and used 2,412 of them (0.5%) on the first
recalculation and after a Rebuild, none in three Full recalculations or six one-cell edits
(1.3 and 1.7 million snapshots); the dynamic workbook used 1,000 on its first recalculation and
none afterwards. The other workbooks never waited.

Equivalence: `cargo xtask corpus --oracle` (62 workbooks, 76,957 formula cells: pass 75,209,
mismatched 600, refused 952, failed or missing 1, awaiting capture 108) and `cargo xtask witness
judge --bank full` (295 functions, 3,910,795 rows, 3,887,850 matched) gave identical totals and
per-workbook and per-function counts before and after, in debug and in perf. The test
`a_pending_producer_suspends_the_reading_instruction_and_keeps_its_prefix` passes on the old
evaluator and on this one.
