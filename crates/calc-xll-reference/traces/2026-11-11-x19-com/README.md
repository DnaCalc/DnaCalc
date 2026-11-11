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

## Real early-bound and late-bound clients (D.15's PIA and dispatch routes)

`fixture/X19Com.csproj` + `ComProbe.cs` (Excel-DNA 1.9.0 net48, ExcelDna.Interop 15.0.1):
`COM_PIA()` reaches the object model through the primary interop assembly (the early-bound
`_Application` and `Range` vtables), `COM_DYNAMIC()` through `dynamic` (IDispatch); each returns
one string with the observations in order: Version, Application.Caller (shape of its Address, Row
agrees with it, Count), `Range("A1:B2")` Address/Count/Rows.Count/Columns.Count,
`Range("D5","B2")`, `Union` Address and Count, and a range on another sheet. `COM_PIA_VALUE2()`
reads `Range.Value2` (recorded only). Cases `fixture/x19-com-cases.json` (values are Excel's),
`x19-com-value2-cases.json`. Excel twice (`com-clients/excel`, `excel-repeat`): identical; the
host (`com-clients/host/host.stdout.json`): both strings equal Excel's, byte for byte, through
the PIA and through dispatch, including `BadSheet=HRESULT:800A03EC` (error 1004) in both.

The first host run of the PIA route crashed the process in `_Application.get_Caller`
(AccessViolation): `Caller` and `Union` have an `[lcid]` parameter before the return value in
Excel's type library, which the early-bound slots did not take (the C++ test had called them with
the same wrong shape). Corrected from the type library's parameter flags; the test now calls
slots 40, 50 and 74 with the lcid, and an early-bound bad address returns 0x800A03EC as Excel's
vtable does (there is no EXCEPINFO on that route). `Range.Value2` through the PIA: Excel returns a
2-D array (`excel-value2`); the host answers DISP_E_MEMBERNOTFOUND (`host/value2.stderr`), the
typed refusal the owner's ruling prescribes.

Also at this commit, on a clean tree against Excel captures of the same revision: the X1 traced
pair 289 of 289 formulas, 599 ordered callbacks with the same contexts, 148 xlFree operands
(`x1-traced-summary.json`); x13 65 of 68, x13-open 1 of 1, x14 128 of 145, x15 19 of 21, x17 28
of 28 (ordered engine run), framework host route 333 of 333, all as before.

