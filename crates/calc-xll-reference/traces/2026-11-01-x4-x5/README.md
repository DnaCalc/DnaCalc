# X4/X5 observations, 2026-11-01

These are measured cases, including failures. Excel16.0.20527.20088 x64 and the
desktop application run identical add-in files. Traced captures use the existing
X0 debugger method; independent controls run without the debugger. Environment
records retain source, executable, shim, tracer, fixture and request hashes.
Source-publication bindings are checked against actual Git objects with LF/CRLF
normalization only. They do not rewrite capture metadata.
The final runner/verifier source is published as cf6a9b68.

| Measurement | Result | Remaining difference |
|---|---|---|
| Original xlOil Python X4 |12 formula results agree;13 native invocations agree on defined operands/callbacks | Strict event comparison fails at positions2,5,6 |
| Numeric RTD |2 formula/dependent cases agree in four captures; six client protocol calls and payloads agree | Host has an initial temporary #N/A before P07 input admission |
| Callback permissions |37 matched,2 failed out of39; pass rate94.87% | General EVALUATE is refused in normal and macro-equivalent calls |
| Existing corpus |18812 matched,1218 nonpasses out of20030 cells | All42 complete report/evaluation files across21 workbooks are byte-identical |

X4 retains all13 SheetChange observations per traced host. Arguments and event
counts agree, but a Worksheet.Name call and Range.Address calls differ. Its verification.json
is a failed verdict and its verifier exits1. The asynchronous startup import is
acknowledged by a delivered fixture AfterCalculate callback before edits begin.
Original framework binaries remain unchanged; only the repository fixture script
is custom. Safe reload still fails in isolated Excel and remains dc-snz.21.
PyCall/PyQueued raw-equality flags remain false because undefined xlFree result
operands differ; those raw observations are retained.

Numeric RTD compares ServerStart, ConnectData, UpdateNotify, RefreshData,
DisconnectData and ServerTerminate. Excel uses IDispatch; the host calls the
measured early-bound interface. Only proven fixture-internal method bodies are
excluded from the external protocol, and those observations remain in the report.
The host admits values1 and42 through CalcHost.update_rtd, retains the exact
sequence2 input certificate, and retires the subscription after replacement.
It does not complete a parked native async call. Empty signaling-cache values,
arrays, remote servers and arbitrary provider teardown remain unverified or
refused. A prior xlOil signaling-cache numeric coincidence had no RTD read and
was a failed qualification, never a pass.
A fresh original xlOil run in rtd/xloil-refusal exits1 with
`dc-snz.23: RTD refresh: 0x80070057`; that diagnostic is retained without
claiming a more specific cause.

Permissions compare13 callback return codes under each normal, # and $ form.
The verifier checks actual Excel signatures, arguments and owner/pool threads;
each host request uses an independent application process and logs its actual
callback code and thread. It checks equal Excel fixture/build and host executable/
shim hashes. This does not compare every discarded callback payload. The2 failed
EVALUATE reads remain present with exit1 diagnostics. MESSAGE checks logical
state, not UI painting. See permissions/verification.json for every request.

Native engine tests retain one-activation async completion, overwrite revocation,
late return256 and two-workbook pool overlap with owner-serviced reads. A single
workbook still serializes suspended cells. These tests use the application route;
they do not constitute an Excel trace for every concurrency/cancellation case.
Validation logs retain counts and diagnostics. A metadata-only check selects no
package tests. No full sweep or Linux run was performed for this session.

Replay with the existing verify.py and this source-publication.json:

* X4: x4/excel-traced x4/excel-control --host x4/host-traced --host-control
  x4/host-control --x4-cases ../../fixture/x4-xloil-python-cases.json (expected exit1).
* RTD: rtd/excel-traced rtd/excel-control --host rtd/host-traced --host-control
  rtd/host-control --rtd-cases ../../fixture/x5-rtd-native-cases.json.
* Permissions: permissions/excel-traced permissions/excel-control
  --permission-host permissions/host --permission-cases ../../fixture/x5-permission-cases.json.

Run from this directory, invoking ../../verify.py and supplying
--source-publication source-publication.json. sha256.json covers retained files.
Binary inputs remain outside repositories in codex-runs/x3-upstream and
x5-upstream. Full raw runs and earlier failed attempts remain in
C:/Work/DnaCalc/codex-runs/x4-runs and x5-runs. R14 variance reads are separate
external research results and are not part of these compatibility counts.

Only owned Excel processes were stopped. Excel COM registrations, HKLM,
Trust Center and macro settings were unchanged. The controlled native test
server creates unique private HKCU DnaCalc.X5 classes and removes its own keys.
dc-snz.21, dc-snz.22 and dc-snz.23 remain open on the limits above.
