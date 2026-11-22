# Excel and VBA tips for capture

Each directory in `formulas/` or `vba/` holds one example and its `manifest.json`.
Formula examples use openpyxl workbooks with synthetic inputs on `Sheet1`. The manifest
records the inputs, the Excel formula spelling, and the spelling stored in the workbook.
The workbook marks Formula2 anchors with Excel's dynamic-array package metadata.

VBA examples are importable standard modules. Each exposes
`Public Function DcExample() As Variant`. Some return a one-dimensional Variant
array. Import one module into a fresh workbook for each run. Cases with
`requires_blank_worksheet` use the first sheet of that workbook as scratch space.
The modules passed static triage on kode-fi.

## Licences

Original workbook construction, synthetic inputs and original drivers use the repository's
MIT licence. Each manifest identifies the source, the licence and how the material was
used. Exceljet links attribute the underlying behaviour. The workbook contents for those
cases were authored for this corpus.

Stack Exchange formula adaptations carry the applicable CC BY-SA version shown in
the manifest. Each manifest records the answer, author, licence link, snippet identifier,
and changes. Microsoft VBA reference examples identify the page and pinned commit.
The code licence from that revision is included in [LICENSE-Microsoft-VBA.txt](LICENSE-Microsoft-VBA.txt).

## Excel capture

The [capture manifest](capture-manifest.tsv) gives paths relative to this directory.
The coordinator uses the Windows Excel service to recalculate each formula workbook
and record the specified cells. A location ending in `#` requests all values in the
spill with their shape. If the anchor returns an error or has no spill, record the
anchor result. For a VBA row, record the typed return value of `DcExample`. Preserve
the order of array elements, and record an execution error separately if a driver fails.

The `expected_answer` field is filled from the Excel capture. Formula cases point
to a sibling `oracle.capture.json` and map each declared target to the live value.
VBA cases retain the `Application.Run` return with its VBA subtype and array
bounds. Both identify the source bytes and Excel build.

The Windows run captured 81 of 93 formula workbooks and all 89 VBA drivers.
Excel rejected the other 12 authored workbook packages at open. The comparison
keeps their formula cells in the denominator. See [the capture summary](excel-capture-summary.json).

`cargo xtask corpus --oracle --no-capture` includes the formula examples. The
Formula Explorer VBA corpus reads the captured drivers through its module route.
The interpreter matched 64/89 and the JIT matched 62/89 on both Windows and Linux.
The VBA manifests preserve Excel's original types and array bounds.
