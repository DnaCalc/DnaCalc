# Object-model probes E6, E11, E1 (dc-1nbp.6, harvest 14 section 4)

Inputs for the Rust object model (`docs/architecture/OBJECT_MODEL.md`). Experiment definitions and
predictions: `docs/architecture/review-2026-11/pro/harvest/14-harvest.md` section 4.

## How it was run

- Client: the X19 Excel-DNA add-in, extended. `fixture/X19Com.csproj` now also compiles
  `fixture/OmProbe.cs`: three Excel-DNA commands `OM_E6`, `OM_E11`, `OM_E1`, each building its own
  workbook and writing one tab-separated line per observation (`case, route, what, value`) to
  `om-<probe>.tsv` beside the loaded XLL. Built with
  `dotnet build crates/calc-xll-reference/fixture/X19Com.csproj -o <dir>/build` (Excel-DNA 1.9.0, net48,
  ExcelDna.Interop 15.0.1 embedded).
- Routes per observation:
  - `pia`: the interop types (.NET marshalling);
  - `dynamic`: C# `dynamic` (IDispatch, .NET marshalling);
  - `disp`: `IDispatch::Invoke` called by hand, with the raw result VARIANT, HRESULT, EXCEPINFO and
    `GetErrorInfo`;
  - `vtable`: the dual-interface slot called by hand, with its slot and parameter layout read from
    the object's `ITypeInfo` at run time.
  
  The raw routes use lcid 1033.
- `om-probes-job.ps1` is the Excel session job (`tools/excel-session`): it copies the build into the
  run directory, records the environment and an out-of-process late-bound check (the PowerShell job
  is itself a COM client), registers the XLL and calls `Application.Run("OM_E6" | "OM_E11" | "OM_E1")`.
  `run-om-probes.ps1` submits it three times, each in a fresh owned Excel (`-FreshRequired`), and was
  run under `tools/excel-session/on-private-desktop.ps1` (desktop `DnaCalcOmProbes`). The session host
  marks each job "Baseline drift after reset; outputs retained, job refused". This is expected: the
  registered X19Com XLL stays in the add-in list. Each process was recycled, and the outputs were
  complete before the reset.
- `guard.json`: 3 owned Excel PIDs, 0 windows and 0 foreground hits on the owner's desktop. No
  dialog appeared, every job finished in about 2 s, and no Trust Center, macro-security or registry
  setting was touched. The owner's Excel and the Excel service were not touched.
- Excel 16.0.20527.20150 (build 20527, 64-bit), Windows NT 10.00. Country setting 27, `UseSystemSeparators`
  True, decimal ".", thousands " ", system currency symbol R, `AutomationSecurity` 1 (kept),
  baseline calculation manual.
- Raw evidence: `trial-1..3/om-e6.tsv`, `om-e11.tsv`, `om-e1.tsv`, `job-result.json` (environment,
  hashes, out-of-process checks), `session-receipt.json` (owned PID, command line, baselines). The
  three trials are byte-identical in all three tsv files.

## E6: early bound vs late bound, Value vs Value2

**Finding that reframes E6:** Excel's `Range` (and `Areas`) is a pure dispinterface in Excel's type library
(`Range` TKIND_DISPATCH, not dual). Its vtable twin `IRange` (IID 00020846-**0001**-0000-C000-000000000046)
exists in the type library, but a live Range answers `QueryInterface(IRange)` with E_NOINTERFACE
(0x80004002), both on in-process calls and after a deletion. The PIA declares `Range` as
`InterfaceIsIDispatch`, so an "early-bound" PIA call on a Range is an `IDispatch::Invoke`. Only the
dual interfaces (for example `_Application`, slot 40 `Range`) have a callable vtable. The route
difference therefore exists only on `_Application`-style dual interfaces.

Value and Value2 (raw `disp` VARIANT; `pia` and `dynamic` agree in every case):

| Cell | Value | Value2 |
|---|---|---|
| 42 | VT_R8 4045000000000000 | same |
| 2x2 {1,"x";TRUE,2.5} | VT_ARRAY\|VT_VARIANT, 2 dims [1..2][1..2], elements R8 / BSTR / BOOL -1 / R8 | same |
| empty | VT_EMPTY | VT_EMPTY |
| `=""` and constant `'` | VT_BSTR "" (len 0) | same |
| `=NA()` | VT_ERROR 800A07FA | same |
| 45292.25, `yyyy-mm-dd hh:mm` | **VT_DATE** 40E61D8800000000 (same bits) | VT_R8 same bits |
| 45292.25, `h:mm:ss` (time only) | VT_R8 | VT_R8 |
| -1, `yyyy-mm-dd` | VT_DATE -1 | VT_R8 |
| 3000000, `yyyy-mm-dd` | DISP_E_EXCEPTION, scode 8002000A (DISP_E_OVERFLOW) | VT_R8 |
| 123.45678901234567, Style "Currency" | **VT_CY** int64 1234568 (123.4568) | VT_R8 405EDD3C07FB4C98 |
| -0.00005, Style Currency | VT_CY -1 | VT_R8 |
| 922337203685477.5807 and 1e300, Style Currency | DISP_E_EXCEPTION scode 8002000A | VT_R8 |
| 45292.25, date format then Style Currency | VT_CY 452922500 | VT_R8 |
| 123.456..., `$#,##0.00`, `[$$-409]#,##0.00`, `[$R-1C09] #,##0.00`, accounting `_($* ...` | VT_R8 (not CY) | VT_R8 |
| A9:A10 (date over plain number) | array: (1,1) VT_DATE, (2,1) VT_R8 | both VT_R8 |

`$#,##0.00` set through `NumberFormat` reads back as `\$#\,##0.00`. In this locale (currency R)
"$" is a literal and the cell is not a currency cell. The built-in Currency style reads back as
`_($* #,##0.00_)...` with an unescaped "$" and gives VT_CY. So VT_CY follows the locale's
currency format, not a "$" in the text. Text of the 2x2 block: Null.

Optional arguments: `disp` (true omission = fewer cArgs) and `pia` agree in every row.

| Argument | `Range.Address(RowAbsolute)` | `Application.Range("A1", Cell2)` | `Range.Value(RangeValueDataType)` |
|---|---|---|---|
| omitted / VT_ERROR DISP_E_PARAMNOTFOUND | `$B$2:$C$3` | `$A$1` | VT_DATE |
| VT_EMPTY | `$B2:$C3` (taken as False) | 1004 (800A03EC) | VT_DATE (default) |
| VT_NULL | `$B$2:$C$3` (default) | 1004 | DISP_E_TYPEMISMATCH (80020005) |
| VT_ERROR 800A07FA | `$B$2:$C$3` (default) | 1004 | 80020005 |
| False | `$B2:$C3` | 1004 | 1004 |

Failing calls:

| Call | IDispatch with EXCEPINFO | IDispatch, EXCEPINFO null | vtable (dual) | pia / dynamic |
|---|---|---|---|---|
| `Application.Range("not a range")`, `("Elsewhere!A1")` | 80020009, scode 800A03EC, source/description/helpfile **null**, wCode 0 | 80020009 | 800A03EC, no IErrorInfo | COMException 800A03EC |
| `Range("A1").Offset(-1,0)` | 80020009, scode 800A03EC, no text | 80020009 | (no vtable) | 800A03EC |
| `Union(S!A1, T!A1)` | 80020009, scode 800A03EC, source "Microsoft Excel", description "Union method of Application class failed", helpfile xlmain11.chm | **800A03EC** directly | (not run) | pia 800A03EC; dynamic message "Unable to get the Union property of the Application class" |
| any member of a deleted Range | **800A01A8** directly, EXCEPINFO untouched (scode 0) | (same) | QI(IRange) E_NOINTERFACE | 800A01A8 |

Hypotheses: Value gives VT_DATE for date formats and VT_CY with 4 decimals for currency cells.
Both are **supported**, with two limits. The CY case needs the locale's currency format, and
overflow raises DISP_E_OVERFLOW. "Routes agree logically" is **supported** for Range: they are the
same IDispatch call. The null-EXCEPINFO outcome is **mixed**. Some calls still return
DISP_E_EXCEPTION; others return the bare `800A....` scode. Excel fills a description only for
some members, and the description text depends on the invoke flags.

## E11: Union area order and Count

| Union | Areas | Count (=CountLarge, Cells.Count) | Address | Rows.Count / Columns.Count | Row / Column |
|---|---|---|---|---|---|
| (B2:C3, F5, C3:D4) | 3 | **9** | `$B$2:$C$3,$F$5,$C$3:$D$4` | 2 / 2 | 2 / 2 |
| (C3:D4, F5, B2:C3) | 3 | 9 | `$C$3:$D$4,$F$5,$B$2:$C$3` | 2 / 2 | 3 / 3 |
| (F5, B2:C3, C3:D4) | 3 | 9 | `$F$5,$B$2:$C$3,$C$3:$D$4` | **1 / 1** | 5 / 6 |
| (B2:C3, B2:C3) | 1 | 4 | `$B$2:$C$3` | 2 / 2 | 2 / 2 |
| (B2:C3, B4:C5) | 1 | 8 | `$B$2:$C$5` (merged) | 4 / 2 | 2 / 2 |
| `Range("B2:C3,F5,C3:D4")` | same as the first Union | | | | |

- Areas keep argument order. For Each visits each area in turn, row by row: `B2 C2 B3 C3 F5 C3 D3 C4 D4`,
  so C3 is visited twice. Value2 of a multi-area range is the first area only; F5 first gives a
  scalar (here Empty). An identical area is dropped. An adjacent area of the same width is merged.
- Union across two sheets: 1004 (0x800A03EC), with the EXCEPINFO in the E6 table above.
  `Application.Range("S!A1,T!A1")` also gives 1004. Two ranges of another workbook unite. A range
  of this workbook and one of another give 1004.
- `Address` of the multi-area range: each area is formatted with the same arguments and joined with ",".
  `RowAbsolute:=False` gives `$B2:$C3,$F5,$C3:$D4`, `ColumnAbsolute:=False` gives `B$2:C$3,F$5,C$3:D$4`, and
  both False give `B2:C3,F5,C3:D4`. `xlR1C1` gives `R2C2:R3C3,R5C6,R3C3:R4C4`. R1C1 relative without
  RelativeTo is the same as RelativeTo A1: `R[1]C[1]:R[2]C[2],R[4]C[5],R[2]C[2]:R[3]C[3]`. RelativeTo C3
  gives `R[-1]C[-1]:RC,R[2]C[3],RC:R[1]C[1]`. RelativeTo is ignored under A1 and under absolute R1C1. A
  RelativeTo on another sheet, or the multi-cell C3:D4, acts as its top-left cell (C3). `External:=True`
  prefixes the workbook and sheet once, before the first area: `[Book5]S!$B$2:$C$3,$F$5,$C$3:$D$4`
  (also in R1C1).
- Assignment to `Range("B2:C3,F5:G6").Value2`:
  - a scalar, a 1x1 array or a 2x2 array is written to every area (the 2x2 array is repeated per area);
  - a 1x3 array writes B2:C3 = 1,2 / 1,2 and F5:G6 = 1,1 / 2,2. The two areas are filled differently.
- Hypotheses: **multiplicity (9) supported**, not 8 unique or the bounding box. Rows.Count is that of
  the first area (2, or 1 with F5 first), which confirms X19. Multi-area assignment writes all areas.

## E1: identity and held ranges

- `Is` of two `Application.Range("A2")` acquisitions is **False**. Both are alive at once and have
  different IUnknown and IDispatch pointers. The same holds for `Sheet.Range("A2")`, `Cells(2,1)`,
  `a.Offset(0,0)` and the `dynamic` route. The out-of-process PowerShell client also sees different
  IUnknowns (`ReferenceEquals` False). This supports the recollection, **not** DNA's interning:
  every acquisition is a new object.
- A Range held across `Rows(1).Insert` **tracks**: Address `$A$3`, Value2 20, Row 3. A fresh
  `Range("A2")` reads 10. `Columns(1).Insert` gives `$B$2`.
- Across a delete of its own row (`Rows(2).Delete`), every access raises **424, 0x800A01A8**
  ("Object required"). This covers Address, Value2, Row and Worksheet. IDispatch returns 0x800A01A8 as
  the HRESULT without filling EXCEPINFO. A held A3 below the deleted row tracks to `$A$2` / 30. A held
  A2:A3 shrinks to `$A$2` / 30.
- Across a sheet rename (S to S2), the Range keeps working: `[Book7]S2!$A$2`, 20. Across a sheet
  delete (with DisplayAlerts False, restored), every access raises 0x800A01A8.
- Hypotheses: **tracking** for inserts, and **invalidated with error 424** for deletion. DNA's
  UDF route says "Deleted", and its event route says 91. Neither matches: Excel says 424.

## Not run here

These parts of harvest 14 E1 were not run: close and reopen, Undo, sheet reorder, and recreating
a deleted sheet. Also not run: a VBA-project observer, which needs VBProject access, a Trust Center
setting. E11's enumeration-under-mutation part was not run. A first attempt (scratch
`codex-runs/om-probes/session-attempt1`, not committed) had E1 stop at `Application.Calculation`
with no workbook open. It also had no locale-currency cells. Both are fixed in this version.
