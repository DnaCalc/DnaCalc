# Excel calculation probes

Date 2026-10-31. The experiment catalogue of `pro/4-calc-semantics-answer.md` (E01-E30) merged with the open items of
`pro/7-recalc-contract-answer.md` section 13 and SYNTHESIS appendix E, deduplicated, ordered by how much each outcome
changes the contract in `RECALC_CONTRACT.md`. **Top ten marked ★**: SYNTHESIS P0 (cached UDF reads, Manual range
freshness, invocation counts, draw identity, module reload, Formula vs Formula2, CalculateBeforeSave) plus spills,
async identity and observation order. A4's iteration (E07) and data tables (E08) move to P14/P15: the slice returns a
named unsupported outcome for both, and iteration order is partly measured. Target: Excel 16.0.20527, x64, AMD.

## 0. Protocol (every probe)

- **Driver.** PowerShell over COM starts its own Excel (`calc-oracle-excel` worker pattern), stops only that process,
  injects a probe module through the VBA project (the OxVba VBE supervisor route that produced
  `calc-vba/tests/fixtures/excel-recalculation/`), runs one `RunProbe()` per clone that returns a single string, then
  reads the saved package as ZIP parts before any reopen. Extend that fixture's `probe.bas` style; do not start a second
  harness. Never change Trust Center, macro security or registry settings (AGENTS.md). XLL probes load a small local
  test XLL with `Application.RegisterXLL` (no registry). P20's RTD server needs COM registration, which is a registry
  write: ask the owner first or host the server in-process through the test XLL.
- **Logger.** A preallocated module array; capture each observation once, return it, flush after the tested command.
  No worksheet writes, `Debug.Print`, `DoEvents` or extra `Evaluate` inside UDFs. A VBA wrapper changes threading and
  order, so every instrumented fixture has an uninstrumented formula control; MTC probes log natively per thread.
- **State.** Record Excel version/build/`CalculationVersion`, OS, bitness, CPU, locale, add-ins and safe vs normal mode
  (the existing run used safe mode after an add-in crash), mode, iteration settings, `CalculateBeforeSave`,
  `EnableEvents`, `PrecisionAsDisplayed`, `Date1904`, MTC. Cold (just opened), warm and rebuilt chains are different
  initial conditions; open order chooses the mode, so the controller never opens a workbook first by accident.
- **Dirtiness is inferred** from later invocations (no getter; `Range.Dirty` mutates). An unexpected seed = invalid trial.
- **Values.** Typed `Value2` plus VarType before COM marshalling; doubles as binary64 bits and text. C API calls log
  status and value separately; after `xlretUncalced`/`xlretAbort` only `xlFree` and native-memory logging.
- **Repetitions.** About ten fresh clones per priority condition; more only where order or timing varies. **Record**: `case_id, fixture_hash, platform_profile, setup_operations, accepted_initial_observations,
  tested_operation, live_observations, callback_trace, event_trace, saved_artifact_hashes, outcome_class,
  competing_explanations, design_decision`. Raw files, scripts and hashes are kept apart from derived observations.

## 1. Already measured in the repository (O unless marked)

| Fact | Where | Feeds |
|---|---|---|
| Nonvolatile VBA UDF not re-invoked by clean F9 or an unrelated edit; re-invoked by a same-value input rewrite, by a dirty upstream formula with unchanged result, by `Range.Calculate`, Full and Rebuild | `docs/plan/VBA_UDFS.md:59-62,78-83`; `probe.bas` stages clean ... rebuild | P02, P03 |
| Initial evaluation called twice (entry in Manual plus first calculation, inferred); no fixed count assumed | `VBA_UDFS.md:61-62`; baseline stage | P03, P13 |
| Volatility per formula cell: last executed request wins, untaken branch requests nothing, nested helper counts, re-evaluation starts nonvolatile, request outside a UDF has no effect | `VBA_UDFS.md:85-90` | P17 |
| A UDF reading a non-argument **constant** gets no dependency; a forced call reads the new constant | `probe.bas:37-40,147-157` | P01 (dirty formula read still open) |
| After `B1.Calculate` (clean target forced), the next `Worksheet.Calculate` re-invoked B1 but not clean B3; same for B12 then `Application.Calculate` (inferred: range calculation leaves its target dirty) | `observations.json` stages range-force, sheet-clean, hidden-force, toggle-back | P02 |
| Iteration on: cell order within a sheet, sheets by name ignoring case, Gauss-Seidel visibility, seeds from held values; iteration off: circular cells keep held values, entered ones show 0 | `calc-engine/src/recalc.rs:36-67`; fixtures `dc_86f`, `dc_10y`, `dc_10y_iteration`, `dc_y2o` | P14 |
| On open Excel shows a file's `<v>` without recalculating, calculates an ordinary formula lacking `<v>`, an array lacking `<v>` only under `fullCalcOnLoad` (16.0.20430) | `calc-doc/tests/fixtures/d1/excel-open.json`, `calc-doc/src/save/author.rs:712-723` | P06, P24 |
| File form of formulas entered through `Formula2` (prefixes, dynamic-array storage) | `calc-doc/src/save/file_form.rs:1-30`, `tests/fixtures/d1/measure-file-form.ps1` | P09 |
| Structural edits, cut and external-link storage of entered formulas | `calc-host/tests/fixtures/d2`, `dc_gf0`, `dc_ylr` | P11, P25 |

## 2. Catalogue

### P01 ★ Cached VBA read of a dirty formula (E01; A7 cached-read frontier)
- **Setup.** Sheet S: A1=1, B1=`=A1+1`, C1=`=PeekB1()` (reads `S!B1.Value2`, logs value, VarType, `Application.Caller`,
  returns it), C2=`=PeekSelf()` (reads its own caller), C3=`=PeekTwice()` (reads B1, calls `WorksheetFunction.Sum(1)`,
  reads B1 again). Manual; seed B1=2, C1=2 by `B1.Calculate` then `C1.Calculate` (Full alone does not seed a hidden
  observer). Secondary lane: B1=`=PlainProbe(31,A1)+1` to see whether B1 itself ran.
- **Operations.** A1:=10, read B1 by COM and by `ExecuteExcel4Macro`, then on separate clones: `C1.Calculate`;
  `B1.Calculate` then `C1.Calculate`; `Range("B1:C1")`; `Range("C1,B1")`; `Worksheet.Calculate`; `Application.Calculate`;
  `CalculateFull`; `CalculateFullRebuild`. Cold and warm chains; C2 on an existing cache and freshly entered.
- **Record.** Every observed B1 value and type in order, entry/exit order, B1/C1 after, secondary counter.
- **Outcome -> design.** C1 sees 2 before B1 is calculated: `CachedMacroObservation` of the old cache is Excel (SliceV1
  demo 5 confirmed). Sees 11 and B1's counter ran: Excel calculates before the read in that command, so the VBA read
  policy becomes per command kind and fences tighten. Differs by command: encode it in `ReadPolicy`. Self-read gives
  old value, Empty or error: keep it; never turn it into a cycle. Two reads differ inside one call: per-read
  advancement exists and the pinned-root rule (contract 5) is wrong.

- **Measured 2026-11-01 (O).** Build 20527, Windows x64/AMD, safe mode; 25 conditions x 10 fresh workbooks.
  C1-only read the dirty cache 2 with no B1 counter entry; B1 then C1 and contiguous scope yielded 11.
  Reversed `C1,B1` observed Empty then 11 on re-entry. Warm sheet/application calculation left the clean hidden
  observer at 2 while updating B1. Self-read: 0 on entry, Empty on forced calls; both PeekTwice reads stayed 2.
  Settles narrow `CachedMacroObservation` and no hidden edge; provisional re-entry remains a distinct evaluation.
  [Evidence and scope limits](../../crates/calc-engine/tests/fixtures/excel-calc-probes/P01/README.md).

### P02 ★ Command scope, forcing and post-command dirtiness (E02; A7 range scope)
- **Setup.** A1=1, B1=`=A1*2`, C1=`=B1+1` (seeded 2, 3); counter lanes B1'=`=PlainProbe(32,A1)`,
  C1'=`=PlainProbe(33,B1')`; clean counter K1=`=PlainProbe(34,1)`; unrelated dirty branch X1, Y1=`=X1+1`. Variants:
  B1 on Sheet2; B1 in a second workbook; reversed chain A1=`=B1+1`, B1=`=C1*2` for row-major.
- **Operations.** Manual, A1:=5 (and X1:=1), fresh clone each: `C1.Calculate`; `B1:C1`; `B1` then `C1`;
  `Union(C1,B1)` both area orders; `Worksheet.Calculate`; `Application.Calculate`; Full; Rebuild;
  `CalculateRowMajorOrder`. After each narrow command: F9 with no edit, then Full. Repeat in Automatic and Automatic
  Except Tables, logging calculations caused by the mode switch; selected-sheet group, disabled sheet
  (`EnableCalculation=False`) and `ForceFullCalculation` as separate factors.
- **Record.** Values after every step, counter entries per step, what the follow-up F9 repairs, return/event order.
- **Outcome -> design.** C1=3 with B1=2: boundary caches (SliceV1). C1=11: range calculation pulls dirty precedents;
  change `CalcRange` normalization. Range-calculated counters re-run on the next F9: `RangePostState::StillRequired`.
  Row-major differs: a separate command. Mode-dependent results: per-mode normalization table in calc-host.
- **Measured.** Clean-target forcing and the re-invocation hint (section 1).

### P03 ★ Repeated semantic evaluation and callback privileges (E04; A7 repeated evaluation, callback privileges)
- **Setup.** VBA `LateDep(x)` counter with an explicit late precedent and a hidden one; parameters `As Range`,
  `As Variant`, `As Double`; `F(B1)` vs `F(B1+0)`; an unused argument. Test XLL `XCount(R)` incrementing a native
  counter before `xlCoerce`, unwinding on `xlretUncalced`, registered ordinary, `$` and `#` (`#` with R is volatile by
  default: add an `xlfVolatile(false)` variant); a variant choosing its reference inside the callback.
- **Operations.** Entry, first calculation, warm F9, Full, Rebuild, same-value input rewrite, upstream change with
  equal output; dirty-B read through ordinary, `#` and `$` registrations.
- **Record.** Entries, arguments and types per entry, callback status codes, unwind and re-entry, thread id, results.
- **Outcome -> design.** Two native entries around `xlretUncalced`: `ReevaluationReason::XllUncalcedUnwind` with prefix
  effects kept. VBA call with Empty/Null arguments then real ones: `ProvisionalArguments`. `#` reads old cache: map
  macro-equivalent registration to `CachedMacroObservation`; ordinary gets uncalced. A single call in one arrangement
  proves nothing about other arrangements.

### P04 ★ Draw identity, LET reuse and volatile spill passes (E05; A7 random/name/LAMBDA)
- **Setup.** Separate areas: `=RAND()-RAND()`; `=LET(draw,RAND(),draw-draw)`; `=LAMBDA(x,x-x)(RAND())`;
  `=DrawName-DrawName` with name DrawName `=RAND()` used in two cells; `=LET(fn,LAMBDA(RAND()),fn()-fn())`;
  `=MAP(SEQUENCE(4),LAMBDA(x,RAND()))`; `=LET(draw,RAND(),MAP(SEQUENCE(4),LAMBDA(x,draw)))`; `=RANDARRAY(2,3)`;
  `=SEQUENCE(RANDBETWEEN(1,1000))`. Witness workbook: `=SEQUENCE(SizeUdf())` with SizeUdf returning scripted
  sequences (2; 2,3,3,3; alternating 2,3; 2,2,3; error on call k), volatile and nonvolatile, armed after setup.
- **Operations.** Entry-time evaluation apart from clean F9, forced range, Full, Rebuild.
- **Record.** Full arrays (bits), SizeUdf call sequence and returned sizes, final footprint or error, downstream calls.
- **Outcome -> design.** Nonzero `RAND()-RAND()` over trials: two keys per formula. LET zero, lambda calls nonzero:
  `OccurrencePath` boundaries. Alternating sizes give `#SPILL!` while a frozen draw would settle: new draws per
  `SpillShape` pass (contract 3.3); the prefix of SizeUdf calls gives the pass count and stabilization rule that
  replaces `MAX_WAVES = 32`. A single zero difference is inconclusive.

### P05 ★ Blank occupancy, spill growth/shrink and conflicts (E06; A7 spill contention)
- **Setup A.** Z1=1, A1=`=SEQUENCE(Z1)`, C10=`=A10+1`, calculated. **Setup B.** Z1=0; A3 and B1 both
  `=IF(Z1,SEQUENCE(4,3),1)`; F1=`=SUM(B3:C4)`; with Z1=1 the footprints overlap.
- **Operations.** A: Manual Z1:=10; `C10` only, anchor only, both, sheet, Full; shrink to 1; a range holding a child
  but not the anchor. B: vary entry order, dirty order, single-anchor calculations, cold/warm/rebuild, MTC. Separate
  clones with a literal, `=""`, a formatted blank, a merged cell, a ListObject and the grid edge in the footprint.
- **Record.** Anchor values/errors, footprints (`HasSpill`, `SpillingToRange`), child values, readers, cleared
  children, saved array metadata.
- **Outcome -> design.** C10-only returns 1 while A1 is dirty: `CachedAbsent` qualification (SliceV1). Shrink leaves a
  stale child after a command that covered it: removal must invalidate readers. Winner depends on order/history:
  reservation order is semantic input; both fail: no first-finisher winner. Child-only expands scope: command rule.

### P06 ★ Save truth table and open-time caches (E09; A7 save scope, SYNTHESIS CalculateBeforeSave)
- **Setup.** P02's chain calculated, then Manual A1:=5 with old B1/C1 caches. Variants with a volatile formula, a
  stale data table (Automatic Except Tables), a disabled sheet, a pending async call (P07's XLL), a dirty spill.
- **Operations.** `CalculateBeforeSave` false/true x `Save`/`SaveAs`/`SaveCopyAs` x Manual/Automatic Except Tables;
  events off, logging handlers, a `BeforeSave` handler that edits one precedent once. Verify caches before Save; log
  calculations caused by the mode switch. Read the ZIP; then reopen copies in a fresh process in controlled Manual and
  Automatic, as first workbook and after a controller; synthetic `calcId`, `fullCalcOnLoad`, `calcCompleted`, `ca`,
  `sheetCalcPr` variants kept apart from genuine old-version files.
- **Record.** Live values before/after, `<v>`, `<f ca>`, `calcPr` attributes, calcChain, event order, first values
  after open, invocations during save and open.
- **Outcome -> design.** Old `<v>` survives Save(false) with `ca`/`calcCompleted=0`: switch the default to
  `PreservePermittedCaches` (contract 8). Excel omits stale `<v>`: today's drop policy is Excel's. Only Save(true)
  updates the chain: conditional calculation. `BeforeSave` edits reach the file: event order is part of the save
  contract. Save completes with async pending: the receipt names the serialized view.
- **Measured.** Open behaviour for current-version caches (section 1); Excel's own Save(false) output is not.

### P07 ★ Async identity, continuation and stale returns (E10; contract async sentinel)
- **Setup.** Test XLL `AsyncEcho(input, latch)`: deep-copies arguments, keeps its handle, waits on a latch the
  controller opens without entering Excel's STA. Formula `Before()+AsyncEcho(A1,latch)+After()` (measure operand order
  first) and nested `After(AsyncEcho(Before(A1),latch))`, a dependent cell, a duplicate-return switch.
- **Operations.** Start from COM and from UI F9; release normally; then before release: overwrite the cell, undo the
  overwrite, edit A1, move the cell, cancel (Esc), close and reopen, `UnregisterXLL`/re-register; two async sites in
  one formula; a superseding call from the same cell; release old and new in reverse order; deliver a duplicate.
- **Record.** Native entry/exit, handle identity, copied arguments, prefix/suffix effects, `xlAsyncReturn` status and
  Boolean, `CalculationState`, events, method return time, final and saved caches.
- **Outcome -> design.** Prefix not repeated: continuation is observable. A full new evaluation after an edit: new
  identity, not a retry. Old handle accepted in some state: relax SliceV1's revoke-on-any-edit for that state only.
  COM and UI lifecycle events differ: the supervisor never waits for an event the route does not raise.

### P08 ★ Observation order vs scheduling (E03; SYNTHESIS decision 13)
- **Setup.** A long pure chain ending at B1; `PeekB1()` with no edge to B1; a second observer reading several
  independent chain outputs; distinguishable old/new values.
- **Operations.** Same command under different entry histories, cold/warm/rebuild, active sheet, producer position,
  viewport and selection (kept fixed in a separate lane), MTC on/off/one thread.
- **Record.** Values each callback saw, final pure values, environment, invocation order.
- **Outcome -> design.** Equal pure values but different observations: publication is part of the effect boundary
  (fences stay). Observations depend on history: put history into the compatibility profile. Stable across all: the
  tested class may run ahead under a logical view; no general commutation claim.

### P09 ★ Formula vs Formula2 through every ingress (E11; SYNTHESIS C0)
- **Setup.** A1:A4 = 1,4,9,16. On separate sheets B2.Formula and B2.Formula2 = `=SQRT(A1:A4)`; explicit
  `=SQRT(@A1:A4)`; legacy CSE in B2:B5 (`FormulaArray`); paste, `FillDown`, R1C1 setters, file load.
- **Operations.** Read back; save and reopen in Manual and Automatic.
- **Record.** `Formula`, `Formula2`, both R1C1 forms, `Value2`, `HasArray`, `HasSpill`, `SavedAsArray`, raw `<f>`,
  `t`, `ref`, `cm` and metadata parts.
- **Outcome -> design.** Scalar 2 vs array 1..4 from identical text: `FormulaForm` stays in the semantic key and every
  ingress sets it. Rewrites on paste/fill: dialect follows the operation. Mixed `SavedAsArray`: tri-state.
- **Measured.** `Formula2` file form only (section 1).

### P10 ★ Module and provider reload; stored Range across edits (A7 reload and stored Range; E21)
- **Setup.** Controller add-in workbook holding a module-level `Range` and a Static counter; UDF `Hold(r As Range)`
  stores its argument; zero-argument `ReadHeld()` reads it. Test XLL with an async call for the provider lane.
- **Operations.** Between calls: insert/delete rows above, cut/move the cells, multi-area ranges, switch active
  workbook, close and reopen the owning workbook. Replace the module source through `VBComponents` (signature
  `F(x As Double)` -> `F(x As String)`) and observe counters, dirtiness of callers and errors. Unregister/re-register
  the XLL with an async call outstanding.
- **Record.** Parent, `Address(External:=True)`, `Areas`, `Value2`, TypeName, VBA errors, module state, whether F9
  invokes the reader or the callers of a replaced function.
- **Outcome -> design.** Tracking vs invalidation vs rebinding of a stored handle; whether reload resets state and
  dirties callers (CallableLease generation, contract 5). DNA-only half (Excel cannot suspend a UDF): suspend a CalcVBA
  invocation, replace `F(Double)` by `F(String)`, resume, then call the retained handle; check code and state lease.

### P11 Formula transformations and binding after structural edits (E29; SYNTHESIS C0 edit algebra)
Relative/absolute/mixed references, ranges, local/global names, structured references, `@`, `#`, legacy arrays,
external references; copy, cut/move, partial and whole insert/delete, sheet copy/move, UI Undo; edits across a spill
boundary; P10's stored Range. Record both dialects, rewritten text, values, errors, array metadata, saved form. Each
operation is its own `AddressTransform`; binding changes invalidate `BindingProof`s. Extends `d2` and `dc_gf0`.

### P12 Defined names, shadowing and per-use evaluation (E16; SYNTHESIS BindingProof)
Workbook N = `Trace("global",10)` used by `=N+N` and `=N`; add then delete a sheet-local N returning 20 after the
formulas calculated; constants, references, RAND, OFFSET, unused and LAMBDA names; relative names from different
active cells. Record old/new formula results, scope, calls (including those caused by editing the name). Outcome:
whether old bindings are kept or rebound (probe misses in `BindingProof`); per-use effects forbid per-name memo;
unused names never evaluated by display.

### P13 Formula entry and explicit evaluation in Manual (E26)
Set a formula, set it again, rewrite an input via `Value` and `Value2`, multi-cell formula batch, `Range.Dirty`;
`Application.Evaluate`, `Worksheet.Evaluate`, `WorksheetFunction` vs reading `Value2`; F9 while editing (UI lane);
clean unused names, disabled sheet, `ForceFullCalculation`. Record calls during each operation. Outcome: Manual is not
"no execution until Calculate"; entry and `Evaluate` are loci of their own (NameView `Evaluate`, contract fixture setup).

### P14 Iteration remainder (E07, E17)
Measured already: order, visibility, seeds (section 1). Remaining: `MaxIterations` 1/2/large from verified zero
seeds; strict vs non-strict `MaxChange` (d=1/8 just below, equal, above); per-cell vs aggregate stopping with a
downstream amplifier; independent volatile counters per sweep or per command; downstream `PairLog(A1,B1)` timing;
two independent SCCs; errors, text and booleans; cycle membership changing within a command (evolving INDIRECT
selector). Feeds `IterationSession`; also settles whether an SCC is externally atomic.

### P15 Data-table scenarios (E08)
One- and two-input tables built with `Range.Table`; result columns with an input-reference name, a reference-valued
INDEX name, RAND, a counter, a hidden-read UDF peeking A1, an explicit-argument counter; duplicate and permuted
scenario inputs; all modes; table feedback `B1=A1+N(E2)`; cancel during a long table. Record scenario-observed
values, base caches before/after, partial outputs, COM busy responses. Feeds the scenario transaction and `InputView`.

### P16 Dynamic references and invalidation beyond executed reads (E15)
B1=10, C1=20, D1="B1"; F1=`=INDIRECT(D1)`, G1=`=F1+1`; OFFSET, reference-valued INDEX, IF selecting B1/C1, an unused
explicit UDF argument, a hidden-read UDF. Change selector, calculate narrowly, edit old and new targets. Outcome:
whether old targets keep invalidating (today filed as dynamic prerequisites at publish, `recalc.rs:9-17`); an unused
explicit argument that still invokes refutes an executed-read-only graph.

### P17 Volatility matrix (E12)
Every volatile built-in, each CELL/INFO argument, matched/mismatched SUMIF ranges, INDEX control, with dependent
counters, across clean F9, unrelated edit, same-value write, range, sheet, Full, Rebuild and all modes; compare
`IF(TRUE,1,RAND())` with an untaken volatile UDF. VBA request rules are measured (section 1). Feeds the catalogue's
volatility **policy** (argument-dependent), not a flag.

### P18 Branch evaluation and error order (E13)
`Trace(tag,v)` in operands of arithmetic, IF, IFERROR, IFNA, CHOOSE, IFS, SWITCH, AND, OR, array selectors, nested
LET, unused LET bindings and LAMBDA arguments, failing branches; both dialects. Defines legal hoisting, inlining and
common-subexpression elimination in calc-fml.

### P19 LAMBDA recursion, helpers and parameter representation (E14)
Largest successful depth of `Down` by parameter count, wrappers, mutual and tail recursion; MAP/BYROW/REDUCE/SCAN
witness order; LET-bound vs fresh values; references through parameters. Feeds resumable LAMBDA state and the
`#NUM!` depth policy.

### P20 RTD admission (E19)
Local deterministic RTD server (see protocol on registration); same-value, changed and burst updates; duplicate topic
cells; all modes, edit mode, long calculation; save, close, reopen, reconnect. Record `ConnectData`, `RefreshData`
batches, `UpdateNotify`, dependent counts, throttle (restore it). Feeds `RtdUpdate` sequencing and coalescing.

### P21 MTC eligibility and thread safety vs purity (E18)
`$`-registered `TSSeq` (atomic counter) and a pure native workload with per-thread logs over independent branches;
MTC off, one thread, several; one VBA/INDIRECT branch; allowed and unsafe C API calls under ordinary, `$`, `#`. Record
thread ids, counter-to-cell mapping, raw status codes. Feeds capability fields for the later parallel pool.

### P22 Event-visible caches and reentrant edits (E27)
Log `Worksheet_Calculate`, `Workbook_SheetCalculate`, `Application_AfterCalculate`, `Worksheet_Change` under range,
sheet, full, async completion and RTD; one lane with a guarded handler editing a precedent once. Feeds event read views
and the cause/revision of handler edits.

### P23 Cancellation, yields and expensive calls (E28)
Native UDF polling `xlAbort` and one that does not; long pure chain; async latch; data table; interrupt from UI and
COM; calculate again. Record accepted cancellation, partial caches, next-call counts, outstanding handles. Feeds
`Cancelled` vs yield and what a cancelled command leaves published.

### P24 Open order, flags and open events (E23)
Clean/dirty, NOW/RAND, missing UDF, external link, table; first vs second open in a fresh process; genuine older-build
files vs synthetic `calcPr`/`sheetCalcPr` flags; `Workbook_Open` off, logging, editing. Extends `d1/measure-open.ps1`.
Feeds the load policy and `UnverifiedFromFile`.

### P25 External sources and link caches (E22)
Source with stale vs recalculated saved caches; destination opened with `UpdateLinks` 0 and 3; source closed, open in
the same and in another process, missing, changed on disk; reference-valued external names; dynamic-array links.
Extends `dc_ylr`. Feeds `LinkRefresh` and the three-version model (source, admitted snapshot, result).

### P26 UI, formatting and ambient observations (E20)
CELL("width"/"format"/"address"), INFO("origin"/"recalc") under width, AutoFit, hide, font, format, zoom, scroll,
selection, active workbook, mode; Formula2 and Formula lanes. Feeds the fact `OnChange`/`OnNextAuthorizedCalculation`
/`ObservationOnly` policies and keeps interest apart from Excel-visible selection.

### P27 Precision as displayed (E24)
Literal 1.234567 and `=1/3` with dependents under several formats; enable, inspect, change formats, disable; `=(1/3)*3`
beside a split chain; General under width/font/zoom. Feeds the semantic invalidation class for number formats.

### P28 1904 dates and transfer routes (E25)
Paired books, serials 0, 1, 59, 60, 61, 1462, DATE/TODAY; toggle per mode; transfer by `Value2`, `Value`, clipboard,
sheet copy, link; VBA Double vs Date parameters. Feeds the object library's conversion per route.

### P29 DNA-only measurements (E30; not Excel)
Names at scale under snapshots; typed VBA path cost; FEC overhead; blocks under edits; tile coherence; adapter
lifecycle; spill-barrier selectivity (P05 ladder). Counters over geometric sizes, then timing; scheduled, never per
commit. The VBA eligibility/context family stays in SYNTHESIS appendix E.

Dedup: A7 13 rows map to P01 (frontier), P02 (range), P03 (privileges, re-evaluation), P04, P05, P06, P10, P09/P12/P13
(entry, binding), P14, P15; SYNTHESIS appendix E rows to P01-P16 and P29.

## 3. Invariants from answer 4 we may rely on

| Invariant | Evidence | Limit |
|---|---|---|
| Formula dialect and reference/array distinction are preserved; equal text is not equal semantics | D (Formula vs Formula2, `@`); O for `Formula2` file form | Semantics of same text through both setters awaits P09 |
| A command receipt describes the operation actually completed, not workbook freshness | D + I | Exact narrow-command scope awaits P02 |
| Permitted cached observations are not replaced by fresh-demand reads | D (macro-sheet-equivalent access) + O (no hidden edge, constant read) | Stale-formula value and order await P01, P08 |
| Physical retries invent no effects; required semantic re-invocations stay possible | D (XLL unwind protocol) + I | Resume and unwind/reinvoke are different states (P03) |
| Value equality alone cannot suppress a stateful dependent | O (same-value rewrite and unchanged-upstream re-invocation) | Pure-cone cutoff needs an observational-equivalence proof |
| A fresh blank read needs occupancy/absence information | I (spill production) | Selectivity is measured separately (P29) |
| File caches carry provenance, not currentness | D + O (Excel trusts `<v>` on open) | Reopening can change what is inspected |
| Async calls, RTD subscriptions and link snapshots have distinct identities and lifetimes | D + I | Completion policy per kind (P07, P20, P25) |
| Context affecting interpretation (bindings, precision, dates, metadata, permitted environment reads) belongs in validity | D + I | Not every context change recalculates (P26, P27) |
| Dependency-respecting reorder of an isolated, deterministic, effect-free, acyclic region with resolved occupancy and no observer of intermediate publication gives the same final values | Mathematical (induction over the graph) | Excludes UDFs, unstable occupancy, observable caches, nondeterminism, reduction order inside SUM, cycles, tables, competing spills |

Not invariants (A4): one invocation or one draw per formula per calculation; whole-SCC or whole-table atomic
publication; sheet-name order beyond the measured shapes; a fixed volatile-spill retry count; volatiles always on open.
