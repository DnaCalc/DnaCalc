# Excel sheet painting comparison

`cargo xtask app paint-compare calc --out DIR` builds the app, generates the four authored
inputs through the existing fixture writer, captures Excel at 100/75/150%, closes its owned
process, and captures both DOM and Canvas in headless Chrome through the app's upload route.
Use `--bundle DIR`, `--workbook FILE`, `--zooms 100` or `--excel EXE` to select existing inputs.
`--normal-font "Arial,10"` changes Normal in generated inputs; imported workbooks keep their font.
Python with Pillow and NumPy, ChromeDriver and installed Excel are required. Nothing is installed.
`DNACALC_PAINT_PYTHON` selects an installed interpreter; the default is `python` on PATH.
No workbook with merged cells is authored. An existing workbook can supply merged-cell input.

`--reference DIR` reuses Excel images without launching Excel. Workbook hashes, sheets, zooms
and device scales must match. `py tools/xtask-harness/src/paint/compare.py SOURCE --out REPORT`
regenerates the report alone from retained original captures. It opens no apps. Earlier reports
stay intact when a separate output directory is supplied. Reference and input hashes are checked.

`comparison.json` and `comparison.html` report each cell's geometry, display text, text ink box,
relative baseline, alignment, visible clipping/overflow envelope, line count, border position,
thickness, joins and dashes, fill and text colour classes, pattern periods and visible selection.
Every category gives pass, fail, measured and unmeasured/not-applicable counts. A cell may fail
several checks. The viewer sorts by failed checks and never by pixel difference rate.
Display text retains its value provenance; a literal or cached value is not a calculation pass.

Ink boxes use core foreground pixels with a one-pixel edge tolerance. Baseline displacement
is estimated by vertically registering ink-row profiles for identical text and matching visible
line counts, with zero displacement allowed. The strongest match needs similarity at least
0.85 and a lead of at least 0.03 over the next candidate; these establish confidence in the
offset, not a pixel-equivalence score. Ink-bottom deltas remain diagnostic because a descender
fringe can move them alone. This is not Excel's unexposed absolute font baseline. Clipped lines
and ambiguous registrations do not supply baselines. Alignment allows
one pixel. Clipping/overflow compares visible extents over the input's empty neighbours,
independently of the planned text rectangle, and allows two pixels of width difference. This
is not OCR glyph identity. Line counts use ink bands; their characters are not OCR verified.
Patterns and rotated text whose foreground cannot be separated remain unmeasured. Underlines
and strike-through do not supply baseline measurements. Colour classes group dark ink, neutrals
and broad hues; they do not require identical antialiasing or RGB fringes.

Border positions, widths and dash lengths come from strips across physical cell boundaries.
Pen bands use a strict colour distance so ClearType text fringes cannot add a false band.
Dash lengths use complete runs; runs cut by either strip limit are excluded, while interior
gaps remain. Two missing lengths never establish a match. The median complete dash and gap
lengths are measured; dash phase and exact waveform identity remain unmeasured.
Joins compare occupied quadrants at the original corners. All tolerances and original
observations are retained. Selection requires visible reference edges; an inactive Excel
capture without an outline establishes no selection-focus parity. Hidden bands, partial cells
and missing raster calibration remain explicitly outside the measured area. Source coverage
records Normal font, DPI, zoom, merges, freeze state and scroll; missing coverage is not a pass.
Exact and above-16/64 pixel differences and original crops remain as coarse regression
observations. They do not determine cell results, ordering or whether a change is an improvement.

`py tools/xtask-harness/src/paint/compare.py --self-test` exercises the same measurement functions
on known damaged images: erased/wrong-colour ink, a displaced baseline, lost overflow, changed
line breaks/fill, border displacement/thickness, broken joins/dashes, pattern period and missing
selection edges. Antialiasing fringes leave the cell result unchanged. The native harness test
runs these controls without Excel or a browser.

Excel runs on a newly created, owned Windows desktop. The desktop is never switched onto the
owner's screen. The worker verifies its desktop name before any window work, starts Excel
explicitly with `/x`, and records its PID and process start time before COM discovery. It uses
PrintWindow on the verified owned window; there is no screen-copy, input, focus or foreground
fallback. The earlier wall-clock restriction and shared desktop lock are no longer needed:
window operations cannot reach the owner's desktop. A per-worker lease still terminates only
its owned Excel PID/start-time tuple if a step stalls, and `excel-session.json` records launches
and cleanup. Security settings, Excel registrations and other Excel processes are untouched.

The default four inputs measure Calibri 11 Normal at 96 DPI, the requested text/style cases
and unscrolled frozen panes. Other Normal fonts, automatic heights, merged inputs and scrolled
freeze panes require their own recorded captures. Desktop painter comparison remains unmeasured:
`dna-calc-desktop` hosts XLL/COM and has no worksheet painter. Chrome establishes no desktop parity.
