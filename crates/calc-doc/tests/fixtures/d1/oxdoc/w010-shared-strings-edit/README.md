# W010-3 Shared Strings Edit Fixture

Status: checked-in W010 (Measured Excel) conformance fixture.

This fixture stores `.xlsx` package parts as readable XML files under `parts/`.
The conformance tests zip those parts in memory before passing them to
`oxdoc-xlsx`.

The parts follow the shape Excel writes (`mc:Ignorable`, `xr:uid`, `<row spans>`,
`x14ac:dyDescent`, `standalone="yes"`, single-line XML, workbook relationships in
Excel's reverse order) so the shared-string append is exercised against the
byte layout of a real Excel-authored `sharedStrings.xml`.

Coverage (`xl/sharedStrings.xml`, `count="4" uniqueCount="4"`):

- `<si>` 0: plain text, `<t>plain</t>`
- `<si>` 1: rich text with two `<r>` runs carrying `<rPr>` formatting, the
  second run's `<t>` marked `xml:space="preserve"` (`Bold` + ` tail`)
- `<si>` 2: `漢字` with a phonetic `<rPh>` hint (`カンジ`) and `<phoneticPr>`;
  the hint is not part of the string value
- `<si>` 3: a leading-space string, `<t xml:space="preserve"> leading space</t>`

Coverage (`xl/worksheets/sheet1.xml`):

- `A1`..`D1`: `t="s"` cells pointing at `<si>` 0..3 (`C1` also styled, `s="2"`)
- `E1`: an unstyled number, `<c r="E1"><v>12</v></c>`
- `F1`: an inline string, `<c r="F1" t="inlineStr"><is><t>inline</t></is></c>`

The tests edit `A1` (a text cell that is none of the three special entries)
and assert the other four `<si>` are byte-identical with one plain `<si>`
appended and the counts shifted; edit `B1` (the rich-text cell) to a plain value
and assert its rich `<si>` stays in place while the cell points at a new plain
`<si>`; and give `E1` a text the table already carries and assert the table is
not written at all. `numbers_only_xlsx()` in the test module derives the
no-`sharedStrings.xml` shape (part, override, and relationship removed) for the
create lane.

Reference anchors:

- `docs/FORMAT_REFERENCES.md`
- ISO/IEC 29500-1 §18.4 Shared String Table (`sst`, `si`, `r`, `rPh`,
  `phoneticPr`, `t`, `count`, `uniqueCount`)
- ISO/IEC 29500-1 §18.4.12 `t` (`xml:space="preserve"`)
- ECMA-376 Part 2 (OPC) content types and relationships for the created-part lane
