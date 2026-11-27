# Part I. Principles

Clusters: `CL-01-01`–`CL-01-13` · Governing decisions: `D-01`, `D-03`, `D-08`, `D-15`–`D-21`, `D-88` · Mockups: [Pro 21 D01](http://127.0.0.1:8720/p21/design-system.html#D01), [Pro 21 D02](http://127.0.0.1:8720/p21/design-system.html#D02), [Pro 21 D03](http://127.0.0.1:8720/p21/design-system.html#D03)

---

## 1.1 Aim: Excel Under the Hood, Modern on the Surface (`CL-01-01`, `CL-01-13`)

Spreadsheets endure because a spatial two-dimensional grid makes state immediately visible and reactive without forcing the author to build a custom application first `[P19-CARBON-0002]`. They fail when real models outgrow an undifferentiated grid—where typed constants masquerade as calculated formulas, copied formula runs hide silent manual overrides, range references stop five thousand rows short of a growing table, and managers must tiptoe around fragile formulas just to test two assumptions `[P19-CARBON-0002]`.

DNA Calc resolves that tension without inventing a competing spreadsheet dialect. Its governing thesis is **super Excel-compatible in the engine, and modern in the presentation** `[FRAMING-0001, FRAMING-0048]`:

1. **The calculation substrate and file format stay plain Microsoft Excel.** Every formula, dynamic array, structured Table, defined name, chart, PivotTable, conditional format, data validation rule, VBA module, and native add-in evaluates with Excel's exact semantics—quirks, 15-digit precision, blank-to-zero coercions, and 1900 leap-year rules included—and saves back to standard `.xlsx` or `.xlsm` files that open and calculate identically in desktop Excel `[FRAMING-0001, FRAMING-0048, P19-NEON-0021]`.
2. **The front end separates the projection lens from the substrate without ever separating them in truth.** One live workbook projects into five task-specific views—**Classic**, **Inspector**, **Notebook**, **Model map**, and **App**—plus two focused tools (**Formula Explorer** at host Level 0, and **Forms maker** for declarative pages), all mounted inside one shared shell, sharing one active selection and one undo history, with the Classic grid always one keystroke or click away `[P19-CARBON-0003, P19-GPT-0001, P19-FABLE-0017]`.

"Modern on the surface" never means pastel SaaS card walls, oversized padding that starves financial analysts of rows, decorative motion, or a simplified formula language `[P19-FABLE-0013, P19-CARBON-0004, P19-NEON-0002]`. It means four concrete capabilities that traditional grid chrome omits `[P19-FABLE-0013]`:

- **Visible structure:** calculation blocks, uniform R1C1 formula runs, defined names, Excel Tables, and cross-sheet flows are recognised from the file and exposed as first-class addressable objects `[P19-FABLE-0014]`.
- **Visible state:** calculation freshness (`as saved`, `stale`, `previous`, `pending`, `current`), error root causes, external links, RTD feeds, and change attribution sit in standing instruments rather than blocking startup dialogs `[P19-FABLE-0015, P19-FABLE-0052]`.
- **Reader-appropriate surfaces:** an analyst edits at speed in Classic, an auditor traces discrepancies in Inspector, a data scientist or educator builds a step-by-step calculation chain in Notebook, an engineer inspects feedback and lag topology in Model map, and a manager operates validated inputs and scenarios in App—while the file on disk remains one ordinary Excel workbook `[P19-FABLE-0017, FRAMING-0048]`.
- **Safe customisation by people and AI agents:** presentation layouts, themes, panels, and App pages live in an optional versioned OOXML `customXml` part (`urn:dnacalc:views:v1`) that Excel preserves untouched, so humans and agents can reshape how a model looks and operates with zero risk of altering how it calculates `[FRAMING-0001, FRAMING-0048]`.

To keep this specification focused on user experience, behavioural contracts, and the public skinning boundary without drowning product flows in internal engine plumbing `[FRAMING-0052]`, every design conflict across the source corpus is resolved once under the owner and coordinator rulings (`D-01`), producing one unified design here and recording every superseded option in [Appendix Z](appendix-z-ideas-not-taken.md) `[FRAMING-0164]`.

---

## 1.2 Website Hook, Positioning, and App↔Website Story (`CL-01-13`, `D-08`)

Owner Ruling #8 retires the provisional `dnacalc.com` hero headline `"A spreadsheet for your Excel workbooks."` `[FRAMING-0173]`. That line was tautological—every spreadsheet is for workbooks—and concealed everything distinctive about DNA Calc: bit-exact Rust engineering, five persona views over one file, headless CLI and library embedding, VBA and native `.xll` hosting, browser/Windows/Linux reach, and safe agentic customisation `[FRAMING-0165, FRAMING-0173]`.

### 1.2.1 Six Candidate Hooks Evaluated

| # | Candidate Headline | Strength | Why Taken or Not Taken |
|---|---|---|---|
| **1** | **`"Excel to the last bit. Five ways to see your model."`** *(Selected)* | Pairs the hard engineering proof (`to the last bit`, echoing DNA Calc's `last-bit` numerical evidence articles and 4.79M-call Function Bank) with immediate visual intrigue (`Five ways to see your model`). | **Selected (`D-08`).** Captures both halves of the product thesis in nine concrete words without jargon, earns instant credibility with sceptical Excel power users, and sets up the UI showcase plus the four technical pillars directly beneath. |
| 2 | `"Excel under the hood. Modern on the surface."` | Faithful internal design motto (`FRAMING-0001`). | **Retained as section eyebrow / subtitle, not H1:** `"Modern on the surface"` sounds like a cosmetic theme refresh until the five views, CLI, automation, and agent capabilities are named. |
| 3 | `"Your Excel workbooks—as grid, notebook, map, inspector, and app."` | Names all five views explicitly in one breath. | **Not chosen as H1:** Omits the calculation-exactness guarantee; readers assume a multi-view web tool approximates Excel formulas and breaks `.xlsm` files. Used as the interactive view-switcher caption instead. |
| 4 | `"The open-source Excel engine, with views built for humans and agents."` | Speaks directly to Rust developers, CI auditors, and AI agent builders. | **Not chosen as apex H1:** Leads with developer infrastructure rather than the UI that makes the first impression (`DECISIONS_20261126.md`). Adopted as the `/developers/` landing headline instead. |
| 5 | `"Every workbook stays Excel. Every view fits the job."` | Highlights round-trip safety and persona alignment. | **Not chosen as H1:** `"Stays Excel"` reads passively as a compatibility constraint rather than showcasing speed, diagnostic power, automation, and headless execution. |
| 6 | `"Run, audit, and present Excel models anywhere."` | Verb-driven and highlights cross-platform deployment. | **Not chosen as H1:** `"Anywhere"` overpromises on host-specific runtimes—native Windows `.xll` binaries and COM servers do not execute inside a browser tab (`P19-GPT-0092`). |

### 1.2.2 Hero Lockup and Foregrounded Website Pillars

On `dnacalc.com` (`/`), the hero opens with the UI making the immediate visual impression, paired directly with the four technical pillars required by `DECISIONS_20261126.md` `[FRAMING-0165]`:

- **Eyebrow:** `Open-source Rust · MIT · Plain .xlsx & .xlsm files`
- **H1 Hook:** **`Excel to the last bit. Five ways to see your model.`**
- **Lede:** `Open, calculate, and reshape real Excel workbooks—as a familiar Classic grid, a diagnostic Inspector, a Jupyter-style Notebook, a live Model map, or a clean operator App. Built on one bit-measured Rust engine for Windows, WebAssembly in the browser, the Linux command line, VBA automation, and AI agents.`
- **Interactive Hero Stage:** A live, tabbed switcher over the canonical `Sales` and `Loan` cast workbooks letting visitors click between **Classic**, **Inspector**, **Notebook**, **Model map**, and **App** in place, with a primary button **`Open in your browser`** (`/app/#src=sample:sales&view=notebook`, stating `Nothing is uploaded — runs locally in WebAssembly`), **`Try Formula Explorer`** (`/onecalc/`), and **`Windows, Linux & CLI downloads →`** (`/download/`).

Immediately beneath the hero stage, four equal first-screen pillars foreground DNA Calc's full capability surface alongside the UI `[FRAMING-0165]`:

1. **Automation (`VBA, COM & Native Add-ins` -> `/developers/vba/`, `/developers/addins/`):** Run `.xlsm` and `.xlam` VBA projects via **CalcVBA** under explicit `Off` / `Guarded` / `Unguarded` host policies, host native Windows x64 `.xll` add-ins in an isolated surrogate bench, and script views safely via `Application.Run "DnaCalc.View.*"`.
2. **Engine & Headless CLI (`Rust Crates & dnacalc Runner` -> `/developers/engine/`):** Recalculate, diff, query, and validate `.xlsx` models in CI pipelines or server workflows without starting Excel (`dnacalc recalc`, `dnacalc diff`, `dnacalc skin validate`), or embed `calc-host` and `calc-formula-host` directly in Rust, C (`xlc`), COM, and Office.js.
3. **Cross-Platform Reach (`Windows · Browser · Linux` -> `/desktop/`, `/web/`, `/download/`):** One Rust and Leptos codebase compiled natively with WebView2 on Windows, running 100% client-side in WebAssembly under `connect-src 'self'` in any desktop browser, and running headlessly or via `dnacalc serve` on Linux (with macOS on the roadmap)—stating honestly per host which native capabilities apply.
4. **Agentic Tweaking (`Declarative Skins & Staged Proposals` -> `/developers/#agents`, `/llms.txt`):** AI agents inspect workbooks through the exact human address grammar, generate contrast-checked `dna.skin/1` themes and `dna.page/1` App forms in seconds with zero risk to formulas, and stage multi-unit model proposals with subset value previews and per-unit Undo.

### 1.2.3 Bidirectional App↔Website Connection

The application and `dnacalc.com` / `handbook.dnacalc.com` speak one shared protocol (`SKIN_ARCHITECTURE.md §14`) `[FRAMING-0165]`:

- **Website -> App (`#fragment` deep links & `dnacalc://`):** Every story step, article, and gallery card on `dnacalc.com` carries an `"Open in DNA Calc"` link using privacy-preserving URL fragments (`#src=sample:sales&view=notebook&at=Analysis!A10&panel=inspector.trace`)—and `dnacalc://open#...` for the installed desktop app—so user formulas and trial inputs never cross HTTP server logs. Every function page on `handbook.dnacalc.com` and numerical investigation in `/articles/` mounts an inline `<div id="onecalc" data-link="#f=...">` `skin-0` Formula Explorer box with one-click expansion into standalone Explorer.
- **App -> Website (`HelpId`s, Handbook, & Build Provenance):** Every command in `Alt+Q`, every function in completion and Formula Explorer (`https://handbook.dnacalc.com/functions/<NAME>.html` + `/api/functions/FUNC.<NAME>.json`), every pattern reading, and every `NotAvailableHere` or `NotModelled` refusal carries a compile-time-verified `HelpId` linking to its exact guide or evidence table on `dnacalc.com` (with offline help text bundled locally). `About` and Formula Explorer's function browser embed the exact commit's oracle-corpus and witness-bank JSON summary that powers `dnacalc.com/status/` and `/build.json`, while `/llms.txt` and `.md` editions expose the same documentation and schemas to AI agents.
- **Shared Cast Storybooks:** `Budget`, `Sales`, `Loan`, `Stock`, and `Inherited` are generated from one repository fixture definition and appear identically in the app's `"New from example"` File screen, the website's interactive stories, and the browser smoke suite.

---

## 1.3 Standing Constraints: "No More Than Excel" and Visual IP Independence (`CL-01-02`)

Five non-negotiable boundaries govern every requirement in this book:

1. **The engine does no more than Excel.** Every model mutation made in any view maps to an ordinary Excel workbook operation; no view may invent a calculation feature, formula function, scope rule, or hidden runtime that causes a saved file to calculate differently when opened in Microsoft Excel `[FRAMING-0057, FRAMING-0110, FRAMING-0132]`.
2. **Views use only what Excel exposes.** Precedents, dependents, defined names, tables, spills, pre/post-calculation values, RTD topics, data tables, and iterative calculation settings are standard Excel concepts; calculation events may drive visual feedback (such as ripples or Inspector traces) and never alter cell values `[FRAMING-0110]`.
3. **Zero Microsoft intellectual property or trade dress.** Classic preserves Excel's behavioural structure and keyboard muscle memory, while using 100% original Strand visual styling, custom geometric SVGs, and open/system typefaces (`Recursive`, `Recursive Mono`, `Source Serif 4` with `system-ui`/Noto/DejaVu fallbacks—never bundling `Calibri`, `Aptos`, `Segoe` branding, Office ribbon artwork, or Microsoft icons) `[FRAMING-0132, P19-CARBON-0177, P19-NEON-0194]`.
4. **WebView2 + Leptos baseline with milestones, not dates.** Desktop and web share the Leptos CSR / WebView2 architecture (`D-07`), with `dna-grid`'s `RenderPlan` kept clean for Canvas2D and future WebGPU painting; macro security is strictly `Off` / `Guarded` / `Unguarded` (`D-88`), and delivery is sequenced by verifiable milestones (`M0`–`M8`), never calendar dates `[FRAMING-0057]`.
5. **Seven structural anti-patterns permanently rejected.** DNA Calc explicitly rejects: (a) keeping six experimental prototype skins as top-level modes (folding `atlas`, `ledger`, `tree`, and `canvas` into the five coherent views), (b) a separate visual-programming calculation runtime, (c) stateful out-of-order run-once script cells in Notebook, (d) unconfirmed heuristic App auto-publication, (e) isolated per-view model undo stacks, (f) hiding or locking away the Classic grid from managers, and (g) cloning Microsoft's 280 px Ribbon chrome `[P19-GPT-0093, P19-NEON-0002, P19-CARBON-0004]`.

---

## 1.4 The Inviolable Honesty Constraint and Deciding Principles (`CL-01-03`, `CL-01-04`, `D-15`)

### 1.4.1 Five Deciding Principles

Hard UX trade-offs across the Design Book are decided by five principles `[P19-FABLE-0018, P19-GPT-0002, P19-GPT-0003, P19-GPT-0004, P19-GPT-0005, P19-GPT-0006]`:

1. **Honesty is an inviolable floor, never a weighted trade-off.** No view or Formula Lens may display a value, formula, dependency, or freshness state unsupported by Excel-faithful calculation; views never silently drop rows, hide interior `#Error`s or manual overrides inside collapsed blocks, disguise cached missing-link values as live, or smooth over Excel oddities (such as blank-to-zero coercion or `OFFSET` volatility)—they expose and explain them plainly `[P19-CARBON-0007, P19-FABLE-0018, P19-NEON-0008, P19-NEON-0021]`.
2. **Keep the meaning; improve the reading.** Every result, error, reference, spill, cycle, and external link survives presentation unchanged; readability never justifies silently rewriting a user's formula `[P19-GPT-0002]`.
3. **Show the source at the moment of consequence.** Every editable control outside the grid names its exact Excel destination before commitment; every summary or folded run states its exact member and exception counts; and any unrecorded runtime trace is labelled `Not observed` rather than fabricated `[P19-GPT-0004, P19-FABLE-0018]`.
4. **Protect practiced hands and preserve context through movement.** Classic honours a pinned Microsoft 365 Windows Excel keyboard/mouse baseline while DNA Calc tools live on `Alt+Q` and `Alt+Shift+<key>` `[P19-GPT-0003]`. Outside Classic, familiarity means keeping Excel's formula syntax, editing keys (`F2`, `F4`, `F9`), and terminology rather than copying grid cells `[P19-FABLE-0020]`. Switching views preserves active selection, sheet, formula draft, and place history while retaining each view's own scroll and zoom `[P19-GPT-0005]`.
5. **Spend visual attention deliberately.** Dense chrome supports working hands while calm white paper supports reading; status words stay silent when current and speak clearly when abnormal `[P19-GPT-0006, P19-FABLE-0052]`. Together these rules directly retire the three highest front-end risks: overstating what the tool knows, breaking Excel muscle memory, and letting one expensive view stall the session `[P19-GPT-0094]`.

### 1.4.2 Per-View Quality Priority Matrix (`D-15`)

Because different personas choose different views for opposite ergonomics, DNA Calc rejects a single global ranking across views and enforces a per-view quality hierarchy underneath the inviolable **Honesty** constraint `[P19-FABLE-0019]`:

| Surface | Primary Persona | Quality Priority Order (under Honesty) | Practical Consequence |
|---|---|---|---|
| **Classic** | Maria (Excel analyst) | **Familiarity > Density > Clarity** | Exact Excel keys/key tips, compact 22 px rows, zero default diagnostic tints on the author's paper. |
| **Inspector** | Sam (auditor / reviewer) | **Clarity > Density > Familiarity** | Count-first root-cause tables, explicit `Observed` vs `Derived` labels, high-information trace ladders. |
| **Notebook** | Maria / data analyst | **Clarity > Comfort (familiarity with chain) > Density** | Written formula visibly above live output, 68 ch `Source Serif 4` prose, Jupyter Command/Edit ergonomics. |
| **Model map** | Lee (systems engineer) | **Clarity > Density > Familiarity** | Legible `<=60` default boxes (`<=200` cap), explicit cycle vs delay enclosures, counted wire bundles. |
| **App** | Jo (operator / manager) | **Clarity > Task familiarity (comfort) > Density** | Generous `>=36 px` Reading-density controls, domain labels, zero cell-coordinate clutter unless asked. |
| **Formula Explorer** | Ade (add-in / formula author) | **Clarity > Speed > Density** | `<0.5 s` cold start, instant AST value decomposition, raw value kept visibly distinct from formatted text. |

### 1.4.3 The Five Owner-Accepted Push-Backs

Five foundational corrections from Pro 19–21 are enforced by type and UI contract across every chapter:

1. **A circular reference is not a time delay (`D-03`).** Iterative circular references render inside an explicit convergence enclosure (`Iterates · Max N`); time-delay notation (`[t-1]` or `z⁻¹`) requires explicit prior-period row/column indexing or a confirmed RTD/macro tick clock.
2. **A dependency diagram is not an execution recording (`D-20`, `D-53`).** Precedent/dependent graphs and wire ripples are strictly labelled **dependency explanations**; actual evaluation order is shown only when recorded by the host and strictly labelled **`DNA Calc's calculation order`**—never Excel's `[P21-0058]`.
3. **The Formula Lens never rewrites formulas (`D-23`).** Pretty-printing names and `LET` bindings is strictly read-only; refactoring a cell to `LET` is an explicit, previewed model edit because shared bindings can alter volatile multiplicity (`RAND()+RAND()`).
4. **"Excel keeps it" applies to certified save routes (`D-87`).** CustomXML retention is verified on named `.xlsx`/`.xlsm` routes (`dc-1nbp.22`), while stripping by Document Inspector or third-party tools degrades cleanly to Classic with zero model loss.
5. **Keeping a file intact is distinct from running every provider everywhere (`D-70`, `D-88`).** Preserving VBA modules or `.xll` formulas in a browser tab never implies the browser can execute native Windows DLLs; unavailable capabilities return typed `NotAvailableHere` refusals with a route to the desktop app.

---

## 1.5 Strand v0.2 Visual Language, Tokens, and Contrast Law (`CL-01-05`–`CL-01-12`, `D-16`–`D-21`)

See [Pro 21 D01 — Strand tokens and type](http://127.0.0.1:8720/p21/design-system.html#D01) and [Pro 21 D02 — Cells & interaction states](http://127.0.0.1:8720/p21/design-system.html#D02).

### 1.5.1 Cockpit Chrome, Bright Paper, and Authoritative Token Source (`D-16`)

Strand's temperament is **flight-deck, not showroom** (`instruments, not ornaments`) `[FRAMING-0082, P21-0001]`. Derived from the DNA Kode base-pair mark, calm dark petrol chrome (`#193F49`) holds surrounding instruments (Mast, Deck, Bridge, Strip), while active work sits on crisp bright paper (`#FFFFFF`) framed by mist surround (`#EDF1F2`) in `cockpit-light`, charcoal paper (`#1A262B`) in `cockpit-dark`, and pure black/white surfaces in `high-contrast` `[FRAMING-0082, FRAMING-0083, P21-0001]`. Canonical token values live in `apps/dna-strand` (where `STRAND.md` and the crate ever diverge, the crate wins, and legacy non-Excel `Rich-Tree` token labels are retired under `D-16`) `[FRAMING-0070, FRAMING-0081, FRAMING-0112]`.

### 1.5.2 Sacred Semantic Channels, Dual-Stroke Focus, and Non-Colliding Glyphs (`D-17`)

Colour in Strand is a **semantic signal, never decorative paint**, and every colour signal is paired with a distinct geometric glyph or border treatment so zero information is carried by colour alone `[FRAMING-0085, FRAMING-0091, FRAMING-0096, P19-FABLE-0001, P19-FABLE-0521]`:

| Signal Role | Surface / Accent | Light-Paper `-ink` | Dark-Paper `-ink` | Petrol-Chrome `-ink` | Non-Colour Shape / Cue (`D-17`) |
|---|---|---|---|---|---|
| **Focus / Active / Calculating** | Amber `#FFB301` | `--dna-amber-ink` `#8A5A00` (~5.7:1) | `#FFD04D` (~12.4:1) | `#FFD04D` | **Dual-stroke focus ring:** dark outer `#193F49` stroke (>=3:1 against `#FFFFFF`) + `#FFB301` inner accent `[FRAMING-0133, P19-GPT-0008, P19-NEON-0015]` |
| **Excel `#Error` (sole owner of red)** | Red `#D02A23` | `#C12923` / `#D02A23` | `#E4625C` | `#FF9C96` | **2 px left-edge cell tick** (preserves cell fill) + **`⊗`** (root error) / **`⊘`** (propagated error) `[P19-FABLE-0006, P19-FABLE-0398]` |
| **Structural Warning / Anomaly** | **No colour hue** | Bold ink `#353938` | Bold ink `#EDF1F2` | `#EDF1F2` | **Hollow triangle `△`** in bold neutral ink (never steals red or amber) `[P19-FABLE-0009, P19-FABLE-0036]` |
| **External / Volatile / RTD / UDF** | Signal `#FF8001` | `#A35400` | `#F0A35C` | `#F0A35C` | **Hollow diamond `◊`** `[P19-FABLE-0007]` |
| **Freshly Calculated (this rev)** | Green `#3F8B76` | `#2E6E5B` | `#74C7AB` | `#74C7AB` | **Fading 2 px bottom underline** (strictly means *calculated in this generation*, never *human-audited*) `[P19-FABLE-0008, P19-GPT-0008]` |
| **Structure / Names / Links** | Teal `#318995` / `#3DB6CF` | `#1B5F6B` | `#4FC0D8` | `#4FC0D8` | **Top-right teal corner fold** for cell notes; rounded 5 px token chips `[P19-FABLE-0012]` |
| **Non-Current (`AsSaved` / `Stale` / `Pending`)** | Neutral hairline | `#5B717B` | `#7D97A0` | `#A9C0C7` | **45° neutral diagonal hatch** + explicit distinct status word & ARIA state (`as saved <date>`, `stale`, `previous`, `pending`) `[P19-FABLE-0012, P19-FABLE-0036]` |

**Epistemic line-style channel (`CL-01-07`):** Across every view, border line style encodes provenance independently of hue `[P19-FABLE-0024, P19-FABLE-0035]`:
- **Solid stroke:** Explicit fact stored in the workbook file (sheets, defined names, Excel Tables, committed wires) `[P19-FABLE-0003]`.
- **Dashed teal stroke + guillemets `‹label›`:** Structural reading inferred by DNA Calc and not yet named in the workbook `[P19-FABLE-0004]`.
- **Dotted teal stroke + casual/slanted type + 8% hairline hatch:** Uncommitted preview ghost or staged AI agent proposal (`[Agent]`), never drawn in amber `[P19-FABLE-0005, P19-FABLE-0012]`.

Consistent structural provenance cues allow reviewers to spot hardcoded constant overrides or external feeds in ~200 ms when overlays or non-grid views are active, without scattering decorative colours across sheets or inventing new saturated hues `[P19-CARBON-0009, FRAMING-0103]`.

### 1.5.3 Quiet Steady-State Surface (`CL-01-08`, `D-18`)

The 26 px bottom **Instrument Strip** is always present, yet **silent when everything is normal** `[FRAMING-0136, P21-0005]`. When a workbook is calculated, current, and free of errors or quarantined code, the Strip shows only a quiet steady mark alongside mode (`Ready`) and selection aggregates—no standing `"✓ Clean"` banner, no elapsed-millisecond clutter, and no internal revision/snapshot IDs on working screens (revision IDs live in the History drawer and print footers) `[FRAMING-0136, P21-0005]`. The instant an abnormal state arises—`As saved`, `Stale (Manual · F9)`, `Calculating % (Esc)`, `Previous`, `#Error` count, `Missing link`, or `Macros quarantined`—explicit orthogonal badges appear immediately `[FRAMING-0136, P21-0005]`.

### 1.5.4 Block Geometry, Typography, Density, and Motion (`CL-01-09`, `CL-01-10`, `D-19`, `D-20`)

- **Block geometry:** Every visual unit derives from the base-pair rounded rectangle on one radius scale—grid cells `0 px`, chips/tokens `5 px`, cards `9 px`, panels and stage corners `14 px`—and one spacing ramp (`2 · 4 · 6 · 10 · 14 · 22 px`) `[FRAMING-0084, FRAMING-0100, P19-NEON-0009]`.
- **Three typographic voices & two densities (`D-19`):** `Recursive` (sans-linear with `tnum` tabular numerals across `11 · 12.5 · 13.5 · 15.5 · 18 · 22 · 27 px`, weights `480/560/650`) renders UI chrome and tabular values; `Recursive Mono` renders formulas and addresses on the exact same letterform skeleton; `Source Serif 4` (~`68 ch` measure) renders narrative prose in Notebook and long explanatory blocks in App `[FRAMING-0086]`. **Working density** (22 px rows, compact 4–6 px gaps) governs Classic, Inspector, Model map, Formula Explorer, and Notebook formula/output cells; **Reading density** (`>=36 px` interactive hit targets, generous vertical rhythm) governs App forms and Notebook prose `[D-19]`.
- **Motion discipline (`D-20`):** Only three purposeful transitions may animate—focus/selection settle (`120–160 ms`), cross-view continuity halo (`150 ms`), and topological dependency-propagation ripple (capped at `400 ms` total, stepping by block above `200` items, never claiming Excel evaluation order)—while Classic's copy border stays a crisp **static dashed outline** paired with a Strip clipboard readout (`Copied B4:M12 · Esc cancels`) `[P19-CARBON-0175, P19-FABLE-0038, P19-FABLE-0521]`. Under `prefers-reduced-motion: reduce`, all three animations collapse to `0 ms` instant highlights `[P19-CARBON-0175]`.

### 1.5.5 WCAG 2.2 AA Accessibility and Ergonomic Floor (`CL-01-11`, `CL-01-12`, `D-21`)

- **Mechanical contrast enforcement:** Surface tokens are never painted directly as text; all 61 `SANCTIONED_PAIRS` in `dna-strand` (and every user/agent `dna.skin/1` override) must pass automated unrounded sRGB WCAG 2.2 AA checks (`>= 4.5:1` for text/small glyphs and chrome ink on petrol; `>= 3.0:1` for large text, control boundaries, and the dual-stroke focus ring) across `cockpit-light`, `cockpit-dark`, and `high-contrast` `[FRAMING-0097, P19-FABLE-0519, P19-NEON-0192]`.
- **100% keyboard, screen-reader, zoom, and baseline touch operability:** Every workflow, storyboard, and drag gesture across all five views, Formula Explorer, Trust bars, and Agent review has a complete keyboard-only and click-only path `[P19-CARBON-0178, P19-FABLE-0517]`. Screen readers receive semantic WAI-ARIA `grid`, `tree`, `document`, `region`, and `tablist` projections announcing cell coordinates, values, formulas, freshness, and polite completion summaries without chattering on every cell `[P19-CARBON-0178, P19-FABLE-0518, P19-NEON-0234]`. Worksheet stage zoom (`10%–400%`) is independent of UI scale; at `200%` OS/browser zoom on `1280 × 720`, non-Classic views reflow cleanly with zero lost controls `[P19-FABLE-0520, P19-NEON-0234]`. While dedicated touch/pen authoring gestures (such as pen-wiring on Model map) are deferred post-v1, Reading-density controls meet `>=36 px` target sizes from day one and no primary action relies solely on hover `[P19-NEON-0238, D-21]`.
