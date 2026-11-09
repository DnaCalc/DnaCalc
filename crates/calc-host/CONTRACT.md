# calc-host contract (draft)

Read at origin/master `cb0b76a5` of DnaCalc. Short paths (`recalc.rs:70`) are under `crates/calc-host/src/`;
`tests/...` is under `crates/calc-host/`. RC = `docs/architecture/RECALC_CONTRACT.md`; probes =
`docs/architecture/EXCEL_CALC_PROBES.md`, evidence in `crates/calc-engine/tests/fixtures/excel-calc-probes/`.

## 1. Purpose and position

- The one owner of an open workbook: the authored revision, the engine state at that revision, the package
  calc-doc loaded, and every change to them (`lib.rs:1-11`, `state.rs:94-98`). It turns commands into one authored
  batch, decides when and over what scope the engine calculates, projects cells, and coordinates save.
- Depends on `calc-value`, `calc-func`, `calc-doc`, `calc-doc-model`, `calc-engine`, `calc-fml`; dev-only `calc-vba`,
  `calc-verdict` (`crates/calc-host/Cargo.toml`). Declared `portability = "portable"`.
- Above it (Cargo.toml files): `calc-host-vba`, `dna-calc-worker`, `dna-calc-desktop`, `xtask-harness`; `dna-calc`
  only as a dev-dependency (`apps/dna-calc/Cargo.toml:104-106`). `tools/xtask/architecture.json` (closure rules):
  `dna-onecalc`, `dna-formula-ui`, `dna-onecalc-desktop`, `dna-calc` (the worker composes workbooks),
  `dna-calc-model`, `dna-calc-protocol`, `calc-addin-host`, `calc-platform-desktop` must not reach calc-host.

## 2. Public API by concept

- **Workbook.** `HostState` (`state.rs:100`) from `HostState::open(source, &OpenPolicy, HostOptions)` (275):
  `calc_doc::open`, `load`, `EngineState::with_file_caches`; nothing is calculated. `HostOptions` (17).
- **Session.** `HostSession` (`session.rs:355`; `new` 436, `open` 453) is what applications drive.
  `dispatch(SkinIntent)` / `dispatch_from(CommandIngress::{UserInterface, Automation}, ..)` (850, 856) return
  `IntentReceipt { seq, parent, error, committed, recalc, revision, deltas }` (247). Undo, redo: `history.rs:1-5`.
- **Edits.** `SkinIntent::{EnterCell, EnterSelection, Edit, EditAxis, Copy{cut}, Undo, Redo, Command}`
  (`session.rs:133-195`); `WorkbookCommand` (`actions.rs:58`). All end in `dispatch(&mut HostState, HostCommand
  { base, edits })` (`command.rs:266`; `CellEdit` 18) -> `CommandReceipt` (142: revision, changed cells, input
  writes, pending anchors) or `CommandError::{NoEdits, StaleBase, Document, LayoutSave, Engine, FormulaBudget}`.
- **Formula entry and form.** `interpret_entry_in` (`entry.rs:96`) reads typed text as Excel does or returns
  `EntryRefusal`. `HostSession::authored` (`session.rs:1108`): a `FormulaText` with an explicit `FormulaForm`
  keeps it (Formula passes `Legacy`); else an array-capable formula is a one-cell dynamic array (Formula2; 1221-1248).

- **Calculation.** `SkinIntent::{Recalculate, Calculate{scope, selection}, CalculateRange, Rebuild, SetCalcMode}`
  (`session.rs:159-176`). Headless: `calculate`, `calculate_full`, `step(host, WorkBudget, observer)` ->
  `HostProgress::{Cancelled, Current, Manual, Calculating, Waiting, Published(ValueDelta), Failed}`
  (`recalc.rs:276,286,70,40`). `AdmissionReceipt`, `CompletionReceipt` arrive as `CalcEvent`s (`session.rs:478`).
- **Normalization to an engine request** (`recalc.rs:146-187`, `session.rs:947-983`); a scoped request or Rebuild
  is refused while a calculation is active (`session.rs:953,1016`):

| Command | Engine request | Code |
|---|---|---|
| Edit, Automatic | `begin_recalc` (dirty, volatile); running work superseded | `session.rs:1366`, `recalc.rs:115` |
| Edit, Manual | none: `HostProgress::Manual`, results project stale | `recalc.rs:154` |
| Edit, AutomaticExceptTables | as Automatic, `set_tables_deferred(true)` | `recalc.rs:151` |
| `Recalculate` (F9) | `begin_recalc` in any mode, even with nothing pending | `recalc.rs:276,148` |
| `Calculate{scope, selection}` | `begin_recalc_scoped_with_poststate`, selection kept | `session.rs:1010` |
| `CalculateRange` | `Range` + `ForceAll`; Manual: `StillRequired`, else `Satisfied` | `recalc.rs:165-172` |
| `calculate_full` | all pending; `Workbook` + `ForceAll`; plans and graph kept | `recalc.rs:286,95` |
| `Rebuild` | `rebuild_bindings`, then `Workbook` + `ForceAll` | `session.rs:952` |

- **Structural edits, names.** `CellEdit::Axis` -> `calc_doc::prepare_structural_edit` with
  `calc_engine::ReferenceRewriter` (`command.rs:587`); `CellEdit::Sheet` -> `prepare_sheet_edit`, one per command
  (556-583); `shift_cell_edits` (`shift.rs:8`); cut is `Copy{cut}` with `move_coordinates` (`session.rs:934`).
  A commit applies the same `StructuralEdit` to the stored-reference tracker (`command.rs:656-665`) and to an
  active scoped calculation's scope (339-362). Names: `NameEdit` (`names.rs:30,68`) -> `CellEdit::SetName`.
- **Callable tables.** `set_user_functions(calc_fml::CallableTable)` (`state.rs:239`; session 391) replaces the
  table and calls `set_callable_bindings`; work bound to the old table is superseded with `CallableReload`
  (`recalc.rs:84-133`). Also `set_rtd_connector`, `update_rtd`, `complete_async` (`recalc.rs:383,424,317`). The
  host loads no add-in: XLL and COM hosts sit under `calc-addin-host`; the application registers their callables
  and returns async and RTD values here (`XLL_COM_HOSTING.md:583-586,614`). VBA: `crates/calc-host-vba/src/lib.rs`.
- **Observation and events.** `project` -> `ProjectedValue::{Empty, Literal, FileCached, Calculated, NotModeled,
  Stale}` (`project.rs:204,84`). `subscribe(Interest, DeltaSink)` -> `HostDelta` (`session.rs:823,114`).
  `WorkbookEventHandler` (`events.rs:31`). `CommandJournal` (`envelope.rs:40`): at-most-once admission by id.
- **Save and load.** `prepare_save` / `save` (`save.rs:199,407`) return a `PreparedSave` owning the plan and its
  `SavePin` (134, 145); `SaveCachePolicy::{CurrentCalculatedOnly, PreservePermittedCaches}` (84). Session saves
  take `ExcelSaveContext` (`save.rs:67`; `session.rs:681`). `begin_calculated_save` calculates one isolated pin
  (`save_calculation.rs:89-111`). calc-doc plans and writes (`save.rs:398,158`).
- **Detached and multi-workbook.** `evaluate_formula_text` (`names.rs:271`) and `begin_name_calculation`
  (`name_calculation.rs:108`) install no cell result. `CalculationDomain` (`domain.rs:65`): `connect`,
  `request_calculation`, `request_rebuild`, `advance`, `begin_save` (130, 175, 227, 237, 378); sources go first.

## 3. Guarantees and invariants

- **A refused command changes nothing.** Model and engine are always at one revision (`state.rs:98`). `dispatch`
  prepares the document share, then the engine share, checks the formula budget and reserves the receipt before
  committing; a refusal drops both (`command.rs:246-265,589-620`). After `commit_edit` nothing errors (653).
- **A receipt describes the operation, not freshness.** `CommandReceipt` names the revision made and what is
  pending (`command.rs:180-187`); `SavePin` claims no freshness (`save.rs:131-132`); see `project.rs:189-203`.
- **Recalculation is outside the edit's transaction.** A failed one keeps the commit (`recalc.rs:11-16,293-297`).
- **Order for an edit.** Commit; `Committed` deltas (`session.rs:1357`); history step; UI ingress: Change event,
  then one budgeted step unless Manual; Automation ingress: calculation runs to its end, then Change, or a
  recorded failure if it did not finish (`session.rs:1361-1392`). `WorksheetCalculate`: `events.rs:182-218`.
- **Save.** BeforeSave edits commit before the pin; SaveCopyAs skips the handler; cancel keeps the edit and
  writes no image (`session.rs:736-753`). Writing consumes only the pinned plan (`save.rs:143-144`).
- **One owner thread** (inferred): `HostState` and `HostSession` hold `Rc` and `Cell` (`state.rs:117,125-135`).
- **Trusted to the engine:** order, evaluation, publication, `stale()` as "no longer stands", reuse of validated
  results on supersession, boundary caches. Trusted to calc-doc: reference rewriting and what a save can move.

## 4. What it must never do

- Touch `std::fs`, `std::process`, `std::os`, `std::net` (portable, wasm32). Files, DLLs, COM: the caller's.
- Be reached by OneCalc, the formula UI, the page or the add-in set. Keep a second graph, evaluator or store.
- Show or count a file cache, stale value or refusal as calculated; save one as current outside the measured
  `PreservePermittedCaches` profile; refuse without naming the owning bead; widen a refused scoped calculation.

## 5. Excel behaviours owned, with the measurement

| Behaviour | Measured by | Code; test |
|---|---|---|
| Manual range: forced, required at next F9, boundary caches | P02 | `recalc.rs:165-172`; `tests/round_trip.rs:4724` |
| Save truth table (below) | P06 | `session.rs:721-820`; `tests/round_trip.rs:5276-5953` |
| Formula vs Formula2 form kept at every ingress and reload | P09 | `session.rs:1221`; `tests/round_trip.rs:4656` |
| Stored Range follows edits, never revives; reload re-evaluates | P10 | `command.rs:656`; `tests/round_trip.rs:6414` |
| Row and column insert, delete: rewriting, reopen | `tests/fixtures/d2` | `command.rs:587`; `tests/round_trip.rs:3439` |

| Cut: references follow moved cells; local names | `tests/fixtures/dc_gf0` | `copy.rs:22`; `tests/round_trip.rs:3742` |
| External links stored by file name; xlsx and xlsb agree | `tests/fixtures/dc_ylr` | `tests/external_links.rs:204,257` |
| COM write: calculation precedes Change | P22 | `session.rs:855`; `tests/events.rs:365` |

Save as coded (`session.rs:736-814`): a disabled active sheet suppresses calculation; `calculate_before_save`
calculates dirty and volatile work over the workbook or the enabled sheets; false in AutomaticExceptTables still
calculates data tables. Manual or a disabled sheet -> `PreservePermittedCaches` (`save.rs:188-190`, comment only).

## 6. How it is tested

- `tests/acceptance.rs` with `acceptance.json` (chost-m1.1, dc-hqcv, dc-besk, dc-xrxb; windows, linux, wasm).
- `tests/round_trip.rs` (334 browser-attributed cases); `circular.rs`, `events.rs`, `threads.rs` (native-only).
- AGENTS.md rule: `#[cfg_attr(not(target_arch = "wasm32"), test)]` plus the `wasm_bindgen_test` attribute, data by
  `include_bytes!`; else `#[test]` with `/// Native-only: <reason>`. Browsers run in `cargo xtask sweep` only.
- Corpus goes through the host: `HostState::open`, `step`, `project` (`tools/xtask-harness/src/compare.rs:9-18,724`).

## 7. Open items

- dc-hsa5.4: save continuations (`save.rs:75-80`). dc-hsa5.12: Automation Change (`events.rs:178`). dc-hsa5.6: names.
- dc-ylr: cross-workbook cycles (`domain.rs:197`). dc-b1l: number-shaped entries (`entry.rs:25-29`). dc-8wp: pivot
  output and source sheets (`command.rs:286-310`). dc-971p.16, .37, .38: views, filters, notes (`command.rs:379-449`).
- No bead in code: precision as displayed (`state.rs:47-49`); `FileCacheUnverified` reach (`project.rs:43-44`).
- P11, P13: no probe folder. Entry in Manual evaluates nothing unless the caller sends `Calculate` with
  `FormulaEntry` (`session.rs:1366,1022`); probes section 1 infers that Excel evaluates at entry.

## 8. Findings for the tracker

1. `command.rs:259-260` and RC:15-16 say a commit cancels the running recalculation; the code keeps it
   (`command.rs:673`) and `step` supersedes it (`recalc.rs:84-121`). RC's line references are stale.
2. `recalc.rs:284-285` and RC:153 say `calculate_full` cancels running work; `recalc.rs:286-291` cancels nothing,
   95-104 supersedes. RC:152 says `HostProgress` has no waiting state; `recalc.rs:54` has one.
3. `session.rs:166-167`: caches outside a `CalculateRange` are refused; `tests/round_trip.rs:4756` shows them read.
4. `lib.rs:37-39` says save leaves every non-standing formula to Excel; `save.rs:199-206` keeps permitted caches.
5. RC:674 names three `CachePolicy` variants; the code has two, differently named (`save.rs:84-87`).
6. RC:684: `CalculateBeforeSave` "absent today", a domain setting; it is a per-save field (`save.rs:69`).
7. Two rules pick the cache policy: `save.rs:200-206` (Manual and an XML or spreadsheet format) and
   `session.rs:809-814` (Manual or a disabled sheet, no format test; a later format test was not looked for).
8. Refusals name beads `.beads/issues.jsonl` lists as closed: dc-971p.40 (`command.rs:428,477`), dc-971p.8
   (`command.rs:523,528,550`), dc-971p.7 (`shift.rs:20`).

Verified: each `path:line` above was read or grepped at origin/master `cb0b76a5`; bead status from the tracker file.
Inferred: one owner thread; the saved flag values; that add-in hosts reach the host only through the application.
Not read: `copy.rs`, `paste.rs`, `tables.rs`, `pivot*.rs`, `chart.rs`, `facts.rs`, most of `round_trip.rs`, RC 3.2-5.
