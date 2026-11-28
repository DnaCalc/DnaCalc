# Excel C API command results

The AF-6 C2 fixture called one Excel12 command per row from a registered XLL
macro. Excel 16.0.20625.20108, 64-bit, on AMD Ryzen 7 4800H, lane ww17.
The source rows include the callback return code, the result operand type
and payload, and the before and after worksheet and application state.

Of 57 callbacks, 54 returned xlretSuccess with Boolean TRUE. Sheet-qualified
SELECT returned xlretFailed (32) with error operand 15. The authored VBA
Sub set B1 to 1701 and RUN returned xlretSuccess with a missing (256) result.
RUN of the authored Excel 4.0 macro-sheet reference returned xlretSuccess
with error operand 42.

Omitted ECHO toggled screen updating in both directions. Omitted CLEAR
cleared cell contents and kept the format and note. FORMULA entered the
relative formula at the upper-left cell of the supplied range or selection.
FORMULA.FILL entered it at both rows. The raw rows show where INSERT and
EDIT.DELETE moved values and notes for each tested shape and shift number.

The session ran on an owned private desktop. The guard found zero windows
and zero focus events on the user desktop. The Excel session receipt flagged
that the XLL and its registered functions were still present when resetting
that fresh session. The host then shut down the owned Excel process. The
retained callbacks were captured before the reset.

The fixture is calc-xll-fixture, built for x86_64-pc-windows-msvc and copied
as an XLL. SHA-256 of the captured fixture was
6e747ab3cf8cf3274e519da3f5457f213df41f10c32b74d9e5df001aa2a53ebc.
Place command-probe.enable beside the XLL to register MyCommandProbe.
For each invocation, write command-probe-request.txt in UTF-8 with the
decimal ordinal on its first line and optional tab-separated operand type
and operand value on the remaining lines. Invoke MyCommandProbe as an Excel
macro. Read command-probe-result.txt for the actual Excel return code, type
and payload. Use a fresh workbook with the cell state in the trace.

## Windows follow-up, lane ww18

`ww18-followups.jsonl` records 90 callbacks on the same Excel build.
The fixture also accepts `miss` with an empty payload and `i2` with a
signed 16-bit decimal payload. The recorded command return includes the
Excel12 return code and result operand separately from worksheet changes.

73 callbacks returned xlretSuccess with Boolean TRUE. `RUN` with a second
operand returned xlretSuccess with error operand 15 and left the sheet
unchanged. 13 callbacks returned xlretFailed and three returned
xlretInvCount. `FORMULA` with integer 23 and range `R1C1:R1C2` wrote only
A1. `WORKBOOK.NAME` renamed the quoted-name control using two strings,
returned TRUE with one string, and failed on the duplicate name.

Windows CalcHost replayed 89 cases through the command dispatcher. Of
those, 58 differed in the result or in a recorded field whose initial
value agreed. The `RUN` VBA case needs the VBA macro in the comparison
fixture. The native Excel12 ingress remains with dc-17spk.16 and
dc-17spk.17. The raw snapshots and the eight direct StatusBar probes are
in `C:/Work/DnaCalc/codex-runs/ww18/xlc2/`.

`ww18-execute-excel4-macro.json` adds 16 C4 cases from direct IDispatch calls.
Both fresh sessions agreed on the result types, values, errors and sheet
effects. `1+2` returned a double 3, `1/0` returned a Variant error, and
`=1+2` raised a dispatch exception. SELECT, FORMULA, FORMULA.FILL, MESSAGE
and ECHO returned Boolean TRUE. The CalcHost object model refused all 16
calls because `_Application.ExecuteExcel4Macro` has no handler.

## Windows file context, lane ww27

The same probe accepts `function <number>` on the first line for Excel12
functions 132–139, 166, 186 and 188. Its result appends the full XLOPER12
type word to the status, base type and payload. Strings are counted UTF-16
in hex. Array payloads include dimensions and each element's type and value.
The existing decimal command request and its three-column response still work.

[ww27-file-context.json](ww27-file-context.json) records 43 calls on Excel
16.0.20625.20108. GET.DOCUMENT(2) returned an Excel error in the new workbook
and a path after SaveAs. FILES returned an array for the scratch file and an
Excel error for the unmatched pattern. FOPEN, FREAD, FREADLN, FSIZE, FPOS and
FCLOSE returned results on the scratch input. FWRITE and FWRITELN succeeded
on existing files opened with modes 1 and 3. The source capture includes the
exact paths and complete GET.WORKSPACE(44) registration table.
