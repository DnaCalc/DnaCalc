# Part XI. Big Workbooks

Clusters: `CL-11-01`–`CL-11-12` · Governing decisions: `D-48`, `D-65`, `D-79`, `D-80` · Mockups: [Pro 21 S01](http://127.0.0.1:8720/p21/storyboards.html#S01), [Pro 21 W01](http://127.0.0.1:8720/p21/workbook.html#W01), [Pro 21 M03](http://127.0.0.1:8720/p21/model-map.html#M03), [Pro 21 M04](http://127.0.0.1:8720/p21/model-map.html#M04), [Pro 21 M05](http://127.0.0.1:8720/p21/model-map.html#M05), [Pro 21 C07](http://127.0.0.1:8720/p21/classic.html#C07)

---

## 11.1 Stress Benchmarks (`Inherited` & Enron) and Measurement Rigor (`CL-11-01`, `CL-11-04`)

Non-grid views must remain fast, truthful, and calm on messy corporate spreadsheets rather than working only on 50-cell toy demos `[FRAMING-0050, P19-FABLE-0228]`. DNA Calc evaluates scale against two complementary benchmarks, carrying forward OxCalc's `CORE_ENGINE_GRID_PERF_REGISTER` cell/edge/byte workload counters `[FRAMING-0078]`:

1. **Canonical `Inherited.xlsm` stress fixture (`D-91`):** `40` worksheets (`6` hidden), `~300,000` formulas including dense edge-to-edge ranges (`Archive` / `Revenue!B4:M4003`), `90` directed cross-sheet links (`85,272` reference records in the Pro 21 manifest), missing external workbook `Rates2024.xlsx`, quarantined VBA module `LegacyClose`, volatile `OFFSET` names (`Revenue.Window` `#REF!`), and unfixed errors `[FRAMING-0119, P19-CARBON-0073, P19-FABLE-0165, P19-NEON-0063, P21-0128]`.
2. **Real corpus & Enron tier (`20.7M` formulas across ~`16,000` workbooks):** Automated runs open every workbook across views asserting `before.model == after.model` and zero crashes or hangs (`SKIN_ARCHITECTURE.md §17.2`).

**Reference measurement protocol (`R-51`, `R-78`):** Responsiveness is benchmarked (`cargo xtask bench` on the `perf` profile, never as a flaky per-commit check) on a stated reference machine (`Windows 11 / Linux x64, 4 modern cores, 16 GB RAM, local SSD, 1440×900, current Chromium/Firefox/WebView2, checksummed Inherited.xlsm`, plus a separate `8 GB / 200%–400% zoom` constrained report) across `10` cold opens and `>=1,000` input actions recording `p50`/`p95`/`p99`, memory bytes, cache state, and provider state—where missing a budget is reported plainly as a performance failure, never as license to drop data silently `[P19-FABLE-0490, P19-GPT-0062, P19-GPT-0281]`.

**Progressive non-blocking orientation (`CL-11-04`, `R-23`, `S01`):** For big workbooks, *"everything is available" never means "everything is drawn at once"* `[P19-GPT-0056]`. Opening `Inherited.xlsm` paints the shell and last-saved Classic viewport (`AsSaved` cache visibly marked, macros disabled, missing links unrefreshed) before dependency indexing finishes, streams incremental background counts in `Overview` (`18/40 sheets inspected; 14 errors found so far`), keeps `Ctrl+G`, editing, and off-thread volatile `OFFSET` recalculation non-blocking, and announces completion politely once `[P19-CARBON-0116, P19-GPT-0011, P19-GPT-0012, P19-GPT-0253, P21-0050]`.

---

## 11.2 Unbreakable Scale Invariants and Honest Simplification (`CL-11-02`, `CL-11-03`)

Three failure modes are **never acceptable** under any workbook size `[FRAMING-0126, P19-CARBON-0085, P19-FABLE-0183, P19-NEON-0077]`:

1. **Zero UI thread hangs or ignored `Esc`:** Long analysis or calculation runs in 6 ms host slices or background workers with Strip progress and `Esc` cancellation `[P19-CARBON-0085, P19-NEON-0077]`.
2. **Zero silent truncation (`R-52`, `R-79`):** Every folded block, windowed DataFrame, or sampled preview states an exact coverage count (`what is shown + what is stated as omitted == total existing`, `R-79`), and display sampling (`Showing 20 of 25,000 rows`) **never** feeds a calculated aggregate (`Strand Tenet 8`) `[FRAMING-0090, P19-FABLE-0172, P19-FABLE-0491, P19-GPT-0282, P19-NEON-0070, P19-NEON-0231]`.
3. **Zero anomaly concealment (`R-29`) or fabricated pictures:** Collapsing identical R1C1 formula runs (`12/12 runs · 182,400 formulas`), stacking sheets, or zooming out (`<40%`) **always bubbles interior `#Error`s (`⊗`), broken external links (`◊`), and manual overrides (`Exception`) onto the parent badge** `[P19-CARBON-0022, P19-CARBON-0200, P19-FABLE-0183, P19-NEON-0070]`. Whenever a view declines an over-budget scope (`View::fit` over zero-calc `Census` before mounting, `R-30`, `R-53`, `R-80`), it states the exact counts, the plain reason, and **at least two concrete ways forward** (such as scoped sheet/block submap, counted list, Inspector, or Classic) `[P19-CARBON-0201, P19-FABLE-0492, P19-GPT-0283]`.

---

## 11.3 WebView2/Leptos Latency and Progress Budgets (`CL-11-05`, `CL-11-06`, `D-79`)

Grounded in Owner Ruling #7 (`WebView2 + Leptos` baseline; browser allowed up to `2×` only on cold startup and initial file open) `[P19-FABLE-0173, D-79]`:

| Operation | Target | Hard Ceiling (`p95` / `p99`) | User Feedback & Cancellation Contract (`D-79`) |
|---|---|---|---|
| **Cold shell launch** | `<=0.5 s` | Desktop `<=1.0 s` · Standalone Explorer `<=0.5 s` | Immediate Start Stage `[P19-FABLE-0174]`. |
| **Warm `Open` ack / shell paint** | `<=50 ms` | `<=100 ms` ack / `<=300 ms` shell frame (`p95`) | Strip shows staged open progress within `100 ms` `[P19-FABLE-0175, P19-GPT-0063]`. |
| **First useful `AsSaved` viewport** | `<=0.5 s` (small) | `<=1.0 s` desktop / `<=2.0 s` browser on `Inherited` | Usable Classic grid before dependency graph finishes `[P19-FABLE-0175, P19-GPT-0064]`. |
| **Sheet inventory / 300k `Overview`** | `<=1.5 s` / `<=3 s` | Inventory `<=2.0 s` · Full `Overview` `<=5.0 s` (`<=8.0 s` web) | Incremental `"18/40 sheets"` counts; zero UI blocking `[P19-FABLE-0176, P19-GPT-0065]`. |
| **Keystroke / cursor / scroll** | `<=16 ms` (60 Hz) | `<=50 ms` `p95` (`<=100 ms` `p99`) under heavy recalc | Never blank `>100 ms` during fast scroll `[P19-FABLE-0177, P19-FABLE-0178, P19-GPT-0066, P19-GPT-0069]`. |
| **`Alt+Q` palette / Peek / live frame** | `<=25 ms` | `Alt+Q` & Peek `<=50 ms` · Live RTD coalescing `<=100 ms` | Coalesces rapid bursts without dropping final generation `[P19-FABLE-0181, P19-GPT-0069]`. |
| **View switch (`Ctrl+Alt+1..4`)** | `<=100 ms` warm | Selection continuity `<=150 ms` · Content `<=400–500 ms` | Explicit bounded loading card on `Inherited` if `>500 ms` `[P19-FABLE-0180, P19-GPT-0067]`. |
| **Edit-to-dependents & `Esc` cancel** | `<=100 ms` (`<5k` cells) | `0` spinner `<150 ms` · Strip pill `@300 ms` · Cancel bar `@1 s` · Background `@10 s` | `Esc` acknowledged `<=100 ms`, control returned `<=500 ms` (uninterruptible native `.xll` named honestly) `[P19-FABLE-0179, P19-FABLE-0182, P19-GPT-0068]`. |

---

## 11.4 Per-View Show / Simplify / Decline Matrix and Inner-Layer Contracts (`CL-11-07`–`CL-11-12`, `D-80`)

See [Pro 21 M03 — 40-sheet bounded map & expand-all refusal](http://127.0.0.1:8720/p21/model-map.html#M03), [Pro 21 M04 — Revenue blocks & exceptions](http://127.0.0.1:8720/p21/model-map.html#M04), and [Pro 21 M05 — Bounded sheet excerpt](http://127.0.0.1:8720/p21/model-map.html#M05).

| Surface | Shows Directly | Simplifies With Exact Counts (`D-80`) | Declines Cleanly With `>=2` Routes (`D-80`) |
|---|---|---|---|
| **Classic** (`CL-11-07`) | Viewport tiles (`XFD1048576`), unloaded tiles say `Loading` (never blank), 4,096-cell semantic ARIA window keeping real row numbers (`row 10483`) `[P19-GPT-0057]`. | Below **`40%` zoom** (`D-48`), swaps unreadable `<6 px` text for block/defined-name outlines + scroll-gutter error/formula density marks `[P19-FABLE-0051, P19-FABLE-0166]`. | **Never declines** `[P19-FABLE-0166]`. |
| **Inspector** (`CL-11-08`) | All 40 sheets, error roots, links, `OFFSET` names, and tabular lists up to **`2,000` rows**; 1 dependency frontier initially `[P19-GPT-0058, P19-NEON-0065]`. | Folds `>2,000` list rows by root/R1C1 run (`182,400 cells -> 12 runs`) and `>500` direct precedents into 1 bounded range node (`K2:K8761 · 8,760 cells`) `[P19-GPT-0058, P19-NEON-0065]`. | Visual graph caps hard at **`200 nodes / 400 edges`**; refuses *"draw every cell edge"* offering block graph or counted list `[P19-GPT-0058, D-80]`. |
| **Notebook** (`CL-11-09`) | Saved `notebook` doc or user-selected scope; tables `<=20` rows inline; single sheets up to **`150` folded R1C1 runs** `[P19-FABLE-0168, P19-GPT-0059]`. | Windows `>20`-row tables to **`20` rows** (expandable to `200`/page, totals over 100% of rows) and folds runs surfacing exceptions `[P19-FABLE-0168, P19-GPT-0059, D-80]`. | Refuses *"paragraph per cell"* or auto-dumping 40 sheets / `1,907` blocks; offers 1 sheet, chosen outputs + feeders, or `Outline` `[P19-FABLE-0168, P19-GPT-0059]`. |
| **Model map** (`CL-11-10`) | Calm default `<=60` sheet/block nodes; hard viewport cap **`200 nodes / 400 edges`** (`M03`: `8` groups drawn, `32` in searchable inventory; `18/40` partial scope badged) `[FRAMING-0158, P19-GPT-0060, P21-0072, P21-0073]`. | Drills into contiguous calculation blocks (`M04`: `Revenue!B4:M4003` -> `4 × 12,000` formulas with `D63`/`H219`/`F2058` exceptions & `Rates2024.xlsx` separated) + opt-in `M05` sheet excerpt `[P21-0074]`. | Refuses **`Expand all 300,000 cells`**, preserving selection and offering block submap, counted reference list, or `Classic` `[P19-GPT-0060, P21-0072]`. |
| **App & Explorer** (`CL-11-11`) | Authored `dna.page/1` contract; Explorer evaluates full AST of any single formula and arrays `<=50` elements inline `[P19-GPT-0061, P19-NEON-0069]`. | Explorer windows `>50`-element arrays to **`20` rows** (`1-click` expand to `200`, virtualised modal `[Inspect All]`) `[P19-NEON-0069, D-80]`. | `App` declines unauthored `Inherited` (`2,300` constants), offering `Forms maker`, agent draft, or `Classic` (`D-65`) `[P19-FABLE-0170, P19-GPT-0061]`. |

**Host scale contracts (`CL-11-12`):** Every `calc-skin` query accepts a `Budget` (`C-31`, `C-32`), backed by `Census` pre-mount estimates (`C-6`), incremental tile/row reads (`Empty` vs `Loading` vs `Failed`, `C-5`, `C-31`), viewport-prioritised 6 ms calculation slices, and `Esc` cancellation (`C-2`, `C-19`) `[FRAMING-0036, P19-CARBON-0225, P19-GPT-0304, P19-GPT-0305, P19-GPT-0330, P19-GPT-0331, P19-NEON-0240, P19-NEON-0269]`.
