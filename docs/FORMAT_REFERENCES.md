# OxDoc Format Reference Provenance

Historical OxDoc reference index. Source: [OxDoc `c103183`, `docs/FORMAT_REFERENCES.md`](https://github.com/DnaCalc/OxDoc/blob/c1031833ad91b82b6e6d777df260c1eb34e62fb9/docs/FORMAT_REFERENCES.md).

For the current product surface, see the [CalcDoc contract](../crates/calc-doc/CONTRACT.md).
The scope statements below describe OxDoc at the source revision. CalcDoc also reads binary
Excel workbooks. The [MS-XLSB chart-sheet specification](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/3e08e9aa-bad3-4b65-821e-8f9b8219494d)
and [legacy format work](plan/BULK_PORT_PLAN.md) provide further context for binary inputs.

This document gathers the authoritative reference material OxDoc should use when building its document model, `.xlsx` reader/writer, preservation policy, and conformance fixtures. It also records where the wider DNA Calc project already mirrors or extracts those sources in `Foundation`.

## OxDoc fixtures used by CalcDoc

The [CalcDoc OxDoc suite](../crates/calc-doc/tests/d1_oxdoc.rs) embeds package parts from
`crates/oxdoc-conformance/fixtures/` at the revision above.

| Source fixture | CalcDoc coverage |
|---|---|
| `w005-structure-customxml` | An unrelated cell edit keeps the CustomXML item, properties and relationships, plus the worksheet's view, merges, hyperlinks, validation, AutoFilter and sort metadata. |
| `w008-drawing-form-controls` | An unrelated cell edit keeps the drawing, chart, images, control property parts, VML, OLE bytes and their relationships. Formulas, linked ranges and macro assignments in those parts travel with them. |
| `w009-review-metadata` | An unrelated cell edit keeps legacy notes, threaded comments and replies, person metadata, VML, relationships and workbook name metadata. |
| `w010-shared-strings-edit` | Appending new text preserves every existing shared-string entry, including rich text, phonetic runs and leading spaces. |
| `formula-topology` | The source combines valid records with an unknown formula kind and defective shared groups, which stops CalcDoc load before the other cases can be checked. CalcDoc has separate [cell and formula load cases](../crates/calc-doc/tests/acceptance.rs) for formula topology, caches and refusals. |
| `malformed-formula-topology`, `malformed-workbook` | CalcDoc exercises typed worksheet-cell and workbook XML errors through the same [load acceptance suite](../crates/calc-doc/tests/acceptance.rs). |

CalcDoc models aspects of notes, threaded comments, hyperlinks, drawings, controls, merged regions,
views and filters. The fixture assertions exercise package preservation across a cell edit.
They cover the CustomXML relationship graph, opaque extension data and sort metadata through the
file codec and leave feature-specific editing behavior to its own tests.

## Authority order

Use references in this order:

1. **Normative OOXML standard**: ECMA-376 / ISO/IEC 29500 for package structure, SpreadsheetML, markup compatibility, and transitional features.
2. **Microsoft Open Specifications**: MS-XLSX for Excel `.xlsx` extensions, plus MS-OI29500 and MS-OE376 for Microsoft Office implementation notes, known deviations, and extension behavior.
3. **Foundation mirrors and extraction runs**: byte-preserved downloads, SHA-256 hashes, source ids, and processed `SPEC-*` / `CONF-*` candidates under `C:/Work/DnaCalc/Foundation/reference/`.
4. **Excel-observed evidence**: OxXlPlay/OxReplay bundles and OxDoc corpus fixtures for behavior the written specs do not settle.

Do not treat blogs, Stack Overflow answers, library behavior, or Wikipedia-like summaries as normative sources. They may be discovery aids only.

## Primary external references

- ECMA-376, Office Open XML File Formats: `https://ecma-international.org/publications-and-standards/standards/ecma-376/`
  - Use Part 1 for Fundamentals and Markup Language Reference, including SpreadsheetML.
  - Use Part 2 for Open Packaging Conventions: packages, parts, relationships, content types, core properties, and signatures.
  - Use Part 3 for Markup Compatibility and Extensibility.
  - Use Part 4 for Transitional Migration Features when reading or preserving transitional workbooks.
- MS-XLSX, Excel `.xlsx` Extensions to the Office Open XML SpreadsheetML File Format: `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/2c5dee00-eff2-4b22-92b6-0738acd4475e`
  - Use for Excel-specific extensions and for anchors into the downloadable PDF/DOCX spec.
  - Foundation source id: `REFX-001`.
- MS-XLSX formula grammar anchor: `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/3d025add-118d-4413-9856-ab65712ec1b0`
  - Foundation source id: `ECS-008`.
  - OxDoc preserves and streams formula text plus file-topology records for shared, array, data-table, cached-value, and calc-chain surfaces; formula semantics stay with OxFml/OxFunc.
- MS-XLSX cell formulas anchor: `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/bb813b21-5c61-41e8-b91c-3ebb4550666c`
  - Foundation source id: `ECS-009`.
- MS-XLSX rich value error types (section 2.3.6.1.3, the `_error` rich value structure with its `errorType` / optional `subType` keys): `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/1d44a793-c90a-47da-8943-08802dfad1fd`
  - Per-error sections quoted by OxDoc: Spill Error Type (`errorType` 8, with `colOffset` / `rwOffset`): `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/7a27b5fb-48a8-4a1c-b67a-2ca5edc07514`; Unknown Error Type (11): `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/98d06bc5-fa31-4bcc-8425-2665aa251288`; Calc Error Type (13): `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/a4cdc4c3-45b4-45c4-be49-d244f598a538`.
  - OxDoc resolves a `<c t="e" vm="n">` cell's real error through `xl/metadata.xml` and `xl/richData/*` (`oxdoc-gnl`); the `errorType` enumeration is `ERROR.TYPE - 1` (`errorType` 2 `#VALUE!` and 12 `#FIELD!` are pinned by the in-tree Excel-authored corpus). The vocabulary lives in `oxdoc_model::cell_error_code`.
- MS-XLSB `BErr` (the 1-byte error code table behind the classic seven codes and `0x2B` `#GETTING_DATA`): `https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/c3d25119-1da4-44dc-bdb5-19b8ba2ddf90`
  - OxDoc's bytes for `#SPILL!` .. `#CALC!` (`0x2C` .. `0x31`) are its own, continuing that table in `ERROR.TYPE` order; `.xlsb` is out of scope.
- MS-OI29500, Office Implementation Information for ISO/IEC 29500 Standards Support: `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/1fd4a662-8623-49c0-82f0-18fa91b413b8`
  - Use for Microsoft Office behavior relative to ISO/IEC 29500.
  - Foundation source id: `REFX-004`.
- MS-OE376, Office Implementation Information for ECMA-376 Standards Support: `https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/db9b9b72-b10b-4e7e-844c-09f88c972219`
  - Use for Microsoft Office behavior relative to ECMA-376.
  - Foundation source id: `REFX-005`.
- MS-OVBA, Office VBA File Format Structure: `https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/b39ac32f-0ce1-4533-9297-2ff3ff62c9ec`
  - Use for `vbaProject.bin` CFB storage, compressed `VBA/dir`, module stream metadata, `MODULEOFFSET`, `PROJECTCODEPAGE`, and MS-OVBA source compression/decompression.
  - Foundation mirror path: `../../Foundation/reference/downloads/learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/b39ac32f-0ce1-4533-9297-2ff3ff62c9ec.md`.

The Microsoft OpenSpecs URLs above were checked live on 2026-06-16. The ECMA-376 page identifies the standard as Office Open XML file formats, 5th edition, and lists the four parts and ISO/IEC number 29500.

## Foundation provenance already available

Foundation already has the reference-processing doctrine and most Microsoft Office format source mirrors OxDoc needs:

- `../../Foundation/REFERENCE_SPEC_FORMAT_AND_CONFORMANCE.md`
  - Defines the raw mirror, normalized reference, conformance-candidate, empirical finding, and unified conformance-source layers.
- `../../Foundation/reference/spec_seeds.csv`
  - Seeds Microsoft OpenSpecs discovery, including `MS-OFFSTANDLP`, `MS-XLS`, `MS-XLSB`, and Office 2003 XML references.
- `../../Foundation/reference/index.csv`
  - Source registry with local paths, download status, byte sizes, captured timestamps, and SHA-256 hashes.
- `../../Foundation/reference/conformance/excel-worksheet-engine/SOURCE_BINDINGS.csv`
  - Bridges evidence ids such as `ECS-*`, `REFX-*`, `EMP-*`, and `INT-*` to sources used by worksheet-engine requirements.
- `../../Foundation/reference/runs/20260305-ms-formatting-formal-pass-01/outputs/`
  - Processed extraction run over `MS-XLSX`, `MS-OI29500`, `MS-OE376`, and related OpenSpecs pages.

Important mirrored Microsoft artifacts:

- `../../Foundation/reference/downloads/officeprotocoldocs-f5hpbjgea6b8gneq.b02.azurefd.net/files/MS-XLSX/[MS-XLSX].pdf`
  - SHA-256 in Foundation selected sources: `81aeb7f35ce906eb673eabb05a6dfb5d36ec7a5fb1fb8caf7f82812cceab42cc`.
- `../../Foundation/reference/downloads/officeprotocoldocs-f5hpbjgea6b8gneq.b02.azurefd.net/files/MS-XLSX/[MS-XLSX]-260108.docx`
  - SHA-256: `8bdea2143ec7478df201b91184c66cbc4f2ae92fd3462d4d14dd9b5d4590fbfa`.
- `../../Foundation/reference/downloads/learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/2c5dee00-eff2-4b22-92b6-0738acd4475e.md`
  - SHA-256: `e10a0e5a4c37246fd326e9dc281a0e0bd3124d4cc5fd60051a41937119aeae4c`.
- `../../Foundation/reference/downloads/officeprotocoldocs-f5hpbjgea6b8gneq.b02.azurefd.net/files/MS-OI29500/`
  - Microsoft implementation notes for ISO/IEC 29500.
- `../../Foundation/reference/downloads/officeprotocoldocs-f5hpbjgea6b8gneq.b02.azurefd.net/files/MS-OE376/`
  - Microsoft implementation notes for ECMA-376.

Foundation does not appear to mirror the ECMA-376 PDF parts directly yet. Until that is added, cite the ECMA public page plus the exact part and clause, and pair any implementation-sensitive decision with Microsoft OpenSpecs or Excel-observed evidence.

## How OxDoc should cite sources

When adding OxDoc requirements, model fields, event variants, fixture expectations, or ledger classifications:

- Cite at least one source id or source path from this file.
- Prefer Foundation ids where they exist: `REFX-*` for mirrored specs, `ECS-*` for existing worksheet-engine web anchors, `EMP-*` for curated empirical findings.
- For processed claims, cite the extracted `SPEC-*` or `CONF-*` item id and its run output path.
- For unresolved or contradictory behavior, keep both the spec source and the Excel-observed evidence visible; do not silently choose one.
- For opaque preservation, cite both the OOXML package/relationship source and the feature-specific source, if any.

## Ongoing OxDoc source needs

As the `.xlsx`/`.xlsm` reader-writer grows, OxDoc should add or reference focused source notes for:

- OPC package graph: content types, relationships, central-directory traversal, part naming, and relationship target handling.
- SpreadsheetML workbook and worksheet parts: workbook, sheet list, sheet data, shared strings, styles, sheet dimensions, sheet views/panes, merges, hyperlinks, data validations, auto filters, sort state, tables, defined names, calc chain, comments, threaded comment storage, person metadata, and drawings.
- Shared formulas, array formulas, data-table formulas, cached formula values,
  calc-chain invalidation, and formula text preservation.
  The active OxDoc formula-topology scope note is
  [`FORMULA_TOPOLOGY_SCOPE.md`](https://github.com/DnaCalc/OxDoc/blob/c1031833ad91b82b6e6d777df260c1eb34e62fb9/docs/FORMULA_TOPOLOGY_SCOPE.md); it uses these sources
  to define file-record coverage while keeping formula semantics in OxFml/OxFunc
  and workbook model policy in OxCalc.
- Styles, differential styles, conditional formatting rule families, extension lists, and rendering-vs-file-format boundaries.
- Package/storage surfaces: CustomXML item/itemProps parts, datastore item ids, schemaRefs, relationships, and content types.
- Sheet drawings and form controls: worksheet `drawing`, drawing
  relationships, `xdr:wsDr`, absolute/one-cell/two-cell anchors, shape shells,
  worksheet `controls/control`, classic `controlPr`, Office 2010+
  `x14:formControlPr`, legacyDrawing/VML detection, raw linked formula/range
  strings, raw macro assignment strings, and deferred chart/OLE/ActiveX
  blockers. The active W008 scope/status note is
  [`DRAWING_FORM_CONTROLS_SCOPE.md`](https://github.com/DnaCalc/OxDoc/blob/c1031833ad91b82b6e6d777df260c1eb34e62fb9/docs/DRAWING_FORM_CONTROLS_SCOPE.md), with
  conformance evidence under
  `crates/oxdoc-conformance/fixtures/w008-drawing-form-controls/`.
- Opaque and geometry-coupled parts: VBA project, drawings, pivot table definitions, pivot caches, unmodeled custom XML related parts, print areas, and sheet anchors.
- Review metadata: legacy `commentsN.xml` notes, threaded comment parts,
  workbook person-list storage, legacy placeholder comments, VML comment
  geometry linkage, and `definedName` metadata attributes. The active W009
  scope/status note is [`REVIEW_METADATA_SCOPE.md`](https://github.com/DnaCalc/OxDoc/blob/c1031833ad91b82b6e6d777df260c1eb34e62fb9/docs/REVIEW_METADATA_SCOPE.md).
- Strict vs transitional handling and markup compatibility behavior.

Each note should use this provenance chain: external standard or OpenSpecs source, Foundation mirror/hash or extraction id where available, then OxDoc fixture/evidence id once implemented.
