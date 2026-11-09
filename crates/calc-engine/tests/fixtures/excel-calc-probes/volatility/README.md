# Excel probes: volatility

Measured 2026-11-09/10 (lane vol-1, bead dc-hsa5.21) for `docs/architecture/VOLATILITY.md`. Excel 16.0 build
20527 x64 (CalculationVersion 191029), Manual calculation, multi-threaded calculation on with 16 threads (the
instance's own setting), AutomationSecurity 1. Owned hidden Excel sessions from `tools/excel-session`, each job
run through `on-private-desktop.ps1`; every guard file recorded 0 windows and 0 foreground hits on the owner's
desktop. No Excel, Trust Center or macro setting was changed (`CalculateBeforeSave` is switched off inside a
trial and restored before the book closes).

## Method

- `volatility-job.ps1` runs a spec (`specs/*.json`, written by `make-specs.py`) in one owned session. Each trial
  is a new workbook with `Probe.bas` (VBA test functions), first saved as `.xlsb` so that `SaveCopyAs` keeps that
  format; constants, names and formulas (`Formula2`) are entered; steps are Calculate kinds, edits, name
  redefinitions, saves and reopen. Every calculation step records the VBA call counts (`name@caller`), the XLL
  log (`name#call@row,col` and the Excel12 status of each `xlfVolatile` call) and the watched values.
- Instrument, layer 1: **dependent counters**. Formula `A<i>` has `B<i> = Cnt(A<i>&"")`; `B<i>` is called in a
  clean Calculate exactly when `A<i>` was recalculated. (A bare `=Cnt(A<i>)` is not recalculated by a volatile
  precedent, so the concatenation form is used; see `results/instr.txt`.)
- Instrument, layer 2: **file readout** (`readout.py`). After a completed Calculate the book is saved and, per
  formula cell, the leading `PtgAttrSemi` token (`tok`: the syntactic volatile token) and the always-calculate
  bit (`ca`) are read. XLSB flag bit 1 of the cell record (bit 0 of the BrtArrFmla flag byte for array-stored
  formulas) is the same bit as `ca` in XLSX; measured in `cal1` (12 formulas, 3 trials, also saved as XLSX).
- **Calibration** (`results/cal1.txt`): 12 formulas of known behaviour (RAND, NOW, TODAY, OFFSET, INDIRECT,
  `IF(TRUE,1,RAND())`, plain sums, VBA functions that do and do not request) over 3 trials and 3 clean
  Calculates: the counters and `ca` agree on all 12; the token is present exactly on the built-in volatile ones.
- `Probe.bas` and the XLL fixture (`crates/calc-xll-fixture/src/fixture/volatility.rs`, opt-in with a
  `volatility-tests.enable` file beside the module) share one script language: `T`/`F` request
  true/false, `N` requests with no argument, `|` separates one part per call, `*` cycles.
- Three trials per reading (`x4` and `p5` also as separate repeat runs: Excel died after the first trial of
  some multi-trial runs, error rows are kept in the raw file).

## Files

- `specs/<name>.json` the inputs; `results/<name>.json.gz` the raw rows; `results/<name>-readout.json.gz` the file
  readout of every saved workbook; `results/<name>.txt` the table from `summarize.py`;
  `results/<name>-records.txt` per-calculation records (`records.py`) for the stateful probes.
- `collect.py` builds `results/` from a raw run directory; `cells.py` prints one saved workbook's readout.
- Run: `DNACALC_EXCEL_SESSION=<dir> pwsh tools/excel-session/on-private-desktop.ps1 -Command "& volatility-job.ps1
  -SpecFile specs/p2.json -OutDirectory <out>" -Log <log> -GuardOut <json> -SessionRoot <dir>`.

| Spec | Probe |
|---|---|
| cal1, instr | instrument calibration; counter form |
| p1a, p1b, p1ca..p1cf, p1d, p1e, p1f | P1 built-ins: list, branches/LET/LAMBDA, CELL/INFO, SUMIF family, spills, per-evaluation shape |
| p2, p2x, p2h | P2 one flag or two: VBA, ordinary XLL, `#` XLL |
| p3 | P3 `!` registration |
| p4 | P4 `#` and argument types |
| p5, p6 | P5 names, untaken code, redefinition; run-time decisions against requests |
| x1 .. x7, x2b | X1 persistence, X2 lifted/repeated, X3 threads/async, X4 abandoned attempt, X5 outside a cell, X6 file round trip, X7 dependents |
| scan, scan2 | search for the function number of VOLATILE and its status by call context |
