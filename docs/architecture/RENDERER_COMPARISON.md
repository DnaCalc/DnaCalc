# Grid painter comparison (U3, 2026-11-01)

Recommendation: keep recycled DOM as the default; offer Canvas2D for dense sheets.
Canvas improves dense scrolling here, but adds raster memory and text/test work.
Both remain runtime selectable under View & panes, or `?renderer=dom|canvas`.
Disagreement G remains provisional until actual screen-reader and device IME checks.

Both consume the same committed TileBatch/RenderPlan. Headers, selection, editor,
charts, controls and the bounded semantic-row accessibility overlay stay shared.
Canvas is aria-hidden; switching painters issues no workbook command or evaluation.

## Method

Linux (Haswell CPU), headless Chromium 156 and Firefox 159;
1440x1000 CSS viewport, DPR 1, system fonts.
Three fresh browser processes per painter/workbook/browser; alternating painter order.
Same upload and keyboard-edit route; formats.xlsx, features.xlsx and generated
512x24 renderer-grid.xlsx (12,288 cells, 4,096 formulas; existing fixture generator).
Each trial records three 180-interval vertical scroll sweeps after calculation ends.
Dense trials record `Recalculated`, zero pending requests and calculated visible formulas.
Frame columns pool 1,620 intervals; latency and memory columns are medians of 3 trials.
Missed slots = sum(max(0, round(rAF interval / 16.667)-1)); this is a headless estimate,
not a physical compositor dropped-frame counter. Small sheets provide little scroll work.
Open = file input change to first new-publication paint opportunity. Edit = Enter to
first authored-revision paint opportunity, which may precede dependent recalculation.
Observers wait for publication then rAF + task; they do not timestamp actual scan-out.

## Results

DOM / Canvas pairs. Times in ms; RSS in MiB summed across owned browser processes,
including shared pages per process (not private RAM); JS heap from Chromium CDP.

|Workbook/browser|Scroll p50|Scroll p95|Missed slots|Open|Edit|RSS|JS heap|
|---|---:|---:|---:|---:|---:|---:|---:|
|formats/Chromium|16.7/16.7|16.7/16.7|0/0|73/80|45/61|1335/1325|3.00/3.55|
|features/Chromium|16.7/16.7|16.7/16.7|0/0|104/118|53/71|1395/1393|2.79/3.15|
|dense/Chromium|33.4/33.3|66.7/50.0|2693/1434|368/322|230/206|1283/1587|3.56/4.90|
|formats/Firefox|17.1/17.1|17.2/17.2|0/0|65/75|27/33|1193/1227|unavailable|
|features/Firefox|17.0/17.0|17.2/17.2|0/0|64/88|19/45|1203/1234|unavailable|
|dense/Firefox|49.3/33.2|66.6/34.2|2654/1129|213/206|181/199|1395/1337|unavailable|

Canvas allocates 3.35–3.36 MiB of RGBA bitmap per visible grid here (zero in DOM mode).
Dense grid DOM nodes fall from 2,784 to 752; semantic cells remain 660 in both.
RSS varies across fresh browser processes; these samples establish no general RAM saving.

## Text and accessibility

Trial-0 screenshots and matching plan crops at 50/100/125/200/400% compare the same
shown text/physical rectangles, including Arabic, accented text and emoji. No missing
ink in 5,771 sampled nonempty plain-text crops (active A2 and conditional artwork
excluded). This is DOM comparison, not Excel parity.
Chromium glyph bounding widths differ by at most 2 px through 200%, 0 px at 400%;
median vertical shift is 1 px at 125%, 0 at other zooms. Changed thresholded ink is
14.7–50.4% through 200% (subpixel positioning matters), 0–0.43% at 400%.
Firefox bounding widths/top positions match; changed ink is 0–0.20% in these crops.
Styled wrapping, distributed text, rotation and decorations need broader pixel coverage.

Both expose role=grid, full sheet dimensions, one active descendant and the same
semantic rows, values and provenance; visual paint nodes are hidden from accessibility.
Canvas cannot replace this DOM overlay. Chrome AX inspection is structural evidence;
no actual screen-reader or physical IME test was available in this Linux session.

## Smoke cost and reproduction

Default DOM core remains 29 Calc + 9 OneCalc cases per browser. Three renderer cases
check identical plans/command counts on switches, real editing, focus and nonempty raster.
Canvas core probe: six basics passed, then formats failed on absent DOM cell selectors;
the run was interrupted, so there is no whole Canvas core result. Six alignment cases
also inspect DOM geometry and need Canvas pixel equivalents;
shared input/worker/semantic assertions continue to apply. Pixel coverage is extra cost.
Run `cargo xtask app smoke calc --workbook @renderer-grid --edit Data!A1=42
--renderer-report <directory>` (one shell line); repeat with formats/features fixture paths.
Use `--renderer canvas` for ordinary smoke runs. Raw JSON, plans and screenshots:
`~/codex-runs/ui-r11-renderer-u3/`; run logs `ui-r11-u3-measure-*.log`.
Measurements used a84d1ce9 + U3 changes, before upstream 48d2c378 and U2 layout
changes; retain them as this implementation's baseline.
