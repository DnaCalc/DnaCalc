# Complete core bead graph

**204 tasks · 48 epics · 252 total records.** Full descriptions and named acceptance cases are in `beads.plan.jsonl`.

An arrow `←` names blocking prerequisites. Epic membership is listed by the grouping and is not a blocking wait for epic closure.

## M0: Bootstrap checkpoint — independent lanes, CI and evidence safety

### `infra-m0` — Infrastructure

- **`infra-m0.1`** Create the monorepo and isolated crate workspaces (P0) ← none
- **`infra-m0.2`** Install Windows Linux and browser CI (P0) ← `infra-m0.1`, `infra-m0.3`
- **`infra-m0.3`** Build the acceptance runner and architecture gates (P0) ← `infra-m0.1`
- **`infra-m0.4`** Pin legacy sources and resolve inaccessible migration inputs (P0) ← `infra-m0.1`
- **`infra-m0.5`** Quarantine questionable numeric provenance (P0) ← `infra-m0.4`
- **`infra-m0.6`** Back up and content-address existing oracle evidence (P0) ← `infra-m0.4`

### `cfunc-m0` — CalcFunc

- **`cfunc-m0.1`** Define portable values IDs and numerical policy (P0) ← `infra-m0.1`, `infra-m0.4`

### `cdoc-m0` — CalcDoc

- **`cdoc-m0.1`** Define engine-neutral authored operations and revisions (P0) ← `infra-m0.1`, `cfunc-m0.1`

## M1: First functional milestone — real Excel open/recalc/edit/save/reopen

### `infra-m1` — Infrastructure

- **`infra-m1.1`** Unify cache and live-oracle verdicts (P0) ← `infra-m0.3`, `cfunc-m0.1`
- **`infra-m1.2`** Implement one isolated Excel COM capture adapter (P0) ← `infra-m1.1`, `infra-m0.4`
- **`infra-m1.3`** Prove the first real workbook round trip (P0) ← `chost-m1.2`, `infra-m1.2`

### `cfunc-m1` — CalcFunc

- **`cfunc-m1.1`** Generate the initial function catalogue and call interface (P0) ← `cfunc-m0.1`, `infra-m0.3`
- **`cfunc-m1.2`** Port base coercions and scalar arithmetic (P0) ← `cfunc-m1.1`, `infra-m0.4`

### `cfml-m1` — CalcFml

- **`cfml-m1.1`** Port the lossless Excel lexer and basic parser (P0) ← `infra-m0.1`, `infra-m0.3`, `infra-m0.4`, `cfunc-m0.1`
- **`cfml-m1.2`** Implement typed reference binding without the engine (P0) ← `cfml-m1.1`, `cfunc-m1.1`
- **`cfml-m1.3`** Expose reusable compiled plans and evaluation frames (P0) ← `cfml-m1.2`, `cfunc-m1.2`

### `ceng-m1` — CalcEngine

- **`ceng-m1.1`** Create workbook-wide graph storage and literal input access (P0) ← `infra-m0.3`, `cfunc-m0.1`, `cdoc-m0.1`
- **`ceng-m1.2`** Calculate the thin slice through CalcFml (P0) ← `ceng-m1.1`, `cfml-m1.3`

### `cdoc-m1` — CalcDoc

- **`cdoc-m1.1`** Implement bounded ZIP package reading and raw preservation (P0) ← `infra-m0.3`, `cdoc-m0.1`, `infra-m0.4`
- **`cdoc-m1.2`** Load scalar cells formulas and file-cache provenance (P0) ← `cdoc-m1.1`
- **`cdoc-m1.3`** Author cell changes and write a repair-free thin workbook (P0) ← `cdoc-m1.2`

### `chost-m1` — CalcHost

- **`chost-m1.1`** Commit authored and engine edits as one transaction (P0) ← `cdoc-m1.3`, `ceng-m1.2`
- **`chost-m1.2`** Expose open recalc save and delta projections (P0) ← `chost-m1.1`

### `onecalc-m1` — DNA OneCalc

- **`onecalc-m1.1`** Ship the engine-free OneCalc application controller (P0) ← `cfml-m1.3`, `infra-m0.3`

## M2: Shared formulas and all four skins over one workbook

### `infra-m2` — Infrastructure

- **`infra-m2.1`** Prove cross-skin xlsx persistence with Excel edits (P0) ← `dnacalc-m2.6`, `chost-m2.4`, `infra-m1.2`

### `cfml-m2` — CalcFml

- **`cfml-m2.1`** Canonicalize R1C1 templates and binding-context keys (P0) ← `cfml-m1.3`
- **`cfml-m2.2`** Invalidate reusable plans by semantic dependencies (P0) ← `cfml-m2.1`
- **`cfml-m2.3`** Compile LET LAMBDA closures and invocation (P0) ← `cfml-m2.1`, `cfunc-m1.1`
- **`cfml-m2.4`** Port metadata-driven lazy control forms (P0) ← `cfml-m2.3`
- **`cfml-m2.5`** Expose incremental editing and formula-language services (P0) ← `cfml-m2.1`
- **`cfml-m2.6`** Own number-format parsing and numeric text policy (P0) ← `cfml-m1.3`

### `ceng-m2` — CalcEngine

- **`ceng-m2.1`** Instantiate shared formula regions without a second evaluator (P0) ← `ceng-m1.2`, `cfml-m2.2`

### `cdoc-m2` — CalcDoc

- **`cdoc-m2.1`** Persist view manifests and stable locator names (P0) ← `cdoc-m1.3`

### `chost-m2` — CalcHost

- **`chost-m2.1`** Define capability-organized skin intents and subscriptions (P0) ← `chost-m1.2`, `cdoc-m2.1`
- **`chost-m2.2`** Implement canonical commands and undo journal (P0) ← `chost-m2.1`
- **`chost-m2.3`** Implement block editing on ordinary worksheet anchors (P0) ← `chost-m2.2`
- **`chost-m2.4`** Reconcile non-Excel layouts after real Excel edits (P0) ← `chost-m2.3`

### `onecalc-m2` — DNA OneCalc

- **`onecalc-m2.1`** Add OneCalc editing traces formats and callable fixtures (P0) ← `onecalc-m1.1`, `cfml-m2.3`, `cfml-m2.5`, `cfml-m2.6`

### `dnacalc-m2` — DNA Calc

- **`dnacalc-m2.1`** Build shared shell and desktop/browser transport (P0) ← `chost-m2.1`, `infra-m0.2`
- **`dnacalc-m2.2`** Implement the notebook skin editor (P0) ← `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`
- **`dnacalc-m2.3`** Implement the tree editor skin (P0) ← `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`
- **`dnacalc-m2.4`** Implement the free-form canvas skin (P0) ← `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`
- **`dnacalc-m2.5`** Implement the Excel skin minimum grid and formula bar (P0) ← `dnacalc-m2.1`, `cfml-m2.5`
- **`dnacalc-m2.6`** Show formula details and dependency navigation in every skin (P0) ← `dnacalc-m2.2`, `dnacalc-m2.3`, `dnacalc-m2.4`, `dnacalc-m2.5`, `cfml-m2.3`

## M3: Workbook-wide references, arrays, names and extension attachment points

### `infra-m3` — Infrastructure

- **`infra-m3.1`** Capture and reconcile the complete reference Excel UI inventory (P0) ← `infra-m1.2`, `chost-m2.2`
- **`infra-m3.2`** Prove the unified workbook engine milestone (P2) ← `ceng-m3.4`, `ceng-m3.5`, `ceng-m3.6`, `ceng-m3.7`, `cfml-m3.4`, `chost-m3.1`, `infra-m1.2`, `infra-m3.3`, `ceng-m3.9`
- **`infra-m3.3`** Complete the advanced skin persistence interchange matrix (P0) ← `infra-m2.1`, `dnacalc-m3.1`, `cdoc-m3.1`, `cdoc-m3.3`, `chost-m2.4`, `infra-m1.2`
- **`infra-m3.20`** Plan charting and drawing objects implementation (P2) ← `cdoc-m3.5`, `ceng-m3.8`, `chost-m3.2`
- **`infra-m3.21`** Plan pivot tables caches and getpivotdata implementation (P2) ← `cdoc-m3.5`, `ceng-m3.8`, `chost-m3.2`
- **`infra-m3.22`** Plan printing page layout and export implementation (P2) ← `cdoc-m3.5`, `chost-m3.2`
- **`infra-m3.23`** Plan external data connections and analytical integrations implementation (P2) ← `ceng-m3.7`, `chost-m3.2`
- **`infra-m3.24`** Plan extended desktop command and dialog surface implementation (P2) ← `chost-m3.2`, `infra-m3.1`

### `cfunc-m3` — CalcFunc

- **`cfunc-m3.1`** Port text and Unicode functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.2`** Port numeric rounding and integer functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.3`** Port transcendental and trigonometric adapters (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.4`** Port aggregate and counting functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.5`** Port conditional aggregates and criteria matching (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.6`** Port lookup and reference-returning functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.7`** Port date and time functions (P1) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.8`** Port business-calendar functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.9`** Port distribution functions in bounded dispatch batches (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.10`** Port descriptive statistics and ranking functions (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.11`** Port regression correlation and forecast functions (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.12`** Port financial cash-flow functions (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.13`** Port securities and depreciation functions (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.14`** Port complex and engineering functions (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`
- **`cfunc-m3.15`** Port database and filtering aggregates (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.16`** Port dynamic array transformation functions (P1) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.17`** Port callable array helpers (P2) ← `cfunc-m1.2`, `infra-m0.4`, `cfml-m2.3`
- **`cfunc-m3.18`** Port information and host-context functions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.19`** Port volatile and provider-backed function adapters (P1) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.20`** Port modern text and locale-sensitive catalogue additions (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.21`** Port remaining logical and compatibility aliases (P2) ← `cfunc-m1.2`, `infra-m0.4`
- **`cfunc-m3.22`** Declare numerical sensitivity and generate backend routing (P1) ← `cfunc-m1.1`
- **`cfunc-m3.23`** Implement the deterministic portable Fast backend (P0) ← `cfunc-m3.22`
- **`cfunc-m3.24`** Implement a reviewed native Exact instruction adapter (P2) ← `cfunc-m3.22`, `infra-m0.5`
- **`cfunc-m3.25`** Audit complete catalogue migration and generated outputs (P2) ← `cfunc-m3.1`, `cfunc-m3.2`, `cfunc-m3.3`, `cfunc-m3.4`, `cfunc-m3.5`, `cfunc-m3.6`, `cfunc-m3.7`, `cfunc-m3.8`, `cfunc-m3.9`, `cfunc-m3.10`, `cfunc-m3.11`, `cfunc-m3.12`, `cfunc-m3.13`, `cfunc-m3.14`, `cfunc-m3.15`, `cfunc-m3.16`, `cfunc-m3.17`, `cfunc-m3.18`, `cfunc-m3.19`, `cfunc-m3.20`, `cfunc-m3.21`, `cfunc-m3.22`, `cfunc-m3.23`, `cfunc-m3.24`

### `cfml-m3` — CalcFml

- **`cfml-m3.1`** Bind scoped and caller-relative defined names (P1) ← `cfml-m2.2`
- **`cfml-m3.2`** Bind structured tables and 3D sheet spans (P1) ← `cfml-m3.1`
- **`cfml-m3.3`** Report dynamic references and reference algebra reads (P1) ← `cfml-m3.2`
- **`cfml-m3.4`** Preserve array evaluation and legacy array contexts (P1) ← `cfml-m2.4`, `cfunc-m3.16`
- **`cfml-m3.5`** Bind external workbook references through a typed catalogue (P1) ← `cfml-m3.2`

### `ceng-m3` — CalcEngine

- **`ceng-m3.1`** Index ranges whole axes and absence dependencies (P1) ← `ceng-m2.1`, `cfml-m3.3`
- **`ceng-m3.2`** Schedule scoped name instances and forward lookups (P1) ← `ceng-m3.1`, `cfml-m3.1`
- **`ceng-m3.3`** Update dynamic read overlays atomically (P1) ← `ceng-m3.1`, `cfml-m3.3`
- **`ceng-m3.4`** Implement spill allocation and blocker invalidation (P1) ← `ceng-m3.3`, `cfml-m3.4`
- **`ceng-m3.5`** Calculate sheet spans and table-structure dependencies (P1) ← `ceng-m3.2`, `cfml-m3.2`
- **`ceng-m3.6`** Replay volatile time and random effects per tick (P1) ← `ceng-m3.3`, `cfunc-m3.19`
- **`ceng-m3.7`** Resolve external workbook snapshots and link updates (P1) ← `ceng-m3.3`, `cfml-m3.5`
- **`ceng-m3.8`** Attach generic derived participants to the workbook graph (P1) ← `ceng-m3.5`, `cdoc-m3.5`
- **`ceng-m3.9`** Unify connected open workbooks in one calculation domain (P2) ← `ceng-m3.7`

### `cdoc-m3` — CalcDoc

- **`cdoc-m3.1`** Read and write workbook and sheet defined names (P1) ← `cdoc-m2.1`
- **`cdoc-m3.2`** Read and write structured table definitions (P1) ← `cdoc-m3.1`
- **`cdoc-m3.3`** Preserve shared array and dynamic-array formula encodings (P1) ← `cdoc-m1.3`, `cfml-m3.4`
- **`cdoc-m3.4`** Preserve external links richData and cell metadata (P2) ← `cdoc-m1.3`
- **`cdoc-m3.5`** Model chart drawing pivot and page-setup attachment data (P1) ← `cdoc-m3.2`
- **`cdoc-m3.6`** Model styles themes rows columns and panes (P1) ← `cdoc-m1.3`

### `chost-m3` — CalcHost

- **`chost-m3.1`** Publish calculation generations and object revisions (P1) ← `chost-m2.4`, `ceng-m3.8`, `ceng-m3.4`
- **`chost-m3.2`** Expose generic refresh and render attachment interfaces (P2) ← `chost-m3.1`
- **`chost-m3.3`** Implement canonical named-function block editing (P0) ← `chost-m2.3`, `cdoc-m3.1`, `ceng-m3.2`, `cfml-m2.3`

### `dnacalc-m3` — DNA Calc

- **`dnacalc-m3.1`** Complete named-callable and spill inspection across all skins (P0) ← `dnacalc-m2.6`, `ceng-m3.2`, `ceng-m3.4`, `cfml-m3.4`, `chost-m3.1`, `chost-m3.3`

## M4: Structural editing, transactions and calculation settings

### `infra-m4` — Infrastructure

- **`infra-m4.1`** Prove edit recalc and save failure consistency (P2) ← `chost-m4.9`, `chost-m4.8`, `ceng-m4.5`, `infra-m1.2`

### `cfunc-m4` — CalcFunc

- **`cfunc-m4.1`** Expose a source-neutral mutable UDF registry (P2) ← `cfunc-m3.22`

### `cfml-m4` — CalcFml

- **`cfml-m4.1`** Transform references for copy fill move and paste (P1) ← `cfml-m3.3`
- **`cfml-m4.2`** Transform references for cell axis and sheet lifecycle edits (P1) ← `cfml-m4.1`
- **`cfml-m4.3`** Bind UDF generations and asynchronous result tokens (P2) ← `cfml-m3.3`, `cfunc-m4.1`

### `ceng-m4` — CalcEngine

- **`ceng-m4.1`** Apply structural patches without rebuild-from-input (P1) ← `ceng-m3.5`, `cdoc-m4.2`, `cfml-m4.2`
- **`ceng-m4.2`** Implement calculation modes and recalc command scopes (P1) ← `ceng-m4.1`, `ceng-m3.6`
- **`ceng-m4.3`** Apply date precision and calcPr policies (P1) ← `ceng-m4.2`, `cfml-m2.6`, `cfunc-m3.7`
- **`ceng-m4.4`** Invalidate all numeric results on mode change (P2) ← `ceng-m4.3`, `cfunc-m3.23`
- **`ceng-m4.5`** Publish counted escalation facts and replay traces (P2) ← `ceng-m4.1`
- **`ceng-m4.6`** Apply external RTD and async completion operations (P2) ← `ceng-m3.9`, `cfml-m4.3`

### `cdoc-m4` — CalcDoc

- **`cdoc-m4.1`** Implement persistent sparse pages and axis order maps (P1) ← `cdoc-m3.3`
- **`cdoc-m4.2`** Prepare reversible cell range and axis patches (P1) ← `cdoc-m4.1`, `cfml-m4.2`
- **`cdoc-m4.3`** Author sheet lifecycle and relationship changes (P2) ← `cdoc-m4.2`

### `chost-m4` — CalcHost

- **`chost-m4.1`** Prepare one atomic operation stream for structural edits (P1) ← `chost-m3.1`, `ceng-m4.1`
- **`chost-m4.2`** Implement core range editing and clipboard commands (P2) ← `chost-m4.1`, `cfml-m4.1`
- **`chost-m4.3`** Implement sheet name table and workbook lifecycle commands (P2) ← `chost-m4.1`, `cdoc-m4.3`, `cdoc-m3.2`
- **`chost-m4.4`** Implement sort filter and visibility commands (P2) ← `chost-m4.2`, `ceng-m3.5`
- **`chost-m4.5`** Implement core cell formatting and style commands (P2) ← `chost-m4.1`, `cfml-m2.6`, `cdoc-m3.6`
- **`chost-m4.6`** Implement find replace and navigation query commands (P2) ← `chost-m4.2`
- **`chost-m4.7`** Implement workbook protection and permission checks (P2) ← `chost-m4.3`, `chost-m4.5`
- **`chost-m4.8`** Expose application-level recalc and numeric preferences (P2) ← `chost-m4.1`, `ceng-m4.4`
- **`chost-m4.9`** Complete bounded undo redo and save snapshot retention (P2) ← `chost-m4.1`
- **`chost-m4.10`** Coordinate multiple open documents and cross-book transactions (P2) ← `chost-m4.8`, `ceng-m3.9`
- **`chost-m4.11`** Add events and exclusive batch execution mode (P2) ← `chost-m4.10`, `ceng-m4.6`
- **`chost-m4.12`** Expose object dispatch over host commands (P2) ← `chost-m4.11`
- **`chost-m4.13`** Reserve add-in callback capabilities IDE panels and range picking (P2) ← `chost-m4.12`

## M5: Iterative calculation, data tables, conditional formatting and validation

### `infra-m5` — Infrastructure

- **`infra-m5.1`** Capture iterative and data-table scheduling witnesses (P3) ← `infra-m1.2`
- **`infra-m5.2`** Capture CF DV and precision-as-displayed witnesses (P3) ← `infra-m1.2`
- **`infra-m5.3`** Prove hard-calculation first-version coverage (P3) ← `chost-m5.1`, `chost-m5.2`, `chost-m5.3`, `infra-m5.2`, `ceng-m5.9`

### `cfml-m5` — CalcFml

- **`cfml-m5.1`** Compile per-cell rule contexts for CF and DV (P3) ← `cfml-m4.2`, `cdoc-m5.2`, `cdoc-m5.3`

### `ceng-m5` — CalcEngine

- **`ceng-m5.1`** Detect and maintain workbook strongly connected components (P3) ← `ceng-m4.1`
- **`ceng-m5.2`** Implement captured iterative scheduling and convergence (P3) ← `ceng-m5.1`, `infra-m5.1`
- **`ceng-m5.3`** Handle iterative arrays dynamic topology and failure publication (P3) ← `ceng-m5.2`, `ceng-m3.4`
- **`ceng-m5.4`** Evaluate one-variable data tables with input overlays (P3) ← `ceng-m4.2`, `cdoc-m5.1`, `infra-m5.1`, `ceng-m3.6`
- **`ceng-m5.5`** Evaluate two-variable data tables and cyclic table semantics (P3) ← `ceng-m5.4`, `ceng-m5.2`
- **`ceng-m5.6`** Register conditional-formatting dependency participants (P3) ← `ceng-m3.1`, `cfml-m5.1`
- **`ceng-m5.7`** Evaluate color scales data bars and icon sets (P3) ← `ceng-m5.6`, `cfunc-m3.10`
- **`ceng-m5.8`** Evaluate validation candidates in staged input contexts (P3) ← `ceng-m3.3`, `cfml-m5.1`, `cfunc-m3.21`
- **`ceng-m5.9`** Calculate cycles crossing linked open workbooks (P3) ← `ceng-m5.2`, `ceng-m3.9`

### `cdoc-m5` — CalcDoc

- **`cdoc-m5.1`** Model and author what-if data-table formulas (P3) ← `cdoc-m3.3`
- **`cdoc-m5.2`** Model conditional-format rules and formula anchors (P3) ← `cdoc-m3.4`
- **`cdoc-m5.3`** Model data-validation rules and input messages (P3) ← `cdoc-m3.4`

### `chost-m5` — CalcHost

- **`chost-m5.1`** Expose iterative settings and what-if data-table commands (P3) ← `chost-m4.8`, `ceng-m5.3`, `ceng-m5.5`
- **`chost-m5.2`** Implement CF rule authoring and projection commands (P3) ← `chost-m4.5`, `ceng-m5.7`
- **`chost-m5.3`** Implement validation authoring prompts and rejected edits (P3) ← `chost-m4.2`, `ceng-m5.8`

## M6: Excel core UI and complete core document authoring

### `infra-m6` — Infrastructure

- **`infra-m6.1`** Prove complete core UI and document authoring (P3) ← `dnacalc-m6.24`, `onecalc-m6.1`, `cdoc-m6.7`, `infra-m3.1`, `infra-m1.2`, `cdoc-m6.1`, `cdoc-m6.2`, `cdoc-m6.3`, `cdoc-m6.4`, `cdoc-m6.5`, `cdoc-m6.6`, `cfml-m6.1`, `cfml-m6.2`, `chost-m6.1`, `chost-m6.2`, `chost-m6.3`, `chost-m6.4`, `dnacalc-m6.1`, `dnacalc-m6.2`, `dnacalc-m6.3`, `dnacalc-m6.4`, `dnacalc-m6.5`, `dnacalc-m6.6`, `dnacalc-m6.7`, `dnacalc-m6.8`, `dnacalc-m6.9`, `dnacalc-m6.10`, `dnacalc-m6.11`, `dnacalc-m6.12`, `dnacalc-m6.13`, `dnacalc-m6.14`, `dnacalc-m6.15`, `dnacalc-m6.16`, `dnacalc-m6.17`, `dnacalc-m6.18`, `dnacalc-m6.19`, `dnacalc-m6.20`, `dnacalc-m6.21`, `dnacalc-m6.22`, `dnacalc-m6.23`

### `cfml-m6` — CalcFml

- **`cfml-m6.1`** Complete number-format sections locales and numeric parsing (P3) ← `cfml-m2.6`, `cfunc-m3.25`
- **`cfml-m6.2`** Complete editor reference modes and diagnostics (P3) ← `cfml-m2.5`, `cfml-m4.2`

### `cdoc-m6` — CalcDoc

- **`cdoc-m6.1`** Author complete cell formats and style tables (P3) ← `cdoc-m3.6`
- **`cdoc-m6.2`** Author row column outline and pane metadata (P3) ← `cdoc-m3.6`
- **`cdoc-m6.3`** Model comments notes and hyperlinks (P3) ← `cdoc-m3.5`
- **`cdoc-m6.4`** Preserve macro projects signatures and package extensions (P3) ← `cdoc-m3.4`
- **`cdoc-m6.5`** Write calcPr calcChain and trustworthy formula caches (P3) ← `cdoc-m3.3`, `ceng-m4.3`
- **`cdoc-m6.6`** Complete source-preserving package authoring and Save As (P3) ← `cdoc-m4.3`
- **`cdoc-m6.7`** Enforce transactional save output and lazy source access (P3) ← `cdoc-m6.6`

### `chost-m6` — CalcHost

- **`chost-m6.1`** Implement panes dimensions grouping and sheet-view commands (P3) ← `chost-m4.5`, `cdoc-m6.2`
- **`chost-m6.2`** Implement comments notes hyperlinks and review commands (P3) ← `chost-m4.3`, `cdoc-m6.3`
- **`chost-m6.3`** Implement formula audit trace and watch commands (P3) ← `chost-m4.12`, `ceng-m4.5`
- **`chost-m6.4`** Implement document backstage and recovery commands (P3) ← `chost-m4.9`, `cdoc-m6.7`

### `onecalc-m6` — DNA OneCalc

- **`onecalc-m6.1`** Complete OneCalc function exploration and fixture persistence (P3) ← `onecalc-m2.1`, `cfunc-m3.25`, `cfml-m6.2`, `cfunc-m4.1`, `cfml-m4.3`

### `dnacalc-m6` — DNA Calc

- **`dnacalc-m6.1`** Build ribbon backstage quick access and key tips (P3) ← `dnacalc-m2.5`, `infra-m3.1`, `chost-m6.4`
- **`dnacalc-m6.2`** Implement range multi-area and axis selection (P3) ← `dnacalc-m2.5`
- **`dnacalc-m6.3`** Implement edit enter point and IME modes (P3) ← `dnacalc-m6.2`, `cfml-m6.2`
- **`dnacalc-m6.4`** Implement fill handle and AutoFill series (P3) ← `dnacalc-m6.2`, `chost-m4.2`
- **`dnacalc-m6.5`** Implement drag move copy and insert gestures (P3) ← `dnacalc-m6.2`, `chost-m4.2`
- **`dnacalc-m6.6`** Implement clipboard and Paste Special UI (P3) ← `dnacalc-m6.3`, `chost-m4.2`
- **`dnacalc-m6.7`** Implement freeze split and axis layout controls (P3) ← `dnacalc-m6.2`, `chost-m6.1`
- **`dnacalc-m6.8`** Implement Format Cells Number and Alignment (P3) ← `dnacalc-m6.3`, `chost-m4.5`, `cfml-m6.1`
- **`dnacalc-m6.9`** Implement Format Cells Font Border Fill and Protection (P3) ← `dnacalc-m6.8`, `chost-m4.7`, `cdoc-m6.1`
- **`dnacalc-m6.10`** Implement cell styles themes and format painter (P3) ← `dnacalc-m6.9`
- **`dnacalc-m6.11`** Implement Name Manager and name-box navigation (P3) ← `dnacalc-m6.3`, `chost-m4.3`, `cfml-m3.1`
- **`dnacalc-m6.12`** Implement conditional-formatting UI and rule manager (P3) ← `dnacalc-m6.9`, `chost-m5.2`
- **`dnacalc-m6.13`** Implement data-validation dialog dropdowns and prompts (P3) ← `dnacalc-m6.3`, `chost-m5.3`
- **`dnacalc-m6.14`** Implement Find Replace Go To and Go To Special (P3) ← `dnacalc-m6.3`, `chost-m4.6`
- **`dnacalc-m6.15`** Implement sort AutoFilter and advanced-filter UI (P3) ← `dnacalc-m6.2`, `chost-m4.4`
- **`dnacalc-m6.16`** Implement table creation and contextual table tools (P3) ← `dnacalc-m6.15`, `chost-m4.3`
- **`dnacalc-m6.17`** Implement insert delete cells rows columns and sheets UI (P3) ← `dnacalc-m6.2`, `chost-m4.3`
- **`dnacalc-m6.18`** Implement calculation options and what-if table dialogs (P3) ← `dnacalc-m6.3`, `chost-m5.1`, `chost-m4.8`
- **`dnacalc-m6.19`** Implement Evaluate Formula trace watch and error tools (P3) ← `dnacalc-m6.3`, `chost-m6.3`
- **`dnacalc-m6.20`** Implement comments notes hyperlinks and protection UI (P3) ← `dnacalc-m6.3`, `chost-m6.2`, `chost-m4.7`
- **`dnacalc-m6.21`** Implement status aggregates zoom sheet tabs and window navigation (P3) ← `dnacalc-m6.2`, `chost-m6.1`, `chost-m6.3`
- **`dnacalc-m6.22`** Complete keyboard routing accessibility and command parity (P3) ← `dnacalc-m6.1`, `dnacalc-m6.21`, `chost-m4.13`
- **`dnacalc-m6.23`** Complete non-Excel skin rules tables and protected editing (P0) ← `dnacalc-m2.6`, `chost-m5.2`, `chost-m5.3`, `chost-m4.7`, `ceng-m3.4`, `ceng-m3.2`, `cfml-m3.4`, `chost-m4.2`, `dnacalc-m3.1`
- **`dnacalc-m6.24`** Test original visual assets browser limits and input interoperability (P3) ← `dnacalc-m6.22`, `dnacalc-m6.23`

## M7: Conformance expansion, numerical campaigns and measured workloads

### `infra-m7` — Infrastructure

- **`infra-m7.1`** Define licensed corpus manifests and stratified coverage (P3) ← `infra-m0.6`, `infra-m1.1`
- **`infra-m7.2`** Generate and capture cross-feature workbook families (P3) ← `infra-m7.1`, `infra-m1.2`, `infra-m5.3`
- **`infra-m7.3`** Acquire public corpus candidates with provenance review (P3) ← `infra-m7.1`
- **`infra-m7.4`** Unify live and cached verdicts with raw numeric observations (P3) ← `infra-m7.2`, `infra-m1.1`
- **`infra-m7.5`** Port one black-box smart-fuzzer campaign runner (P3) ← `infra-m7.4`, `infra-m0.5`, `cfunc-m3.25`
- **`infra-m7.6`** Measure native target variation and software-Exact feasibility (P3) ← `cfunc-m7.1`, `cfunc-m3.24`, `infra-m7.4`
- **`infra-m7.7`** Verify portable Fast determinism and numeric mode defaults (P3) ← `cfunc-m7.2`, `chost-m4.8`, `onecalc-m6.1`
- **`infra-m7.8`** Record named product-path benchmarks without thresholds (P3) ← `infra-m6.1`, `infra-m7.7`, `ceng-m7.1`
- **`infra-m7.9`** Fuzz operation sequences package parsing and cancellation (P3) ← `infra-m6.1`, `ceng-m7.1`
- **`infra-m7.10`** Publish conformance and campaign evidence through one report (P3) ← `infra-m7.4`, `infra-m7.5`, `infra-m7.7`, `infra-m7.8`, `infra-m7.9`, `infra-m7.3`, `infra-m7.6`

### `cfunc-m7` — CalcFunc

- **`cfunc-m7.1`** Measure a portable extended-precision arithmetic candidate (P3) ← `cfunc-m3.22`, `infra-m0.5`
- **`cfunc-m7.2`** Declare measured Fast-mode tolerance contracts (P3) ← `cfunc-m3.25`, `infra-m7.4`

### `ceng-m7` — CalcEngine

- **`ceng-m7.1`** Retain the reference strategy and differential-twin harness (P3) ← `ceng-m5.9`, `ceng-m4.5`

## M8: Release integration and first-version scope accounting

### `infra-m8` — Infrastructure

- **`infra-m8.1`** Verify core completion and mandatory phase-2 attachments (P4) ← `infra-m7.10`, `dnacalc-m8.1`, `onecalc-m8.1`, `chost-m4.13`, `cfunc-m4.1`, `cfml-m4.3`, `ceng-m4.6`, `infra-m0.1`, `infra-m0.2`, `infra-m0.3`, `cfunc-m0.1`, `cdoc-m0.1`, `infra-m1.3`, `cfml-m2.1`, `cfml-m2.2`, `ceng-m2.1`, `onecalc-m2.1`, `infra-m2.1`, `infra-m3.2`, `infra-m3.3`, `ceng-m3.8`, `chost-m3.2`, `infra-m4.1`, `chost-m4.10`, `chost-m4.11`, `chost-m4.12`, `infra-m5.3`, `infra-m6.1`
- **`infra-m8.2`** Enforce first-version release scope including separable areas (P4) ← `infra-m8.1`, `infra-m3.20`, `infra-m3.21`, `infra-m3.22`, `infra-m3.23`, `infra-m3.24`

### `chost-m8` — CalcHost

- **`chost-m8.1`** Complete application lifecycle and cross-skin recovery (P4) ← `chost-m6.4`, `chost-m4.13`, `dnacalc-m6.23`

### `onecalc-m8` — DNA OneCalc

- **`onecalc-m8.1`** Verify the shipped OneCalc package stays engine-free (P4) ← `onecalc-m6.1`, `infra-m7.7`

### `dnacalc-m8` — DNA Calc

- **`dnacalc-m8.1`** Integrate all skins and commands in the shipped packages (P4) ← `dnacalc-m6.24`, `chost-m8.1`, `infra-m7.10`

