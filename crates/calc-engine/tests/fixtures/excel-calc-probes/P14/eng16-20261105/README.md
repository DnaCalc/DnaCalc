# Eng16 iterative observations

Excel 16.0.20527, 2026-11-05: 16 conditions, ten fresh owned processes each.
Manual calculation, iteration enabled, MTC disabled and events disabled. The
full delivery remains at `~/codex-runs/excel-results-eng16`; all 2,634 delivered
file hashes were checked. `delivery-audit.json` retains each trial's input and
result hash and the original delivery/specification hashes.

`inputs/` contains the exact trial-one packages. `observations.jsonl` retains
the original command cells and complete UDF events from one trial per condition;
all ten trials have identical command/event objects and no startup UDF entries.
Seeds were verified before arming, and arming did not change any cell. Range
returns System.DBNull; Full and Rebuild return null. Neither is a Boolean success
assertion. `sha256.json` records the retained package and observation bytes.

The host replay registers callbacks with the recorded PacketStep and
PacketPairVariant bodies to isolate scheduling and pinned reads through the
ordinary callable/evaluator route. It retains the input formulas and caches;
it does not execute the original project's setup or claim full file-VBA parity.
Held producer values remain unmet requirements and never count as calculated
matches. No source workbook or Excel answer is rewritten.
