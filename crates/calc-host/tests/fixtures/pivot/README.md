# Pivot measurement fixtures

Excel captures for the pivot work (plan: `C:\Work\DnaCalc\codex-runs\pivot-plan.md`, bead dc-8wp).
One directory per measurement set. Each set has its own README with the question, the Excel build,
counts and where the raw outputs are kept (R2 prefix, not committed).

How a set is taken (Windows, owned hidden Excel on a private desktop, no owner Excel touched):

    pwsh tools/excel-session/on-private-desktop.ps1 -Command "py -3 tools/excel-session/run-pivot.py --out <dir> --session <dir> <kind[:scenario]>..." -Log <log> -GuardOut <json> -SessionRoot <session dir>
    py -3 tools/excel-session/pivot-fixtures.py cells|formats <result.json> <stage> <out.tsv>

`tools/excel-session/pivot-job.ps1` is the one job script (ported from the UI28 job). The UI28 set
under `../ui28/` was re-run from it on 2026-11-08 (Excel 16.0.20527): the writer produced
`pivots-refreshed.tsv` again with 0 differences over 669 cells. Raw run: R2
`dnacalc-evidence/excel/pivot-w0b-20261108/` (13 files, 2132490 bytes, 0 differences).

Sets: `m1-calculation/` (dc-8wp.9), `m2-formatting/` (dc-8wp.11), `../pivot_getpivotdata/` (dc-8wp.13) and
`../pivot_cube/` (dc-8wp.17); the last two are oracle workbooks (`fixtures/<case>/oracle.xlsx`) with their
captures under `tests/captures/<case>/`, so `cargo xtask corpus --oracle` counts their formula cells (not
calculated yet: file-cached, which is a failure in every statistic until the engine calculates them).
`pivot-job.ps1` kinds: `pivots` and `refresh` (UI28 cases), `roundtrip`, `m1`, `m2`, `m3`, `m4`.
