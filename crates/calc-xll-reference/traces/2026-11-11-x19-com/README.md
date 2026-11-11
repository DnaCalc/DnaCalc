# x19 COM0/COM2 for OneCalc: Application.Range, Union, Caller (dc-6332.12, answer 13 B.9, D.15 first half)

The three page-acceptance failures of the generated calcvba-udf add-in (`Native_ReferenceKind(A1)`
"Range:1", `Native_OptionalReference(A1)` "Range", `Native_CallerRow()` 3). The add-in is VBA
compiled to native code; it reaches Excel as the original Excel-DNA and xlOil add-ins do:
`xlGetHwnd`, `FindWindowEx` XLDESK/EXCEL7, `AccessibleObjectFromWindow(OBJID_NATIVEOM)`, then
**late-bound** calls: `app.Range("'[Book1]X0'!$A$1:$A$1")`, `app.Union`, `Application.Caller.Row`,
`TypeName(value)` and `value.Rows.Count`. Discovery already worked; what failed was the object
model behind it. Excel's answers are the recorded ones (`codex-runs/m4/artifacts`, Excel
16.0.20527.20150).

What changed (inside the owner's ruling, ONECALC_ADDINS.md: no full object model, `Range.Value`
reads are typed refusals): `Application.Range` (DISPID 0xc5), `Union` (0x30b), `Caller` (0x13d),
and early-bound slots 40, 50 and 74 of `_Application` (DISPIDs and slots read from Excel's type
library with `inspect-com.ps1`, which registers nothing). A Range names cells and holds no
values. `Caller` is `calc_xll_host::running_caller`, the cell of the native call on the thread.

Measured, first run (a host with the three members but no type information; the host's case
runner stops at its first failure): `Native_ReferenceKind(A1)` printed
`calculated "IDispatch::<invoke-result>:1"` against Excel's "Range:1":
VBA's `TypeName` of a returned late-bound object reads the name from `IDispatch::GetTypeInfo(0)` and
falls back to the binding's label. A Range now answers a private in-memory dispinterface named
`Range` (CreateTypeLib2, nothing saved or registered). After it:

- the 20 calcvba-udf cases through the host route (`run.ps1 -HostExecutable`, values checked
  against Excel's answers): 20 of 20 (`calcvba-udf-host.stdout.json`);
- page acceptance (`xtask app xll-accept onecalc`, bundle and exe built at the commit that carries
  this file, Excel answers from `codex-runs/x13-boundary/m4-excel-all`, run on a private desktop):
  353 of 353 (`summary.json`; 350 before): X1 289, Excel-DNA 7 + 7 + 1 + 2, xlw 5 + 5, xlOil 12 + 5,
  calcvba-udf 20. `guard.json`: 0 windows and 0 foreground hits on the owner's desktop.
- the native test `application_range_union_and_caller_answer_range_metadata` drives the real
  dispatch (Range, bounding Range(Cell1, Cell2), Union, Address/Count/Rows/Columns/Row/Column,
  error 1004 with EXCEPINFO for another sheet or a bad address, `Caller` with and without a
  cell, type information named Range) and the three early-bound slots.

Not done, on purpose: `Range.Value`, `Value2`, `Cells`, `Item`, `Areas`, `Application.Intersect`,
`ActiveCell`, commands and `Range.Value2` writes (the answer's suggestions conflict with
ONECALC_ADDINS.md lines 51, 122 and 156); CSE, spill and name callers for `Caller`; the no-Office
and second-apartment lanes (N5).
