# Excel probe: a formula that reads a cell not yet calculated

Measured 2026-11-09 for `docs/architecture/EVALUATOR_SUSPENSION.md`. Excel 16.0.20527 x64
(CalculationVersion 191029), multi-threaded calculation on with 16 threads (the instance's own
setting, not changed), AutomationSecurity 1. Owned hidden Excel instances from
`tools/excel-session`, each session started through `on-private-desktop.ps1`; every guard file
recorded 0 windows and 0 foreground hits on the owner's desktop. No Excel, Trust Center or macro
setting was changed.

Method: `suspension-job.ps1` (the third round's version) adds the module `Probe.bas` to a new
workbook through the VBE object model, sets Manual calculation, writes C1 = 5, enters each
case's formulas with `Formula2` in the listed order, then runs three stages, reading the UDF's
call log after each: s1 C1 := 6 and `Application.Calculate` (op `calculate`) or `CalculateFull`
(op `full`); s2 C1 := 7 and Calculate; s3 a clean Calculate. Ten workbooks per case for
`calculate`, five for `full`.

| Round | Cases | Workbooks | Result file |
|---|---|---|---|
| 1 | 12: one-level producer B1 `=C1*2`, reader entered first or last | 180 | `raw-round1.json.gz`, `summary-round1.txt` |
| 2 | 15: the same through INDIRECT/OFFSET, RAND, NOW, a text-selected target; `chain_rev` a three-level chain | 225 | `raw-round2.json.gz`, `summary-round2.txt` |
| 3 | 14: a three-level chain B1..B3 (and B1..B8, two chains) | 210 | `raw-round3.json.gz`, `summary-round3.txt` |

Each raw row holds the case's formulas, so rounds 1 and 2 are reproducible from their rows with
this job's runner. `summary-*.txt` groups identical call sequences (`summarize.py`); for the RAND
and NOW cases of round 1 the logged numbers are replaced by `v` there, and kept in the raw rows.

Outcome: only the three-level and deeper chains (round 2's `chain_rev`, every round 3 case)
abandoned formulas, in all trials, both ops, first calculation only; the one-level cases entered
every UDF once. INDIRECT and OFFSET cases entered their UDFs again on the clean
Calculate (volatile); static references did not.
