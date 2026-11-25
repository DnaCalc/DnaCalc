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
