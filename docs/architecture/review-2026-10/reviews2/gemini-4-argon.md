# Second-Round Review — Gemini 4 Argon

## 1. Must-fix

1. **Defined-name memoisation predicate (`SYNTHESIS.md:126, 482`)**: "A name without relative references is one node with a memoised result" is wrong in three ways:
   - **Unqualified sheet (`calc-fml/src/reference/mod.rs:140-143, 205`)**: A workbook-scoped name `=$A$1` has only `AxisRef::Absolute` yet `SheetTarget::Anchor` resolves to the *caller's* sheet (`Sheet1!$A$1` vs `Sheet2!$A$1`).
   - **Caller-facility & volatile/stateful calls (`calc-engine/src/random.rs:19-28`)**: A name with no relative refs calling `ROW()`, `COLUMN()`, `SHEET()`, 1-arg `CELL()`, `FORMULATEXT()`, `INDIRECT()`, `OFFSET()` depends on caller anchor; a name calling `RAND()` (PRNG keyed by `at: CellKey`, which a workbook name lacks, and Excel draws per call site in `=MyRand-MyRand`) or a stateful UDF cannot memoise once per tick.
   - **Reference/empty coercion (`calc-fml/src/eval/run.rs:202-218` vs `:1001-1017`)**: Top-level `evaluate()` dereferences `CoreRef::Reference` (using caller-row `@` in `Legacy` form) and coerces `Empty -> 0.0` (`run.rs:214`). Inside a formula (`Instruction::Name`), a name returning `Sheet1!$A$1:$A$10` or a blank cell must remain an uncoerced `CalcValue::Reference` / `Empty` so `=ISREF(Nm)`, `=ROW(Nm)`, `=OFFSET(Nm,…)`, `=SUM(Nm)`, `=ISBLANK(Nm)`, and caller-row `@Nm` work.
   - **Correction**: Memoise a single shared `NameId` result only when the plan has no `AxisRef::Relative`, no `SheetTarget::Anchor`, and no caller-context, volatile, or stateful calls; cache the raw pre-coercion `CalcValue` and apply top-level reference/empty coercion only at cell root or notebook-name projection.

2. **Non-inlined `NameId` breaks `LAMBDA` closures (`SYNTHESIS.md:123, 244`)**:
   - Moving defined names out of inlined plan blocks (`NameId` binding) breaks every `LAMBDA` stored in a `DefinedName` (`MyFn = LAMBDA(x, x+1)`): `Closure` stores `plan: usize` (`Arc::as_ptr(&instance.plan)`) and `block: usize` (`calc-fml/src/eval/closure.rs:20-29`), and `calc-fml/src/eval/run.rs:961-997` refuses any closure where `closure.plan != plan_address(self.instance)` (`"a LAMBDA value made by another formula"`) while indexing `self.instance.plan.blocks` and `self.instance.areas` (the caller's plan/areas).
   - **Correction**: `Closure` must own `Arc<CompiledPlan>` plus its placed references (or caller-instantiated template references for caller-relative LAMBDAs), remove the `closure.plan != plan_address(self.instance)` refusal in `run.rs:961`, and switch the active `FormulaInstance` frame in `call_closure` and `NameId` evaluation.

3. **Defined-name off-grid wrapping is already implemented and measured (`SYNTHESIS.md:63-66, 494`)**:
   - Synthesis cites `calc-fml/src/reference/mod.rs:183-195` to claim the binder gives `#REF!` for off-grid relative references in defined names and treats name wrapping as unmeasured. In fact, `calc-fml/src/reference/mod.rs:236-284` (`instantiate_wrapped`, measured on Excel 16.0 on 2026-10-23) and `calc-fml/src/eval/compile.rs:1254-1260, 1398` (`self.wrapped.push(self.in_name)`) **already** wrap relative references in defined names modulo `MAX_ROWS`/`MAX_COLUMNS` because `.xlsx` stores defined-name relative offsets wrapped from `A1` (`RC[-1]` -> `XFD1`).
   - **Correction**: Remove the false bug claim in §0.3/§6.8; ensure `calc-syntax` `Template` binds defined names at `A1` with `wrapped = true` (where only `AxisRef::Absolute` shifts on structural edits, `transform.rs:25-30`), distinct from cell `FormulaBlock` templates.

4. **`.xlsb` conversion & `FormulaBlock` save pipeline will break `.xlsx` save (`SYNTHESIS.md:52-53, 110, 114, 361`)**:
   - **Fact check (`calc-doc/src/xlsb/sheet.rs:782-789, 873-883`)**: `xlsb` does *not* print every shared member's `Rgce` to A1 text; it prints only the `BrtShrFmla` master once and emits empty `<f t="shared" si=".."/>` for members (`load/sheet.rs:668-704` expands them).
   - **Save breakage 1 (`calc-doc/src/lib.rs:141-147`, `save/sheet.rs:39`)**: `calc-doc::open()` converts `.xlsb` into an in-memory XML `Package` because `calc-doc` saves by streaming/splicing opened XML parts. Bypassing XML `<f>` generation in `xlsb` leaves no XML `<f>` bytes for `save/sheet.rs` to copy on untouched formulas.
   - **Save breakage 2 (`calc-doc/src/save/diff.rs:23-54`, `save/structural.rs:1750-1780`, `save/author.rs:494, 591, 682`)**: With `FormulaBlock` ("covered cells hold no formula entry", `SYNTHESIS.md:110`), `author.rs:494` returns `Err(SaveRefusal::ValueWithoutFormula)` for every covered cell with a cached result! Moreover, when a row is inserted above an untouched block, its `TemplateId` (R1C1) is unchanged, but its `.xlsx` master `<f>` is A1 text (`=A2*2` -> `=A3*2`) and *must* be rewritten without unsharing (`save/structural.rs:1768` and `save/sheet.rs:233-245` currently unshare every moved group).
   - **Correction**: Update `save/{diff,structural,author,sheet}.rs` to diff and validate `FormulaBlock`s directly, synthesize or rewrite master `<f>` A1 text and `ref` from `FormulaBlock` + `Template` on structural shifts without unsharing non-straddling blocks, and keep `xlsb` populating the backing `Package` or generate `<f>` from `FormulaBlock` on save.

5. **`calc-syntax` vs `calc-fml` split does not compile as specified (`SYNTHESIS.md:356, 359`)**:
   - Synthesis puts `Template` (`AxisRef`) and `reference/transform.rs` in `calc-syntax` while keeping `reference/bind.rs` in `calc-fml`. `calc-fml/src/reference/transform.rs:32-38` (`transform_formula`, `transform_name`) and `Template` construction directly call `bind_references` and require `ReferenceCatalog`, `ReferenceOperand`, `SheetTarget`, and `AxisRef` (`reference/mod.rs`, `reference/bind.rs`) to resolve sheet qualifiers and 3-D spans (`Ja:Ma!A1`, which Excel does not shift on row/col edits, `transform.rs:19-23`).
   - **Correction**: Move `calc-fml/src/reference/{mod.rs, bind.rs, structured.rs, transform.rs}` into `calc-syntax` alongside `syntax/` and `notation.rs`.

6. **`calc-function-meta` omits 1,400+ lines of function tables in `calc-fml` and `calc-doc` (`SYNTHESIS.md:100, 355, 359`)**:
   - Beyond `calc-func` metadata and `file_form_table.rs`, `calc-fml` has 950+ lines of function-name tables: per-argument 3-D admission `THREE_D_TAKEN`/`THREE_D_VALUE`/`THREE_D_REF` (`calc-fml/src/eval/compile.rs:1503-2052`), legacy implicit-intersection parameter places `AGGREGATE_UNDER_ARRAY`/`AGGREGATE`/`ARRAY`/`intersects_reference` (`calc-fml/src/eval/legacy.rs:81-458`), and reference-array lifting `LEGACY_LIFTS_OVER_REFERENCES`/`LIFTS_OVER_REFERENCES`/`TAKES_REFERENCE_ARRAYS` (`calc-fml/src/eval/run.rs:1123-1159`). `calc-doc` also has `xlsb/functions.rs:7-485` (`Ftab` IDs/arities).
   - **Correction**: Put per-parameter 3-D admission, legacy `Place` class, reference-array lifting, and `Ftab` ID/arity into `calc-function-meta` keyed by `FunctionId`.

7. **Step 3a budgeted stepping disables native parallel recalculation (`SYNTHESIS.md:186, 381-382, 399, 444`)**:
   - `calc-engine/src/recalc.rs:644` guards `parallel::evaluate_levels` on `budget == WorkBudget::UNLIMITED`, and `parallel::eligible` (`recalc.rs:748-755`) requires `work.next == 0 && work.waves == 0 && work.spills.is_empty()`. Running the actor in finite `WorkBudget` chunks disables Rayon parallelism completely.
   - **Correction**: Make `parallel::evaluate_levels` accept a finite `WorkBudget` (chunking within a level) and resume across `work.next > 0` and staged spills.

8. **In-process transport fallback in `dna-calc` contradicts dropping the engine from the page wasm (`SYNTHESIS.md:222, 372, 381`)**:
   - `SYNTHESIS.md:372` says `dna-calc` never sees `calc-host`/`calc-engine`, while `SYNTHESIS.md:381` keeps the "in-process transport fallback". In `apps/dna-calc/src/port.rs:47, 154-159`, `Transport::InProcess(Box<Server>)` lives inside the page crate, forcing the page wasm to link `Server`, `calc-host`, and `calc-engine`.
   - **Correction**: Put a transport trait in `dna-calc-model`; the browser `dna-calc` binary links only `WorkerTransport`, while native tests inject `InProcessTransport(Server)` from `calc-harness`.

9. **Citation & behavior error on `Soft` x87 transcendentals (`SYNTHESIS.md:260-262, 439`)**:
   - `calc-func/src/ported/excel_numeric/mod.rs:9-15` does *not* refuse transcendentals; `soft87` currently computes 1-ulp-rounded `F2XM1`/`FYL2X`/`FPATAN`/`FSIN`/`FCOS`/`FPTAN`. If `Soft` is changed to refuse unmodelled transcendentals under `Exact` before a vendor model is identified, every formula using `^`, `POWER`, `EXP`, `LN`, `LOG10`, trig, or `PMT`/`PV`/`FV`/`IRR`/`RATE` will return `Refused` in the browser and on Intel hosts.
   - **Correction**: Note that `soft87` currently approximates rather than refuses; only flip `Soft` transcendentals to `Refused` in `Exact` mode as each Zen microcode routine is identified, or keep `Fast` as the interactive default when no vendor model exists.

10. **`FunctionContext` `HostQuestions` omits half of `HostFacts`/`WorkbookFacts` (`SYNTHESIS.md:90, 358`)**:
    - Kernels in `calc-func/src/call.rs:113-216` also require `is_external_sheet` (`SUMIF`/`COUNTIF`/`OFFSET`/`CELL`/`ISFORMULA`/`FORMULATEXT`), `is_formula` (`ISFORMULA`), `formula_text` (`FORMULATEXT`, rendering `Template` + `SpellingOverride` at `cell`), `sheet_index`/`sheet_name`/`sheet_named`/`sheet_count` (`SHEET`/`SHEETS`/`CELL`), `locked`/`color`/`parentheses` (`CELL`), `active_cell` (1-arg `CELL`), and `top_left_cell`/`calculation_mode`/`application` (`INFO`). Also `WhatIf::evaluate` (`calc-engine/src/data_table.rs:248-253`) drops `random` and `functions`, not just `host_facts`.

## 2. Missing

1. **Authoring & saving defined names (`calc-doc/src/save/author.rs:731-758`, `save/parts.rs:25-39`)**: Today `author.rs:731` assumes no edit removes a file name (`parts.rs:28-29` only deletes `_dna.block.*`) and `author.rs:752` writes `now.formula` into `<definedName>` **without calling `file_form_with_links`**! Authoring/deleting names (ruling 4.8, `Names` command) will write unprefixed `_xlfn.LAMBDA`/`LET`/`XLOOKUP`/`_xlfn._xlws.`/`_xlpm.` into `<definedName>` (corrupting `.xlsx` for Excel) and cannot delete user names.
2. **Saved VBA `Range` objects outliving `FEC` (`docs/plan/VBA_UDFS.md:33-34`, `calc-vba/src/lib.rs:424-509`)**: A VBA UDF can save a `Range` parameter in a module global (`Set g = r`) during call 1 and read `g.Value2` in call 2. `calc-object-model` `Range` handles must store value descriptors (`SheetId`, `GridArea`) and resolve against the active thread/actor invocation context slot (refusing when no call is active), never borrowing a single call's stack `FEC`.
3. **Internal VBA `Range` reads: no dependency edges, no stateful replay (`docs/plan/VBA_UDFS.md:91-93`, `calc-engine/src/recalc.rs:611-614`)**: Excel does *not* add dependency edges for cells a VBA UDF reads internally outside its formula arguments (`VBA_UDFS.md:91-93`), so `EffectSink` must not record them as dynamic graph edges. However, if such a read hits an uncalculated formula cell, `recalc.rs:611-614` currently returns `Attempt::Waits` and re-runs the formula later—replaying the stateful VBA UDF and double-mutating its `Static`/module globals! Step 1 (`dc-snz.6`) must evaluate uncalculated prerequisites synchronously on demand (with cycle detection) without recording a dependency edge.
4. **Remaining `axis_edits` replays (`calc-host/src/state.rs:621-754`, `conditional.rs:730-776`, `facts.rs:182-191, 217`, `calc-doc/src/save/structural.rs:1885-1935`)**: §1.2 folds only `LoadedSheet.styles` into `ModelRoot`, leaving `merged_areas`, `validations`, `sheet_view`, and `sheet_review` replaying `axis_edits` on every query (`facts.rs` also forgets to shift `active_cell`/`top_left_cell` or update `active_sheet`), and `save/structural.rs:1885-1935` runs `O(axis_edits)` full XML splice passes on save.

## 3. The open disagreements (section 5.2, A–J)

- **A**: `FormulaBlock` storage with a per-member dirty/ready bitset, affine block edges, and 64-row scheduling slices.
- **B**: Two sub-steps: Step 3a ships the budgeted actor and yield loop; Step 3b immediately ships validated partial publication on scalar nodes before Step 5 blocks.
- **C**: Ship `ready_count` and a static/bound-edge height-ordered min-heap in Step 3b; defer Pearce-Kelly dynamic height maintenance until counters show dynamic-edge churn requires it.
- **D**: `DemandSet` defines completion obligation (`Automatic` = all dirty nodes; `Manual` = requested target only); `InterestSet` only orders ready work and never drops demanded nodes.
- **E**: Put symbolic search in a workspace tool crate `tools/calc-func-lab` (depending on `calc-numeric` and `calc-verdict`) so `xtask` stays free of product dependencies.
- **F**: Separate `calc-object-model` crate (`calc-value` + `calc-fml` ceiling) shared by `calc-vba` (UDF-time read-only traits) and `calc-host-vba` (macro-time read/write traits).
- **G**: Build `RenderPlan` IR and the binary tile mirror first, then benchmark recycled DOM vs Canvas2D + accessibility overlay on real scroll traces before switching.
- **H**: Keep formatting as module `calc-fml::format` implementing `calc-func::NumberText` (`calc-doc` drops `calc-fml`, so no separate crate is needed).
- **I**: Keep the fast `Cargo.toml` DAG check in `xtask check` (it takes <10 ms on `cargo metadata`); leave AST/source scans to `xtask sweep`.
- **J**: Bump `CallableTable` generation on VBA reload with lease-pinned in-flight compiled projects and lazy plan rebinding; never swap pointers in place without invalidating cached arity/volatility.

## 4. The owner decisions (section 6, 1–10)

1. **Agree**: Keep `calc-value` + `calc-func` (plus leaf `calc-function-meta` and `calc-numeric`) and `calc-doc-model` + `calc-doc`.
2. **Agree**: Ship `FormulaEvaluationContext` (`SemanticEnv`, `VolatileServices`, `CallableTable`, `EffectSink`) in Step 1 (`dc-snz.6`).
3. **Agree**: Put `NumberText` formatting in `calc-fml::format` and drop `calc-doc -> calc-func`/`calc-fml` via `calc-function-meta` + `calc-syntax`.
4. **Agree with Must-fix #1**: Use `NameId` in `ModelRoot`/`GraphState` and notebook-over-names, but only memoise a single shared `NameId` result when the name has no `AxisRef::Relative`, no `SheetTarget::Anchor`, and no caller/volatile/stateful calls, storing the uncoerced `CalcValue`.
5. **Agree with Must-fix #9**: `Exact` (refusing unmodelled transcendentals once Zen models are in place) vs `Fast` (`f64`), defaulting to `Exact` in `calc-verdict` and `Fast` in interactive hosts where no vendor model is available yet.
6. **Agree**: Actor loop over `WorkBudget` with Step 3a (budgeted yield) followed immediately by Step 3b (validated partial publication + `DemandSet`/`InterestSet`).
7. **Agree**: Separate `calc-object-model` crate with value-descriptor `Range` handles bound to the active invocation slot.
8. **Agree**: Defer the grid coordinate-representation switch until `FormulaBlock` and `StyleRuns` land; remove `NameTable` wrapping from this item (already implemented in `calc-fml/src/reference/mod.rs:236-284`).
9. **Agree**: Binary `TileBatch` protocol + worker-maintained `RenderPlan` first; hold any Canvas2D renderer change on measured frame-time benchmarks.
10. **Agree**: Keep `xtask check` fast, add the `Cargo.toml` DAG check to `xtask check`, and add workspace/dependency-budget checks to `xtask sweep`.

## 5. Implementation order (section 4)

1. **`calc-syntax` and `Template` are in the wrong order**: Step 2 (`NameTable` holding `Template`s in `calc-doc-model`) requires `calc-syntax` and `Template`, which §4 places in Step 5. Extract `calc-syntax` (including `reference/{mod,bind,structured,transform}`) and `Template` **before Step 2** (or as Step 2a).
2. **Step 1 is too large as one step**: Split into **Step 1a** (`calc-function-meta`, `calc-fml::format` + `NumberText`, `SemanticEnv`/`VolatileServices` split, fixing `WhatIf` context) and **Step 1b** (`FEC`, `CallableTable`, `EffectSink`, `calc-object-model` vtable dispatch, synchronous uncalculated-cell evaluation for VBA `Range` reads in `dc-snz.6`).
3. **Step 4 is too large as one step**: Split into **Step 4a** (missing host commands, `dna-calc-model`, shell keymap, removing `InProcess(Box<Server>)` from the browser `dna-calc` binary) and **Step 4b** (binary `TileBatch` wire format, page tile mirror, and `RenderPlan` IR).
4. **Step 5 is too large as one step**: Split into **Step 5a** (`FormulaBlock` in `calc-doc-model`/`calc-doc` load & save diff/author/structural rewrite), **Step 5b** (engine block result columns, affine block edges, straddle splitting), and **Step 5c** (`StyleRuns`, typed CF/DV in `ModelRoot`, and folding remaining `LoadedSheet` `axis_edits` state into `ModelRoot`).
