# DNA Calc target architecture: appendix to SYNTHESIS.md

Date 2026-10-31. Detail moved out of `SYNTHESIS.md` to keep it within 500 lines. Sections A-C are the round-2
text kept for reference (section numbers inside them refer to the round-2 synthesis); D-G are new in the final
round (GPT-6 Pro red-team, VBA hosting and symbolic-search answers); H is the round-2 changelog.

## A. What Excel persists for formulas (full fact-check)


**xlsx** (ECMA-376 `f` element; MS-XLSX 2.2.3):
- Cell formula text is A1 without `=`; reference style is a separate setting (`calcPr/@refMode`). `t` = normal
  | array | dataTable | shared; `ref` on the master; `si`; `aca`, `ca`; data-table `dt2D/dtr/r1/r2/del1/del2`.
- **Shared**: master `t="shared" ref si` + text; dependents `t="shared" si`. `ref` bounds; **membership is per
  cell**; the loader checks only that members lie inside `ref` (`load/sheet.rs:680-682`) and expands each to
  its own A1 text (`:668-704`, `load/shift.rs:1-25`); a dependent carrying text fails as a second master
  (`:626-637`) where ECMA ignores the text (Fable(r2)); a touched group saves unshared (`save/sheet.rs:32-35`).
- **Arrays** `t="array"`, dynamic via `cm=` (`load/sheet.rs:558-610`); **data tables** `t="dataTable"`
  (`:521-548`). **Prefixes** (`_xlfn.`, `_xlfn._xlws.`, `_xlpm.`, `_xleta.`, ANCHORARRAY, SINGLE) and
  dynamic-array storage come from measured per-function tables (`save/file_form.rs:1-74`).
- **Defined names**: `<definedName name localSheetId hidden function vbProcedure xlm comment>`, 0-based
  `localSheetId` (`load/mod.rs:932-950, 1274-1285`). A name's relative references are stored **relative
  to A1** and wrap at the grid edge when placed (`RC[-1]` stored as `Nm!XFD1`): measured on Excel 16.0
  2026-10-23 and implemented (`calc-fml/src/reference/mod.rs:236-250`, `eval/compile.rs:1254-1260`).
  Under row/column edits a name moves only its absolute coordinates (`reference/transform.rs:25-30`).
  Today names are re-authored on save without the file-form pass (`save/author.rs:728-758`).

**xlsb** (MS-XLSB 2.2.2): formulas are parsed expressions (`Rgce`, reverse Polish). Shared: BrtShrFmla
(2.4.796) holds range and Rgce with **PtgRefN/PtgAreaN offsets**; a member's Rgce is a lone **PtgExp**
(2.5.98.40); offsets wrap (RgceLocRel 2.5.98.92); names by **index** (PtgName); PtgTbl + BrtTable for data
tables. The codec converts the package to SpreadsheetML first (`xlsb/mod.rs:1-14`): masters and ordinary
formulas are printed to A1 (`xlsb/formula.rs:1-10, 584-600`), members become empty `t="shared" si`
(`xlsb/sheet.rs:766-783, 829-884`), names print their spelling (`formula.rs:695-712`). The save path
splices the converted XML (`calc-doc/src/lib.rs:141-147`), so a token-to-template loader must keep the
converted parts or generate `<f>` at save (Gemini(r2)).

**A1 vs R1C1, observable**: (1) INDIRECT's `a1` flag (`calc-fml/src/eval/run.rs:1222-1240`; R1C1 offsets
wrap, measured, `tests/acceptance/dc_51ka.rs:253`); (2) names may not look like references in either style,
`R1C1` is a name token in A1 and `B3` in R1C1 (`reference/mod.rs:77-79`, `bind.rs:934-935`); (3) off-grid
relative references: names and INDIRECT-R1C1 wrap (measured), xlsb wraps (spec), the xlsx member mover
refuses, cell placement gives `#REF!` with copy/fill routes unmeasured (`reference/mod.rs:183-195`). GPT(r2)
and Gemini(r2) corrected round 1, which called the whole of (3) unmeasured.

**Consequence for 4.8**: Excel persists templates with relative offsets over areas, and calc-fml already has
`AxisRef::{Absolute, Relative}` (`reference/mod.rs:88-95`). In memory: templates over blocks with plain
coordinates and explicit membership, A1 at the edges, no row/column keys, placement rules per template kind.

Sources: [ECMA-376 f](https://webapp.docx4java.org/OnlineDemo/ecma376/SpreadsheetML/f.html), [MS-XLSX 2.2.3](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf),
[MS-XLSB 2.2.2](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/220abf5e-f561-4333-9fe0-7ac590ed4ad5), [PtgExp](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/dc8d38db-f584-4ff6-b250-a854fe93b365),
[RgceLocRel](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/571409d8-2ed2-47cd-8792-be3c2c9556dc), [INDIRECT](https://support.microsoft.com/en-us/office/indirect-function-474b3a3a-8a26-4f44-b491-92b6306fa261), [Names in formulas](https://support.microsoft.com/es-es/Excel/names-in-formulas).


## B. Seam pressures (round 2 wording)


**S1. Function context vs presentation.** Format engine in calc-func and called by apps
(`calc-func/src/format/mod.rs:1052`, `apps/dna-calc/src/cells.rs:187`); grid width vs CELL("width")
(`calc-host/src/facts.rs:53-71`). Response: `NumberText`; one presentation resolver with base facts apart from
the CF overlay; width-dependent text owned by the resolver with platform metrics. No `Format` facet for CELL
(volatile, `calc-fml/src/eval/mod.rs:190-201`), but fact versions are recorded for validation.

**S2. Formula storage vs structural edits [4.8].** Shared groups expanded on load; a row insert rewrites every
formula (`calc-doc-model/src/structural.rs:1-31`, `calc-engine/src/edit.rs:479-609`); undo rebuilds the engine
(`calc-host/src/command.rs:447-459`). Response: blocks with membership; members individually dirty, ready and
cycle-checked; `Rect`/`Affine` edges, member bitsets, bounded chunks. **Cost, restated (GPT(r2))**: an insert at
row p on sheet S touches (a) blocks on S whose extent spans p (split there) or lies below p (shift); (b) every
reference occurrence, on any sheet, whose target on S is at or below p or whose range spans p, relative or
absolute (Excel moves both, `reference/transform.rs:6-11`); (c) for same-sheet relative references, the members
whose row and target straddle p (a run of at most |d| rows per reference), which split the block; (d) range
endpoints and grid-edge clipping. 3-D references and INDIRECT text are left alone (`transform.rs:19-23`); names
move absolutes only. Round 1's "at most 1 + 2r pieces" holds only for (c) and is withdrawn as a general bound.
Optional indexes, built where counters show a scan: a reference-occurrence index over all targets (absolute,
cross-sheet, untaken branches), and a lazy row-shift index. Undo: inverse model and engine patches.

**S3. Names at scale vs bind-by-spelling [4.8].** Linear lookup, whole-`Definitions` clone, all templates cleared,
name text inlined, LAMBDA closures tied to one plan. Response: 1.2.

**S4-S5. Visible-first recalc vs Excel semantics, dynamic topology and effects.** Off-screen spills into visible
blanks; observable UDF order and counts; iteration; tables; command scopes; new edges close cycles; retries replay
side effects. Response: 1.3; pure units validate, effectful units are admitted; ordered groups run in the serial
reference order until Excel's calc-chain order is measured.

**S6-S7. Worker boundary; early-bound VBA.** JSON per message (`wire.rs:830-838`), eight strings per cell, DOM
rebuilt per scroll, in-process server in the page: 1.6. VBA's remaining cost is per-call boxing, not lookup: 1.7.

**S8. OneCalc's ceiling vs UDF registration.** `Rc` registry in calc-func (`calc-func/src/udf.rs:5,53`); ROW()
refused in OneCalc (`catalogue/lookup.rs:1751`). Response: registry to calc-fml; calc-vba on calc-fml,
calc-object-model, `calcvba-host-api`. **S9. Skins**: Grid-owned shortcuts (`grid/mod.rs:695-794`), string menus,
per-skin requests; response 1.5, neutral `Target` including names.

**S10-S12.** No summary-tree SUM (`aggregate.rs:120,142`), no yield that inserts a binary64 store, vendor
provenance on Exact results. `set_clock` dirties every node (`calc-engine/src/state.rs:782-788`): tick services,
RTD as external updates. CF parsed from XML per query (`calc-host/src/conditional.rs:712`): typed CF/DV decoded
once, with the presentation resolver.

**S13. Codec save vs crate splits.** calc-doc needs the parser (`save/file_form.rs:40` imports `parse_formula`;
`edit.rs:160`); structural transforms call `bind_references` (`reference/transform.rs:32-38`); function tables
live in calc-fml and calc-doc (3-D admission `eval/compile.rs:1438-1568`, `eval/legacy.rs`, lifting
`eval/run.rs:1123-1159`, Ftab `xlsb/functions.rs`). Response: calc-syntax owns `syntax/` (lexer, parser, tree) and
`reference/` (notation, bind, structured, transform, `ReferenceCatalog`); every function table goes to
calc-func-catalog (1.10). Fable(r2), Gemini(r2).

**S14. Save under blocks.** The author path refuses a cached value in a cell without its own formula
(`save/author.rs:486-498`) and unshares moved groups. Response: save diffs blocks directly (C3).


## C. Round-2 settled outcomes and final positions


#### 5.1 Settled by evidence or ruling

| Dispute | Outcome | Why |
|---|---|---|
| Stable `AxisKey`s; non-grid participant keys; `Unbounded64`; shard executor | coordinates + optional indexes; names only; Excel limits | **[4.8]** |
| asupersync unusable (GPT(d)) | permitted, ring-fenced | **[4.7]**; GPT withdrew |
| One node per block; insert cost O(unique templates) (Gemini(d)); general 1 + 2r (round 1) | members dirtiable; cost list in S2 | false cycles; `transform.rs:6-11` (GPT(r2)) |
| Name memo when no relative references (round 1) | reference-or-value, guarded memo | all four (r2); `reference/mod.rs:139-146`, `run.rs:1000-1015` |
| Off-grid references "unmeasured" (round 1) | names, INDIRECT-R1C1 measured; cells open | `reference/mod.rs:236-250` (GPT(r2), Gemini(r2)) |
| ComCallEarly name-keyed (round 1) | descriptor-keyed at 0ad2ad28; boxing remains | `oxvba-vm3/src/lib.rs:4258-4300` |
| Exact hardware only on vendor match; soft transcendentals "refuse" (round 1) | hardware always natively, vendor as provenance; soft approximates today | owner 4.4; `excel_numeric/mod.rs:9-15` |
| "Only staged spills stop parallelism" (round 1) | also iteration, cycles, tables, observer, finite budget | `recalc.rs:644, 742-757` (Opus(r2), Gemini(r2)) |
| calc-syntax without parser/binder | owns both | `file_form.rs:40`, `transform.rs:32-38` |
| `PageSummary` SUM; philox; same-tick invariant; Excel build in target id; TLA+ | rejected as in round 1 | `aggregate.rs:120,142`; `random.rs`; `read.rs:435`; 4.4; OCT |
| VBA reload keeps values vs recalcs dependents | neither evidenced; measure | `observations.json` has no module-edit stage |

#### 5.2 Final positions on the remaining choices

| | Opus (r2) | Fable (r2) | GPT (r2) | Gemini (r2) | Synthesis |
|---|---|---|---|---|---|
| A block grain | chunked blocks with membership, bitset, affine | same | same; chunk size tunable, independent of storage | same, 64-row slices | converged |
| B partial publication | 3a then 3b over a `SchedUnit` | 3a then 3b on scalar nodes | same; semantics specified first | 3a then 3b immediately | converged: 3a, 3b-1, 3b-2 |
| C ordering | ready counts + heap in 3b | same (withdrew "measure first") | same; elaborate algorithm on counters | same; Pearce-Kelly deferred | converged |
| D order policy | no Excel mode drops off-interest work | same | interest reorders inside command demand | demand per mode; interest never drops | converged on GPT's wording |
| E research home | `tools/calc-func-lab` | same; "within xtask" = repository | same; xtask verbs forward | same | converged; owner confirms (6) |
| F object library | `calc-object-model` | same | same | same | converged |
| G renderer | measure; accessibility required | same | recycled DOM first, then compare | measure on scroll traces | open until U3 |
| H formatting | `calc-format` crate (page bundle) | calc-fml module | module unless a lower consumer exists | module | open; see below |
| I layering check | sweep + edge check on `Cargo.toml` change | same | sweep; on manifest or rules change | `Cargo.toml` DAG check in every `xtask check` (<10 ms) | 3-1: sweep + on manifest change |
| J reload | generation bump | same | same, plus effect ordering | same, no in-place swap | converged |

**H, evidence**: dna-formula-ui depends on calc-fml and calc-func today, and the page formats through
`calc_func` (`apps/dna-calc/src/cells.rs:173-192`). If tile text is formatted in the worker (1.6), a module
suffices and the page links calc-syntax + calc-func-catalog; if the page must format locally, a `calc-format`
crate keeps the evaluator out of the bundle. Recommend: decide in U1 by measuring the bundle; default module.


## D. Red-team findings checked against the code

The GPT-6 Pro red-team (`pro/3-architecture-redteam-review.md`) reviewed the pack only. Each finding was
checked here against DnaCalc-review 5a9348b (`git show`) where the code can answer it.

| # | Finding (red-team rank) | Code check | Disposition |
|---|---|---|---|
| 1 | Cached VBA observations make pure ordering observable | `docs/plan/VBA_UDFS.md:91-93`: Excel kept the old value of a cell read inside a UDF until explicit calculation. Today a read of an uncalculated cell discards the evaluation and re-runs it after the cell (`calc-engine/src/recalc.rs:607-610`), i.e. calculate-before-read. Round 2's "evaluate synchronously on demand" contradicted the observation. | Accepted; evidence decides against round 2. `ReadPolicy` and observation frontier added (1.3). |
| 2 | "Settled" conflates command completion and freshness; invocation identity missing | Calc modes exist (`calc-host/src/recalc.rs:74-82`); no range-calculation command exists in calc-host (no match). RAND streams are keyed (seed, tick, cell) and "a formula evaluated again in the same recalculation ... draws the same values" (`calc-engine/src/random.rs:4-8`), the exact property the red-team flags for indeterminate-size spills. | Accepted. `ResultStamp`, `Freshness`, `InvocationId`, draw identity per semantic pass. |
| 3 | Structural transactions and identities | Undo already restores as a new revision (`calc-host/src/command.rs:443-447`), but rebuilds the engine with everything pending. | Accepted; "undo is a new revision" is kept as an invariant, the rest added. |
| 4 | Formula blocks lack dialect | `FormulaForm::{Legacy, Dynamic}` exists (`calc-fml/src/eval/mod.rs:107-113`) and the engine's template key already includes it (`calc-engine/src/state.rs:401`); round 2's `Template` omitted it. | Accepted; dialect is part of the semantic key. Edit algebra and save mode added. |
| 5 | Name definition vs evaluation; transitive memo eligibility | Names inline into the caller's plan and run per use (`calc-fml/src/eval/run.rs:1000-1015`). | Accepted; `NameView`, `BindingProof`, transitive eligibility. Round 2's "inspection adds no evaluations" withdrawn. |
| 6 | Spill absence, iteration, data-table scenarios | Spill ownership is first-established-wins (`calc-engine/src/spill.rs:3`); the parallel route stops at waiting arrays (`recalc.rs:710-727`). | Accepted; absence certificates, `IterationSession`, scenario transactions. |
| 7 | Typed hosting needs continuations, code+state leases | OxVba 0ad2ad28 per-call boxing (`oxvba-vm3/src/lib.rs:4264-4287`). | Accepted, merged with the VBA answer (appendix F). |
| 8 | Persistence behind partial publication; XLSB writer unclear | The codec saves an .xlsb as .xlsx (`calc-doc/src/xlsb/mod.rs:11-14`); `calcOnSave` is read only by the xlsb converter (`xlsb/workbook.rs:562`). | Accepted; the "gap" is real: no same-format XLSB writer. Decision 12. |
| 9 | Fact validation is not invalidation; presentation-only too broad | `set_workbook_facts` dirties nothing because CELL/INFO are volatile (`calc-engine/src/state.rs:776-779`). | Accepted; four change classes kept apart (1.3). |
| 10 | Budgets vs Exact kernels and foreign calls | Budget counts evaluations (`recalc.rs:216-229`), not work inside a cell. | Accepted; resumable kernels, safe yield points. |
| 11 | Tiles need transaction closure; semantic projection | Today JSON per message, whole-window reset (`calc-host/src/session.rs:735-752`). | Accepted; `TileFrame`, bounded queues, accessibility projection. |
| 12 | A runtime seed is not a workbook replay contract | n/a (design). The red-team reports asupersync 0.5.0 docs list a nightly default feature set and a stable subset with defaults disabled, contrary to research-libraries.md; not checked here. | Accepted; semantic vs publication replay; pin version, features and toolchain for the spike. |

## E. Measurements that settle open claims

Targeted experiments, run when the dependent step starts (owner 4.6: not per-commit process). Excel cases
keep input files, operations, saved caches, build/platform/CPU provenance and traces.

| Question | Experiment (red-team; VBA answer) |
|---|---|
| Cached VBA reads | Peek a dirty non-argument cell in Manual, range and full calculation; a UDF observing its own caller |
| Effect order under interest changes | Same workbook, alternate viewport priority around Peek and a pure producer |
| Range.Value and save as barriers | Read dirty values; save with CalculateBeforeSave false/true; inspect caches before reopen |
| Effect invocations per unit | Counter UDF before a late dynamic dependency; cold, warm and rebuild chains |
| Random draw identity | `RAND()-RAND()`, `LET(r,RAND(),r-r)`, a named RAND twice, RAND in a repeated LAMBDA, `SEQUENCE(RANDBETWEEN(1,1000))` |
| Iteration | One/two iterations across sheets, nonconvergence, errors, a downstream counting UDF |
| Data-table scenarios | Duplicate scenario inputs with RAND and a counter UDF, a reference-valued name, callback reads, Automatic Except Tables, cancellation |
| Spill shadow selectivity | Thousands of unknown-shape anchors above a viewport; time to first settled value |
| Dialect | `Range.Formula` vs `Range.Formula2` with `=SQRT(A1:A4)` through UI, clipboard, setters and files |
| Edit algebra | Copy vs move of `B1=A1`, cut, partial insert/delete, sheet copy/move/delete, undo |
| Name binding | Add/delete a shadowing local name after a global lookup succeeded, for existing and new formulas |
| Names at scale | Edit one of 100k-1M names with snapshots and closures retained |
| Reload | Suspend F(Double), load F(String), resume and call a retained handle |
| Stored Range | Insert/delete/cut, switch workbook, close/reopen, multi-area Range |
| Typed path cost | Empty, scalar, string, array procedures and one Range callback, against a plain native call |
| Budget responsiveness | One very expensive formula with a queued edit; longest non-yielding span, p95/p99 |
| FEC overhead | Arithmetic, large range, name-heavy and callback workloads |
| Block storage under edits | Dense fill, sparse membership, scattered edits, insert/undo cycles |
| Tile coherence | Multi-tile spill, dictionary rotation, slow subscriber, reset |
| Native runtime choice | Provider churn with delayed async returns, RTD bursts, reload; serial driver plus modest pool vs adapter |
| VBA eligibility and context (VBA answer) | Public/Private, Option Private, module kinds; cell vs Run vs Macro dialog; idle vs in-macro calculation; argument kinds; ByRef aliasing; Err.Raise/CVErr; nested calls with conditional volatility |

## F. CalcVBA hosting detail (GPT-6 Pro VBA answer)

**Four tightenings** (`pro/2-vba-hosting-answer.md`, "Recommended direction"):
1. Two call contracts: a standard-module procedure is a typed callable entry with explicit runtime-state
   ownership; a host object exposes typed interfaces (COM on Windows, equivalent interfaces elsewhere). They share
   type descriptions and ownership conventions; module procedures are not made into COM objects.
2. Early binding is not low call overhead: a preserved IID/slot/signature is early-bound; Variant vectors,
   repeated interface acquisition, generic helpers and libffi can still make it expensive.
3. Common semantics, explicit target ABIs: Windows COM layout and conventions; a Linux native projection; wasm
   memory/handles plus typed imports or function-table adapters. `#[repr(C)]` fixes layout, not calling convention
   or ownership.
4. Execution lifetime and observable state are part of the contract: generations alone do not stop calls into
   reclaimed code; read-only Range access does not make VBA pure; cancellation does not undo mutated globals.

**Ownership objects**: `ProjectImage` (immutable verified code, references, descriptors, compiled artifacts),
`ProjectSession` (globals, Statics, class instances, init state, host bindings, affinity), `BoundProcedure`
(procedure, ABI identity, entry, ownership of image/session), `Invocation` (budget, cancellation, error state,
reentry, opaque host context). Control API: `prepare_project`, `create_session`, `enumerate_procedures`,
`bind_procedure`, `reset_session`, `close_session`; a failed replacement leaves the old project active.

**Descriptors**: a language signature (identity, kind, visibility, resolved types, ByVal/ByRef, Optional,
defaults, Missing, ParamArray, array rank/element, nominal class/interface/UDT) plus a lowered ABI signature
(target, calling convention, widths, hidden session/context/receiver/LCID/result parameters, ownership and
writeback, signature fingerprint, entry forms). Names serve diagnostics and rebinding only.

**Entry profiles**: a typed native entry (`CvStatus (CV_CALL*)(session, invocation, double a, double b, double*
result)`), reaching typed execution, not a Variant frame; a bound typed-frame entry for the interpreter (offsets
fixed at bind time); an explicit, separate Variant adapter for dynamic calls. Wasm host calls use build-time
generated adapters; an unsupported shape fails binding.

**Host-member plans** keep interface identity, member and accessor kind, logical slot, language and ABI
signatures, ownership/writeback and error plans; DISPID, accessor and slot are separate tokens (FUNCDESC). The JIT
emits a typed indirect vtable call through the receiver; pure dispinterfaces use a compiler-bound Invoke plan,
distinct from a fallback.

**Execution results**: `Success` (which may carry `CVErr`), `UnhandledVbaError`, `ForeignCallFailure`,
`Cancelled`, `BudgetExceeded`, `StaleBinding`, `HostExecutionAbort`. Capture `IErrorInfo` promptly. Backend
fallback is safe only before execution begins. Sessions are thread-affine and nonconcurrent by default; reentry
is a capability; synchronous ByRef events need synchronous callbacks with nested invocation state.

**Registration metadata in the glue**: types from the compiler; reference-versus-value demand from signature plus
measured rules; worksheet eligibility by host policy; affinity defaults to the VBA lane; purity conservatively
stateful; volatility from runtime requests; host writes refused in worksheet context; WorksheetFunction methods
bound to CalcFunc operations once; genuine Variant signatures (`Range.Value2`, `WorksheetFunction.Sum`) kept.

**GUID policy**: DNA's own LIBID, CLSIDs, ProgIDs and IIDs, with an explicit Excel source-compatibility mapping
(`Dim r As Excel.Range`); an external client that requires Excel's original interface needs a separately specified
facade implementing that interface's full contract, without registering under Excel's Application CLSID.

**First three steps**: (1) specify the neutral ABI and build a non-spreadsheet host fixture (counter/calculator
with a typed method, a property, a failing method, a synchronous callback); (2) prove and measure the complete
typed paths: formula -> `Add2(ByVal Double, ByVal Double)` -> typed JIT; VBA -> generated typed host method in
interpreter and JIT; `Range.Value2`; ByRef alias, failed HRESULT, synchronous callback, reload with an active
lease; browser typed-frame path; (3) extract the spreadsheet model into DNA Calc and migrate registration
transactionally, widening the object surface only after both runtimes and targets support the mechanism.

## G. Function research: placement and the per-function claim record (GPT-6 Pro search answer)

Four owned areas (search answer 9.1): observation bank (immutable Excel observations with profile, transport,
provenance, exposure history); numeric semantics (versioned Fpu contracts, backend adapters, conformance vectors);
research tooling (grammars, enumeration, exact preimages, solver queries, traces, first kills, probe selection);
product library (the selected executable graph, compact regression witnesses, published fallbacks with honest
status). Research depends on the cleared product interface; the product never depends on search machinery,
rankings, answer tables or learned policies. `xtask` is the command surface, reusing the loader and verdict
encodings; a narrow research interface to private Fpu operations, never a copied substrate.

The claim of an identification is "the unique surviving executable model within grammar G, under profile E and
scope D, passing the specified independent validation", not Excel's source program.

**Per-function claim record** (search answer 9.2): function, surface, aliases, selected elements, domain
predicate; Excel build/architecture/CPU and compatibility version; transport, coercion and publication contract;
claim strength (published fallback, conditional hypothesis, unique grammar survivor, independently validated model,
proved equivalence); program, grammar, numeric-model and evidence hashes; enumeration bounds and coverage; helper
dependencies and assumptions (versioned, so a changed helper makes dependent claims stale); constants and
provenance; surviving alternatives; first-kill records and family certificates; discovery/validation exposure;
frozen questions and predictions; validation results; product mapping and replay results; the next experiment
for each material uncertainty. "Retained after rollback" never means "identified".

## H. Round-2 changelog


- 0: per-cell group membership, dependent-with-text failure (Opus, Fable); names and INDIRECT-R1C1 wrapping
  measured, names move absolutes only (GPT, Gemini); xlsb member and save-splicing detail (Gemini).
- 1.1: full `HostQuestions`, WhatIf drops random and functions (Gemini); read purposes, RTD as external updates,
  fact versions (GPT). 1.2: block membership and save eligibility (all four); load coalescing, per-table `Arc`s
  (Fable); names as reference-or-value with guarded memo (all four); watches by `(scope, name)`, tables in the
  namespace, persisted name order (Opus); closures own their plan, name save via file form, axis replays (Gemini).
- 1.3: command-scoped demand, effectful units not retried (GPT); VBA internal reads not edges (GPT, Gemini);
  volatiles under `merge_edit`, completion points, presentation-only invalidation (Opus); open demand (Fable).
- 1.4: budgeted parallel levels (Gemini, Opus); event-loop yield (GPT); asupersync spike off the critical path (Opus).
  1.6: wire ordering fields (GPT); transport trait out of the page (Gemini).
- 1.7: OxVba 0ad2ad28 state, typed-slot entry, typed JIT entries (Opus, Fable, owner note); Range descriptors
  (Gemini, Opus); `.tlb` pipeline (GPT); runtime composition (Opus, GPT); spreadsheet.rs freeze (Fable).
- 1.8, decision 5: hardware x87 always natively, vendor as provenance (owner note, Opus, Fable); soft
  transcendentals approximate today, browser cost stated (Gemini, Opus).
- 1.10, decision 11: function metadata defined once in `calc-func-catalog` (owner question; Gemini's table list).
- S2: insertion cost restated, 1 + 2r withdrawn as general (GPT). S13-S14: parser and binder in calc-syntax
  (Fable, Gemini); save under blocks (Gemini). 3: parallelism claim corrected (Opus, Gemini); H reopened (Opus).
- 4: steps split into 0a/0b, 1a-1e, 2a/2b, 3a/3b-1/3b-2, C1-C4, U1-U3, V0-V4, R1-R2; codec half of blocks now;
  `SchedUnit` (Opus, Fable, GPT, Gemini); counter checks to sweep (Opus); measurements first (GPT).
- 5.2 final positions recorded; D in GPT's command-scoped wording. 6: 1 composition, 3 by lane, 4 reworded,
  5 restated, 8 widened (Fable, GPT), 9 GUID question and `FUNCDESC` (Opus, GPT), 10 scoped (GPT, Fable), 11 new.
- Rejected: Gemini(r2)'s per-commit `Cargo.toml` check (structural checks run periodically; the
  manifest-triggered check covers it); Gemini(r2)'s "drop names from decision 8" only in part (cell routes open).
