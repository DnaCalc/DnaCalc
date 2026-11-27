# Part V. Inspector

Clusters: `CL-05-01`–`CL-05-12` · Governing decisions: `D-05`, `D-12`, `D-50`–`D-54` · Mockups: [Pro 21 F02](http://127.0.0.1:8720/p21/inspector.html#F02), [Pro 21 I01](http://127.0.0.1:8720/p21/inspector.html#I01), [Pro 21 I02](http://127.0.0.1:8720/p21/inspector.html#I02), [Pro 21 I03](http://127.0.0.1:8720/p21/inspector.html#I03), [Pro 21 S02](http://127.0.0.1:8720/p21/storyboards.html#S02), [Pro 21 S06](http://127.0.0.1:8720/p21/storyboards.html#S06), [Pro 21 S07](http://127.0.0.1:8720/p21/storyboards.html#S07)

---

## 5.1 Dockable Instruments Everywhere, Enlarging to a Five-Tab Bench (`CL-05-01`, `D-50`)

Inspired by **phyphox**, **Inspector** (`dna-view-inspector`) systematically exposes what a workbook depends on, how it calculated, why an answer is wrong or errored, and what live external sources are doing—for Sam (model reviewers and auditors), add-in developers, and debugging agents `[FRAMING-0008]`.

Because auditing happens *beside* the data as often as on a dedicated screen, **every Inspector instrument is built first as a resizable right- or bottom-docked panel usable inside Classic, Notebook, Model map, and App**, and enlarges into the full-stage **Inspector** view (`Ctrl+Alt+2` / `Alt+Shift+2`, or clicking the Strip error/progress pill) with its exact active target, expanded branch, and recording preserved (`R-65`, `D-50`) `[FRAMING-0066, P19-FABLE-0068, P19-FABLE-0218, P19-FABLE-0477, P19-NEON-0050]`. Returning to Classic (`Ctrl+Alt+1`) restores the exact cell selection and scroll offset `[P19-FABLE-0068]`.

Full-stage Inspector organises its `dna.page/1` bench into **five focused tabs** (`D-50`) accompanied by a collapsible **Pinned Watch** strip, on-demand **Details** cell peek, **Notes** (`D-05`), and one-click **`Copy evidence`** (`Ctrl+Shift+C`) `[P19-GPT-0041, P19-NEON-0050, P21-0063]`:

1. **`Overview`:** Count-first whole-workbook triage tables + optional sheet-tier Dependency Atlas (`D-51`).
2. **`Precedents`:** Grouped upstream tree/list + bounded graph (`<=200` nodes / `<=400` edges) + conditional-row **Contribution Ladder** (`Explain` / `Contributions`).
3. **`Dependents`:** Grouped downstream blast-radius tree/list + bounded graph (`Deps`).
4. **`Calculation`:** Strict three-layer separation of Dependency waves, Value diffs, and optional `DNA Calc's calculation order` (`D-53`).
5. **`Live`:** Phyphox-style time-series signal recorder, RTD/external source inventory, and side-effect-free replay (`I01`–`I03`).

Inside tree/graph views, `Left`/`Right` collapse/expand frontiers, `Up`/`Down` step siblings, `Enter` reveals the source or opens docked Formula Explorer, `Alt+Left` steps back, and empty selections prompt cleanly `[P19-GPT-0041, P19-NEON-0050]`.

---

## 5.2 Count-First `Overview` Triage and Root-Cause Error Collapse (`CL-05-02`, `CL-05-03`, `D-51`)

See [Pro 21 S02 — Find the broken name](http://127.0.0.1:8720/p21/storyboards.html#S02).

1. **Count-first `Overview` + opt-in Sheet Atlas (`D-51`, `R-17`):** Whole-workbook entry opens immediately on fast, keyboard-accessible triage tables—error roots, structural anomalies, `Hidden` and `xlSheetVeryHidden` sheets (inspectable read-only; unhiding a hidden sheet previews `Model change: sheet visibility` with `Ctrl+Z`, while protected/VeryHidden sheets disclose restrictions without bypassing them), broken/active external links, VBA modules, dead unread backup sheets, and volatile (`OFFSET`, `INDIRECT`, `NOW`, `RAND`, `RTD`) fan-out counts with profiled durations—and offers a one-click toggle to a **Sheet-Tier Dependency Atlas** capped at sheet/block granularity `[P19-CARBON-0028, P19-CARBON-0188, P19-GPT-0014, D-51]`.
2. **Deterministic root-cause error collapse (`R-15`, `R-16`, `C-5`, `C-13`, `C-16`):** Every `#Error` cell in the workbook is classified topologically as either a **Root Origin** (`⊗`, producing an error from non-error inputs) or **Propagated** (`⊘`), folding thousands of downstream `#REF!`/`#N/A` cells into their exact root cells with downstream affected counts (`Enter` jumps to the root, `Alt+Left` returns, `Ctrl+C` copies the root list for email) `[P19-CARBON-0186, P19-CARBON-0211, P19-FABLE-0045, P19-FABLE-0133, P19-FABLE-0540, P19-NEON-0206, P19-NEON-0251]`.
3. **Broken-name vs. missing-link isolation (`S02`):** On `Inherited!Summary!F18`, Inspector traces `#REF!` directly to the broken defined name `Revenue.Window =OFFSET(Revenue!#REF!,0,0,12,1)`—keeping it explicitly separate from both the stale `AsSaved` cache (`2,401,300`) and the unrelated missing external file `Rates2024.xlsx`, labelling the diagram `Dependency explanation; execution not recorded`, and never guessing a replacement reference or substituting `0` `[P19-GPT-0013, P21-0056]`.

---

## 5.3 Precedents/Dependents, Contribution Trace, Discrepancies, and Repair (`CL-05-04`–`CL-05-07`, `D-52`)

See [Pro 21 F02 — Sales contribution trace](http://127.0.0.1:8720/p21/inspector.html#F02), [Pro 21 S06 — Trace the wrong Sales number](http://127.0.0.1:8720/p21/storyboards.html#S06), and [Pro 21 S07 — Review the exact formula repair](http://127.0.0.1:8720/p21/storyboards.html#S07).

1. **Multi-level dependency & AST structure queries (`CL-05-04`):** While `Ctrl+[` in Classic keeps Excel's jump-to-precedents reflex (offering a once-per-session Strip tip for `Alt+Shift+T`), `Deps` and `Formula` (`C-3`, `C-7`–`C-14`, `C-11`) preserve **name-to-name**, **structured Table**, **spill (`#`)**, and **dynamic (`INDEX`/`XLOOKUP`/`FILTER`/`OFFSET`/`INDIRECT`)** edges with exact run membership and exceptions rather than flattening everything into raw cell coordinates `[P19-CARBON-0209, P19-FABLE-0064, P19-FABLE-0532, P19-FABLE-0533, P19-FABLE-0534, P19-FABLE-0535, P19-FABLE-0536, P19-FABLE-0537, P19-FABLE-0538, P19-GPT-0306, P19-GPT-0307, P19-NEON-0249]`.
2. **Full-scope Conditional Contribution Ladder & View Filter (`CL-05-05`, `R-24`, `R-61`–`R-63`):** Unlike Excel's `Evaluate Formula` (which collapses `SUMIFS` in one opaque step), `Trace` (`Alt+Shift+T`, `<=200 ms` on cast workbooks) evaluates criteria-testing functions (`SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `FILTER`, `XLOOKUP`) across **100% of source rows** (`C-15`, `C-20`, `C-23`), rendering an accessible WAI-ARIA tree ladder of rows passing each test, rows **`failing only this`** test, and graded non-mutating diagnostic hints (`△` hidden time-of-day serial decimals or trailing spaces) `[P19-CARBON-0226, P19-FABLE-0065, P19-FABLE-0072, P19-FABLE-0073, P19-FABLE-0473, P19-FABLE-0475, P19-FABLE-0539, P19-GPT-0322]`. On `Sales!Report!B5` (`128,450` vs `124,250`), `Contributions` scans all `25,000` `Orders` rows and isolates `125` `Paid` orders (`124,250`) plus `1` `Pending` order (`SO10482`, Table row `10482` / worksheet `Orders!A10483:G10483`, `4,200`) `[P19-GPT-0020, P19-GPT-0254, P21-0053]`. Pressing `Enter` on any ladder row opens the source table under a labelled **non-mutating `View filter from Trace`** (`R-62`, `C-32`) showing gapped teal row numbers and raw stored serials alongside formatted text without touching Excel's `AutoFilter` (`Esc` clears) `[P19-FABLE-0066, P19-FABLE-0474, P19-NEON-0270]`.
3. **Dual-target discrepancy comparison & anomaly probes (`CL-05-06`, `R-16`, `R-17`, `R-66`):** Selecting two numeric cells (`Ctrl+Click`) shows their **exact Difference** in the Strip (`R-66`) and lets Inspector compare upstream coverage (catching truncated `Orders!$I$2:$I$20000` ranges vs full 25,000-row Tables, interior run overrides, and `OFFSET` fan-out, `C-15`) `[P19-CARBON-0187, P19-FABLE-0063, P19-FABLE-0478, P19-NEON-0207, P19-NEON-0253]`.
4. **Single-cell repair default with opt-in sibling-run batching (`CL-05-07`, `D-52`):** Previewing a formula repair (`S07`: adding `Orders[Status],"Paid"`, delta `-4,200`, checking pre-commit loop/spill/validation effects via `C-17`, `C-18`, `C-20`) **defaults strictly to the single inspected cell (`1 formula changed`)**; if R1C1-identical sibling cells share the defect, an **unchecked** checkbox (`Also apply to N matching cells in run`) lets the user include them in the same preview and single Undo step `[P19-FABLE-0541, P19-FABLE-0542, P19-FABLE-0544, P21-0055, D-52]`.

---

## 5.4 Three-Layer `Calculation` Honesty and Iterative Telemetry (`CL-05-08`, `CL-05-09`, `D-53`)

Enforced by `Evidence<T>` (`SKIN_ARCHITECTURE.md §4.3`), Inspector's `Calculation` tab never conflates static dependency structure with runtime execution (`R-10`, `D-53`) `[P19-GPT-0097, P19-GPT-0240, P19-GPT-0319]`:

1. **Layer 1 — Dependency waves (`Evidence::Derived`):** Topological dependency frontiers and strongly connected circular components (`CalcState` / `Deps`, `C-19`).
2. **Layer 2 — Value changes (`Changes`, `C-5`):** Exact before->after cell and name deltas from the latest `CalcGeneration`, including post-open `F9` differences from `AsSaved` caches (`38 values differ`) `[P19-FABLE-0050, P19-NEON-0243]`.
3. **Layer 3 — Observed execution (`Evidence::Observed` vs `Unavailable`, `C-20`):** Shown **only** when host event tracing was active, preserving concurrent evaluation bands, strictly titled **`DNA Calc's calculation order`** (never Excel's), and displaying **`Not observed`** otherwise `[P19-GPT-0097, P19-GPT-0240, P19-GPT-0319, D-53]`.

Iterative loop inspection exposes cycle members, last-pass vs current-pass edge reads, passes run, convergence vs `MaxIterations` stop status, and residual delta (`C-7`, `C-8`, `C-19`, `C-21`, `C-22`) alongside scoped recalculation triggers (`F9`, `Shift+F9`, `Ctrl+Alt+F9`, `Ctrl+Alt+Shift+F9`) `[P19-FABLE-0543, P19-FABLE-0545, P19-FABLE-0546, P19-GPT-0318, P19-NEON-0245, P19-NEON-0246]`.

---

## 5.5 Phyphox `Live` Instruments, Pivot Freshness, Notes, and Audit Printing (`CL-05-10`–`CL-05-12`, `D-05`, `D-54`)

See [Pro 21 I01 — Stock live signal](http://127.0.0.1:8720/p21/inspector.html#I01), [Pro 21 I02 — Sales RTD price & coalescing](http://127.0.0.1:8720/p21/inspector.html#I02), and [Pro 21 I03 — Stock (ticked) Live panel](http://127.0.0.1:8720/p21/inspector.html#I03).

1. **Non-intrusive `Live` recorder & RTD coalescing (`CL-05-10`, `C-21`, `C-22`):** External links, RTD topics (`Report!E2 =RTD("DnaCalc.Price",,"LARK","last")`), volatile functions, and pinned cells (`Stock!E17`) plot over time with `Min`, `Max`, and `Last` computed strictly from the displayed observation window `[FRAMING-0138, P19-FABLE-0135, P19-GPT-0320, P19-GPT-0321, P21-0060]`. Observation controls (**`Record`**, **`Pause capture`**, **`Clear`**) stay strictly separate from source controls (`Run`/`Pause`/`Step`/`Reset` on `Stock (ticked)`): pausing capture leaves the underlying feed running; **Replay** scrubs recorded history against the dependency explanation without writing to cells or re-firing RTD events; `Reset` on `Stock (ticked)` restores tick `0` / stock `400` without erasing the recording; and `I02` discloses source rate (`4/s`), sample rate (`2/s`), received/observed/coalesced counts, and `stale` age when disconnected `[P21-0059, P21-0060, P21-0061, P21-0062]`.
2. **PivotTable cache freshness & parity trust (`CL-05-11`, `R-25`, `R-67`):** Grounded in `CalcVerdict` provenance states `[FRAMING-0073]`, every PivotTable displays its last refresh timestamp and whether upstream rows changed since refresh; an unrefreshed Pivot (`124,250` in `S06`/`S07` exception) is badged **`Stale cache`** and never treated as fresh confirmation `[P19-FABLE-0071, P19-FABLE-0479, P19-GPT-0255, P21-0054]`.
3. **CustomXML Findings/Notes & Audit Printing (`CL-05-12`, `D-05`, `D-54`):** Cell history (`Details > History`) and auditor findings/notes store by default in `<notes>` inside `urn:dnacalc:views:v1` (with opt-in export/mirror to Excel cell comments, `D-05`), **`Copy evidence`** preserves formulas, scopes, error roots, and freshness, and **`Ctrl+P`** prints the **active diagnostic instrument** by default with a one-click **`Full Workbook Audit Dossier`** option (`D-54`) `[P19-FABLE-0114, P19-FABLE-0198, P21-0057, D-05, D-54]`.
