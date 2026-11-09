# calc-doc-model contract (draft)

Read at origin/master `cb0b76a5` of DnaCalc. Short paths (`root.rs:685`) are under `crates/calc-doc-model/src/`.
Plan = `docs/plan/DNA_CALC_REIMPLEMENTATION_PLAN.md`; layering = `tools/xtask/architecture.json`.

## 1. Purpose and position

- The engine-neutral authored workbook model: sheets, authored cell inputs, formulas as entered, array formulas,
  names, tables, document metadata, the operation vocabulary and its wire codec (`lib.rs:1-53`). It is "the only
  part of CalcDoc that calc-engine may depend on" (`lib.rs:3`). It holds no calculated value and parses no formula.
- Depends on `calc-syntax`, `calc-value`, `serde`, `serde_json`; wasm-only dev dependencies for browser tests
  (`crates/calc-doc-model/Cargo.toml`). Declared `portability = "portable"`.
- Layering (line 81): its closure must not contain `calc-engine`, `calc-fml`, `calc-host`, `calc-doc`. Must not
  reach it: `calc-syntax`, `calc-verdict`, `xtask`, `calc-platform-desktop` (lines 9, 99, 115, 185).
- Direct dependents (Cargo.toml grep): `calc-doc` (re-export `calc_doc::model`, `crates/calc-doc/src/lib.rs:95`),
  `calc-engine`, `calc-host` (holds the current root, `crates/calc-host/src/state.rs:108`). No application names
  it in a Cargo.toml; apps reach it through calc-doc or calc-host (inferred).

## 2. Public API by concept

- **Root and revisions.** `ModelRoot` (`root.rs:246`): revision, sheets in order, per-sheet cell pages,
  `Arc<SkinLayout>`, `Arc<Definitions>`, lineage; always handed out as `Arc<ModelRoot>`. Builders `new` (414),
  `with_cells` (438), `with_layout` (449), `with_definitions` (499); refusals `ModelRootError` (64).
  `AuthoredRevision(u64)` with `INITIAL` and `checked_next` (`revision.rs:7-22`). `same_lineage` (537): a process-wide
  counter gives each built root a lineage that `apply` continues (`root.rs:18,240-244`). `pin` (271) re-wraps a
  borrowed root; `restored(earlier)` (546) brings an earlier root of the same lineage back as the next revision.
  `with_file_identity` (284), `with_formula_storage` (324), `with_initial_data_model` (525) and
  `with_imported_worksheets` (572) return a different root at the *same* revision and lineage.
- **Sheets.** `SheetEntry` (`root.rs:27`): session `SheetId` (calc-value), document-stable `SheetKey(u32)`
  (`sheet.rs:16`, the xlsx `sheetId` role), `SheetName` (79), `SheetVisibility::{Visible, Hidden, VeryHidden}` (110).
  Lookups `sheets`, `sheet_by_key`, `sheet_by_id` (`root.rs:587,610,614`), linear scans.
- **Cell storage.** `CellPage` (`cells.rs:82`): occupied cells of one block of `PAGE_ROWS` 256 by `PAGE_COLUMNS` 16
  (`cells.rs:18,21`), immutable, never empty, behind `Arc`, found through a four-level index (`cells.rs:5-10`). Reads
  borrow: `cell` (625), `cells(area)` and `sheet_cells` in row-major order (631, 644; `OccupiedCells`,
  `cells.rs:555`), `pages`, `page_of` (650, 656).
- **Cell content.** `CellInput::{Number(FiniteNumber), Text(CellText), Logical, Error, Formula(FormulaText)}`
  (`input.rs:233`). `FiniteNumber` refuses NaN and infinities and compares by bits (`input.rs:12-19,41`); `CellText`
  is at most 32,767 UTF-16 code units, kept exactly (`input.rs:55-63`). `FormulaText` (96): text after `=`,
  non-empty, no syntax or length check (87-94); stored inline or as a member of a shared `FormulaBlock`
  (`formula_block.rs:39`, rendered on demand through `FormulaTemplateSource`, 31); carries a `FormulaForm::{Legacy,
  Dynamic}` (`crates/calc-syntax/src/lib.rs:8`) and whether an ingress chose it (`is_form_explicit`, 143).
- **Arrays and spill metadata.** `ArrayFormula {area, kind}`, `ArrayKind::{Legacy, Dynamic {collapsed}}`
  (`definitions.rs:16-33`); the anchor holds the formula, the other cells of the area hold no input.
  `Definitions::not_available` (935), `array_containing` (874), `arrays_meeting` (884). What-if `DataTable` (300).
- **Names.** `DefinedName` (`definitions.rs:61`), `NameScope::{Workbook, Sheet}` (re-export of calc-value, 55).
  `NameTable` (`name_table.rs:74`): `NameNode` with `NameId`, `NameOrigin`, `binding_version` (45-67); `lookup`,
  `lookup_key`, `matching`, `set_formula`, `snapshot_keys` (135-319).
- **Tables and other definitions.** `Table` (`definitions.rs:163`): area, header and totals row counts, columns,
  display name, calculated-column sources, `opaque` flags. `Definitions` (375) also holds unloaded cells, hidden
  rows, iteration settings, external books, per-cell format overrides, data model, pivots (`pivot.rs`), filters,
  validations, conditional rules, review notes, charts, page setup, panes, axis layouts, the workbook name, the
  macro-enabled flag (376-404) and the styles: `StyleTable` (`style.rs`; `definitions.rs:376,430`), replaced by
  `WorkbookOp::SetStyles` (`op.rs:35`); no calculation reads it (`style.rs:3-4`).
- **Identifiers and grid.** `CellKey`, `GridArea`, `SheetId`, `MAX_ROWS` 1,048,576, `MAX_COLUMNS` 16,384 come from
  calc-value (`crates/calc-value/src/grid.rs:10,13,53,90`), re-exported as `value` (`lib.rs:90`). `Axis::size`
  (`op.rs:222`). Sheet name limit `SHEET_NAME_MAX_UTF16_CODE_UNITS` 31 (`sheet.rs:29`).
- **Operations.** `OpBatch {base, operations}` (`op.rs:374`); `WorkbookOp` (33-106), 24 variants: `SetCell`,
  `Clear`, `SetArray`, `SetFormat`, `SetNameFormula`, `RestoreNames`, `SetIteration`, `EditAxis(AxisEdit)`,
  `EditSheet(SheetEdit)`, `EditSkinLayout`, and whole-state setters (`SetStyles`, `SetTables`, `SetPivots`, ...).
- **Applying.** `ModelRoot::apply(&OpBatch)` -> `Result<Arc<ModelRoot>, ApplyError>` (`root.rs:685`): next revision,
  or a refusal with nothing made (131-168). Row, column and sheet edits are refused there (915-916) and applied by
  `apply_structural(batch, &dyn FormulaRewriter, issue_sheet)` (`structural.rs:201`); the rewriter trait
  (`structural.rs:32`) is implemented by calc-engine (`crates/calc-engine/src/structural.rs:68`). Position helpers
  `moved_position`, `source_position`, `moved_run`, `moved_cell`, `moved_area` (`structural.rs:59-157`).
- **File caches.** `FileCached {CachedValue, Arc<SourcePart>}` (`cache.rs:16-87`): the type only; the codec stores
  them beside the model (`crates/calc-doc/src/load/mod.rs:118`).
- **Wire codec.** `encode_batch`, `decode_batch` (`wire/mod.rs:328,410`), `BATCH_VERSION` 1 (53). Four capabilities:
  `cells`, `axes`, `sheets`, `skin_layout` (`wire/capability.rs:204-242`); every other operation is refused by
  `EncodeError` (`wire/mod.rs:221-268,332-345`).

## 3. Guarantees and invariants

- **Immutable, structurally shared.** No method takes `&mut ModelRoot`; an edit makes a new `Arc`. `apply` builds a
  new page only for a block the batch changed, once per batch; every other page, a page set to what it already held
  included, is the old allocation (`root.rs:675-679`; checked by `pages_match_an_ordered_map_under_random_batches`,
  `root.rs:1219-1225`). Layout and definitions are shared by pointer unless changed (`root.rs:930-952`).
- **Refusal changes nothing.** A stale base, exhausted revision, unknown sheet, array overlap or unknown name
  returns an error and no root (`root.rs:128-129,686-695`). Operations of a batch apply in order (668-669).
- **Revision and lineage.** `apply`, `apply_structural` and `restored` advance the revision by one and keep the
  lineage. (lineage, revision) is not a content identity: the same-revision builders of section 2 exist. Equality
  ignores lineage and compares definitions (`root.rs:256-263`); `NameTable` equality compares definitions only, not
  ids or versions (`name_table.rs:93-101`).
- **Identity across edits.** `SheetId` and `SheetKey` survive every edit except removal; an inserted sheet's id is
  issued by the caller's `issue_sheet` (`structural.rs:195-196`). `NameId`s survive formula edits and bulk
  rewrites, and a removed id is never issued again in one table lineage (`name_table.rs:1-2`). Key reuse after a
  sheet is removed is not checked here; the applier checks it (`root.rs:236-238`, `sheet.rs:11-14`).
- **Authored versus calculated.** No `CellInput` variant holds a calculated or cached value (`input.rs:1-3`); a
  filled array cell and an unloaded cell read as not available, never blank (`definitions.rs:203-204,932-943`).
  `apply` drops an array whose anchor a batch sets or clears, and a touched unloaded cell (`definitions.rs:970-992`).
- **Formula storage.** A shared block's membership is its actual cells, never the bounding rectangle
  (`formula_block.rs:50`). `FormulaText` equality is form plus rendered text (`input.rs:218-221`).
- **Threads.** No `unsafe`, `Rc`, `RefCell` or `Mutex` (grep); shared state is `Arc`, `OnceLock` and two `AtomicU64`
  lineage counters (`root.rs:18`, `name_table.rs:31`). No code asserts `ModelRoot: Send + Sync` (inferred to hold).

## 4. What it must never do

- Depend on `calc-engine`, `calc-fml`, `calc-host` or `calc-doc` (layering); parse, bind or evaluate a formula
  (`structural.rs:5-8`, `input.rs:91-94`); read or write a package (the ZIP and XML codec is calc-doc).
- Touch `std::fs`, `std::process`, `std::os`, `std::net` (AGENTS.md, Portability); it builds for wasm32.
- Put a calculated or cached value into a `CellInput`, an operation or the wire (`op.rs:1-3`); present a
  `FileCached` as calculated (honesty rule 1; `cache.rs:1-5,65-67`).
- Default or skip on decode: an unknown capability, field or version is a typed `DecodeError`; numbers travel as
  binary64 hex bits, never through decimal (`wire/mod.rs:20-33`).

## 5. Excel behaviours owned, with the measurement

| Behaviour | Measured by or cited | In code |
|---|---|---|
| Grid 1,048,576 by 16,384; pages divide it exactly | constants of calc-value | `cells.rs:24-35`, `op.rs:222` |
| Sheet name: 1-31 units, none of `: \ / ? * [ ]`, no edge apostrophe | none cited | `sheet.rs:29-99` |
| Formula form Legacy or Dynamic; `new` gives Legacy, not explicit | none cited | `input.rs:113-145` |
| Cached value provenance: part name, CRC-32, size | honesty rule 1 | `cache.rs:11-20` |
| Iteration defaults: off, 100, 0.001 (`calcPr`) | none cited; dc-m3u, dc-y2o.2 | `definitions.rs:242-248` |
| What-if data table inputs and results | Excel 16.0 COM, 2026-10-23 (comment) | `definitions.rs:294-298` |
| Rejected formula: file opens only by repair | Excel 16.0.20527, 2026-11-07; dc-mx3x | `definitions.rs:220-227` |
| External sheet cache: an uncached cell is blank | Excel 16.0.20527, 2026-10-30; dc-ylr | `definitions.rs:99-102` |
| Inserted row or column takes the format before it | `d2/structural-excel.json` | `structural.rs:83-88` |
| Pivot output under axis edits; Excel's refusal text | `pivot/m5-structure`; dc-8wp.16 | `structural.rs:821-845` |
| Names Excel gives table columns | `pivot/m7-names`; dc-8wp.16 | `table_names.rs:1-14` |
| Name matching under full Unicode case fold | dc-cpr; `workbook_names_equal` | `op.rs:73-74` |

Fixture paths are under `crates/calc-host/tests/fixtures/`. Not owned here: the 1904 date flag (no `1904` in this
crate; calc-doc reads it: `load/mod.rs:1187` and `session.rs:120` under `crates/calc-doc/src/`). "Formula" and
"Formula2" are not names in this crate; the terms are `FormulaForm::Legacy` and `Dynamic`.

## 6. How it is tested

- 18 unit tests in `src/`, each with the AGENTS.md pair `#[cfg_attr(not(target_arch = "wasm32"), test)]` plus the
  `wasm_bindgen_test` attribute, so each also runs in the browsers; no `Native-only:` test. By file: `name_table.rs`
  5, `style.rs` 3, `table_names.rs` 3, `root.rs` 2, `axis_layout.rs` 2, `layout.rs` 2, `structural.rs` 1; none in
  the other files, `wire/` included. `tests/` holds only `.gitkeep`; `acceptance.json` registers no bead.
- From other crates: `crates/calc-doc/tests/acceptance.rs` (the wire codec: 34 lines name `encode_batch` or
  `decode_batch`), `crates/calc-engine/tests/acceptance.rs` (`apply_structural` with `ReferenceRewriter`, line 4635),
  `crates/calc-host/tests/{acceptance,round_trip,names,charts,cube,pivot_ui28,pivot_ui28_refresh}.rs`.
- Per commit: `cargo xtask check`. Browser runs (Chrome, Firefox) are in `cargo xtask sweep`, never per commit.

## 7. Open items

- dc-m3u (L3): structural edits, data tables, iteration settings (`structural.rs:1`, `definitions.rs:231,295`);
  `UnloadedCell::DataTable` names owner "L3" without a bead (214); rich values have no bead (209, 212).
- Wire version 1 has no form for page setup (dc-971p.56), pivots (dc-8wp), charts (dc-e5z5), arrays, cell formats,
  names, iteration, panes, tables, filters, conditional rules, review notes (`wire/mod.rs:221-285`).
- dc-8wp.16: pivots a structural edit cannot place keep the refusal (`structural.rs:829,844`).
- dc-ylr (external books), dc-xuw (macro sheets), dc-09v (format overrides), dc-gf0 (names moved by a cut),
  dc-0f1k.11.3 (`style.rs:258`): named in comments; their tracker state was not checked.

Verified: each `path:line` above was read or grepped at origin/master `cb0b76a5`; test counts are grep counts.
Inferred: how apps reach the crate; `Send + Sync` of `ModelRoot`; that the wire codec has no caller outside tests.
Not read: bodies of the metadata modules (`layout`, `style`, `pivot`, `chart`, ...), `wire/json.rs`, `wire/value.rs`.

## Findings for the tracker (as reported by the drafting agent at cb0b76a5; verify before filing)

- `root.rs:243-244` says equality compares "the revision, the sheets, the cells and the layout"; the code also
  compares `definitions` (`root.rs:262`).
- `root.rs:784`: an invalid `SetPageSetup` returns `ApplyError::AxisEdit`, the variant documented as "inserts or
  deletes rows or columns" (`root.rs:141-144`).
- `definitions.rs:768-769` says structural edits do not move hidden rows yet (dc-m3u); `structural.rs:379-407`
  moves them and `definitions_move_with_the_rows` (`structural.rs:675,703`) tests it.
- `lib.rs:39-40` says the `StyleTable` is metadata "the codec keeps beside the model"; it is a field of
  `Definitions` (`definitions.rs:376`) and is replaced through `apply` by `WorkbookOp::SetStyles` (`root.rs:788`).
- `lib.rs:23-25` and `root.rs:668-684` describe `WorkbookOp` and `apply` as cell, clear, structural and layout
  edits; the enum has 24 variants (`op.rs:33-106`) and `apply` has refusals the doc omits (`ArrayInput`,
  `ArrayOverlap`, `UnknownName`, `InvalidNameRestoration`).
- Plan lines 172-183 give `WorkbookOp` as `Fill`, `MoveCells`, `EditName`, `EditTable`, `EditRule`, `EditStyle`,
  `EditObject`, `EditSettings` and `Clear {area, fields}`; none of those exists in `op.rs:33-106`.
- The layering reason for calc-doc-model (line 89) says the model "depends only on the CalcFunc value types and
  service contracts"; it also depends on `calc-syntax` (`Cargo.toml`; the comment above the dependencies omits it).
- `encode_batch` and `decode_batch` are named only in this crate and in `crates/calc-doc/tests/acceptance.rs` and
  `crates/calc-host/tests/{round_trip,charts}.rs` (grep over `crates` and `apps`): no product caller was found, and
  `wire/` has no unit test.
