# P02 result

Excel 16.0 build 20527, CalculationVersion 191029, Windows x64/AMD Ryzen 7 4800H, safe mode.
43 conditions x 10 fresh workbooks, then eight core conditions x 10 in a fresh Excel process.
There are 430 final scenario workbooks and 30 saved external-provider seeds; the isolated run
replaced its eight scenario files. `matrix-saved-caches.json` retains the original matrix XML
and hashes; `saved-caches.json` describes the current files. All pure chain seeds were 2/3.

- Manual C1-only calculation kept B1=2/C1=3; F9 repaired them to 10/11. Including B1 yielded 11.
- Cross-sheet and cross-workbook C1-only and sheet calculation used boundary caches (C1=3).
  Application calculation yielded 11. No out-of-scope producer demand is needed for these cases.
- In all ten isolated Manual range trials, counter 32 advanced 2 -> 3 -> 4 for seed -> range -> F9.
  The clean unrelated counter 34 stayed 2. Counter 33 sometimes entered twice; exact counts vary.
  Decision: `RangePostState::StillRequired` for Manual range calculation.
- On the reversed pure chain, dependency-aware range calculation yielded A1=11; row-major yielded
  A1=3, repaired by F9. Keep a distinct row-major command.
- Automatic and Automatic Except Tables performed work during input edits before the explicit
  command. Keep those operations and mode-switch work in the trace; do not attribute all calls
  to the later explicit command. Disabled sheets remained stale even under Full in these trials.

The full matrix's ForceFullCalculation factors changed later counter behavior in the same
process. Those later counts do not prove clean-target behavior. `raw-matrix.tsv` preserves that
run; `raw-results.tsv` is the isolated core run with no ForceFullCalculation factors. The isolated
run's clean counter stayed unchanged through dirty edits and follow-up F9. Both supervisors
compiled/ran successfully, with zero probe faults and owned-process-zero cleanup.

Worksheet.Calculate with grouped sheets measures the explicit COM/VBA sheet command, not UI
Shift+F9. UI selected-sheet behavior and a separate cold-open process per condition remain open.
The raw archive contains exact injected modules, environment, runtime/event order and cleanup.
Reproduce using the shared run-calculation-probe.ps1; use -Cases to select the eight isolated cases.
