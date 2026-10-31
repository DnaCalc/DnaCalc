# Question for GPT-6 Pro: red-team the DNA Calc target architecture

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet (native Windows/Linux and
browser via wasm), aiming at Excel's exact observable behaviour (bit-identical function results,
real xlsx/xlsb files, LAMBDA/UDFs/VBA/add-ins), several UI skins over one functional model, and an
architecture ready for extreme flexibility and optimisation (visible-first, cancellable,
incrementally published recalculation; large numbers of defined names; Excel's own limits as the
first scale target). Four models (Claude Opus 5.5, Claude Fable 5.1, GPT-Astra 6, Gemini 4 Argon)
each drafted a target architecture, reviewed each other twice, and the result was synthesised
(Appendix A). The owner's principles and rulings are in Appendix B (sections 4.1-4.8 override
everything). Appendices C-H are the evidence the models worked from: a foundation risk audit of the
code as built, early design ideas recovered from the project's history, and a library survey.

You did not take part. Your job is adversarial: find where this architecture will fail or
quietly force a costly redesign later.

1. **Seam pressures missed or mishandled.** Where two concerns meet and the agreed design will
   break: Excel calculation semantics (volatility, iteration, data tables, spills, dynamic arrays,
   LAMBDA, INDIRECT/OFFSET, external links, RTD, calculation modes) against visible-first,
   incremental, cancellable, parallel recalculation; shared-formula blocks against structural
   edits, sparse membership and saving; defined names as graph nodes (reference-valued names,
   scope, LAMBDA closures) at large counts; the explicit evaluation context against performance;
   early-bound VBA (JIT, COM type libraries, reload) against portability and the engine-free
   OneCalc front end; one command model against six skins and agent customisation; the binary
   viewport protocol against accessibility and testing.
2. **Wrong or unproven claims.** Anything in Appendix A that is internally inconsistent, contradicts
   Excel's documented behaviour, or rests on an assumption nobody measured. Name the measurement
   that would settle it.
3. **Ordering risk.** Which step in the implementation order (Appendix A section 4) is most likely
   to stall or to be built twice; what should move earlier or later; what is the smallest first
   slice that proves the recalculation contract end to end.
4. **What is missing** that a team building a spreadsheet engine at this ambition would regret
   not deciding now (data model, undo/redo and transactions, persistence, collaboration hooks,
   determinism and replay, observability/debugging, security of UDFs and add-ins, testing at
   corpus scale).
5. **The owner decisions** (Appendix A section 6): for each, agree or disagree with the
   recommendation and say why in one or two lines.
6. **Concurrency foundation.** Assess the position on asupersync (an async runtime with structured
   concurrency and cancellation checkpoints, used only as an optional native adapter behind our own
   scheduler interface with our own serial path always present) against alternatives, given a
   wasm browser target, determinism needs and Excel's ordering semantics.

Be concrete (types, invariants, failure scenarios with inputs). Rank findings by how expensive they
become if discovered late. Skip praise and restatement.

---

# Appendix A: SYNTHESIS.md

# DNA Calc target architecture: synthesis (final, after round 2)

Date 2026-10-31. Inputs: BRIEFING.md (owner rulings 4.7 and 4.8 override everything), four drafts, four
round-1 reviews (`reviews/`), four round-2 reviews (`reviews2/`), foundation-ideas.md, the risk files.
Code: DnaCalc-review **5a9348b** (`git show`; HEAD dbcc791 adds `docs/plan/SYMBOLIC_SEARCH.md`), OxVba
**0ad2ad28** (2026-10-31 16:38) for CalcVBA state; calc-vba still pins c7ce1349.

Attribution: **Opus**, **Fable**, **GPT** (GPT-Astra 6), **Gemini** (Gemini 4 Argon); (d) draft, (r)/(r2) review
rounds. **[4.7]**/**[4.8]**: an owner ruling decided against a position. Section 7 lists round 2's changes.

## 0. What Excel persists for formulas (fact-check)

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

## 1. Consensus architecture

### 1.1 Layers and explicit contexts (all four; owner 4.1)

| Boundary | Context (every field required, no default methods) |
|---|---|
| func <- fml: `FunctionContext` | numeric mode; `LocaleContext`; date system; caller from the instance; value reader; `ReferenceTextResolver`; `VolatileServices`; `CallableInvoker`; `NumberText`; `HostQuestions` answering everything `HostFacts`/`WorkbookFacts` answer today (`call.rs:113-216`): width, prefix, format class, locked, color, parentheses, file name, `is_external_sheet`, `is_formula`, `formula_text`, sheet index/name/count, active cell, top-left cell, calc mode, application (Gemini(r2)) |
| fml <- engine / OneCalc / CF-DV / data-table scenario: `FormulaEvaluationContext` | the above plus `ReferenceCatalog` (sheets, names, tables, one matcher), `CallableTable` snapshot, `EffectSink`, `EvalControl` (budget, cancel), and a **read purpose** per read: formula dependency, UDF callback observation, diagnostic (GPT(r2)) |
| engine <- host: `CalcContext` | `SemanticEnv` (date system, locale, compatibility, numeric mode, iteration) apart from tick services (clock, seed: never dirty non-volatiles); RTD and async completions are **sequenced external-update operations** with provider/call generation, not tick services (GPT(r2); plan `DNA_CALC_REIMPLEMENTATION_PLAN.md` external-links row); facts snapshot with versions; typed CF/DV |
| host <- platform | clock, application facts, file identity, resolved locale, font metrics (OffscreenCanvas in the worker), thread policy, runtime providers |
| model <- skins | commands, availability, projections, `ViewStateStore`, focus registry |

Absent services are typed `Absent(Owner)` refusals. Decorators override one service: today `WhatIf::evaluate`
builds services from mode, reader and catalog only, dropping host facts, random and functions
(`calc-engine/src/data_table.rs:248-253`). Forwarders and the `call.rs:385-398` fallback go in one change.
Functions are identified by `FunctionId` (1.10); fact reads record versions for validation (GPT(r2)).

### 1.2 Representations

**Formulas [4.8 overrides stable axis keys: Opus, GPT(r), Gemini(r), foundation R10].**
- `Template { id, kind: Cell | Name | Rule, tokens }`, R1C1-normalised with `AxisRef`; the kind carries
  placement (cells: off-grid `#REF!` pending measurement; names: wrap from A1) and structural rules (names
  move absolutes only). Sparse `SpellingOverride` where authored text differs from canonical A1.
- `FormulaBlock { id, sheet, extent: GridArea, membership: Dense | Runs | Bitmap, template, kind: Shared |
  Fill | LegacyArray | Single }` over plain coordinates (membership: Opus(r2), Fable(r2)). A member is
  `(BlockId, offset)`; identity is the address at the current revision. Bound plans live in fml/engine
  caches, never in the model. `BlockId` is not `NotebookBlockId`.
- Load: shared groups become blocks with their membership; runs of equal-template `normal` formulas from
  other writers coalesce into blocks at load (Fable(r2)); a dependent with text is tolerated. Save writes a
  block back as a shared group when it is eligible (same template, `si` only on members), rewrites a master's
  A1 text after a shift without unsharing, and otherwise writes individual formulas; legacy arrays, dynamic
  arrays and data tables keep their own forms (GPT(r2), Gemini(r2)).

**Defined names: first-class non-cell nodes, in large numbers [4.8].**
- `NameTable` in the model: `NameId`, `scope: Workbook | Sheet(SheetId)`, folded key, `Template(kind: Name)`,
  attributes, `binding_version`, persisted notebook order (custom XML part; workbook.xml has none, Opus(r2)).
  Hashed lookup by `(scope, folded name)` (today a linear scan per bind, `calc-engine/src/read.rs:222-226`);
  tables share the namespace and the folded index, with `TableId` binding (`reference/bind.rs:44-48`, Opus(r2));
  per-table `Arc`s instead of cloning all `Definitions` on any format, pane, rule, iteration or name edit
  (`calc-doc-model/src/root.rs:657-683`; Fable(r2)).
- Formulas bind to `NameId`, not to inlined text (today a name edit clears all templates, `edit.rs:609-610`).
  A LAMBDA held in a name must survive this: `Closure` stores its plan's address and the evaluator refuses a
  closure from another plan (`eval/closure.rs:20-29`, `eval/run.rs:961-963`), so closures own an
  `Arc<CompiledPlan>` and the evaluator switches instance frames (Gemini(r2)).
- **Definition vs evaluation** (Opus, Fable, GPT, Gemini (r2), all agree): Excel evaluates a name per referring
  formula and not at all when unused; today the name block runs at each use (`run.rs:1000-1015`). A name
  node is the definition plus binding version. Its **result is reference-or-value**: a reference-valued name
  gives readers edges to the referenced cells through the name's version; nothing materialises `$A:$A`. A
  shared memo exists only for a value-valued name that is caller-independent (no `AxisRef::Relative`, no
  unqualified `SheetTarget::Anchor`, `reference/mod.rs:139-146`), pure (no volatile, stateful or caller
  facility: ROW, CELL, INDIRECT, RAND, UDFs), keyed by scenario identity; the memo holds the uncoerced
  value, and top-level coercion happens only at a cell root or notebook projection (Gemini(r2)).
- **Negative-lookup watches** keyed by `(scope, folded name)`, covering qualified `Sheet2!Foo` from other
  sheets (Opus(r2)), deletion, rename, tables and UDF registration (GPT(r2)).
- Name save uses the file-form pass and can delete names (Gemini(r2)). No other non-cell node kind **[4.8]**.

**Values, styles, IDs.** Engine results columnar per block; authored pages unchanged until measured.
`StyleRuns`; `LoadedSheet.styles` and the other `axis_edits` replays (merged areas, validations, views,
`calc-host/src/state.rs:621-754`, `facts.rs:182-217`) fold into the model (Gemini(r2)). Excel grid limits
**[4.8]**; `PartitionId`, `InputView` as types only.

### 1.3 Recalculation contract

```rust
pub struct RecalcRequest { mode: CalcMode, command: Auto|CalcNow|CalcSheet|CalcRange(AreaSet)|Full|RebuildFull,
                           tables: Include|Defer, demand: DemandSet, interest: InterestSet /* versioned */ }
fn update_interest(&mut self, interest: &InterestSet);   // reorders; discards nothing
fn merge_edit(&mut self, change: &PreparedEngineChange); // new tick for volatiles; validated reuse
fn step(&mut self, budget: WorkBudget, cancel: &CancelToken) -> Progress;
fn drain(&mut self) -> Publication;                      // { revision, tick, seq, units }
pub enum CellState { Settled { computed_at, valid_for }, Pending { last }, Refused(Owner), FileCached }
```
- **Demand** comes from mode, command, target and forced-vs-dirty evaluation; Automatic Except Tables and
  range/sheet calculation leave work pending by design (`calc-host/src/recalc.rs:74-82`; GPT(r2)). Interest only
  orders ready work inside that demand. Freshness outside the command's scope is reported, not forced.
- **Publication unit** (R1/R2): value, observed edges, shape and spill reservation commit together, validated
  against read/binding/topology/effect versions. `Settled` = no dirty node and no unresolved may-spill or
  dynamic-reference root in the backward cone, within the demand scope. Clean prerequisites keep their own
  tick (`calc-engine/src/read.rs:435,455`).
- **Effectful units** are admitted and committed, never validated-and-retried: a unit calling a stateful UDF is
  serialised against conflicting edits and reloads, and a suspended activation is preserved, not replayed
  (GPT(r2)). Today a read of an uncalculated cell discards the evaluation and re-runs it later
  (`calc-engine/src/recalc.rs:607-610`), which replays VBA side effects (Gemini(r2)).
- **VBA reads**: argument references follow measured rules; internal reads of cells outside the arguments are
  UDF callback observations, not dependency edges (Excel kept the old value until explicit calculation,
  `docs/plan/VBA_UDFS.md:91-93`, `probe.bas`, `observations.json`; GPT(r2), Gemini(r2)). An uncalculated
  prerequisite is evaluated synchronously on demand with cycle checks, without recording an edge.
- **Volatiles under edits**: `merge_edit` re-dirties the volatile set under a new tick, since RAND keys on (seed,
  tick, cell) (`random.rs:4-8`; Opus(r2)). **Spill shadow**: per-sheet may-spill index, conservative reach.
  **Unsplittable units**: SCCs, data-table runs, ordered effectful groups; atomic is not uninterrupted (GPT(r2)).
- **Completion points**: save, `Application.Calculate` and a macro's `Range.Value` demand the relevant scope and
  drain at one tick (Opus(r2)); save writes pending cells under an explicit file-cache policy (GPT(r2)).
- **Open**: volatiles recalculate at open, everything under `fullCalcOnLoad`; file values stay `FileCached`
  until settled (Fable(r2)). **Cancel means yield**, never inside a live x87 sequence; ready counts and a height
  heap; early cutoff only in cones without effectful callables (UDFs are effectful by default,
  `VBA_UDFS.md:78-80`); `SetInterest` journalled for replay.
- **Presentation-only invalidation**: resize, zoom and font changes re-render tiles (General digits, `####`)
  without recalculating; today cells render at standard width (`apps/dna-calc/src/cells.rs:53`; Opus(r2)).

### 1.4 Concurrency, scheduler and asupersync [4.7]

- `calc-sched`: executor traits, `CancelToken`, `WorkBudget`, checkpoints, lanes, seeded scheduler. Layer 0 is our
  serial stepping executor (reference twin, browser default). Native pool at block/chunk grain; **parallel levels
  must accept a finite budget and resume** (today parallel runs only with `WorkBudget::UNLIMITED` on the first
  step, `recalc.rs:644`, and needs no iteration, cycles, staged spills or data tables and a silent observer,
  `:742-757`; a waiting array serialises the rest, `:710-727`; Gemini(r2), Opus(r2)).
- The browser actor **returns to the worker event loop** between slices; a test delivers an edit and an interest
  change mid-calculation (GPT(r2)). Owner-lane mailbox for thread-affine callables; no lock across user code.
- **asupersync [4.7 overrides GPT(d); GPT withdrew]**: permitted, rider in notices, a native-only adapter behind
  `calc-sched`; serial executor always compiled; never browser or inner loop. The actor uses calc-sched anyway;
  the spike runs off the critical path (RTD, async UDFs, lab replay; Opus(r2)), adopted on measured benefit (GPT(r2)).
- Distribution designed only (`InputView`, `PartitionId`, frontiers) **[4.8; overrides Gemini(d) Phase 5]**.

### 1.5 Command and functional model; skins (owner 4.2)

- Host commands in calc-host (typed families; Excel rules from `apps/dna-calc/src/server.rs:597-1350, 1636-1815`);
  the host command enum is also the session actor's inbox. UI model DOM-free: registry with availability,
  `Target::{Cells(AreaSet), Name(NameId), Sheet, Object, LayoutItem}`, `EditSession` on registered surfaces,
  `ViewStateStore<SkinId>`, focus registry, `view.reveal`; shell keymap as data; skin contract `mount`,
  `focus_target`, gestures, `view_state_schema`.
- **Notebook over cells or names [4.8]**: `NotebookSource::{CellSequence, Names { scope, filter }}`; switching
  changes the projection only (migration is a separate command); a shown name evaluates at an explicit anchor
  through normal demand, so inspection adds no RAND or UDF evaluations (GPT(r2)).

### 1.6 Wire and transport

Control envelope (commands, receipts, Calculate completion: ordered, reliable) apart from binary `TileBatch`:
per 32x8 tile, tag `u8`, `f64`, string id, style id, state; versioned dictionaries; formatted text produced in
the worker. Every batch carries session id, model revision, interest version, publication sequence and
dictionary generation, and is ordered after the edit receipts it reflects, so a pre-insert tile cannot repaint a
moved cell; gaps trigger snapshot reset; old dictionaries are retired explicitly (GPT(r2)). Page: tile mirror,
one signal per tile; tests on a semantic layer and RenderPlan. A **transport trait** lives in `dna-calc-model`:
the browser links only the worker transport; native tests inject the in-process server from calc-harness
(today `Transport::InProcess(Box<Server>)` sits in the page crate, `apps/dna-calc/src/port.rs:44-48, 154-159`;
Gemini(r2)).

### 1.7 CalcVBA hosting and early binding (owner 4.5)

- **State at OxVba 0ad2ad28** (corrects round 1, which read c7ce1349; Opus(r2), Fable(r2), owner note):
  `ComCallEarly` now carries the compiler's descriptor (IID, slot, wire signature) to
  `ComHal::invoke_early_bound` in VM3 (`oxvba-vm3/src/lib.rs:2181-2192, 4258-4300`) and the JIT
  (`oxvba-jit/src/helpers.rs:4397-4404`); native x64 dual members go to vtables with no Invoke retry;
  `docs/spec/HOSTING_AND_EARLY_BINDING.md` is the hosting contract. Remaining: per call a `Vec<DynamicCallArg>`
  of Variants and a `HashMap` (`lib.rs:4264-4287`), libffi for Windows vtables (`windows_vtable.rs:1-10`), a JIT
  helper call, Variant-backed JIT frames (CORE-7). The commit also adds 186 lines to `spreadsheet.rs`.
- Target: `calcvba-host-api` = OxVba's host-neutral interface, **with a typed-slot entry from the start**:
  calls into DNA's in-process `#[repr(C)]` objects with typed arguments, no Variant vector, no libffi, emitted
  as direct calls by the JIT; typed JIT primary entries for procedures; formula -> procedure through
  `CallableTable` slot + generation and a typed shim per signature (no intermediate `Value`).
- **Object library** in `calc-object-model`: descriptors -> generated vtables on every platform; UDF-time
  backend (reads, refuses writes, inside OneCalc's ceiling) and macro-time backend above calc-host. `Range`
  handles are value descriptors (`SheetId`, area) resolved against the active invocation, refusing when none
  is active, so a Range kept in module state outlives its call safely; allocation-free per call; no 4,096 cap
  (Opus(r2), Gemini(r2)). Windows: windows-core implements interfaces but does not generate `.tlb`; the
  pipeline is descriptor -> IDL/MIDL or `CreateTypeLib2`, with HRESULT/retval, optional arguments, BSTR and
  SAFEARRAY ownership, apartments and reentry specified (GPT(r2)).
- **Reload**: new generation, code leases, lazy rebind, state restarts, ordered against effectful units (1.3).
- **Composition**: calc-* libraries depend only on `calcvba-host-api`; each app binary (`dna-calc-worker`,
  `dna-onecalc`, native shells) has one composition edge to an OxVba runtime facade that supplies the provider
  factory (native: linked VM3 + JIT; browser: interpreter, wasm emission later), with ABI version negotiation
  and allocator ownership stated (Opus(r2), GPT(r2)).

### 1.8 Exact numerics and research (owner 4.4; owner note in round 2)

- **Excel follows the CPU it runs on.** Natively on any x86-64 hardware x87 is the Exact path; vendor and model
  are provenance on results and witnesses; witnesses and published claims are per vendor (AMD measured). Replaces
  round 1's vendor-match rule (Opus(r2), Fable(r2), owner note).
- **Soft path** (browser, non-x86): soft x87 is exact for arithmetic, loads, stores, remainder; its
  transcendentals (`F2XM1`, `FYL2X`, `FPATAN`, `FSIN`, `FCOS`, `FPTAN`) round once where microcode rounds wider,
  so they **approximate today, they do not refuse** (`calc-func/src/ported/excel_numeric/mod.rs:9-15`;
  Gemini(r2)). Those instructions carry `^`, POWER, EXP, LN, LOG, trig and the financial functions. Under Exact
  they refuse (counted as fails) until a vendor microcode model is identified; decision 5.
- `calc-numeric` (product) holds the substrate (hardware and soft backends, `ExactDecimal`, digit generation);
  per-function `ProgramStatus::{Identified, Unidentified}`. Research in `tools/calc-func-lab` on calc-numeric
  and calc-verdict, outside the product closure; `cargo xtask` verbs may forward to it as a subprocess (GPT(r2)).

### 1.9 Platform profiles

| | Windows | Linux | Browser |
|---|---|---|---|
| Recalc | serial + budgeted native pool | same | serial worker yielding to the event loop; optional SAB pool |
| Exact | hardware x87, vendor as provenance | same on x86-64; Soft elsewhere | Soft; transcendentals refuse under Exact |
| VBA | VM3 + JIT; typed slots; real COM refs | VM3 + JIT; in-process objects | interpreter; wasm emission later |
| Add-ins | XLL, COM, WIT | WIT | WIT via jco |
| asupersync | optional adapter | optional adapter | never |

### 1.10 Function metadata, defined once (owner question after round 2)

- **Facts (verified).** `calc-func/src/catalogue.rs:1-11` is already a compile-time generated declarative
  catalogue with explicit stable `FunctionId`s; duplicate IDs or surfaces fail the build. Metadata is still
  duplicated: `calc-func/src/file_prefix.rs`; `calc-doc/src/save/file_form_table.rs` (measured prefixes and
  array positions); the nine-name `VOLATILE_FUNCTIONS` (`calc-fml/src/eval/mod.rs:190-201`); SUBTOTAL/AGGREGATE
  found by spelling in the engine; `_xlfn` literals in about ten product files (calc-doc-model, calc-doc load,
  save and xlsb, `calc-engine/src/read.rs`, calc-fml assist, builtins, compile, legacy and parser, calc-host
  session); the 3-D, legacy-place and lifting tables (S13); the xlsb Ftab (`xlsb/functions.rs`); `help.rs`; and
  the separate ExcelFunctionsHandbook repository.
- **Design.** `calc-func-catalog`, data-only at the bottom (calc-value only), holds every declaration per
  `FunctionId`: surfaces (current, legacy, prefixed), file prefix, xlsb Ftab index and fixed arity, parameters
  and argument demand, 3-D admission, legacy intersection place, reference-array lifting, volatility, effect
  class and declared context facilities, subtotal/aggregate role, measured file-form positions, help key. The
  existing generator moves with the declarations. calc-func attaches kernels by ID, and its generator fails the
  build when an ID has neither a kernel nor an explicit typed refusal. calc-syntax, calc-doc, calc-fml,
  calc-engine and calc-host consume IDs and catalogue fields, never spellings; name -> ID resolution exists
  once. Refinement: `_xlpm.`, `_xleta.`, `ANCHORARRAY` and `SINGLE` are syntax roles, not function facts, so one
  prefix reader/printer in calc-syntax applies them from catalogue fields; runtime callables (UDF, XLL, VBA)
  stay in `CallableTable`, not the catalogue.
- **Evidence and docs.** The measured prefix and array-position fixtures remain the single Excel check of
  those fields. Signature help, UI help and the handbook are generated from a catalogue export, with
  Microsoft's text stored once, keyed by ID, with source links.
- **Kept so by construction**: only the catalogue crate can construct a `FunctionId`; derived tables are
  generated; a periodic sweep check (not per commit, 4.6) flags function-name tables or `_xlfn` literals
  outside the catalogue and the prefix printer. Decision 11.

## 2. Seam pressures and the agreed response

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

## 3. Target crate map

| Crate | Owns | Depends on |
|---|---|---|
| calc-value | values, ids, grid constants | - |
| calc-func-catalog (new) | every function declaration (1.10); the only `FunctionId` constructor; generator | value |
| calc-syntax (new) | lexer, parser, tree, templates, A1/R1C1 rendering, prefix reader/printer, reference binding and transforms | value, catalog |
| calc-numeric (new) | x87 substrate (hardware, soft), `ExactDecimal`, digits | value |
| calc-format (open, 5.2 H) | number formats, `NumberText` impl | value, numeric |
| calc-func | kernels attached by ID, coercion, `FunctionContext`, `NumberText` trait | value, catalog, numeric |
| calc-fml | plans, FEC, evaluator, `CallableTable`, LAMBDA, name evaluation (formatting here if no crate) | func, catalog, syntax |
| calc-doc-model | pages, templates + blocks, `NameTable`, `StyleRuns`, typed CF/DV, per-table `Arc`s, ops + inverses | value, syntax |
| calc-doc | codecs; blocks <-> shared groups; Rgce -> templates | doc-model, syntax, catalog |
| calc-sched (new), calc-sched-asupersync (optional, native) | executors; adapter [4.7] | - ; sched + asupersync |
| calc-engine | edges, member sets, height heap, units, results, CF/DV and name participants, `InputView` | fml, doc-model, sched |
| calc-host | actor, command enum, journal, presentation, projections, save | engine, doc, sched |
| calc-object-model (new) | descriptors, vtables, `ObjectServices`, UDF-time backend | fml, value |
| calc-vba | registration, typed shims, leases | fml, object-model, calcvba-host-api |
| calc-host-vba (later) | macro-time backend, events | host, vba, object-model |
| dna-formula-ui; dna-calc-model (+ transport trait); dna-calc-protocol; dna-calc-worker; dna-calc | shared editor; model; wire; translator + runtime composition; shell + skins | dna-calc never sees host/engine |
| dna-onecalc | model, fixture, front end, runtime composition | fml, vba, object-model, formula-ui |
| xtask (thin), calc-harness, calc-verdict, tools/calc-func-lab | scans; harness and in-process transport; verdicts; search | xtask: no product crates |

One `architecture.json`. Kept: `Arc` pages, generational IDs, `RecalcWork`, per-result `Calculation`, prefix-sum
geometry, touched-plus-dependents check, captures, array-aware parallel levels (`recalc.rs:835-878`), `random.rs`.

## 4. Implementation order (split into landable steps)

Every step lands on master in working pieces; both apps keep working; no time estimates; five lanes after 0a.

**Tools.** 0a thin xtask, smoke cases beside the apps with `--case`. 0b deletions (receipts, retired CI, frozen
plans) any time (GPT(r2)). Counter-slope checks in sweep, or in check only if seconds (Opus(r2); 4.6).

**Engine lane.**
- 1a `SemanticEnv` vs tick services: the clock fix (`state.rs:782-788`). Ships: clock edits recalculate volatiles only.
- 1b calc-func-catalog (1.10): declarations split from kernels, every duplicate table folded in, consumers moved
  to IDs, help export. Ships: spelling lists, `file_prefix.rs` and calc-doc's table deleted.
- 1c FEC in one change: forwarders, fallback, caller from the instance, data-table decorator with all services,
  read purposes. Ships: ROW() in OneCalc, CELL/SHEET/RAND in data tables.
- 1d `CallableTable` slot + generation, per-binding versions, negative watches (with V2).
- 1e formatting out of calc-func; presentation resolver; typed CF/DV decoding; CF overlay. Ships: grid and CELL agree.
- 2a `NameTable`, `NameId` binding, hashed lookup, tables in the namespace, per-table `Arc`s, closures owning
  their plan, name save via file form (needs C1). Ships: a name edit no longer recompiles everything.
- 3a (needs U1's command enum) actor on calc-sched, budgeted steps that yield to the event loop, budgeted
  parallel levels, `update_interest`, cancel as yield, O(workbook) costs removed (`recalc.rs:552, 1018, 1363`,
  `project.rs:211-214`, CF full resets), open-time demand; whole-recalc publication kept.
- 3b-1 certificates and scalar publication units, atomic fallback for unsupported units with its frequency counted;
  synchronous prerequisite evaluation for VBA reads; atomic-generation tests replaced. Ships: visible cells first.
- 3b-2 may-spill shadow, SCC/table units, effectful-unit admission, external updates, ready counts and height heap,
  journalled interest, early cutoff for pure cones, completion points for save and macros.
- 2b name evaluation nodes (reference-or-value, guarded memo) after 3b-1, since they are publication units.
- 5b engine blocks (needs C2, 3b-1): affine edges, member bitsets, result columns, straddle splitting,
  optional indexes, scheduled as `SchedUnit = Node | Chunk` so 3b is not rebuilt (Opus(r2)).

**Codec lane (starts after 0a; decision 3's urgency).** C1 calc-syntax extraction (parser, tree, reference
binding and transforms). C2 templates and blocks in the model with membership; groups load as blocks; load-time
coalescing; dependent-with-text fix; xlsb Rgce to templates while keeping converted parts; the engine still
expands members through `InputView`. C3 save diffs blocks directly: masters rewritten after shifts, eligibility
rules, array/table forms kept. C4 `StyleRuns` and folding the remaining `axis_edits` replays.

**UI lane.** U1 (4a) host command enum (also 3a's inbox), `dna-calc-model`, shell keymap, neutral `Target`,
skin contract, transport trait, UI/worker crate split. U2 (4b) `TileBatch` with ordering fields, tile mirror,
RenderPlan, semantic tests, presentation-only invalidation. U3 renderer comparison (5.2 G). Exit for U1: a new
skin is two edits; every skin has undo, find and F9.

**CalcVBA lane.** V0 now: no new member in `oxvba-embed/src/spreadsheet.rs`; bump calc-vba's pin to 0ad2ad28;
adopt HOSTING_AND_EARLY_BINDING.md as the hosting contract (Fable(r2)). V1 `calcvba-host-api` with the
typed-slot entry and the runtime composition facade. V2 typed JIT primary entries (CORE-7) and formula ->
procedure shims (with 1d), in parallel with V3 (Opus(r2)). V3 typed-slot calls into DNA objects without Variant
boxing or libffi, proven first with one Range callback through VM3 and JIT including ownership and reentry
(GPT(r2)); then calc-object-model widened, spreadsheet surface removed. V4 `.tlb` pipeline and Windows COM
exposure, after the decision 9 observations.

**Research lane.** R1 calc-numeric now, witness commands kept by delegation (GPT(r2)). R2 `tools/calc-func-lab`
per SYMBOLIC_SEARCH.md, vendor provenance on witnesses.

**Measurements** (decision 8) precede what they inform: module edit before V1; off-grid cells and non-members
inside `ref` before C2/C3; UDF order and name evaluation multiplicity before 3b-2 and 2b.

## 5. Open disagreements

### 5.1 Settled by evidence or ruling

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

### 5.2 Final positions on the remaining choices

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

## 6. Decisions for the owner

1. **CalcVBA dependency.** calc-* libraries depend only on `calcvba-host-api` (OxVba's host-neutral interface,
   with a typed-slot entry); each app binary has one composition edge to an OxVba runtime facade (provider
   factory, ABI version, allocator ownership); the browser instantiates the interpreter. Recommend yes. (All
   four; composition named by Opus(r2), GPT(r2).)
2. **Retire plan 2.3's atomic generation** for validated units with effect admission, as 3a, 3b-1, 3b-2.
   Recommend yes.
3. **Formula blocks now**: codec half (C1-C3) starts now in its own lane; engine half (5b) after 3b-1.
   Recommend yes.
4. **Names.** (a) `NameId` nodes separating definition from evaluation; results are reference-or-value; a shared
   memo only for caller-independent, pure, value-valued names, per scenario. (b) Notebook over cells or names;
   switching changes the projection only; name order persisted; inspection adds no evaluations. (c) CF/DV, spill
   anchors and data-table runs are engine scheduling units, not new node kinds. Recommend yes to all three.
5. **Exact (restated).** Excel follows the CPU it runs on, so natively hardware x87 is the Exact path on every
   x86-64 machine, with CPU vendor and model recorded as provenance; witnesses and published claims are per
   vendor (AMD measured so far). On the soft path (browser, non-x86) transcendental instructions approximate
   today; under Exact they refuse and count as fails until a vendor microcode model is identified, which makes
   browser Exact refuse `^`, EXP, LN, trig and the financial functions built on them. Recommend: accept that,
   keep Fast as the browser's interactive default, and fund vendor models as research.
6. **Program search location.** Recommend `tools/calc-func-lab` on calc-numeric and calc-verdict, with a
   forwarding `cargo xtask` verb, which satisfies "within xtask" without product dependencies in xtask.
7. **asupersync [4.7].** Native-only adapter behind calc-sched; spike off the critical path (RTD, async UDFs,
   lab replay); adopt on measured benefit; never browser or inner loop; rider in notices. Recommend yes.
8. **Excel measurements before the steps they inform**: VBA module edit in Automatic mode; off-grid relative
   references for cells via copy, fill and R1C1 entry (regress the measured name and INDIRECT cases); non-member
   cells inside a shared `ref` and overwriting a member; side-effecting UDF order along a calc chain; name
   evaluation multiplicity (`=MyRand-MyRand`, UDFs in names); saved-Range dependencies. Recommend yes, P1.
9. **COM compatibility.** Observe a running Excel through public COM (type library GUID, IIDs, inheritance,
   `FUNCDESC` with `oVft` and calling conventions, DISPIDs, dual views). Recommend yes. Owner question (Opus(r2)):
   should DNA's `.tlb` reuse Excel's library GUID, IIDs and DISPIDs so existing "Microsoft Excel 16.0 Object
   Library" references resolve? Reuse gives binary compatibility for compiled clients; it collides with Excel's
   own registration on machines that have Excel. Recommend: match IIDs, DISPIDs and slot layout; register
   under Excel's library GUID only in an explicit compatibility install.
10. **Pause new UI feature slices** (defect fixes continue) during 0a and U1, and **new members in
    `oxvba-embed/src/spreadsheet.rs`** from today. Recommend yes.
11. **Function metadata once** (1.10): `calc-func-catalog` as the only source of function facts and `FunctionId`s;
    kernels attached by ID with build failure on gaps; help and the handbook generated from its export; a
    periodic sweep check for stray name tables. Recommend yes, as step 1b.

## 7. Changelog: what round 2 changed (reviewer)

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

# Appendix B: BRIEFING.md

# DNA Calc target architecture review: briefing pack

Date: 2026-10-31. Audience: the three drafting models (Opus 5.5, Fable 5.1, GPT-Astra 6), their
mutual reviewers, and a final GPT-6 Pro red-team. Everything you need to start is here or in the
files this pack names. Read this whole file first.

## 1. What is asked

Produce a target architecture for DNA Calc that prepares it for **extreme flexibility and extreme
optimisation**, worked out first at an abstract level and then reflected down onto the current
Rust implementation (crates, boundaries, data representations), with an implementation strategy
that gets there from today's code. The value of this exercise is in the **non-obvious seam
pressures**: places where two concerns meet and the convenient design quietly breaks something
later. Generic layered-architecture advice is worthless here; ground every claim in the scope
below and in the evidence of what has already gone wrong.

Process: three independent drafts; each model then reviews the other two; a synthesis keeps
disagreements visible; one more review cycle; then a GPT-6 Pro red-team. Do not read other
models' drafts until you are asked to review them.

## 2. Product and scope

- **DNA Calc**: a re-implementation of an Excel-compatible spreadsheet in Rust, running natively
  (Windows, Linux) and in the browser (wasm in web workers, Leptos UI). Goal: Excel's observable
  behaviour, including bit-identical function results (see 4.4), opening and saving real xlsx/xlsb
  files, Excel's extensibility (LAMBDA, UDFs, VBA, add-ins), with several UI skins.
- **Long-term ambition (Jane Street "Webs"-class)**: distributed computation; incremental
  recomputation over billions of nodes and edges; strong typing and rich data structures;
  definition from Rust and from a web interface; a web formula debugger; performance at that scale.
  All still on Excel formulas, functions and extensibility. Until now this was a deferred epic with
  a rule against designing toward it; **for this review the owner explicitly asks the architecture
  to be ready for it**. Implementation stays Excel-scope unless the owner places more.
- **Preferential recalculation of what is visible**: the grid viewport and anything a skin
  subscribes to should drive recalculation priority; stale work should be cancellable when the view
  moves. Rendering speed depends on suitable representations all the way from UI state to engine.
- **DNA OneCalc**: an engine-free front end on CalcFml and CalcFunc only (formulas, number
  formatting, UDF integration). It is the honesty check that the formula/function layers stay
  simple and separate from the engine, which owns dependency-graph smarts and optimisation. It is
  not a reduced skin of DNA Calc and not a parallel product. DNA Calc is a strict superset.
- **Skins**: a strict skinnable model. All functionality, state and commands live in a skinnable
  functional model; skins (Grid, Atlas, Ledger, Canvas, Tree, Notebook, OneCalc's front end) only
  present and route. Purpose: trivially agent-customisable UIs that still present full
  functionality. Shared UI lives once in a library at OneCalc's dependency ceiling.
- **CalcVBA** (now repo OxVba): an independent VBA engine with interpreter and JIT, see 4.5.

## 3. The system as built (2026-10-31)

Crates: `calc-value` -> `calc-func` (about 500 Excel functions) -> `calc-fml` (parse, bind,
compile, evaluate formulas) -> `calc-doc-model` / `calc-doc` (document model, xlsx/xlsb codecs) ->
`calc-engine` (dependency graph, recalculation) -> `calc-host` (session facade, projections, edits)
. Apps: `dna-onecalc`, `dna-calc` (Leptos UI + web-worker `server.rs`), `dna-formula-ui` (shared,
ceiling calc-fml), `dna-strand` (design tokens). `crates/calc-vba` adapts OxVba. Tools: `xtask`
(check, sweep, capture, witness, site publishing), `calc-verdict`. Platform: `calc-oracle-excel`.
Tracker `br`; master-only; Codex workers on two Linux hosts and Windows.

The original plan is `C:\Work\DnaCalc\DnaCalc-review\docs\plan\DNA_CALC_REIMPLEMENTATION_PLAN.md`
(sections 2-8 are the intended architecture; compare it with the audits). Code is at
`C:\Work\DnaCalc\DnaCalc-review` (master 5a9348b); read code to check claims, read-only.

## 4. Owner principles and rulings for this review

4.1 **Explicit context, not convenient placement.** Formatting and presentation must not live in
calc-func. Where functions need such facts (CELL, TEXT, INFO, reference resolution for
INDIRECT/OFFSET, volatile services for NOW/RAND, user-defined function lookup), calc-fml passes a
Formula Evaluation Context with information and callbacks, at a suitable abstraction. The same
holds for the layers above calc-fml: boundaries are explicit, and the full context each layer
needs is passed to it, never smuggled by moving code to a convenient but architecturally
inconsistent place.

4.2 **Skins**: the app shell has drifted from the strict skinnable model (see risk-skins.md).
Course-correct; refactor the functional model and the top-level skins.

4.3 **Representations end to end**: grid performance and visible-first recalculation require
suitable representations from UI state through the worker boundary to the engine.

4.4 **Bit-exact functions mean Excel's algorithm, not a fit.** The target is an implementation that
is the same program as Excel's (assumed straightforward human-written code, possibly assembly):
expression tree, operation order, precision and store points, constants, branches. Found by
symbolic search over candidate calculation programs, constrained by recorded Excel answers
(OxFunc's `docs/SMART_SEARCH_AND_ACTIVE_LEARNING_LOOP.md` method). Counting matches and optimising
toward a higher match ratio is the wrong method; fitted bounds, coefficients or error-based switches
are fix-up tables and are being reverted. The architecture should give this research a proper home
(where candidate programs, precision models and witnesses live) without contaminating the product.
Excel versions are assumed stable for function results; every reading records build, platform,
CPU. Measured fact: on AMD Excel follows the CPU's x87 result exactly (15 of 15 inputs where Intel
and AMD hardware differ).

4.5 **CalcVBA and DNA Calc.** CalcVBA is a separate repo and engine with nothing
spreadsheet-specific (no "UDF" concept). It exposes the right kind of external interface, as Office
VBA presumably does: load code, enumerate public module procedures and their signatures, give the
host callable entry points. DNA Calc has a glue library that registers shims for those as worksheet
functions, and builds and registers its own COM type library (Application, Range,
WorksheetFunction) that VBA code uses as it would Excel's. Owner, verbatim in substance:
- the UDF call path must be an **early-bound, high-performance call** from CalcFml/CalcFunc
  invocation through to JIT-compiled code; do not settle for "getting the right number across";
- the DNA Calc object library exposed to VBA needs **full early-binding COM through to JIT code**
  where appropriate and compatible; late binding is not a start to improve later, experience says
  that builds the wrong system; if early-bound COM is not fully baked in both interpreter and JIT,
  that is the priority before any late-bound path from DNA Calc;
- module procedures are not COM objects, but their exposure should be as close to COM as
  possible; Excel's VBA glue likely has a small non-COM step in registration and COM-like dispatch
  otherwise; understand and mirror it, do not overdo our own;
- both interfaces may be needed; Windows gets full compatibility; Linux and web a subset (no
  general COM references on Linux, no direct file I/O on the web).
Open: whether "no code cross-dependencies" forbids a Cargo dependency on a neutral CalcVBA hosting
API crate (so the boundary is a binary interface: C/COM ABI, wasm component), or allows the glue
crate alone to depend on it. Propose an answer with reasons.

4.6 **Inner loop stays tight.** No process-laden development; per-commit checks catch real defects
and stay fast; slow validation is explicit and schedulable, never something work waits on; fix
couplings at the root rather than working around them.

4.7 **Libraries.** The owner asked to consider basing the next-generation engine on `asupersync`
and similar high-value work by the same author or others. Licence (OWNER RULING, 2026-10-31,
correcting research-libraries.md, which overstates the problem): asupersync and the author's other
projects are MIT plus a rider that keeps OpenAI and Anthropic themselves from taking the code for
their own products and tooling. It does not stop us, using coding agents on our own behalf, from
depending on, reading, fixing or improving it. asupersync is acceptable as a dependency, with a
pointer to the rider in our third-party notices. The residual risk is only a future licensee of
DNA Calc that is one of those companies; to be doubly cautious, ring-fence it: a clean dependency
boundary plus a simple non-concurrent execution path we maintain ourselves, so the engine runs
without it. Evaluate asupersync on technical merit (maturity, nightly requirement, browser support,
overhead; see research-libraries.md) alongside alternatives (salsa, formualizer, Arrow,
msft-typelib, wasm components/WIT, Cranelift).

4.8 **Owner rulings during review round 1 (2026-10-31).**
- **Non-cell nodes are defined names.** Excel already needs nodes that are not cells; defined
  names (workbook- and sheet-scoped, with their affinity rules) are the first-class non-cell graph
  nodes, and the design must support very large numbers of them. A Jupyter-notebook-style front end
  over Excel formulas must be able to sit on either a cell sequence on a sheet or on defined names,
  chosen later or switched dynamically. No further kinds of non-cell node are in scope.
- **Scale target**: what Excel supports first (sheet limits, name counts, workbook sizes), with the
  architecture ready to scale; billions of cells is not a goal now.
- **No stable row/column keys baked into the architecture.** Inserting a row is disruptive in Excel
  too. Start from what Excel persists (xlsx: A1 formula text with shared-formula masters and
  ranges; xlsb: parsed token streams with relative references; note that R1C1 and A1 formulas can
  behave differently) and treat the in-memory state as an optimisable representation of that. Fast
  structural updates come from optional indexes or pointer structures for the bad cases (for
  example large formula sets that would otherwise be scanned after a structural change), not from
  overhead built into every cell and reference.

## 5. Evidence: the foundation risk audit (read all six files)

All in `C:\Work\DnaCalc\parked\arch-review\`. Headlines:

- **risk-layering.md**: calc-fml has no evaluation context (six forwarders default to "not
  modelled"; ROW/CELL/SHEET/FORMULATEXT refused inside data tables and ROW() in OneCalc); a
  single-threaded UDF registry in calc-func headed into the multi-threaded engine; number-format
  rendering in calc-func against plan 4.4; grid and CELL derive width/alignment/sheet-name facts
  separately and disagree; locale half passed, half hard-coded; setting the clock makes every
  formula pending; conditional formats evaluated in the host over raw XML; function identity by
  spelling (volatility list, SUBTOTAL/AGGREGATE, two `_xlfn.` tables). Ends with what each
  boundary's explicit context would need.
- **risk-skins.md**: no command layer (shortcuts bound in the Grid skin, string-dispatched menus;
  Undo/Find/F9 absent in other skins); controller shaped around the Grid; worker protocol has one
  request per skin, some carrying pixel geometry and SVG; Excel rules (sheets, panes, axes, CF,
  replace, paste) live in the app's worker instead of calc-host; five copies of "show in skin X";
  a new skin touches about eight places; OneCalc has no model under its front end; god files.
- **risk-engine.md**: formulas stored as text per cell, shared formulas expanded (a 1M-row fill is
  1M graph nodes; row insert re-parses everything); recalculation runs to completion and publishes
  all at once with no priority input; cancel discards; parallel recalc native-only and disabled by
  spills/data tables; per-cell JSON with about eight strings each, viewport resent per block, DOM
  rebuilt per scroll; 600-900 B per formula cell over six maps; per-cell styles replaying edits;
  single-message worker; what is worth keeping (paged shared store, generational IDs, resumable
  recalc state, per-result calc stamp, prefix-sum geometry, worker boundary).
- **risk-vba.md**: the Excel object model lives inside OxVba (`oxvba-embed`), breaking its own
  charter; `ComCallEarly` exists but both runtimes lower it to name-keyed dynamic dispatch; the
  only vtable path is Windows x64 libffi per call; formula-to-VBA is name-keyed with three
  conversions each way and 8-10 allocations; a 4,096 host-object cap; DNA Calc's side is small.
- **risk-inner-loop.md**: check is the right shape but every edit compiles twice (xtask depends on
  product crates); app smokes (482 cases, one 8,754-line file, no case filter, pinned wording and
  pixels) grew fastest and run nowhere; 10 MB of receipts nobody reads; retired-CI tooling; four
  overlapping layering checks; tests and fixtures named after tracker beads.
- **research-libraries.md**: asupersync and related, plus alternatives.
- **foundation-ideas.md** (added for the review round): good ideas from the early maximum-ambition
  Foundation plans (Feb-Mar 2026) that were deferred or lost: per-value epochs and per-node
  publication with a replayable visibility stream; stable row/column IDs and formula blocks; an
  observer-driven necessary set with a height-ordered heap and early cutoff; declared
  evaluation-context facilities with invalidation tokens and call handles; one protocol surface;
  RenderPlan IR; counter-slope checks; non-grid nodes as graph participants.

## 6. What each draft must contain

Write to `C:\Work\DnaCalc\parked\arch-review\drafts\<model>.md`, at most 450 lines, in this order:

1. **Abstract architecture**, independent of today's code: core concepts and invariants; the
   layers and the explicit context each boundary carries (who supplies what, including callbacks);
   data representations (formulas and formula blocks, values, styles/formats, names, the dependency
   graph, IDs, partitions); the recalculation contract (demand and interest sets, priority,
   cancellation, incremental publication, determinism); concurrency and distribution; the command
   and functional model beneath skins; extension seams (LAMBDA, UDFs, CalcVBA hosting, the COM
   object library, add-ins, agents); the place of the Exact numeric path and of function research;
   platform profiles (Windows, Linux, browser).
2. **Seam pressures**: the non-obvious ones, each with the evidence that shows it is real and the
   design response. Include at least: function context vs presentation; shared formulas vs
   per-cell identity and structural edits; visible-first recalc vs Excel's calculation semantics
   (volatility, iteration, data tables, spills); worker boundary vs fine-grained reactivity;
   early-bound VBA vs portability and reload; OneCalc's ceiling vs UDF registration; skins vs one
   command model; bit-exact research vs product code.
3. **Reflection onto Rust**: the target crate map, what each crate owns, dependency rules, which
   current crates and modules move, split, merge or are rewritten, and what is kept as is.
4. **Implementation strategy**: order of work, what stays shippable at each step, the inner loop
   and test strategy consequences, library choices, and how the Codex workers' current lanes (UI
   slices, bit-exact research, CalcVBA) should be redirected.
5. **Decisions for the owner**: the few that only the owner can make, each with your
   recommendation.

Be concrete (name types, messages, invariants), skeptical, and short where the answer is simple.
Read code to verify anything you rely on; cite file:line where it matters.

# Appendix C: risk-engine.md

# Risk lens: engine data representations and the UI <-> engine path

Repo `C:\Work\DnaCalc\DnaCalc-review`, master 5a9348b. Read-only; nothing built or run. Byte figures are
estimates from struct layouts, not measurements (the bench in `tools/xtask/src/bench.rs` has no recorded
results). Goal measured against: dc-8abf (Webs-class: billions of nodes, work proportional to change,
distribution) plus viewport-priority, cancellable recalculation.

## Verdict

The authored cell store (`calc-doc-model` pages) and the resumable `RecalcWork` are sound foundations.
Most of the rest is built for one Excel-sized workbook in one process, recalculated to completion and
published all at once. Nothing in the engine knows what is visible. Ranked below by how expensive each is
to change later, most expensive first.

## 1. Formulas are per-cell A1 text everywhere (most expensive to change)

- Authored formula = `CellInput::Formula(FormulaText(ExcelText))`, one `Arc<[u16]>` per cell
  (`crates/calc-doc-model/src/input.rs:85-117`). There is no shared or relative (R1C1/template) form in the model.
- Shared formulas are expanded on load: "Each member's input is the master's text with its relative
  references moved" (`crates/calc-doc/src/load/mod.rs:268-271`). The group survives only as a side
  record in `LoadedSheet.formulas` (`load/mod.rs:110`).
- Engine binding: each node decodes its text into a `Vec<char>` and builds a template-key `String`
  (`crates/calc-engine/src/recalc.rs:1128-1135`). It gets its own `FormulaInstance` with a boxed
  `areas` slice (`crates/calc-fml/src/eval/mod.rs:407-411`) and its own `Vec<Prerequisite>`
  (`calc-engine/src/state.rs:22-27`, `recalc.rs:1207`). Plans are shared per template, but nothing else is.
- Plan 6.2 promises "compressed interval/rectangle edges" for fills. A 1M-row fill of `=A1*2`
  instead becomes 1M nodes, 1M prerequisite vectors and 1M reverse-index cell entries
  (`graph.rs:342`, `BTreeMap<CellKey, NodeSet>`).
- Structural edits rewrite every formula's text through a `FormulaRewriter`
  (`calc-doc-model/src/structural.rs:1-8, 23-31`). Every anchor in the workbook moves
  (`calc-engine/src/edit.rs:479-560`) and all templates are dropped (`edit.rs:609`). Inserting one
  row in a big sheet is O(all formulas), with a re-parse of each.
- Undoing a structural edit rebuilds the engine with every formula pending
  (`calc-host/src/command.rs:447-459`, `EngineState::rebuilt`). That costs a full recalculation.
- Why this is expensive later: formula text is what the op vocabulary, the undo inverses, the wire
  (`wire::Cell.input`), the save writer and many tests carry. Adding a template-plus-offset or
  run-length "formula block" representation touches doc-model, codec, engine, host and app together.
  It is cheapest to decide before the large function-family ports and the save path settle further.

## 2. Recalculation: runs to completion, publishes all at once, sees no interest (expensive)

- Scheduling is the dirty closure (pending + volatile + cycle members), ordered completely up front in
  `begin_recalc` (`recalc.rs:480-620`). There is no target or priority parameter. Full recalc happens
  only when the numeric mode changes, on `calculate_full`, or on `set_environment` (`state.rs:782-788`).
- A budget exists (`WorkBudget::evaluations`, `recalc.rs:216-229`), but the product never uses it.
  `HostSession::recalculate` loops `step(.., WorkBudget::UNLIMITED, ..)` to the end
  (`calc-host/src/session.rs:679-700`). Budgeted steps appear only in tests and the bench.
- Publication is all-or-nothing: `finish` refuses `Incomplete` and `Superseded`
  (`recalc.rs:1001-1013`). Until then, every cell in the closure projects as `Stale`. Nothing is
  published while the recalc runs.
- Cancellation is supersession only: a commit drops `host.work` (`command.rs:460`, `recalc.rs:233`),
  and all staged work is thrown away. The worker cannot see a cancel request in any case (see 5).
- Interest exists, but only for delivery. `Interest::{Area, Areas, Sheet, Selection}`
  (`session.rs:56-68`) decides which re-projected cells a sink receives. It never reaches the engine.
- Parallelism and budgeting exclude each other. Parallel runs only when the budget is UNLIMITED and on
  the first step. It also needs no spills, no data tables, no iteration, no cycles and a silent
  observer (`recalc.rs:644, 739-757`). It is native-only, and spawns scoped threads per level
  (`recalc.rs:968-990`).
- What is already in place for a priority/cancellable design:
  - `Calculation` is stored per result (`result.rs:30-40`), so per-cell "settled at tick T" is representable.
  - `RecalcWork` is owned and resumable.
  - Readiness (a node waits on a not-yet-evaluated prerequisite) exists (`recalc.rs:2078`).
- What is missing:
  - (a) `begin_recalc(targets)`, which needs the backward cone of the visible dirty cells. Forward
    prerequisites exist per node, so this is feasible.
  - (b) publishing settled nodes before the end, which breaks the single `generation` check in
    `step`/`finish` and the "one generation" contract in plan 2.3.
  - (c) a cancel/yield token.
  - (d) a host scheduler loop outside `HostSession`.
- The engine and host acceptance suites pin the atomic semantics (`Superseded`/`Incomplete`).
  So does the `StaleCause` vocabulary (`project.rs:21-47`). Every month of tests written against
  "stale until the whole generation publishes" raises the cost.

## 3. The wire and the grid carry formatted strings per cell; tests check DOM structure (expensive)

- The transport is JSON text through `postMessage` (`apps/dna-calc/src/wire.rs:830-838`).
- Each cell crosses as `wire::Cell` with about 8 owned strings: text, kind, input, prose provenance
  `why`, colour, owner, error meaning (`wire.rs:730-745`). `why` is a sentence per cell, for example
  "calculated by DNA Calc's engine at revision N" (`cells.rs:321-350`).
- Each delta is fully formatted on the server (`server.rs:1496-1535`). Estimate: 300-500 B of JSON per
  non-empty cell.
- Scrolling: the interest is rounded to 32x8 blocks (`grid/mod.rs:52-53, 133-152`). Each block crossing
  sends `SetInterest`, which delivers a full reset of the whole new window (`session.rs:735, 752`),
  not the cells that came into view. The page mirror is cleared and refilled (`controller.rs:97-117`).
- The render-plan `Memo` depends on `viewport`, so it rebuilds on every scroll event, converting every
  mirrored cell back to `CellShown` (`grid/mod.rs:355-372`). `cells_view` then rebuilds every cell
  `<div>` with freshly formatted inline-style strings, without keying (`grid/mod.rs:1216-1281`).
- The selection subscription follows Select-All. After Ctrl+A, every commit re-projects, diffs and
  JSON-encodes the whole sheet's occupied cells (`controller.rs:764`, `session.rs:758-770`).
  `Capture` builds a `Vec<CellKey>` of every cell in the area (`server.rs:100-118`).
- Why this is expensive later: the browser harness `tools/xtask/src/app.rs` has 97 references to the
  per-cell DOM attributes (`data-cell`, `data-provenance`, ...). Five skin stylesheets also key on them.
  A canvas/WebGL grid, or a binary columnar wire (value kind + f64 + string-table index + style id),
  would invalidate that harness and the `wire::Cell` round-trip tests. This matches the owner's point
  that rendering speed depends on suitable representations all the way through: today the
  representation is a string per field per cell from the host to the DOM.

## 4. Memory footprint and storage shapes (moderate to expensive)

- Per formula cell, summed across layers (estimate): 600-900 B, at least 6 heap allocations.
  - Model: an `Arc` of text in a page.
  - `LoadedSheet` per-cell `BTreeMap`s: `styles`, `formulas`, `value_caches`, held for the whole
    session next to the retained source part (`calc-doc/src/load/mod.rs:105-121`).
  - Engine:
    - `by_anchor: BTreeMap<CellKey, NodeId>`, duplicated by `AnchorIndex` (`state.rs:358-366, 179-189`).
    - `nodes: Vec<Option<FormulaNode>>` at about 80 B, plus the prerequisite `Vec`.
    - `Binding`/`FormulaInstance` plus a boxed areas slice.
    - `Results.by_slot` with a `FormulaResult`: `CalcValue` (~48 B, including `Option<Arc>`) plus a
      `Calculation` (~32 B, with an `Arc` refcount bump per result).
    - A reverse-index entry per cell read, and a readers-cache `Box<[NodeId]>` per node
      (`recalc.rs:1377-1391`).
- Styles have two layers, both per-cell maps:
  - The file layer is `LoadedSheet.styles: BTreeMap<(u32,u32),u32>`, reached through
    `opened_position`, which replays every axis edit since open on each lookup
    (`calc-host/src/state.rs:649-657`, `facts.rs:36-47`).
  - Authored overrides are `Definitions.formats: BTreeMap<CellKey,u32>` (`definitions.rs:315`).
  - Neither is a run or range representation, so formatting a whole column means 1M entries.
- `Definitions` is cloned whole on any format, CF, pane, iteration or name edit (`root.rs:657-683`).
  It holds every authored format, array, unloaded cell and hidden row.
- ID widths:
  - `NodeId`/`SheetId` = u32 slot + u32 generation (`calc-value/src/id.rs:150-160`), at most 4.29e9
    slots per allocator. Memory runs out long before that.
  - Slot-indexed dense `Vec`s (`nodes`, `Results`, `Progress`, `ReadersCache`) are sized by the
    largest slot ever used, with no compaction.
  - `CellKey` is 16 B, of which the `SheetId` is 8.
  - The grid is fixed at 1,048,576 x 16,384 and baked in (`calc-value/src/grid.rs:9-13`; page index
    const-asserts in `calc-doc-model/src/cells.rs:29-35`; 183 `MAX_ROWS`-style uses).
  - `ArrayShape` uses `usize`, which is 32-bit on wasm.
  - No node has a document-stable key (already noted in dc-8abf).
- What breaks first:
  - 10^6 formulas: works natively, at roughly 0.6-0.9 GB. In the browser (wasm32, 4 GB cap, with the
    retained XML and `LoadedSheet` maps) it is near the ceiling. Opening recalculates everything on
    one thread. One row insert or its undo is O(10^6) with a re-parse.
  - 10^8: does not fit in one process (tens of GB). The fixed grid forces about 6+ full sheets. The
    O(N) costs per keystroke in item 6 reach seconds.
  - 10^9: needs a partitioned graph. There is no seam for one. `EngineState` reads `&ModelRoot` pages
    in process. Results, ids and indexes are process-local dense `Vec`s.

## 5. Concurrency model (moderate)

- Browser: one dedicated worker with a thread-local `RefCell<Option<Server>>`
  (`apps/dna-calc/src/worker.rs:94-97`). `handle` runs each message to completion: commit, then a full
  recalc, then delta projection, then posting (`worker.rs:60-90`). Scroll `SetInterest` messages queue
  behind a long recalc, and a cancel message cannot arrive. There are no wasm threads.
- `HostSession` is not `Send`: `DeltaSink = Box<dyn FnMut>` (`session.rs:120`), and `Workbook` keeps
  `Rc`/`RefCell` caches (`workbook.rs:210-217`). `EngineState` is `Sync` (it is shared across scoped
  threads), which is good.
- What a structured-concurrency runtime would plug into:
  - The only resumable seam is `calc_host::recalc::step(host, budget, observer)` (`calc-host/src/recalc.rs:68-125`).
  - It would need:
    - a scheduler that owns the session (actor style) and yields to the message pump between budgeted steps;
    - a cancellation and priority token passed into `begin_recalc`/`step`. Today `ReadObserver` can
      only stop a recalc by failing it, which counts as a failure, not a cancellation;
    - per-level task spawning reusing a pool instead of `std::thread::scope` per level;
    - a `Send` session or an explicit message boundary.
  - Delta delivery happens inside `dispatch` through a callback. It would have to become pull- or
    channel-based.

## 6. O(workbook) costs on every recalc and projection (cheap to fix now, internal)

- Every recalc, whatever the closure size:
  - `Progress::new(state.nodes.len())` (`recalc.rs:552`, `read.rs:358-364`);
  - `vec![false; state.nodes.len()]` in `finish` (`recalc.rs:1018`);
  - the readers cache is dropped whole whenever any edge, anchor or `Definitions` pointer changes
    (`recalc.rs:1407-1427`, `ptr_eq` at `1363`), and then ranks are rebuilt over the whole `by_anchor` map.
  - So typing one formula, or one format edit, costs O(all formulas).
- `project()` recomputes `engine.stale()`, the full dirty closure, sorted, on every call
  (`calc-host/src/project.rs:211-214`; `state.rs:670-698`). Conditional formatting calls `project` once
  per cell per rule (`conditional.rs:656-658`). In Manual mode, or while anything is pending, each
  scroll costs O(closure x cells shown).
- Any CF rule anywhere in the workbook turns every delta of every subscription into a full reset
  (`session.rs:852-866`). `snapshot` walks every spill in the workbook per subscription
  (`session.rs:808`).
- CF is evaluated in host projection at render time (`conditional.rs:145-162`), not as engine
  participants as plan 6.4 describes. Its statistics re-project whole rule areas (`conditional.rs:849-875`).

## What to keep

- The doc-model store: 256x16 `Arc`-shared pages under a 4-level index, so a one-cell edit copies a
  bounded path (`calc-doc-model/src/cells.rs:1-10`).
- Generational IDs.
- Slot-indexed results with a dense anchor grid (`state.rs:170-210`).
- The owned, resumable `RecalcWork`, with readiness and waits.
- `Calculation` provenance per result.
- Geometry by prefix sums and binary search, with `ScrollScale`.
- A real worker boundary, with formatting done off the main thread.

## Suggested order of decisions (not work)

1. Settle the formula representation: a template-and-offset or block form in the model and engine,
   with A1 text produced at the edges. Shared/filled regions get one node per block and interval edges.
2. Specify the recalc contract now:
   - `begin_recalc` takes a priority set (the viewport and subscriptions);
   - settled nodes are published before the whole generation finishes;
   - a cancellation token is checked within `step`;
   - the host scheduler yields between budgeted steps.
   After that, stop adding tests that assume one atomic generation.
3. Choose a viewport wire format: columnar or binary, a style-id table instead of per-cell strings,
   provenance as a code with prose on demand, and differential window moves. Decouple the browser
   harness from per-cell `<div>`s, for example by querying an accessibility or semantic layer.
4. Fix the O(N) costs per recalc and per projection in item 6 cheaply now.
5. Introduce run- or range-based style storage before formatting features grow.

# Appendix D: risk-layering.md

# Foundation risk audit: layering and context leaks

Lens: where facts, services and presentation concepts cross the crate stack by convenient routes
instead of an explicit context handed down by the layer above. Clone `DnaCalc-review` at 5a9348b,
read only. Ranked by how much current or queued work builds on each problem.

## R1. calc-fml has no evaluation context; the cell reader carries the environment, host facts and caller

Evidence
- `calc-fml/src/eval/mod.rs:480-489`: `EvaluationServices` holds numeric mode, reader, observer,
  catalog, random, UDF registry, diagnostics, effects. No environment, no host facts, no caller.
- `calc-fml/src/reference/read.rs:47-55`: `ReferenceReader` (a value reader) also answers
  `environment()` (date system, locale profile, clock) and `host_facts()`. `eval/cells.rs:208-254`
  forwards them into calc-func's `CellReader`, re-borrowing the reader once per question.
- `calc-func/src/call.rs:51-62, 385-398`: calc-func reads the environment and host facts from the
  services or else from the cell reader. `with_environment` (`:364`) and `with_host` (`:380`) have
  no production caller, but `locale.rs:8-10` documents them as the route.
- Six reader wrappers re-implement the same forwarding: engine `read.rs:934/1054`,
  `recalc.rs:2201/2264`, `detached.rs:125/151`, `data_table.rs:308`, calc-fml `cells.rs:158/219`,
  OneCalc `fixture.rs:130`. Every `HostFacts` method has a default of `None`
  (`call.rs:113-169`), so a wrapper that forgets one refuses silently. It has already happened:
  `data_table.rs:308-388` (`WhatIfReader`) has no `host_facts`. Inside a data table's what-if
  evaluation, ROW(), COLUMN(), SHEET, ISFORMULA, FORMULATEXT, CELL and INFO are refused.
- The caller cell has two routes. The UDF context takes it from the instance anchor
  (`calc-fml/src/eval/run.rs:1868`). ROW()/COLUMN() and CELL take it from `HostFacts::caller`
  (`calc-func/src/catalogue/lookup.rs:1751,1768`), which the engine supplies
  (`recalc.rs:2265`, `detached.rs:152`). So ROW() is refused in OneCalc, which has no host facts,
  even though calc-fml holds the anchor.

Why it is weak: each new fact (precision as displayed, external books, RTD, add-in caller) adds
another defaulted trait method and another forwarding edit in six places. A missed edit
shows up only as a refusal in one evaluation route.
What builds on it: dc-snz.6 (in progress), dc-zmk, dc-jnt.14, CF/DV detached evaluation, data tables.
Direction: put a `FormulaEvaluationContext` in calc-fml, built once per evaluation by the caller.
It holds the caller identity (taken from the instance, never from the host), environment, host
facts, volatile services, the callable/UDF registry and effect sinks. Reduce `ReferenceReader` to
value, shape and spill access. calc-fml derives calc-func's `EvalServices` from it in one place.
Delete the fallback from services to cell reader in `call.rs:385-398`.

## R2. The UDF registry lives in calc-func, uses Rc, and is about to be carried into a multithreaded engine

Evidence
- `calc-func/src/udf.rs:5,53`: `UserFunctions` is `BTreeMap<String, Rc<dyn UserFunction>>`, which
  is not `Send`. calc-func never calls it. calc-fml compiles against it
  (`compile.rs:174-204`, `compile_with_functions`) and evaluates against it
  (`eval/mod.rs:507`, `with_functions`). That is two separate supply points for one registry, and
  nothing checks that they match.
- `calc-vba/src/lib.rs:11-13`: a `thread_local!` CODE_CACHE. `:85-86` puts the session in `Rc`
  shared by every registered function.
- `calc-engine/src/recalc.rs:968`: levels evaluate on `std::thread::scope` workers, and
  `WorkbookFacts` is explicitly `Send + Sync` (`call.rs:177`). The engine passes no UDF registry
  today (`recalc.rs:2469-2472`).
- Bead dc-snz.6 (in progress) says to carry the "caller-owned calc-func UDF registry through
  CalcFml ... CalcEngine graph into CalcHost".
- Plan §2.2 and §4.1 put the registry generation in `PlanKey`. The engine's plan and template
  caches (`state.rs:395-401`) have no such key.

Why it is weak: the type sits in the bottom crate only so calc-vba can avoid calc-fml; its thread
affinity conflicts with the engine's threading; the generation is not part of plan identity.
What builds on it: dc-snz.6, dc-snz.1/.2 (.xll), dc-snz.5, mostly still to come, so fixing it now
costs less than after it lands.
Direction: give calc-fml a `UserFunctionResolver` trait for compile-time resolution with a
generation, and a call callback for eval. Supply both through the one context in R1. Decide
explicitly how evaluation threads run: route thread-affine UDF calls to an owning thread, or
evaluate on one thread when a plan has `user_calls()`. Add the registry generation to the
engine's plan-cache key.

## R3. Number-format rendering and cell presentation live in calc-func, and the apps call it directly

Evidence
- Plan §4.4: "CalcFml owns the formatting engine ... CalcFunc ... calls an injected lower-level
  `NumberTextService`". The code does the opposite. `calc-func/src/format/mod.rs` (1,364 lines)
  exports `cell_text` (`:1052`), `cell_color` (`:1126`), `DEFAULT_PALETTE` (`:1095`) and
  `general` (`lib.rs:106-110`). The calc-fml crate doc (`calc-fml/src/lib.rs:1`) says it holds
  "number formats", but it only re-exports them.
- `calc-func/src/call.rs:218-286`: `CellFormatFacts`, `HorizontalAlignment`, `ColumnWidth`,
  `WorkbookFile`, `ApplicationFacts` (INFO's OS and release) are host and presentation types in the
  function crate. They are re-exported upward (`calc-fml/src/lib.rs:52-56`) and filled by calc-host
  (`calc-host/src/facts.rs:100-204`).
- The apps skip the host and call calc-func for display: `apps/dna-calc/Cargo.toml` depends on
  calc-func for `cell_text`. The calls are at `apps/dna-calc/src/cells.rs:187,209`,
  `workbook.rs:232`, and `apps/dna-formula-ui/src/result.rs:184,428`.
  `result.rs:430` hard-codes `DateSystem::System1900` instead of the session's environment.
Why it is weak: grid, CF number formats, formula UI and TEXT share a renderer only because all
reach into calc-func; no shared format context (date system, locale, palette) is passed.
What builds on it: dc-971p.6 (grid formats, alignment, fonts: "one existing formatting route and
actual column width"), dc-zsn.4 (locale month names), dc-jnt.12 (number-format tests), dc-wr9
(OneCalc format panel), precision as displayed (`calc-host/src/state.rs:47-65`, no bead yet).
Direction: either move the renderer to calc-fml as the plan says and inject a narrow
`NumberText` service into calc-func for TEXT/FIXED/DOLLAR/VALUE, or amend the plan and stop
claiming it. In either case, apps should get display text through one host or fml presentation
function that takes the same format context the engine uses.

## R4. The same presentation fact is derived by two routes (grid vs CELL, style lookup, column width)

Evidence
- Cell style index: `calc-host/src/state.rs:649-657` (`HostState::cell_style`) and
  `calc-host/src/facts.rs:86-97` (`HostFacts::style`) are copies. Each maps positions through its
  own axis-edit helper (`facts.rs:37` vs `self.opened_position`).
- Column width: CELL("width") uses `facts.rs:53-71` `maximum_digit_width`, which refuses any font
  other than Calibri or Aptos Narrow 11. The grid hard-codes `DIGIT_PX = 7.0`
  (`apps/dna-calc/src/grid/mod.rs:65`) and estimates text width as `chars * 7 + 5`
  (`grid/render_plan.rs:140-143`) for whatever font the workbook has.
- Alignment: CELL("prefix") reads the file's `<alignment horizontal>` through
  `facts.rs:114-127`. The grid places text by value kind (`render_plan.rs:154-172`) and reads
  alignment only to refuse AutoFit (`apps/dna-calc/src/server.rs:661-668`).
- Sheet name lookup: `calc-engine/src/read.rs:197-211` (`ReferenceCatalog`, used by references
  and INDIRECT) folds with Unicode `to_uppercase`. `read.rs:1096-1108` (`HostFacts::sheet_named`,
  used by SHEET("name")) folds ASCII only. A non-ASCII sheet name resolves for INDIRECT but not
  for SHEET.
Why it is weak: each pair agrees only on measured fixtures; dc-971p.6 will build on the app route.
Direction: one host-level "resolved cell presentation" query (style index → number format,
alignment, font metrics → column width in characters and pixels). The grid and `WorkbookFacts`
both consume it. Use one sheet-name matcher in calc-fml, the same one the binder uses.

## R5. Locale is half explicit context and half ambient en-ZA

Evidence
- Collation is fixed to en-ZA (`calc-func/src/lib.rs:44-50` `text_order`;
  `catalogue/comparison.rs:22`, `comparison/sort_key.rs:3`) with no environment parameter.
  DNA Calc opens every workbook as en-US (`apps/dna-calc/src/workbook.rs:28,37`).
- `LocaleId::CurrentExcelHost` is the owner's machine: rand currency, space as group separator
  (`locale.rs:968-985`), lowercase am/pm shared with en-ZA (`format/mod.rs:49-53`).
- The clock is a value inside `CalcEnvironment` (`locale.rs:1111-1115`), not a service like
  `RandomSource` (`call.rs:78-81`). OneCalc samples it once when the environment is set
  (`apps/dna-onecalc/src/surface.rs:153-158`). DNA Calc never calls `set_clock`, so NOW and TODAY
  are refused there.
Why it is weak: one workbook compares text under en-ZA but parses and formats under en-US.
What builds on it: dc-zsn (collation, Unicode folding, locale-aware parsing), dc-zsn.4,
dc-jnt.14, dc-wr9 (locale choice).
Direction: a `LocaleContext` that carries the format profile and the collation together, held
in the evaluation context, with `CurrentExcelHost` resolved at the app boundary. Make the clock a
callback service next to random.

## R6. Changing the clock invalidates the whole engine

Evidence: `calc-host/src/state.rs:234-238` `set_clock` → `engine.set_environment`. In
`calc-engine/src/state.rs:783-789`, a changed environment makes every formula node pending.
Volatile NOW/TODAY are already recalculated every time (`recalc.rs:515`).
Why it is weak: once DNA Calc sets the clock before each recalculation (needed for NOW/TODAY),
every recalculation becomes a full one.
Direction: split environment into a semantic part (changing it makes nodes pending) and volatile
services (clock, random) that are read per tick and never dirty anything.

## R7. Conditional formats and data validation live in the host, and their model form needs the codec to read

Evidence: `calc-host/src/conditional.rs:1-10` evaluates CF through `DetachedFormula` against
published results. `:703-713` re-parses each rule with `calc_doc::conditional_rule(source)` on
every `conditional_formats` call. The model keeps the rule as raw XML bytes
(`calc-doc-model/src/conditional.rs:6-15`). Plan §6.4 says CF and DV are engine participants with
dependencies. `tools/xtask/architecture.json` forbids calc-engine → calc-doc, so the engine can
never interpret these rules as they are stored.
The app also reads codec load records directly for live state: protection, views,
unread CF containers and validations (`apps/dna-calc/src/server.rs:603,713,827,887,1365,1627`
via `calc_host::LoadedSheet`, re-exported at `calc-host/src/lib.rs:96`). These are load-time
facts, and edits never update them.
What builds on it: dc-jnt.17 (lands in `calc-host/conditional.rs`), dc-971p.6, pivots (dc-8wp).
Direction: parse rules into a typed model record at load (keeping source bytes for round-trip),
so CF can become an engine participant and the app reads session state, not load records.

## R8. Function identity and file spelling are matched by string in several layers

Evidence
- Volatility is a name list in calc-fml (`eval/mod.rs:190-201`), not catalogue metadata (plan
  §4.3: "the catalogue declares ... volatility once").
- The engine detects SUBTOTAL/AGGREGATE by spelling, including `"_XLFN.AGGREGATE"`
  (`calc-engine/src/recalc.rs:1161-1166`).
- There are two file-prefix tables: calc-func `file_prefix.rs` (`FilePrefix`, used by calc-fml
  `compile.rs:2233`), and calc-doc's own `Prefix` enum plus 509 rows in
  `save/file_form_table.rs` (3,571 lines, `file_form.rs:44-60`). A third substring check
  is in `calc-engine/src/read.rs:1074` (FORMULATEXT refuses `_xlfn.`).
Why it is weak: a new function needs edits in up to four places tied only by separate tests.
Direction: volatility and the "is a subtotal" flag as catalogue metadata, engine decisions by
`FunctionId`, and one prefix table (calc-func's) that calc-doc reads through calc-fml.

## R9 (minor). Edges and globals

calc-doc (codec) → calc-fml → calc-func pulls the 77k-line function crate into the codec for the
lexer (`load/shift.rs:30`); `calc-doc-model/src/root.rs:18` has a process-global `NEXT_LINEAGE`
atomic; `calc-vba` falls under no `architecture.json` rule; `help.rs` (4,516 lines of UI text)
is in calc-func.

## What each boundary's explicit context would need (inferred from what is passed by side routes today)

- **func ← fml (`EvalServices`)**: numeric mode; locale (format profile and collation); date
  system; clock and random callbacks; caller cell; cell reader (values, subtotal exclusions);
  reference-text resolver; callable invoker; host facts as abstract questions. CELL's needs
  (prefix char, width in characters, format class, locked, file name) come as answers, not style
  enums. One route for each, no fallback.
- **fml ← engine/host (`FormulaEvaluationContext`)**: the above, plus the reference catalogue,
  the UDF resolver with generation and call callback (thread affinity declared), observer and
  effect sinks, and the format context shared with the presentation query.
- **engine ← host**: semantic environment (makes nodes pending) apart from per-tick volatile
  services; the workbook-facts snapshot; UDF registry with generation; thread policy; CF and DV
  rules as typed model records.
- **host ← app/worker**: clock source, application facts (INFO), file identity, locale choice
  (resolving "current host"), threads, registered add-ins. In return, a presentation query
  (shown text, colour, alignment, width), so the app stops calling `calc_func::cell_text` and
  reading `LoadedSheet` directly.

# Appendix E: risk-skins.md

# Risk lens: skins and app-shell drift

Audit of `C:\Work\DnaCalc\DnaCalc-review` at master `5a9348b`, read-only. Paths below are relative to
`apps/` unless noted. Baseline for growth figures: `git diff --stat ead996d~1 HEAD -- apps/`
(the dc-971p.2 to .15 slices, 2026-10-30/31): controller.rs +2,373, server.rs +2,106,
grid/mod.rs +1,228, wire.rs +366, 15,480 insertions across apps/.

Verdict: the owner's model (a functional model beneath, skins that only present and route) exists
only as a transport. There is no command layer, the controller is Grid-shaped and DOM-coupled, the
protocol has one bespoke request per skin, and Excel command rules live in dna-calc's server, not
calc-host. A new skin needs edits to six or more core files. Ranked by risk to future work.

## R1 (highest). No command layer: functionality is reachable only through the skin that wired it

Plan §8.1 (docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md:485-491) asks for `CommandId` plus typed
payloads, a capability descriptor, and "every ribbon item, shortcut, menu, gesture and dialog
confirmation invokes a named command". None of this exists: no `CommandId`, no capability
descriptor, no `calcctl` anywhere in the tree (grep). What exists instead:

- Keyboard commands are bound inside the Grid skin's keydown handler: Ctrl+F/H/G/F5
  (dna-calc/src/grid/mod.rs:695-706), Ctrl+Z/Y (747-754), Ctrl+C/X (759-764), Ctrl+D/R (770-775),
  Ctrl+plus/minus (781-786), F9 (792-794). Canvas re-binds F9 and Escape itself (canvas.rs:515-518).
  Atlas, Ledger, Tree and Notebook have no undo/redo/find/recalculate keys at all; the shell's
  app-level handler (app.rs:284-330) only does F6 region cycling and Alt+digit skin switching.
  So "every skin presents full functionality" is already false for the keyboard.
- Menu commands are string-dispatched in a view: `match command { "copy" => ..., "insert" => ...,
  "find" => controller.find_open.set(Some("find".into())) ... }` (commands.rs:245-256).
- Mode and command arguments are stringly typed in the model: `find_open: RwSignal<Option<String>>`,
  `paste_mode: RwSignal<String>`, `fill_series: RwSignal<String>` (controller.rs:166-200),
  `freeze_panes(&self, choice: &'static str)` matching `"first-row" | "first-column" | "active"`
  (controller.rs:1971, 1986-1988).
- Grid-only command surfaces: Paste Special and fill tools (`clipboard::controls`), axis
  size/hide (`axis::controls`), shift-cells (`structure::controls`), freeze panes and
  Insert/Delete buttons are all mounted inside the grid view (grid/mod.rs:1740-1760). The shell
  mounts Find and Review for every skin (app.rs:483-484, 665-666), so coverage is accidental, not
  designed.

Risk: each slice adds per-skin bindings; other skins fall further behind and an agent customising
a skin must rediscover and re-wire every command. Root cause of most findings below. Seam: a typed
`Command` (or `CommandId` + payload) in the model, an availability query returning a reason, and
one shell-level keymap for all skins; skins add only skin-local gestures.

## R2. The controller is Grid-shaped and DOM-coupled, and holds per-skin state

`Controller` is the de facto functional model (skin.rs:12-14 says skins reach the workbook only
through it), but:

- It imports the Grid skin's module: `use crate::grid::selection::{GridExtent, NavAction, next_active, opposite}`
  (controller.rs:26).
- It focuses the Grid's DOM after model operations: `crate::grid::focus_grid()` at
  controller.rs:798, 1018, 1520, 2192, 2223, 2389, 2570, and queries `[data-dnacalc=grid]` /
  `[data-dnacalc=formula-bar]` directly (432, 532, 2233). Escape or Ctrl+Enter in the Notebook's
  editor runs `editor_key` (notebook.rs:358-362 delegates to controller.rs:2181) which focuses a
  grid that is not mounted; `focus_grid` silently falls back to `.dc-main` (grid/mod.rs:1777-1787).
- It installs a document-level capturing keydown listener, re-synthesises `KeyboardEvent`s and
  re-implements caret movement, Backspace/Delete and Home/End over UTF-16 for the text field
  (controller.rs:340-647, about 300 lines). This is editor logic, keyed to grid/bar DOM
  attributes, inside the model.
- The edit session enumerates skins: `EditOrigin { Cell, Bar, Notebook }` (controller.rs:48-56).
- It owns the persistent view state of three skins: `ledger: crate::ledger::ViewState`,
  `tree: crate::tree::ViewState`, `notebook: crate::notebook::ViewState` (controller.rs:221-223),
  plus Grid's `grid_views` and `grid_geometry` (173-174), and resets them by name on open
  (controller.rs:726-738). Canvas and Atlas have none, so Canvas depth/zoom resets on remount while
  Ledger filters survive: the persistence rule depends on whether the slice author edited the
  controller.
- `crate::clipboard::grid_owns_clipboard` decides clipboard ownership by
  `document().active_element().closest("[data-dnacalc=grid]")` (clipboard.rs:458-469), called from
  the controller (controller.rs:346-352).

Plan §8.1 separates four state classes (workbook data, calculated facts, durable per-skin layout,
ephemeral selection/editor state). Here all four share one `Copy` struct of ~40 public signals.
Seam: (a) a DOM-free `Model` (selection, edit session, history, clipboard snapshot, command
dispatch) that `tests/workbook.rs` can drive natively; (b) a `ViewStateStore` keyed by skin id
that skins register into; (c) a thin `dom` adapter for focus/keyboard replay that asks the active
skin for its focus target instead of naming the grid.

## R3. The worker protocol is a list of per-skin view models, and it depends on skin modules

`wire::Request` (wire.rs:251-345) mixes three kinds of thing: generic model operations
(`Intent`, `Subscribe`, `Save`), Grid-specific queries (`Geometry`, `UsedArea`, `Charts`,
`ReviewMarkers`, `Validation`, `Capture`, `FitContents`), and one request per non-Grid skin.
The per-file mapping is one-to-one:

| Skin file | Requests/intents it uses |
|---|---|
| atlas.rs | `Atlas`, `AtlasRefusals` |
| canvas.rs | `Canvas` |
| ledger.rs | `Ledger` |
| tree.rs | `Outline`, `Dependents` |
| notebook.rs | `Notebook`, `BlockDetail`, `Intent::NotebookLink`, `Intent::NotebookOrder` |

The replies are presentation, not model facts:
- `wire::Canvas` carries pixel `x`/`y` per chip, `width`/`height`, and `wires_svg: String`
  (wire.rs:643-685) produced from layout constants `COLUMN_WIDTH = 230.0`, `CHIP_HEIGHT = 110.0`
  (canvas.rs:41-44) and `format!("<svg ...>")` (canvas.rs:195-211). A second canvas-like skin
  cannot reuse the dependency trace without inheriting this layout.
- `Request::Ledger` takes `filter`, `sort` as free strings (wire.rs:335-345) parsed by
  `filter_for_choice("errors" | "refused" ...)` (ledger.rs:138-160); the option values in the
  `<select>` (ledger.rs:531-533) are the protocol.
- `Intent::Navigate { nav: crate::grid::selection::Navigation, .. }` (wire.rs:147): the wire type
  is defined in the Grid skin.

Dependency direction is inverted: the worker-side server calls into skin modules:
`crate::canvas::view_model` (server.rs:352-361), `crate::ledger::view_model` (362-376),
`crate::grid::selection::navigate` (1339), `crate::grid::sheet_metrics` (619, 1338, 1996),
`crate::grid::autofit::FIT_CELLS` (630, 640), `crate::grid::axis::stored_size` (742),
`crate::grid::chart::chart_svg` (2057), and even the page-side `crate::controller::memory_bytes`
(server.rs:311). Plan §8.1 asks for projections that "name interests: worksheet windows, block
IDs, dependency neighborhoods, object data or diagnostics". Seam: a `projections` module in the
server with skin-neutral queries (dependency neighbourhood to depth N as keys and edges; formula
cell listing with a typed filter/sort; layout blocks and placements per `ViewKind`; census) and
move pixel layout back into the skins. Navigation semantics (`selection::navigate`,
`sheet_metrics`) belong in the model, not in `grid/`.

## R4. Excel command semantics live in dna-calc's server, behind calc-host's `Edit { edits }` escape hatch

calc-host's `SkinIntent` (crates/calc-host/src/session.rs:121-170) has a generic
`Edit { edits: Vec<CellEdit> }`. dna-calc's server composes the edits itself for:
worksheet add/rename/move/remove/hide, including the "History is reserved" rule and sheet-key
allocation (server.rs:1085-1160); frozen panes (1217-1245); axis size/hide and fit (1161-1216,
702-777); conditional-format rule edits (872-1013); find/replace (1041-1068 with
`search::replace_edits`); paste plans (405-519, 1246-1257); notebook link/order, including
block-id and placement-id allocation by `max + 1` (1636-1815). None of this is reachable without
dna-calc's server, so a headless `calcctl`, an automation route (plan §8.8) or another front end
would have to copy it. `host_intent` (server.rs:2087-2104) still has arms that refuse these
intents with "needs the workbook worker"; they are unreachable because `intent()` handles them
first, and the messages are now misleading.

Risk: each UI slice adds Excel rules to the app layer. Seam: typed calc-host commands (sheet,
pane, axis, rule, layout) with validation in calc-host; dna-calc's server becomes a translator.

## R5. Duplication and drift between skins

- "Show this cell in skin X" is implemented five times with string skin ids, a 20 ms timer and a
  hard-coded focus selector: atlas.rs:134-144, ledger.rs:441-458 (chooses `"[data-dnacalc=canvas]"`
  or grid by string compare), notebook.rs:289, canvas.rs:525, tree.rs:647. All bypass
  `skin::switch_skin` and `SkinId` (skin.rs:157-165) by writing `active_skin` directly.
- "Reads" means different things: Ledger counts precedent *areas* (`host.precedents(at).len()`,
  ledger.rs:347), Canvas lists area labels (canvas.rs:339-343), Tree fetches dependents through a
  separate `Request::Dependents` and shows a count (tree.rs:585-600). Three dependency
  inspections, three code paths.
- Value/reason wording is recomputed per view model: `"refused".to_owned()` substitution at
  canvas.rs:259, ledger.rs:263, cells.rs:230; the reason sentence is
  `"{provenance}; {meaning}"` in Canvas (canvas.rs:317-321) but `"Calculated worksheet error:
  {meaning}"` only when Calculated in Ledger (ledger.rs:332-336), and the subscription `wire::Cell`
  has its own `error_meaning` (wire.rs:766). The same cell can explain itself differently per skin.

## R6. What a new skin would have to touch (no documented API)

`Skin::mount(ctx: SkinContext)` (skin.rs:84-91) hands over the whole `Controller` (about 40 public
signals and 70 public methods, none documented as a stable surface) and a raw
`RwSignal<Option<String>>` for the active skin. To add a seventh skin an agent must edit:
(1) the closed `SkinId` enum (skin.rs:31-74); (2) the registry (app.rs:32-40); (3) a new
`Request`/`Reply` pair in wire.rs; (4) a `handle` arm and view model in server.rs:84-392;
(5) a state field and its reset in controller.rs:165-232, 726-738; (6) a new `EditOrigin`
(controller.rs:48-56) and the key-replay selector (controller.rs:432) if it edits cells;
(7) the stylesheet list in tools/xtask/src/app.rs:126-130; (8) the five "show in skin" copies
(R5). It also has to follow undocumented DOM conventions (`data-dnacalc=grid`, `.dc-main`) for
focus, which the smoke harness also keys on.

## R7. OneCalc: the boundary holds, but its front end has no model under it

Positive: dna-formula-ui stays at calc-fml/calc-func (lib.rs:3-6); OneCalc re-exports it
(dna-onecalc/src/display.rs:9, result.rs:7) instead of copying; DNA Calc uses it for labels,
results and the formula field. The sweep's dependency ceiling enforces this (AGENTS.md).
Risks:
- `surface()` is one ~740-line view function (dna-onecalc/src/surface.rs:408-~1150) holding all
  application state as local signals (formula, rows, scratch entries, locale, format, fixture,
  share link, VBA source/status, dirty baseline), with `recalculate` (184-355) in the view module.
  The plan's "OneCalc has its own small application controller" (plan §8.7) does not exist;
  OneCalc cannot be reskinned without extracting one first.
- Shell pieces are duplicated rather than shared: theme `<select>` (app.rs:512-515 vs
  surface.rs:904-907), dialog styling (shell.css:76-77 `.dc-replace` vs surface.css:92-93
  `.oc-session-dialog`, same shadow/backdrop), unsaved-work protection (controller
  `has_dirty_work` vs OneCalc `baseline`).
- The edit contract diverged: DNA Calc wraps `FormulaAssist` with its own commit, point mode and
  F4 (controller.rs:2107-2485, using `dna_formula_ui::editor::{cycle_anchors, point_reference_span}`),
  while OneCalc drives `FormulaAssist` directly. Key semantics for the shared formula field now
  depend on which app hosts it.

## R8. God-object growth and the seams to split along

| File | Lines | Shape |
|---|---|---|
| dna-calc/src/controller.rs | 2,817 | One struct, ~70 methods; input queue/key replay 305-648; selection/navigation 1092-1240; clipboard/paste/fill 1245-1640; sheets 1640-1760; axis/fit/panes 1767-2015; go-to 2015-2106; edit session 2107-2485; structure/clear/history/calc 2485-2680; subscriptions/save 2682-2772 |
| dna-calc/src/server.rs | 2,661 | `handle` router 84-392; Excel commands 597-1350; notebook layout 1538-1875; outline 1877-1992; charts 1993-2066; tests 2197+ |
| dna-calc/src/grid/mod.rs | 1,870 | `grid_view` is a single ~1,610-line function (165-1776) containing keymap, pointer, editor, validation list, review markers, toolbars |

Natural cuts, in order of payoff: (1) controller → `model::{selection, edit, clipboard, sheets,
layout, history}` plus a `dom` adapter (R2); (2) server → `router`, `commands` (moving toward
calc-host, R4), `projections` (skin-neutral, R3); (3) `grid_view` → keymap table (to the shared
command layer, R1), pointer, editor overlay, and toolbars as separate components. Do (1) before the
next skin slice: every dc-971p slice added to all three files at once, so merge conflicts and
regressions concentrate there.

**Lower risk.** Strand tokens are mostly respected; one raw amber `rgb(255 179 1 / 0.16)` (dna-calc/web/css/grid.css:18)
should be a token. The transport is sound: one JSON message type, the in-process fallback runs the
same `Server` (port.rs:43-50), and tests/workbook.rs drives `Server::handle` through `wire::encode`.

# Appendix F: risk-vba.md

# Risk lens: VBA coupling and binding model

Read-only audit, 2026-10-31. DnaCalc at `5a9348b` (C:\Work\DnaCalc\DnaCalc-review). OxVba read at
the consumed pin `c7ce1349` via `git show` (HEAD is `b05b8ef1`, one commit later; working tree
not read because another agent is active there). Paths below are relative to each repo.

Owner model, for reference: CalcVBA is host-neutral (no UDF term, no Excel types) and exposes
load / enumerate / callable entry. DNA Calc's glue registers shims and builds its own type library
(Application, Range, WorksheetFunction). Early binding both ways: formula -> JIT code by a fast
typed call; VBA -> host by vtable through a type library, in interpreter and JIT.

## Verdict

The current integration is a late-bound, name-keyed, `Value`-enum bridge whose spreadsheet
object model lives inside OxVba. Both directions contradict the owner model. The DnaCalc side
is small (about 1,100 lines) and cheap to unwind. The OxVba side is the problem: the only
embedding API (`oxvba-embed`) is spreadsheet-specific, and the backends lower even the
"early-bound" IR instruction to by-name dynamic dispatch. Every further dc-snz.5 /
bd-59co.2.10.6 increment (more Range members, Caller, WorksheetFunction methods) is being
added to the wrong repo on the wrong mechanism.

## Ranked findings (most building on the wrong shape first)

### 1. The Excel/DNA Calc object model is implemented inside OxVba (wrong repo, wrong owner)
- `crates/oxvba-embed/src/spreadsheet.rs` (635 lines) implements Application, Range and
  WorksheetFunction as a `ComHal` (`:416-557`): `Application.Caller`, `.Volatile`,
  `.WorksheetFunction` (`:486-511`), Range `Value2/Count/Row/Column/Address/Rows/Columns/Cells/Item`
  (`:292-367`), Excel's grid limits 1,048,576 x 16,384 (`:41,45`), error 1004 semantics
  (`:340,519-522`), and "Excel materializes an object-valued UDF result through its Range value"
  (`:275`).
- A hardcoded type-library resolver answers both `"DnaCalc"` and `"Excel"` with this surface
  (`:559-634`); `load_inner` injects `DnaCalc.Application` and a reference named `Excel`
  (`crates/oxvba-embed/src/lib.rs:127-152`). ProgID `DnaCalc.Application` and `DnaCalc.ReadOnly`
  are string literals in OxVba (`spreadsheet.rs:420,431,628`).
- Spreadsheet vocabulary is in the public API: `Value` enum doc "Spreadsheet values",
  `Value::Range`, `Value::Error(i32)` "Excel error code" (`lib.rs:20-38`), `Module::load_spreadsheet`
  (`:109`), `SpreadsheetContext` (`spreadsheet.rs:77-86`), `Error::HostRefused` "spreadsheet host
  refused" (`lib.rs:75-76`), project name `"UdfProject"` (`:119,512,559`).
- The spec is `docs/spec/UDF_EMBEDDING_V1.md` ("UDF value-call contract v1"), which tells the
  consumer to "register the admitted functions in the caller-owned calc-func registry / calc-fml
  resolver" and names `dc-snz.3`, `dc-snz.5`, OneCalc, CalcFunc evaluator callbacks.
- This contradicts OxVba's own charter: out of scope "Office application object model
  implementation (host-provided)" (`CHARTER.md:49`) and "Spreadsheet engine" (`:47`). It also
  reverses the May 2026 boundary contract (`docs/evidence/host_callable/NEUTRAL_DESCRIPTOR_MODEL.md:14,22`:
  "UDF-named options, shims, bridges ... are removal targets"; runtime must not own "a worksheet-UDF
  concept; Excel-specific caller semantics"). That neutral `VbaHost` facade
  (`crates/oxvba-host/src/vba_host.rs`, `invoke_callable_typed`) was deleted with the legacy stack
  (commit `2dbaec43`) and `oxvba-embed` (first commit `0113e7ee`, 7 commits) replaced it with a
  UDF-shaped API. The neutral design existed and was lost.
- Remaining spreadsheet traces elsewhere in OxVba core are test fixtures only
  (`oxvba-runtime/src/call_frame.rs:345`, Excel typelib tests); the coupling is concentrated in
  `oxvba-embed`, which keeps removal tractable.

### 2. VBA -> host calls are late-bound by string in both backends, even for "early" IR
- OxIR has the right instruction: `OxInst::ComCallEarly { method: ComMethodRef, ... }` carries a
  typed type-library descriptor (`crates/oxvba-oxir/src/inst.rs:230-254`), distinct from
  `ComCallLate` (`:255-262`).
- VM3 lowers it to a dynamic request: `TokenNamed { token: dispid, name: member_name.clone() }`
  through `dispatch_com_method` (`crates/oxvba-vm3/src/lib.rs:2175-2203`), which allocates a
  `DynamicCallRequest` with a `Vec<DynamicCallArg>` of boxed Variants and calls
  `host.com().dispatch_invoke_dynamic_variant_with_writebacks` (`:4161-4188`).
- The JIT lowers `ComCallEarly` and `ComCallLate` to the same generic helper
  `project_member_get_slot`, passing a name pointer and a token
  (`crates/oxvba-jit/src/lowering.rs:497-540,3307-3375`); the helper builds the same heap
  `DynamicCallRequest` with `name.to_string()` and calls the same `ComHal` dynamic entry
  (`crates/oxvba-jit/src/helpers.rs:4359-4383`). No compiled vtable call exists in the JIT.
- The spreadsheet host then ignores the token: `TokenNamed { name, .. }` -> `to_ascii_uppercase()`
  -> string `match` (`spreadsheet.rs:467-474,486-527`). Object handles are resolved through a
  `Mutex<Vec<Object>>` lookup (`:196-208`). Its synthetic type library declares
  `vtable_slot: None`, `is_dual: false`, all-Variant parameters, and overlapping tokens
  (Application 1-3, Range 1-8) (`:572-615`). The "type library" is decorative.
- The only real vtable path is the Windows x86_64 standard HAL: a runtime, per-call libffi
  slot call under a `PreferVtable` policy with IDispatch fallback (`crates/oxvba-com/src/windows_vtable.rs:1-22`,
  `windows_invoke.rs:2954-2990`, `crates/oxvba-hal/src/adapters/standard/com.rs:660-670`). It
  is reached only after the same dynamic request has been built, needs a live Windows COM
  object, and does not exist for an in-process Rust host object or on Linux/web. Non-Windows
  COM is an extended profile (`README.md:162`, `CHARTER.md:44`).
- `ComHal` (`crates/oxvba-hal/src/traits.rs:360-500`) is the only host-object extension point,
  and it is dynamic by construction. No host-supplied vtable/interface ABI exists.
- Correctly late: members called on a `Variant`/`Object` receiver are late-bound in Excel too.
  The gap is typed receivers (`r As Range`, `Application.`), which Excel binds early.

### 3. Formula -> VBA is a name-keyed, triple-converted call into a Variant-frame JIT
Path for `=Add2(A1,2)` (native, JIT selected), per call:
1. Compile: `Instruction::UserCall { name: String, argc }` (`crates/calc-fml/src/eval/mod.rs:235`,
   emitted at `compile.rs:1015-1022`). Plans keep names by design ("Plans retain names rather
   than engine objects", `docs/plan/VBA_UDFS.md:19`).
2. `Run::user_call`: `stack.split_off` allocates the argument Vec (`run.rs:266-267`);
   `functions.contains(name)` uppercases (alloc) and probes a BTreeMap (`:270`, `udf.rs:71-73`).
3. `UserFunctions::call_with_context`: uppercases again and probes the BTreeMap again
   (`udf.rs:86`), then a `dyn UserFunction` call.
4. calc-vba `call_with_context`: builds `ContextBridge`, maps each `CalcValue` to
   `oxvba_embed::Value` into a new Vec; text becomes a `String` from UTF-16 (`calc-vba/src/lib.rs:135-146,160`).
5. oxvba-embed: scoped TLS installs the context (`spreadsheet.rs:118-129`); the function is
   found by linear scan with `eq_ignore_ascii_case` on bare and qualified names (`lib.rs:392-400`);
   each arg becomes a `Variant` (`to_variant`, `:583-617`), is coerced (`coerce_argument`,
   `:526-553`) and pushed to another Vec (`:419-444`).
6. JIT entry: `invoke_in_session` -> `execute_in_session` allocates per-call `globals` and
   `program_images` Vecs and installs a proc-invoker bridge (`crates/oxvba-jit/src/runtime.rs:136-244`);
   `invoke_func` allocates a heap frame of Variant locals and clones the args into it
   (`:396-399`, `new_jit_frame` `:648-659`). There is no typed native entry point to call.
7. Return: `Variant` -> `SpreadsheetHost::result` -> `from_variant` -> `Value` (`lib.rs:499-502`)
   -> `from_vba` -> `CalcValue` (text re-encoded to UTF-16 twice, `calc-vba/src/lib.rs:200-204`).
   Errors are flattened to `String` at each layer (`.map_err(|e| e.to_string())`, `:127,146`)
   and mapped to `#VALUE!` (`run.rs:286-294`).

Count: 4 name lookups (compile `contains`; runtime `contains`; registry `get`; embed linear
scan), 2 uppercase allocations, 3 representation hops each way (CalcValue -> Value -> Variant ->
frame slot clone), and roughly 8-10 heap allocations for a two-Double call before any VBA code
runs. Inferred from reading; no benchmark exists. Zero of these are early-bound.

VBA -> host on `Range.Value2` inside that call adds: dynamic request (step 2 above) ->
uppercase match -> object-table lookup -> TLS context -> `ContextBridge::read_range` ->
`Sheets::reference` (linear sheet list, `calc-vba/src/lib.rs:443-459`) -> `values_of` ->
`CalcValue` -> `to_vba` -> `Value` -> `to_variant`. Callback `WorksheetFunction.Sum` resolves
by name again through `calc_func::catalogue().resolve_surface` (`run.rs:1900-1903`).

### 4. Host object identity has no lifetime and caps at 4,096 per loaded module
`SpreadsheetHost.objects` only grows (`spreadsheet.rs:147,177-189`); `MAX_OBJECTS = 4096`
(`:151`), then every new Range/Caller/Rows/Columns identity is refused. The host lives as long
as the session (`crates/oxvba-embed/src/cache.rs:20-29`). One module whose UDF reads
`Application.Caller` from more than 4,096 distinct cells would start refusing (inferred from
code, not tested). Identity is deduplicated by coordinates (`VBA_UDFS.md:33`), not reference
counted. With COM-style host objects (IUnknown refcount) this table disappears.

### 5. DnaCalc dependency surface (small, contained)
- Only `crates/calc-vba/Cargo.toml:15,18` depends on OxVba: `oxvba-embed` by git rev
  `c7ce1349`, `jit` feature on non-wasm. `Cargo.lock:1535-1718` pulls 17 OxVba crates.
- `apps/dna-onecalc/Cargo.toml:26` depends on calc-vba; `src/session.rs:338-348` calls
  `calc_vba::load_module`; also `surface.rs:469`, `fixture.rs:275,354`, `examples/vba.rs`.
- calc-engine, calc-host and calc-doc have no UDF/VBA wiring at all (grep clean). Workbook
  VBA hosting has not started, so nothing in the engine has to be unwound.
- calc-func `UserFunction`/`UserFunctionContext`/`UserFunctions` (`crates/calc-func/src/udf.rs:11-110`)
  and calc-fml `UserCallContext` (`run.rs:1862-1930`) are spreadsheet-side and engine-neutral
  in naming. Their shape is value-slice in, `Result<CalcValue, String>` out, name-keyed
  registry; that suits a late-bound adapter, not a typed shim. The refusal owner `"dc-snz.5"`
  is hardcoded in calc-fml (`run.rs:1879`).
- If "no code cross-dependencies" means no Cargo edge at all, the git pin itself must become
  a binary/ABI boundary. If it means only the glue crate depends on the neutral hosting API,
  today's single edge is already the right topology; only the contents are wrong. Worth an
  owner ruling.

## What is reusable
- `ComCallEarly` with typed `ComMethodRef` descriptors in OxIR (`inst.rs:230-254`) and the
  pluggable `oxvba_symbol::TypeLibResolver` at bind time (`embed/lib.rs:148-153`): a
  host-supplied type library can already reach the compiler. The missing parts are backend
  lowering and a host-object ABI.
- Windows type-library loader with dual/PSOA classification and the libffi vtable marshaller
  (`oxvba-com/src/windows_vtable.rs`, `windows_typelib_loader.rs:4763-4774`); `oxvba-comhost`
  dual-interface tier for exporting VBA classes.
- Procedure catalogue metadata (`Function`/`Parameter`, `embed/lib.rs:46-61,175-217`) is
  host-neutral apart from naming; optional/ParamArray/default folding (`:419-452`) is reusable.
- Behavioral evidence is mechanism-independent: Excel run-3 observations
  (`crates/calc-vba/tests/fixtures/excel-recalculation/observations.json`, 17 stages), volatility
  rules (`VBA_UDFS.md:64-100`), the interpreter/JIT parity tests. Keep these as the oracle.
- Per-call budget, state retention across calls, code cache keyed by target/ABI.

## What must be unwound to reach the owner model
OxVba (larger):
1. Delete `oxvba-embed/src/spreadsheet.rs`, `Value::Range`, `load_spreadsheet`,
   `SpreadsheetContext`, `HostRefused`, `"UdfProject"`, the `Excel`/`DnaCalc` resolver, and
   rename/replace `UDF_EMBEDDING_V1.md` with a neutral hosting contract (load project,
   enumerate public procedures and signatures, obtain a callable handle, register host type
   libraries and host object implementations). Restore the May neutral-contract boundary.
2. Define a host-object ABI that works without Windows COM: host-registered interfaces with
   vtable slots (Rust `extern "C"` tables), so a DnaCalc Range is reachable on Linux/web.
   Windows can additionally accept real COM objects.
3. Lower `ComCallEarly` to a direct slot call: JIT emits load-vtable / call-indirect with
   typed arguments; VM3 calls a resolved slot without building a name-carrying request.
   Keep `ComCallLate` for Variant/Object receivers.
4. Give the JIT typed native entry points per public procedure (scalar args in registers,
   no per-call frame/globals Vec setup) and hand the host a function pointer plus signature.

DnaCalc (smaller, about 1,100 lines: calc-vba 551, udf.rs 171, run.rs user-call path ~120,
OneCalc vba tests 353):
5. calc-vba becomes the glue: it owns the type library and the Application/Range/
   WorksheetFunction implementations (moved from OxVba), and generates typed shims.
6. Replace `Instruction::UserCall { name }` with a resolved handle (slot into a
   generation-stamped registry) so evaluation does no name lookup; reload bumps the generation
   instead of relying on names. This reverses the "plans retain names" decision in VBA_UDFS.md.
7. Replace `UserFunction::call(&[CalcValue]) -> Result<_, String>` with a typed shim entry and
   structured errors; keep a slow path for Variant/Range parameters.

## Risk if unchanged
- Each new Range member or WorksheetFunction method grows a string switch in OxVba that has
  to be moved and rewritten later; every test written against `oxvba_embed::Value` and
  `SpreadsheetContext` is mechanism-coupled.
- Performance ceiling: the formula -> VBA path cannot approach a direct call while plans hold
  names and the JIT takes a Variant heap frame; billion-node recalculation with UDFs is out of
  reach on this shape.
- The JIT gives no binding advantage on host calls today: its host calls are identical in
  shape to the interpreter's, so "native prefers JIT" buys only VBA-internal speed.

# Appendix G: foundation-ideas.md

# Foundation ideas worth reconsidering: the Feb-May 2026 maximum-ambition plans

Date: 2026-10-31.

**Source.** `C:\Work\DnaCalc\Foundation`, read-only. The repository has 79 commits. The first is `fcc6d4b` (2026-02-22); the October plan bundle is `c1a1a00`.

**Comparison points.** `parked/arch-review/risk-engine.md` (code at 5a9348b) and sections 3 and 6 of `DnaCalc-review/docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`.

This review does not argue for a return to February. It recovers mechanisms that were specified, then deferred or lost, and that bear on today's targets: Webs-class scale, visible-first recalculation, explicit layer contexts, skins, early-bound VBA and bit-exact functions.

**Status tags.** **KEPT**: in plan and code. **PLAN-ONLY**: in the plan, not the code. **DEFERRED**: deferred in February, with a trigger for revisiting. **LOST**: in neither. **REJECTED**: the history gives a reason.

**Abbreviations**

| Short | Path | Commit |
|---|---|---|
| AR | `ARCHITECTURE_AND_REQUIREMENTS.md` | |
| BN | `notes/BRAINSTORM_NOTES.md` | |
| CEFM | `CORE_ENGINE_FORMAL_MODEL.md` | |
| FMI | `notes/archive/formal-model/FORMAL_MODELS_IDEAS.md` | `d91e01b`, 02-25 |
| DAG | `research/runs/20260305-201430-dag-computation-theory-deep-research-pass-01/outputs/` (07 is Claude's report, 09 is the reconciliation) | `c0f97b7`, 03-06 |
| JS | `research/runs/20260222-123425-run4-janestreet-oxcaml-incremental-internal/outputs/response.md` | 02-22 |
| FEC | `prompts/runs/20260308-171858-core-engine-fec-f3e-deep-research-pack-01/responses/claude_opus_response.md` | `4ceda07` |
| DEC | `synthesis/runs/20260307-223304-core-engine-dag-fec-synthesis-pass-01/outputs/03_stable_topic_entries.md` | `4ceda07` |
| PPnn | `prompts/runs/20260222-011351-prompt-pack/responses/nn_*.md` | `ce858d6` |
| OCT | `plans/calc-reimplementation-2026-10/calc-reimplementation-planning-gpt6pro.md` | `c1a1a00`, 10-13 |

## 0. The ambition at its peak, and when and why it was cut back

**Peak, 2026-02-22 to 03-09.** The mission was "a near-formal, state-of-the-art spreadsheet platform" (CHARTER, `fcc6d4b`). It rested on:
- two engines, Rust (Red) and .NET (Blue), behind an identical protocol;
- a "Green" stack of Lean, TLA+, an OCaml oracle and obligation packs;
- four rounds: VisiCalc, PreCalc, SuperCalc, DnaCalc;
- a universal OpLog with MVCC epochs, and versioned profiles;
- collaboration through OpLog replication, with full RTD and XLL support.

The 2026-02-22 synthesis pass 02 (`cb5fe27`) added "Alien Artifact leverage" and "Design for Evolution".

The recalc model peaked in the "Option B" staged plan (AR §3.19 and CEFM §6.8, `4ceda07`/`4b7c233`, 2026-03-09):
- **Stage 1**: a sequential coordinator.
- **Stage 2**: partitioned parallel evaluators.
- **Stage 3**: "dynamic-topology/SAC-inspired lanes and stream-heavy advanced lanes" (SAC is self-adjusting computation).

FEC's Phase 3 went as far as "Distributed evaluation: Partition large workbooks across compute nodes using the MVCC snapshot model".

**Cutbacks, in order**
- **02-23, independent review** (`notes/archive/independent-review-20260223/INDEPENDENT_REVIEW.md`, `b9731f5`):
  - criticised "The Dual Engine Trap", the language proliferation and the scope of verification;
  - concluded "The Process Is Producing Documents, Not Software".
- **02-27, `f50611d`**: Round 0 was bound to the DnaVisiCalc v0 scope (single sheet, A1..BK254). Multi-sheet support and full coercion became non-goals. Missing formal artifacts were named as "the principal Round 0 exit blocker" (AR §6.3).
- **03-09, `3583a6d`**: spec ownership moved to OxFml and OxCalc. Stage 3 stayed, but only behind "parity/equivalence evidence".
- **05-20, `b7dccb7`**: ambition actually rose. "High-throughput full-model recalculation is a core long-term architecture goal", with optimiser metadata and value-only versus trace-rich modes.
- **July and August**: OxCalc realised part of the model. The 08-30 investigation (`notes/PROGRAM_INVESTIGATION_2026-08-30.md` §6.3) found:
  - a per-cell `value_epoch`;
  - an optimised grid lane plus an oracle lane;
  - `WorkbookRecalcTick` "for coherent NOW()/RAND".

  It also found that Foundation still described "two independent engines", "OpLog", and "Lean + TLA+ + OCaml", none of which existed.
- **10-05, OCT**: "About ten months of work produced strong pieces and a tangled whole." The plan made these cuts:
  - "drops the invented 'tree' reference language and the dual-profile model. There is only Excel";
  - no Lean and no formal methods;
  - "Single-threaded recalc for now, as the reference implementation for a future optimised (incremental/parallel) version".

**Why it was cut (OCT §6, "Lessons")**
- "Conservative full rebuild is always legal" became the default path. February's Stage 1 had explicitly allowed a "conservative fallback".
- "No seams without traffic": interfaces that nothing called "rotted and misled audits".
- "One of everything": two engine lanes meant "every feature landed in only one".
- The measured baseline: "a warm F9 that evaluates 2 cells: 53 s".

These are good reasons to cut the ceremony and the duplicate lanes. They are not reasons against the mechanisms below. The constraint they leave: re-adopt a mechanism only with a real caller, inside the one scheduler, and with every fallback counted.

## 1. The envisioned recalc model

**R1. Per-value epochs; staleness allowed but visible.** LOST in its per-value form.
- **Was**:
  - `committed_epoch`, `stabilized_epoch` and a per-value `value_epoch`.
  - "Stale values are allowed but **must be detectable**".
  - "clients can pin an epoch for consistent reads while newer epochs progress" (AR §3.3, `fcc6d4b`).
  - PP05 adds that `E` "may remain partially pending while new work starts".
- **Now**: OxCalc had a per-cell `value_epoch`; the October rewrite dropped it. Today there is one generation, published only when it finishes (risk-engine §2). `Calculation` per result and `StaleCause` are what is left.
- **Matters**: incremental publication, cancellation and distribution all need versions per value. The independent review called the epoch model "the crown jewel of the architecture". Nothing rejected it.

**R2. Atomic commit per node, not per generation.** LOST.
- **Was**: "`Applied` commits publish exactly one atomic `DerivedBundle` per committed node" (CEFM §5.3; CONSTR-018). The node's value, its read edges and its spill shape travel together.
- **Now**: plan 3.3 makes the generation the unit of commit. Risk-engine §2(b) names that as the blocker.
- **Matters**: this is the missing half of visible-first. It keeps "no new values with old edges" and still allows partial publication.

**R3. Visibility is scheduling metadata, never semantics.** DEFERRED, then LOST.
- **Was** (DEC-CALC-008; FEC VIS-RULE-001..003):
  - `VisibilityState {visible_regions, visible_nodes, visibility_version, priority_policy}`.
  - The scheduling key `(is_visible, topological_height, node_id)`, with a starvation bound `MAX_VISIBILITY_DEFER`.
  - "Same operations + same visibility-event stream => same publication stream".
  - Final values are unchanged across policies (CONSTR-021, CONSTR-023).
- **Now**: `Interest` is used only for delivery (`session.rs:56-68`). Plan 8.1 even states that state separation "prevents a viewport move from triggering recalc".
- **Matters**: this is the owner's visible-first requirement, already specified. The non-obvious part is to record viewport moves as a replayable input stream, so runs stay deterministic.

**R4. An observer-driven "necessary" set (the Incremental/Anchors hybrid).** DEFERRED; the reason for deferring has lapsed.
- **Was** (DAG/07 lines 183-193 and 272):
  - node states clean, dirty and necessary;
  - "observer markers on output cells (visible viewport, chart data sources, external API consumers)";
  - recalculation from a height-ordered min-heap.
- **Why deferred**: DAG/09 kept Topo+SCC and reserved "hybrid internals for targeted next-step prototype lanes" because proofs were immature.
- **Now**: the dirty closure is ordered up front in `begin_recalc`.
- **Matters**: this is the route to Webs-class "work proportional to change". Excel's Automatic mode still computes everything, so "necessary" orders the work. It may drop work only in a declared mode (Manual, or a Webs profile).

**R5. Early cutoff with verifying traces.** DEFERRED, then LOST.
- **Was**:
  - "Early cutoff is the single most impactful optimization missing from Excel's architecture" (DAG/07 l.13).
  - Compare "the hashes of its actual inputs" (l.276), described as "Salsa-style backdating" (FEC).
  - Proof obligation DAG-PO-008.
- **Now**: none. The closure is fixed before evaluation starts.
- **Matters**: skipped evaluations are observable only through impure calls (UDF call counts, VBA, volatile functions), so cutoff needs per-function purity facts (O1).

**R6. Incremental topological order with a resumable stabiliser.** DEFERRED.
- **Was**:
  - Pearce-Kelly ordering, "O(δ·log δ)" per edge change (DAG/07 l.104).
  - Incremental's two heaps and `do_one_step_of_stabilize` (JS §4.2).
  - Promotion was tied to `PACK.dag.dynamic_topo_vs_rebuild`.
- **Now**: `RecalcWork` is kept, but the order is rebuilt for each closure, and the readers cache drops on any edge change.
- **Matters**: an up-front ordering costs O(closure) before the first visible cell can be computed.

**R7. Dynamic references as an overlay on the structural graph.** KEPT. "structural deps union runtime-observed deps" (CORE_ENGINE_THEORY §3.4); DAG-PO-006: same reads, same result; INDIRECT as observed reads, not volatile. Implemented per plan 6.2.

**R8. Three invalidation classes; external updates are operations.** KEPT in substance: `Standard`, `Volatile`, `ExternallyInvalidated` (CONSTR-016) and `OpExternalUpdate(topic_id, topic_seq)`. Still open is PP05's "Async completion is reified as an explicit external-update op that creates a later epoch", which fits async UDFs, off-worker OxVba calls and remote partitions.

**R9. Format and environment tokens.** LOST.
- **Was**: `FormatDepToken` (DEC-CALC-007). "Format changes that do not affect any format dependency token do not trigger recalculation" (FEC FORMAT-RULE-002).
- **Now**: setting the clock makes every formula pending (risk-layering).
- **Matters**: this is the invalidation half of principle 4.1, with one token per context facility.

**R10. Stable row and column identity; addresses are derived.** PLAN-ONLY. This is the costliest omission today.
- **Was**:
  - CONSTR-010: "ID-based (`RowId`/`ColId`/`CellId`), not coordinate-string-based".
  - FMI §11 warned that with a persistent tile DAG, "Row/col inserts are hard if tile coordinates are absolute".
  - It recommended a Roslyn-style hybrid: "immutable axis trees (row order and column order)", with tiles keyed by stable `(RowId, ColId)`.
- **Now**: `calc-doc-model` is exactly that tile DAG, with absolute coordinates.
  - The predicted failure has arrived: a row insert rewrites every formula, and undo rebuilds the engine.
  - Plan 3.2's "axis intervals/order maps" were never built.
- **Matters**: stable identity would make structural edits O(change) and give the stable node keys that dc-8abf lacks. FMI §11.4-11.5 already contains a benchmark matrix (W1-W6, T1-T3).

**R11. Ranges as graph nodes; formulas that differ only by offset share an identity.** Partly KEPT.
- **Was**:
  - "Range references `A1:A1000` add a single edge from a synthetic range node" (FEC D-RULE-001).
  - Regions "exist only in Layer 2" (FMI §3.2).
  - Nominal memoisation for formulas "structurally identical with offset" (DAG/07).
  - An R1C1-relative formula identity (`notes/GRID_EXTENSION_PROFILE_IMPACT_2026-06.md`).
- **Now**: the engine has rectangle edges, but still one node and one text per formula.
- **Matters**: this points to one node per formula block, with interval edges.

**R12. Deterministic spill rules.** Partly KEPT. Spill priority "row-major, then column-major"; on a spill change, only readers of "the *symmetric difference* of old and new spill ranges" are invalidated (FEC SPILL-RULE-002/004). Plan 6.2 has spill participants; spills still disable parallel recalc. Check the code against both rules.

**R13. Cycles: SCCs plus a declared iteration mode.** KEPT. A monotone fixed point was guaranteed only "when assumptions are declared" (DAG/09); plan 6.3 follows the order observed in Excel.

**R14. One publisher, workers on immutable snapshots, partitions later.** Partly KEPT.
- **Was**:
  - "Never call user code (UDF/plugin callback) while holding engine internal locks" (PP05; CONSTR-008).
  - A serialised lane for UDFs that are not thread-safe.
  - "Fixed-order reduction trees" for parallel aggregation.
  - Partitioning by snapshot (FEC Phase 3).
- **Now**: plan 3.5 keeps a single-thread oracle. Parallel recalc is native-only and turned off whenever a budget applies. The UDF registry is single-threaded.
- **Matters**: the serialised lane and the lock rule fit OxVba and XLL. Partitioning by snapshot is the corpus's only idea for distribution.

**R15. Results are independent of the schedule.** KEPT. "Parallel runs at fixed profile/seed produce bit-identical value outputs" (DAG-CONF-006). The reference twin (plan 6.5) checks this, and it is what makes R3-R6 safe to adopt.

**R16. Differential aggregates (DBSP-style, e.g. "SUM: add new − old").** REJECTED, for a good reason: an inverse update is not Excel's left fold, so it breaks bit-exactness (4.4). Usable only in a declared non-Excel profile, or for exact counts.

**R17. Analyzer-first introspection.** LOST.
- **Was**:
  - Incremental's `save_dot`, `For_analyzer` and per-node change stamps (JS §5.1).
  - DAG-CONF-001: recalc-order metadata.
  - Value-only versus trace-rich modes, with no "per-call trace structures in hot value paths" (AR §3.4, `b7dccb7`).
- **Matters**: this is the basis for the web formula debugger, and it costs nothing when turned off.

**R18. Calculation over ID-keyed nodes that are not on the grid.** REJECTED in October, with cause.
- **Was**:
  - "We don't have arbitrary bags of calculation nodes - they are rooted as tree(s)" (FMI).
  - DNA TreeCalc first, with "no coordinates" (CEFM §6.1.1).
  - Augmentation trees that "partake in the formal calculation 'from the outside'" (charts and controls).
- **Why rejected**: OCT dropped "the invented 'tree' reference language", about 75-85k lines in a second lane. That was the right call for an invented language and a duplicate lane.
- **Matters**: Webs-class definition from Rust or the web, strong typing, and the Tree and Canvas skins still need non-cell nodes. The open question is whether these can be ordinary participants in the one graph (for example, names or blocks with stable IDs) rather than a second language. The grid could then be one profile, `strict-excel-grid`.

## 2. Other ideas

**O1. The Formula Evaluation Context as declared facilities.** Partly KEPT.
- **Source**: `reference/conformance/excel-worksheet-engine/model/archive/fec-f3e-legacy/EXCEL_FORMULA_EVALUATION_CONTEXT_FEC.md`, `c6915f6`.
- **Was**:
  - "`FEC` is the set of host-provided facilities that are external to the pure formula/function/format core."
  - "A function or operator declares which `FEC` facilities it is allowed to observe".
  - A per-function `fec_dependency_profile` covering references, caller, time, random, external, locale and feature gates.
  - Checks that "no undeclared FEC facility is observed".
- **Now**: calc-fml has no evaluation context (risk-layering).
- **Matters**: this is owner principle 4.1 almost word for word. The same declaration would also drive the R9 tokens, supply the purity facts that R5 and parallel recalc need, and define what OneCalc must supply.

**O2. Optimiser metadata is part of the contract.** LOST.
- **Was**: use "resolved OxFunc call-site handles and metadata rather than repeatedly routing hot paths through broad string dispatch" (AR §3.4, `b7dccb7`; CONSTR-036/037).
- **Now**: functions are identified by their spelling.
- **Matters**: this is also the early-bound UDF and VBA path (4.5): resolve once at bind time, then call a typed handle.

**O3. One protocol surface for every writer.** PLAN-ONLY.
- **Was**:
  - "dispatch ops/transactions, query snapshots, subscribe to deltas, capability negotiation" (AR §3.1).
  - "no 'special cases' for UI, scripts, VBA, XLL, or collaboration" (CHARTER §2.2).
  - "COM automation is a separate facade layered on top of the identical protocol surface" (AR §3.7).
- **Now**: there is no command layer, and the object model lives in OxVba (risk-skins, risk-vba).

**O4. A canvas grid, tested through a RenderPlan IR.** PLAN-ONLY; the code drifted.
- **Was**:
  - "Never create DOM-per-cell" (BN §E).
  - Geometry and hit-test invariants UI-GEO-001..007.
  - "compare normalized JSON `RenderPlan` output, not pixels" (PP09).
- **Now**: the code rebuilds a `<div>` per cell on every scroll. 97 references in the test harness key on DOM attributes.

**O5. Scaling slopes from operation counters.** LOST.
- **Was**: "Time-only metrics are insufficient for complexity diagnosis". Fit log-log slopes of per-phase counters over a geometric ladder of input sizes (PP10; CONSTR-009).
- **Matters**: "nodes touched per one-cell edit versus workbook size" catches every O(workbook) cost in risk-engine §6. The check is deterministic and cheap enough to run on every commit (4.6).

**O6. Degradation classes and capability manifests.** KEPT. `Native/Lowered/Opaque/Rejected` and `profile_id + profile_version`; extend them to the 4.5 platform profiles (full on Windows, subsets on Linux and the web).

**O7. Capture discipline.** The Replay appliance as a system was REJECTED (08-30 investigation; OCT). Its cheap rules still hold: capture modes run from off to forensic, and "`forensic` is never allowed to change semantic outcomes"; no JSON on the hot path; shrinking "must fail explicitly" when the oracle is non-deterministic (`REPLAY_APPLIANCE.md`, `84b19a0`).

**O8. UDF/XLL admission states and lanes.** PLAN-ONLY. "discovery success does not imply enablement"; volatile and thread-safe flags at registration; "Keep a symbolic type layer first" (xll-non-interesting pack, pass 2). Typed signatures at registration are what an early-bound shim needs.

**O9. A function oracle as a product, "DNA Prove".** LOST.
- **Was**: it "Takes a function identifier + typed arguments, returns the Excel-correct result", shipped as a crate, as WASM and as an MCP server (`notes/SUGGESTION_DNA_PROVE_20260315.md`).
- **Matters**: a candidate home for the 4.4 research. Candidates and witnesses sit behind one interface, and the product links only the chosen program.

**O10. Evidence pinned to the Excel build, with both sources kept.** KEPT. `excel_exe_sha256`; disagreements "retained as explicit dual-source records" (`REFERENCE_SPEC_FORMAT_AND_CONFORMANCE.md`, `eb7a2ed`).

**O11. A small TLA+ model of the recalc protocol.** REJECTED (OCT: no formal methods), against the review's advice.
- **Was**: `INV-NoStaleCommit`, `INV-SnapshotConsistency`, `LIV-EventualStabilize` (PP03). The independent review said "TLA+ for the epoch/concurrency protocol: **Yes, definitely, do this**".
- **Matters**: incremental publication, cancellation, priority and partitions together are where interleaving bugs hide. A small model kept off the inner loop is cheap.

**Rejected for good reasons; do not revive:** dual Red/Blue engines; Lean and the OCaml oracle; rounds and packs; a second lane of any kind (OCT lesson 4); a full SAC runtime and a lock-free speculative coordinator (CORE_ENGINE_THEORY §4.2 and §4.6); inverse-update aggregates (R16).

## 3. Ranked list for the architecture review

1. **R1-R3: per-value epochs, per-node bundles and a recorded visibility stream.** These form the recalc contract that risk-engine §2 asks for. They were specified in February, keep determinism, and have a real caller (the viewport), so OCT lesson 3 is met.
2. **R10-R11: stable row and column identity with axis order maps, and one node per formula block with interval edges.** FMI predicted today's failure. Decide this before the save path and the function ports settle.
3. **R4 and R6: an observer-driven necessary set and a height-ordered heap, in the one scheduler.** This is the Webs-class route. Automatic mode still finishes the rest of the work.
4. **O1, R9 and O2: declared FEC facilities, invalidation tokens and resolved call-site metadata.** Together they deliver principle 4.1, end the clock-makes-everything-pending bug, supply the purity facts that cutoff and parallel recalc need, and give UDF and VBA calls an early-bound path.
5. **R5: early cutoff for functions declared pure**, checked against the reference twin, with every fallback counted (OCT lesson 2).
6. **O3: one protocol surface** for skins, agents, the VBA COM library and collaboration.
7. **O4: a RenderPlan IR and a canvas grid**, before the per-cell DOM harness grows further.
8. **O5: counter-slope checks** on every commit.
9. **R18 recast: non-grid nodes as participants in the one graph**, not a second language. Decide whether this belongs in the Webs-class definition surface.
10. **O11 and R17: a small TLA+ model of the new contract, plus analyzer-first tracing** for the debugger.

# Appendix H: research-libraries.md

> **Owner ruling 2026-10-31 (supersedes the licence conclusion below):** the rider keeps OpenAI/Anthropic themselves from taking the code for their own products; it does not stop us, using coding agents on our own behalf, from depending on, reading, fixing or improving these projects. asupersync is acceptable as a dependency, with a pointer to the rider in third-party notices; optionally ring-fenced behind a clean boundary with a simple non-concurrent path we maintain. The technical findings below stand.

# Research brief: libraries for DNA Calc's next-generation engine

Date: 2026-10-31. Sources fetched from the web on that date. Tags: **[V]** means verified on the cited page today.
**[K]** means prior knowledge that was not re-checked today. **[I]** means my inference. Star counts are point-in-time.

---

## 1. asupersync (Jeffrey Emanuel / Dicklesworthstone)

Repo: https://github.com/Dicklesworthstone/asupersync. Crate: https://docs.rs/crate/asupersync/latest.
README: https://raw.githubusercontent.com/Dicklesworthstone/asupersync/main/README.md.
Licence: https://raw.githubusercontent.com/Dicklesworthstone/asupersync/main/LICENSE.

### What it is (verified)
- An async runtime for Rust, presented as an alternative to tokio, with the tagline "Async runtime for Rust where correctness is structural". [V]
- **Runtime model.** The native runtime uses a multi-thread work-stealing scheduler (owner LIFO, thief FIFO) plus a blocking pool, and a current-thread flavour is available. [V]
  - Work is dispatched through three lanes in a fixed order: a Cancel lane (priority 200-255), then a Timed lane ordered earliest-deadline-first, then a Ready lane. [V]
  - Cancel preemption is bounded by `cancel_streak_limit=16`. [V]
  - `!Send` local tasks are pinned to their worker. [V]
- **Structured concurrency.** Every task belongs to a region, and "region close waits for all children". A region also cannot close until its obligations are resolved. `race!` cancels and drains the losers before it returns. [V]
- **Cancellation.** The protocol is request, then drain, then finalize. Cancellation is cooperative: a task stops only at `cx.checkpoint()` or at a cancel-aware await. [V]
  - A task that never reaches a checkpoint "is never forcibly stopped", and it holds up its region's close. [V]
  - `cx.masked(f)` defers cancellation, and finalizers run masked and budgeted. [V]
  - Budgets carry `deadline`, `poll_quota`, `cost_quota` and `priority` (0-255), but they are *advisory* on the production runtime. The deadline monitor only warns. [V]
  - `Outcome` has four values: Ok, Err, Cancelled and Panicked. [V]
  - Channels and oneshots use two-phase reserve/commit sends so that cancellation is safe. [V]
- **Determinism (lab runtime).** The lab runtime gives virtual time, seeded deterministic scheduling ("same seed, same execution") and trace capture and replay. [V]
  - A DPOR-style explorer exists but makes no completeness guarantee. [V]
  - 11 of the 24 oracles are fed from runtime state. [V]
  - The Lean proofs cover six invariants of an abstract model, not the Rust code. [V]
- **Capability model.** Spawning, timers and effects go through an explicit `&Cx`. [V]
  - Most I/O entry points take no `Cx`. They check the caller's context at runtime and refuse with error ASUP-E009 when the capability is missing. [V]
  - The README lists capability gaps around I/O and `spawn_blocking` as tracked work. [V]
- **Wasm/browser.** It compiles to `wasm32-unknown-unknown`, and there is a JS/TS "Browser Edition" built with wasm-bindgen. [V]
  - The browser runtime "is single-threaded" and runs on the main thread and in dedicated workers. [V]
  - Service and shared workers get a broker/coordinator role only. [V]
  - On the browser local executor, cancellation "drops the future instead of draining it". [V]
  - The README says "no wasm-side scheduler loop yet" and "no browser-engine test runs in CI". [V]
  - The JS packages are a release candidate that has not been published to npm. The Rust browser lane is marked "preview". [V]
- **Other surface.** The crate also provides:
  - Spork, an OTP-style actor and supervisor layer. [V]
  - MPSC, broadcast, watch and session channels. [V]
  - Fair cancel-safe Mutex and RwLock. [V]
  - Combinators: quorum, hedge, bulkhead, retry, circuit-breaker. [V]
  - A "remote runtime" with RaptorQ snapshots. The README does not claim multi-host or WAN operation. [V]
- **Maturity.** The README says "Experimental, pre-1.0" and "no known independent production user". [V]
  - Latest version on docs.rs is 0.5.0 (2026-09-12). There were about 30 releases since 0.1.0 (2026-02-04), and several early ones were yanked. [V]
  - It uses edition 2024. Default features build on stable Rust 1.95, but `?` on `Outcome` needs nightly, and contributors use a pinned nightly. [V]
  - There is no semver or API-stability promise. [V]
- **Size and activity.** The repo shows 25,055 commits on main, 288 stars and 32 forks. The source crate is 57 MB. [V]
  - Documentation coverage is 100% across 47k items. [V]
  - About 100 dependency entries. [V]
  - GitHub shows it was updated on 2026-10-31. [V]
- **Performance (author's own figures).** Spawn plus join is 12-23x slower than tokio, and a task "costs several times what a tokio task costs". [V]
- **Dependents.** These are almost all the author's own crates: sqlmodel-*, frankensearch-*, franken-snowflake-*, ftui-runtime, frankenlab and others. A handful come from third parties: oraclemcp-*, pureflow-*, koompi-kesa. See https://crates.io/api/v1/crates/asupersync/reverse_dependencies. [V]
- **Governance.** "I do not accept outside contributions for any of my projects". Issues and illustrative PRs are welcome, but the author won't merge them. [V]
- **Licence.** MIT plus an "OpenAI/Anthropic Rider" that "controls" in any conflict. [V]
  - It grants "no rights" to OpenAI, Anthropic, their affiliates, or anyone acting "on behalf of, for the benefit of, or under the direction of" them. [V]
  - It forbids anyone to "provide, disclose, distribute ... or otherwise permit access" to those parties. [V]
  - Its definition of "use" includes "analyzing, indexing" and incorporation into "any ... evaluation harness, or pipeline for ... automated systems". [V]
  - Distributions must carry the rider unmodified, and a breach terminates the licence automatically. [V]

### Fit for DNA Calc's engine (inference unless tagged)
- **Recalc scheduling.** [I] The fit is strong *in concept*.
  - A region per recalc wave, with child tasks per dependency stratum, matches the shape of a recalc.
  - "Region close implies quiescence" gives a clean "this recalc is done or fully abandoned" point.
  - The EDF lane and per-task `priority` could express "visible grid region first".
  - However, priority is a budget field, not a re-prioritisable queue. Raising priority when the viewport moves means cancelling and respawning, not reordering.
- **Cancelling stale recalcs.** [I] Request/drain/finalize semantics are a good fit for "view moved, abandon off-screen work". There are two catches:
  - Formula evaluation is CPU-bound synchronous code. Cancellation reaches it only if the evaluator calls `cx.checkpoint()` every N cells. That is a design choice DNA Calc must make whatever runtime it uses.
  - Two-phase commit is useful for publishing computed values to the grid only when a wave commits.
- **Distributing work across workers or threads.** [I] It is a poor fit for fine-grained cell work.
  - A per-task overhead of several times tokio's rules out one task per cell. Tasks would have to be coarse: per chunk or per stratum.
  - CPU-parallel evaluation of independent cells is better done with rayon or a custom chunk scheduler *inside* a task, which asupersync cannot see or cancel.
  - The remote runtime makes no multi-host claim, so it does not deliver the distributed or billion-node goal.
- **Deterministic testing of concurrent recalc.** [I] This is the strongest argument for it. Seeded replayable schedules and virtual time would let DNA Calc pin "recalc A cancelled mid-wave by edit B" interleavings as regression cases.
  - Comparable alternatives that need no runtime switch: shuttle (awslabs, Apache-2.0, randomised/PCT schedule testing) and loom for model-checking small kernels. [K]
- **Browser (wasm32-unknown-unknown in a web worker).** [V+I] It is weak today.
  - The runtime is single-threaded and preview-grade. There is no wasm-side scheduler loop, no browser CI, and cancel-by-drop rather than drain.
  - A worker-pool engine in the browser would still need its own multi-worker layer: SharedArrayBuffer with COOP/COEP headers, or message-passing workers.
  - The native and browser lanes have different semantics, which undermines "same engine everywhere".
- **Native.** [I] It works, but needs nightly for ergonomic `Outcome` handling. The crate is large (about 100 deps, 57 MB source), so builds get heavier, which is a cost given the known shared-target ICE history.

### Risks
1. **Licence (decisive).** [I] DNA Calc is built and reviewed by Claude agents, and Anthropic is a named Restricted Party.
   - Feeding asupersync source into Anthropic-hosted models plausibly counts as "disclose ... or otherwise permit access" and as "analyzing" in an "automated system".
   - The published product would have to carry the rider, and DNA Calc could not lawfully be provided to OpenAI or Anthropic or their affiliates.
   - This is a legal question for counsel. On the plain text, it conflicts with the project's AI-agent workflow and with an unrestricted publication.
   - The same rider is on frankensqlite, frankentui, frankenscipy, frankengraphdb and franken_engine (section 2).
   - Disclosure: this brief was produced by Claude. It read only the public README and LICENSE pages, not the source.
2. **Maturity.** Pre-1.0, no stability promise, a fast release cadence, yanked early versions, a nightly dependency, and some oracles and proofs unfinished. [V]
3. **Wasm.** The browser lane is preview, single-threaded and untested in CI. [V]
4. **Single maintainer, no outside merges.** The bus factor is 1, and DNA Calc fixes would have to be forked or upstreamed through issues. [V+I]
5. **Performance.** Per-task overhead runs against "extreme optimisation" unless task granularity is coarse. [V+I]

**Recommendation (inference).**
- Do not adopt it as the base. The licence alone settles that.
- Do borrow the design ideas, which are not licensed code: region-owned recalc waves, request/drain/finalize cancellation with explicit checkpoints, reserve/commit publication of results, cancel-lane priority, and seeded lab replay.
- Implement those ideas over a small in-house scheduler that also targets wasm workers.

## 2. Other work by the same author (verified to exist; agent-workflow tools skipped)

Repo list: https://github.com/Dicklesworthstone?tab=repositories&language=rust (82 Rust repos).
Every one checked carries the same MIT plus OpenAI/Anthropic rider. That rules out using any of them as a dependency of DNA Calc for the reason in risk 1.

- **frankensqlite** (`fsqlite`, https://github.com/Dicklesworthstone/frankensqlite). [V]
  - A ground-up Rust reimplementation of SQLite with page-level MVCC and concurrent writers, compatible with SQLite 3.x files.
  - The README calls it "real, but still hybrid". It needs nightly, depends on asupersync, and its wasm build has memory-only persistence (OPFS and IndexedDB raise errors).
  - Possible use as a workbook or session store. rusqlite or plain SQLite is safer.
- **frankentui** (https://github.com/Dicklesworthstone/frankentui). [V]
  - A terminal UI kernel of about 20 crates, with a diff renderer, Fenwick-tree variable-height virtualised lists, and a wasm/WebGPU web backend.
  - Status is WIP and it needs nightly.
  - Only relevant as a reference for virtualisation, or for a TUI front end.
- **frankengraphdb** (https://github.com/Dicklesworthstone/frankengraphdb). [V]
  - A property-graph database with an MVCC commit stream, `AS OF` time travel and `SUBSCRIBE` changefeeds. A DBSP-style engine is planned.
  - Much of the README describes a target state. It is built on asupersync and has no wasm.
  - Only a design reference for versioned dependency graphs.
- **frankenscipy** (https://github.com/Dicklesworthstone/frankenscipy). [V]
  - A clean-room port of SciPy: special functions (gamma, beta, erf/erfinv, Bessel, hypergeometric) and 95+ distributions.
  - It is differentially tested against SciPy 1.17.1 under named tolerances, not bit-exact.
  - Relevant only as a *reference* for statistical functions. Excel's bit-exact oracle remains Excel itself.
- **franken_engine** (https://github.com/Dicklesworthstone/franken_engine). [V]
  - A native Rust JS/TS engine (no V8 or QuickJS) with a typed-capability sandbox for untrusted extensions.
  - Self-described "research-grade", v0.2.0.
  - A conceptual match for agent-written skins or extensions, but not usable.
- **frankensearch** (https://github.com/Dicklesworthstone/frankensearch) is hybrid BM25 plus embedding local search. [V: exists, description only]
- **franken_numpy** and **frankenpandas** are clean-room NumPy and pandas ports. [V: exist, descriptions only] They are not needed, because Arrow covers columnar data.
- Tools the project already uses: `beads_rust` (br) and `beads_viewer_rust` (bv). [V]
  - These are tooling, not product dependencies, so the rider matters less there. [I]
  - Counsel should still note that Claude agents operate them. [I]

## 3. Other projects worth considering

**salsa** (https://github.com/salsa-rs/salsa, https://docs.rs/crate/salsa/latest).
- Facts:
  - "A generic framework for on-demand, incrementalized computation", inspired by Adapton and rustc's query system. [V]
  - Licensed Apache-2.0 or MIT. Version 0.28.5, released 2026-09-24. About 3.0k stars. [V]
  - Cancellation unwinds in-flight queries with `Cancelled::PendingWrite` when a write moves the database to revision R+1. [V: https://docs.rs/salsa/latest/salsa/enum.Cancelled.html]
  - Used by rust-analyzer and Astral's ty. [K]
  - Supports durability levels and early cutoff (backdating). [K]
- Fit [I]:
  - It is an excellent model for demand-driven evaluation: pull only what the viewport needs, with free memoisation and invalidation.
  - Its granularity is per query, with per-query overhead and a hash-map memo. A billion cells as salsa queries is unrealistic.
  - It suits sheet-level or block-level derived data (names, parsed formulas, range dependency summaries) better than per-cell values.
  - Its unwind-based cancellation is crude compared with a viewport-priority scheme.

**Adapton** (https://github.com/Adapton/adapton.rust).
- MPL-2.0. The repository was archived on 2026-09-08. [V]
- Useful only as theory (demanded computation graphs). Do not depend on it.

**differential dataflow / timely** (https://github.com/TimelyDataflow/differential-dataflow).
- MIT, about 3.0k stars, research-grade but long-lived. [V]
- Fit [I]:
  - Its strengths are distributed, data-parallel incremental computation over collections, including iteration.
  - A spreadsheet's irregular per-cell formula graph, with volatile functions and Excel's evaluation-order quirks, does not map naturally onto relational operators.
  - It is worth studying for the distributed-recalc goal: progress tracking and frontiers.
  - It is not a recalc engine. There is no wasm story. [I]

**DBSP / Feldera** (https://github.com/feldera/feldera, https://docs.rs/crate/dbsp/latest).
- Incremental computation whose updates are "proportional to the size of the change". [V]
- Repo licence MIT. `dbsp` is at 0.373.0, released 2026-10-31 (very fast churn). About 2.1k stars. [V]
- No wasm. [V: not mentioned]
- Fit [I]: the same relational mismatch as differential dataflow. It is a good fit for "spreadsheet over a streaming table" features (Power Query or data model style), not for core cell recalc.

**Apache Arrow (arrow-rs)** (https://docs.rs/crate/arrow/latest).
- Version 60.1.0, released 2026-10-28. Builds for `wasm32-unknown-unknown` with default features off. [V]
- Apache-2.0. [K]
- Fit [I]:
  - Strong for chunked columnar storage of *values*: typed columns, null bitmaps, zero-copy slices, and an IPC/FFI boundary to Python or JS.
  - Weak for mixed-type, sparse, mutable cells. You need a dense-chunk-plus-overlay design, which formualizer already uses (below).
  - The dependency weight on wasm needs measuring.

**IronCalc** (https://github.com/ironcalc/IronCalc).
- A "work-in-progress spreadsheet engine" with an xlsx reader and writer, wasm and Node bindings, and a browser preview. [V]
- MIT or Apache-2.0, about 4.2k stars. [V]
- The recalc model is not documented on the README. Its API shows an explicit `model.evaluate()`. [V]
- Fit [I]: a useful comparator and a source of xlsx and locale handling ideas. Its whole-model evaluate approach doesn't target demand-driven or billion-cell recalc.

**formualizer** (https://github.com/psu3d0/formualizer, https://docs.rs/crate/formualizer-eval/latest).
- An "Arrow-backed Excel formula engine with dependency graph and incremental recalculation". [V]
  - Uses a CSR edge format, cycle detection, and optional rayon parallelism.
  - Claims 320+ functions.
  - Has Python (PyO3) and wasm/npm bindings.
  - Version 0.13.0, released 2026-10-25. About 205 stars. MIT or Apache-2.0.
  - Calls itself "the fastest open-source spreadsheet engine" while saying formal benchmarks are in progress.
- Fit [I]:
  - It is the closest existing match to DNA Calc's storage and graph direction: Arrow chunks with spill overlays, and a CSR graph.
  - Worth a benchmark comparison against DNA Calc's own measured Excel corpus.
  - Its parallelism uses rayon with no cancellation or viewport priority.

**windows-rs** (https://github.com/microsoft/windows-rs).
- MIT or Apache-2.0, about 12.9k stars. [V]
- `windows-core` provides `#[interface]` and `#[implement]` for authoring COM. `windows-bindgen` and `windows-metadata` generate code from ECMA-335 (winmd) metadata. [V]
- The README does not mention typelib or IDispatch. [V]
- Fit [I]:
  - The natural base for in-process COM interop on Windows, for example exposing an Excel-like object model to a VBA engine, or hosting existing COM add-ins.
  - Typelibs are not winmd. Early binding from `.tlb` needs either a converter (tlb to RDL/winmd to bindgen) or a custom generator.

**msft-typelib** (https://docs.rs/crate/msft-typelib/latest).
- An "allocation-free parser for MSFT-format type library (.tlb) files". Version 0.2.0, released 2026-10-10. Apache-2.0. A small, new third-party crate (ATRAPSLLC). [V]
- Fit [I]:
  - Could feed a cross-platform early-binding layer: the VBA compiler resolves vtable slots and DISPIDs from the .tlb without Windows.
  - Its youth means it should be vendored and tested against the Excel and Office typelibs.

**intercom** (docs.rs/crate/intercom) generates typelibs from Rust COM components, at version 0.4. [V: exists] I have not checked whether it is still maintained.

**Wasm component model / WIT** (https://component-model.bytecodealliance.org/, https://github.com/bytecodealliance/jco).
- WASI 0.3.x is current and adds async `future` and `stream`. [V]
- jco transpiles components into ES modules for browsers and Node. It is Apache-2.0 with the LLVM exception. [V]
- wasmtime (Apache-2.0, about 18.8k stars) runs components natively. [V]
- wit-bindgen and cargo-component are the Rust tooling. [K]
- Fit [I]:
  - WIT is a good fit for the plugin, add-in and skin boundary: typed, language-neutral, sandboxed, and the same interface natively (wasmtime) and in the browser (jco).
  - Costs: copying across the boundary per call, so plugins should get range or batch calls, not per-cell calls. Browser support goes through transpilation rather than native component support.

**Cranelift** (https://docs.rs/crate/cranelift-codegen/latest, inside wasmtime).
- Version 0.137.1, released 2026-10-29. [V]
- Licence Apache-2.0 WITH LLVM-exception. Backends are x86-64, aarch64, riscv64 and s390x. [K]
- Fit for VBA and formula JIT [I]:
  - Native JIT is credible, and cranelift-jit and cranelift-module exist. [K]
  - It **cannot JIT inside the browser**, because wasm32 cannot create executable memory.
  - In the browser, the equivalent is to emit a wasm module at runtime (for example with `wasm-encoder`) and instantiate it through JS.
  - A shared mid-level IR with two backends (Cranelift native, wasm emission in the browser) is the realistic design.
  - The weekly 0.x breaking releases mean the version must be pinned.

**Browser threading helpers.**
- wasm-bindgen-rayon (rayon over web workers with SharedArrayBuffer, which requires COOP/COEP headers). [K]
- calamine (MIT, xlsx/xlsb reading). [K]
- Both are worth listing for the "same engine in a worker pool" and import paths.
