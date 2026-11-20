# DNA Calc Front-End Design: User Experience, Views, Interaction, and Mockups

**Design response to Pro 19 · Companion interactive mockups: `mockups.html`**

---

## 1. Design philosophy

### What "modern on the surface" means

Spreadsheets won the business and engineering world because a spatial grid makes state visible and reactive without forcing the author to write an application first. They fail when a model outgrows its birth medium: a 300,000-formula trading workbook (`Inherited`), a live control loop (`Stock`), or a borrower calculator (`Loan`) all look identical in Excel—an undifferentiated expanse of rectangles where constants masquerade as formulas, silent range truncations look as calm as verified totals, and reading the logic requires playing human compiler across `Monthly!C11` and `Inputs!$C$8`.

"Excel under the hood, modern on the surface" means **separating the calculation substrate from the projection lens without ever separating them in truth**. Under the hood, DNA Calc evaluates every formula, dynamic spill, volatile `OFFSET`, `RTD` topic, data table, and iterative circular loop identically to Excel, and writes a plain `.xlsx` or `.xlsm` workbook that opens unchanged in desktop Excel. On the surface, DNA Calc behaves like a modern flight-deck instrument bench (`STRAND.md`): it infers structure already latent in the workbook—inputs, time axes, calculation blocks, table pipelines, feedback delays—and projects that single live workbook into five purpose-built views (**Classic**, **Inspector**, **Notebook**, **Model map**, and **App**) plus the **Formula Explorer** workbench.

We reject two common dead ends:
- *Why not a "clean SaaS spreadsheet" with giant padding and simplified formulas?* Because analysts like Maria live at 200 keystrokes per minute; stripping density or inventing incompatible formula dialects breaks real workbooks on day one.
- *Why not a clone of Excel's multi-tabbed Ribbon?* Because a 280-pixel command warehouse of ornamental icons buries model diagnostics while starving the work stage of vertical space.

### Weighing familiarity, clarity, density, and honesty

When these four qualities pull against each other, DNA Calc resolves the tension in a strict order: **Honesty > Familiarity of hands > Clarity of structure > Density of chrome**.

1. **Honesty is absolute.** A view may group, fold, label, or re-layout a workbook, and the **Formula Lens** may present `=Monthly!C6 * Inputs!$C$8` as `Ending_Customers × ARPU` or structured `LET` steps—but DNA Calc never silently drops rows, hides an error inside a collapsed block, invents non-Excel evaluation rules, or claims certainty when structural inference is heuristic.
2. **Familiarity belongs in the hands, not in legacy visual clutter.** In **Classic**, every muscle-memory reflex an Excel veteran owns—`F2` edit/point cycling, `F4` `$`-anchoring, `F9` sub-expression evaluation, `Ctrl+Enter` array/range fill, `Alt` mnemonic chords, double-clicking the fill handle—works on the first try without relearning.
3. **Clarity comes from structural provenance, not decoration.** Constants, formulas, external/volatile feeds, fresh recalculations, and errors carry subtle, consistent structural cues across every view so a user spots a hardcoded override inside a formula row in 200 milliseconds without opening an audit dialog.
4. **Density is modal by task, never bloated.** Per Strand, **Working** density gives high-information tabular tightness (`tnum`, compact 22px grid rows, zero wasted gutters) where hands edit; **Reading** density (`Source Serif 4`, ~68ch measure, generous leading) governs **Notebook** and **App** consumption.

### What every view shares: the One Shell

Every workbook window wraps its active stage in a single calm **petrol-800 (`#193F49`) flight-deck shell** around a daylight paper stage (`#FFFFFF` / `#EDF1F2` in `cockpit-light`, `#1A262B` in `cockpit-dark`):

- **The Mast (top, 34px):** Holds the two-block Strand profile badge (`twin` teal+petrol for live workbooks; `dot-long` cyan+petrol for standalone **DNA Calc - Formula Explorer**), workbook title, non-blocking security/link quarantine pills, the **5-view switcher** (`Classic · Inspector · Notebook · Model map · App`, `Ctrl+1`..`Ctrl+5`), the **Formula Lens** toggle (`Alt+L`), **Formula Explorer** trigger (`Ctrl+Shift+E`), **Command Palette** pill (`Ctrl+K`), and Agent Change-Set badge when proposals are staged.
- **The Bridge (30px, collapsible in App view):** Unified **Address & Target Box** (accepting `A1`, defined names, `Table[Column]`, or block paths) + live **Formula / Expression Bar** (`Recursive Mono`, expandable for multi-line `LET`) + Contextual Strip tools.
- **The Stage (center, 14px top corners):** Renders the active view per sheet (or workbook-wide in Model Map / Inspector). Sheet tabs sit at the bottom-left of the Stage with per-sheet preferred-view micro-glyphs.
- **The Instrument Strip (bottom, 26px):** The *only* home for persistent telemetry—calculation mode & Director (`Auto · Recalc`, `Manual · Stale`, `Iter · 100 / 0.001`, `Events · RTD`), recalc progress/duration (`14 ms`), workbook error counter (`[!] 417 errors · 3 roots`), selection aggregates (`Sum · Count · Avg`), and semantic zoom (`100%`).

### Refinements to `STRAND.md` (v0.1)

We adopt Strand's tokens, typography, geometry, and WCAG AA `-ink` contrast law in full, with **five targeted refinements** required by real spreadsheet workflows and the attached model-map sketches:

1. **Multi-reference token disambiguation in `F2` edit mode (refining Tenet 3 & Section 4):** Strand rightly forbids decorative rainbow fills and reserves `amber`, `signal`, and `red` exclusively for focus/calc, external/volatile, and errors. Yet editing `=SUMIFS(Orders[NetAmount], Orders[Region], B7)` requires instant visual pairing between formula tokens and sheet ranges. We add a **4-step cool structural reference ramp** (`teal-600`, `sage-500`, `petrol-500`, `slate-600`, all paired with `-ink` text variants >= 4.5:1) combined with **numbered geometric corner notches** (`①` solid rect, `②` double-tick corner, `③` long-dash, `④`chamfered pill) on both the formula token and the grid/map range. Users with or without color vision pair up to four references effortlessly while `amber`, `signal`, and `red` stay sacred.
2. **Harmonising `names-in-motion.html` and `watershed.html` into Strand:** Those sketches proved the power of Ptolemy-style Directors, `[z⁻¹]` delays, and event queues, but used ad-hoc orange recalc flashes (`#e8590c`), purple cycle strokes (`#d68cff`), and continuously spinning turbine blades. We bring them strictly under Strand: recalculation pulses use **amber (`#FFB301`)** for 140 ms along topological order; `RTD`/volatile event dots use **signal orange (`#FF8001` + `◈` glyph)**; legal iterative cycles use **double-line `teal-600` arcs with `[z⁻¹]` or `[↻ iter]` pills** (reserving `red` `[! CIRC]` strictly for illegal cycles when Iteration is off); and continuous ornamental animations at rest are cut per Tenet 6.
3. **Agent Change-Set preview grammar (extending Tenet 5):** Rather than inventing a new saturated hue for AI proposals, staged agent edits use Strand's ghost-preview vocabulary: **`sage-400` diagonal-hatched preview seams**, `Recursive` casual-axis (`CASL 0.6`) proposed tokens, and a `[Δ Agent]` twin-chip—turning solid only upon explicit accept.
4. **Skins vs. Themes reconciliation (refining Section 6):** Strand v0.1 locked visual variance to three themes (`cockpit-light`, `cockpit-dark`, `high-contrast`) to end legacy CSS sprawl, while the brief requires declarative user/agent skins. We separate **Theme** (palette & contrast pairs, locked to Strand's unit-tested tokens) from **Skin** (declarative layout, panel composition, App card bindings, density, keymaps, and commands). Neither users nor agents can ship CSS that breaks WCAG AA or repurposes `red`/`amber`/`signal`.
5. **Standalone Formula Explorer badge:** We bind Strand's `dot-long` base-pair mark (`teal-400` dot + `petrol-800` bar) to **DNA Calc - Formula Explorer** so a workbookless scratchpad window is never confused with a file-backed workbook (`twin` mark).

### Five principles that decide hard cases

1. **The Backing-Sheet Primacy Rule:** If a gesture in Notebook, Model Map, or App cannot be expressed as a deterministic, reversible Excel workbook mutation (plus optional metadata in the custom XML view part), DNA Calc refuses or scopes it as view-only annotation. *Decides:* dragging a Model Map wire rewrites the cell/name AST; deleting a card in App View removes the card from the layout, never the cells from the sheet.
2. **Never Summarize Away an Anomaly:** Simplification (collapsing 2,800 identical R1C1 rows or grouping 40 sheets) must always surface interior exceptions—errors, hardcoded formula overrides, broken links—onto the collapsed parent badge. *Decides:* a collapsed block of 1,000 rows containing 999 valid formulas and 1 `#DIV/0!` displays a red `[! 1 err inside]` badge at every zoom level.
3. **Preview Seam Before Commitment:** Any action with blast radius—filling 25,000 rows, accepting an agent hunk, rewiring a block, evaluating `F9` inside a formula, or running iterative ticks—shows its exact target scope and reversible preview before committing, and `Esc` always restores exact prior state.
4. **One Selection, Five Projections:** Navigating views changes *projection*, never *place*. Selecting `Monthly!O14` (`Gross Profit`) in Classic and pressing `Ctrl+3` (Notebook) or `Ctrl+4` (Model map) lands on the exact block containing `Monthly!O14` and pulses Strand's 150 ms amber continuity halo around it.
5. **Quarantine Without Hostage-Taking:** Untrusted macros, missing external links, and huge recalc queues never trap the user behind startup modal dialogs. The workbook opens immediately in safe read/cached state with clear Mast quarantine pills so the user can inspect before deciding.

---

## 2. The experience, walked through

Every storyboard below uses the shared cast and workbooks, records exact keys, clicks, screen-reader output, and response latencies, and pairs key moments with 96-column monospace wireframes (mirrored interactively in `mockups.html`).

---

### 2.1 Maria opens `Inherited` for the first time and finds her way around

**Context:** It is Monday morning. Maria has just been handed `Inherited.xlsm` (`18.4 MB`, last saved in 2002: 40 sheets—9 hidden—`304,812` formulas, edge-to-edge sheet `Gas_Daily_2001`, two broken external `.xls` links, VBA `Module1`, six volatile `OFFSET` names, and `417` unfixed formula errors). She launches DNA Calc on desktop for the first time.

#### Step 1 — First run, empty state, and opening `Inherited.xlsm`
1. **First run (`0 ms`):** DNA Calc opens in `cockpit-light` within **65 ms** to a calm, zero-clutter Start Stage—no tour popups, no account wall, no template carousel. Three grounded cards sit on `mist` (`#EDF1F2`) paper: **Open Workbook (`Ctrl+O`)**, **New Blank Workbook (`Ctrl+N`)**, and **Open Formula Explorer (`Ctrl+Shift+E`)**, alongside a compact 4-line *Excel Power-User Note* (`All Excel shortcuts work · Views: Ctrl+1..5 · Command Palette: Ctrl+K · Formula Lens: Alt+L`).
2. **Action:** Maria drags `Inherited.xlsm` onto the window (or presses `Ctrl+O`, selects `Inherited.xlsm`, presses `Enter`).
3. **Progressive load & non-blocking quarantine (`0–240 ms`):**
   - At **55 ms**, the petrol shell snaps to `Inherited.xlsm` (`twin` badge).
   - Instead of Excel's two consecutive blocking modal popups (*"Enable Macros?"* followed by *"This workbook contains links to one or more external sources that could be unsafe..."*), DNA Calc paints the cached values of the active sheet (`Summary_P&L`) at **190 ms** while streaming dependency indexing in the background.
   - Across the top of the Stage sits a single dismissible, keyboard-reachable **Quarantine & Triage Bar** (`Alt+Q`) with two crisp pills: `[Macros Blocked: Module1]` and `[2 Missing Links Held: Cached Values Kept]`, plus a one-click **`Inspect Workbook Health (Ctrl+2)`** prompt triggered automatically because `Inherited` exceeds the complexity/error threshold (`40 sheets · 417 errors`).
   - Screen reader (`NVDA`/`Narrator` polite live region): *"Inherited dot xlsm opened in Classic view, Summary P and L sheet. Macros blocked; 2 missing external links held at cached values; 417 errors across 3 root cells; background dependency index 68 percent."*

#### Mockup M-01: First Open of `Inherited.xlsm` — Progressive Triage & Non-Blocking Quarantine (190 ms)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Inherited.xlsm [Read-Safe]  [1 Classic*] [2 Inspector] [3 Note] [4 Map] [5 App] [Ctrl+K]│ (1)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ QUARANTINE [Alt+Q]: [Macros Blocked: Module1 (3 subs)] [◈ 2 Missing Links: Using Cached]     │ (2)
│ TRIAGE: 40 sheets (9 hidden) · 304,812 fmls · [!] 417 errors (3 roots)  [Open Inspector ──►] │ (3)
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Summary_P&L! │ fx =SUM(Gas_Daily_2001!AT12:AT2850) - Transport_West!H90           [Alt+L Lens]│
│ F19     [!]  │    └─[! #DIV/0! from Gas_Daily_2001!AT142]                                    │ (4)
├────┬─────────┴──────────────┬──────────────┬──────────────┬─────────────────┬────────────────┤
│    │ B                      │ C (Prompt)   │ D (Basis)    │ E (Storage)     │ F (Desk Total) │
├────┼────────────────────────┼──────────────┼──────────────┼─────────────────┼────────────────┤
│ 16 │ Henry Hub Physical     │   $14,820,400│  ◈  $412,900 │      ($185,000) │    $15,048,300 │ (5)
│ 17 │ Permian Basin Swing    │    $6,104,250│     $198,400 │              $0 │     $6,302,650 │
│ 18 │ SoCal Border Index     │    $9,440,000│  [! #N/A]    │       ($92,400) │     [! #N/A]   │
│ 19 │║Desk Net VaR (95%)    ║│   $30,364,650│  [! #N/A]    │      ($277,400) │╔══[! #DIV/0!]═╗│ (6)
│ 20 │ Prior Day Settle (Ext) │ ◈ $29,880,100│ ◈   $590,200 │      ($277,400) │◈   $30,192,900 │
├────┴────────────────────────┴──────────────┴──────────────┴─────────────────┴────────────────┤
│ [≡ 40 Sheets] Summary_P&L* │ Gas_Daily_2001 [248k] │ Transport_West [!] │ ··· [+9 Hidden]    │ (7)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ AUTO · Indexing deps 74% [███████░░░] │ [!] 417 Err (3 roots) │ ◈ 6 OFFSET volatile │ 100%   │ (8)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-01:**
- **(1) Mast & View Switcher:** `twin` Excel-strict identity badge (`[==]`), active view `[1 Classic*]`, and Command Palette hint `[Ctrl+K]`. No ribbon steals vertical space.
- **(2) Non-blocking Quarantine Bar (`Alt+Q`):** VBA `Module1` is sandboxed off and the two dead paths (`N:\Trading\Curves\[NYMEX_Settle_2001_10.xls]` and `C:\...\BasisAdj_v4_FINAL2.xls`) hold their last saved values instead of corrupting cells to `#REF!` on open.
- **(3) Automatic Triage Strip:** Appears only when a file has broken links, hidden sheets, or errors; offers a direct jump to **Inspector (`Ctrl+2`)**.
- **(4) Inline Error Provenance under Formula Bar:** With `F19` selected, DNA Calc immediately names the upstream root cell (`Gas_Daily_2001!AT142`) without forcing 15 manual `Trace Precedents` clicks across sheets.
- **(5) Structural Provenance Glyphs:** `◈` (`signal` `#FF8001` / `prov-ext` `#A35400`) marks cells fed by external links (`C20`, `D16`, `F20`). Constants render in `prov-const` (`#2D6A8A`); ordinary formulas render in neutral ink.
- **(6) Active Selection & Error Pairing:** Cell `F19` has the `amber` (`#FFB301`) double-line focus frame `╔══╗` while its `#DIV/0!` value carries the `red-ink` (`#D02A23`) `[!]` badge—never relying on color alone.
- **(7) High-Count Sheet Drawer (`[≡ 40 Sheets]`):** Shows sheet density (`[248k]` on `Gas_Daily_2001`), error flags (`[!]`), and an explicit `[+9 Hidden]` pill (Excel hides hidden sheets behind right-click › Unhide—and completely conceals the 2 `xlSheetVeryHidden` sheets outside the VBA editor!).
- **(8) Instrument Strip:** Streams background dependency indexing (`74%` -> `100%` at `1,180 ms`), root-error count, and volatile `OFFSET` warning.

#### Step 2 — Discovering hidden sheets, volatile `OFFSET` bottlenecks, and root errors in Inspector
4. **Switching to Inspector (`Ctrl+2`):** Maria presses `Ctrl+2` (or clicks `[Open Inspector ──►]`).
   - **What stays put:** Mast, Bridge (`Summary_P&L!F19` remains the active target in the Address Box), sheet tab bar, and Instrument Strip do not move a pixel.
   - **What changes (`95 ms`):** The Stage transitions smoothly into **Inspector**, pulsing a 140 ms `amber` continuity halo on the `Summary_P&L` cluster and highlighting `F19`'s upstream trace.
   - **What an Excel user notices as a win:** In Excel, cross-sheet precedents appear as a tiny black dashed arrow to a generic mini-spreadsheet icon that you must double-click in a tiny `Go To` list one hop at a time. In Inspector, Maria sees all **3 root errors** responsible for the `417` broken cells on one screen, sees the `9` hidden/`VeryHidden` sheets grey-hatched in the Workbook Map, and sees why every edit in Excel took 2.5 seconds: `6` `OFFSET` names fan out to `184,220` downstream cells on every recalculation!

#### Mockup M-02: `Inherited.xlsm` in Inspector — Root-Error Tree, Hidden Sheets & `OFFSET` Blast Radius

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Inherited.xlsm [Read-Safe]   [1 Classic] [2 Inspector*] [3 Note] [4 Map] [5 App] [Ctrl+K]│
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Summary_P&L! │ fx =SUM(Gas_Daily_2001!AT12:AT2850) - Transport_West!H90      [Trace: 3 hops] │
│ F19     [!]  │ Target: Desk Net VaR (95%) -> [! #DIV/0!] · Root: Gas_Daily_2001!AT142        │
├──────────────┴───────────────────────────────┬───────────────────────────────────────────────┤
│ ROOT ERRORS (3 origins -> 417 broken cells)  │ DEPENDENCY & VOLATILITY X-RAY (40 Sheets)     │ (1)
│                                              │                                               │
│ ► [!] #DIV/0!  Gas_Daily_2001!AT142    (390) │  [NYMEX..xls (Missing)]◈──┐                   │ (2)
│   fx: =AR142 / AS142   (AS142 = 0 MMBtu)     │                           ▼                   │
│   ├─► Gas_Daily_2001!AT2851 (Col Sum)        │  ┌──────────────────────────────┐             │
│   └─►║Summary_P&L!F19 (Desk Net VaR)║ [Here] │  │ Gas_Daily_2001 [Dense: 248k] │◄─┐          │ (3)
│                                              │  │ • AT142 [! #DIV/0! Root]     │  │          │
│ ▸ [!] #REF!    Transport_West!H88       (25) │  └──────────────┬───────────────┘  │◈ Volatile│
│   fx: =G88 * #REF!B12  (Deleted sheet)       │                 │ 390 err          │  OFFSET  │
│                                              │                 ▼                  │ (184k    │
│ ▸ [!] #N/A     Summary_P&L!D31           (5) │  ╔══════════════════════════════╗  │  dirty/  │ (4)
│   fx: =VLOOKUP("SOCAL_BORDER ", Ref_Hubs...) │  ║ Summary_P&L [Active Focus]   ║──┘  recalc) │
│   Hint: Trailing space; "SOCAL_BORDER" A14   │  ║ • F19 [! #DIV/0!] ◄══ Selected║             │
├──────────────────────────────────────────────┤  └──────────────────────────────┘             │
│ HIDDEN & DORMANT SHEETS (9 of 40)            │  ┌─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐             │
│ [Hidden] Old_Calc_Backup (14k fmls, 0 deps)  │  ┆ Sys_AuthTable [VeryHidden]   ┆ (0 live     │ (5)
│ [Hidden] Rates_Archive (0 fmls · 2 refs in)  │  ┆ Legacy_Map    [VeryHidden]   ┆  dependents)│
│ [VeryHidden] Sys_AuthTable · Legacy_Map      │  └─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘             │
├──────────────────────────────────────────────┴───────────────────────────────────────────────┤
│ AUTO · Index ready (412 ms recalc est.) │ [!] 3 Roots -> 417 Err │ ◈ 2 Ext · 6 OFFSET │ 100% │ (6)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-02:**
- **(1) Root-Error Folding:** Instead of dumping a flat list of 417 broken cells, Inspector collapses cascades by topological root: fixing `Gas_Daily_2001!AT142` heals 390 cells; `Transport_West!H88` accounts for 25; `Summary_P&L!D31` accounts for 5 (with an automatic string-near-match diagnosis spotting `"SOCAL_BORDER "` vs `"SOCAL_BORDER"` in `Ref_Hubs!A14`).
- **(2) Broken External Link Node (`◈`):** Explicitly drawn as an upstream boundary node showing its cached timestamp (`2001-10-29`) and which sheets depend on it.
- **(3) Dense-Sheet Summary Block:** `Gas_Daily_2001` (`248k` formulas) is rendered at block level rather than choking the screen with 248,000 nodes.
- **(4) Volatile `OFFSET` Loop (`◈`):** Highlights that `Active_Curve_Window` on `Summary_P&L` uses `OFFSET`, dirtying 184,000 cells in `Gas_Daily_2001` on every keystroke.
- **(5) Hidden & `VeryHidden` Disclosure:** Dashed frames surface `Sys_AuthTable` and `Legacy_Map` (`xlSheetVeryHidden`) and flag dead weight (`Old_Calc_Backup` has 14,000 formulas that nothing downstream reads!).
- **(6) Honest Recalc Telemetry:** Reports estimated full recalculation cost (`412 ms` due to `OFFSET`).

#### Step 3 — Navigating to the root cell and checking keyboard/zoom accessibility
5. **Action:** Using keyboard only, Maria presses `Tab` into the `ROOT ERRORS` pane, `Enter` on `Gas_Daily_2001!AT142`, then `Ctrl+1` to jump straight into **Classic** with `Gas_Daily_2001!AT142` centered in the viewport.
6. **Instant viewport jump (`14 ms`):** Even on the 285,000-cell `Gas_Daily_2001` sheet, virtualised grid rendering paints `AR138:AW152` in **1 frame (`11 ms`)**. Hovering `AS142` (`0`) pops a calm 1-line readout (`AS142 · Physical_Vol_MMBtu = 0 · Constant`). Maria presses `Ctrl++` twice (`150%` semantic zoom): grid text scales cleanly in `Recursive` tabular figures without blurring borders, and `Inherited` is demystified in under 60 seconds without modifying a single byte of the file.

---

### 2.2 Maria turns `Budget` into a notebook write-up for her director

**Context:** Maria opens `Budget.xlsx` (`Inputs`, `Monthly`, `Summary`). Her finance director hates hunting across three tabs to review FY27 plan assumptions against monthly burn and EBITDA, yet insists the deliverable remain the real live `.xlsx` file so Finance can open it in Excel.

#### Step 1 — Opening `Budget.xlsx` in Classic and switching on the Formula Lens
1. **Action:** Maria opens `Budget.xlsx` (`38 ms` load, `4 ms` full recalc). In **Classic** on `Summary`, she selects `Summary!C8` (`Operating EBITDA`: `$146,952`, formula `=Monthly!O14 - SUM(Monthly!O16:O19)`).
2. **Discovering Formula Lens (`Alt+L`):** Seeing `[Alt+L Lens]` on the right end of the Formula Bar, Maria presses `Alt+L`.
   - Immediately, the Formula Bar displays a dual-line readable translation synthesized from row/column headers and defined names—without altering the stored Excel formula:
     `Lens: Gross_Profit[FY27_Total] − SUM(Support_Payroll..GA_Fixed[FY27_Total])`
     `Excel: =Monthly!O14 - SUM(Monthly!O16:O19)`

#### Step 2 — Switching to Notebook (`Ctrl+3`) and authoring the live narrative
3. **Action:** With `Summary!C8` selected, Maria presses `Ctrl+3` (**Notebook**).
   - **First-time Notebook synthesis (`85 ms`):** Because `Budget.xlsx` has never been opened in Notebook before, DNA Calc synthesizes a clean initial outline from the workbook's inferred structure (banner at top: *"Auto-arranged from 3 sheets · Edit order or add prose freely; cells stay unchanged in Excel"*):
     1. **Section 1 — Assumptions (`Inputs!B4:D16`)** rendered as an interactive 12-row scenario parameter card with a segmented scenario switch (`Base Case` | `Downside` bound to `Inputs!C2`).
     2. **Section 2 — FY27 Executive Summary (`Summary!B4:E10` + `Revenue vs EBITDA` chart)** with an `amber` continuity halo pulsing around `Operating EBITDA ($146,952)` where Maria's cursor was!
     3. **Section 3 — Monthly Operating Pipeline (`Monthly!B4:O20`)** rendered as a horizontally scrollable, frozen-stub **Data Frame** (`Jan-27`..`Dec-27` + `FY27 Total`, sparklines inline).
4. **Adding narrative prose & live inline tokens:** Maria presses `Enter` above Section 2 to insert a prose block (`Source Serif 4`, Reading density, `68ch` measure). She types:
   `Under the ` then types `@` to open the inline reference picker, selects `Active_Scenario` (`Inputs!C2`), continues ` plan, FY27 revenue reaches @Summary!C5 with Operating EBITDA of @Summary!C8 (` and `@Summary!D8` ` margin), absorbing the Q3 @Inputs!C16 wage step.`
   - Every `@` chip renders as a rounded `5px` Strand value token (`Base Case`, `$6,946,800`, `$146,952`, `2.1%`, `4.0%`) that recalculates live like Pluto—never going stale like Jupyter!
5. **Testing reactivity live:** Maria clicks `Downside` on the inline scenario control (`Inputs!C2` -> `"Downside"`).
   - Within **6 ms**, an `amber` recalc ripple sweeps top-to-bottom in dependency order (`Inputs` -> `Monthly` Data Frame -> `Summary` KPIs -> inline prose chips). The prose now reads: *"Under the `[Downside]` plan, FY27 revenue reaches `[$5,884,200]` with Operating EBITDA of `[($612,400)]` (`[-10.4%]` margin)..."*
   - Maria presses `Ctrl+Z`: `Inputs!C2` reverts to `"Base Case"` (`$146,952`) across Notebook and Classic simultaneously.

#### Mockup M-03: `Budget.xlsx` in Notebook View — Live Reactive Prose, Inline Chips & Data Frame

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx [Saved]      [1 Classic] [2 Inspector] [3 Notebook*] [4 Map] [5 App] [Alt+L]│
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Summary!C8   │ Lens: Gross_Profit[FY27] - SUM(OpEx_Blocks[FY27])  │ [Flow: ↓ Page | → Pipe] │ (1)
│ $146,952     │ Excel: =Monthly!O14 - SUM(Monthly!O16:O19)         │ [+ Prose] [+ Slice]     │
├──────────────┴───────────────────────────────────────────────────────────────────────────────┤
│   FY27 Operating Plan — Finance Director Brief                                [Reading Mode] │ (2)
│                                                                                              │
│   ║ Under the [Base Case ▾] plan, FY27 revenue reaches [$6,946,800] with Operating         ║ │ (3)
│   ║ EBITDA of [$146,952] ([2.1%] margin), absorbing the Q3 [4.0%] wage step while keeping  ║ │
│   ║ cloud infrastructure at [18.0%] of revenue._                                           ║ │
│                                                                                              │
│   ┌─ 1. KEY SCENARIO DRIVERS (Inputs!B5:D16) ────────────── Active: (• Base Case) (Downside)┐│ (4)
│   │ Assumption                 │ Base Case (Live) │ Downside  │ Formula / Provenance        ││
│   │ Monthly Logo Growth        │            3.5%  │     1.5%  │ Constant (Inputs!C6)        ││
│   │ Monthly Churn              │            1.2%  │     2.4%  │ Constant (Inputs!C7)        ││
│   │ ARPU ($/mo)                │            $420  │     $380  │ Constant (Inputs!C8)        ││
│   │ [▸ Show remaining 9 assumptions...]                                                     ││
│   └─────────────────────────────────────────────────────────────────────────────────────────┘│
│   ┌─ 2. MONTHLY P&L DATA FRAME (Monthly!B9:O20 · 12 periods across) ──────── [Lens: ON] ────┐│ (5)
│   │ Line Item          │  Jan-27 │  Feb-27 │ ··· │  Dec-27 │  FY27 Total │ Spark / Rule     ││
│   │ Total Revenue      │ $515.8k │ $527.5k │ ··· │ $647.6k │  $6,946,800 │ ▁▂▃▄▅▆▇█ Cust×ARP││
│   │ Gross Profit (82%) │ $422.9k │ $432.6k │ ··· │ $531.1k │  $5,696,376 │ ▁▂▃▄▅▆▇█ Rev-COGS││
│   │╔Operating EBITDA  ╗│  ($4.2k)│   $1.8k │ ··· │  $34.9k │╔═  $146,952╗│ ▃▃▄▅▅▆▇█ GP-OpEx ││ (6)
│   └─────────────────────────────────────────────────────────────────────────────────────────┘│
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ [≡ 3 Sheets] Inputs │ Monthly │ Summary* [Notebook preferred]                                │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ AUTO · Recalc 4 ms (fresh) │ 0 Errors │ Prose stored in customXml (Excel-safe) │ Read · 100% │ (7)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-03:**
- **(1) Formula Lens & Layout Mode Switch:** Shows both the semantic Lens formula and raw Excel formula; `[Flow: ↓ Page | → Pipe]` lets Maria toggle vertical document flow vs left-to-right pipeline lanes.
- **(2) Source Serif 4 Prose Layer:** Distinct typographic voice (`Reading` density) clearly separates human commentary from workbook cells (`Recursive` sans/mono).
- **(3) Live Inline Value & Control Chips:** `[Base Case ▾]` is directly bound to `Inputs!C2`; `[$6,946,800]` and `[$146,952]` are live references to `Summary!C5` and `Summary!C8`—never copy-pasted dead text.
- **(4) Folded Assumptions Card:** Surfaces the 3 headline drivers and folds the remaining 9 (`[▸ Show remaining 9...]`)—honest folding with explicit count, never silent hiding.
- **(5) Reactive Data Frame:** Renders `Monthly!B9:O20` with sticky row labels, sticky `FY27 Total`, inline sparklines, and Formula Lens row summaries (`Cust×ARPU`).
- **(6) Cross-View Selection Sync:** `Operating EBITDA` (`Summary!C8` / `Monthly!O20`) retains the `amber` focus frame.
- **(7) Excel Round-Trip Honesty:** Strip confirms prose and layout live in `xl/customXml/dnaCalcViews.xml`; saving `Budget.xlsx` (`Ctrl+S`) opens cleanly in Excel with `Inputs`, `Monthly`, and `Summary` intact.

#### Step 3 — Copy/paste with external apps
6. **Clipboard fidelity:** Maria selects the prose paragraph + Section 2 table and presses `Ctrl+C`, then pastes into an email client (`Ctrl+V`): DNA Calc writes clean rich HTML + plain-text Markdown with current evaluated numbers (`$146,952`) and formatted tables, while pasting inside DNA Calc preserves live cell references.

---

### 2.3 Sam traces why one number on `Sales`'s `Report` is wrong

**Context:** Sam (auditor) opens `Sales.xlsx` (`Orders` table with `25,000` order rows `A1:I25001` on `Orders`; `Report` sheet with `SUMIFS`/`XLOOKUP`/`FILTER` summary on the left and `Pivot_RegionSKU` PivotTable on the right). On `Report`, `West` Q4 Net Sales (`Report!C7`) shows **`$1,412,800`**, whereas `Pivot_RegionSKU` (`Report!N8`) shows **`$1,648,250`**—an unexplained **`-$235,450`** gap, with **zero Excel `#` errors** on the sheet!

#### Step 1 — Comparing two numbers that disagree (`Diff / Reconcile` in Inspector)
1. **Action:** In **Classic** on `Report`, Sam clicks `Report!C7` (`$1,412,800`), holds `Ctrl`, clicks `Report!N8` (`$1,648,250`), and looks at the Instrument Strip:
   `2 cells selected · Δ = -$235,450 (-14.3%) · [Trace Discrepancy (Ctrl+2)]`
2. **Action:** Sam presses `Ctrl+2` (**Inspector**). Because two cells reading the same upstream table (`Orders`) are selected, Inspector opens in **Dual-Path Comparison** (`28 ms` analysis over `25,000` rows).
3. **Instant structural diagnosis — Bug #1 (Range Clipping):**
   - Left lane (`Report!C7`): `=SUMIFS(Orders!$I$2:$I$22000, Orders!$C$2:$C$22000, B7, Orders!$B$2:$B$22000, ">="&$C$2)`
   - Right lane (`Report!N8` Pivot): Source `Orders` (`Orders!$A$1:$I$25001`, Cache fresh).
   - Inspector flashes an unmistakable **Structural Coverage Warning** on `Report!C7`:
     `[⚠ Partial Table Range: $2:$22000 covers 88.0% of table 'Orders' (skips 3,001 rows: 22001..25001)]`
   - Better yet, Inspector computes the exact delta contributed by those skipped 3,001 rows for `West` Q4 right in the diagnostic card: **`+$198,450` across 412 skipped rows**!

#### Mockup M-04: `Sales.xlsx` in Inspector — Catching Range Truncation & Trailing-Space String Mismatch

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Sales.xlsx               [1 Classic] [2 Inspector*] [3 Note] [4 Map] [5 App]   [Ctrl+K] │
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Compare 2    │ C7 ($1,412,800) vs N8 ($1,648,250) · Discrepancy: -$235,450  [Explore Ctrl+E] │ (1)
├──────────────┴───────────────────────────────┬───────────────────────────────────────────────┤
│ UPSTREAM COVERAGE ON TABLE 'Orders' (25,000) │ DISCREPANCY WATERFALL (-$235,450 explained)   │
│                                              │                                               │
│  Orders[A2:I25001]                           │  Report!C7 (Formula)             $1,412,800   │
│  ┌──────────────────────────┐                │                                               │
│  │ Rows 2 .. 22,000         │ ◄─ C7 SUMIFS   │  [1] Skipped rows 22001:25001     +$198,450   │ (2)
│  │ (21,999 rows · 88.0%)    │    reads ONLY  │      412 'West' Q4 rows outside               │
│  │                          │    this slice  │      hardcoded $I$2:$I$22000                  │
│  ├══════════════════════════╡ ◄─ CLIP SEAM!  │      [Fix C5:C8 -> Orders[Col] (Preview)]     │ (3)
│  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│                │                                               │
│  │▒ Rows 22,001 .. 25,001  ▒│ ◄─ Pivot N8    │  [2] Criteria mismatch (1 row)     +$37,000   │ (4)
│  │▒ (3,001 rows SKIPPED!)  ▒│    reads ALL   │      Orders!C23804 = "West␣"                  │
│  └──────────────────────────┘    25,000 rows │      (trailing space; Pivot grouped/trimmed   │
│                                              │       via Report!N9 'West ' subtotal)         │
│  C7 Formula:                                 │      [Jump to Orders!C23804 ──►]              │
│  =SUMIFS(Orders!$I$2:$I$22000,               │  ───────────────────────────────────────────  │
│          Orders!$C$2:$C$22000, B7,           │  Reconciled Total                $1,648,250   │ (5)
│          Orders!$B$2:$B$22000, ">="&$C$2)    │  Remaining Unexplained                   $0   │
├──────────────────────────────────────────────┴───────────────────────────────────────────────┤
│ [≡ 2 Sheets] Orders [Table: 25,000 rows] │ Report* [SUMIFS · XLOOKUP · FILTER · Pivot]       │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ AUTO · Audit 28 ms │ 0 #Err · [⚠ 4 Clipped Ranges on Report!C5:C8] │ Δ -$235,450 │ 100%      │ (6)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-04:**
- **(1) Two-Target Comparison Mode:** Selecting any two cells (`Ctrl+Click`) and entering Inspector compares their dependency subgraphs directly.
- **(2) Visual Clip Seam on Table `Orders`:** Shows proportionally (`88.0%` vs `12.0%` hatched) that `SUMIFS` stops at row `22,000` while the Excel Table extends to `25,001`.
- **(3) Actionable Preview Fix:** Offers `[Fix C5:C8 -> Orders[Col] (Preview)]`—rewriting `$I$2:$I$22000` to structured references `Orders[NetAmount]`, `Orders[Region]`, `Orders[OrderDate]` across all four regional rows (`C5:C8`) with a Ghost Preview before commit.
- **(4) Bug #2 Caught Automatically (Dirty Key `"West␣"`):** Even after extending to row `25,001`, `$1,412,800 + $198,450 = $1,611,250`—still `$37,000` shy! Inspector's near-match predicate check spots `Orders!C23804` containing `"West "` (with a visible `␣` whitespace glyph), worth `$37,000`.
- **(5) Zero Unexplained Residual:** `$1,412,800 + $198,450 + $37,000 = $1,648,250` (`$0` unexplained).
- **(6) Honest Non-Error Warning:** Excel shows zero green triangles or `#` errors when a range stops at row 22,000 of a 25,000-row table. DNA Calc surfaces `[⚠ 4 Clipped Ranges]` in the Strip without misusing `red` (reserved strictly for evaluation errors).

#### Step 2 — Verifying in Formula Explorer and fixing cleanly
4. **Action:** Sam presses `Ctrl+Shift+E` to pop open the docked **Formula Explorer** on `Report!C7` just to verify the predicate filter slices (`Orders[Region]="West"` matches `6,114` rows; `Orders[OrderDate]>=2026-10-01` narrows to `1,842` rows), clicks `[Fix C5:C8 -> Orders[Col]]` (`Preview Seam` shows `C7` moving `$1,412,800 -> $1,611,250`, presses `Enter` to commit), then clicks `[Jump to Orders!C23804 ──►]`, switches to **Classic (`Ctrl+1`)**, presses `F2`, deletes the trailing space in `"West "`, and presses `Enter`.
5. **Result (`9 ms` recalc):** `Report!C7` updates to **`$1,648,250`** (`green-500` fresh value tick `✓`), matching `Report!N8` to the cent. Two `Ctrl+Z` presses undo both edits cleanly if Sam wants to leave the audit file untouched and export the Inspector Discrepancy Card to clipboard (`Ctrl+Shift+C`) for his audit memo.

---

### 2.4 Lee builds `Stock` on the model map, then checks it in Classic

**Context:** Lee (systems engineer) wants to model a warehouse inventory control loop (`Stock.xlsx`): customer demand, physical stock on hand, inflow, outflow, a 3-period supplier reorder delay, a form-control slider for the reorder point, and a deliberate circular feedback reference solved under Excel's iterative calculation. He hates writing `=IF($B$1, 500, C12 + D12 - E12)` in anonymous cells before seeing the system topology.

#### Step 1 — Empty state on Model Map and rapid keyboard-first block creation
1. **Action:** Lee presses `Ctrl+N` (New Workbook) then `Ctrl+4` (**Model map**).
2. **Empty state (`18 ms`):** On crisp `mist` paper sits a single centered prompt card—no clutter:
   `Empty Model Map · Double-click or press [N] to add a named block · [G] to add a group · Every block writes plain Excel cells & names on Sheet1.`
3. **Creating blocks (`N` key, zero mouse required):**
   - Lee presses `G`, types `Stock`, hits `Enter` (creates group frame `Stock.`).
   - Inside `Stock.`, he presses `N`, types `OnHand = 500` (`Enter`), presses `Tab` to spawn a connected sibling, types `Outflow = MIN(Demand.Rate, OnHand)` (`Enter`), adds `Demand.Rate = 120`, `Policy.ReorderPoint = 280`, `Policy.OrderQty = 400`, `Supply.LeadTime = 3`, `Supply.DelayPipe = SEQUENCE(1, Supply.LeadTime, 0, 0)`, and `Inflow = INDEX(Supply.DelayPipe, 1)`.
   - Notice the **Formula-Name Presenter & Interpreter (`LINKED_BOXES_SKIN.md`)**: inside group `Stock.`, Lee simply types `Outflow = MIN(Demand.Rate, OnHand)`; DNA Calc stores the real cell formula `=MIN(Demand.Rate, Stock.OnHand)` on the backing sheet `Inventory_Model` and binds workbook-scoped defined names (`Stock.OnHand -> Inventory_Model!$C$8`, etc.).

#### Step 2 — Refusal & preview moment when closing the circular feedback loop
4. **Action:** Lee updates `Stock.OnHand` to `=IF(Sim.Reset, 500, OnHand + Inflow - Outflow)` and drags an output wire from `Stock.OnHand` to `Policy.OrderPlaced` (`=IF(OnHand + SUM(Supply.DelayPipe) < Policy.ReorderPoint, Policy.OrderQty, 0)`), then wires `Policy.OrderPlaced` into the tail of `Supply.DelayPipe`.
5. **Cycle refusal & honest resolution seam:** Because a brand-new Excel workbook has `Iterative Calculation = OFF`, closing that loop would trigger Excel's modal *"There are one or more circular references..."* warning and freeze evaluation at `0`. Instead, before committing the wire, DNA Calc freezes the wire in an **amber/red Preview Seam** right at Lee's cursor:
   - `[! Circular Feedback Loop: OnHand → OrderPlaced → DelayPipe → Inflow → OnHand]`
   - `Plain Excel Recalc forbids cycles. Choose how Excel should evaluate this loop:`
     - **`[1] Enable Ticks Director`** *(sets Excel Iterative Calc = ON, Max Iterations = 1, Max Change = 0.001 — steps 1 period per F9)*
     - **`[2] Enable Equilibrium Iteration`** *(sets Excel Iterative Calc = ON, Max Iterations = 100, Max Change = 0.001 — settles to fixed point)*
     - **`[Esc] Cancel wire`**
6. **Action:** Lee presses `1`. Immediately, the wire commits as a double-line `teal-600` feedback arc badged `[z⁻¹]`, and the Stage header displays `[Director: Ticks · Iter=1, Δ=0.001 · ▶ Run | Step (F9) | Reset]`.
7. **Adding the Form-Control Slider:** Lee right-clicks `Policy.ReorderPoint` (`280`) -> `Attach Excel Form Control › Slider (ScrollBar)` (min `0`, max `600`, step `10`). DNA Calc creates a genuine Excel `Form Control ScrollBar` linked to `Inventory_Model!$C$5` (`Policy.ReorderPoint`), rendering an interactive range slider directly inside the `Policy.ReorderPoint` actor card!

#### Mockup M-05: `Stock.xlsx` on Model Map — Mid-Tick Firing Pulse, `[z⁻¹]` Delay & Form-Control Slider

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Stock.xlsx               [1 Classic] [2 Inspector] [3 Note] [4 Model map*] [5 App]      │
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Stock.OnHand │ Lens: OnHand⁻ + Inflow - Outflow                              [Alt+L Lens: ON]│
│ 340 units    │ Excel: =IF(Sim.Reset, 500, Stock.OnHand + Stock.Inflow - Stock.Outflow)       │
├──────────────┴───────────────────────────────────────────────────────────────────────────────┤
│ Crumb: Stock.xlsx › All Blocks  │ [Director: Ticks (Iter=1)] [▶ Run] [Step F9] [Reset] t=14 │ (1)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│                        ┌───[z⁻¹]───┐                                                         │ (2)
│  ┌─ Demand. ────────┐  │           ▼                                                         │
│  │ Rate         120 │──┼──────►╔═══════════════════╗       ┌─ Policy. ─────────────────────┐ │
│  └──────────────────┘  │       ║ Stock.OnHand  340 ║──────►│ OrderPlaced               400 │ │ (3)
│                        │  ┌───►║ OnHand⁻+In-Out    ║  ┌───►│ OnHand+ΣPipe < ReorderPt ? Q  │ │
│                        │  │    ╚═════════╤═════════╝  │    └───────────────┬───────────────┘ │
│                        │  │              │            │                    │                 │
│                        │  │    ┌─────────▼─────────┐  │    ┌───────────────┴───────────────┐ │
│                        └──┼────│ Stock.Outflow 120 │  │    │ Policy.ReorderPoint       280 │ │ (4)
│                           │    │ min(Rate,OnHand⁻) │  │    │ [──────●─────────] (0 .. 600) │ │
│  ┌─ Supply. ──────────────┼────┴───────────────────┘  │    └───────────────────────────────┘ │
│  │ DelayPipe  {0, 0, 400} │───────────────────────────┘                    │                 │
│  │ shift(Pipe⁻) ◄─ Order  │◄══[z⁻¹]════════════════════════════════════════┘                 │ (5)
│  │           │            │    (Amber pulse: OrderPlaced just fired 400 into DelayPipe[3])   │
│  │           ▼            │                                                                  │
│  │ Inflow               0 │──► (feeds Stock.OnHand next tick; 3-tick reorder delay)          │
│  └────────────────────────┘                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ [≡ 1 Sheet] Inventory_Model* [Map preferred · 9 named cells · 1 Form Control]                │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ITER (Max 1 · Δ 0.001) · Step 2.1 ms │ 0 Errors │ [↻ 2 deliberate z⁻¹ loops] │ Work · 100%  │ (6)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-05:**
- **(1) Director Bar (Excel Calculation Properties Made Visible):** Exposes Excel's `Iterative Calculation (Max=1)` as an intelligible `Ticks` director with `Step (F9)` and `▶ Run` (which triggers standard Excel recalculations at a chosen rate).
- **(2) `[z⁻¹]` Self-Reference Feedback Loop:** Draws `Stock.OnHand` reading its prior value (`OnHand⁻`) explicitly as a delayed feedback arc rather than an invisible self-reference or alarming error arrow.
- **(3) Active Firing Pulse (`amber`):** On `Step (F9)` at `t=14`, `OnHand` dropped to `220 < 280` (with `0` in pipeline), firing `OrderPlaced = 400` in topological order (`140 ms` amber border settle).
- **(4) Genuine Excel Form Control Slider:** Rendered cleanly inside `Policy.ReorderPoint` on the map, backed by an actual OOXML Form Control linked to `$C$5`.
- **(5) Dynamic-Array Reorder Delay (`{0, 0, 400}`):** `Supply.DelayPipe` shows its live 3-element spill vector right inside the block.
- **(6) Honest Strip Readout:** Confirms `ITER (Max 1 · Δ 0.001)` and `[↻ 2 deliberate z⁻¹ loops]` so no reviewer mistakes iterative state for an accidental circular bug.

#### Step 3 — Checking the backing sheet in Classic (`Ctrl+1`)
8. **Action:** Lee presses `Ctrl+1` (**Classic**) to inspect what Excel users will actually see when he emails `Stock.xlsx`.
9. **Win over node-only tools (`M-06`):** Every block Lee created on the Model Map was cleanly allocated onto `Inventory_Model` (`B3:F15`) grouped by prefix (`Demand`, `Policy`, `Supply`, `Stock`), with column `B` holding labels, column `C` holding named cells (`$C$5` = `280` with the Excel Form Control slider beside it in `D5:E5`), and dynamic array `C11:E11` spilling `{0, 0, 400}`! Lee drags the Form Control slider in Classic from `280` to `310`, presses `F9` twice, watches `C8` (`Stock.OnHand`) step cleanly (`<2 ms`), and knows `Stock.xlsx` is 100% native Excel.

#### Mockup M-06: `Stock.xlsx` Checked in Classic — Clean Backing Grid & Live Form Control

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Stock.xlsx               [1 Classic*] [2 Inspector] [3 Note] [4 Model map] [5 App]      │
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Policy.      │ fx 310                                            [Linked Control: ScrollBar1]│
│ ReorderPoint │ Cell: Inventory_Model!C5 · Constant · Read by: Policy.OrderPlaced (C7)        │ (1)
├────┬─────────┴──────────────┬──────────────┬──────────────┬─────────────────┬────────────────┤
│    │ B (Block / Parameter)  │ C (Value)    │ D (Spill/Ctl)│ E (Spill/Ctl)   │ F (Formula/NT) │
├────┼────────────────────────┼──────────────┼──────────────┼─────────────────┼────────────────┤
│  4 │ Demand.Rate            │          120 │              │                 │ units / tick   │
│  5 │ Policy.ReorderPoint    │╔════════ 310╗│ [◄───●────►] │ (0..600, stp 10)│ Form ScrollBar │ (2)
│  6 │ Policy.OrderQty        │          400 │              │                 │ lot size       │
│  7 │ Policy.OrderPlaced     │            0 │              │                 │ =IF(C8+SUM...) │
│  8 │ Stock.OnHand       [↻] │          340 │              │                 │ =IF(C3,500,...)│ (3)
│  9 │ Stock.Inflow           │            0 │              │                 │ =INDEX(C11#,1) │
│ 10 │ Stock.Outflow          │          120 │              │                 │ =MIN(C4,C8)    │
│ 11 │ Supply.DelayPipe   [↻] │┌────────── 0 │            0 │             400┐│ =IF(C3,SEQ...) │ (4)
├────┴────────────────────────┴┴─────────────┴──────────────┴────────────────┴┴────────────────┤
│ [≡ 1 Sheet] Inventory_Model* [Map preferred]                                                 │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ITER (Max 1 · Δ 0.001) · F9 = Step +1 │ 0 Errors │ [↻ Iterative cells: C8, C11] │ 100%      │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-06:**
- **(1) Defined-Name Address Priority:** Selecting `C5` displays `Policy.ReorderPoint` in the Address Box and notes `Linked Control: ScrollBar1`.
- **(2) Native Form Control Integration:** Dragging `[◄───●────►]` in `D5` updates `C5` (`280 -> 310`) at `120 fps` with `prov-const` (`#2D6A8A`) constant styling.
- **(3) `[↻]` Iterative Badge in Classic:** Cells participating in an active iterative cycle (`C8`, `C11`) carry a subtle `teal-ink` `[↻]` tag so Classic users never miss self-referencing logic.
- **(4) Dynamic Array Spill Outline:** `C11#` (`C11:E11`) shows a crisp `teal-600` spill bracket around `{0, 0, 400}`.

---

### 2.5 Jo runs `Loan` as an App, tries scenarios and prints the result

**Context:** Jo (regional manager, non-model-builder) opens `Loan.xlsx` (`$450,000` principal, `6.25%` rate, `30-year / 360-month` schedule using `PMT`, `IPMT`, `PPMT`, running balance, and chart) to evaluate paying an extra `$300/month` toward principal and bring a clean printed summary to a bank meeting.

#### Step 1 — Opening directly into App View and safe refusal of formula overwrite
1. **Action:** Jo double-clicks `Loan.xlsx`. Because its view metadata sets `preferredView="app"`, DNA Calc opens directly in **App View (`Ctrl+5`)** in **75 ms** (`Bridge` formula bar collapsed cleanly into a compact `[Show Formulas (Alt+L)]` pill so Jo is not confronted with `=-PMT(C5/12, C6*12, C4)`).
2. **Accidental-edit refusal (make-or-break safety moment):** Jo clicks the headline **`$2,770.74` Scheduled Monthly Payment** card and types `2400` trying to see what loan `$2,400/mo` buys.
   - In Excel, typing `2400` into that summary cell silently destroys the `-PMT(...)` formula forever!
   - In **App View**, formula cells are **write-protected against literal clobbering**. Nothing is overwritten; a calm inline callout appears for 4 seconds (dismissible with `Esc`):
     `Scheduled Payment is calculated from Principal, Rate & Term. Adjust inputs on the left, or click [Solve Principal for $2,400/mo…] (Goal Seek).`

#### Step 2 — Pinning baseline, dragging scenario controls, and printing (`Ctrl+P`)
3. **Action:** Jo clicks **`[Pin Current as Baseline]`** (locks `$450,000 · 6.25% · 30 yrs · $0 extra` as reference), then moves focus (`Tab`) to **Extra Monthly Principal** (`Inputs!C7`), presses `Right Arrow` three times (`+$100/step` -> **`$300`**) or types `300` (`Enter`).
4. **Instant scenario comparison (`3 ms` recalc across all 360 rows):**
   - Every KPI card shows the live figure beside its delta against Baseline (`-$154,652.46` Total Interest; `-7 yrs 7 mos` Payoff Time).
   - Screen reader announces: *"Extra Monthly Principal set to 300 dollars. Payoff shortened by 7 years 7 months to 269 months; Total Interest reduced by 154,652 dollars and 46 cents."*
5. **Action:** Jo presses `Ctrl+P` to open the **Print & Export Sheet Drawer** right alongside the App stage. Instead of printing 360 raw rows across 11 pages, App View defaults to **Executive 2-Page Brief** (Page 1: Inputs, Baseline vs. Scenario Comparison, Balance & Interest Curve; Page 2: Annualised 30-Row Schedule, with a toggle `[ ] Append full 360-month schedule (+6 pages)`). Jo clicks **`[Print / Save PDF]`**.

#### Mockup M-07: `Loan.xlsx` in App View — Baseline vs. Scenario Delta & Executive Print Preview

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Loan.xlsx                [1 Classic] [2 Inspector] [3 Note] [4 Map] [5 App*]  [Ctrl+P*] │
├─────────────────────────────────────────────────────────────┬────────────────────────────────┤
│ LOAN AMORTISATION CALCULATOR     [Pinned: Base ($0 Extra)]  │ PRINT PREVIEW (2 Pages · A4)   │ (1)
│                                                             │ ┌────────────────────────────┐ │
│ ┌─ BORROWER INPUTS ────────┐ ┌─ OUTCOMES vs BASELINE ─────┐ │ │ Page 1 of 2   [cockpit-lt] │ │
│ │ Loan Principal           │ │ Scheduled Base PMT  [Lock] │ │ │ LOAN SCENARIO SUMMARY      │ │ (2)
│ │ [$450,000          ]     │ │ $2,770.74/mo  (+ $300 ext) │ │ │ Principal: $450k @ 6.25%   │ │
│ │                          │ ├────────────────────────────┤ │ │ Base: $2,770.74 · 30.0 yrs │ │
│ │ Annual Rate              │ │ Total Interest Paid        │ │ │ +$300: $3,070.74 · 22.4 yr │ │
│ │ [6.25%     ] [─●───────] │ │ $392,814.12  ▼ -$154,652!  │ │ │ Interest Saved: $154,652   │ │
│ │                          │ ├────────────────────────────┤ │ │ [Balance Curve Vector SVG] │ │
│ │ Term (Years)             │ │ Actual Payoff Horizon      │ │ └────────────────────────────┘ │
│ │ (15 yr) (20 yr) (•30 yr) │ │ 22 yrs 5 mos ▼ -91 months  │ │ ┌────────────────────────────┐ │
│ │                          │ └────────────────────────────┘ │ │ Page 2: Annual Rollup (30) │ │ (3)
│ │╔Extra Monthly Principal═╗│ ┌─ BALANCE CURVE (360 mos) ──┐ │ │ Yr 1..23 Ending Balances   │ │
│ │║[$300              ]    ║│ │ $450k ╲.. (dashed = Base)  │ │ └────────────────────────────┘ │
│ │║[────●─────────] $0..1k ║│ │ $225k   ╲  ...             │ │ [x] Show Scenario Delta        │
│ │╚════════════════════════╝│ │    $0 ────╲______ (m269)   │ │ [ ] Full 360-row table (+6p)   │ (4)
│ └──────────────────────────┘ └────────────────────────────┘ │ [ ] Formula Audit Appendix     │
│ [▸ Schedule: Showing Annual Rollup (23 active yrs) · Toggle All 360 Months (PMT/IPMT/PPMT)]  │ (5)
│   Need grid edits? Switch to [Classic (Ctrl+1)] — all changes sync to Loan!B4:H371.          │
├─────────────────────────────────────────────────────────────┴────────────────────────────────┤
│ AUTO · 360 rows in 3 ms │ 0 Errors │ Formulas Protected in App View │ Print: 2 pp · 100%     │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-07:**
- **(1) Consumer-Grade App Stage:** Automatically hides grid coordinates and the formula bar while keeping `[1 Classic]` one shortcut away for builders.
- **(2) `[Lock]` Outcome Cards with Baseline Deltas:** Shows `$392,814.12` alongside `▼ -$154,652!` (`green-ink` `#2E6E5B` positive-outcome delta with downward triangle) and protects `-PMT`/`IPMT`/`PPMT` from accidental typing.
- **(3) Live Paginated Print Preview (`cockpit-light` paper enforced):** Always renders crisp dark-on-white vector typography regardless of whether Jo's screen uses `cockpit-dark`.
- **(4) Explicit Schedule Pagination Choice:** Prevents accidental 8-page paper waste while keeping all 360 monthly rows one checkbox away.
- **(5) Rollup Toggle over `A11:H371`:** Summarises `PMT`/`IPMT`/`PPMT`/`Ending_Bal` by calendar year without mutating the 360-row table in `Loan.xlsx`.

---

### 2.6 Ade loads an add-in in the standalone Formula Explorer, tests a function, then uses it from a cell in `Budget`

**Context:** Ade develops `FinKit.xll` (a C++/Rust XLL add-in exposing SaaS unit-economics UDFs, including `FK.SAAS_LTV(arpu, gross_margin, monthly_churn, [discount_rate_yr])`). Before touching any production workbook, he launches the standalone stripped build **`DNA Calc - Formula Explorer`**.

#### Step 1 — Launching standalone `DNA Calc - Formula Explorer` and loading `FinKit.xll`
1. **Cold launch (`35 ms`, no workbook):** The window opens with Strand's **`dot-long` identity mark (`[•━]`)** and title **`DNA Calc - Formula Explorer`**. There are no sheet tabs, no file-save nag, and no grid—just a precision 3-pane workbench: **Synthetic Inputs Shelf** (left), **Formula & Live AST Decomposition** (center), and **Format, Conditional Rules & Boundary Matrix** (right).
2. **Loading the add-in:** Ade presses `Ctrl+K`, types `addin`, selects **`Load Add-in (.xll / .dna / .wasm)…`**, and picks `FinKit.xll`.
   - A yellow/amber **Developer Sandbox Strip** notes: `Unsigned local XLL: FinKit.xll (8 UDFs, thread-safe, non-volatile) · [Trust for Session (Enter)]`. Ade hits `Enter`.
   - Instantly, the left shelf lists all 8 exported functions with parameter types, help strings, and volatility flags read from XLL registration metadata.
3. **Testing `FK.SAAS_LTV` and catching a boundary bug (`M-08`):**
   - Ade clicks `FK.SAAS_LTV` to seed the scratchpad: Explorer auto-creates four typed synthetic input bindings (`arpu = 420`, `gross_margin = 0.82`, `monthly_churn = 0.012`, `discount_rate_yr = 0.10`) and evaluates `=FK.SAAS_LTV(arpu, gross_margin, monthly_churn, discount_rate_yr)` -> **`17118.4481`**.
   - In the **Number & Conditional Formatting** pane, Ade enters format `"$#,##0"` -> **`$17,118`** and tests a conditional rule `Value < 3 * 3900` (`3× CAC`) -> verifies it stays green (`4.39x CAC`).
   - In the **Boundary Sweep Matrix** (auto-generated edge-case table probing `0`, negative, `NA()`, empty, and array inputs!), Ade spots **`Case #3: monthly_churn = 0 -> [! #DIV/0!]`**! He edits his C++ guard, recompiles, and hits **`Ctrl+Shift+R` (`Hot-Reload XLL`)**—something Excel makes painful because Excel locks `.xll` DLL handles on disk! DNA Calc shadow-copies `.xll` binaries on load so Ade recompiles and reloads in **45 ms**, seeing `Case #3` cleanly return `#NUM!` (with XLL diagnostic `"monthly_churn must be > 0"`).

#### Mockup M-08: Standalone `DNA Calc - Formula Explorer` — Workbookless UDF, AST & Boundary Bench

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [•━] DNA Calc - Formula Explorer (No Workbook)           [Add-ins: FinKit.xll ↻]    [Ctrl+K] │ (1)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ fx =IFERROR(FK.SAAS_LTV(arpu, gross_margin, monthly_churn, discount_rate_yr), 0)             │
│    Result: $17,118  (raw: 17118.448129 · eval: 0.04 ms · Thread-Safe XLL · Non-volatile)     │ (2)
├──────────────────────────┬───────────────────────────────────┬───────────────────────────────┤
│ SYNTHETIC INPUTS         │ LIVE AST & SUB-EXPRESSION TREE    │ FORMAT, COND-FMT & SWEEP      │
│                          │                                   │                               │
│ arpu           [   $420] │ ▼ IFERROR( ... , 0 )  ->  17118.4 │ Number Format:                │
│ gross_margin   [  82.0%] │   ├─▼ ◈ FK.SAAS_LTV(...) -> 17118 │ ["$#,##0"        ] ──► $17,118│ (3)
│ monthly_churn  [   1.2%] │   │   ├─ arpu          = 420      │ ["$#,##0.00;($#)"] ──► ...    │
│ discount_rate  [  10.0%] │   │   ├─ gross_margin  = 0.82     │                               │
│                          │   │   ├─ monthly_churn = 0.012    │ Cond-Fmt Rule Preview:        │
│ [+ Add Scalar / Array]   │   │   └─ discount_rate = 0.10     │ [Value < 11700] -> False (OK) │ (4)
├──────────────────────────┤   └─ fallback          = 0        ├───────────────────────────────┤
│ FINKIT.XLL (8 functions) │                                   │ AUTO BOUNDARY PROBE (6 cases) │
│ • FK.SAAS_LTV  [Selected]│ Signature & Portability:          │ ✓ Base inputs      -> $17,118 │
│ • FK.COHORT_IRR          │ (arpu:Num, gm:Num, churn:Num,     │ ✓ Array arpu {380,420}->Spill │
│ • FK.NDR_WATERFALL       │  [disc=0.08]:Num) -> Num          │╔! churn = 0 (raw)  -> #NUM!  ╗│ (5)
│ [Ctrl+Shift+R Hot-Reload]│ Saved in .xlsx as _xll.FK.SAAS_LTV│ ✓ IFERROR wrapped  ->      $0 │
└──────────────────────────┴───────────────────────────────────┴───────────────────────────────┘
```

**Callouts for M-08:**
- **(1) Standalone `dot-long` Shell (`[•━]`):** Zero workbook chrome; shadow-copied `[Add-ins: FinKit.xll ↻]` allows instant recompilation without DLL file-locking.
- **(2) Dual Raw + Formatted Readout:** Shows IEEE-754 float precision (`17118.448129`), formatted display (`$17,118`), execution time (`0.04 ms`), and volatility/thread-safety metadata.
- **(3) Live Number Format Tester:** Previews Excel format codes across positive, negative, zero, and text states before touching a sheet.
- **(4) Conditional Formatting Tester:** Evaluates rule formulas against the live output.
- **(5) Automatic Boundary Probe:** Feeds `0`, negative, array spill, and `#N/A` inputs into the expression in isolated sandbox evaluations so add-in authors catch crashes and edge errors immediately.

#### Step 2 — Using `FK.SAAS_LTV` from a cell in `Budget.xlsx`
4. **Action:** Ade clicks **`[Copy Formula for Workbook]`**, switches to DNA Calc with `Budget.xlsx` open on `Summary!C11`, presses `F2`, types `=FK.SAAS_LTV(`, and picks `Inputs!C8` (`$420`), `Summary!C6` (`82.0%`), `Inputs!C7` (`1.2%`), `, 0.10)` -> `Enter`.

#### Mockup M-09: Using `FK.SAAS_LTV` in `Budget.xlsx` (`Summary!C11`) — Mid-Edit Signature & Portability Badge

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx              [1 Classic*] [2 Inspector] [3 Note] [4 Map] [5 App] [Add-in: 1]│
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Summary!C11  │ fx =FK.SAAS_LTV([①Inputs!C8], [②C6], [③Inputs!C7], 0.10)█                     │ (1)
│ Editing [F2] │    ┌─ ◈ FK.SAAS_LTV(arpu, gross_margin, monthly_churn, [discount_rate_yr]) ──┐│
│              │    │  Preview: $17,118.45 -> "$#,##0" = $17,118  │ FinKit.xll (Portable cache││ (2)
├────┬─────────┴────┴─────────┬──────────────┬──────────────┬────┴────────────┬───────────────┘
│    │ B (KPI Metric)         │ C (Base FY27)│ D (Downside) │ E (Notes / Bench│ F              │
├────┼────────────────────────┼──────────────┼──────────────┼─────────────────┼────────────────┤
│  6 │ ┌②Gross Margin (%)────┐│        82.0% │        79.5% │ SaaS target >78%│                │ (3)
│  7 │ └─────────────────────┘│              │              │                 │                │
│  8 │ Operating EBITDA       │     $146,952 │    ($612,400)│ +2.1% margin    │                │
│  9 │ Blended CAC            │       $3,900 │       $4,250 │ S&M / New Logos │                │
│ 10 │ Payback Period (mos)   │         11.3 │         14.1 │                 │                │
│ 11 │║Cohort LTV (10% disc) ║│╔◈ $17,118═══╗│     $11,042  │ LTV:CAC = 4.4x  │                │ (4)
├────┴────────────────────────┴┴─────────────┴──────────────┴─────────────────┴────────────────┤
│ [≡ 3 Sheets] Inputs [①③] │ Monthly │ Summary* [②]                                            │ (5)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ EDIT (Point: F2 cycles) │ Live Preview: $17,118 │ ◈ 1 Add-in (FinKit.xll) · Excel-safe cache │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-09:**
- **(1) Notched Reference Tokens (`①②③`):** Strand Refinement #1 in action—each picked cell carries a matching cool-palette token and numbered geometric corner mark (`①` `Inputs!C8`, `②` `Summary!C6`, `③` `Inputs!C7`).
- **(2) Live Preview Seam Before `Enter`:** Displays the evaluated result (`$17,118`) and portability note *while Ade is still typing*!
- **(3) On-Sheet Notched Highlight (`②`):** `C6` (`82.0%`) displays the `②` double-tick border frame matching `[②C6]` in the Formula Bar.
- **(4) Add-in Provenance Glyph (`◈`):** Marks `C11` as powered by external add-in `FinKit.xll`.
- **(5) Cross-Sheet Reference Badges on Sheet Tabs:** `Inputs [①③]` shows Ade at a glance which off-sheet tabs hold the other picked precedents!

---

### 2.7 The agent proposes a change to `Budget`; Maria reviews it, accepts part of it, and undoes the rest

**Context:** Maria is finalizing `Budget.xlsx` alongside an AI agent connected through DNA Calc's Agent Bridge (`Ctrl+J`). She asks: *"In Downside, freeze R&D hiring after June, raise Downside monthly churn from 2.4% to 2.8%, and cap monthly S&M at $140k."*

#### Step 1 — Agent proposes a staged 3-hunk Change-Set (never mutating silently)
1. **Staged proposal (`310 ms`):** Instead of overwriting cells directly, the agent submits a **3-Hunk Staged Change-Set (`Δ Agent #4`)**.
   - **Mast & Impact Strip:** An `amber`-bordered review bar appears at the top of the Stage showing **Workbook Output Impact** across pinned summary KPIs *before a single cell is committed*:
     - `Summary!C8 (Base EBITDA): $146,952 → $412,056 (+$265,104) [⚠ Unexpected Base shift!]`
     - `Summary!D8 (Downside EBITDA): ($612,400) → ($389,120) (+$223,280)`
   - **Instant catch:** Because DNA Calc automatically diffs key outputs across scenarios, Maria spots in **two seconds** that `Base EBITDA` moved by `+$265,104` even though she only asked for `Downside` changes!

#### Step 2 — Reviewing hunk-by-hunk, rejecting Hunk 3, accepting Hunks 1 & 2
2. **Inspecting the 3 Hunks (`J` / `K` keyboard navigation between hunks):**
   - **Hunk 1 (`Monthly!I17:N17` — R&D Payroll Jul..Dec, 6 R1C1-equivalent cells collapsed as 1 formula run):** Wraps Downside freeze `=IF(Inputs!$C$2="Downside", H17, ...)` -> Maria presses **`A` (`Accept Hunk 1`)**.
   - **Hunk 2 (`Inputs!D7` — Downside Monthly Churn):** `2.4% → 2.8%` -> Maria presses **`A` (`Accept Hunk 2`)**.
   - **Hunk 3 (`Monthly!C18:N18` — S&M Spend Jan..Dec, 12 cells):** Agent wrote `=MIN(140000, C11 * INDEX(Inputs!$C$14:$D$14, ...))` unconditionally across *both* scenarios instead of scoping the `$140k` cap to `Downside`! Maria presses **`R` (`Reject Hunk 3`)** (with optional 1-key `[Reply to Agent: Scope cap to Downside only]`).
   - Immediately in the preview, `Base EBITDA` snaps back to **`$146,952` (`Δ $0`)** while `Downside EBITDA` reflects Hunks 1 & 2 (`-$518,900`).

#### Mockup M-10: `Budget.xlsx` — Reviewing Agent's 3-Hunk Staged Change-Set & Output Impact Seam

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx [Δ Agent: 3 Hunks]  [1 Classic*] [2 Inspector] [3 Note] [4 Map] [Ctrl+J Agt]│
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ Δ AGENT PROPOSAL #4: "Downside Q3 R&D freeze, 2.8% churn & $140k S&M cap"    [Accept All]    │
│ KPI IMPACT: Base EBITDA $146,952 ──► $412,056 [⚠ +$265k!] │ Downside ($612k) ──► ($389k)     │ (1)
├───────────────────────────────────────────────────────────┬──────────────────────────────────┤
│ STAGED DIFF ON SHEET (Monthly!C16:N20 · Hatched = Ghost)  │ HUNK REVIEW RAIL (J/K, A/R)      │
│                                                           │                                  │
│ Row │ Line Item      │ Jul-27 (I) │ Aug-27 (J) │ FY27 (O) │ [✓] Hunk 1 · Monthly!I17:N17 (6) │ (2)
│  16 │ Support Cost   │    $64,000 │    $64,000 │ $768,000 │     R&D Q3 freeze in Downside    │
│  17 │ R&D Payroll    │▒▒ $171,360▒│▒▒ $171,360▒│$2,056.3k │     [A: Accepted] [Undo]         │
│     │ └─ Δ Hunk 1    │ =IF(Inputs!$C$2="Downside",H17,..) │                                  │
│  18 │╔S&M Spend     ╗│▒▒ $140,000▒│▒▒ $140,000▒│$1,680.0k │ [✓] Hunk 2 · Inputs!D7 (1 cell)  │
│     │║was:          ║│   $164,892 │   $168,655 │$1,945.1k │     Downside Churn: 2.4% -> 2.8% │
│     │╚═► Δ Hunk 3 ══╝│ =MIN(140000, C11*Inputs!C14)       │     [A: Accepted] [Undo]         │
│  20 │ Oper. EBITDA   │▒▒  $38,132▒│▒▒  $42,795▒│ $412,056 │                                  │
│                      │ (Base shifted by Hunk 3!)          │╔[?] Hunk 3 · Monthly!C18:N18(12)╗│ (3)
│                                                           │║    Unconditional MIN(140000,..)║│
│                                                           │║    ⚠ Alters Base Case (+$265k) ║│
│                                                           │║    [A: Accept] [R: Reject*]    ║│ (4)
│                                                           │╚════════════════════════════════╝│
├───────────────────────────────────────────────────────────┴──────────────────────────────────┤
│ REVIEW SEAM ·Workbook untouched until committed │ [Commit 2 Accepted & Drop 1 (Enter)] [Esc] │ (5)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-10:**
- **(1) Automatic KPI Impact Diff:** Surfaces unintended side-effects (`Base EBITDA +$265k!`) across summary outputs even when the edited formulas sit on another sheet (`Monthly`).
- **(2) R1C1 Formula-Run Hunk Grouping:** Instead of forcing Maria to review 18 separate cell diffs (`I17..N17` and `C18..N18`), identical R1C1 formula changes collapse into single logical hunks (`(6)` and `(12)` cells).
- **(3) Causality Link Between Hunk & Impact:** Selecting Hunk 3 highlights why `Base EBITDA` moved (`Unconditional MIN(140000,...)`).
- **(4) Keyboard Triage (`A` / `R`):** Single-key accept/reject per hunk.
- **(5) Zero Premature Mutation:** `Esc` discards the entire proposal cleanly; `Enter` commits only the 2 accepted hunks.

#### Step 3 — Partial undo & permanent provenance trace
3. **Action:** Maria presses `Enter` to commit Hunks 1 & 2 (`Downside EBITDA = -$518,900`, `Base EBITDA = $146,952`). Five seconds later she decides to keep `Downside Churn` at `2.4%` after all, and presses **`Ctrl+Z`** once.
4. **Granular Undo (`M-11`):** Because multi-hunk agent commits push **per-hunk atomic entries nested under a collapsible group** in the Undo History, a single `Ctrl+Z` cleanly reverts **Hunk 2 (`Inputs!D7: 2.8% -> 2.4%`)** while preserving **Hunk 1 (`Monthly!I17:N17` R&D freeze)**! Pressing `Ctrl+Z` a second time would undo Hunk 1; `Ctrl+Y` redoes Hunk 2.

#### Mockup M-11: Granular Undo Stack & Session Audit Trail After Partial Revert

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx              [1 Classic*] [2 Inspector] [3 Note] [4 Map] [5 App] [↺ History]│
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Inputs!D7    │ fx 2.4%                      │ UNDO / PROVENANCE STACK [Ctrl+Shift+Z]         │
│ Reverted     │ Constant · Downside Churn    │ ────────────────────────────────────────────── │
├────┬─────────┴──────────────┬───────────────┤ ↷ [Redone if Ctrl+Y] Agent #4 · Hunk 2         │ (1)
│    │ B (Assumption)         │ C (Base)│D(Dn)│   Inputs!D7: 2.4% -> 2.8% (Reverted via Ctrl+Z)│
├────┼────────────────────────┼─────────┼─────┤ ● [Current State]    Agent #4 · Hunk 1 [Kept]  │ (2)
│  6 │ Monthly Logo Growth    │    3.5% │1.5% │   Monthly!I17:N17 (6 cells): R&D Q3 freeze     │
│  7 │║Monthly Churn         ║│    1.2% │╔2.4%╗   Prompt: "In Downside, freeze R&D hiring..."  │ (3)
│  8 │ ARPU ($/mo)            │    $420 │$380 │ ○ [14:08] Maria · Summary!C8 Notebook prose    │
│  9 │ Cloud COGS (% Rev)     │   18.0% │20.5%│ ○ [14:02] Ade   · Summary!C11 =FK.SAAS_LTV(..) │
├────┴────────────────────────┴─────────┴─────┴────────────────────────────────────────────────┤
│ AUTO · Recalc 3 ms │ Base EBITDA: $146,952 · Downside: ($464,120) │ Undo ready (Ctrl+Z)      │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-11:**
- **(1) Redo Branch Preserved:** Undoing Hunk 2 (`Ctrl+Z`) keeps it on the redo branch (`Ctrl+Y`) without losing Hunk 1.
- **(2) Kept Agent Hunk:** Hunk 1 remains active and inspectable across any view.
- **(3) Session Provenance:** Associates the agent prompt and rejected hunk count in the session audit log (stored optionally in `customXml`, never polluting Excel cells).

---

### Coverage verification: make-or-break moments across Section 2

| Required Moment | Where Walked Through & Verified |
|---|---|
| **First run & empty states** | `2.1` (DNA Calc cold start), `2.4` (blank Model Map), `2.6` (standalone workbookless Formula Explorer) |
| **Feature discovery** | `2.1` (automatic Triage Strip & `[+9 Hidden]` pill), `2.2` (`[Alt+L Lens]` & `@` inline tokens), `2.3` (`2 cells selected · Δ` prompt in Strip) |
| **Loading & calculating feedback** | `2.1` (`55 ms` shell, `190 ms` cached paint, streaming dep index bar), `2.2` (`6 ms` Notebook ripple), `2.4` (`140 ms` topological wire pulse) |
| **Switching views (anchor continuity)** | `2.1` (`Summary_P&L!F19` Classic ↔ Inspector ↔ `Gas_Daily_2001!AT142`), `2.2` (`Summary!C8` Classic → Notebook halo), `2.4` (Model Map → Classic) |
| **Error, warning & refusal moments** | `2.1` (Macro + missing-link quarantine), `2.3` (clipped-table warning), `2.4` (cycle refusal -> Director picker), `2.5` (App formula-clobber refusal), `2.7` (KPI side-effect warning) |
| **Undo & recovery** | `2.2` (cross-view scenario undo), `2.3` (2-step audit revert), `2.7` (hunk-granular `Ctrl+Z` / `Ctrl+Y` over agent change-set) |
| **Copy/paste across views & external apps** | `2.2` (Notebook prose+table to email HTML/Markdown; live ref paste inside DNA Calc), `2.3` (Inspector discrepancy card export), `2.6` (Explorer -> `Budget` cell) |
| **Hover, focus & selection feedback** | `2.1` (`amber` double frame + `[!]` error glyph), `2.4` (port & wire preview), `2.6` (`①②③` notched reference tokens + sheet-tab precedent pills) |
| **Perceived speed** | Explicit budgets demonstrated: `3–9 ms` recalc on `Budget`/`Loan`/`Sales`, `11 ms` virtualised jump on 285k-cell `Gas_Daily_2001`, `45 ms` XLL hot-reload |
| **Accessibility** | `2.1` & `2.5` (`NVDA`/screen-reader live announcements, `150%` semantic zoom), `2.4` & `2.7` (100% keyboard block creation `N`/`G` & hunk review `J`/`K`/`A`/`R`), non-color shape cues throughout |
| **Excel user wins vs. surprises** | `2.1` (no startup modal hostage-taking; root-error folding; `VeryHidden` sheets visible), `2.3` (catches truncated `SUMIFS` with zero `#` error), `2.4` (cell-backed names visible in Excel), `2.6` (non-locking XLL hot-reload) |


---

## 3. The conceptual model

A multi-view spreadsheet collapses into confusion if each view invents its own private objects, selection rules, or undo history. DNA Calc rests on **one shared conceptual model** where views are live projections over a single Excel workbook graph.

### 3.1 What a user points at: the unified target hierarchy

Every selectable entity in DNA Calc has a canonical **Target Address** (Strand Tenet 7—what a human types in `Ctrl+G` or `Ctrl+K` is identical to what an AI agent sends):

1. **Cell (`Monthly!O20`):** The atomic storage and evaluation unit on a worksheet.
2. **Range or Dynamic Spill (`Monthly!C11:N11`, `Inventory_Model!C11#`):** A rectangular slice or dynamic-array spill anchored at its formula cell (`C11`).
3. **Block (`#Monthly.OpEx`, `#Loan.Schedule`):** A contiguous, structurally homogeneous region—such as a 12-column formula run sharing one R1C1 syntax tree plus its row/column labels—inferred by DNA Calc and confirmable by the user.
4. **Defined Name (`Stock.OnHand`, `Active_Scenario`, `FK_DiscountLambda`):** Workbook- or sheet-scoped Excel names, including `LAMBDA` definitions and `OFFSET` dynamic ranges.
5. **Table or Pivot (`Orders[NetAmount]`, `Pivot_RegionSKU`):** Native Excel `ListObject` structured tables (columns, headers, totals) and `PivotTable` caches/axes.
6. **Sheet Object (`Chart_Amortisation`, `ScrollBar1`):** Charts, form controls, and shapes anchored over a sheet or surfaced inside Notebook/Map/App cards.
7. **View Item (`@notebook/sec-1/prose-1`, `@app/card-interest`):** Presentation-only containers stored in `xl/customXml/dnaCalcViews.xml`. A View Item either *projects* one of targets `1–6` or holds pure annotation (such as `Source Serif 4` prose).

### 3.2 Selection and navigation that carry across views

DNA Calc tracks focus as an ordered pair: **`(ActiveModelTarget, OptionalViewItem)`**.
- **Projection resolution:** Pressing `Ctrl+1`..`Ctrl+5` never resets focus to `A1`. The destination view resolves `ActiveModelTarget` to its tightest visible representation: cell `Monthly!I17` in **Classic** maps to cell `Jul-27 · R&D Payroll` in **Notebook**'s Data Frame, to the `Monthly.OpEx` composite block in **Model map**, and to `I17`'s dependency node in **Inspector**.
- **Continuity feedback:** On arrival (`<100 ms`), the resolved target receives Strand's `amber` (`#FFB301`) focus outline and a single **150 ms cross-stage continuity halo** (`0 ms` static outline when `prefers-reduced-motion` is set) so the eye locks onto its place instantly.
- **When a pure View Item is focused:** If Maria's cursor sits inside a Notebook prose block when she switches to **Classic (`Ctrl+1`)**, focus lands on the nearest cell or `@chip` referenced in that paragraph (or the immediately following Data Frame's anchor cell).

### 3.3 What an edit means in a view that is not a grid

Authoring follows Strand Tenet 5 (**1-bit modeless truth: `SELECTED` vs `EDITING`**) and Principle 1 (**Backing-Sheet Primacy**):

- **Scalar input edit (App slider, Notebook driver cell, Model Map parameter box):** Writes the literal value directly to the bound cell (`Inputs!C8` or `Loan!C7`), triggering standard Excel dirty-set recalculation.
- **Formula edit in Model Map or Notebook (`F2` or wire drag):** Runs through the bidirectional **AST Formula-Name Presenter & Interpreter** (`LINKED_BOXES_SKIN.md`). Inside group `Stock.`, typing `OnHand + Inflow - Outflow` parses into an AST, resolves local names (`OnHand` -> `Stock.OnHand`) without touching string literals or Excel function names, and writes `=Stock.OnHand + Stock.Inflow - Stock.Outflow` to the backing cell.
- **Block-run formula edit:** Editing the formula column of a 12-month row in **Notebook** (`Monthly!C17:N17`) shows an explicit **Preview Seam** before commit: `[Updating 12 R1C1-identical cells: Monthly!C17:N17 · Press Shift+Enter for single cell]`. If row `17` contains a manual override at `I17`, DNA Calc refuses silent clobbering and asks whether to preserve or overwrite the exception.
- **Structural delete (`Delete` / `Backspace`):** Pressing `Delete` on a **cell/range in Classic** clears cell contents as in Excel. Pressing `Delete` on a **card/slice in Notebook or App** removes *only the presentation card* (`[V]`), leaving workbook cells untouched; pressing `Delete` on a **Model Map actor** prompts `[Remove from Map only (Enter)]` vs `[Delete backing cell & name (Shift+Delete)]` (with blast-radius preview if dependents exist).

### 3.4 Undo across views

One chronological **Workbook Undo/Redo Stack** (`Ctrl+Z` / `Ctrl+Y`, inspectable via `Ctrl+Shift+Z`) spans all five views and the docked Formula Explorer:
- Every entry is badged **`[M]` (Model mutation)** or **`[V]` (View/presentation edit)**.
- `Ctrl+Z` always undoes the user's last action regardless of which view they are currently in. If Maria edits `Inputs!D7` in Notebook, switches to Classic on `Summary`, and hits `Ctrl+Z`, `Inputs!D7` reverts immediately, `Summary` recalculates in `4 ms`, and the Strip confirms `Undid Inputs!D7 (2.8% → 2.4%) [Jump]` without yanking her viewport away from `Summary`.
- Pure navigation, scrolling, folding/expanding diagnostic trees, and Formula Explorer scratchpad experiments *never* pollute the Undo stack.

### 3.5 Errors, calculation state, Directors, and live data as visible things

State is never hidden in modal dialogs or ambiguous colors; every state uses one Strand semantic channel paired with a distinct geometric glyph:

| Underlying Excel State | Visual & Non-Color Signal Across All Views |
|---|---|
| **Evaluation error (`#DIV/0!`, `#REF!`, `#N/A`, `#VALUE!`, `#NAME?`, `#SPILL!`, `#NUM!`, `#CALC!`)** | `red` (`#D02A23` / `red-ink`) border + `[!]` prefix badge + 1-hop/root provenance subtitle in Bridge; bubbles up onto collapsed parent blocks (`[! 390 err]`). |
| **Active focus & running calculation** | `amber` (`#FFB301`) focus frame; topological recalc ripple (`140 ms` settle) comparing pre/post values on `AfterCalculate`. |
| **Stale values (`Manual` calc mode)** | Desaturated hatched corner mark `[stale]` on dirty dependents + `amber` `[F9 Recalc Needed]` pill in Strip. |
| **External, volatile & live (`RTD`, `.xls` links, `OFFSET`/`INDIRECT`/`NOW`, XLL UDFs)** | `signal` orange (`#FF8001` / `prov-ext` `#A35400`) + diamond glyph `◈`; live `RTD` topics optionally surface the **Events Queue** lane in Inspector/Model Map. |
| **Calculation Directors (Model Map & Strip)** | Honest readout of Excel's own evaluation mode: `Recalc` (Automatic), `Manual`, `Iteration (Max N, Δ ε)`, `Ticks (Iter=1)` with `[z⁻¹]` self-loops, `Sweep` (`{=TABLE}`), and `Events` (`RTD`). |

### 3.6 Showing, reviewing, accepting, and tracing agent actions

An AI agent shares the exact Target Address grammar (`3.1`) and declarative skin API (`Section 7`), but **never mutates the live model directly**:
1. **Stage:** Agent tool calls compile into a **Staged Change-Set (`Δ Agent #N`)** overlaid on the current workbook in an isolated copy-on-write fork.
2. **Diff & Impact:** Contiguous R1C1-identical edits collapse into logical **Hunks** rendered with `sage-400` hatched preview seams and `Recursive` casual-axis ghost text. DNA Calc automatically evaluates pinned/summary KPIs across scenarios to flag unintended blast radius (`M-10`).
3. **Review:** Users navigate hunks (`J`/`K`), inspect formulas in Explorer (`E`), and accept/reject individually (`A`/`R`) or in bulk (`Ctrl+Enter` / `Esc`).
4. **Commit & Trace:** Accepted hunks push hunk-atomic `[M]` entries onto the Undo stack under a collapsible `Agent #N` group (`M-11`), recording timestamp, prompt summary, and accepted/rejected hunk counts in session provenance.

---

## 4. The views and the Formula Explorer

All five views sit inside the **One Shell** (`Mast` + `Bridge` + `Stage` + `Strip`), honour **Formula Lens (`Alt+L`)**, and switch in `<100 ms` via `Ctrl+1`..`Ctrl+5` or the Command Palette (`Ctrl+K`). Touch and pen are deferred for v1, though we note where their natural affordances hook in cleanly later.

---

### 4.1 Classic (`Ctrl+1`)

**Purpose:** High-velocity grid authoring and inspection with 100% Excel muscle-memory conformance (Appendix A) and Strand's calm visual precision.

- **Gestures & keyboard model:** Full Excel keyboard parity—arrows, `Ctrl+Arrows` data-edge jumps, `End` mode sequences, `F2` (`Edit` ↔ `Enter` ↔ `Point`), `F4` (`$A$1` cycling & repeat-last-action), `F9` (recalc / sub-expression eval with `Esc` revert), `Ctrl+Enter` multi-cell fill, `Alt` mnemonic key-tips along the compact Command Strip, fill-handle drag/double-click, border drag-move (`Ctrl` copy, `Shift` insert), and notched `①②③④` range-picking across sheets. *Future touch/pen note:* Smooth inertial two-finger pan and stylus tap-to-pick ranges during `F2` point mode.
- **Entering & leaving:** Default view for unannotated workbooks; `Ctrl+1` enters from any view keeping `ActiveModelTarget` centered; `Ctrl+2..5` leaves while preserving selection.
- **Workbook suitability:**
  - **Suits best:** `Budget` (fast tabular block building), `Sales` (virtualised scrolling/filtering over `Orders`), `Loan` (inspecting the 360-row schedule), and `Stock` / `Inherited` for surgical cell-level fixes.
  - **Does not suit alone:** Understanding `Inherited`'s 40-sheet dependency tangle (`Inspector` wins), presenting `Budget` as prose (`Notebook` wins), sketching `Stock`'s feedback topology (`Model map` wins), or handing `Loan` safely to Jo (`App` wins).

#### Mockup M-12: Classic View Full-Screen Reference (`Budget.xlsx` — Mid-Edit Cross-Sheet Range Picking)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx              [1 Classic*] [2 Inspector] [3 Note] [4 Map] [5 App]   [Ctrl+K] │ (1)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Alt] File · Home · Insert · Format · Data · Audit │ [$#,##0] [%] [.00] │ [Provenance: ON]   │ (2)
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Monthly!C11  │ fx =[①C7] * [②Inputs!$C$8]█                                      [Alt+L Lens] │ (3)
│ Point [F2]   │    Lens preview: Ending_Customers[Jan-27] * ARPU  ──►  $515,760               │
├────┬─────────┴──────────────┬──────────────┬──────────────┬─────────────────┬────────────────┤
│    │ B (Cohort & Revenue)   │ C (Jan-27)   │ D (Feb-27)   │ E (Mar-27)      │ O (FY27 Total) │
├────┼────────────────────────┼──────────────┼──────────────┼─────────────────┼────────────────┤
│  4 │ Opening Customers      │ c      1,200 │        1,228 │           1,256 │          1,200 │ (4)
│  5 │ (+) New Logos          │           42 │           43 │              44 │            579 │
│  6 │ (-) Churned Logos      │          (14)│          (15)│             (15)│           (237)│
│  7 │ Ending Customers       │┌①──────1,228┐│        1,256 │           1,285 │          1,542 │ (5)
│  8 │                        │└────────────┘│              │                 │                │
│ 10 │ REVENUE ($ USD)        │              │              │                 │                │
│ 11 │║Subscription Revenue  ║│╔═$515,760═══╗│     $527,520 │        $539,700 │     $6,946,800 │ (6)
│ 13 │ Cloud COGS (18.0%)     │      $92,837 │      $94,954 │         $97,146 │     $1,250,424 │
│ 14 │ Gross Profit           │     $422,923 │     $432,566 │        $442,554 │     $5,696,376 │
├────┴────────────────────────┴──────────────┴──────────────┴─────────────────┴────────────────┤
│ [≡ 3 Sheets] Inputs [② $420] │ Monthly* [①] │ Summary                        [Frozen: Col B] │ (7)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ POINT (F2->Edit · F4->$ lock) │ Preview: $515,760 │ Sum: $515,760 · 0 Errors │ Work · 100%   │ (8)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-12:**
- **(1) Petrol Mast:** Consistent across all 5 views.
- **(2) Single-Line Command Strip (`30px`):** Replaces Excel's tall Ribbon; pressing `Alt` lights crisp single-letter badges (and accepts multi-key Excel `Alt, H, V, S` sequences silently!).
- **(3) Formula Bar + Live Lens Preview:** Shows raw formula tokens `[①C7]` and `[②Inputs!$C$8]` alongside evaluated preview `$515,760`.
- **(4) Subtle Provenance Marker (`c` / `prov-const`):** `C4` (`1,200`) is visually distinct from formula cells `D4:E4` so hardcoded overrides jump out immediately.
- **(5) Notched Precedent Highlight (`①`):** Matches `[①C7]` by both cool hue (`teal-600`) and corner notch.
- **(6) Active Cell with Fill Handle:** `C11` ready for `Ctrl+Enter` or double-click fill across `C11:N11`.
- **(7) Off-Sheet Reference Peek on Sheet Tab:** `Inputs [② $420]` confirms the off-sheet value without leaving `Monthly`.
- **(8) Mode-Honest Strip:** Explicitly states `POINT` sub-mode (`F2->Edit · F4->$ lock`).

---

### 4.2 Inspector (`Ctrl+2`)

**Purpose:** Phyphox-inspired diagnostic instrument bench revealing dependency topology, topological evaluation order, root-error cascades, silent range clipping, volatile/RTD activity, and hidden state.

- **Gestures & keyboard model:** `Up`/`Down` walks error roots or volatile bottlenecks; `Left`/`Right` steps upstream precedents vs downstream dependents; `Space` peeks the backing grid slice inline; `Ctrl+Click` two targets to trigger **Discrepancy Waterfall** (`M-04`); `F9` triggers a profiled recalculation showing per-sheet/per-block evaluation milliseconds; `Enter` jumps to Classic at the highlighted cell. *Future touch/pen note:* Tap-hold to X-Ray a cluster's upstream fan.
- **Entering & leaving:** `Ctrl+2`, clicking the Strip's `[! N Errors]` pill, or clicking `[Open Inspector]` on a Triage banner. `Esc` or `Ctrl+1/3/4/5` returns to authoring with the selected diagnostic cell focused.
- **Workbook suitability:**
  - **Suits best:** `Inherited` (taming 40 sheets, 9 hidden, 417 errors, 6 `OFFSET` bottlenecks), `Sales` (catching clipped `SUMIFS` and dirty string keys), and auditing circular convergence on `Stock`.
  - **Less needed for:** `Loan` (simple 1-sheet linear schedule, except quick audit verification).

#### Mockup M-13: Inspector View Full-Screen Reference (`Inherited.xlsm` — Phyphox-Style Multi-Probe Bench)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Inherited.xlsm           [1 Classic] [2 Inspector*] [3 Note] [4 Map] [5 App]   [Ctrl+K] │
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Probe Target │ Gas_Daily_2001!AT142  │ fx =AR142 / AS142  ──► [! #DIV/0!] (Root #1 of 3)     │ (1)
├──────────────┴───────────────────────┴───────┬───────────────────────────────────────────────┤
│ 1. CALCULATION CHAIN & COST (Total: 412 ms)  │ 2. ROOT-CAUSE & ANOMALY RADAR                 │
│                                              │                                               │
│ Stage 0 Inputs & 2 Held Ext Links     0.4 ms │ [!] 3 Error Roots ──► 417 Broken Cells        │ (2)
│ Stage 1 Ref_Hubs + 6 OFFSET names ◈  18.6 ms │  •║Gas_Daily_2001!AT142 (#DIV/0!) ──► 390   ║ │
│ Stage 2 Gas_Daily_2001 (248k fmls)  364.0 ms │  • Transport_West!H88   (#REF!)   ──►  25     │
│         ██████████████████████████░░  (88%!) │  • Summary_P&L!D31      (#N/A)    ──►   5     │
│ Stage 3 Transport_West (19k fmls)    21.2 ms │                                               │
│ Stage 4 Summary_P&L (4.1k fmls)       7.8 ms │ [⚠] Structural & Hygiene Warnings             │ (3)
├──────────────────────────────────────────────┤  • 6 volatile OFFSET names dirty 184,220 cells│
│ 3. LIVE / EXTERNAL / MACRO INVENTORY         │  • 47 hardcoded overrides inside formula cols │
│ ◈ NYMEX_Settle_2001_10.xls  [Missing · Held] │    on Gas_Daily_2001 (e.g. AW412, AW419)      │
│ ◈ BasisAdj_v4_FINAL2.xls    [Missing · Held] │  • 9 Hidden sheets (2 xlSheetVeryHidden)      │
│ ⚙ VBA Module1 (3 procedures)[Quarantined]    │                                               │
├──────────────────────────────────────────────┴───────────────────────────────────────────────┤
│ 4. INLINE CELL PEEK (Gas_Daily_2001!AR140:AU144)                         [Jump Classic Enter]│ (4)
│  Row 142: AR142 = $184,200 │ AS142 = 0 (Constant!) │╔AT142 = [! #DIV/0!]╗│ AU142 = [! #DIV/0]│
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ AUTO · Profiled 304,812 fmls (412 ms) │ [!] 3 Roots │ ◈ 2 Missing Ext · 6 OFFSET │ 100%      │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-13:**
- **(1) Active Probe Header:** Focuses on root cell `Gas_Daily_2001!AT142`.
- **(2) Phyphox-Style Calc Chain Cost Meter:** Shows topological evaluation stages and pinpoints `Gas_Daily_2001` taking `88%` (`364 ms`) of recalculation time due to `Stage 1` `OFFSET` volatility.
- **(3) Structural Hygiene Scanner:** Detects hardcoded numbers breaking uniform formula columns (`AW412`, `AW419`), volatile `OFFSET` blast radius, and `VeryHidden` sheets.
- **(4) Zero-Switch Inline Peek:** Shows the 5-row neighborhood around `AT142` (`AS142 = 0`) right at the bottom of Inspector so Sam or Maria verifies the bug before even switching to Classic.

---

### 4.3 Notebook (`Ctrl+3`)

**Purpose:** Live reactive computational document (like Pluto, backed 100% by Excel) interleaving `Source Serif 4` prose, live `@cell` inline value chips, data frames over tables/time-series, inline charts, and scenario controls—arranged either down the page (`↓ Page`) or across as multi-stage pipelines (`→ Pipeline`).

- **Gestures & keyboard model:** `Up`/`Down` moves between document blocks; `Enter` edits prose or steps into a Data Frame; `@` inside prose invokes live cell/name autocomplete; `/` opens block-insert menu (`Prose`, `Sheet Block Slice`, `Chart`, `Control`, `Pipeline Row`); `Alt+L` toggles Formula Lens column rules. *Future touch/pen note:* Natural scrolling for executive reading and stylus margin annotations stored in the custom XML view layer.
- **Entering & leaving:** `Ctrl+3` synthesizes a reading-order outline automatically on unconfigured workbooks (`Inputs` → `Calculations` → `Summaries`), or loads saved Notebook layout from `dnaCalcViews.xml`. Leaving (`Ctrl+1/2/4/5`) preserves cell focus and saves prose seamlessly.
- **Workbook suitability:**
  - **Suits best:** `Budget` (board/director narrative over 3 sheets), `Loan` (borrower explanation + schedule), `Stock` (systems engineering lab notebook), and `Sales`'s `Report` summary (with `Orders` previewed as a folded 10-row Data Frame header rather than dumping 25,000 rows!).
  - **Declines / simplifies:** Raw `Inherited` (`Gas_Daily_2001` edge-to-edge 285k grid declines full inline expansion and offers block-summary slices instead—see Section 6).

#### Mockup M-14: Notebook View Full-Screen Reference (`Budget.xlsx` — Pipeline Layout Mode `→ Pipe`)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Budget.xlsx              [1 Classic] [2 Inspector] [3 Notebook*] [4 Map] [5 App] [Alt+L]│
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Monthly!O11  │ Lens: SUM(Subscription_Rev[Jan..Dec])       │ [Flow: → Pipeline*] [+ Block /] │ (1)
│ $6,946,800   │ Excel: =SUM(Monthly!C11:N11)                │ Density: Reading                │
├──────────────┴───────────────────────────────────────────────────────────────────────────────┤
│  FY27 SaaS Revenue-to-EBITDA Pipeline                                                        │
│  Prose: Growth compounds at [3.5%/mo] net of [1.2%] churn, crossing cash breakeven in May.   │ (2)
│                                                                                              │
│  ┌─ STAGE 1: INPUTS ─────┐   ┌─ STAGE 2: MONTHLY ENGINE (C:N) ───┐   ┌─ STAGE 3: FY27 KPIs ─┐│ (3)
│  │ Scenario: [Base Case▾]│   │ Metric     │ Jan-27 │ ··· │ Dec-27│   │╔FY27 Revenue        ╗││
│  │                       │──►│ End Cust   │  1,228 │ ··· │  1,542│──►│║$6,946,800          ║││
│  │ Start Cust:     1,200 │   │ Subscript. │$515.8k │ ··· │$647.6k│   │╚════════════════════╝││
│  │ Logo Growth:     3.5% │   │ Cloud COGS │ $92.8k │ ··· │$116.6k│   │ Gross Margin   82.0% ││
│  │ Churn Rate:      1.2% │   │ Tot OpEx   │$427.1k │ ··· │$496.2k│   │ Oper. EBITDA $147.0k ││
│  │ ARPU ($/mo):     $420 │   │ EBITDA     │ ($4.2k)│ ··· │ $34.9k│   │ Cohort LTV   $17,118 ││ (4)
│  │ [▸ +8 assumptions]    │   │ Lens: Cust * ARPU - COGS - OpEx   │   │ ┌─ Rev vs EBITDA ──┐ ││
│  └───────────────────────┘   └───────────────────────────────────┘   │ │ ▁▂▃▄▅▆▇█ (Chart) │ ││ (5)
│                                                                      │ └──────────────────┘ ││
│                                                                      └──────────────────────┘│
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ [≡ 3 Sheets] Inputs ──► Monthly* ──► Summary [Pipeline linked]                               │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ AUTO · Live Pluto-style reactivity (4 ms) │ 0 Errors │ 3 Pipeline Stages │ Reading · 100%    │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-14:**
- **(1) Pipeline Layout Toggle (`→ Pipeline*`):** Arranges `Inputs` → `Monthly` → `Summary` left-to-right across the canvas so cross-sheet pipelines read naturally.
- **(2) Reactive Prose Header:** Inline chips update synchronously on every edit.
- **(3) Connected Stage Cards:** Directional connectors (`──►`) reflect real cross-sheet precedent flow, not static drawing arrows.
- **(4) Live Add-in Value Included:** Displays `Cohort LTV $17,118` from Ade's `FK.SAAS_LTV`.
- **(5) Inline Workbook Chart:** Renders the native Excel chart object inline inside Stage 3.

---

### 4.4 Model map (`Ctrl+4`)

**Purpose:** Hierarchical actor-and-wire schematic (`LINKED_BOXES_SKIN.md`, `names-in-motion.html`, `watershed.html`) drawn directly from workbook names, inferred blocks, precedents, and dependents—with explicit **Directors** (`Recalc`, `Ticks`, `Iteration`, `Sweep`, `Events`), `[z⁻¹]` state delays, and `IFS`/`SWITCH` state-machine badges.

- **Gestures & keyboard model:** `Arrow` keys traverse wired precedents/dependents; `Enter` looks inside a composite sheet/group (`Alt+Up` climbs breadcrumb); `F2` edits block formula via the short-name Interpreter; `N` creates a cell-backed named block; `Tab` spawns a wired downstream block; dragging from an output port `▸` to an input port rewrites the target's Excel formula with live Preview Seam; `Ctrl+Wheel` performs **semantic zoom** (Strand Tenet 8: swapping sheet districts → block summaries → individual formulas without shrinking text below `11px`). *Future touch/pen note:* Pinch-to-semantic-zoom and pen-drag port wiring.
- **Entering & leaving:** `Ctrl+4` enters at sheet/district overview (for multi-sheet files) or actor level (for compact models like `Stock`). `Ctrl+1` jumps straight to the backing cell row on the worksheet.
- **Workbook suitability:**
  - **Suits best:** `Stock` (feedback loops, `[z⁻¹]` delays, `IFS` operating modes, `Ticks` simulation), `Budget` (3-sheet `Inputs` → `Monthly` → `Summary` block pipeline), and high-level sheet-cluster architecture of `Sales` (`Orders` → `Report` + `Pivot`).
  - **Simplifies / declines cell-level:** `Loan` (collapses the 360 amortisation rows into one `Schedule [360 rows · running balance ↻]` composite block rather than 1,800 cell boxes) and `Inherited` (`40` sheet districts at Level 0; refuses raw 248k-cell explosion on `Gas_Daily_2001`).

#### Mockup M-15: Model Map Full-Screen Reference (`Stock.xlsx` — Directors, `[z⁻¹]` Loops & `IFS` Modal Block)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Stock.xlsx               [1 Classic] [2 Inspector] [3 Note] [4 Model map*] [5 App]      │
├──────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ Store.Mode   │ Lens: Lost > 0 ? "Stockout" : (OrderPlaced > 0 ? "Reorder" : "Normal")        │
│ "Reorder"    │ Excel: =IFS(Stock.Lost>0,"Stockout", Policy.OrderPlaced>0,"Reorder", 1,"Norm")│ (1)
├──────────────┴───────────────────────────────────────────────────────────────────────────────┤
│ Crumb: Stock.xlsx › Inventory_Model │ [Director: Ticks (Iter=1)] [▶ Run] [Step F9]   Tick 14 │ (2)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ┌─ Demand. ───────┐        ┌──[z⁻¹]──┐           ┌─ Store.Mode (Modal IFS) ───────────────┐ │
│  │ Rate        120 │───┐    │         ▼           │╔(Normal)══►(•Reorder*)──►(Stockout)   ╗│ │ (3)
│  └─────────────────┘   ├───►│ Stock.OnHand    340 │║ Guards: Order>0 │ Lost>0             ║│ │
│                        │    │ OnHand⁻ + In - Out  │╚══════════════════════════════════════╝│ │
│  ┌─ Supply. ───────────┼─┐  └────┬───────────┬────┘                    ▲                   │ │
│  │ LeadTime          3 │ │       │           │                         │                   │ │
│  │ DelayPipe {0,0,400} │ │       ▼           ▼                         │                   │ │
│  │ [z⁻¹] ──► Inflow  0 ├─┘  ┌─────────┐ ┌──────────┐   ┌─ Policy. ─────┴─────────────────┐ │ │
│  └─────────────────────┘    │Out: 120 │ │Lost:   0 │   │ ReorderPt: 280 [───●─────]      │ │ │ (4)
│         ▲                   └─────────┘ └──────────┘   │ OrderQty:  400 ──► Order:   400 ├──┘ │
│         └══════════════════════[Wire: Policy.OrderPlaced]───────────────────────────────┘    │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ HISTORY STRIP (Last 20 Ticks · OnHand ─── · Order ●)   500 ╲___╱‾‾╲___●  (live spark-trace) │ (5)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ITER (Max 1 · Δ 0.001) │ 0 Errors · 2 [z⁻¹] delays · 1 Modal IFS │ Zoom: 100% (Actor LOD)    │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-15:**
- **(1) Bidirectional Presenter:** Displays clean short-name boolean guards in the Lens bar and standard Excel `=IFS(...)` underneath.
- **(2) Breadcrumb + Active Director:** Shows hierarchy level and `Ticks` controls.
- **(3) `IFS` Modal Block Visualisation:** Recognises `=IFS(...)` over discrete string states (`"Normal"`, `"Reorder"`, `"Stockout"`) and renders a live 3-state pill diagram inside the actor (`•Reorder*` lit), inspired by `names-in-motion.html` and `watershed.html`.
- **(4) Embedded Form Slider + Fan-Out Port:** Adjusts `Policy.ReorderPoint` (`280`) directly on the canvas.
- **(5) Dockable History Strip:** Plots transient values observed across recalculation ticks without storing extra history arrays inside the `.xlsx` model.

---

### 4.5 App (`Ctrl+5`)

**Purpose:** Published, zero-hazard application interface for consumers like Jo who run scenarios, filter reports, and print outcomes without ever risking formula deletion.

- **Gestures & keyboard model:** Standard accessible web-app ergonomics—`Tab` / `Shift+Tab` moves strictly across unlocked inputs, scenario controls, filter dropdowns, and view toggles; `Arrow` keys step sliders/spinners; `[Pin Baseline]` snapshots any output set for live delta comparison; typing on a locked output card surfaces the polite Goal-Seek / Classic redirect callout (`2.5`). *Future touch/pen note:* Large `40px` touch-friendly slider thumbs and segmented buttons on tablets.
- **Entering & leaving:** Opens automatically when `preferredView="app"` is saved in `dnaCalcViews.xml`, or via `Ctrl+5`. Builders press `Ctrl+Shift+A` (`Edit App Layout`) to rearrange cards or `Ctrl+1` to return to Classic.
- **Workbook suitability:**
  - **Suits best:** `Loan` (borrower calculator), `Budget` (executive scenario sandbox), `Stock` (interactive inventory simulator), and `Sales`'s `Report` (regional filter dashboard).
  - **Declines automatic synthesis on:** `Inherited` (because `Inherited` has 40 unannotated sheets, 417 errors, and no unambiguous input/output contract, App View declines guessing a fake UI and offers `[Select Input & Output Ranges to Build App]` or `[Inspect Workbook First]`).

#### Mockup M-16: App View Full-Screen Reference (`Loan.xlsx` — Published Borrower Calculator)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Loan.xlsx                [1 Classic] [2 Inspector] [3 Note] [4 Map] [5 App*]   [Ctrl+P] │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ MORTGAGE & LOAN SCENARIO APP          [Baseline: $450k · 6.25% · 30y] [Reset Inputs] [Share] │ (1)
├──────────────────────────────┬───────────────────────────────────────────────────────────────┤
│ ASSUMPTIONS (4 Inputs)       │ KEY OUTCOMES (Protected Formulas · Live Delta vs Baseline)    │
│                              │ ┌──────────────────┐ ┌──────────────────┐ ┌─────────────────┐ │
│ Loan Principal ($B$4)        │ │ Monthly Outflow  │ │ Total Interest   │ │ Payoff Horizon  │ │ (2)
│ [$450,000             ]      │ │ $3,070.74        │ │ $392,814.12      │ │ 22 yrs 5 mos    │ │
│ [──────●──────────────]      │ │ ▲ +$300/mo extra │ │ ▼ -$154,652.46 ✓ │ │ ▼ -91 months ✓  │ │
│                              │ └──────────────────┘ └──────────────────┘ └─────────────────┘ │
│ Annual Interest Rate ($B$5)  │ ┌─ REMAINING BALANCE & CUMULATIVE INTEREST (Months 1..360) ──┐│
│ [6.25%      ] [ - ] [ + ]    │ │ $450k ╲· · · · · (dotted: Baseline 360m)                   ││ (3)
│                              │ │ $225k   ╲_____   (solid:  Current  269m)                   ││
│ Loan Term ($B$6)             │ │    $0 ────────╲___________________________                 ││
│ (15 yr) (20 yr) (• 30 yr)    │ └────────────────────────────────────────────────────────────┘│
│                              │ ┌─ AMORTISATION SCHEDULE (Annual Rollup* │ All 360 Months) ──┐│ (4)
│ Extra Monthly Payment ($B$7) │ │ Year │ Beg Balance │ Scheduled │ Extra  │ Interest│ End Bal││
│╔[$300                 ]═════╗│ │ Yr 1 │   $450,000  │  $33,249  │ $3,600 │ $27,915 │$441,066││
│║[────●────────────────]     ║│ │ Yr 2 │   $441,066  │  $33,249  │ $3,600 │ $27,340 │$431,557││
│╚════════════════════════════╝│ └────────────────────────────────────────────────────────────┘│
├──────────────────────────────┴───────────────────────────────────────────────────────────────┤
│ AUTO · 3 ms recalc │ 0 Errors │ Formulas Write-Protected │ Reading Density · 100%            │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-16:**
- **(1) Zero-Grid App Header:** Collapsed formula bar; baseline pinning and 1-click `[Reset Inputs]`.
- **(2) Outcome Cards with Semantic Delta Arrows:** Uses `▼`/`▲` plus `✓` so positive savings (`-$154,652.46` interest) never depend on green hue alone.
- **(3) Dual-Curve Chart Overlay:** Automatically overlays pinned baseline curve (`dotted`) against active scenario (`solid`).
- **(4) Safe Rollup Switch:** Collapses 360 monthly rows into annual rows for fast scanning while keeping `[All 360 Months]` one click away.

---

### 4.6 Formula Explorer (`Ctrl+Shift+E` in-workbook · Standalone binary)

**Purpose:** Dedicated microscope and test bench for **one formula**—its AST evaluation tree, function signatures, number formatting, conditional formatting rules, edge-case sweeps, and XLL/WASM add-in functions.

1. **Form A — Docked / Modal Tool inside a Workbook (`Ctrl+Shift+E`):** Opens seeded with the active cell's formula and live workbook precedent slices (`M-17`). Temporary input tweaks inside Explorer run in a **sandboxed overlay** (`What-If Override` badge) without dirtying the sheet until the user clicks **`[Apply Formula to Cell (Ctrl+Enter)]`**.
2. **Form B — Standalone Stripped Build (`DNA Calc - Formula Explorer`):** Launches in `35 ms` with **no workbook at all** (`dot-long` badge `[•━]`, `M-18`). Users type or paste any Excel formula, bind scalar/array fixtures on the left, test custom number formats and conditional formatting rules, load/hot-reload `.xll` add-ins (`2.6`), and copy verified formulas or shareable `.dnafe.json` snippets.
- **Gestures & keyboard model:** Clicking or arrowing any AST sub-node highlights the exact substring in the editor and displays its intermediate value, type (`Scalar`, `Array 4×1`, `Ref`, `Error`), and execution microseconds (`F9` parity without mutating the formula text!); `Ctrl+Shift+R` hot-reloads XLL add-ins.
- **Suitability:** Essential across **every** workbook whenever a formula exceeds 40 characters (`Sales`'s nested `SUMIFS`/`XLOOKUP`/`FILTER`, `Inherited`'s `OFFSET` formulas, `Budget`'s `FK.SAAS_LTV`, `Loan`'s `IPMT`/`PPMT`).

#### Mockup M-17: Formula Explorer (In-Workbook Tool Docked on `Sales.xlsx` `Report!B14#`)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [==] Sales.xlsx               [1 Classic*] [2 Inspector] [3 Note] [4 Map] [5 App] [Explorer*]│
├────┬─────────────────────────┬──────────────┬──────────────┬─────────────────────────────────┤
│ 14 │║Top Enterprise Deals   ║│╔ORD-24819═══╗│ West         │ $84,200 (Spill B14:E21 · 8 rows)│
├════╧═════════════════════════╧══════════════╧══════════════╧═════════════════════════════════┤
│ FORMULA EXPLORER — Seeded from Report!B14 (Sandbox: Sheet Untouched)   [Apply Ctrl+Enter] [X]│ (1)
│ fx =SORT(FILTER(Orders[[OrderID]:[NetAmount]], Orders[NetAmount]>=F12, "None"), 9, -1)       │
├───────────────────────────┬──────────────────────────────────┬───────────────────────────────┤
│ SEEDED WORKBOOK INPUTS    │ LIVE AST SUB-EXPRESSION VALUES   │ NUMBER & CONDITIONAL FORMAT   │
│                           │                                  │                               │
│ Orders[[OrderID]:[NetAmt]]│ ▼ SORT( ..., 9, -1 )             │ Col 9 Number Format:          │
│ └─ Table: 25,000 × 9      │   ──► Array [8 × 9] ($84.2k..51k)│ ["$#,##0;($#,##0);\"-\""]     │ (3)
│                           │   ├─▼╔FILTER( ... ) ──► [8 × 9]╗ │ Sample: $84,200 │ ($84,200)   │
│ F12 (Min_Deal_Threshold)  │   │  ├─ include: Orders[Net]>=50k│                               │
│ └─ Live: $50,000          │   │  │  ──► Bool[25000] (8 TRUE) │ Conditional Format on B14#:   │ (4)
│    Sandbox Try: [$75,000] │   │  │  (Sandbox $75k -> 2 TRUE!)│ Rule 1: =$E14>=75000          │
│    [Reset to Sheet]       │   │  └─ if_empty: "None"         │ ──► Matches 2 of 8 spill rows │
│                           │   ├─ sort_index: 9 (NetAmount)   │     Style: Teal-Tint + Bold   │
│ Eval time: 1.8 ms         │   └─ sort_order: -1 (Descending) │                               │
└───────────────────────────┴──────────────────────────────────┴───────────────────────────────┘
```

**Callouts for M-17:**
- **(1) Docked Bottom Workbench:** Keeps `Report!B14#` visible in the upper viewport while dissecting the dynamic array pipeline.
- **(2) Non-Destructive Sandbox Override:** Typing `$75,000` in `Sandbox Try` immediately previews `FILTER` shrinking from `8 TRUE` rows to `2 TRUE` rows without editing `Report!F12`.
- **(3) Multi-State Number Format Preview:** Shows positive, negative, and zero renderings side by side.
- **(4) Spill-Aware Conditional Formatting:** Evaluates `=$E14>=75000` across all 8 spilled rows and reports exact hit count (`2 of 8`).

#### Mockup M-18: Standalone `DNA Calc - Formula Explorer` Full-Screen Reference (No Workbook)

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ [•━] DNA Calc - Formula Explorer                              [Load Add-in .xll] [Share] [?] │ (1)
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ SCRATCHPAD EXPRESSION (Recursive Mono · Multi-line LET / LAMBDA formatter: Shift+Alt+F)      │
│  1 │ =LET(                                                                                   │
│  2 │    r_mo,  annual_rate / 12,                                                             │
│  3 │    pmt,   -PMT(r_mo, term_yr * 12, principal),                                          │
│  4 │    ipmt1, -IPMT(r_mo, 1, term_yr * 12, principal),                                      │
│  5 │    HSTACK(pmt, ipmt1, pmt - ipmt1) )                                                    │ (2)
├───────────────────────────┬──────────────────────────────────┬───────────────────────────────┤
│ FIXTURE BINDINGS (No File)│ STEP-BY-STEP LET / AST TRACE     │ FORMAT, COND-FMT & FUNCTIONS  │
│                           │                                  │                               │
│ principal    [  $450,000] │ • r_mo   = 0.0052083333 (0.521%) │ Result Array [1 × 3]:         │
│ annual_rate  [     6.25%] │ • pmt    = 2,770.7405            │ ┌─────────┬─────────┬────────┐│
│ term_yr      [        30] │ • ipmt1  = 2,343.7500            │ │$2,770.74│$2,343.75│ $426.99││ (3)
│                           │ • pmt - ipmt1 = 426.9905         │ └─────────┴─────────┴────────┘│
│ Presets:                  │                                  │ Number Format: "$#,##0.00"    │
│ [SaaS LTV (FinKit.xll)]   │ Function Reference: IPMT         │ Cond-Fmt: [ipmt1/pmt > 0.80]  │
│ [Amortisation Check*]     │ IPMT(rate, per, nper, pv, [fv],  │ ──► TRUE (84.6% interest m1!) │ (4)
│ [Dynamic Array Lookup]    │      [type]) -> Excel-exact parity│                               │
├───────────────────────────┴──────────────────────────────────┴───────────────────────────────┤
│ STANDALONE ENGINE · 512 Built-in Excel Functions · 0 Add-ins Loaded │ Eval: 0.02 ms · 100%   │ (5)
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Callouts for M-18:**
- **(1) Dedicated Standalone Chrome (`[•━]`):** Zero file/sheet chrome; starts in 35 ms as a calculator, formula debugger, and add-in SDK test host.
- **(2) Multi-Line `LET` Editor:** Auto-indents `LET`/`LAMBDA` bindings with inline line numbers.
- **(3) Live Intermediate `LET` Variable Readouts:** Displays every bound `LET` name (`r_mo`, `pmt`, `ipmt1`) simultaneously alongside the spilled `1 × 3` output card.
- **(4) Combined Format + Rule Check:** Confirms month-1 interest is `84.6%` of payment (`$2,343.75 / $2,770.74`).
- **(5) Strip:** Shows pure standalone engine footprint and loaded UDF count.

### 4.7 The Formula Lens (`Alt+L`) across every view

Pressing `Alt+L` in **any** view toggles the **Formula Lens** without changing a single byte of stored formula semantics:
1. **Readable Reference Substitution:** Replaces opaque `Monthly!O14` or `Orders!$I$2:$I$22000` with inferred or defined structural names (`Gross_Profit[FY27_Total]`) while showing the raw Excel formula on the secondary line (`M-03`, `M-12`).
2. **Prefix Stripping in Scoped Contexts:** Inside group `Stock.` on Model Map or Notebook, strips redundant `Stock.` prefixes (`OnHand⁻ + Inflow - Outflow`).
3. **Read-Only `LET` De-duplication View:** When inspecting a monster repeated-subexpression legacy formula (`=IF(VLOOKUP(...)>0, VLOOKUP(...)*1.1, 0)` in `Inherited`), Formula Lens displays a virtual `LET(_hit, VLOOKUP(...), IF(_hit>0, _hit*1.1, 0))` readout—and only rewrites the cell to `LET` if the user explicitly clicks `[Refactor Cell to LET]`.


---

## 5. Real spreadsheet models

Real workbooks are not arbitrary bags of cells; authors assemble them from a small vocabulary of recurring structural idioms. Because DNA Calc prefers **what the program can work out from the file over anything the user must declare—while always allowing correction**, our structural recogniser analyses formula ASTs, R1C1 equivalence classes, spatial adjacency, header typography, Excel `ListObject` / `PivotTable` / `DataTable` metadata, and the precedent graph on background load.

### 5.1 Recognition confidence and non-intrusive confirmation

Every inferred pattern receives one of three honest confidence tiers:

1. **Deterministic (`100%` — Native OOXML constructs):** Excel Tables (`Orders`), PivotTables (`Pivot_RegionSKU`), Form Controls (`ScrollBar1`), Data Tables (`{=TABLE}`), `RTD` / external links, VBA modules, and explicit circular loops under Iterative Calculation. Presented without tentative phrasing.
2. **High Confidence (`≥85%` — Strong AST + layout convergence):** E.g., a 12-column horizontal header of sequential dates (`Jan-27`..`Dec-27`) above rows whose columns share identical R1C1 formulas (`Monthly!C11:N11`), or a block of constants (`prov-const`) with zero precedents and `≥3` downstream dependents (`Inputs!C5:D16`). Used automatically to synthesize Notebook sections, Model Map blocks, and Formula Lens names.
3. **Probable / Ambiguous (`50–84%` — Heuristic layout cues):** E.g., an unlabelled scratch range or mixed constant/formula column (`Gas_Daily_2001`). Views keep raw coordinates alongside a subtle dashed header chip (`[? Assumptions block · Confirm ✓ | Split | Rename]`).

**How users and agents confirm or correct:** Right-clicking any block header or pressing `Ctrl+Shift+B` (`Edit Block Reading`) lets a user or agent **Confirm**, **Rename**, **Merge/Split range**, **Re-tag role** (`Input`, `Calc`, `Summary`, `Database`, `Ignore`), or **Mark Override as Intentional**. Corrections save purely inside `xl/customXml/dnaCalcViews.xml`—never altering Excel cells—and override heuristics on future opens.

### 5.2 The 19 recurring model patterns (15 core + 4 essential additions)

Alongside the 15 patterns in the brief, Enron-grade corporate audits and modern dynamic-array workbooks require **four critical additions** (*#16–#19 below*), because omitting them leaves silent overrides, tie-out checks, ragged subtotals, and spill chains misclassified. **Appendix D** holds the full 19-row engineering & presentation matrix across all five views; here is how the families group and compose in our cast workbooks:

- **Data & Lookup Foundations:** *(1) Sheet-as-database* (`Sales` `Orders` 25k rows), *(6) Reference tables* (`Sales` `QuotaTable`, `Inherited` `Ref_Hubs`), *(11) Pivots* (`Pivot_RegionSKU`). Views treat these as queryable schema sources rather than plotting 25,000 individual row boxes.
- **Assumptions, Controls & Scenarios:** *(2) Inputs and assumptions* (`Budget` `Inputs`, `Loan` `B4:C8`), *(5) Scenarios and sensitivity tables* (`Budget` Base/Downside selector, `{=TABLE}` sweeps), *(10) Control panels* (`Stock` Form Slider). Surfaced as interactive parameter rails in Notebook, Model Map, and App.
- **Core Calculation & Temporal Structure:** *(3) Calculation blocks* (`Budget` cohort/revenue/OpEx), *(4) Time series with periods across* (`Budget` `Monthly` `C:N`), *(7) Pipelines across sheets* (`Budget` `Inputs → Monthly → Summary`), plus ***(18) Dynamic array spill pipelines & `LAMBDA` helpers*** (`Sales` `SORT(FILTER(...))`, `Stock` `HSTACK(DROP(...))`). Collapsed from `N` repetitive columns into single readable rules under Formula Lens.
- **State, Iteration & External Dynamics:** *(8) Running balances and deliberate circular references* (`Loan` `Ending_Bal` corkscrew, financial debt-interest loops), *(9) Simulations* (Monte Carlo `RAND`/`SEQUENCE` trials or `Ticks` runs), *(12) Live data* (`RTD`, external links, `OFFSET`), *(13) Macro-driven models* (`Inherited` `Module1`), *(15) Stocks, flows, feedback and delays* (`Stock` inventory + `[z⁻¹]` delay + `IFS` mode). Exposed honestly via Strand `◈` / `[↻]` / `[z⁻¹]` badges and Model Map Directors.
- **Consumption, Governance & Real-World Mess (including additions #16, #17, #19):** *(14) Reports and dashboards* (`Budget` `Summary`, `Sales` `Report`), ***(16) Reconciliation & check blocks*** (`ABS(Total - Sum)=0` tie-out flags), ***(17) Ragged hierarchies & `SUBTOTAL`/`AGGREGATE` rollups*** (multi-level P&L trees where naive summing double-counts!), and ***(19) Override & manual-adjustment layers*** (a hardcoded constant typed mid-row over `Monthly!I17` or `Gas_Daily_2001!AW412`—flagged immediately in Classic/Inspector so an override never masquerades as a formula).

---

## 6. Big and busy workbooks

`Inherited.xlsm` (`40` sheets, `9` hidden, `304,812` formulas, `Gas_Daily_2001` packed across `A1:CV2850`, missing links, macros, `6` volatile `OFFSET` names, `417` errors) is the stress test every view must survive gracefully.

### 6.1 Per-view scaling rules: show all, simplify, or decline with a way forward

| View | Shows Everything When | Simplifies Automatically When (and How) | Declines With Plain Explanation & Way Forward When |
|---|---|---|---|
| **Classic** | Always at viewport level (`100%` cell fidelity via 2D canvas/DOM virtualisation; only visible `~60×25` cells mount). | At zoom `<70%`, switches to **Semantic Minimap LOD** (Strand Tenet 8): renders structural blocks, provenance heat-tints, and `[!]` error beacons instead of illegible `4px` text. Sheet bar collapses `>12` tabs into searchable `[≡ 40 Sheets]` drawer. | **Never declines** any legal Excel worksheet (`up to 1,048,576 × 16,384`). |
| **Inspector** | Workbooks `≤2,000` formulas and `≤10` sheets show full block-level graph directly. | On `Inherited` (`304k` formulas): (1) folds `417` errors into **`3` topological Root Trees**; (2) collapses R1C1-identical runs (`2,838` rows × `1` formula rule); (3) groups graph into `40` Sheet Clusters expanding on click; (4) samples value distributions `>50k` rows with banner `[Sampled 10,000 stratified rows · Compute Exact (180 ms)]`. | Never declines workbook triage; declines **all-to-all cell-level hairball rendering** (`>2,500` visible edges) with message: *"Showing 40 sheet clusters & 3 error roots (304,812 cell edges grouped). Click any sheet or cell to expand local 2-hop neighbourhood."* |
| **Notebook** | Sheets with `≤15` blocks and tables `≤200` rows render inline in full. | Tables/schedules `>200` rows (`Loan` `360` rows, `Sales` `25,000` rows) render inside a **windowed Data Frame** (`15` rows + sticky totals + `[Scroll / Filter 25,000 rows]`). Time series `>12` columns pin first/last periods + sparkline. | Declines **auto-generating prose/document layout** for unannotated ultra-dense sheets (`>25,000` non-table formula cells like `Gas_Daily_2001`): *"Gas_Daily_2001 holds 248,400 dense grid formulas without named sections. [Pin Specific Ranges to Notebook] or [Inspect Structure in Ctrl+2]."* |
| **Model map** | Models with `≤80` actors/blocks (`Stock`, `Budget`, `Loan` block-level) show all blocks & wires on one canvas. | `81–1,500` blocks: groups into **Composite Sheet/Prefix Districts** (`⤵` click to look inside); collapses `360`-row runs into `[Schedule · 360 rows]`; bundles parallel cross-sheet wires with edge counts (`──[14 refs]──►`) and bubbles interior `[! 390 err]` badges to parent boxes. | Declines **flat ungrouped cell-to-cell diagrams** when a single ungrouped sheet exceeds `500` distinct R1C1 formula blocks: *"Gas_Daily_2001 has 612 distinct formula blocks (too dense for one flat schematic). Showing 8 column-zone districts—click a zone to look inside, or select a cell and click [Map 2-Hop Neighbourhood]."* |
| **App** | Workbooks with an explicit `dnaCalcViews.xml` App spec or unambiguous `≤25` inputs + summary outputs (`Loan`, `Budget`, `Stock`). | Collapses detail tables (`Orders` `25k` rows, `Loan` `360` rows) behind summary KPIs and filter/rollup drawers. | Declines **guessing an automatic App** on `Inherited` (`>50` scattered constants, `417` errors, missing links, no clear output sheet): *"Inherited.xlsm has 40 sheets and 417 active errors, so DNA Calc won't guess a simplified App that could hide broken numbers. [Pick Input & Output Cells for App] or [Triage in Inspector]."* |

### 6.2 Hard latency & responsiveness budgets (tested on `Inherited.xlsm`)

- **Shell frame ready (cold start):** `≤ 80 ms` desktop (`≤ 150 ms` warm WASM browser).
- **Workbook open to interactive cached first paint (`Inherited`, `18.4 MB`):** `≤ 250 ms` (quarantine banner + cached grid values scrollable immediately; zero main-thread stalls `> 16 ms`).
- **Background dependency & R1C1 block indexing (`304,812` formulas):** Streams off-thread in `≤ 1,500 ms` with live progress in the Strip; user can navigate, search, and switch views throughout.
- **Keystroke to screen / cell cursor step / viewport scroll:** `≤ 16 ms` p95 (`≤ 25 ms` p99, locked `60–120 fps`).
- **Hover precedent peek / Formula Explorer sub-expression click:** `≤ 32 ms`.
- **View switch (`Ctrl+1..5`):** `≤ 100 ms` warm (`≤ 200 ms` initial structural synthesis).

**Unbreakable invariants (never acceptable under any load):** (1) Freezing the UI thread or ignoring `Esc`; (2) silently truncating rows/sheets without an explicit count badge; (3) collapsing a block in a way that conceals an interior `#Error`, broken external link, or hardcoded override.

---

## 7. Customisation

Users and AI agents customise **how DNA Calc looks, arranges, and operates—never what Excel calculates**.

### 7.1 What a skin can declare, how it travels, and what stays fixed

A **Skin** is a validated declarative manifest (JSON in workspace packs `.dnaskin.json`; mirrored as schema-validated XML inside `xl/customXml/dnaCalcViews.xml` when travelling inside a workbook, which Excel preserves untouched across opens and saves):

- **Declarative capabilities (no code):**
  1. `theme`: Base Strand theme (`cockpit-light`, `cockpit-dark`, `high-contrast`) plus optional surface/accent token overrides validated against Strand's WCAG AA contrast assertions (`≥4.5:1` text, `≥3:1` marks).
  2. `views`: Per-workbook and per-sheet preferred view, density (`working` | `reading`), visible chrome bars (`bridge`, `commandStrip`, `provenanceTints`), and frozen rails.
  3. `layout`: Notebook prose/slice blocks & pipeline flow; Model Map composite groups, `[x, y]` positions, and Director bar defaults; App View input controls (sliders, segmented toggles, dropdowns), protected KPI cards, baseline pins, and print templates.
  4. `ergonomics`: Custom keymap bindings, Command Palette aliases, and pinned Inspector/Explorer side panels.
- **Custom component code (strictly host-installed plugins, never raw code inside `.xlsx`):** New visual widgets (e.g., a custom geographic map over hub prices) ship as signed workspace Web Components receiving read-only cell values and emitting standard cell-edit intents.
- **Try, validate & share:** Changes apply live in a **Skin Preview Seam** (`[Previewing Skin · Esc to Revert · Save to Workbook | Save for Me]`). Any rule violating WCAG AA contrast or attempting to rebind reserved semantic signals fails validation with a plain line-numbered fix hint.
- **What stays immutably fixed:** Excel evaluation parity; reserved semantic hues & shapes (`red` `#D02A23` + `[!]` = error only; `amber` `#FFB301` = focus/calc-now only; `signal` `#FF8001` + `◈` = external/volatile only); App formula-overwrite protection; `Esc` revert and `Ctrl+Z` undo; and Macro / External-Link security quarantine banners (no skin can hide or style away a security warning).

### 7.2 Worked example 1: Agent restyles `Classic` for Maria's late-night audit session

**Maria's prompt:** *"Give me a dark Classic audit workspace: turn on constant/external provenance tints, dock a narrow root-error & precedent rail on the right, and bind `Alt+R` to jump to the next formula override."*

```json
{
  "skinVersion": "1.0",
  "scope": "user-workspace",
  "name": "Maria Night Audit",
  "theme": "cockpit-dark",
  "classic": {
    "density": "working",
    "provenanceTints": { "constants": true, "externalVolatile": true, "iterative": true },
    "commandStrip": "compact",
    "rightRail": { "panel": "inspector.miniTrace", "widthPx": 280, "showOverrides": true }
  },
  "keymap": [
    { "chord": "Alt+R", "command": "audit.nextFormulaOverride" },
    { "chord": "Alt+Shift+R", "command": "audit.prevFormulaOverride" }
  ]
}
```
*Result (`40 ms` live preview):* `Classic` shifts to `cockpit-dark` (`#0F272E` chrome, `#1A262B` paper), lights up `prov-const` (`#7DB8D8`) and `prov-ext` (`#F0A35C`) tints, docks a `280px` trace rail, and activates `Alt+R`—without touching a single cell of `Budget.xlsx`.

### 7.3 Worked example 2: Agent builds an `App` view of `Loan` from a three-line brief

**Three-line brief:**
> *"Turn `Loan.xlsx` into a clean borrower app for Jo, opening in App view by default.*
> *Put sliders on Principal ($100k–$1M), Rate (2%–12%), Term (15/20/30 yr buttons), and Extra Monthly ($0–$1,000).*
> *Highlight Monthly Outflow, Total Interest, and Payoff Horizon with baseline deltas above the Balance chart and an annual-by-default schedule."*

```json
{
  "skinVersion": "1.0",
  "scope": "workbook-customXml",
  "preferredView": "app",
  "app": {
    "title": "Mortgage & Loan Scenario App",
    "density": "reading",
    "baselinePin": "auto-on-open",
    "inputs": [
      { "target": "Loan!C4", "label": "Loan Principal", "control": "slider", "min": 100000, "max": 1000000, "step": 5000 },
      { "target": "Loan!C5", "label": "Annual Interest Rate", "control": "stepper", "min": 0.02, "max": 0.12, "step": 0.00125 },
      { "target": "Loan!C6", "label": "Loan Term (Years)", "control": "segmented", "options": [15, 20, 30] },
      { "target": "Loan!C7", "label": "Extra Monthly Payment", "control": "slider", "min": 0, "max": 1000, "step": 50 }
    ],
    "kpis": [
      { "target": "Loan!F4", "label": "Monthly Outflow", "delta": "neutral" },
      { "target": "Loan!F5", "label": "Total Interest", "delta": "lower-is-better" },
      { "target": "Loan!F6", "label": "Payoff Horizon", "delta": "lower-is-better" }
    ],
    "body": [
      { "kind": "chart", "target": "Chart_Amortisation", "overlayBaseline": true },
      { "kind": "schedule", "target": "Loan!A11:H371", "defaultRollup": "annual", "allowExpandAll": true }
    ],
    "print": { "preset": "executive-2page", "appendFullSchedule": false }
  }
}
```
*Result:* Produces **Mockup M-16 / M-07** immediately, embeds inside `Loan.xlsx`'s custom XML part, and opens intact in both DNA Calc (as Jo's App) and Excel (as the unchanged `Loan` sheet).

---

## 8. Printing and macros

### 8.1 What each view prints

Every view prints crisp paginated vector output forced to **`cockpit-light` daylight paper** (dark chrome stripped, WCAG high-contrast inks, tabular numbers aligned, zero clipped scrollbars):
- **Classic:** Honours Excel's native `PageSetup` (`PrintArea`, `PrintTitles` repeating rows/columns, page breaks, `FitToPagesWide/Tall`, headers/footers, margins) **100% faithfully**, with an optional DNA Calc checkbox `[ ] Print Formula/Error Provenance Legend`.
- **Inspector:** Prints a **Model Audit Dossier**—1-page Executive Health Summary (sheet inventory, hidden sheets, external links, volatile bottlenecks, 3 root-error traces) plus Discrepancy Waterfalls (`M-04`).
- **Notebook:** Prints as a typeset publication (`Source Serif 4` prose + full-width vector charts + formatted Data Frame tables with repeating headers; folded assumptions optionally expanded in an appendix).
- **Model map:** Prints an **Engineering Schematic Sheet** (like `names-in-motion.html`'s title block: model name, active Director & iteration settings, vector block diagram scaled to single sheet or poster tiles with cross-page wire reference tags `──► [B3]`, plus optional Defined-Name Formula Schedule).
- **App & Formula Explorer:** App prints the **Executive Scenario Brief** (`M-07`: inputs, baseline vs active KPI deltas, dual-curve chart, annual rollup + opt-in full schedule). Formula Explorer prints a 1-page **Formula & UDF Verification Card** (AST trace, format samples, boundary test matrix).

### 8.2 How macros interact with views

1. **Quarantine first:** Untrusted `.xlsm` files (`Inherited.xlsm`) open with macros blocked (`M-01`) until explicitly enabled per file/signature.
2. **Legacy Excel VBA works across every view without rewrite:** Standard VBA (`Worksheets("Summary").Activate`, `Range("C8").Select`, `Range("C5").Value = 300`, `Application.Calculate`, `Application.ScreenUpdating = False`) maps through the Conceptual Model (`Section 3`). If `Roll_Month_End()` updates 4,000 cells while Maria is in **Notebook** or **Model map**, screen updates batch cleanly while `ScreenUpdating = False`, recalculate once, and push a single compound `[M] Macro: Roll_Month_End` Undo checkpoint (or warn before running if the macro calls irreversible external I/O!).
3. **View-aware automation (`DnaCalc.View.*` host bridge):** Macros and add-ins can optionally call `Application.Run("DnaCalc.View.Switch", "app")`, `"DnaCalc.View.Select", "Stock.OnHand"`, or `"DnaCalc.View.ShowPanel", "inspector"`—written so that in desktop Excel `On Error Resume Next` ignores the optional host call cleanly. Macros can **never** suppress security banners, spoof formula error badges, or bypass App formula protection without dirtying the audit trace.

---

## 9. Testing the experience

### 9.1 Usability-test scripts derived from Section 2 (no hints allowed)

Run with 8 participants (4 Excel power analysts, 2 auditors/engineers, 2 non-builder managers) on clickable/live builds:

| Script | Participant & Starting File | Exact Task Prompt Given | Quantitative Pass Bar |
|---|---|---|---|
| **UT-1** (`2.1`) | Analyst · `Inherited.xlsm` (cold start) | *"Open `Inherited.xlsm`. Without enabling macros, tell us how many sheets are hidden and find the exact cell causing `Desk Net VaR` (`Summary_P&L!F19`) to show `#DIV/0!`."* | `100%` avoid macro execution; find `9` hidden sheets + `Gas_Daily_2001!AT142` in `≤ 90 s` (`0` modal traps). |
| **UT-2** (`2.2`) | Analyst · `Budget.xlsx` | *"Create a live write-up showing `Operating EBITDA` in a sentence that updates automatically when you switch from `Base Case` to `Downside`, then copy it into an email."* | Live `@chip` authored + verified across scenario switch in `≤ 3 min`; pasted email holds `$146,952` / `($612,400)`. |
| **UT-3** (`2.3`) | Auditor · `Sales.xlsx` | *"On `Report`, `West` Q4 (`C7`) is `$1,412,800` while the PivotTable (`N8`) says `$1,648,250`. Account for 100% of the difference."* | Both root causes (`$2:$22000` clip `+$198,450` and `"West "` space `+$37,000`) found in `≤ 2 min`. |
| **UT-4** (`2.4`) | Engineer · Blank workbook | *"On Model Map, build a stock loop (`OnHand`, `Inflow` with 3-step delay, `Outflow`, `ReorderPoint` slider), step 5 ticks, then check how it looks in Classic."* | Loop wired + `Ticks` enabled without help in `≤ 5 min`; user confirms Classic backing cells are legible. |
| **UT-5** (`2.5`) | Manager · `Loan.xlsx` | *"Open `Loan.xlsx`. Try typing `$2,400` onto Monthly Payment, then compare paying `$300/mo` extra against `$0` extra and prepare a 2-page PDF."* | `0` overwritten formulas; exact `$154,652` savings read aloud; 2-page PDF configured in `≤ 2 min`. |
| **UT-6** (`2.6`) | Add-in Dev · Standalone Explorer + `Budget` | *"Load `FinKit.xll` in Formula Explorer, find any input that breaks `FK.SAAS_LTV`, then compute `Cohort LTV` in `Budget!Summary!C11`."* | `churn = 0` caught via Boundary Probe in `≤ 90 s`; `Summary!C11 = $17,118` entered cleanly. |
| **UT-7** (`2.7`) | Analyst · `Budget.xlsx` + staged Agent #4 | *"Review the agent's proposed Downside update, accept only what matches the brief, then undo the churn edit while keeping the R&D freeze."* | `100%` spot Base Case `+$265k` leak and reject Hunk 3; single `Ctrl+Z` reverts Hunk 2 only. |

### 9.2 Mandatory reviewer screenshot matrix across cast workbooks

Before shipping any milestone, design review inspects **27 mandatory screenshots** (`cockpit-light` + `cockpit-dark` + `high-contrast` on Classic/Inspector, plus resting & active states across the cast):
- **`Budget.xlsx`:** Classic (`M-12` mid-pick + `M-09` UDF + `M-10` Agent diff), Notebook (`M-03` page + `M-14` pipeline), Model Map (3-sheet pipeline), App (scenario brief).
- **`Sales.xlsx`:** Classic (`25k` table scroll), Inspector (`M-04` clip + string waterfall), Docked Formula Explorer (`M-17` `SORT(FILTER)`), App (regional report filter).
- **`Loan.xlsx`:** Classic (`360`-row corkscrew), Notebook (loan explainer), App (`M-16` resting + `M-07` print drawer).
- **`Stock.xlsx`:** Model Map (`M-05` mid-tick + `M-15` modal `IFS`), Classic (`M-06` backing grid + slider), Inspector (iterative loop audit).
- **`Inherited.xlsm`:** Classic (`M-01` open quarantine + `50%` semantic zoom LOD on `Gas_Daily_2001`), Inspector (`M-02` + `M-13` multi-probe), Notebook & App **honest refusal screens**, Model Map (40-sheet district overview).
- **Standalone Explorer:** Empty state, `M-08` (`FinKit.xll` probe), and `M-18` (multi-line `LET`).

---

## 10. Where I would push back

Elsewhere in this document we built strictly on the settled direction. Here are **four places where the settled direction carries real product risk**, why, and what we should adjust before locking implementation:

1. **`Inspector` is stronger as an orthogonal X-Ray drawer/overlay (`Alt+I`) across *every* view than solely as View #2.**
   - *Why risky:* Treating `Inspector` purely as a separate place (`Ctrl+2`) forces Sam and Maria to leave the spatial grid or Model Map every time they trace a precedent, then jump back (`Ctrl+1`) to edit—recreating a context switch right when hands and eyes want to stay together.
   - *What to do instead:* Keep full-screen **Inspector (`Ctrl+2`)** for workbook-wide triage (`Inherited`'s 40 sheets and 417 errors, `M-02`/`M-13`), **and** promote its core probes (Root-Error Trace, Discrepancy Waterfall, Volatility Halo) into a toggleable `Alt+I` **X-Ray Overlay** inside Classic, Notebook, and Model Map.
2. **Name-only boxes (`LINKED_BOXES_SKIN.md`) create "ghost workbooks" in desktop Excel unless backed by worksheet cells.**
   - *Why risky:* Excel allows defined names whose `RefersTo` holds pure formulas (`MyCell.Result = MyCell.Input1 * 2`) with *zero cells on any sheet*. If Lee builds `Stock.xlsx` out of pure workbook-scoped formulas in Name Manager, a colleague opening `Stock.xlsx` in Excel sees an **completely blank `Sheet1`** and thinks the file is empty or corrupted!
   - *What to do instead:* Enforce **cell-backed named blocks** (`2.4` / `M-06`) whenever DNA Calc creates actors on Model Map: allocate a clean, human-readable row on the backing sheet *and* bind the dotted Defined Name (`Stock.OnHand -> Inventory_Model!$C$8`) to that cell. Existing pure-name workbooks still render fine, while newly authored files look transparent in plain Excel.
3. **Editing directly through a synthesized `LET` Formula Lens risks silent AST churn.**
   - *Why risky:* Section 21.1 proposes a Formula Lens using names and `LET`/`LAMBDA` structure across views. Pretty-printing `=Monthly!O14 - SUM(...)` with readable names (`4.7`) is lossless, and formatting an *already-`LET`* formula (`M-18`) is lossless. However, if the Lens synthesizes a virtual `LET` wrapper around a legacy Excel 2016 `IF(VLOOKUP(...))` formula and the user edits inside that virtual `LET`, saving `=LET(...)` back into `.xlsx` breaks compatibility with pre-365 Excel readers and creates shocking diffs for co-workers.
   - *What to do instead:* Keep name/prefix Lens rewriting bidirectional, but keep `LET` de-duplication **strictly read-only** until the user explicitly clicks `[Refactor Cell to LET]`.
4. **Never allow executable skin component code inside `.xlsx` files.**
   - *Why risky:* Item 4 says *"View settings travel inside the file... Code is used only for new components."* Bundling JS/WASM component code inside `.xlsx` `customXml` turns macro-free `.xlsx` files—trusted by corporate email gateways precisely because they cannot carry code—into an active code-delivery vector.
   - *What to do instead:* In-file `xl/customXml/dnaCalcViews.xml` must be **100% declarative data** (`Section 7.1`). Custom code components must be installed separately as signed host/workspace extensions, with graceful declarative fallback cards when a recipient lacks the extension.

---

## 11. Risks, open questions for the owner, and a milestone order

### 11.1 Top UX & architectural risks

- **Heuristic mis-grouping on messy legacy sheets:** If DNA Calc merges two adjacent tables that happen to touch on `Inherited`, Notebook or Model Map could imply false structure. *Mitigation:* Strict R1C1 + header boundary checks, visible confidence pills on heuristic blocks (`5.1`), and 1-click `[Split / Ignore]` overrides persisted in `customXml`.
- **Perceived sluggishness on volatile legacy models (`OFFSET` / `INDIRECT`):** Even a fast engine takes hundreds of milliseconds when `OFFSET` dirties 184k cells. *Mitigation:* Off-thread async recalculation with non-blocking editing, Strip cost meter, and Inspector's 1-click `[Suggest INDEX replacement for OFFSET]` refactor preview.
- **Excel round-trip drift if someone edits `Budget.xlsx` in desktop Excel and re-opens in DNA Calc:** Co-workers in Excel may insert rows on `Monthly`, shifting `O14` to `O16` while `dnaCalcViews.xml` stayed static. *Mitigation:* Bind `dnaCalcViews.xml` targets to **Defined Names + R1C1 structural fingerprints** (not brittle raw `"O14"` strings alone), and run a fast reconciliation pass on open that re-anchors moved blocks or flags `[1 Notebook card needs re-anchoring]`.

### 11.2 Five open questions for the owner (with decisive recommendations)

1. **Backing storage for newly drawn Model Map blocks (`LINKED_BOXES_SKIN.md` Q1 & Q4):** Pure Name-Manager formulas or cell-backed names?
   - *Recommendation:* **Cell-backed workbook-scoped dotted names** (`Stock.OnHand -> Inventory_Model!$C$8`), with sheet-scoped names used only when duplicating a template sheet. Ensures opening in plain Excel shows a clean table rather than an empty grid.
2. **External edits in Excel between DNA Calc sessions:** How should `dnaCalcViews.xml` anchor references so Excel row insertions don't break Notebook/App cards?
   - *Recommendation:* Store **dual anchors**—create hidden/clean workbook Defined Names (`_dna.kpi_ebitda`) *only* if the user opts in, and by default store `(SheetName, RangeAddress, HeaderLabelFingerprint, FormulaHash)` in `customXml` so DNA Calc silently tracks shifted rows edited in Excel without littering Name Manager.
3. **Undo stack behaviour for VBA macro execution:** Excel wipes the entire Undo stack whenever any VBA macro runs! Should DNA Calc copy that limitation or improve on it?
   - *Recommendation:* **Improve on it.** Snapshot a copy-on-write workbook delta around pure workbook-mutating VBA runs so `Ctrl+Z` can undo `Roll_Month_End()` cleanly—an enormous, risk-free UX win over Excel.
4. **Standalone `DNA Calc - Formula Explorer` file persistence:** Completely stateless scratchpad, or saveable `.dnafe` / `.xlsx` export?
   - *Recommendation:* Auto-persist recent scratchpads locally + save lightweight `.dnafe.json` test suites for add-in devs like Ade, plus `[Export Test Bench as .xlsx]` (creates a 1-sheet workbook with the fixture inputs and formula) for sharing with Excel colleagues.
5. **Agent autonomy level:** May an agent ever auto-apply model edits without the Staged Change-Set (`M-10`), e.g. in an unattended batch workflow?
   - *Recommendation:* **Interactive sessions always stage model edits `[M]`** for human review (`Ctrl+Enter`), while allowing agents to live-preview `[V]` skin/layout changes reversibly (`Esc`). Unattended CLI runs may auto-commit onto a branched file copy (`Budget.agent.xlsx`), never overwriting the user's open source file in place.

### 11.3 Milestone order (dependency-sequenced, no calendar dates)

- **Milestone 1 — Strand Shell, Full-Fidelity Classic & Standalone Formula Explorer:** Ship the `petrol-800` One Shell, WCAG-tested token themes, virtualised **Classic** grid with 100% Appendix A keyboard/mouse conformance, Notched `①②③④` range-picking, non-blocking Quarantine Bar, and the standalone **`DNA Calc - Formula Explorer`** (with XLL shadow-load & hot-reload). *Proves keyboard speed, visual identity, and formula/UDF parity.*
- **Milestone 2 — In-Workbook Formula Explorer, Formula Lens & Inspector Triage Bench:** Add docked Explorer (`M-17`), `Alt+L` Formula Lens presenter, background R1C1 block/pattern recogniser, and **Inspector** (Root-Error Folding, `VeryHidden` & `OFFSET` radar, and Dual-Cell Discrepancy Waterfall `M-04`). *Conquers `Inherited` and `Sales`.*
- **Milestone 3 — CustomXML View Persistence & App View + Print Engine:** Implement `xl/customXml/dnaCalcViews.xml` round-trip anchor reconciliation, **App View** (protected KPIs, baseline deltas, rollups), paginated `cockpit-light` vector printing across Classic/Inspector/App (`M-07`), and declarative Skin engine (`Section 7`). *Delivers `Loan` for Jo.*
- **Milestone 4 — Live Reactive Notebook View:** Ship `Source Serif 4` prose blocks, live `@cell` value/control chips, windowed Data Frames, and vertical/pipeline layouts (`M-03`, `M-14`). *Delivers `Budget` for Maria's director.*
- **Milestone 5 — Hierarchical Model Map & Calculation Directors:** Build read/write actor schematics over names and inferred blocks, bidirectional short-name AST Interpreter, `[z⁻¹]` iterative loops, `IFS` modal state pills, and `Ticks` / `Iteration` / `Sweep` / `Events` (`RTD`) Director controls (`M-05`, `M-15`). *Delivers `Stock` for Lee and `Watershed`-class systems.*
- **Milestone 6 — Collaborative Agent Staged Change-Sets & VBA Bridge:** Add copy-on-write Agent Change-Sets with R1C1 hunk grouping, automatic KPI blast-radius diffing, hunk-granular Undo (`M-10`, `M-11`), and reversible VBA macro checkpoints.


---

## Appendix A: Classic keyboard and mouse conformance list

Every shortcut and mouse gesture below must behave identically to desktop Excel on Windows (with `Cmd`/`Ctrl` platform adaptation on macOS noted in settings) while rendering feedback in Strand tokens. No mode may trap the user without a visible Strip indicator and `Esc` exit.

### A.1 Key tips and command access

| Input / Gesture | Expected Behaviour |
|---|---|
| `Alt` (tap and release) | Lights single-letter `amber`-bordered mnemonic badges across the 30px Command Strip (`F` File, `H` Home, `I` Insert, `M` Formulas/Audit, `A` Data, `V` View, `L` Lens). Pressing `Alt` again or `Esc` dismisses badges cleanly without moving cell focus. |
| Classic Excel `Alt` sequences (`Alt, H, V, S` Paste Special; `Alt, H, O, I` AutoFit Column Width; `Alt, A, T` Filter; `Alt, M, P` Trace Precedents; `Alt, W, F, F` Freeze Panes) | Accepted natively as multi-key chords at full typing speed (`0 ms` animation delay)—muscle memory from Excel works even though DNA Calc uses a compact Command Strip instead of a tall Ribbon. |
| `Ctrl+K` (or `Ctrl+Shift+P`) | Opens the Strand Command Palette overlaid at top-center; fuzzy-matches commands, defined names, sheets, view switches, and recent actions; shows exact keyboard shortcut beside every hit. |
| `Context Menu key` / `Shift+F10` | Opens compact cell/header context menu anchored to the active cell without moving the mouse. |

### A.2 Grid & workbook navigation

| Input / Gesture | Expected Behaviour |
|---|---|
| `Arrow keys` (`↑ ↓ ← →`) | In `READY` or `ENTER` mode, commits any pending entry and moves active cell by 1 row/column (skipping hidden rows/columns; stepping across merged cells as one unit). |
| `Ctrl + Arrow keys` | Jumps to the last non-empty cell before a blank in the current data region, or—if starting adjacent to/on a blank—jumps to the next non-empty cell (or sheet boundary `row 1,048,576` / `col XFD`). |
| `End` (tap, then release — **End Mode**) | Lights `[END]` in the Instrument Strip; the next `Arrow` key behaves identically to `Ctrl+Arrow`; `End, Home` jumps to the bottom-right cell of the worksheet's used range; `End, Enter` jumps to the last occupied column in the current row; `End` again cancels `[END]`. |
| `Home` / `Ctrl+Home` / `Ctrl+End` | `Home` moves to column `A` of the current row (or first unfrozen column when panes are frozen); `Ctrl+Home` moves to `A1` (or top-left unfrozen cell); `Ctrl+End` moves to the bottom-right cell of the active used range. |
| `PgUp` / `PgDn` & `Alt+PgUp` / `Alt+PgDn` | Scrolls and moves selection one full viewport height up/down (`PgUp`/`PgDn`) or one full viewport width left/right (`Alt+PgUp`/`Alt+PgDn`). |
| `Ctrl+PgUp` / `Ctrl+PgDn` | Switches to the previous / next visible worksheet tab, restoring that sheet's saved active cell and viewport scroll. |
| `Enter` / `Shift+Enter` & `Tab` / `Shift+Tab` | Single cell selected: commits edit and moves `Down`/`Up` (`Enter`/`Shift+Enter`) or `Right`/`Left` (`Tab`/`Shift+Tab`); after a horizontal `Tab` run, pressing `Enter` carriage-returns to the starting column of the next row! Multi-cell range selected: moves the active cell *strictly inside the selection boundary*, wrapping top-to-bottom / left-to-right without collapsing the selection. |
| `Ctrl + .` (Ctrl+Period) | Cycles the active cell clockwise around the four corners of the current rectangular selection (scrolling the viewport to inspect big range boundaries). |
| `Ctrl + Backspace` | Scrolls the viewport immediately back to show the active cell without altering the current selection or leaving `F2` formula-editing mode. |
| `F5` or `Ctrl+G` (Go To & Go To Special) | Opens Go To dialog (pre-listing Defined Names, Tables, and recent jump return-stack so `F5, Enter` jumps *back* to the previous location!). Pressing `Alt+S` enters **Go To Special**: Constants (Num/Text/Logicals/Errors), Formulas (Num/Text/Logicals/Errors), Blanks, Current Region, Direct/All Precedents, Direct/All Dependents, Row/Column Differences, Visible Cells Only (`Alt+;`). |

### A.3 Editing, anchoring, and sub-expression evaluation (`F2`, `F4`, `F9`)

| Input / Gesture | Expected Behaviour |
|---|---|
| `F2` (Edit / Enter / Point cycle) | From `READY`: enters `EDIT` mode with caret at the end of the cell's text/formula, lighting notched `①②③④` borders on all referenced ranges. While typing a formula after an operator (`=`, `+`, `(`, `,`, `:`), pressing `F2` toggles cleanly between **`POINT`** (arrow keys pick sheet ranges) and **`EDIT`** (arrow keys move the text caret inside the formula!). |
| `Shift+F2` | Opens or creates the cell's Note / threaded Comment editor; `Esc` closes editor and returns focus to cell. |
| `F4` (in `EDIT` or `POINT` mode) | Cycles `$`-anchoring on the cell/range reference at or immediately touching the caret (or all references intersecting a text selection): `B5` → `$B$5` → `B$5` → `$B5` → `B5` (`Inputs!C8:D8` cycles both endpoints synchronously). |
| `F4` (in `READY` mode) | Repeats the last repeatable formatting or structural command (e.g., insert row, apply number format, fill color, delete column)—identical to `Ctrl+Y` when Redo is empty. |
| `F9` / `Shift+F9` / `Ctrl+Alt+F9` (in `READY` mode) | `F9`: recalculates all dirty formulas across open workbooks (and advances `+1` tick under `Ticks` iterative mode). `Shift+F9`: recalculates dirty formulas on the active worksheet only. `Ctrl+Alt+F9`: forces full dependency-rebuild recalculation of every formula. |
| `F9` (inside Formula Bar / `EDIT` mode) | Evaluates the currently highlighted sub-expression (or entire formula if nothing is highlighted) in-place to its literal result (`e.g. [420]`, `{"North","West"}`, `#DIV/0!`) inside a reversible preview pill: pressing **`Esc` reverts** the whole edit cleanly, **`Ctrl+Z` un-bakes just the `F9` evaluation** back to the sub-expression AST (!), and `Enter` commits if desired. |
| `Esc` | Universal safe exit: cancels cell edit restoring exact prior formula/value, cancels range drag/copy, dismisses popovers, or clears clipboard marching border without mutating the model. |

### A.4 Data entry, arrays, fills, and formatting shortcuts

| Input / Gesture | Expected Behaviour |
|---|---|
| `Ctrl+Enter` | Commits the typed literal or formula across **every cell in the current selection** (rectangular or `Ctrl`-multi-range), translating relative references from the active cell's R1C1 offset and preserving the selection intact. |
| `Ctrl+Shift+Enter` | Accepted for legacy CSE muscle memory; wraps/maintains CSE metadata where required for Excel 2016 compatibility while dynamic array formulas (`FILTER`, `SORT`, `SEQUENCE`, `UNIQUE`) spill natively on plain `Enter`. |
| `Alt+Enter` | Inserts a line break (`LF`, `CHAR(10)`) inside cell text or multi-line formula (`LET`) and enables Wrap Text on text cells. |
| `Alt + =` (AutoSum) | Inserts `=SUM(...)` (or `=SUBTOTAL(9, ...)` when below a filtered Excel Table!), automatically proposing the contiguous numeric block above (or to the left) in `POINT`/`EDIT` selection ready for `Enter`; if a block + empty total row/col are selected together, fills totals in one stroke. |
| `Ctrl+D` / `Ctrl+R` | Fills the top row (`Ctrl+D`) or leftmost column (`Ctrl+R`) of a multi-cell selection down/right; if a single cell is selected, copies from the cell immediately above/left. |
| `Ctrl + '` / `Ctrl + Shift + "` | `Ctrl+'` copies the *exact unshifted formula text* from the cell above into `EDIT` mode; `Ctrl+Shift+"` copies the *evaluated value* from the cell above. |
| `Ctrl + ;` / `Ctrl + Shift + :` | Inserts static current local date (`YYYY-MM-DD` / locale short date) or current time. |
| `Ctrl+Shift+ ~ / $ / % / # / ! / ^ / @` | Applies General (`~`), Currency 2-dp (`$`), Percentage 0-dp (`%`), Date (`#`), Number `#,##0.00` (`!`), Scientific (`^`), or Time (`@`) number formats immediately. |
| `Ctrl+1`..`Ctrl+5` vs `Ctrl+Shift+1` / Format | Note: per settled direction, `Ctrl+1..5` switches DNA Calc views (`Classic`..`App`), while `Ctrl+Shift+F` (and right-click › `Format Cells` or `Alt, H, F, M`) opens the Format Cells Inspector; users migrating from Excel who press `Ctrl+1` while *already in Classic* see a one-time dismissible Strip hint (`Already in Classic · Format Cells: Ctrl+Shift+F [Swap Ctrl+1 to Format in Keymap]`). |

### A.5 Selection & auditing shortcuts

| Input / Gesture | Expected Behaviour |
|---|---|
| `Shift+Space` / `Ctrl+Space` | `Shift+Space`: selects entire row(s) (or Table data row when inside an Excel `ListObject`, pressing again for entire sheet row). `Ctrl+Space`: selects Table data column → Table column+header → entire sheet column. |
| `Ctrl+A` | Progressive structural select: inside a Table selects Table body → Table+headers+totals → Sheet; inside a contiguous grid block selects Current Region → Entire Sheet; on a blank surrounded by blanks selects Entire Sheet. |
| `F8` / `Shift+F8` | `F8` toggles `[EXTEND]` mode (arrows expand selection without holding `Shift`); `Shift+F8` toggles `[ADD]` mode (arrows position a new cursor to add discontiguous ranges via keyboard alone!). |
| `Alt + ;` | Restricts current selection to **Visible Cells Only** (crucial before `Ctrl+C` or `Ctrl+Enter` over filtered or hidden rows so hidden cells are never clobbered!). |
| `Ctrl + [` & `Ctrl + ]` (`+ Shift` for transitive) | `Ctrl+[` selects direct precedents (`Ctrl+Shift+{` all transitive precedents); `Ctrl+]` selects direct dependents (`Ctrl+Shift+}` all transitive dependents), lighting trace arrows if off-screen. |

### A.6 Mouse gestures, fill handle, drag-move, and range picking

| Mouse Gesture | Expected Behaviour |
|---|---|
| **Click / Drag cells; `Ctrl+Drag`; `Shift+Click`** | Standard rectangular selection, additive multi-range selection (`Ctrl`), and anchor extension (`Shift`); Strip updates `Sum · Average · Count · Numeric Count · Min · Max` in `<8 ms`. |
| **Row/Column header click/drag & border double-click** | Selects whole rows/columns; dragging header divider resizes (simultaneously across all selected columns!); **double-clicking column right divider** auto-fits width to longest visible/sampled content + header. |
| **Fill Handle (`5×5px` bottom-right corner square) drag** | Crosshair cursor `+`: drags formulas with relative adjustment, increments recognized series (`Jan-27` → `Feb-27`, `Q1` → `Q2`, 2-cell numeric step `10, 20` → `30, 40`), or copies single numbers. Holding **`Ctrl` while dragging** inverts copy ↔ series increment and shows live ghost value tooltip beside cursor; drops non-blocking `[AutoFill Options ▾]` chip (`Copy Cells`, `Fill Series`, `Fill Formatting Only`, `Fill Without Formatting`). |
| **Double-click Fill Handle** | Fills down to the bottom of the adjacent non-empty column (preferring left neighbor, then right neighbor), stopping cleanly before a total/blank break just as Excel does, and showing a 1-line Strip confirmation (`Filled 360 rows C12:C371 · Ctrl+Z to undo`). |
| **Selection Border Drag-Move / `Ctrl` Copy / `Shift` Insert** | Grabbing any edge of the selection border (not the fill handle) moves cells—rewriting formulas pointing at the moved cells just as Excel's cut-paste does (`Preview Seam` warns if overwriting non-empty target cells!). Holding **`Ctrl`** drag-copies; holding **`Shift`** shows an `amber` insertion bar to insert-and-shift rows/columns cleanly; **Right-button drag** pops `[Move Here | Copy Here | Copy Values Only | Shift Down & Move]`. |
| **Picking ranges while typing a formula (`POINT` mode)** | Whenever the formula caret follows `=`, `(`, `,`, `;`, `:`, or an arithmetic/comparison operator, clicking or dragging any cell, range, whole row/col, Table header/column (`Orders[NetAmount]`), or clicking another **Sheet Tab** (`Inputs!C8`) inserts the structured/A1 reference as a notched `①②③④` token without committing the formula. Grabbing the border of an *already highlighted* `①②③④` range on the sheet moves that reference; dragging its corner handle resizes the range token live in the Formula Bar! |
| **Wheel / `Shift+Wheel` / `Ctrl+Wheel`** | Smooth vertical scroll (`Wheel`), horizontal scroll (`Shift+Wheel` or tilt-wheel), and **Semantic Zoom** (`Ctrl+Wheel`, `25%–250%`, swapping to structural minimap LOD below `70%` per Strand Tenet 8). |

---

## Appendix B: Front-end requirements

Every requirement below (`R-1` to `R-46`) is mandatory and verifiable by automated UI test or usability inspection.

### Shell, Strand Design System & Accessibility
- **R-1 (One Shell Invariance):** Switching between `Classic`, `Inspector`, `Notebook`, `Model map`, and `App` (`Ctrl+1`..`Ctrl+5`) shall keep the Mast, Bridge coordinates, Sheet Tabs, and Instrument Strip at fixed screen coordinates with zero layout jump.
- **R-2 (Contrast Gate):** Every foreground-on-background text token across `cockpit-light`, `cockpit-dark`, and `high-contrast` shall pass WCAG 2.1 AA (`≥ 4.5:1` normal text, `≥ 3.0:1` borders, focus rings, and graphical badges), enforced by automated build assertions.
- **R-3 (Sacred Semantic Channels):** `red` (`#D02A23`) shall appear *exclusively* on evaluation errors; `amber` (`#FFB301`) *exclusively* on active focus, selection, and live recalculation activity; `signal` (`#FF8001`) *exclusively* on external links, `RTD`, add-in UDFs, and volatile formulas; every color cue shall pair with a distinct geometric glyph (`[!]`, double-frame, `◈`, `[↻]`, `①②③④`).
- **R-4 (Motion Discipline):** Only three transitions may animate (focus settle, topological recalc ripple, cross-view continuity halo), each `120–160 ms`, and all animations shall collapse to instant `0 ms` state changes when `prefers-reduced-motion: reduce` is active.
- **R-5 (Non-Blocking Quarantine):** Opening a workbook containing VBA macros or missing external file links shall never present a blocking startup modal; macros shall default to blocked and missing links to cached values behind a keyboard-accessible Quarantine Bar (`Alt+Q`).
- **R-6 (Zero Microsoft Assets):** No Microsoft icons, fonts (`Calibri`/`Aptos` bundling), or Ribbon chrome artwork shall be used; Struktur uses `Recursive`, `Recursive Mono`, and `Source Serif 4` with system fallbacks.
- **R-7 (Screen Reader & Full Keyboard Operability):** `100%` of features across all five views, Quarantine Bar, Formula Explorer, and Agent Review Rail shall be operable via keyboard alone with ARIA grid/tree/region announcements for errors, view switches, and recalculation completion.

### Conceptual Model, Selection, Formula Lens & Undo
- **R-8 (Cross-View Selection Continuity):** Switching views shall map the active `ModelTarget` (`Cell`, `Range`, `Block`, `Name`, `Table`) to its tightest visible projection in the destination view within `≤ 100 ms` and highlight it with the continuity halo.
- **R-9 (Modeless 1-Bit Editing & `Esc` Safety):** Every editable target shall distinguish `SELECTED` from `EDITING`; pressing `Esc` during any edit, drag, wire gesture, `F9` sub-eval, or agent review shall restore exact prior workbook and view state.
- **R-10 (Formula Lens Losslessness):** Toggling Formula Lens (`Alt+L`) shall present readable name/header substitutions and read-only `LET` structure without altering stored cell formulas unless the user explicitly invokes `[Refactor Cell to LET]`.
- **R-11 (Unified Cross-View Undo):** One chronological Undo/Redo stack (`Ctrl+Z` / `Ctrl+Y`) shall record both `[M]` model mutations and `[V]` view/skin edits across all views without clearing on view switch; pure navigation and Explorer sandbox tests shall not add Undo entries.

### Classic View
- **R-12 (Excel Conformance):** Classic view shall pass `100%` of the keyboard and mouse behaviours specified in **Appendix A**, including `F2` `Edit/Point` cycling, `F4` anchoring/repeat, `F9` sub-expression preview, `End` mode, `Ctrl+Enter`, fill-handle double-click, and notched `①②③④` range picking across sheets.
- **R-13 (Provenance Tints):** Classic shall provide an instant toggle (`Provenance Tints`) visually distinguishing literal constants (`prov-const`), formulas, iterative cells (`[↻]`), and external/volatile/UDF cells (`◈` `prov-ext`), plus flagging hardcoded constants breaking an otherwise uniform R1C1 formula row/column.
- **R-14 (Off-Sheet Precedent Tab Badges):** During `F2` edit/point mode, worksheet tabs containing referenced precedents shall display matching `①②③④` badges and scalar value previews when scalar.

### Inspector View
- **R-15 (Root-Error Folding):** Inspector shall group all workbook `#` errors topologically by upstream root origin cell, displaying exact downstream broken-cell counts per root and expanding trace paths on demand.
- **R-16 (Dual-Target Discrepancy Waterfall):** Selecting two numeric cells (`Ctrl+Click`) that share upstream tables/sheets and opening Inspector shall compute and display structural coverage differences (such as truncated `SUMIFS` ranges vs full Excel Tables) and dirty criteria mismatches (such as trailing whitespace or text-formatted numbers).
- **R-17 (Hidden & Volatile Inventory):** Inspector shall list all `xlSheetHidden` and `xlSheetVeryHidden` sheets, broken/active external links, VBA modules, and volatile (`OFFSET`, `INDIRECT`, `NOW`, `RAND`, `RTD`) fan-out counts alongside profiled stage recalculation durations.

### Notebook View
- **R-18 (Pluto-Style Live Reactivity):** Notebook prose `@chips`, inline controls, Data Frames, and charts shall recalculate automatically in topological dependency order upon any input change without manual "Run Cell" execution.
- **R-19 (Dual Flow Layouts):** Notebook shall support both vertical document flow (`↓ Page`) and horizontal multi-stage flow (`→ Pipeline`), storing prose (`Source Serif 4`) and slice layout exclusively in `xl/customXml/dnaCalcViews.xml`.
- **R-20 (Windowed Data Frames & Explicit Fold Counts):** Tables or schedules exceeding `200` rows shall render as virtualised, scrollable Data Frames with sticky headers/totals; folded rows or assumptions shall always state exact hidden item counts (`[▸ +9 assumptions]`).

### Model Map View
- **R-21 (Sheet-Truth Schematic):** Model Map boxes and wires shall project directly from defined names, inferred R1C1 blocks, and host precedents/dependents; rewiring or editing a box formula shall rewrite the backing Excel formula via the AST Presenter/Interpreter (`LINKED_BOXES_SKIN.md`).
- **R-22 (Cell-Backed Authoring Default):** Creating a new actor on Model Map shall allocate a structured cell row on the backing worksheet and bind a Defined Name to it so the saved `.xlsx` displays human-readable sheet content in desktop Excel.
- **R-23 (Cycle Gate & Explicit Directors):** Closing a circular wire when Excel Iterative Calculation is `OFF` shall halt at a Preview Seam offering to enable `Ticks (Max=1)` or `Iteration (Max=100, Δ=0.001)` or cancel (`Esc`); active iterative self-loops shall display `[z⁻¹]` or `[↻ iter]` badges, and `IFS`/`SWITCH` mode cells shall offer state-diagram rendering.

### App View
- **R-24 (Formula Overwrite Refusal):** In App View, calculated formula cells shall reject direct literal typing, preserving formulas intact and offering Goal Seek or switching to `Classic`.
- **R-25 (Baseline Pinning & Deltas):** App View shall allow pinning a baseline state in memory (or `customXml`) and displaying live directional deltas (`▼`/`▲` + semantic text, never color alone) across KPI cards and chart overlays.

### Formula Explorer (In-Workbook & Standalone)
- **R-26 (In-Workbook Sandbox Isolation):** Opening Formula Explorer from a cell (`Ctrl+Shift+E`) shall seed inputs from that cell's precedents and allow temporary "Sandbox Try" value overrides without mutating the workbook until `Ctrl+Enter` is pressed.
- **R-27 (Standalone Stripped Binary):** `DNA Calc - Formula Explorer` (`dot-long` identity mark) shall launch in `≤ 50 ms` with zero workbook loaded, providing multi-line `LET`/`LAMBDA` editing, AST sub-expression values, Number Format & Conditional Formatting rule testers, automatic boundary probes (`0`, negative, array, `#N/A`), and shadow-copied `.xll` UDF hot-reloading (`Ctrl+Shift+R`).

### Performance, Big Workbooks, Customisation, Agents, Print & Macros
- **R-28 (Open & Keystroke Latency Budgets):** On a 40-sheet, 300,000-formula workbook (`Inherited.xlsm`), cached first paint shall complete in `≤ 250 ms`, background indexing in `≤ 1,500 ms` with zero UI hangs (`> 50 ms`), cell navigation/typing in `≤ 16 ms` p95, and hover peeks in `≤ 32 ms`.
- **R-29 (No Anomaly Concealment Under Simplification):** Collapsed blocks, composite districts, and semantic zoom levels (`< 70%`) shall always bubble interior errors (`[! N err]`), broken links (`◈`), and formula overrides onto the parent summary badge.
- **R-30 (Honest Refusal With Way Forward):** When Notebook, Model Map, or App declines automatic synthesis on an unstructured dense sheet (`> 25,000` ungrouped formulas or `> 500` ungrouped blocks), it shall state the exact reason and provide 1-click actions to scope a range or open Inspector.
- **R-31 (Declarative Skin Safety):** Skins travelling inside `.xlsx` `customXml` shall contain declarative data only (zero executable scripts), pass automated WCAG AA contrast checks, and never override error/focus/external signal tokens or security banners.
- **R-32 (Excel Round-Trip Anchor Resilience):** `xl/customXml/dnaCalcViews.xml` shall preserve cleanly when opened and saved by Excel, and DNA Calc shall reconcile shifted rows/columns on re-open via Defined Name + header/formula fingerprints.
- **R-33 (Agent Staged Change-Sets):** All interactive AI agent model mutations shall land in a copy-on-write Staged Change-Set grouped by R1C1 formula runs, display automated KPI impact diffs across scenarios before commit, support per-hunk `Accept`/`Reject`, and push hunk-atomic Undo entries upon commit.
- **R-34 (Daylight Vector Printing):** All views shall print paginated vector layouts using `cockpit-light` tokens; Classic shall match Excel `PageSetup` (`PrintArea`, `PrintTitles`, breaks, fit-to-page) exactly.
- **R-35 (Reversible Macro Checkpoints):** Running an enabled VBA macro shall batch UI repaints when `ScreenUpdating = False` and record a pre-run workbook snapshot so `Ctrl+Z` can revert pure workbook macro mutations.


---

## Appendix C: Needs from the system underneath

Stated strictly as plain front-end needs without prescribing internal engine implementation:

- **C-1 (Immediate Cached Snapshot on Open):** Open a workbook and immediately read sheet names, visibility states (`Visible`, `Hidden`, `VeryHidden`), grid dimensions, formatting, and last-saved cached cell values before full dependency indexing or recalculation completes.
- **C-2 (Security & External Inventory Up Front):** Know on open whether the file contains VBA/macro modules, external workbook links (and whether their targets are reachable), `RTD` topics, or add-in UDF calls, and hold macros/broken links inactive until instructed.
- **C-3 (Cell, Name, and Block Precedents & Dependents):** Query direct and transitive precedents and dependents for any cell, range, or defined name—including dynamic references (`INDEX`, `XLOOKUP`, `FILTER`, `OFFSET`, `INDIRECT`) resolved from the latest calculation—both at individual cell granularity and summarised by Defined Name, block, and sheet (preserving name-to-name edges rather than dropping names down to raw cells).
- **C-4 (Calculation Delta & Order Notification):** Be told after every recalculation which cells and defined names changed value, what their previous and new values are, which external/`RTD` event or user edit triggered the pass, and a topological dependency ordering among changed items, plus elapsed time per sheet/stage.
- **C-5 (Root-Error Attribution):** Given all cells currently holding `#` error values, know which cells are root origins (producing an error from non-error inputs) versus downstream propagations, along with the shortest error path from root to any dependent cell.
- **C-6 (R1C1 Formula Equivalence & Anomaly Detection):** Group contiguous runs of cells that share an identical relative (R1C1) formula structure into single blocks, and be told of interior anomalies—such as a literal constant overriding one cell inside a formula run, or a range reference (`Orders!$I$2:$I$22000`) that covers most but not all rows of an Excel Table (`Orders`).
- **C-7 (Parsed Syntax Tree & Lossless Name Rewriting):** Obtain the syntax tree of any formula (with character spans for every reference, literal, operator, and function call) and rewrite references bidirectionally between raw Excel syntax and scoped/shortened defined names without touching strings, comments, or function identifiers.
- **C-8 (Side-Effect-Free Sub-Expression & Sandbox Evaluation):** Evaluate any selected sub-expression (`F9` in Formula Bar), complete scratchpad `LET`/`LAMBDA` expression, or temporary "what-if" input override (for Formula Explorer and Boundary Probes) and get back its exact value, data type, spill dimensions, number-formatted string, and conditional-formatting rule match—without mutating or dirtying the live workbook.
- **C-9 (Workbookless Standalone Engine Mode):** Start the formula parser, 500+ built-in Excel functions, number formatter, conditional-format evaluator, and add-in UDF host with no workbook loaded at all (`DNA Calc - Formula Explorer`).
- **C-10 (Excel Calculation Mode & Iterative Parameters):** Read and set workbook calculation mode (`Automatic`, `Automatic Except Data Tables`, `Manual`), `Iterative Calculation` enablement, `Maximum Iterations`, and `Maximum Change`, and trigger single recalculation steps (`F9` / `Shift+F9` / full rebuild) on demand.
- **C-11 (Dirty / Stale State Visibility):** Know under `Manual` calculation mode (or mid-stream during large async recalculations) which cells and blocks are currently dirty/stale relative to upstream edits.
- **C-12 (Dynamic Spill Boundaries & Blockers):** Know for every dynamic-array anchor cell its current spilled rectangle (`B14:E21`), or—if `#SPILL!` occurs—the exact cells blocking the spill.
- **C-13 (Native Table, Pivot, Data-Table & Control Metadata):** Read and update Excel `ListObject` tables (headers, column names, total rows), `PivotTable` source ranges/axes/cached values, What-If `{=TABLE(row, col)}` definitions, charts, and Form Control bindings (`ScrollBar` min/max/step/linked cell).
- **C-14 (Custom XML View Part Round-Trip):** Read and write arbitrary UTF-8 XML/JSON blobs in a dedicated custom part of `.xlsx` / `.xlsm` files that desktop Excel ignores and preserves intact across edits and saves.
- **C-15 (Structural Fingerprint Reconciliation):** Compute lightweight label/formula fingerprints for ranges referenced by custom view settings so that if rows or columns were inserted in desktop Excel while DNA Calc was closed, moved blocks can be located on re-open.
- **C-16 (Copy-on-Write Staged Change-Sets):** Apply a batch of proposed edits (from an AI agent or refactor preview) onto an isolated fork of the workbook, read the resulting diffs across all affected cells and summary KPIs, and selectively commit chosen subsets or discard with zero residual state.
- **C-17 (Atomic & Compound Undo/Redo Checkpoints):** Group multi-cell fills, agent hunks, or VBA macro runs into named, reversible checkpoints that restore exact values, formulas, formatting, and calculation state in `O(changed cells)` time.
- **C-18 (Non-Locking Add-in UDF Host & Metadata):** Load and unload `.xll` / add-in packages without locking their files on disk, enumerate exported function names, argument descriptions, return types, thread-safety, and volatility flags, and store portable `_xll.` prefixes + cached results in `.xlsx`.
- **C-19 (Non-Blocking Cancellation & Viewport Priority):** Keep viewport reads, edits, and `Esc` cancellation responsive while indexing or recalculating 300,000+ formulas in the background, prioritizing cells visible on screen or displayed in the Strip.
- **C-20 (Fast Aggregate & Criteria Mismatch Queries):** Compute Strip aggregates (`Sum`, `Avg`, `Count`, `Min`, `Max`) over multi-thousand-cell selections instantaneously, and identify near-miss rows in a table column (e.g., `"West "` with trailing whitespace or text-stored numbers `"37000"`) when auditing `SUMIFS`/`XLOOKUP` discrepancies.
- **C-21 (Excel PageSetup & Print Geometry):** Read and write Excel `PageSetup` settings (`PrintArea`, `PrintTitles`, manual/automatic page breaks, orientation, scaling) and query exact row/column pagination boundaries for `Classic` print parity.
- **C-22 (Goal Seek Single-Input Solver):** Solve for a single numeric input cell needed to make a formula cell hit a target number (powering App View's safe `[Solve Principal for $2,400/mo…]` action).

---

## Appendix D: Pattern catalogue table

Complete catalogue of **19 recurring spreadsheet model patterns** (the 15 core patterns plus 4 essential real-world additions `#16–#19`), showing human recognition, programmatic inference & confidence tier, confirmation/correction, and presentation across all five views.

| # | Pattern & Cast Example | How a Person Recognises It | Program Recognition & Confidence | User / Agent Confirmation or Correction | How Each View Presents It (`Classic` · `Inspector` · `Notebook` · `Model map` · `App`) |
|---|---|---|---|---|---|
| **1** | **Sheet-as-database**<br>(`Sales`: `Orders!A1:I25001`, 25k rows) | Tall uniform table with column headers in row 1, typed columns (IDs, dates, categories, amounts), minimal formulas, read by `SUMIFS`/`XLOOKUP`/Pivots elsewhere. | **100% (Deterministic)** if `ListObject`; **≥90% (High)** if `>100` uniform typed rows with header row and external aggregation dependents. | Confirm as Table (`Ctrl+T`) or tag `role: database` / `headerRow: 1` in `Ctrl+Shift+B`. | **Classic:** Frozen header, filter arrows, virtualised scroll.<br>**Inspector:** Coverage bar (`88%` clip detector `M-04`), schema & dirty-type summary.<br>**Notebook:** Windowed 15-row Data Frame with column types & row count.<br>**Map:** Single `[Orders · 25,000 rows]` source cylinder/card (never 25k boxes).<br>**App:** Hidden behind search/filter controls unless drilled into. |
| **2** | **Inputs & assumptions**<br>(`Budget`: `Inputs!B5:D16`; `Loan`: `B4:C8`) | Labelled block of literal constants (often blue text or shaded fill in Excel) with units (`%`, `$/mo`), zero formula precedents, feeding many downstream formulas. | **≥90% (High):** Contiguous constants (`0` precedents, `≥2` dependents) paired with adjacent string labels and numeric formats (`%`, `$`). | `[✓ Confirm Input Block]`, adjust range bounds, set slider min/max/step, or mark a constant as a fixed physical constant (`Ignore in App`). | **Classic:** `prov-const` (`#2D6A8A`) tint + `c` badge.<br>**Inspector:** Stage-0 root source list with downstream fan-out count.<br>**Notebook:** Collapsible Parameter Card with live editable cells (`M-03`).<br>**Map:** Left-hand parameter port cards (`Demand.Rate`).<br>**App:** Interactive sliders, steppers, and numeric inputs (`M-16`). |
| **3** | **Calculation blocks**<br>(`Budget`: Cohort, Revenue & OpEx blocks on `Monthly`) | Titled group of rows/columns sharing related formulas, bounded by blank spacer rows or bold subtotal lines. | **≥85% (High):** Spatial rectangle of formulas bounded by empty rows/headers whose rows share precedent neighbourhoods and feed a block subtotal. | Rename block (`#Monthly.OpEx`), split two touching blocks, or merge sub-lines via `Ctrl+Shift+B`. | **Classic:** Subtle structural gutter outline on zoom-out.<br>**Inspector:** Single stage node with cumulative ms cost.<br>**Notebook:** Named section Data Frame with Formula Lens column.<br>**Map:** Composite actor box (`⤵` click to expand individual rows).<br>**App:** Hidden intermediate engine (only summary rows exposed unless pinned). |
| **4** | **Time series with periods across**<br>(`Budget`: `Monthly!C3:N3` `Jan-27`..`Dec-27` + `FY27`) | Sequential dates/months/quarters/years across a top row, horizontally dragged R1C1-identical formulas, corkscrew carry-forwards (`D4 = C7`), and a right-hand `SUM`/`AVERAGE` column. | **≥92% (High):** Monotonic date/period header vector (`Δ = 1 mo/qtr/yr`) above rows where `≥80%` of columns share identical R1C1 ASTs. | Confirm period axis, mark actuals-to-forecast switchover column (e.g. `Jun-27 | Jul-27`), or exclude non-period columns. | **Classic:** Sticky period row + sticky label/total columns.<br>**Inspector:** Flags horizontal R1C1 breaks (mid-year overrides!).<br>**Notebook:** Compact Data Frame (`Jan`, `Feb`, `···`, `Dec`, `Total` + inline sparkline `M-03`).<br>**Map:** Collapsed `[12-Period Vector]` block (avoids 12× box duplication).<br>**App:** Time-range chart + annual/quarterly toggle. |
| **5** | **Scenarios & sensitivity tables**<br>(`Budget`: `Base`/`Downside` selector; `Watershed`: `{=TABLE}`) | Parallel assumption columns switched by `INDEX`/`CHOOSE`/`XLOOKUP` over an active scenario cell—or a 1D/2D `{=TABLE(r,c)}` matrix sweeping outputs. | **100%** for `{=TABLE}`; **≥88% (High)** when an input selector cell switches `INDEX`/`CHOOSE`/`IFS` across parallel constant columns. | Confirm scenario names (`Base Case`, `Downside`) or tag selector cell (`Inputs!C2`). | **Classic:** Highlights active scenario column; marks `{=TABLE}` border.<br>**Inspector:** Shows active vs dormant scenario branches & sweep cost.<br>**Notebook:** Inline segmented switch (`• Base Case | Downside`) updating prose `@chips` live.<br>**Map:** `Sweep` Director composite (`25 futures` fan) or mode selector.<br>**App:** Top-bar Scenario Switcher + `[Pin Baseline]` delta comparison. |
| **6** | **Reference tables**<br>(`Sales`: `QuotaTable`; `Inherited`: `Ref_Hubs`) | Small static lookup grid (tax brackets, SKU prices, hub codes) queried via `XLOOKUP`, `VLOOKUP`, or `INDEX/MATCH`. | **≥90% (High):** Small (`2–200` row) constant table appearing as the `lookup_array` / `table_array` argument of lookup functions. | Tag `role: reference-lookup`, lock against casual App edits, or promote to Excel Table. | **Classic:** Clean constant styling; hover peek when editing `XLOOKUP`.<br>**Inspector:** Checks key uniqueness & flags unmatched `#N/A` keys (`"SOCAL_BORDER "`).<br>**Notebook:** Folded reference appendix card.<br>**Map:** Compact `[Lookup: Ref_Hubs]` badge wired into lookup blocks.<br>**App:** Supplies valid dropdown options; table itself stays tucked away. |
| **7** | **Pipelines across sheets**<br>(`Budget`: `Inputs → Monthly → Summary`) | Ordered tabs moving left-to-right from raw assumptions/data through calculations to executive outputs with strictly forward cross-sheet references. | **≥90% (High):** Sheet-level dependency DAG forms a mostly acyclic topological chain (`Sheet A → Sheet B → Sheet C`). | Reorder pipeline display or mark auxiliary back-edges in Model Map / Notebook. | **Classic:** Sheet-tab flow indicators (`Inputs [②] → Monthly* → Summary`).<br>**Inspector:** Left-to-right Stage Cost pipeline (`M-13`).<br>**Notebook:** `→ Pipeline` multi-lane horizontal flow (`M-14`).<br>**Map:** Top-level sheet composites wired left-to-right.<br>**App:** Maps first stage to Input Rail and final stage to Outcome Cards. |
| **8** | **Running balances & deliberate circular references**<br>(`Loan`: `Ending_Bal` 360 rows; debt-interest loop) | Either a temporal corkscrew (`Beg = Prior_End`, `End = Beg + In - Out`, acyclic per row) OR an algebraic simultaneous loop (`Interest ↔ Repay ↔ Debt`) with `Iterative Calc = ON`. | **100%** for iterative cycles (SCC in dependency graph + workbook `iterate=true`); **≥90%** for corkscrew running-balance columns (`Row[t] = f(Row[t-1])`). | Distinguish `[✓ Intentional Iteration]` from accidental cycle; set `Max Iterations` / `Max Change` on Director pill. | **Classic:** `[↻]` badge on iterative cells (`M-06`).<br>**Inspector:** Reports exact convergence iterations & residual delta (`Δ < 0.001`).<br>**Notebook:** Shows corkscrew identity (`End = Beg − PPMT`) + balance chart.<br>**Map:** `Iteration` Director pill + double-line `[↻ iter]` loop wire.<br>**App:** Clean amortisation / debt payoff schedule & curve (`M-16`). |
| **9** | **Simulations**<br>(`Stock` / `Watershed`: `Ticks` run or `RAND()` trials) | Repeated stochastic (`RAND`, `NORM.INV`) or discrete-time (`Ticks` self-loop) evaluations tracked across rows or recalculation steps. | **≥85% (High):** Presence of `RAND`/`RANDBETWEEN`/`RANDARRAY` feeding a Data Table, or self-referencing `Tick = Tick + 1` counter under `Iter=1`. | Confirm `[Simulation / Ticks Model]`, set seed/reset condition (`Sim.Reset`), and bind Plotter targets. | **Classic:** Standard `F9` step/resample with Strip tick counter.<br>**Inspector:** Volatility & convergence stability monitor.<br>**Notebook:** Embedded live percentile fan chart + `[Step / Run 100]` control.<br>**Map:** `Ticks` / `Sweep` Director + dockable transient History Plotter (`M-15`).<br>**App:** Interactive simulator (`▶ Run / Pause / Reset` + outcome bands). |
| **10** | **Control panels**<br>(`Stock`: `ScrollBar1` on `ReorderPoint`; checkboxes/dropdowns) | Dedicated cluster of Excel Form Controls (sliders, checkboxes, combo boxes) or Data Validation dropdown cells driving model switches. | **100% (Deterministic):** OOXML Form Control / ActiveX shapes with `fmlaLink` cell bindings + Data Validation list cells. | Re-style control widget, adjust min/max/step, or add modern App slider bound to the same linked cell. | **Classic:** Crisp Strand-rendered native Form Control aligned to grid (`M-06`).<br>**Inspector:** Shows `Control → Linked Cell → Downstream Dependents`.<br>**Notebook:** Inline interactive control pill inside prose or parameter card.<br>**Map:** Embedded slider/toggle inside the parameter actor (`M-05`).<br>**App:** Promoted directly to primary App Input Rail (`M-16`). |
| **11** | **Pivots**<br>(`Sales`: `Pivot_RegionSKU` at `Report!J4:O12`) | Dynamic cross-tabulation summarising a database table by row/column dimensions with refreshable PivotCache. | **100% (Deterministic):** Native OOXML `pivotTableDefinition` + `pivotCacheDefinition`. | Trigger `[Refresh Pivot]`, inspect cache staleness, or compare against `SUMIFS` cells (`M-04`). | **Classic:** Clean tabular Pivot rendering with field filter pills & stale-cache badge.<br>**Inspector:** Verifies cache freshness vs source `Orders` rows (`M-04`).<br>**Notebook:** Live summary Data Frame with interactive slicer pills.<br>**Map:** `[Pivot: Region × Quarter]` aggregation node fed from `Orders`.<br>**App:** Interactive cross-tab card with region/period filters. |
| **12** | **Live data**<br>(`Watershed` `RTD` feeds; `Inherited` external `.xls` & `OFFSET`) | Real-time streaming `=RTD(...)`, external workbook links, web queries, or clock-driven volatile functions (`NOW`) pushing asynchronous updates. | **100% (Deterministic):** AST contains `RTD`, external `[Workbook.xls]` references, or volatile functions. | Hold/resume feed, inspect cached timestamp (`M-01`), or adjust throttle rate. | **Classic:** `◈` `prov-ext` (`#A35400`) glyph + optional 140 ms `amber` update flash.<br>**Inspector:** Live Event Queue strip & external-link health table (`M-13`).<br>**Notebook:** Timestamped live `@chips` (`As of 16:09:47`).<br>**Map:** `Events` Director + `◈` source actors lighting only dirty downstream paths.<br>**App:** Live ticker badge + `[Pause Live Feed]` freeze toggle. |
| **13** | **Macro-driven models**<br>(`Inherited`: VBA `Module1` `Roll_Month_End`) | `.xlsm`/`.xlsb` files with macro buttons or `Worksheet_Change` hooks that copy/paste values, roll periods, or run custom VBA UDFs. | **100% (Deterministic):** Presence of `vbaProject.bin`, shape macro bindings (`onAction`), or VBA UDF calls in formulas. | Enable/keep quarantined per file (`Alt+Q`); inspect macro procedure names & button bindings. | **Classic:** Quarantine Banner on open (`M-01`); reversible `[M] Macro` Undo checkpoint when run.<br>**Inspector:** Lists all procedures, macro-bound shapes, and VBA UDF dependencies.<br>**Notebook / Map / App:** Surfaces safe macro trigger buttons *only* after user trusts macros; blocks silent execution. |
| **14** | **Reports & dashboards**<br>(`Budget`: `Summary`; `Sales`: `Report`) | Presentation-oriented sheet combining formatted KPI callouts, variance columns, charts, and zero raw data storage—purely downstream of model sheets. | **≥88% (High):** Terminal sink sheet (`0` or minimal downstream dependents on other sheets) holding charts, summary `SUM`/`SUMIFS` cells, and rich number formatting. | Mark as `preferredView: notebook` or `app`; select headline KPIs for Agent Impact Banner (`M-10`). | **Classic:** Print-ready grid layout with clean chart rendering.<br>**Inspector:** Default starting anchor for backward root-cause traces (`M-02`, `M-04`).<br>**Notebook:** Executive Headline KPI Strip + charts.<br>**Map:** Rightmost sink district.<br>**App:** Automatically populates Outcome Cards & charts (`M-16`). |
| **15** | **Stocks, flows, feedback & delays**<br>(`Stock`: `OnHand`, `Inflow`, `Outflow`, `DelayPipe`, `IFS` Mode) | State accumulation (`Stock[t] = Stock[t-1] + Inflow - Outflow`), multi-step pipelines (`DelayPipe`), and threshold policies (`ReorderPoint`, `IFS` state guards) forming closed feedback loops. | **≥85% (High):** Self-referencing additive state formulas (`X = X + A - B`), shift arrays (`HSTACK(DROP(...))`), and closed precedence cycles through `IF`/`IFS`/`MIN`/`MAX`. | Confirm state variables (`[z⁻¹]`), pipeline length, and modal `IFS` state cells (`M-15`). | **Classic:** Clean backing table with `[↻]` iterative tags & spill borders (`M-06`).<br>**Inspector:** Loop gain/stability & tick-order trace.<br>**Notebook:** Systems write-up with equation Lens & live step chart.<br>**Map:** Native home—`Ticks` Director, `[z⁻¹]` delay arcs, `IFS` state machine pills, History Plotter (`M-05`, `M-15`).<br>**App:** Interactive policy simulator with slider + time-series plot. |
| **16** | **Reconciliation & check blocks** *(Added)*<br>(`Sales` / `Budget` tie-out rows: `=ABS(C9-N12)<0.01`) | Dedicated `"Check"`, `"OK / ERROR"`, or `"Variance"` cells testing accounting identities (`Assets - Liab_Eq = 0`, `Sum of Regions = Total`) to catch silent drift. | **≥90% (High):** Formulas comparing two aggregates via `ABS(A-B)<ε`, `ROUND(A-B,2)=0`, or `IF(A<>B, "ERR", "OK")`, or rows labelled `Check`/`Tie`/`Diff`. | Pin check cells to the **Workbook Health Strip** so any edit anywhere that trips a check alerts immediately! | **Classic:** Highlighted check badge; trips show `[⚠ Check Failed]` in Strip even if no `#Error` exists.<br>**Inspector:** Dedicated **Model Checks** probe auto-monitored on every recalc.<br>**Notebook / App:** Quiet `[✓ All 4 Checks Balanced]` footer badge (turns `amber`/`red` if tripped).<br>**Map:** Guard assertion node wired from compared blocks. |
| **17** | **Ragged hierarchies & rollups** *(Added)*<br>(Multi-level Chart of Accounts / `SUBTOTAL(9,...)` trees) | Indented category trees where parent rows sum child blocks of varying depths—dangerous when someone writes `=SUM(C4:C40)` and accidentally double-counts subtotals! | **≥88% (High):** Nested `SUM` ranges sitting inside outer `SUM`/`SUBTOTAL`/`AGGREGATE` spans, paired with leading-space or indent-level row labels. | Confirm hierarchy levels or repair double-counted `SUM` -> `SUBTOTAL(9, ...)` via Preview Seam. | **Classic:** Collapsible outline gutter (`1 · 2 · 3` levels) + double-count audit warning.<br>**Inspector:** Flags overlapping `SUM` double-count bugs immediately.<br>**Notebook / App:** Expandable tree table (`▸ Operating Expenses`) with accurate rollups.<br>**Map:** Nested tree rollup block. |
| **18** | **Dynamic array spill pipelines & `LAMBDA`s** *(Added)*<br>(`Sales` `SORT(FILTER(...))` `M-17`; named `LAMBDA`s) | Single top-left formula cell (`B14`) dynamically spilling variable-sized 2D results (`B14#`), chained via `#` spill references, `LET`, `MAP`, `SCAN`, `REDUCE`, or custom `LAMBDA` names. | **100% (Deterministic):** Dynamic array AST operators (`#` spill range, array-returning functions, workbook `LAMBDA` defined names). | Inspect/debug in Formula Explorer (`Ctrl+Shift+E`); promote repeated `LAMBDA`s to named helpers. | **Classic:** `teal-600` spill frame; `#SPILL!` highlights exact blocking cell.<br>**Inspector:** Tracks dynamic `#` resize cascades.<br>**Notebook:** Auto-sizing live Data Frame expanding cleanly as `FILTER` row count changes.<br>**Map:** Reusable `LAMBDA` shown as an `[Actor Class]` badge; spills show live `[R × C]` dimension pill.<br>**App:** Self-sizing result table (zero blank padding rows!). |
| **19** | **Override & manual-adjustment layers** *(Added)*<br>(`Inherited`: `47` hardcoded cells inside `Gas_Daily_2001` formula cols) | A literal number (`$140,000`) or one-off `+ 5000` plug typed directly over one cell (`Monthly!I17`) in the middle of an otherwise uniform 12-column formula row! | **≥94% (High):** Single cell (or small island) whose R1C1 signature (`Constant` or `Formula + Plug`) breaks the surrounding `≥5`-cell row/column equivalence class. | Mark `[✓ Intentional Override (Documented)]` (adds clean note) or `[Restore Row Formula]` (`Ctrl+R` preview). | **Classic:** Unmistakable notched corner flag on `Provenance: ON` so overrides never hide.<br>**Inspector:** Listed in Hygiene Radar (`47 hardcoded overrides` `M-13`, `Alt+R` jumps through them).<br>**Notebook / Map:** Collapsed block header explicitly warns `[! 1 manual override at Jul-27]` (Principle 2: never summarize away an anomaly!).<br>**App:** Protected from accidental creation by end-users. |
