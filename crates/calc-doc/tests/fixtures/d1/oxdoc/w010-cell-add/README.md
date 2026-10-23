# W010-5 Cell Add Fixture

Status: checked-in W010 (Measured Excel) conformance fixture (`oxdoc-1vg.16`).

This fixture stores `.xlsx` package parts as readable XML files under `parts/`.
The conformance tests zip those parts in memory before passing them to
`oxdoc-xlsx`.

The parts follow the shape Excel writes (`mc:Ignorable`, `xr:uid`, `<dimension>`,
`<row spans>`, `x14ac:dyDescent`, `standalone="yes"`, single-line XML) so the
cell insertion is exercised against the attribute layout of a real
Excel-authored sheet, including a `calcChain.xml` the formula insertion has to
invalidate.

Coverage (`xl/worksheets/sheet1.xml`, `<dimension ref="A1:C4"/>`, every row
`spans="1:3"`):

- row 1: `A1` numeric, `C1` formula `A1*2` cached `2`; `B1` is a gap, so a
  cell inserted there must land between `A1` and `C1`
- row 2: `A2` styled empty (`<c r="A2" s="1"/>`, the `oxdoc-znf` shape: not a
  cell in the event stream, but an element in the source), `B2` shared string
- row 3: absent, so a cell inserted there creates a `<row r="3">` between
  rows 2 and 4
- row 4: `<row r="4" ... ht="30" customHeight="1" .../>`, a self-closing row
  carrying only properties, which an inserted cell has to open
- rows 5 and beyond: absent and outside the dimension, so an inserted cell
  grows `<dimension ref>`

`xl/sharedStrings.xml` carries one entry so a text insert appends through the
W010-3 rule. `xl/styles.xml` carries two `cellXfs` entries (0-1).

Reference anchors:

- `docs/FORMAT_REFERENCES.md`
- ECMA-376 Part 1 / SpreadsheetML `CT_SheetData`, `CT_Row` (`r`, `spans`,
  `ht`, `customHeight`), `CT_Cell` (`r`, `s`, `t`), `CT_SheetDimension`
