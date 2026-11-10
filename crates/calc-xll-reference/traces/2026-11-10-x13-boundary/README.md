# x13 boundary batch (dc-6332.10, answer 13 D.3, D.9 sentinel pass, D.22)

Fixture: the X1 Rust fixture with its opt-in probes (`crates/calc-xll-fixture/src/fixture/boundary.rs`,
marker `boundary-tests.enable`; `boundary-open.enable` adds xlGetInst inside xlAutoOpen). Cases and
Excel's answers: `crates/calc-xll-reference/fixture/x13-boundary-cases.json` (68) and
`x13-boundary-open-cases.json` (1). Runner: `run-batch.ps1` (one untraced Excel control through
`run.ps1` in the lane's owned session, then one OneCalc host process per case group, because the
host's case runner stops at its first failure). Each Excel job ran under
`tools/excel-session/on-private-desktop.ps1`; every guard file in `guards.json` records 0 windows
and 0 foreground hits on the owner's desktop.

Files:

- `excel/`, `excel-open/`, `excel-repeat/`: `controller.jsonl` and `environment.json` of the final
  Excel runs (run6, run6-open, run6-repeat; Excel 16.0.20527.20150, x64, en-ZA, MTR on, 16
  threads; fixture X1.xll SHA-256 2D1DD43E...). `excel-repeat` is the Excel-to-Excel control.
- `excel-crash-free-frame/`: the first run (run1), where Excel terminated (RPC server died) during
  `=MyFreeFrame()` while xlAutoFree12 called xlStack, xlGetName, xlfCaller and SUM. That fixture
  still registered the three permission forms on one export (see the hosting study, section 28),
  so its other rows are superseded; it is kept as the crash witness only.
- `verify-before.json`, `verify-after.json` (and `-open-`): `verify.py --boundary-host` of the
  final Excel answers against OneCalc before the dc-6332.10 host changes (the 85ea49e5 build) and
  after them, with the C.1 manifest, the four counts and the Excel repeat.
- `framework-regression.json`: the existing framework sets through `run.ps1 -HostExecutable`
  (values checked against Excel's recorded answers) on the old and new hosts.

Results: before 36 of 68 matched (fail 32: 20 mismatched, 11 refused, 1 not run); after 65 of 68
(fail 3, all refusals). Open-time xlGetInst: before 0 of 1, after 1 of 1. Excel repeat: 67 of 68
identical; the 16401 Int payload differs per process, so that case has no recorded value.

Every Excel job but one ended with "Baseline drift after reset": the X1 fixture's xlAutoOpen calls
`CALL("kernel32","lstrlenA","JC",...)`, which leaves `KERNEL32.DLL lstrlenA JC` in Excel's
registered functions; the session host keeps the outputs and recycles its Excel. One start after
the crash failed ("did not expose its worksheet native object model"); the next job got a fresh
Excel. Raw host run directories (with executable copies) stay outside the repository under
`C:/Work/DnaCalc/codex-runs/x13-boundary`.

## x14 operand batch

`x14/`: the operand batch (`fixture/x14-operand-cases.json`, 145 cases: xlCoerce masks, UTF-16
units, input mutation, nested xlGetName), Excel run and repeat (identical on all 145) and the
OneCalc comparisons before (31478370 build) and after the mask coercion: 61 and 128 of 145; the
17 remaining failures are OneCalc MissingInput refusals for cells other than A1. These host runs
used `run-batch.ps1 -Direct`. See the hosting study, section 29.
