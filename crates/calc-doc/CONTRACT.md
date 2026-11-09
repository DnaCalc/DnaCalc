# calc-doc contract (draft)

Read at origin/master `cb0b76a5` of DnaCalc. Short paths (`save/mod.rs:821`) are under `crates/calc-doc/src/`;
`tests/...` is under `crates/calc-doc/`; `d1/` = `tests/fixtures/d1/`. RC = `docs/architecture/RECALC_CONTRACT.md`;
probes = `docs/architecture/EXCEL_CALC_PROBES.md`; P06, P09 = README in `crates/calc-engine/tests/fixtures/
excel-calc-probes/<id>/`; model = `crates/calc-doc-model/src/`.

## 1. Purpose and position

- The CalcDoc codec: reads, writes and preserves .xlsx/.xlsm packages (ZIP, OPC, SpreadsheetML) and reads .xlsb
  by converting it to the SpreadsheetML package it stands for (`lib.rs:1-2`). It reads into and writes from
  `calc-doc-model`, re-exported as `model` (`lib.rs:95`). It opens no file: the caller supplies source and sink
  (`lib.rs:71`, `source.rs:62,78`).
- Depends on `calc-catalogue`, `calc-doc-model`, `calc-fml` (lexer, parser, entry check, reference templates; grep
  finds no `calc_fml::eval`), `quick-xml`, `miniz_oxide`; `portability = "portable"` (`crates/calc-doc/Cargo.toml`).
  Used by `calc-host`, `calc-host-vba`, `dna-onecalc`, `dna-calc-worker`, `calc-oracle-excel`, `xtask-harness`.
- `tools/xtask/architecture.json` has no rule whose package is `calc-doc`; it is only a forbidden entry in other
  closures (`calc-fml`, `calc-engine`, `calc-doc-model`, `calc-verdict`, `dna-calc` and more). The `dna-onecalc`
  and `dna-formula-ui` rules forbid `calc-engine` and `calc-host` and allow `calc-doc` (lines 51-67).

## 2. Public API by concept

- **Open.** `open(Arc<dyn PackageRead>, &OpenPolicy)` -> `OpenedDocument` or `OpenError` (`lib.rs:155`,
  `error.rs:31`); limits in `PackageLimits`, `XmlLimits` (`policy.rs:10,39`). `Package::read_part`, `read_xml`
  (`opc.rs:324,332`) decompress and tokenize only on demand.
- **Load.** `load(&OpenedDocument, &mut IdAllocator<SheetId>)` -> `LoadedWorkbook` or `LoadError`
  (`load/mod.rs:1471`, `load/error.rs:12`); `load_preserving_sheet_ids` (1480). It holds `model()` (a `ModelRoot`
  at the first revision, 723), `sheets()` (782), `styles()` (803), `date_system()` (814), `calc_mode()` (825),
  `layout_warnings()` (865). Of `calcPr`, `calcMode` and the `iterate*` settings are read (1252-1281).
- **Cached values with provenance.** Beside the model, per sheet: `LoadedSheet::formula(r, c)` -> `FormulaRecord
  { shared, cache }` (`load/mod.rs:298,313`); `value_cache`, `value_caches` (211, 216) for cells with a cache and
  no input (spill members, data table cells, macro sheet and rejected formulas). A cache is `model::FileCached`:
  a `CachedValue` and the `SourcePart` (name, CRC-32, size) it was read from (model `cache.rs:16,42,69`).
- **Load diagnostics.** `LoadError` with `CellError` per cell (`load/error.rs:12,53`); cells kept but not loaded
  are `model::UnloadedCell` (`load/sheet.rs:527,564,372`); `LayoutWarning` is never a refusal (`lib.rs:34-37`).
- **Edit and save.** `prepare_edit(&ModelRoot, &OpBatch)` -> `PreparedEdit` or `EditError` (`edit.rs:169,35`).
  `LoadedWorkbook::snapshot`, `snapshot_with_values`, `snapshot_with_results` (values plus spill areas) ->
  `DocumentSnapshot` (`load/mod.rs:919,945,962`); `OpenedDocument::snapshot` saves as opened (`lib.rs:148`).
  Modifiers: `with_structural_edits`, `with_custom_xml`, `with_calculation_save_state(dirty, completed, mode,
  calculate_before_save)` (`save/mod.rs:152,166,184`). `prepare_save(&snapshot, &SavePolicy)` -> `SavePlan` or
  `SaveRefusal` (821, 280); `write(plan, &mut dyn PackageWrite)` -> `SaveReceipt` or `WriteError` (980, 743,
  783). `SaveFormat::{Xml, Spreadsheet(SpreadsheetFormat), Xlsb}` (236).
- **Which caches are written.** Without values every formula's `<v>`, `t` and `vm` are left out
  (`save/sheet.rs:26-31`). With values each supplied value is written; a formula without one, or with #BUSY!, is
  left for Excel and counted (`values_left_to_excel`, `caches_dropped`, `save/mod.rs:751-768`). A value for a cell
  without a formula is `SaveRefusal::ValueWithoutFormula` (359). Saving the loaded revision copies the package.
- **`ca`, `calcPr`, calcChain.** Only a snapshot with a calculation state is tracked: a retained scalar cache gets
  `ca="1"` when its cell is in the caller's dirty set and no `ca` otherwise (`save/sheet.rs:973-976,1399-1410`;
  shared members 488-490; arrays excluded, 1404), and `calcPr` gets `calcMode`, `calcCompleted`, `calcOnSave`
  (`save/parts.rs:479-501`). `fullCalcOnLoad="1"` is set when an array formula is written without a value or a
  data table keeps caches the caller did not supply (`save/author.rs:813-826`). The calcChain part, relationship
  and content type are removed when the set of formula cells or arrays changed or cells moved (779-797).
- **XLSB.** `open` converts a binary workbook first and keeps the validated original (`lib.rs:160-168`);
  `convert_binary_workbook` (177). Converted: workbook, sheets, macro sheets, styles, shared strings, tables,
  external links, comments, metadata; binary index, calcChain and volatile dependencies are left out; an unread
  part kind is `OpenError::BinaryWorkbook` (`xlsb/mod.rs:1-21,91-116`). The model of an .xlsb is the model of
  its .xlsx twin (`tests/dc_5sr_xlsb.rs:145,173`). The default save is .xlsx or .xlsm; `SaveFormat::Xlsb` copies
  the original only when no part is rewritten, else a dc-snz.14 refusal (`save/mod.rs:845-857`).
- **Parts it does not model.** Every entry a save does not rewrite is copied as its stored bytes, in stored order
  (`save/mod.rs:813-814,901-911`); in a rewritten worksheet, every byte outside `<sheetData>` (`save/sheet.rs:3-5`).
- **Formula file form.** `file_form`, `file_form_with_links`, `file_form_with_lookup` -> `FileForm { text, dynamic
  }` or `FileFormError` (`save/file_form.rs:650,657,677,54,63`): `_xlfn.`, `_xlfn._xlws.`, `_xlpm.`, `_xlop.`,
  `_xleta.`, `_xlfn.ANCHORARRAY`, `_xlfn.SINGLE`, link numbers `[N]` (6-25). A dynamic array is written `<f
  t="array" ref="..">` with a `cm` naming an `XLDAPR` record; a legacy array the same without `cm`
  (`save/sheet.rs:957-972`, `save/array_metadata.rs:4-11`). Load reads `t` normal, shared, array, dataTable
  (`load/sheet.rs:683-790`), resolves `cm` to `ArrayKind::{Legacy, Dynamic}` (714-723) and keeps the text as
  filed after `_xHHHH_` decoding (332-344; prefixes are not removed here, inferred).
- **Other content.** Shared strings: appended on save, indices kept (`save/strings.rs:1-2`). Styles: `StyleTable`
  (`load/styles.rs:1-4`). Defined names: in the model's definitions (`load/mod.rs:836-839`); unchanged names stay
  as written (`save/parts.rs:218-224`). Tables: name, area, header and totals counts, columns; an edit in a
  header or totals row is `SaveRefusal::EditInTable` (`load/tables.rs:1-3`, `save/mod.rs:305-315`). External
  links: cached books; a reference to an unread link is refused (`load/external.rs:1-12`). VBA: an opaque blob,
  `vba_project` (`load/mod.rs:742`); macro-free output of a book with macros is `SaveRefusal::MacroLoss`
  (`save/mod.rs:834-835`). Custom XML: opaque items by namespace (`load/mod.rs:856`, `custom_xml.rs:21`).
  OneCalc session file: `read_session`, `write_session` over the same route (`session.rs:208,508,1-9`).

## 3. Guarantees and invariants

- **A refusal comes before any byte.** All rewritten parts are serialized in `prepare_save` (`save/mod.rs:276-278`).
  `write` first reads every copied entry and checks its CRC-32 against the one recorded at open; a mismatch is
  `WriteError::SourceChanged` with an empty sink (956-973). A later failure leaves a sink to discard (777-781).
- **Byte preservation and its limits.** Copied entries are their stored bytes (`zip/mod.rs:4-9`). Bytes before the
  central directory that belong to no entry are not written; `SaveReceipt::gap_bytes_dropped` counts them
  (`save/mod.rs:769-774`). No ZIP64: a package past 4 GiB - 1 is refused (253-255). An untouched Excel-saved
  fixture saves byte for byte (`tests/d1_arrays.rs:199`).
- **Never invented on load.** A cache is never part of the input (`load/mod.rs:6-8`). An empty `<v/>` on a
  number-typed formula, and a rich value that is no error, give no cache (`load/sheet.rs:579-590`). A macro
  sheet's formula is not read (516-528). A formula Excel's entry refuses by a measured rule gets no input
  (346-378). No layout is invented (`lib.rs:36-37`).
- **Bounds and lineage.** Limits are checked against the central directory before any payload; decompression
  stops at the declared size; any DOCTYPE is refused (`policy.rs:8-21`, `xml.rs:4-7`). A model that is no revision
  of the loaded one is `SaveRefusal::ForeignModel` (`save/mod.rs:295-298`). A formula past 8,192 UTF-16 units is
  refused on load, edit and save (`load/mod.rs:80`). An edit never changes the caller's revision (`lib.rs:44`).

## 4. What it must never do

- Touch `std::fs`, `std::process`, `std::os`, `std::net` (AGENTS.md, Portability; grep finds none in `src/`), or
  stop building for `wasm32-unknown-unknown`.
- Evaluate a formula or depend on `calc-engine` or `calc-host` (plan line 231, "codec knows no engine"; case
  `cdoc_m0_1__no_engine_dependency`, `tests/acceptance.rs:3353`).
- Present a file cache as calculated (honesty rule 1). Values given to a save "grant no calculation or freshness
  proof" (`save/mod.rs:90-92`); `ca` and `calcCompleted` report the caller's state, never currency (173-174).
- Guess: what it does not read is a typed refusal or an unloaded cell, and an `owner` field names a bead. Write a
  stale calcChain, overwrite a manifest it did not read (`SaveRefusal::LayoutNotWritable`), or drop VBA silently.

## 5. Excel behaviours owned, with the measurement

Open-time rows name variants of `d1/excel-open.json` (Excel 16.0.20430), written by `d1/measure-open.ps1`.

| Behaviour | Measured by | In code |
|---|---|---|
| A `<v>` is shown on open, not recalculated | `wrong-cache` (A2=99) | `load/sheet.rs:387` |
| Formula without `<v>` is calculated on open | `no-value`, `no-value-no-chain` | `save/sheet.rs:26-31` |
| Array without `<v>`: only under `fullCalcOnLoad` | `array-no-value`, `no-values-full` | `save/author.rs:813-826` |
| `fullCalcOnLoad` or older `calcId` recalculates a wrong `<v>` | `wrong-full`, `old-calc-id` | flag not read on load |
| A chain listing a constant cell stops the open | `chain-extra`; `no-chain` opens | `save/author.rs:779-797` |
| Manual Save(false): stale `<v>`, `ca="1"`, `calcCompleted="0"` | P06:15-16 (20527) | `save/sheet.rs:973-976,1402` |
| Spill members carry an empty `<f ca="1"/>` | `tests/d1_arrays.rs:22` | `load/sheet.rs:530-547` |
| File form of formulas entered through `Formula2` | `d1/measure-file-form.ps1` | `save/file_form.rs:1-34` |
| Dynamic array: `cm`, `t="array" ref`; CSE without `cm` | P09:27-29 | `load/sheet.rs:714-723` |
| A function taken as a value is `_xleta.`, no array | dc-h9u, `tests/d1_file_form.rs:126` | `save/file_form.rs:16-19` |
| Newer error: `t="e"`, `#VALUE!`, `vm` to `_error` | `d1/excel-errors.json` (inferred) | `load/metadata.rs:9-12` |
| `[N]` references read the link's cached values | dc-ylr, dc-ifc captures (20527) | `load/external.rs:1-12` |

`ca`: load reads it into the private `calculate_always` (`load/sheet.rs:180,493`), used only to accept the empty
spill-member marker (536). Save writes `ca` from the caller's dirty set, never from it (`save/sheet.rs:489,1399`).

## 6. How it is tested

- `tests/acceptance.rs`: cases named `<bead>__<case>`, registered in `crates/calc-doc/acceptance.json` for
  windows, linux and wasm. Other suites: `d1_arrays`, `d1_authoring`, `d1_file_form` (compares `file_form` with the
  `d1/` measurements, 54, 95), `d1_oxdoc`, `dc_5sr_xlsb`, `dc_xuw_macro_sheet`, `session_file`, `vba_udf_names`.
- Fixtures: Excel-saved packages with generator scripts (`tests/fixtures/cdoc_m1_1`, `cdoc_m1_2`, `cdoc_m2_1`,
  `dc_5sr`, `dc_xuw`, `vba_udf_names`); OxDoc's conformance fixtures as part trees with golden transcripts
  (`d1/oxdoc/c1-preservation/golden_*_transcript.json`), zipped in memory (`tests/d1_oxdoc.rs:1-14`).
- AGENTS.md rule: a portable test has `#[cfg_attr(not(target_arch = "wasm32"), test)]` plus the
  `wasm_bindgen_test` attribute and `include_bytes!` data; one that cannot run in a browser keeps `#[test]` and a
  `/// Native-only: <reason>` line (`tests/d1_oxdoc.rs:270`). Browser runs: `cargo xtask sweep`, not per commit.
- Product route: calc-host calls `open` and `load` and passes every cache to `EngineState::with_file_caches`
  (`crates/calc-host/src/state.rs:280-303`); the corpus harness uses the same entry points
  (`tools/xtask-harness/src/oracle_cases.rs:125`). Save flags: `crates/calc-host/tests/round_trip.rs:5329-5348`.

## 7. Open items

- dc-snz.14: authored .xlsb output (`save/mod.rs:242-244,854`). dc-xuw: a changed macro sheet is refused (318-322).
  dc-bctq: templates, macro loss (835). dc-8wp: pivots. dc-e5z5, dc-971p.49: charts. dc-971p.39: outline.
  dc-971p.16: window views. dc-971p.6: style expansion. Table header and totals rows: "no bead owns it yet" (305-309).
- The loaded `ca` flag has no consumer (findings). P24 (open order, flags) is not run; it extends
  `d1/measure-open.ps1` (probes:337-339). P06: disabled-sheet stale caches can lack `ca` and `calcCompleted`
  (P06:25-28); that absence is not reproduced (RC:841-843).
- Unmeasured: whether Excel's 8,192 limit counts `=` or UTF-16 units (`load/mod.rs:77-79`); the `PackageLimits`
  defaults (`policy.rs:3-4`). Cell type `d` is refused (`load/error.rs:56`).

Verified: each `path:line` above was read or grepped at origin/master `cb0b76a5`, including every `ca` site.
Inferred: prefixes kept in the model; which `d1` file measured newer errors; what follows from the lost `ca`.
Not read: `xlsb/` below `mod.rs`, `session.rs` body, charts, pivots, styles, review, `tests/acceptance.rs` bodies.

## Findings for the tracker (as reported by the drafting agent at cb0b76a5; verify before filing)

- `calculate_always` is read at `load/sheet.rs:493` into a private field (180) used only at 536; a grep of the
  whole workspace finds no other reader, and `FormulaRecord` (`load/mod.rs:313-316`) does not carry it. Every node
  is pending after load (`crates/calc-engine/src/state.rs:826-830`), so the cell is still calculated once; what
  is lost is Excel's always-calculate request after that (inferred), and a tracked save rewrites `ca` from the
  host's dirty set alone (`save/sheet.rs:1402-1410`).
- `lib.rs:29` and `load/mod.rs:16-17` say no `calcPr` attribute other than `calcMode` is read; `load/mod.rs:
  1260-1281` reads `iterate`, `iterateCount` and `iterateDelta`.
- `lib.rs:29-30` and `load/mod.rs:14-15` say defined names are counted and not loaded; `load/mod.rs:836-839`
  says each is in the model's definitions, and `load/mod.rs:1677` builds them.
- RC:57, RC:678 and probes:43 cite `calc-doc/src/save/author.rs:712-723` for the `fullCalcOnLoad` rule; it is at
  `save/author.rs:813-826` (lines 714-724 now build the dirty-cache sets).
- RC:684-685 says `CalculateBeforeSave` is absent and cites `xlsb/workbook.rs:562`; the conversion is at
  `xlsb/workbook.rs:658-660`, and save now writes `calcOnSave` from `calculate_before_save`
  (`save/parts.rs:490-499`), supplied by `crates/calc-host/src/save.rs:389-394`.
- `load/error.rs:28` documents `UnsupportedSheet` as covering a macro sheet; `load/mod.rs:1396-1397` accepts
  Excel 4.0 macro sheets (dc-xuw).
- `docs/plan/VBA_UDF_NAMES_IN_FILES.md:271` names `LoadedWorkbook::read_vba_project`; the method is `vba_project`
  (`load/mod.rs:742`). Its line 210 cites `load/mod.rs:934` for defined names, which is now a snapshot comment.
- `xlsb/mod.rs:12-14` says a save of an .xlsb writes an .xlsx or .xlsm; `SaveFormat::Xlsb` also copies the
  original binary package when nothing is rewritten (`save/mod.rs:242-244,845-857`).
