# Excel sheet painting comparison

Run `cargo xtask app paint-compare calc --out C:/Work/DnaCalc/codex-runs/paint-captures/before`.
It builds the browser bundle, generates authored workbooks through the existing smoke fixture
generator, captures all four inputs at 100/75/150% in one owned visible Excel, closes that process,
and captures DOM and Canvas in Chrome through the existing WebDriver and workbook upload route.
Python with Pillow, ChromeDriver and installed Excel are required. Nothing is installed by the command.

Use `--bundle DIR` to reuse a built bundle, `--workbook FILE` for an existing workbook, and
`--zooms 100` for a smaller measurement. Only the top-left 18 rows and 10 columns are requested.
Use `--excel EXE` for an installed Excel outside the standard Office folders.
Every visible worksheet is captured. Hidden or partly clipped cells are explicitly not compared.
The generated inputs cover widths, heights, fonts, effects, alignment, wrapping, indent, rotation,
shrink, number formats, hashes, overflow, clipping, border styles and joins, fills, conditional
formatting, headers, selection and unscrolled freeze panes. No new merged cells are authored;
an existing workbook can supply that case. No scrolled-freeze or general style coverage is claimed.

For a changed painter, run the same command with a new `--out` and
`--reference C:/Work/DnaCalc/codex-runs/paint-captures/before`. This starts no Excel. References
must match the exact workbook hash and zoom. The browser's device scale matches Excel's window DPI.
`comparison.json` records translation-aligned sheet differences and per-cell raster differences
over the common extent, beside each cell's position/size delta and all cells not compared.
Exact differences and differences above 16/64 channel levels are separate; no tolerance is a pass.
Missing dark text ink is counted separately. A lower difference percentage alone is not an
improvement: erasing displaced glyphs can reduce that percentage while losing the text.
Dark-ink bounds use a luminance threshold; borders/fills can also register, so this diagnostic
does not prove that the complete text remains. The full pixel differences remain available.
The images are never stretched or recoloured. Headers and selection remain in original screenshots;
sheet difference images begin at A1. Source revision, source diff hash and bundle hashes are retained.
Screen-point coordinates are checked against gridlines in the original Excel raster; the frozen
pane origin is located from its retained headers. Calibration failure stops the comparison.
The workspace revision at capture is separate from the bundle's source: a reused bundle has
unknown source unless separately recorded, while a bundle built by this command checks that
tracked runtime source stayed unchanged during its build. Exact served asset hashes are always retained.

Excel starts explicitly with `/x` and an authored blank workbook; its PID/start time are recorded
before waiting for its native COM object. Excel capture uses PrintWindow; no foreground or
screen-copy fallback exists. Window operations
take `C:/Work/DnaCalc/codex-runs/desktop.lock` exclusively inside 23:30–06:55 machine time. A lease
watchdog terminates the owned Excel and worker and releases the lock before two minutes. A lock
older than five minutes is stale. PID 3144, existing PIDs and `/restore` instances are refused.
`excel-session.json` records the launch count and cleanup. Other Excel sessions are untouched.

Desktop comparison is explicitly not run: `dna-calc-desktop` currently supplies an XLL/COM host
window without a worksheet painter. Chrome pixels do not establish desktop equivalence.
