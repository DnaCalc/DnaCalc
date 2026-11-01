# Grid painter comparison (U3, 2026-11-01)

Recommendation: keep recycled DOM as default; retain Canvas2D for dense sheet trials.
Canvas improves dense scrolling here, with raster memory and text/test costs.
Both remain selectable under View & panes, or `?renderer=dom|canvas`.
Disagreement G remains provisional pending actual screen-reader and device IME checks.
Both consume the committed TileBatch/RenderPlan, including U2 merges/font measurements.
Headers, selection, editor, charts and bounded semantic-row accessibility stay shared.
Canvas is aria-hidden; switching issues no workbook command or evaluation.

## Method

The timing and paired-raster tables below measure round 11, source ed59f223. Round 12
retains imported font families and absolute point sizes; that performance matrix has not
been rerun. Its new core and visual checks are described under Smoke cost and reproduction.

Linux (Haswell CPU), headless Chromium 156 and Firefox 159; 1440x1000 CSS viewport, DPR 1.
Three fresh browser processes per painter/workbook/browser, alternating painter order.
Same upload and keyboard-edit route: formats.xlsx, features.xlsx and generated
512x24 renderer-grid.xlsx (12,288 cells, 4,096 formulas; existing fixture generator).
Each trial has three 180-interval vertical sweeps after calculation ends; dense trials
record Recalculated and 210 calculated visible formulas, with zero pending requests.
Frame columns pool 1,620 intervals; latency/memory columns are medians of 3 trials.
Missed slots = sum(max(0, round(rAF interval / 16.667)-1)), a headless estimate
rather than a physical compositor counter. Small sheets provide little scroll work.
Open = file input change to first new-publication paint opportunity. Edit = Enter to
first authored-revision paint opportunity, which may precede dependent recalculation.
Observers wait for publication then rAF + task; they do not timestamp physical scan-out.

## Results

DOM / Canvas pairs. Times in ms. Post-scroll memory samples, without forced GC:
RSS in MiB sums owned browser processes, counting shared pages per process;
JS heap is Chromium page Runtime.usedSize, not a complete worker/Wasm memory total.

|Workbook/browser|Scroll p50|Scroll p95|Missed slots|Open|Edit|RSS|JS heap|
|---|---:|---:|---:|---:|---:|---:|---:|
|formats/Chromium|16.7/16.7|16.8/16.7|0/0|86/87|51/58|1350/1351|3.14/3.12|
|features/Chromium|16.7/16.7|16.7/16.8|0/0|114/116|54/63|1412/1405|3.14/3.28|
|dense/Chromium|50.0/33.3|66.7/50.0|2904/1350|355/342|258/220|1293/1405|5.92/3.81|
|formats/Firefox|17.0/17.1|17.2/17.2|0/0|72/77|29/35|1212/1244|unavailable|
|features/Firefox|17.1/17.1|17.2/17.2|0/0|88/105|34/41|1214/1248|unavailable|
|dense/Firefox|49.4/33.3|66.7/49.5|2885/1414|249/207|247/157|1435/1352|unavailable|

Canvas bitmap estimate: 3.35–3.36 MiB per visible grid (width*height*4); zero in DOM.
Dense grid DOM nodes fall from 2,784 to 752; semantic cells remain 660 in both.
RSS varies across fresh processes; these samples establish no general RAM saving.

## Text and accessibility

Trial-0 screenshots at 50/100/125/200/400% compare matching anchor text/rectangles.
No one-sided missing ink in 5,771 nonempty paired crops; zero plan mismatches.
Active A2 and conditional artwork are excluded. Reference is DOM; Excel pixels were not measured.
Changed ink = XOR/union after thresholding (maximum channel difference >65), worst book per zoom:

|Browser|50%|100%|125%|200%|400%|
|---|---:|---:|---:|---:|---:|
|Chromium|30.82%|21.20%|50.37%|15.78%|0.43%|
|Firefox|0.00%|0.00%|0.16%|0.00%|0.20%|

Chromium bounding widths differ at most 2 px through 200%, 0 px at 400%; median
vertical shift is 1 px at 125%, otherwise 0. Firefox bounds/top positions match.
Styled wrapping, distributed text, rotation and decorations need broader pixel coverage.
Both expose grid dimensions, an active descendant, values, merge spans, permission to begin
editing and provenance. Chromium AX gridcells: 660 each. Canvas still needs the DOM overlay.
AX inspection is structural evidence; actual screen-reader/physical IME checks were unavailable.

## Smoke cost and reproduction

Round 11 on ed59f223: DOM core 33 Calc + 9 OneCalc; full 477 Calc + 52 OneCalc, per browser, all pass.
Three renderer cases check identical plans/command counts, real edits, focus and raster ink;
four U2 core cases cover merge/keyboard behavior, applied-font reflow and protection.
Ordinary core started in Canvas on 18df5c80: Chromium 16 passed/6 failed before stopping;
failures address missing DOM paint nodes, waiting 8 s per assertion. No whole-core or Firefox result.
That run needed paint assertions for Canvas instead of DOM geometry/value selectors.
Round 12 now passes ordinary Canvas core: 51 Calc cases in each browser. Assertions read
the committed RenderPlan and actual raster, requiring matching publication and presentation
epochs; erased transparent pixels cannot satisfy the ink check. The alignment fixture opens
at 1440 by 1200 so its asserted bottom-row glyphs are visible. DOM remains the default.
This is plan and paint evidence, without an Excel pixel comparison. Logs:
`~/codex-runs/ui-r12-canvas-core-second.log` and `ui-r12-canvas-probe-final.log`.
Run `cargo xtask app smoke calc --workbook @renderer-grid --edit Data!A1=42
--renderer-report <directory>` on one shell line; repeat with formats/features fixture paths.
Use `--renderer canvas` for ordinary runs. Raw JSON, plans, screenshots and summaries:
`~/codex-runs/ui-r11-renderer-merged/`; logs `ui-r11-merged-*.log`.
Measured source: ed59f223; all 36 records have clean product sources and workbook hashes.
Earlier matrices remain in ui-r11-renderer-u3/ and ui-r11-renderer-final/ as baselines.
