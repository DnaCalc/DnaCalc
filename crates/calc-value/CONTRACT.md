# calc-value: contract (draft)

Read at origin/master `cb0b76a5`. `value.rs`, `grid.rs` etc. mean `crates/calc-value/src/<file>`; a bare
`:N` continues the last file named. "(inferred)" marks a conclusion drawn without the exact line.

## 1. Purpose and position in the layering
- Lowest layer of CalcFunc: the value types and service contracts that CalcFunc and the CalcDoc model
  share (`lib.rs:1-2`). Portable (`Cargo.toml:9-10`).
- Depends on nothing: no `[dependencies]`, features or build key; wasm32 dev-dependencies only
  (`wasm-bindgen =0.2.117`, `wasm-bindgen-test =0.3.67`, `Cargo.toml:12-22`). `tools/xtask/architecture.json`
  has no rule whose `package` is calc-value; the closures of `xtask` (:107-111) and
  `calc-platform-desktop` (:173-176) may not contain it.
- Direct dependents (their Cargo.toml): calc-func (re-exports it as `calc_func::value`,
  `crates/calc-func/src/lib.rs:16`), calc-catalogue, calc-doc-model, calc-engine, calc-host,
  calc-host-vba, calc-addin-host, calc-com-host, calc-xll-abi, calc-xll-host, tools/calc-verdict,
  dna-calc, dna-calc-model, dna-calc-worker, dna-calc-desktop, dna-onecalc-desktop. calc-verdict
  takes its value types from here so the judge never contains calc-func (architecture.json:104).

## 2. Public API by concept
All modules are private; the surface is the `pub use` list and root functions (`lib.rs:76-87`).
- **Values.** `CalcValue` (`value.rs:584`): a `CoreValue` plus optional private metadata. `CoreValue`
  (`:436`): Number, Text, Logical, Error, Empty, Missing, Array, Reference. `CoreRef<'a>` (`:568`) from
  `core_ref()` (`:722`) is the view callers should match on; `core()` (`:716`) exposes the stored enum.
  Constructors `new`, `number`, `text`, `logical`, `error`, `empty`, `missing`, `array`, `reference`
  (`:653-705`); projections `as_number` .. `as_reference`, `is_empty`, `is_missing` (`:735-784`).
- **Text.** `ExcelText` (`:83`), `Arc<[u16]>` code units: `from_utf16_code_units` (`:89`, exact),
  `from_interop_assignment` (`:97`, cuts at `EXCEL_TEXT_MAX_UTF16_CODE_UNITS` = 32 767, `:16`),
  `utf16_code_units`, `to_string_lossy` (`:114`), `has_dangling_high_surrogate_tail` (`:119`).
- **Arrays.** `ArrayShape` (`:128`); `CalcArray` (`:149`): `new` (`:158`), `from_scalar` (`:170`),
  `from_rows` (`:180`), `get`, `row_slice`, `iter_row_major`, `boxed` (`:201`), `is_boxed`,
  `holds_arrays` (`:212`), `representation_eq` (`:247`).
- **References and grid.** `ReferenceLike` (`:288`): `cell`, `area`, `from_areas` (`:315`), `three_d`
  (`:333`), `sheet`, `spans_sheets`, `areas`; `ReferenceError` (`:260`). `CellKey` (`grid.rs:53`),
  `GridArea` (`:90`; `new`, `cell`, `full_sheet`, `intersection` `:175`, `subtract` `:193`, `contains`),
  `GridError` (`:17`), `MAX_ROWS`, `MAX_COLUMNS` (`:10,13`), `Selection` (`:265`).
- **Errors.** `WorksheetErrorCode` (`value.rs:21`): 14 codes, `excel_spelling` (`:40`), `all` (`:60`).
- **Qualified values.** Private `ValueMetadata` (`:591`), at most one case per value. Callable:
  `CalcValue::callable` (`:709`), `callable_value` (`:786`), `CallableValue` (`:400`),
  `CallableArityShape` (`:369`), trait `OpaqueCallable` (`:392`). Image: `CalcValue::image` (`:799`),
  `image_value`, `ImageValue` (`:521`), `ImageFormat` (`:451`, `sniff` `:463`). Cube: `CalcValue::cube`
  (`:808`), `cube_value`, `CubeValue` (`:512`). Overflow: `excel_overflow_number` (`:668`), `is_excel_overflow`.
- **Equality.** `PartialEq for CalcValue` (`:638`), `same_callable` (`:832`), `representation_eq` (`:843`);
  the four relations are defined in the comment at `:615-637`.
- **Identities.** `WorkbookId`, `SheetId`, `NameId`, `TableId`, `NodeId` (one macro, `id.rs:149-248`):
  `slot`, `generation`, `from_parts` (`:176`). `IdAllocator<T>` (`:113`): `issue`, `release`, `is_live`
  (`:184-198`). `IdError` (`:16`), `NameScope`, `NameKey` (`:220,227`).
- **Names and Unicode.** `workbook_names_equal`, `parameter_names_equal`, `fold_workbook_name`,
  `fold_parameter_name` (`lib.rs:56-74`); `unicode_simple_case_fold`, `is_combining_mark`,
  `is_nonspacing_mark`, `is_connector_punctuation` (`:28-44`).
- **Numeric policy.** `NumericMode` (`numeric.rs:62`; `Fast` default, `Exact(NumericTargetId)`, target `:12`).
- **Sheet helpers kept here.** `COPY_LIMIT`, `matrix_shape` (`lib.rs:90-100`), `stored_axis_size` (`:104`);
  they return English message strings, unlike the typed errors elsewhere in the crate.

## 3. Guarantees to callers and invariants relied on
- **Send + Sync, one shape.** Every exported type is `Send + Sync`; shared payloads are immutable behind
  `std::sync::Arc`, so a clone shares its payload (`value.rs:3-6`, `lib.rs:21-22`). Checked by
  `cfunc_m0_1__send_sync` (`crates/calc-func/tests/acceptance.rs:586`), which also fails on a
  `[features]` table, a thread-bound pointer or cell identifier (comments included), or any `cfg` other
  than the three test forms (acceptance.rs:416-447).
- **Checked constructors.** `CellKey::new` refuses row or column 0 or past the bounds, row first
  (`grid.rs:62-70`). `GridArea::new` refuses two sheets and normalises corners (`:101`).
  `CalcArray::new` gives `None` for a zero dimension or a cell count unlike the shape, so no array is
  empty (`value.rs:158-161`). `from_areas` refuses no areas or two sheets and keeps order and
  duplicates (`:312-326`); `three_d` allows several sheets, and `sheet()` is then the first (`:333-347`).
  `NumericTargetId::new` refuses an empty part (`numeric.rs:27-32`).
- **Not checked here.** `CalcArray` admits `Missing` cells and nested arrays (test `value.rs:981`);
  `CalcValue::number` takes any f64, NaN and infinities included; `Selection` and `CubeValue` have
  public fields. Which kinds a cell may publish is decided in calc-engine
  (`crates/calc-engine/src/result.rs:133-143`), not by a type here.
- **Identities.** A released slot is reissued under the next generation; a slot whose generation would
  wrap is retired (`id.rs:85-101`). Uniqueness holds per allocator only (`:8-9`). `IdAllocator` is not
  `Clone` (`:112`). `from_parts` builds an ID that is live in no local allocator (`:172-176`).
- **Four equality relations** (`value.rs:615-637`; commit 5b60b75b, bead dc-hsa5.19):
  1. Worksheet comparison (`=`, `<`, lookup, coercion, collation) lives in calc-func, never here.
  2. Runtime identity: `same_callable`; `CallableValue ==` is equal arity plus `Arc::ptr_eq` on the
     handle (`:428-432`). Captured state is never compared.
  3. Representation equality: `representation_eq` compares numbers by bit pattern (`-0` differs from
     `0`, a NaN equals itself), every qualification, and array shape, boxed flag and cells (`:843-857`,
     `:247-255`). It decides whether a redelivered RTD or asynchronous result is a duplicate or a
     conflict (calc-fml `rtd::admit`, calc-engine `AsyncReturns`: from the commit message).
  4. Reuse equivalence per facet: no value comparison implements it, and `PartialEq` is not it.
- **`PartialEq for CalcValue`** is structural over core, callable, image and cube, with plain numbers as
  IEEE (`0.0 == -0.0`, `NaN != NaN`); if either side is an overflow number it compares the flag and the
  bits (`:638-650`). `CalcValue` is neither `Eq` nor `Hash`; `ExcelText` and `ReferenceLike` are both.
- **Layout.** Code matching on `CoreRef` does not depend on storage (`:562-566`). Metadata sits behind
  one optional `Arc` so frames do not grow; deep scalar IF must fit a one-megabyte stack (`:589-590`).

## 4. What it must never do
- Use `std::fs`, `std::process`, `std::os` or `std::net`, or stop building for wasm32 (AGENTS.md, Portability).
- Gain a dependency, cargo feature, `build.rs`, public module, or thread-bound pointer or cell
  (`Cargo.toml:12-14`; acceptance.rs:416-447, :589-593).
- Keep a process-global numeric mode (`numeric.rs:1-3`), or parse A1 or R1C1 text (CalcFml's, `grid.rs:4`).
- Implement worksheet comparison or coercion (`value.rs:617-619`), or inspect a callable's handle
  (`:388-389`).
- Grant the overflow qualification to anything but the measured rounding conversion; an identical NaN
  built through `number` never has it (`:664-667`).
- Honesty rules (AGENTS.md): no hard-coded answers; what is not modelled is a typed refusal raised by
  the caller. A new source file needs an entry in `migration/licence-ledger.json`.

## 5. Excel behaviours it owns (behaviour; code; evidence cited)
- Grid of 1 048 576 by 16 384, 1-based: `grid.rs:9-13`; no measurement cited; exercised by
  `cfunc_m0_1__grid_edges` (acceptance.rs:166).
- Text is UTF-16 units and may end in a lone high surrogate; interop assignment cuts at 32 767 units:
  `value.rs:80-103`; doc comment and unit tests `:920`, `:928`; no capture cited.
- `Empty` (empty cell) and `Missing` (omitted argument) are distinct kinds: `:441-444`; no measurement
  cited in this crate; discussed in fable-evaluator-review.md:30-41.
- 14 error codes and spellings, others "not established here" (`:18-57`, nothing cited). A cube value is
  text plus its members or set (`:806-813`, test `:1126`). Error faces: a callable in a cell shows
  `#CALC!` (`:707-714`; the engine cites "retained fn2 q017-020", `calc-engine/src/result.rs:134-135`);
  an image reads as `#VALUE!` where one value is taken, TYPE 128, ISERROR FALSE (`:796-798`, doc only).
- Boxed array (INDEX of an array-holding cell): first element where one value is taken, whole array
  where an array is taken: `:140-147`; Excel 16.0.20527, 2026-10-30, bead dc-h9u (doc comment).
- Overflow number: a rounding result past DBL_MAX is a NaN bit pattern the sheet holds as a number;
  copies keep it: `:664-673`; dc-j9k.19; `crates/calc-engine/tests/fixtures/eng15-overflow/README.md`;
  made only in `crates/calc-func/src/catalogue/rounding.rs:89,135`.
- Union on one sheet with duplicates kept; 3-D reference across sheets: `:312-341`; dc-m3u (doc).
- Names: full Unicode fold for defined, table and column names, simple fold for LET and LAMBDA
  parameters, Turkic and width forms distinct: `name_case.rs:48-69`; table measured on Excel 16.0
  build 20326, 2026-09-15 (`:12-46`); Unicode 15.0.0 tables (`case_folding.rs:1-17`); bead dc-cpr.

## 6. How it is tested
- 35 unit tests in the source files (value 15, name_case 6, id 4, case_folding 4, combining_marks 3,
  grid 2, numeric 1), all with the dual attribute (native `#[test]`, `wasm_bindgen_test` on wasm32);
  `cargo xtask sweep` runs them in headless Chrome and Firefox (AGENTS.md). Equality tests:
  `value.rs:1126`, `:1166`, `:1213`. `grid.rs:287` checks `subtract` exhaustively on small areas.
- calc-func acceptance cases `cfunc_m0_1__grid_edges` and `cfunc_m0_1__send_sync` read this crate's
  sources and manifest at compile time. `tests/` holds only `.gitkeep`; `acceptance.json` has no beads.
- No corpus or bank targets the crate alone; `cargo xtask corpus --oracle` and `cargo xtask witness judge
  --bank full` exercise it (5b60b75b: totals unchanged, 76 354 of 78 151 formula cells, 3 887 850 of
  3 910 795 rows). AGENTS.md asks for `cargo xtask check --dependents` on a core contract change.

## 7. Open items
- dc-dxm step 2: a smaller layout behind `CoreRef` (`value.rs:563-564`); the review estimates about 56
  bytes per value today (fable-evaluator-review.md:38-39).
- `Missing` can enter arrays and is refused at return (`MissingReturned`, `crates/calc-func/src/call.rs:450`);
  REWORK_PLAN.md:30 (R1) asks whether its normalisation is reachable or dead. No bead id named.
- No bead id named for these three: number equality is IEEE in `PartialEq` except overflow values, and
  the review proposes bits (fable-evaluator-review.md:37-42); R5 (REWORK_PLAN.md:49-52) wants argument
  origin kept (raw blank vs computed Empty, boxed vs held), which would change types here (inferred);
  collation expansions (`Æ` = `AE`, `Œ` = `OE`) in workbook names are a documented gap
  (`name_case.rs:56-59`, test `:223`).
- Not representable: spill anchor, external workbook reference (`value.rs:285-286`); column widths
  below one character (`lib.rs:102-103`); error codes beyond the 14 (`value.rs:18-19`).
- Landed: dc-hsa5.19 (cube metadata in `eq`, four relations). Named in code, tracker state not read:
  dc-cpr, dc-p96 (`id.rs:174`), dc-h9u, dc-m3u, dc-j9k.19.

Verified: cited lines of value.rs, grid.rs, id.rs, numeric.rs, lib.rs, Cargo.toml; 5b60b75b message; architecture.json.
Inferred: R5's effect here; the dual-attribute form of all 35 tests (grep count, not each test read).
Not read: `rtd::admit` and `AsyncReturns` call sites; tracker state; table bodies of case_folding.rs, combining_marks.rs.

## Findings for the tracker (as reported by the drafting agent at cb0b76a5; verify before filing)

- `tools/xtask/architecture.json` has no rule naming calc-value; its bottom position is enforced only by its empty
  dependency list.
- Empty against Missing, UTF-16 text with lone surrogates, the 14 error codes and the image `#VALUE!` face cite no
  Excel capture in the crate, only doc comments and unit tests.
- Four open items in section 7 have no bead id in code or in REWORK_PLAN.md.
