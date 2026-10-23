# W010-2 Styled Cell Edit Fixture

Status: checked-in W010 (Measured Excel) conformance fixture.

This fixture stores `.xlsx` package parts as readable XML files under `parts/`.
The conformance tests zip those parts in memory before passing them to
`oxdoc-xlsx`.

The parts follow the shape Excel writes (`mc:Ignorable`, `xr:uid`, `<row spans>`,
`x14ac:dyDescent`, `standalone="yes"`, single-line XML) so the surgical value
edit is exercised against the attribute layout of a real Excel-authored sheet.

Coverage (`xl/worksheets/sheet1.xml`):

- `A1`: styled numeric cell, `<c r="A1" s="1"><v>12</v></c>`
- `B1`: styled shared-string cell, `<c r="B1" s="2" t="s"><v>0</v></c>`
- `C1`: styled boolean cell, `<c r="C1" s="1" t="b"><v>1</v></c>`
- `D1`: styled error cell, `<c r="D1" s="3" t="e"><v>#DIV/0!</v></c>`
- `E1`: styled dynamic-array formula cell carrying `cm="1"`
  (`xl/metadata.xml` holds the matching `XLDAPR` record). Since `oxdoc-li2` a
  cached-value or text edit that leaves it the anchor of the same
  `t="array" ref="E1"` formula keeps `cm="1"` byte-exact and leaves
  `xl/metadata.xml` untouched; replacing the formula with a plain value (the
  shape where Excel clears `cm=`) still refuses (`FormulaRemove`), never
  keeping or dropping `cm=` silently
- `F1`: unstyled numeric cell
- `G1`: styled empty cell, `<c r="G1" s="2"/>`
- `A2`: second styled numeric cell on its own row

`xl/sharedStrings.xml` carries two entries so a string cell can be pointed at a
different existing entry without touching the table. `xl/styles.xml` carries
four `cellXfs` entries (0-3).

Reference anchors:

- `docs/FORMAT_REFERENCES.md`
- ECMA-376 Part 1 / SpreadsheetML `CT_Cell` (`r`, `s`, `t`, `cm`, `vm`, `ph`)
- ECMA-376 Part 1 / SpreadsheetML cell metadata (`xl/metadata.xml`)
