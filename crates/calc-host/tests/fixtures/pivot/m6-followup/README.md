# M6: follow-up questions from the Linux pivot lane, round 4 (Excel 16.0.20527, 2026-11-09)

Captured by `tools/excel-session/pivot-job.ps1` kind `m6` (scenarios `theme`, `conflict`, `fonts`, `exact`, `model`) in an
owned hidden Excel on a private desktop (0 windows on the owner's desktop). A separate research workbook per question; no
committed fixture is altered. Raw outputs: R2 `dnacalc-evidence/excel/pivot-m6-20261109/` (203 files, 16015591 bytes, copy checked, 0 differences; it also holds the P8 replay saves and results). Values are `repr|IEEE bits`
for numbers. The source for 1 and 2 is the M2 `Source` sheet (36 rows) with the W2 layouts.

1. `theme-formats.tsv`: `DisplayFormat` of A3:C26 of pivots styled Light16, Dark11 and Medium2 (rows Region, Product; data Sum
   Amount and Sum Units), stages `office/<style>`, `accent1-changed/<style>` (accent 1 set to RGB C00000, theme colour index 5)
   and `accent1-and-3-changed/<style>` (accent 3, index 7, set to 00C000). Columns: fill RGB/ThemeColor/TintAndShade/pattern, font
   RGB/theme/bold, and each border as `style:weight:RGB:ThemeColor:TintAndShade`. Office theme: accent 1 156082, accent 3 196B24.
   Seen: Light16 A4 bottom border is theme 5 tint 0.3999755851924192 (RGB 44B3E1; FF4040 once accent 1 is changed); Dark11 A4 (the
   East subtotal-style row) fill is theme 7 tint 0.5999938962981048 (83E28E; 80FF80 after accent 3 changes) with a bottom border
   theme 7 tint 0.7999816888943144; Medium2 A4 fill is theme 5 tint 0.3999755851924192 (44B3E1; FF4040), bottom border theme 5 tint
   0.7999816888943144. Cells without a border report `ThemeColor` unreadable (empty), kept as `unread`.
2. `conflict-formats.tsv`: pivot Region x Product, Sum Amount at A3, cell C6 (North/Beta), four cases (`area-then-cell` and
   `cell-then-area`, each with PreserveFormatting on and off). The area format is the data body range (yellow fill FFFF00, red
   font, bottom border); the cell format is `C6` alone (blue fill, white bold font). Stages: `1-first`, `2-second`,
   `3-after-refresh-all` (the North/Beta key moved by renaming its source row to `Nrth`, then `Workbook.RefreshAll`),
   `4-after-undo` (`Application.Undo` after the refresh), `5-keys-restored` (source row restored, refresh), `6-reopened` (saved and
   reopened). Seen: the format applied last wins property by property (area then cell: blue/white/bold; cell then area:
   yellow/red, bold kept because the area sets no weight). After the key moves, with PreserveFormatting on the formats stay at
   their cells (C6 now shows 303 with the same fill and font); with it off they are dropped. Undo restores the values and the
   formats of the pre-refresh state; the reopened file shows the state of the last stage (formats kept with PreserveFormatting on, none with it off).
3. `model.tsv`: model table `Sales` (East 10, West 20, Unknown 5) and `Regions` (East, West), measures TotalAmount, AvgAmount,
   RowCount; CUBEVALUE for `[Sales].[Region].&[East|West|Unknown]`, blank member spellings, `[Regions]` members and no filter,
   and `GETPIVOTDATA` MDX forms over a model pivot, with no relationship and with `Sales[Region]` -> `Regions[Region]`, before and
   after changing 10 to 100 and `RefreshAll`. Seen: blank member spellings (`&`, `[(blank)]`, `&[]`) are #N/A in CUBEVALUE and
   #REF! in GETPIVOTDATA, while the pivot shows an `Unknown` row; `[Regions]` members filter nothing (35, all rows) without the
   relationship and filter East 10 / West 20 with it; the Unknown rows stay in totals (35); values change only after
   RefreshAll (East 10 -> 100, total 35 -> 125). The relationship list could not be read back (0x800AC472, query busy) but the
   `[Regions]` result shows it took effect.
4. `exact.tsv`: model rows (East 0.1), (East 0.2), (West 0.0000001): East SUM is 0.3 with bits 3fd3333333333333 (not the
   binary64 sum 0.30000000000000004, 3fd3333333333334), East AVERAGE 0.15 (3fc3333333333333), West 1E-07 (3e7ad7f29abcaf48), the
   grand SUM 0.3000001 (3fd333339e92fd9e); identical after RefreshAll (the edit rewrote 0.1 with 0.1).
5. `fonts.tsv`: Aptos Narrow 11 and Courier New 11, `Range.Text`, `ColumnWidth` and `Width` (points) of 778.5, 2194.75,
   123456789.125 and 123463844.625 at widths 6.29, 7.14, 8.43, 10.57 (`before-autofit`), after `AutoFit` (`after-autofit`) and
   after `AutoFit` started from each width (`autofit-from-width`). The standard width is 8.43 in both fonts.
