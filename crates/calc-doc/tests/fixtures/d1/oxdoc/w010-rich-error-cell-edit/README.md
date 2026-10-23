# W010 Rich Error Cell Edit Fixture (`oxdoc-gnl`)

Status: checked-in W010 (Measured Excel) conformance fixture.

This fixture stores `.xlsx` package parts as readable XML files under `parts/`.
The conformance tests zip those parts in memory before passing them to
`oxdoc-xlsx`.

The parts follow the shape Excel writes for rich errors (`mc:Ignorable`,
`xr:uid`, `<row spans>`, `x14ac:dyDescent`, `standalone="yes"`, single-line
XML). `xl/metadata.xml`, `xl/richData/rdrichvalue.xml`,
`xl/richData/rdrichvaluestructure.xml`, and `xl/richData/rdRichValueTypes.xml`
are the parts of the in-tree corpus workbook
`SCN-EK011-FUNCTION-RECOGNITION.xlsx` (Foundation research runs,
`excel-compat-empirical-pass-01`) merged with the `#FIELD!` record of
`SCN-EB028-FIELDVALUE-ACCEPT.xlsx`: two `_error` structures and four rich
values, each referenced by one `valueMetadata` record.

Coverage (`xl/worksheets/sheet1.xml`):

- `A1`: plain numeric cell, `<c r="A1"><v>3</v></c>`
- `B1`: `#FIELD!` formula cell, `<c r="B1" t="e" vm="1">` over
  `_xlfn.FIELDVALUE(A1,"Price")` (rich value `errorType` 12, `field` `Price`)
- `C1`: `#CALC!` formula cell, `vm="2"` over `_xlfn.BYCOL(1)` (`errorType`
  13, `subType` 1)
- `D1`: a rich `#VALUE!` formula cell, `vm="3"` over `_xlfn.IMAGE(1)`
  (`errorType` 2, `subType` 10): the resolved error is the same classic
  `#VALUE!` the `<v>` fallback carries
- `E1`: styled `#CALC!` formula cell, `<c r="E1" s="1" t="e" vm="4">` over
  `_xlfn.LAMBDA(1)` (`errorType` 13, `subType` 24)
- `F1`: classic error cell with no `vm=`, `<c r="F1" t="e"><v>#DIV/0!</v></c>`

Every rich-error cell carries `<v>#VALUE!</v>`, the fallback Excel writes for
readers that do not follow `vm=`. `xl/styles.xml` carries four `cellXfs`
entries (0-3) so `E1` can keep `s="1"` through an edit.

Reference anchors:

- `docs/FORMAT_REFERENCES.md`
- ECMA-376 Part 1 / SpreadsheetML `CT_Cell` (`r`, `s`, `t`, `cm`, `vm`, `ph`)
- ECMA-376 Part 1 / SpreadsheetML cell metadata (`xl/metadata.xml`)
- [MS-XLSX] 2.3.6.1.3 Error Types (the `_error` rich value structure) and its
  per-error sections (Spill `errorType` 8, Unknown 11, Calc 13)
