# DNA Calc Skin Architecture

Bead `dc-xnsw4` · 2026-11-27 · Merged from [proposal-fable.md](skin-architecture/proposal-fable.md) (Proposal A, Fable 5.2, `dc-qttnl`) and [proposal-opus.md](skin-architecture/proposal-opus.md) (Proposal B, Opus 5.5, `dc-uxiu1`), verified against the repository at `e49e830fc`.

Owner rulings in [DECISIONS_20261126.md](DECISIONS_20261126.md) override every source. This document specifies **one coherent architecture**: the skin interface every DNA Calc view, tool, skin, extension and agent works against, the host and page code shape behind it, what is kept and what is thrown away, the build order, every hard part stated plainly, and everything not taken with the reason why.

---

## 0. The architecture on one page

1. **Views are compiled engines; skins are data; pages are component trees; one contract sits underneath.**
   - **`calc-skin`** (`crates/calc-skin`) is the single portable contract crate shared by the page and the hosts: targets and addresses, the one selection, stamps and value provenance, typed model and presentation changes in atomic batches, history entries, budgeted queries, watches, atomic frames, ports, capabilities and typed refusals. It reaches no engine, host, codec or UI toolkit.
   - A **view** is a compiled Rust presentation crate written against `calc-skin`, `dna-skin-runtime` and `dna-kit`. Five workbook views are built in (**Classic**, **Inspector**, **Notebook**, **Model map**, **App**), alongside two **tools**: **Formula Explorer** (skin 0, running strictly on host level 0) and **Forms maker** (a dedicated authoring view over the live page renderer, never a mode inside App).
   - A **skin** (`dna.skin/1`) is pure JSON data: theme token overrides, density, shell layout, keymap bindings, per-view settings, panel pages and named command sequences. Built-in defaults and `skin-0` are skins too.
   - A **page** (`dna.page/1`) is one declarative flow-layout tree of standard bound components, used uniformly for App pages, skin panels, notebook control panels and the Inspector bench. A workbook file carries declarations only — never executable code.
2. **Two host levels enforced by crate boundaries.**
   - **Level 0** splits into `crates/calc-formula-kernel` (pure, stateless parsing, completion, read-only formula lens, view-syntax interpretation and localized number formatting — linked on both the page and the host for zero-round-trip typing) and `crates/calc-formula-host` (stateful formula trials, sub-expression evaluation, traces, formula-bearing conditional formats, UDF/add-in providers, VBA module UDFs, RTD topic admission and Explorer session files — zero `calc-engine` or `calc-host` in its closure).
   - **Level 1** (`crates/calc-host`, served over the wire by `crates/calc-skin-host`) owns a `FormulaHost` instance and adds workbook cells, the calculation graph, recalculation, anchors, the view part and the one undo history. It never duplicates level-0 logic.
3. **One workbook, one selection, one history.** Selection and history live in `HostSession` (the same `Selection` and `ActiveCell` that VBA and COM see). Presentation edits mutate the workbook's view part through the same forward-committing `History` as cell edits (`WorkbookOp::EditView`), so `Ctrl+Z` in any view undoes the latest step with a Strip notice naming what was undone and where, without switching views.
4. **Two revisions and one preview seam.** `Stamp` tracks `model: Revision` and `view: Revision` separately alongside `calc: CalcGeneration`. Presentation changes never advance `model` (an O(1) check on every commit), while a saved SHA-256 `model` digest in the view part detects external edits in Excel. Every non-grid edit goes through `preview`, which computes `ChangeKind` (`Model`, `Presentation`, `Both`) from the prepared writes — never from caller claims — and drives either an inline destination label or an explicit review card.
5. **Neutral queries, budgeted answers, atomic frames and consistent snapshots.** No query is named after a view; per-skin worker projections are deleted. Viewport cells stream over the existing binary tile protocol (extended with `Interest::Anchors`). Other reads are footprint-tracked queries carrying `Budget`, `Coverage` and `Shown<T>` provenance. `Evidence<T>` separates `Observed` execution records (`CalcEvents`) from `Derived` dependency explanations (`Deps`, `Structure`). Host updates arrive in atomic `Frame`s (closing cross-watch tearing and honouring `ScreenUpdating = False`), and print/export/App snapshots use `consistent()` so mixed-revision data is never shown as current.
6. **Workbook charts on the host; pagination in `calc-host`; UI styling in Strand.** `calc-chart-render` stays host-side so one renderer (with its 2.7 MB bundled Carlito fonts and `resvg`/`tiny-skia`) serves the sheet pane, `Chart.Export` and printing without bloating the UI wasm bundle, while `Objects` returns both the SVG and structured `ChartData` series. Sheet print pagination moves from `apps/dna-calc-model` into `crates/calc-host`, removing `calc-host`'s upward dependency on an `apps/` crate. On the page, views compose `dna-kit` Leptos components styled by runtime-checked Strand tokens and write no raw CSS.
7. **Extensions run as signed Wasm in a Worker returning Strand `RenderTree`s over bound slots.** Files carry component IDs and slot bindings only. Installed `.dnaext` packages run Rust-compiled Wasm inside an isolated Web Worker with no DOM, no CSS injection and no arbitrary query access: they receive `Shown<T>` data on declared read slots and emit `RenderTree` primitives plus intents on declared write slots. The Live Tree (`NC04`, `E01`) is built first to prove this boundary.
8. **Large chunks thrown away; fresh UI; one switch commit.** Roughly 33,000 lines of tangled page controller, old skins (`atlas`, `canvas`, `ledger`, `tree`, v1 `notebook`), ad-hoc dialogs, per-skin worker handlers, duplicate wire intents and OneCalc's separate port/shell are thrown away. Proven logic (~26,700 lines: `calc-host`, DOM-free grid core, grid painters, tile mirrors/publication, `dna-formula-ui`, `dna-strand`, Explorer's `Session` core, and 937 + 121 browser smoke cases) is kept. Once the contract, kit and shell land, a first working view is hours of work; Classic's Excel conformance tail, graph layout at scale and notebook lane allocation are the honest multi-day exceptions.

```text
 page (one wasm bundle: browser, WebView2, dnacalc serve, headless tests)
 ┌──────────────────────────────────────────────────────────────────────────────┐
 │ apps: dna-calc (workbook entry) · dna-onecalc (Formula Explorer, level 0)    │
 │ views & tools: dna-view-classic · -inspector · -notebook · -map · -app       │
 │                (holds App + Forms maker) · dna-view-explorer (skin 0, L0)    │
 │ dna-shell: Mast, Deck (key tips), Bridge, Stage + Companion, Panels, Strip   │
 │ dna-kit (Strand components, page renderer, View trait) · dna-grid            │
 │ dna-formula-ui (shared formula field) · dna-strand (tokens, contrast check)  │
 │ dna-skin-runtime: Frame store, Live<T>, EditSession, Modes, InputQueue,      │
 │                   CommandTable, Keymap, Places, Validator, Probe             │
 │ calc-formula-kernel: pure parse, complete, lens, interpret, number format    │
 └──────────────────────────────────────┬───────────────────────────────────────┘
                                        │ calc-skin wire (Envelope + binary tiles)
 carriers: Web Worker · WebView2 (shared buffers) · loopback WebSocket · in-process
 ┌──────────────────────────────────────┴───────────────────────────────────────┐
 │ calc-skin-host (L1 serving: queries, footprints, frames, preview, proposals) │
 │ calc-host (L1: HostSession, History, ViewPart & Anchors, PrintPlan)          │
 │ calc-chart-render (SVG + Carlito/resvg raster for pane, Chart.Export, print) │
 │ calc-formula-host (L0: FormulaHost trials, trace, CF, providers, VBA, RTD)   │
 │ calc-engine · calc-doc · calc-doc-model · calc-fml · calc-func · calc-value  │
 └──────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Rulings applied and verified codebase baseline

### 1.1 Owner and decider rulings ([DECISIONS_20261126.md](DECISIONS_20261126.md) & [README.md](README.md))

| Ruling | Architectural realisation |
|---|---|
| **D1** Ledger-first merge, one coherent design, Appendix Z / Not Taken | Section 2 resolves every divergence between Proposals A and B explicitly; Section 18 records every rejected alternative with its reason. |
| **D2** Standalone Formula Explorer early, as a teaser | Existing Explorer stays live until Milestone 1 rebuilds `dna-onecalc` on `calc-formula-host` + `dna-shell` + `dna-view-explorer` (`skin-0`) as the first vertical slice. |
| **D3** RTD clock for example ticked models; macro/Calculate ticks also valid | Ticks are ordinary `Source` updates (`DnaCalc.Clock`) or `Origin::Macro` recalculations producing new `CalcGeneration`s. `calc-skin` defines no custom simulation clock or director (§4.6, §7.4). |
| **D4** Undo across a macro run: try it, design for it, do not promise in v1 | v1 groups macro writes into one `Irreversible` entry naming the procedure and touched cells (matching Excel's undo boundary instead of today's per-statement steps), implements `Application.OnUndo`, and offers an O(1) `ModelRoot::pin` pre-run checkpoint. `Reversibility::DocumentOnly` is designed with a 7-point experimental verification plan (§17.1). |
| **D5** Notes and findings in `customXml` now; visible mirror optional | Stored in the `<notes>` section of `urn:dnacalc:views:v1`, anchored to model targets (§12.1). Visible Excel cell-note or sheet mirroring is an explicit, opt-in `ModelChange` batch. |
| **D6** Notebook works exactly like Jupyter; Read-only is file-open mode only; reader style belongs to Forms maker | One Notebook presentation with Jupyter's Command/Edit modes, keys (`Enter`, `Esc` keeping unrun drafts in session state, `Shift+Enter`, `A`, `B`, `D D`, `M`, `Y`, `J/K`) and live dependency recalculation (§7.3). Read-only is an open capability (`Capabilities.read_only`). Reader-style reports are `dna.page/1` pages authored in the **Forms maker** (§7.6). |
| **D7** FerroUI not now; WebView2/Leptos baseline stays | `dna-kit`, `dna-grid`, `dna-shell` and the view crates use Leptos 0.8 CSR in WebView2 and browsers. `calc-skin` and `dna-skin-runtime` remain DOM-free and toolkit-neutral so toolkit choices stay reversible. |
| **D8** Replace website tagline; app and site talk to each other | Bidirectional fragment-based `Link` grammar (`#...`), shared `HelpId`s, embedded `<div id="onecalc">` live formulas, and cast storybooks shared across app templates, site stories and tests (§14). |
| **D9** Skin architecture is central | One `calc-skin` contract and `dna-skin-runtime` / `dna-kit` client SDK make views small compositions of components over live queries. |
| **D10** Write fresh UI, throw away large chunks, no micro-step migration; publish early | Old page frozen while new crates build alongside kept logic; published side-by-side from Milestone 2; one switch commit in Milestone 3 deletes ~33,000 lines of old UI/worker/wire code (§15, §16). |
| **D11** Classic uses Excel's key-tip letters exactly | Key-tip letter sequences (`Alt, H, V, V`) live in a captured Excel profile data table decoupled from Deck visual layout (§8.3). |
| **D12** `Ctrl+Alt+1…5` switch views unless Excel binds them; command search fallback | `Ctrl+Alt+1…4` switch Classic, Inspector, Notebook and Model map. Excel binds `Ctrl+Alt+5` (floating-shape navigation, Pro 19 `A-111`), so `5` is not bound over the Excel profile; `Alt+Q` (command search) and the Mast switcher reach App and Forms maker, and AltGr printable characters always win (§8.3). |
| **D13** No hidden names as anchors | Legacy v1 `_dna.block.<hex>` hidden names are retired. Anchors live in `urn:dnacalc:views:v1`, keyed to user-defined names/tables/objects or cell addresses with structural fingerprints (§12.2). |
| **D14** Agent change trail kept in the file, can be turned off | `<trail>` section in `urn:dnacalc:views:v1`, recording proposal/unit summaries, targets and outcomes, toggleable via `PresentationChange::Trail` (§10, §12.1). |
| **README agreements & Owner rules** | Engine does no more than Excel; bit-match Excel; every model change maps to an Excel operation; formula lens (`Present`) is read-only and `"Rewrite as LET"` is a previewed `ModelChange`; sheet view preference is a tab badge applied only on initial open; map is a reader first and map boxes are named cells on a visible sheet; Inspector tools dock as panels in every view; files carry declarations only, extensions are signed and installed. |

### 1.2 Verified codebase facts at `e49e830fc`

Measured directly on the workspace (`wc -l` across `src/**/*.rs`, `Case {` counts, and enum variant counts):

| Area | Package / Path | Measured size | Architectural significance |
|---|---|---:|---|
| Level-1 host | `crates/calc-host/src` | 51,264 lines (76 files) | Sound transaction, inverse/restore `History` (`history.rs:56`), delta subscription and `CommandIngress` machinery. **Layering wart verified:** `crates/calc-host/Cargo.toml:28` depends upward on `apps/dna-calc-model` solely for `printing::paginate` (`print_plan.rs:9`). Fixed in §5.2. |
| Document codec & model | `crates/calc-doc/src`, `crates/calc-doc-model/src` | 66,786 + 12,716 lines | `ModelRoot` (`root.rs:264`) uses `Arc`-shared `CellPage`s; `ModelRoot::pin` (`:289`) snapshots a root in O(1). `calc-doc` already supports generic namespace-keyed `CustomXmlItem` read/write (`custom_xml.rs:21`). `git grep` confirms v1 `urn:dnacalc:view-manifest` appears in zero user/corpus files (only `cdoc_m2_1` unit fixtures). |
| Chart renderer | `crates/calc-chart-render/src` + `fonts/` | 3,723 lines + **2.7 MB** TTF | Moved out of the worker in `dc-oxh8w.22` (`e81fa3968`) with `resvg`, `tiny-skia` and 4 Carlito `.ttf` fonts so `Chart.Export` and printing share one renderer with the pane. Moving this into the UI wasm bundle would add >2.7 MB of dead weight (§2, §5.3). |
| Current UI page | `apps/dna-calc/src` | 34,157 lines (54 files) | `Controller` + `controller/*` + `input_adapter.rs` (7,018 lines), grid UI (`grid/mod.rs` 4,296 lines + pure helpers ~4,150 lines), 5 old skins (`notebook`, `atlas`, `tree`, `canvas`, `ledger`: 3,729 lines), 13 dialog/feature modules talking directly to wire requests (11,038 lines), command/wire adapters. |
| Page model & wire | `apps/dna-calc-model/src`, `apps/dna-calc-protocol/src` | 18,627 + 524 lines | Mixes ~12,744 lines of clean, DOM-free logic (selection, geometry, `RenderPlan`, `tile_mirror`, viewport, `printing`, drawn shapes) with 5,883 lines of legacy state and `projection.rs` (**72 `Intent`, 77 `Request`, 53 `Reply`, 8 `Event`, 4 `FromServer`, 12 `CalculationEvent`** variants). |
| Workbook worker | `apps/dna-calc-worker/src` | 17,962 lines (25 files) | `server.rs` (10,027 lines) mixes solid transport/publication/paste/save flows with a 1,200-line intent translator and per-skin projection builders (`canvas.rs`, `ledger.rs`, `tree.rs`, `Notebook`, `Atlas`). Includes `model_clock.rs` (185 lines, `DnaCalc.Clock` RTD provider). |
| Formula Explorer | `apps/dna-onecalc/src`, `apps/dna-onecalc-desktop/src` | 9,846 + 5,481 lines | Contains ~5,940 lines of clean level-0 logic (`session.rs` 1,386 lines, `cf.rs`, `rtd.rs`, `trace.rs`, `fixture.rs`, `file.rs`) trapped behind a separate 32-request / 17-reply `port.rs` and duplicate shell. |
| Shared UI foundations | `apps/dna-formula-ui/src`, `apps/dna-strand/src` | 4,001 + 1,271 lines | Engine-free formula field/value display (depends on `dna-calc-model` only for light types) and Strand design tokens with 61 contrast-checked pairs (`contrast.rs:77`). |
| Desktop & CLI | `apps/dna-calc-desktop/src`, `apps/dnacalc/src` | 3,304 + 1,590 lines | WebView2 shell (`desktop-bridge.js` currently base64-encodes binary tile frames after `U+0001`), STA 6 ms/8 ms calculation slices, and CLI (`recalc`, `get`, `set`, `diff`, `run`, `serve`). |
| Browser smoke suites | `apps/dna-calc/tests/smoke/cases.rs`, `apps/dna-onecalc/tests/smoke/cases.rs` | **937** + **121** cases (1,058 total) | Verified from source: `dna-calc` has `932` direct `Case {` literals + `5` `spreadsheet_font_case!` macro expansions = `937` `Case`s in `120` `CaseGroup`s (Proposal A counted `933` `Case {` tokens; Proposal B counted `1,053` `name:` fields including `CaseGroup`s). `832` test Classic/shell; `105` target the 5 retired skins. |

---

## 2. Point-by-point merge decisions: Fable (Proposal A) vs Opus (Proposal B)

Both proposals independently converged on the core architecture: a single portable contract crate, level 0 (`calc-formula-host`) as a separate crate below `calc-host`, neutral budgeted queries replacing per-skin worker projections, one host-owned selection and undo history across model and view-part edits, custom XML anchors without hidden names, a component kit on Strand tokens, and freezing then deleting the old page in one switch rather than migrating skin by skin.

Where Proposals A and B diverged, the table below records which answer is taken and why.

| # | Decision point | Proposal A (Fable) | Proposal B (Opus) | Merged choice & reason |
|---|---|---|---|---|
| 1 | **Ontology: View vs Skin vs Page** | *View* = compiled engine (5 views + 2 tools); *Skin* = `dna.skin/1` JSON manifest; *Page* = `dna.page/1` component tree used for App pages, panels, notebook controls and Inspector benches. | *Skin* = both built-in views and user restylings (`based_on`); separate schemas for App pages (`dna.app/1`), panels and Inspector bench (`dna.decl.inspector/1`). | **Fable's ontology + Opus's `install(&mut Registry)` crate hook.** Separating compiled *Views* from JSON *Skins* ends the naming confusion in today's codebase, and Fable's single `dna.page/1` grammar lets one renderer, one validator, the Forms maker and agents author App pages, panels, benches and control strips alike. |
| 2 | **Contract crate name & `calc-doc-model` dependency** | `crates/calc-skin`; depends only on `calc-value`, `calc-syntax`, `calc-macro-security` and `serde` (no `calc-doc-model`). | `crates/calc-skin-api`; depends on `calc-doc-model` to re-export `ViewPart` and `Anchor`. | **Fable (`crates/calc-skin`, no `calc-doc-model` in closure) + Opus (`ViewPart`/`Anchor` stored on `ModelRoot` in `calc-doc-model`).** Naming matches `calc-*` (`calc-value`, `calc-fml`, `calc-doc`, `calc-host`), and keeping 12.7k lines of `ModelRoot`/`CellPage` out of `calc-skin` keeps page wasm builds lean while `calc-skin-host` maps `ViewPart` cleanly. |
| 3 | **Target & binding representation** | Flat `Target` enum (`Workbook` included) + dual `Bound { anchor: AnchorId, address: String }` check on page bindings. | Splits `SourceRef` (Excel model entities, with sync `parse`/`display`) from `Target` (`Source \| Anchor \| Item`). | **Both combined:** `SourceRef` (including `Workbook`) nested inside `Target` (`Source \| Anchor \| Item`) prevents passing presentation items to model edits at compile time, while `Bound { anchor, address }` catches silent anchor/address drift (Pro 21 `U01`, `U05`). |
| 4 | **`calc-host` command boundary** | Keep `SkinIntent` inside `calc-host` indefinitely; translate `ModelChange -> SkinIntent` in `calc-skin-host`. | Replace `SkinIntent` with `dispatch(Command)` in `calc-host` so four command enums collapse into one (keeping `SkinIntent` alongside until the M2 switch). | **Opus's end-state, staged via Fable's `calc-skin-host`.** `SkinIntent` (`session.rs:145`) is only 97 lines. Keeping `SkinIntent` untouched through Milestones 0–2 avoids churn in `calc-host` tests and the frozen worker; retiring `SkinIntent` into `HostSession`'s batch path at the Milestone 3 switch achieves "one of everything" (AGENTS.md rule 4). `calc-object-model` stays separate for VBA/COM/Office.js. |
| 5 | **Batch vs multi-unit Transaction & conflict scope** | `Batch` = 1 atomic commit & 1 undo entry (can mix `Model` + `Presentation`), conflict-checked on `base` + `expects: Vec<Target>`. Multi-unit `Proposal` commits via `Decide` as grouped per-unit entries. | Every `Transaction` wraps `Vec<Unit>`, with whole-workbook `expect: Option<Revision>`. | **Fable.** Ordinary edits (including adding a notebook cell = formula + doc placement) must undo in one `Ctrl+Z`, not two. Multi-unit commit with per-unit undo (`commit_units`) is used for `Proposal` acceptance and Notebook `"Run all drafts"`. Target-scoped `expects` prevents unrelated sheet updates from falsely rejecting an Explorer `"Apply"`. |
| 6 | **Revision tracking** | `Stamp { workbook, model, view, calc }` in memory + saved `model` digest in XML. | Single `Revision` in memory + saved `model` SHA-256 fingerprint. | **Fable (both).** Splitting `Stamp.model` from `Stamp.view` makes `"presentation edit touched 0 model state"` an O(1) check on every frame and drives the Mast's `"Saved · layout edited"` state without hashing the workbook on every drag; the XML digest catches external edits in Excel. |
| 7 | **Watch delivery, Evidence & consistent snapshots** | Atomic `Frame { stamp, events, updates }` applied whole; `Evidence<T>` (`Observed` vs `Derived` vs `Unavailable`); `Shown<T>`. | `Invalidated` + re-query; `Budget` + `Observed::Declined`; `Provenance::External` + Pivot staleness; `consistent()` snapshot barrier. | **Fable's atomic `Frame` + `Evidence<T>` + `Shown<T>`, combined with Opus's per-query `Budget`, `External`/Pivot provenance, and `consistent()`.** `Frame` eliminates cross-watch tearing (harvest 17 finding 4) and implements `ScreenUpdating = False` cleanly; `consistent()` prevents mixed-revision prints and App views. |
| 8 | **Level-0 pure code & `dna-formula-ui`** | Extract `crates/calc-formula-kernel` (shared by page and `calc-formula-host`) and keep `apps/dna-formula-ui` (re-pointed to `calc-skin`). | Put completion in `dna-ui` over `FunctionCatalogue`; dissolve `dna-formula-ui` into `dna-ui`. | **Fable.** `calc-formula-kernel` gives one single implementation of tokenizing, completion, the read-only `Present` lens, `Interpret` and localized formatting on both page and host, and keeping `dna-formula-ui` preserves 4,001 lines of working, ceiling-checked code without an unnecessary rewrite. |
| 9 | **Serving crate location** | `crates/calc-skin-host` (portable library, multi-connection with `Origin` + `WritePolicy`), plus `FormulaHost::handle` in `calc-formula-host`. | `apps/dna-calc-service` and `apps/dna-formula-service`. | **Fable.** The desktop shell, `dnacalc serve`, the web worker and integration tests all embed the serving layer; placing it in `crates/calc-skin-host` (and level-0 serving in `calc-formula-host`) avoids peer dependencies between `apps/` crates and isolates level 0 cleanly. |
| 10 | **Workbook chart rendering & print pagination** | Keep `calc-chart-render` on the host (`Objects` returns SVG + series data); put `printing` on the page. | Move `calc-chart-render` to the page; move `printing::paginate` into `calc-host`. | **Fable on charts; Opus on printing.** Verified in code: `calc-chart-render` embeds 2.7 MB of Carlito `.ttf` fonts + `resvg`/`tiny-skia` for `Chart.Export` (`dc-oxh8w.22`) and PDF/print; keeping it host-side keeps the wasm UI small while `dna-kit::Plot` handles lightweight instrument plots. Moving `printing::paginate` into `calc-host` eliminates `calc-host`'s upward dependency on `apps/dna-calc-model` (`Cargo.toml:28`). |
| 11 | **Page crate granularity & Forms maker** | Separate `dna-grid` from `dna-kit`; Forms maker is a second `View` inside `dna-view-app`. | Put `GridSurface` inside `dna-ui`; separate `dna-skin-forms` crate. | **Separate `dna-grid` crate (Fable) + Forms maker as its own registered `View` (`"forms"`) inside `dna-view-app` (both).** Isolating ~13k lines of grid code in `dna-grid` prevents compile/merge contention with other views while registering `"sheet"` as a page component; making `"forms"` a separate `View` keeps `App` free of authoring chrome without adding crate overhead. |
| 12 | **Notebook `Esc`, backing layout & cell deletion** | `Esc` reverts draft; placement via `Placement` query; leaves deletion of placed cells open. | `Esc` leaves Edit Mode for Command Mode holding unrun text in session state (`"edited, not run"`); explicit scalar-zone + array-lane layout; `owned` vs `referenced` cells on delete. | **Opus on all three.** Under D6 ("works exactly like Jupyter"), `Esc` is Jupyter's Edit-to-Command mode transition — losing unrun multi-line formulas on `Esc` breaks Jupyter muscle memory, while holding unrun drafts in session state still never commits to the workbook. Scalar rows + array column lanes (`NC01`, `NC06`) and `owned` (clears cell) vs `referenced` (removes presentation only) resolve Notebook layout and deletion cleanly. |
| 13 | **View-switching shortcuts (D12)** | `Ctrl+Alt+1…5`, tested against Excel profile. | Checked Pro 19 `A-111`: Excel binds `Ctrl+Alt+5` (floating shapes), so bind `Ctrl+Alt+1…4` and reach App/Forms via `Alt+Q` / switcher; yield to AltGr characters. | **Opus.** Directly executes D12's collision rule against `A-111` and handles European AltGr layouts safely. |
| 14 | **View part namespace, v1 migration & session scroll** | `urn:dnacalc:view-manifest` v2 + interactive v1 `_dna.block.*` migration; scroll/camera in local storage only. | `urn:dnacalc:views:v1` over `calc-doc::custom_xml` + delete v1 code outright (0 user files); scroll/camera written into XML `<session>`. | **Opus on `urn:dnacalc:views:v1` & deleting v1; Fable on keeping scroll/camera in local storage & offering embedded skin themes/keymaps via Try-On.** Repo grep proves 0 real files use v1, so deleting v1 honours D10 and D13 cleanly. Keeping scroll/camera in local storage prevents noisy `.xlsx` diffs on pan/scroll, and holding file-embedded themes/keymaps under Try-On prevents a file from hijacking a recipient's keys. |
| 15 | **Extension sandbox** | Sandboxed `iframe` (HTML/JS) speaking slot-bound `ToComponent`/`FromComponent`. | Signed `.dnaext` Rust-to-Wasm in a Web Worker returning Strand `RenderTree` + `Invocation`s; Live Tree first. | **Opus's Wasm-in-Worker + `RenderTree`, constrained by Fable's strict slot-binding data boundary.** Gives zero DOM/CSS access, enforces Strand tokens/contrast/accessibility automatically, runs in Rust (`dna-ext-sdk`) including headless native tests, and exposes only bound slot values (`Shown<T>`), never raw workbook queries. |
| 16 | **Test harness & validation CLI** | New `dnacalc skin check` CLI + `data-dnacalc` roles. | Extend `cargo xtask app smoke --skin` + `window.__dnaProbe()` (`data-command`, `__dnacalcRenderPlans`). | **Extend `cargo xtask app smoke --skin` with `__dnaProbe()` for browser/screenshot verification ("Extend, never duplicate") + fast portable `dnacalc skin validate` in CLI for sub-second schema/contrast/keymap checks.** |
| 17 | **Build staging** | Stage 1 Explorer; Stage 2 thin Classic vertical slice; Stage 3 full Classic + switch commit; Stages 4–7 views in parallel. | M0 massive contract/host/service drop; M1 Explorer; M2 full Classic + switch; M3–M7 sequential. | **Fable's staged vertical slices (thin Classic in M2 before freezing view APIs, parallel view lanes M4–M7 after M2).** Putting a thin Classic on `dna-grid` in M2 exercises gestures, tile streams and optimistic selection before four other views build on the contract. |

---

## 3. Vocabulary, layers and mechanical dependency rules

### 3.1 Vocabulary

| Term | Meaning | Representation |
|---|---|---|
| **Skin interface** | The single portable contract between UI code and hosts. | `crates/calc-skin` |
| **Host level 0** | Workbook-free formula intelligence and trial evaluation (no `calc-engine`, no `calc-host`). | `calc-formula-kernel` (pure) + `calc-formula-host` (session) |
| **Host level 1** | Authoritative workbook state, graph recalculation, view part, anchors and history, owning a `FormulaHost`. | `calc-host`, served by `calc-skin-host` |
| **View** | A compiled Rust presentation engine mounting on the shell stage and translating gestures into commands. Five workbook views: `classic`, `inspector`, `notebook`, `map`, `app`. | Crate implementing `View` |
| **Tool** | A specialized surface that is not a standalone workbook model view: `explorer` (level 0, skin 0) and `forms` (authors `dna.page/1` pages over a live preview). | `FormulaView` (`dna-view-explorer`), `View` (`"forms"` in `dna-view-app`) |
| **Skin** | A declarative JSON manifest (`dna.skin/1`): theme overrides, density, shell layout, keymap layer, per-view settings, panel pages and named command sequences. No code. `skin-0` is the built-in Explorer skin. | JSON (`SkinManifest`) |
| **Page** | A declarative flow-layout tree (`dna.page/1`) of registered components bound to model anchors/addresses: App pages, skin panels, notebook control panels and Inspector benches. | JSON (`Page`) |
| **Component** | A reusable UI unit with typed read/write binding slots that renders `Shown<T>` values honestly on Strand tokens. | Built-in (`dna-kit`, `dna-grid`, `dna-view-inspector`) or signed `.dnaext` extension |
| **View document** | A view's per-workbook or per-sheet authored declaration (notebook cell list, map node layout, App pages, Inspector bench). | Versioned JSON inside `<decls>` in the view part |
| **View part** | The single custom XML item (`urn:dnacalc:views:v1`) storing anchors, preferences, view documents, embedded skins, review notes/findings and the agent change trail. | `CustomXmlItem` in `calc-doc` / `ViewPart` in `calc-doc-model` |
| **Anchor** | A stable view-part reference to a user name, table, object, spill or cell range with a structural fingerprint, transformed on local edits and reconciled after Excel edits. | `Anchor` (`AnchorId`) |
| **Participant & Port** | Any actor reading or mutating the session (user via view, agent, VBA/COM/Office.js automation, extension component), stamped with an unforgeable `Origin` and `WritePolicy` by its port/ingress. | `FormulaPort`, `WorkbookPort`, `CommandIngress` |

### 3.2 Layer stack and `tools/xtask/architecture.json` closure rules

```text
 Layer 7  Compositions     apps/dna-calc, dna-calc-worker, dna-calc-desktop,
                           dna-onecalc, dna-onecalc-desktop, dnacalc (CLI)
 Layer 6  Views & tools    apps/dna-view-{classic,inspector,notebook,map,app,explorer}
 Layer 5  Shell            apps/dna-shell
 Layer 4  UI kit & grid    apps/dna-kit, dna-grid, dna-formula-ui, dna-strand, dna-ext-sdk
 Layer 3  Client runtime   apps/dna-skin-runtime           (portable, no DOM, no Leptos)
 Layer 2  Skin contract    crates/calc-skin                 (portable, data + wire + ports)
 ───────────────────────── wire boundary (4 carriers) ─────────────────────────────────
 Layer 1  Serving          crates/calc-skin-host            (portable, L1 + L0 serving)
 Layer 0  Hosts & render   crates/calc-host (L1), calc-chart-render, calc-formula-host (L0)
 Below    Kernels & model  crates/calc-formula-kernel, calc-engine, calc-doc,
                           calc-doc-model, calc-fml, calc-func, calc-catalogue,
                           calc-value, calc-syntax, calc-addin-host, calc-vba
```

Mechanically checked rules added to `tools/xtask/architecture.json`:

1. **`calc-skin` closure forbids:** `calc-fml`, `calc-func`, `calc-doc-model`, `calc-doc`, `calc-engine`, `calc-host`, `calc-vba`, `calc-skin-host` and every `apps/*` package. It uses CalcValue identities through checked conversions from the existing serializable sheet, cell and range tuples, leaving CalcValue dependency-free.
2. **`calc-formula-kernel` closure forbids:** `calc-doc-model`, `calc-doc`, `calc-engine`, `calc-host`, `calc-vba`, `calc-addin-host` and every `apps/*` package.
3. **`calc-formula-host` closure forbids:** `calc-engine`, `calc-host`, `calc-skin-host` and every `apps/*` package.
4. **`calc-host` closure forbids:** every `apps/*` package (removing today's `dna-calc-model` dependency).
5. **`dna-skin-runtime` closure forbids:** `calc-doc-model`, `calc-doc`, `calc-engine`, `calc-host`, `calc-skin-host`, `leptos`, `web-sys` and every view/shell crate.
6. **Every `dna-view-*`, `dna-shell`, `dna-kit` and `dna-grid` closure forbids:** `calc-doc-model`, `calc-doc`, `calc-engine`, `calc-host`, `calc-skin-host`, `calc-formula-host` and `calc-chart-render`. Views additionally forbid `dna-shell` and every other `dna-view-*` crate.
7. **Level-0 ceiling (`dna-view-explorer`, `dna-formula-ui`, `dna-onecalc`, `dna-onecalc-desktop`):** closure forbids `calc-engine`, `calc-host`, `calc-skin-host`, `dna-grid` and the five workbook view crates. Checked both in `architecture.json` and in `cargo xtask sweep`'s binary/wasm ceiling check.

---

## 4. The skin interface (`crates/calc-skin`)

Every type in `calc-skin` derives `Clone, Debug, PartialEq, Serialize, Deserialize`. Page and host are built from the same commit; the control envelope checks an explicit contract schema version before decoding the payload, while persisted schemas (`urn:dnacalc:views:v1`, `dna.skin/1`, `dna.page/1`) and the extension component port (`dna.component/1`) are explicitly versioned.

### 4.1 Targets, `SourceRef` and the address grammar

One address grammar serves the Name Box, Go To, formula references, page slot bindings, VBA view requests, deep links and agent tools (Strand tenet 7).

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnchorId(pub NonZeroU128); // formatted as UUID in XML/JSON

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocId(pub NonZeroU128);

/// Model entities that exist in Excel itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceRef {
    /// One or more rectangles on a sheet (`Monthly!H13`, `Inputs!B2:D10,F2:F10`).
    Cells { sheet: SheetId, areas: Vec<GridArea> },
    /// Dynamic-array spill by anchor cell (`Analysis!A10#`).
    Spill { anchor: CellKey },
    /// Workbook- or sheet-scoped defined name (`ServicesShare`, `Sheet1!LocalRate`).
    Name { scope: NameScope, name: String },
    /// Structured table reference (`Orders`, `Orders[Amount]`, `Orders[#Headers]`).
    Table { name: String, part: TablePart },
    /// Embedded chart, shape, picture, form control or pivot by Excel name (`Loan!"Balance"`).
    Object { sheet: SheetId, name: String },
    Sheet(SheetId),
    Workbook,
}

/// Anything selectable, navigable or linkable across model and presentation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Source(SourceRef),
    Anchor(AnchorId),
    Item(ViewItemRef), // e.g., prose cell, map group, page section (`view:notebook/<doc>/c4`)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewItemRef {
    pub view: ViewId,
    pub doc: DocId,
    pub item: String,
}

/// Page/component slot binding: requires BOTH stable anchor and expected address to agree (Pro 21 U01, U05).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub anchor: AnchorId,
    pub address: String,
}
```

- **Local syntax formatting + host resolution:** `SourceRef::parse` and `SourceRef::display` format and parse direct A1/R1C1 addresses synchronously on the page using cached sheet/name tables (`Summary`), refusing ambiguous dual-scope names instead of guessing (Pro R-6). Complex or indirect expressions resolve through the `Resolve` query.
- **Inferred blocks are readings, not targets:** A contiguous formula run or inferred block is a `StructureItem` reading over `SourceRef::Cells`, never a phantom model entity. Pinning a name to it is an explicit `ModelChange::Name`.

### 4.2 The one selection and optimistic sequencing

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub seq: SelectionSeq,          // monotonic host sequence
    pub target: SourceRef,          // model selection (always valid on an open workbook)
    pub active: Option<CellKey>,    // Excel's ActiveCell when target is cell-backed
    pub grouped: Vec<SheetId>,      // multi-sheet group; edits write across grouped sheets
    pub item: Option<ViewItemRef>,  // presentation item holding focus, if selected from a view
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionCause {
    ClientEcho { client_seq: u64 },
    Macro { procedure: String },
    Agent { agent: String },
    History,
    Open,
}
```

- **Single authority, zero-latency cursor:** `HostSession` owns the authoritative selection (`Application.Selection` and `ActiveCell` read it). When the user presses an arrow key inside mirrored tiles, `dna-skin-runtime` updates its optimistic selection immediately and sends `select(selection, client_seq)`. When `Event::Selection` returns with `ClientEcho { client_seq }`, echoes older than the latest `client_seq` are ignored; external moves (`Macro`, `History`, `Agent`) always apply immediately (§17.7).
- **Honest cross-view location:** Every mounted view implements `locate(&Selection) -> Located` (`Exact`, `Within { item, label }`, or `Absent`). When a selection is coarser in the current view, the Strip states that; when `Absent`, the view offers `"Reveal in Classic"` and `"Add to this view"` and never silently substitutes another selection (Pro R-3).

### 4.3 Stamps, provenance, coverage and evidence

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub workbook: WorkbookId,   // changes when a different file is opened
    pub model: Revision,        // advances ONLY when Excel-visible content changes
    pub view: Revision,         // advances ONLY when urn:dnacalc:views:v1 changes
    pub calc: CalcGeneration,   // advances on every published calculation pass
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    Empty,
    Literal,
    Calculated { calc: CalcGeneration },
    AsSaved, // cached value from file; this engine has not calculated it (honesty rule 1)
    External { source: SourceId, observed_at: Option<TimestampMs> },
    Stale { since: Revision, cause: StaleCause },
    Refused { owner: String, reason: String }, // typed not-modelled refusal (honesty rule 2)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleCause {
    ManualMode,
    Calculating,
    CalcFailed,
    Cancelled,
    SourceUnavailable,
    UnrefreshedPivotCache { source_revision: Revision },
}

/// No query or component ever carries a bare cell value without its provenance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shown<T> {
    pub value: T,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coverage {
    Complete,
    Partial { done: u64, total: Option<u64>, why: PartialWhy },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer<T> {
    pub stamp: Stamp,
    pub coverage: Coverage,
    pub body: T,
}

/// Enforces Pro 19 push-back 2 by type: dependency structure is never shown as execution order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Evidence<T> {
    Observed { observer: ObserverId, at: Stamp, body: T },
    Derived { from: DerivationKind, at: Stamp, body: T },
    Unavailable { reason: String },
}
```

- **Excel errors stay values; missing capabilities stay provenance/refusals:** `#REF!`, `#NAME?`, `#SPILL!` and `#CALC!` are Excel values inside `T`; an unloaded add-in, uncalculated cache or unmodelled feature is reported via `Provenance`, never as a fabricated Excel error code (Pro R-9).

### 4.4 Changes, batches, preview (the seam) and operations

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Change {
    Model(ModelChange),
    Presentation(PresentationChange),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub id: CommandIdToken,             // deduplicated by host CommandJournal across retries/recovery
    pub base: Option<Revision>,         // model revision read when building this batch
    pub expects: Vec<SourceRef>,        // read-set checked for conflicts if model > base
    pub selection_seq: Option<u64>,     // refuses commands resolved against a superseded cursor
    pub label: Option<String>,
    pub changes: Vec<Change>,           // committed atomically as ONE transaction and ONE HistoryEntry
}
```

**`ModelChange`** covers every workbook mutation Excel itself expresses — unifying today's 67 page `WorkbookCommand` variants, 72 wire `Intent`s and `calc_host::WorkbookCommand`:
- *Cell entry & runs:* `Enter { at, text }` (via `interpret_entry`), `EnterAcross { areas, active, text, array }`, `EnterRun { run, formula_r1c1_or_a1 }` (authors a uniform relative formula across a run in one step), `Clear { areas, what }`.
- *Formula structural rewrites:* `ReplaceReference { at, occurrence, with }` (Model map port rewiring, Pro R-30), `RenameName { scope, from, to }` (updates name and formula references atomically), `RewriteFormula { at, expected_old, new_text }` (`"Rewrite as LET"` after preview, Pro R-5).
- *Clipboard, fill, structure & sheet layout:* `Paste { source, to, special }`, `Fill { from, to, kind }`, `Move { from, to, copy, insert }`, `FlashFill { area }`, `Shift(ShiftEdit)`, `Axis(AxisEdit)`, `Sheet(SheetEdit)`, `Merge(MergeEdit)`, `Panes(PaneEdit)`, `Outline(OutlineEdit)`, `PageSetup { sheet, change }`.
- *Formats, data & objects:* `Format(FormatEdit)`, `Conditional(ConditionalEdit)`, `Validation(ValidationEdit)`, `Name(NameEdit)`, `Table(TableEdit)`, `Filter(FilterEdit)`, `Sort(SortEdit)`, `Pivot(PivotEdit)`, `Scenario(ScenarioEdit)`, `Chart(ChartEdit)`, `Shape(ShapeEdit)`, `Sparkline(SparklineEdit)`, `Control(ControlEdit)`, `CellNote(CellNoteEdit)`, `Hyperlink(HyperlinkEdit)`, `Settings(WorkbookSettingsEdit)`.

Rule: **No `ModelChange` exists that cannot be expressed in Excel.** Dropping a wire onto `=A*B+C` with no unambiguous target operand is refused with the reason and the nearest valid Excel edit.

**`PresentationChange`** mutates `urn:dnacalc:views:v1` only:
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PresentationChange {
    CreateDoc { doc: DocId, view: ViewId, scope: DocScope, schema: u32, body: serde_json::Value },
    PatchDoc { doc: DocId, expect_view_rev: Revision, patch: Vec<JsonPatchOp> }, // RFC 6902; conflicts refused, never merged
    RemoveDoc { doc: DocId },
    Anchor(AnchorEdit),        // create, rebind, remove
    Prefs(ViewPrefsEdit),      // workbook preferred/supported views, per-sheet preferred badge
    Skin(EmbeddedSkinEdit),    // embed or remove travelling skin manifest
    Reading(ReadingEdit),      // confirm, correct or reset an inferred pattern reading
    Note(ReviewNoteEdit),      // anchored review note or finding (D5)
    Trail(TrailSettingEdit),   // toggle agent trail on/off (D14)
}
```

**`Preview` (the seam):**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewDepth {
    WritesOnly,                         // structural prepare without commit (fast, exact)
    WithEffects { budget: Budget },     // budgeted trial recalculation (§17.4)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub base: Stamp,
    pub kind: ChangeKind,               // Model | Presentation | Both — computed from `writes` & `presentation`, NEVER caller-supplied
    pub writes: Vec<ModelWriteDiff>,    // exact cells, formulas, ref adjustments, names, spills changed
    pub presentation: Vec<PresentationDiff>,
    pub warnings: Vec<PreviewWarning>,  // breaks formula run, closes cycle, blocks spill, writes hidden/grouped cells
    pub refusal: Option<Refusal>,
    pub effects: Option<Observed<ValueEffects>>,
}
```
- **Two UI forms of the seam:**
  1. *Inline label* (`"Model change · Loan!B3"` or `"Presentation only · 0 cells changed"`): shown automatically on bound App inputs, already-placed Notebook cells and the formula bar; `Enter` / `Shift+Enter` commits directly without an extra click.
  2. *Review card* (full diff + `Apply` / `Cancel`): shown whenever `Preview` has warnings or a refusal, allocates new cell placements (`Placement`), rewrites multiple cells/references (`EnterRun`, `ReplaceReference`, `RenameName`, `"Rewrite as LET"`), pastes into hidden/grouped cells, or previews an agent `Proposal`.

**`Operation`** covers session actions outside direct batch authoring:
`Undo`, `Redo`, `Revert(EntryId)`, `Calculate { scope, selection }`, `CancelCalculation`, `Open`, `New`, `Save`, `Close`, `CopyToClipboard`, `PreparePaste`, `RunMacro { procedure, args }`, `EnableMacros`, `Refresh`, `Decide { proposal, accept, reject }`, `TryOn(TryOnPayload)`, `RestoreCheckpoint(CheckpointId)`, `ViewRequest(ViewRequest)`, `Automate(AutomationBatch)`.

### 4.5 The one history

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: EntryId,
    pub at: Stamp,
    pub label: String,                  // "Entered Inputs!C5", "Moved box Stock", "Macro: Roll_Month_End"
    pub origin: Origin,                 // stamped by Port / CommandIngress, unforgeable by callers
    pub kind: ChangeKind,               // Model | Presentation | Both
    pub targets: Vec<Target>,
    pub cells_touched: u64,
    pub reversibility: Reversibility,
    pub group: Option<GroupId>,         // ties accepted Proposal units or macro runs
    pub unreviewed: bool,               // set under Agent ApplyThenReview until acknowledged
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    User { view: ViewId },
    Agent { agent: String, proposal: ProposalId },
    Macro { run: RunId, procedure: String },
    Automation,                         // COM / Office.js
    Extension { id: String, view: ViewId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversibility {
    Reversible,
    DocumentOnly { external_effects: Vec<ExternalEffect> }, // experimental macro restore (§17.1)
    Irreversible { reason: IrreversibleReason },            // unloaded cell, v1 macro boundary, Excel non-undoable op
}
```

- **Undo order:** `Ctrl+Z` first undoes inside an active text draft (`EditSession`'s local `TextHistory`); once committed, `Ctrl+Z` undoes the top `HistoryEntry` of the workbook regardless of which view created it. Undoing an entry created in another view stays in the current view and displays `"Undid: moved box Stock (Model map · presentation) · [Show]"` in the Strip.
- **`Revert(EntryId)`:** Computes a new forward batch restoring the targets of an earlier step, refusing with exact cell conflicts if subsequent steps modified the same targets.

### 4.6 Neutral queries, budgets, watches, atomic frames and `consistent()`

Every query implements `Query` (`FormulaQuery` at level 0, `WorkbookQuery` at level 1) and accepts a `Budget { cells, nodes, edges, bytes, millis }`. Over-budget requests never hang or crash; they return `Refusal::OverBudget { counts, alternatives }` offering narrower scopes.

| Level-1 Query | Answer summary | Primary consumers |
|---|---|---|
| `Summary` | Workbook name, dirty/state word (`Unchanged`, `Edited`, `Saved · layout edited`, `Read-only`), sheet descriptors (`SheetKey`, `SheetId`, name, visibility, preferred badge), date system, macro status, `Capabilities` | Shell, all views |
| `Census { scope }` | Zero-calculation counts: formulas, literals, errors by kind, used extents, objects | `View::fit` before mounting |
| `Inventory { kinds }` | Full or paged sheets, names, tables, charts, shapes, pivots, scenarios, external links, VBA modules/procedures, active UDFs | Shell, Inspector, Map |
| `Cells { interest }` | Tile-streamed cells (`input`, `Shown<CellValue>`, formatted text, style, spill/array membership). `Interest` = `Area`, `Areas`, `Sheet`, `Selection`, or **`Anchors(Vec<AnchorId>)`** (resolving spills to their live extent) | Classic, Notebook, App, Watch |
| `Geometry { sheet }` | Row/column spans, hidden axes, outlines, merges, frozen/split panes, sheet zoom | Classic, `dna-grid` |
| `Formula { at }` | Parsed syntax spans, resolved `SourceRef`s, R1C1 form, run membership | Formula bar, Lens, Notebook, Map |
| `Deps { of, dir, depth, group, budget }` | `Evidence::Derived`: edges grouped by cell, run, block, name or sheet; marks dynamic (`INDIRECT`/`OFFSET`), table and unresolved refs; exact member counts | Inspector, Map, Notebook |
| `Structure { scope, level, budget }` | `Evidence::Derived` + file facts: `Sheet`, `Table`, `Name`, formula `Run` (with R1C1 template and exception cells), inferred `Block` (`Inputs`, `Calculation`, `Outputs`, `Text`) and `Reading` (`FromWorkbook`, `Suggested`, `Confirmed`, `NeedsReview`) | Notebook draft, Map, Forms maker |
| `Issues { scope, budget }` | `Evidence::Derived`: formula errors grouped by root-cause cell with downstream dependent counts; structural warnings | Inspector, sheet tab badges |
| `Sources` | External workbook links, live RTD topics (last value, source/receipt timestamps, coalesced count), volatile functions, add-in providers | Inspector Live, Strip |
| `CalcState` & `CalcEvents` | Mode, iteration settings, progress, cycle members and stop reason (`CalcState`); `Evidence::Observed` list of host `CalculationEvent`s, or `Unavailable` if unrecorded (`CalcEvents`) | Strip, Inspector Calculation, Map |
| `Changes { calc }` | Cells whose value changed in generation `calc` (old vs new `Shown<CellValue>`) | Inspector Changes, Map ripple |
| `Explain { at, span }` & `Contributions { at, budget }` | Workbook-contextual sub-expression evaluation (`F9` on a selection, error origin) and passing/near-miss row breakdown for `SUMIFS`/`FILTER`/`XLOOKUP` | Formula bar, Inspector Trace |
| `Rows { source, filter, sort, window }` | Windowed, view-sorted/filtered rows over a table, spill or range with exact total count without mutating Excel's `AutoFilter` | `DataFrame` component |
| `Objects { sheet }` | Charts (**host-rendered SVG** + structured `ChartData` series), shapes, pictures, sparklines, form controls with linked cells | Classic, Notebook, App |
| `Rules { at }` | Data validation, conditional format rules, lock/protection status, editability refusal reason | Classic, App, Forms maker |
| `Placement { sheet, kind, shape }` | Allocates non-colliding backing coordinates (scalar zone vs array column lane, or next block row) | Notebook, Map |
| `TrialFor { at }` | Snapshot `Trial` (`formula`, `CallerSnapshot`, precedent `Shown<CellValue>`s, `Stamp`) for level-0 exploration | Shell -> Explorer |
| `PrintPlan { target, setup }` | Calibrated page breaks (`calc_host::print_plan`) and page layout | Print preview / export |
| `ViewPart` | Anchors with resolution status (`Bound`, `Relocated`, `Orphan`), preferences, document headers, notes, trail, embedded skins | Runtime, all views |
| `History` & `Proposals` | Undo/redo `HistoryEntry` lists, checkpoints, pending agent `Proposal`s | Shell History, Agent panel |
| `Find`, `Special`, `Stats`, `Resolve` | Find/Go To Special, status-bar selection aggregates, complex reference resolution | Shell, Strip |

**Watches and atomic `Frame`s:**
- `Cells` watches use the proven binary tile stream (admission, fragments, completion barrier, single in-flight window, slow-subscriber reset coalescing).
- Every other watched `WorkbookQuery` records its read footprint (sheets, areas, names, view-part sections) in `calc-skin-host`. When a commit or calculation intersects that footprint, `calc-skin-host` re-evaluates it within its budget (or marks it `Update::Outdated { since }` while computing) and bundles all resulting updates and `Event`s of that step into one **`Frame { stamp, events, updates }`**.
- `dna-skin-runtime` applies each `Frame` atomically before notifying signals, preventing any component from observing half-updated state across watches. While a macro runs with `Application.ScreenUpdating = False`, `calc-skin-host` withholds view frames (except Strip `Macro` progress events) until `ScreenUpdating = True` or the run ends.
- **Consistent snapshot barrier (`consistent()`):** Print jobs, exports and multi-output App summaries read through `runtime.consistent(...)`, which yields once all bound reads stand at the same `CalcGeneration` (or pins the last complete generation with a timestamp badge when continuous RTD ticks prevent quiescence, §17.10).

### 4.7 Ports, participants, write policies and carriers

```rust
pub enum WritePolicy {
    Commit,             // interactive user views
    ApplyThenReview,    // opt-in agent mode: commits marked `unreviewed: true`
    Propose,            // default agent mode: `commit` refused; must call `propose`
    Look,               // read-only file open or read-only agent permission
}
```

- **`FormulaPort`** exposes `ask<Q: FormulaQuery>`, `run(FormulaOperation)`, `events` and `capabilities`. A crate given only `FormulaPort` (`dna-view-explorer`) cannot name or issue any workbook read or write.
- **`WorkbookPort`** embeds `&FormulaPort` and adds `ask<Q: WorkbookQuery>`, `watch`, `select`, `navigate`, `preview`, `commit`, `propose`, `run(Operation)` and `frames`.
- **`Origin` and `WritePolicy` are fixed at connection creation** in `SkinHost::connect(origin, policy)` — no wire message can escalate its own policy or spoof another participant's `Origin`.
- **Four `Carrier` implementations** transport `(Envelope, Option<Vec<u8>>)` identically: browser `Worker` (`postMessage` + transferable `ArrayBuffer`s), `WebView2` native bridge, `dnacalc serve` loopback WebSocket, and direct in-process memory channels for tests.

### 4.8 Capabilities and typed refusals

`Capabilities` declares upfront what the active host/platform supports: `read_only`, `vba` (`Off`/`Guarded`/`Unguarded`), `native_addins` (`.xll`, COM, `.xlam`), `com_rtd`, `native_file_dialogs`, `excel_clipboard_formats`, `printing`, and `platform_reserved_chords`.

`Refusal` distinguishes every failure class cleanly:
- `NotModelled { owner: String, what: String }` (honesty rule 2: names the owning bead/package, counted as a failure in corpus/parity metrics);
- `NotAvailableHere { capability: Capability, route: Option<Link> }` (e.g. `.xll` in browser -> link to open in desktop app);
- `Conflict { base: Revision, conflicts: Vec<TargetConflict> }` (stale base on `Batch` or `Proposal`);
- `StaleSelection`, `ReadOnly`, `Protected(SourceRef)`, `Validation(ValidationSummary)`, `Excel(ExcelRefusal)`, `OverBudget { counts, alternatives }`, `PolicyRefused`, `Irreversible(Irreversible)`.

---

## 5. Host levels, serving and chart rendering

### 5.1 Level 0: `calc-formula-kernel` and `calc-formula-host`

Both proposals chose option (b) of [FRONT_END_IMPLEMENTATION_BRIEF.md](../../plan/FRONT_END_IMPLEMENTATION_BRIEF.md) item 9: level 0 lives in its own crates below `calc-host`, enforced by `architecture.json` rather than Cargo features (which unify across workspace builds and erode).

1. **`crates/calc-formula-kernel` (pure, stateless, linked by both UI wasm and hosts):**
   - `Analyse`: token spans, reference extraction, syntax diagnostics, caret completion and signature help over a `CatalogueSnapshot` (`calc-catalogue` built-ins + dynamic `FunctionInfo` descriptors pushed via `FormulaEvent::ProvidersChanged` + workbook names). Keystrokes never wait on an IPC round trip.
   - `Present`: the **read-only formula lens** (structured indentation, LET/LAMBDA visual grouping, human-readable chips with bidirectional span mapping to exact Excel text). Never mutates formulas.
   - `Interpret`: parses view-scoped shorthand into exact Excel formula syntax (for preview in the seam), refusing ambiguous short names.
   - `FormatValue`: pure value-to-text formatting via `calc_func::cell_text_localized`, keeping the underlying `CalcValue` distinct from formatted display text (Pro R-41).
   - `interpret_entry`: Excel raw-entry parsing (moved down from `calc-host/src/entry.rs` so both levels share one parser).

2. **`crates/calc-formula-host` (stateful level-0 session, no `calc-engine`):**
   - Built directly by moving `apps/dna-onecalc/src/{session.rs, cf.rs, rtd.rs, trace.rs, fixture.rs, file.rs}` (~5,940 lines of tested code) plus RTD topic admission from `crates/calc-host/src/recalc.rs:452-552`.
   - Owns `CallableTable` (built-ins + XLL/COM/JS add-ins + loaded VBA module UDFs), `ProviderRegistry` (recording preview-safety flags so side-effecting UDFs never fire on keystrokes, Pro R-42), `RtdTopics` ledger, `CalcEnvironment` (date system, locale) and `MacroPolicy`.
   - Answers `Evaluate` (single `Trial` with optional `CallerSnapshot` — refusing caller-dependent functions like `ROW()`/`CELL("address")` when `caller: None` with a route to a workbook cell, Pro R-39), `EvaluatePart`, `Trace` (live evaluation tree), formula-bearing `ConditionalFormat`, `Catalogue` (including witness-bank Excel match status), `Providers`, and Explorer `.xlsx` session load/save.

### 5.2 Level 1: `calc-host` and `calc-skin-host`

`crates/calc-host` remains the single owner of `ModelRoot`, `CalcEngine` and `History`, with six clean changes:
1. **Owns `FormulaHost`:** `HostState` embeds `FormulaHost` and feeds `engine.set_callable_bindings` and `engine.invalidate_rtd_topic` from it — zero duplicated UDF or RTD admission state.
2. **Fixes the `dna-calc-model` layering inversion:** Moves the PB1-calibrated paginator and its fixtures into `crates/calc-host/src/printing/`. Print settings, the published plan and pure row/column metrics are shared by the host and the legacy preview through `calc-skin`. The shared print types also parse header/footer fields, while the preview draws them. `Worksheet.HPageBreaks`, `VPageBreaks`, `PageSetup.Pages` and the `PrintPlan` query all call one host module.
3. **`ViewPart` replaces `SkinLayout`:** `ModelRoot.layout` becomes `Arc<ViewPart>`, edited via `WorkbookOp::EditView` through the existing `History` inverse/restore machinery, with `model_revision` tracked separately from `view_revision`.
4. **Structural edits transform anchors:** `calc_fml::reference::StructuralEdit` (row/col insert/delete, cell shift, cut-and-paste move, sheet rename) updates `ViewPart` anchors in the same atomic commit as formula references.
5. **Non-committing `prepare` and target conflict check:** Exposes `prepare` without committing (returning node IDs on drop) to power `PreviewDepth::WritesOnly`, and checks `expects` against per-step touched targets since `base`.
6. **Macro run scopes & checkpoints:** Adds `begin_run` / `end_run`, `Application.OnUndo`, and O(1) `ModelRoot::pin` checkpoints (§17.1). At Milestone 3's switch commit, legacy `SkinIntent` is retired into `HostSession`'s batch dispatcher.

`crates/calc-skin-host` replaces `apps/dna-calc-worker/src/server.rs`'s ad-hoc dispatch with a clean multi-connection server (`SkinHost`): query handlers, watch footprint invalidation, binary tile `publication.rs` (kept), `CommandJournal` deduplication (kept), `Proposal` staging, `ScreenUpdating` frame withholding, and budgeted `advance()` slices.

### 5.3 Workbook charts (`calc-chart-render`) vs instrument plots (`dna-kit::Plot`)

`crates/calc-chart-render` stays on the host side (`dc-oxh8w.22`). An Excel chart object is authored workbook content whose rendering must match across the worksheet canvas, `Chart.Export` (PNG/JPG/GIF via `resvg`/`tiny-skia` and 2.7 MB of bundled Carlito fonts) and PDF/print output. `Objects` returns both the host-generated SVG (with `data-series`/`data-point`/`data-value` hooks) and structured `ChartData` series values so `"Open chart data"` and screen-reader tables read exact numbers without shipping 2.7 MB of fonts and a rasterizer in the page wasm bundle. View-specific diagnostic plots (Inspector live signal traces, sparkline strips) use `dna-kit`'s lightweight client-side `Plot` component over neutral query data.

---

## 6. Page architecture: runtime, kit, grid and shell

### 6.1 `dna-skin-runtime` (portable, DOM-free, toolkit-free)

`apps/dna-skin-runtime` holds all reactive state, input ordering and validation logic without touching `web-sys` or `leptos`, so every rule runs natively and under `wasm-bindgen-test` in headless Chrome and Firefox:

- **Frame store & `Live<T>`:** Applies incoming `Frame`s atomically, maintains `tile_mirror.rs` (kept from `dna-calc-model`) for `Cells` streams, and exposes `Live<T>` handles (`Loading`, `Ready(Answer<T>)`, `Outdated`, `Refused`) bridged to Leptos signals in `dna-kit`.
- **Excel input modes as explicit states:** Replaces today's boolean `point_mode` flag with a real state machine:
  ```rust
  pub enum InputMode { Ready, Enter, Edit, Point, Object, KeyTips(Vec<char>), NotebookCommand }
  pub struct ModeFlags { pub end: bool, pub extend: bool, pub add: bool, pub scroll_lock: bool }
  ```
- **One `EditSession` + view-held drafts:** Shared active formula/text draft across in-cell editing, the Bridge formula bar, Notebook cells and Map boxes (surviving view switches without implicit commit, Pro R-4), plus session-local held drafts for unrun Notebook cells (§7.3).
- **Ordered `InputQueue` (exact type-ahead):** Keystrokes resolvable inside mirrored state (in-mirror cursor moves, draft typing) apply immediately; keys requiring host round trips (`Ctrl+Arrow` to an unmirrored data edge, `Enter` commit) enqueue a barrier behind which subsequent keys buffer and replay deterministically in arrival order.
- **`CommandTable`, `Keymap`, `Places` (Back/Forward) and `SkinValidator`.**
- **Semantic test `Probe`:** Powers `window.__dnaProbe()` (exposing selection, mode, draft, visible cells with provenance, Strip readouts, history entries, open dialogs) while retaining `__dnacalcRenderPlans`, `data-dnacalc` roles and `data-command="<CommandId>"` attributes so the 937 + 121 smoke cases port cleanly and survive visual restyling.

### 6.2 `View`, `FormulaView` and the `Registry` (`dna-kit`)

```rust
pub struct Registry { /* views, formula tools, panels, page components, commands, built-in skins */ }

pub trait View: 'static {
    fn spec(&self) -> ViewSpec;
    /// Checked from zero-calc `Census` counts BEFORE mounting; shell renders unified Declined UI.
    fn fit(&self, census: &Census, part: &ViewPartSummary) -> Fit;
    fn mount(&self, cx: ViewCx) -> MountedView;
}

pub enum Fit {
    Whole,
    Simplified { shown: u64, total: u64, explanation: String },
    Declined { counts: Census, reason: String, alternatives: Vec<WayForward> }, // >= 2 actionable routes
}

pub struct MountedView {
    pub body: AnyView,
    pub locate: Box<dyn Fn(&Selection) -> Located>,
    pub reveal: Box<dyn Fn(&Target)>,
    pub print: Option<Box<dyn Fn(&PrintRequest) -> PrintJob>>,
}
```

Each view crate exposes `pub fn install(reg: &mut Registry)` registering its `View` (or `FormulaView` for `dna-view-explorer`, which receives only `FormulaPort`, an optional `Trial` snapshot and an optional `on_apply` callback), its `CommandSpec`s, its dockable panels and any reusable page components. Only the top-level app compositions (`apps/dna-calc`, `apps/dna-onecalc`) call `install`.

### 6.3 `dna-kit`, `dna-grid` and `dna-formula-ui`

- **`dna-kit`** provides the Strand Leptos component library (views write **no CSS**): `ValueView` (renders `Shown<CellValue>` with provenance marks), `InputControl` (field, slider, stepper, dropdown, checkbox, date — driven by Excel `Validation`), `DataFrame` (virtualized window over `Rows` with `"n of N"` coverage and non-mutating view sort/filter), `ChartFrame` (host SVG + `"Open chart data"`), `Plot` (client SVG/Canvas trace plot), `RichProse` (Markdown with live `{=Name}` / `{{Sheet!A1}}` chips), `NotebookExcerpt` (embeds live notebook cell outputs in reader pages), `ScenarioPicker`, `ActionButton` (command or Guarded macro), `PeekCard`, `Seam` (inline label + review diff card), and the **`PageRenderer`** (`dna.page/1`).
- **`dna-grid`** is its own crate isolating the ~13,000 lines of Excel grid logic and rendering: kept DOM-free `geometry`, `viewport`, `render_plan`, `drawn_shapes`, `sparklines` and `controls` from `dna-calc-model`, kept `painter` (Canvas2D + DOM, ready for WebGPU over `RenderPlan`), `panes`, `axis`, `decoration`, `alignment` and `autofit` from `apps/dna-calc/src/grid/`, and fresh Leptos gesture recognisers over `dna-skin-runtime`. `dna-grid` registers `"sheet"` in `Registry`, so Classic, the companion pane, Map sheet pages and embedded sheet excerpts on `dna.page/1` pages all mount one grid implementation.
- **`dna-formula-ui`** (4,001 lines, kept) provides the shared formula editor (syntax colouring, completion popup, signature help, `F4` reference cycling, `F9` sub-expression evaluation, point-mode range chips) used identically in Classic, Bridge, Notebook, Map and Formula Explorer.

### 6.4 `dna-shell`

One shared chrome across web and desktop, in fixed regions:
- **Mast:** workbook title and host-computed state word, view switcher, `Alt+Q` unified search/command/Go-To/agent bar, agent status chip.
- **Deck:** compact Strand command bands with Excel's tab structure and exact key-tip letters (`D11`).
- **Bridge:** Name Box / `Target` address bar and `dna-formula-ui` formula bar.
- **Stage + Companion:** one primary view and at most **one** companion pane (Classic grid by default), sharing the one selection (never free-tiling chaos or multi-view tabs that hide the shared workbook).
- **Dockable Panels:** right or bottom slots hosting any Inspector instrument or skin-declared `dna.page/1` panel.
- **Sheet Tabs:** sheet list with preferred-view badges (never auto-switching views on tab click).
- **Strip:** **never hidden under any skin** — input mode (`Ready`/`Enter`/`Edit`/`Point` + flags), calculation state/progress, error count, external/RTD status, macro status, agent state and selection aggregates.
- **Overlays & Dialogs:** the Seam, macro/extension Trust Cards, the Try-On banner, non-modal open Notices, Print Preview, and Classic's Excel dialogs (Format Cells, Paste Special, Name Manager, Conditional Formatting, Data Validation, Page Setup, Find & Replace) built cleanly over `WorkbookPort` instead of raw wire requests.
- **At level 0 (`dna-onecalc`):** `dna-shell` mounts Mast, Stage and Strip from `skin-0` — zero duplicate shell code.

---

## 7. The five views and two tools on the contract

None of the views below defines a custom host projection or wire message.

### 7.1 Classic (`dna-view-classic`)
- **Reads:** `Cells` viewport stream per pane, `Geometry`, `Objects`, `Rules`, `Stats`.
- **Stores in view part:** Nothing (panes, zoom and sheet selection live in Excel's standard `<sheetViews>`).
- **Writes:** Full `ModelChange` vocabulary.
- **Fit:** Never declines a workbook.

### 7.2 Inspector (`dna-view-inspector`)
- **Exports as dockable panels in every view AND arranges on the Inspector stage (`dna.page/1` bench):** `Details` (pinned peek), `Trace` (`Explain`, `Contributions`), `Dependencies` (`Deps`), `Issues` (`Issues` by root), `Calculation` (`CalcState`, `CalcEvents`, `Changes`), `Live` (`Sources` with user-local pause/record/replay that never pauses the underlying feed, Pro 21 `I01`), `Watch` (`Cells` over pinned anchors), `Overview` (`Census`, `Inventory`, `Structure`), `History` (`History`, `Proposals`) and `Notes` (`ViewPart` review notes, `D5`).
- **Honest by type:** Every claim comes directly from `Evidence<T>` and `Coverage`.

### 7.3 Notebook (`dna-view-notebook`) — exact Jupyter interaction (D6)
- **Chain first:** Ordered sequence of **formula cells** (exact Excel formula above, live `ValueView` / `DataFrame` / `ChartFrame` output directly beneath) and **prose cells** (Markdown in the view document with live bound `{=Name}` chips), plus optional inline `dna.page/1` control strips.
- **Jupyter Command & Edit modes (`jupyter` keymap layer):**
  - `Enter` enters Edit Mode (inside the editor, Excel formula keys `F2`, `F4`, `F9` part-eval and range pointing work).
  - **`Esc` in Edit Mode transitions to Command Mode while keeping unrun text as a session-local held draft** (`"Edited · not run"`, output beneath remains the committed formula's value labelled as prior; `Revert cell` or editor `Ctrl+Z` discards it; `Esc` never commits to the workbook). Switching views preserves held drafts and notes `"N notebook cells edited, not run"` in the Strip.
  - `Shift+Enter` (commit & advance/append), `Ctrl+Enter` (commit in place), `Alt+Enter` (commit & insert below) commit the cell's `Batch` (showing the inline destination label on known cells). `Run All` commits all held drafts as multi-unit entries (`commit_units`).
  - Command keys: `J`/`K` or `Up`/`Down`, `A`/`B` (insert above/below), `M`/`Y` (prose/code), `D D` (delete), `C`/`X`/`V` (copy/cut/paste cells), `O` (fold output), `L` (line numbers); `Ctrl+Z` invokes the one workbook history.
  - **No separate Read Mode (`D6`):** Opening a read-only file disables edits via `Capabilities.read_only`. Polished reader reports without code cells are `dna.page/1` pages built in the **Forms maker** (§7.6, `NC07`).
- **Two cell ownership kinds (`NC01`, `S03`):**
  - *Owned cell:* placed by the Notebook on its visible backing sheet (`Analysis` or beyond an existing sheet's used range). Deleting (`D D`) clears the backing cell and optional name in the same `Batch` (review card shown if dependents exist).
  - *Referenced cell:* anchored to a pre-existing workbook cell (`Summary!B7`). Editing edits `Summary!B7`; deleting (`D D`) removes the Notebook presentation entry only by default (with an explicit seam option to clear the cell).
- **Backing-sheet allocator — scalar rows & array column lanes (`NC06`):** `Placement` assigns scalars to rows in the scalar zone (label in column left) and dynamic arrays (or unknown-shape formulas) to vertical column lanes separated by spacer columns so downward spill growth never collides. If a formula later spills wider or into an obstruction, Excel yields `#SPILL!` (calculation never triggers hidden edits) and the Notebook offers previewed 1-step model fixes (relocate lane via Excel cut-and-paste, move obstruction, or insert columns).
- **Unauthored workbooks:** Opens a non-persisted draft built from `Structure` (formula runs and tables), clearly marked `"Drafted from sheet structure · Keep to save in view part"`.

### 7.4 Model map (`dna-view-map`)
- **Reader first:** Renders `Structure` (sheets, blocks, named cells, tables) as hierarchical boxes and `Deps` as counted, typed wires (`<= 200` nodes and `<= 400` edges per viewport; beyond that, counted groups and focused submaps, declining `"Expand all"` with scoped alternatives, Pro 21 `M03`).
- **Cycles vs delays (`D3`, push-back 1):** Iterative circular references from `CalcState` render inside an explicit feedback iteration enclosure; prior-period references confirmed by `Structure` render with a `z⁻¹` delay mark (unconfirmed cross-period links show `Unknown`, `M02`). Ticked models (`DnaCalc.Clock` RTD or macro ticks) bind standard `InputControl`s to the clock formula's rate/run/step cells.
- **Port edits through the seam:** Rewiring calls `ModelChange::ReplaceReference`; renaming calls `RenameName`; adding a box writes a label cell, value cell and `Name` via `Placement` + `PatchDoc` in one `Batch`.

### 7.5 App (`dna-view-app` — runner view)
- **Pure face for model users:** Mounts `PageRenderer` over the workbook's `dna.page/1` App document with **zero authoring handles**. Supports live mode (inline destination label, `Enter` commits under Excel `Validation` and protection) and batched mode (drafts held in `Runtime` until `Apply`), plus `ScenarioPicker` and Guarded macro `ActionButton`s. Never hides the Strip or the one-click return to Classic. If no App document exists, states `"No App defined"` and offers Classic or Forms maker.

### 7.6 Forms maker (`"forms"` view in `dna-view-app`)
- **Dedicated authoring view (`D6`):** Mounts the live `PageRenderer` on stage with selection handles, flanked by a candidate rail (`Structure` + `Rules`: unlocked/validated inputs, terminal outputs, tables, charts, Notebook excerpts — certain evidence only, never auto-published), a Classic companion picker sharing `Selection`, and a property inspector.
- **Authoring everything `dna.page/1`:** Edits App pages, reader-style Notebook reports (`NC07`), skin panels and Inspector benches as `PresentationChange::PatchDoc` steps. Model requests (adding a defined name or validation rule) are separate previewed `ModelChange` batches.

### 7.7 Formula Explorer (`dna-view-explorer`, Skin 0)
- **Strictly level 0 (`FormulaView`):** Formula editor, input table with trial overrides, `ValueView` + localized formatting playground (`E02`), step/live evaluation tree (`E01`), function browser with Handbook docs and witness-bank Excel match counts, and add-in provider inspector (`E03`).
- **Standalone (`dna-onecalc`, `dna-onecalc-desktop`) & Embedded (`dna-calc`):** Standalone links zero `calc-engine` / `calc-host`. Inside DNA Calc, `"Explore formula"` passes `TrialFor { at }`; `"Apply to cell"` dispatches a level-1 `Batch` with `base` and `expects: vec![cell]`, refusing stale conflicts (`Pro R-38`).

### 7.8 Effort summary once the foundation lands

| Surface | Host work required | UI effort once queries & `dna-kit` exist | Why |
|---|---|---|---|
| **Formula Explorer** | Move existing `Session` to `calc-formula-host` | **Hours** | Evaluator, trace, CF and catalogue logic already exist (5,940 lines). |
| **App & Forms maker** | None beyond `Cells`, `Rules`, `Objects`, `ViewPart` | **Hours each** | Pure composition of `PageRenderer` and `dna-kit` controls. |
| **Inspector** | Grouped `Deps`, root `Issues`, `Sources`, `Contributions` | **Hours per instrument** | Each panel is one `dna-kit` card/table over 1–2 neutral queries. |
| **Notebook** | `Placement`, `Rows`, `Structure` runs | **Hours** for chain UI; **1–2 days** for lane allocator & relocation fixes | Chain UI is a vertical/horizontal list of `dna-formula-ui` + output components; spill-lane relocation fixes need thorough edge-case tests. |
| **Model map** | Multi-level `Structure`, grouped `Deps` index | **Hours** for reader over small models; **days** for hierarchical layout & LOD | Graph layout stability and 200-node LOD bundling are genuine algorithmic UI work (§17.2). |
| **Classic** | None new (`Cells` tiles, `Geometry`, `Objects` exist) | **Days** for `GridSurface` + dialogs; **ongoing weeks** for Excel gesture tail | Carries 121+ Excel keyboard/mouse conformance rules and 13 dialog families (§17.3). |

---

## 8. Skins, pages, tokens and keymaps

### 8.1 Skin manifest (`dna.skin/1`) and Page schema (`dna.page/1`)

A skin is declarative JSON (`extends` chains to `dna:default` or `dna:explorer`):
```json
{
  "schema": "dna.skin/1",
  "id": "calm-review",
  "name": "Calm Review",
  "extends": "dna:default",
  "level": "workbook",
  "theme": { "base": "cockpit-light", "density": "working",
             "overrides": { "grid.lines": "none", "formula.font.size": "15.5px" } },
  "shell": { "deck": "compact", "panels": { "right": ["inspector.trace", "audit-rail"] },
             "strip_pins": [{ "address": "Summary!B7", "label": "Net Margin" }] },
  "keymap": { "profile": "excel-365-win-en-us",
              "bindings": [{ "when": "view:classic && mode:ready", "chord": "Alt+Shift+X",
                             "command": "inspector.next-issue" }] },
  "commands": [{ "id": "skin.total-style", "label": "Total Row Style",
                 "steps": [["format.borders", { "top": "thin", "bottom": "double" }],
                           ["format.bold", { "on": true }]] }],
  "views": { "classic": { "provenance_tints": true }, "notebook": { "layout": "down" } },
  "pages": { "audit-rail": { "schema": "dna.page/1", "title": "Audit", "input": "live", "root": { "...": "..." } } },
  "requires": { "extensions": [{ "id": "dnakode.live-tree", "version": "^1.0" }] }
}
```

- **Travelling skin safety rule:** When a workbook embeds a `<skin>` in `urn:dnacalc:views:v1`, its *per-view settings and pages* apply on open (they belong to the workbook's authored presentation), whereas *theme overrides, keymap changes and shell layout overrides* are **offered under the Try-On banner** (`Trying "Calm Review" · Keep for this file · Keep for me · Discard [Esc]`) and never rebind a recipient's keyboard unasked.
- **`dna.page/1` layout nodes:** Flow-based (`Stack`, `Row`, `Grid { columns }`, `Section { folded }`, `Tabs`, `Part { id, component, bind: BTreeMap<Slot, Bound>, props }`) — never absolute pixel coordinates, guaranteeing clean reflow at 200% and 400% zoom (`Pro R-56`). Pages hold zero formulas; all computation lives in workbook cells.

### 8.2 Strand tokens and runtime contrast enforcement (`dna-strand`)

`dna-strand::resolve(base_theme, &overrides) -> Result<ResolvedTheme, Vec<ContrastViolation>>` evaluates all 61 `SANCTIONED_PAIRS` (`contrast.rs:77`) at runtime and emits `--dna-*` CSS custom properties applied via CSSOM (`apps/dna-calc/src/style.rs`, respecting the no-inline-style CSP).
- **Open to skins:** neutral surfaces, chrome, borders, accent hue, type scale, density, view overlays.
- **Closed to skins:** semantic signal roles (red = error, amber = focus/now, orange = external/volatile, green = fresh, teal = structure), focus-ring dual strokes, line-style epistemic channel (solid = authored, dashed = inferred, dotted = ghost/preview), Strip readouts, Trust Cards, and **explicit workbook cell formatting** (in dark themes only `Automatic` text/gridlines invert; authored fills, red negatives and conditional formats keep their exact colours, Pro 21 `U04`).

### 8.3 Keymaps, Excel key tips (D11) and view shortcuts (D12)

- **Single profile data table:** The Excel 365 Windows (`en-US`) keyboard and gesture conformance table drives both Classic's default keymap layer and its automated test runner, recording observed Excel build provenance per row.
- **Resolution order:** Non-rebindable safety keys (`Esc`, `Ctrl+Z`, `Ctrl+Y`, `F4` while editing, `Ctrl+Enter`, `Alt+Q`, return to Classic) -> focused editor/menu -> focused component -> active skin layer (departures from the Excel profile are counted automatically: `"Departs from Excel in 2 keys"`, Fable R-94) -> active view layer (`excel-365-win-en-us` in Classic, `jupyter` in Notebook) -> shell layer.
- **Exact Excel key tips (`D11`):** Key-tip letter trees (`Alt` -> `H` -> `V` -> `V`) are data in Classic's profile, independent of how compactly the Deck groups controls visually. Unimplemented Excel ribbon commands reached by key tip return an honest `NotModelled` notice rather than binding a different action.
- **View-switching chords (`D12`):** `Ctrl+Alt+1` (Classic), `Ctrl+Alt+2` (Inspector), `Ctrl+Alt+3` (Notebook), `Ctrl+Alt+4` (Model map). Because Excel binds `Ctrl+Alt+5` to floating-object traversal (`Pro 19 A-111`), `Ctrl+Alt+5` is left unbound under the Excel profile; App and Forms maker are reached via `Alt+Q` command search or the Mast switcher. Whenever a keyboard layout maps `Ctrl+Alt+<digit>` (`AltGr`) to a printable character (`²`, `³`, `{`), character entry wins. `Alt+1…9` remain Classic's Quick Access Toolbar slots.

---

## 9. The extension boundary

1. **Workbooks carry zero executable code:** `urn:dnacalc:views:v1`, `dna.skin/1` and `dna.page/1` contain no scripts, expressions, HTML or remote URLs. Opening any `.xlsx` with custom views, pages and skins triggers zero code execution and zero network requests.
2. **Signed `.dnaext` Wasm-in-Worker packages (`dna-ext-sdk`):**
   - An extension is an installed, Ed25519-signed archive (`extension.json` + `module.wasm` compiled from Rust) approved through an explicit Trust Card naming publisher, version and declared slot capabilities.
   - Each active extension runs in a dedicated **Web Worker** (identical in browsers and WebView2, and runnable in-process in native Rust tests) with **no DOM, no CSS and no network**.
   - **Slot-only data boundary:** The host kit feeds the guest only `Shown<T>` values, `Stamp` and `Coverage` for its explicitly bound `read` slots (`ToComponent::Bind`), plus resolved Strand tokens. The guest returns a restricted **`RenderTree`** (Strand layout primitives, text, SVG paths, bounded Canvas2D draw lists — no per-cell grid loops) and slot intents (`FromComponent::WriteSlot`, `SelectSlot`, `WindowRows`), which `dna-kit` turns into standard `Origin::Extension` batches under cell validation and the seam.
   - **Missing extension handling:** If a workbook names an uninstalled extension, `dna-kit` renders a clear card (`"Needs extension X · not installed"`) with a one-click `"Show bound source as table"` fallback (`DataFrame` / `ValueView`), never silently fetching code.
   - **Proving the boundary in v1:** Third-party marketplace distribution waits until post-v1; Milestone 8 proves the exact `.dnaext` worker boundary by shipping the **Live Tree** (`NC04`, `E01`) as a signed, pre-installed Rust extension (built as an ordinary `dna-kit` component with the same props in earlier milestones so nothing blocks on Milestone 8).
3. **Add-ins remain separate:** Calculation UDFs (`.xll`, COM, `.xlam`, JS) live at level 0 under add-in/macro security; UI extensions live in `dna-kit` and cannot register functions or alter calculation. Neither trust grant implies the other (`E03`).

---

## 10. Agents

- **One grammar, zero private doors:** Agent tool definitions are generated directly from `calc-skin`'s `Query` schemas and `CommandTable` (`ArgsSchema`), addressing workbook content with `SourceRef` / `Target`.
- **Two strict lanes — Looks vs Model:**
  - *Looks (`dna.skin/1`, `dna.page/1`, view docs):* Submitted via `Operation::TryOn`. `dna-skin-runtime` validates schema, all 61 Strand contrast pairs, protected keys and `Bound { anchor, address }` resolution, then mounts under the Try-On banner. Accepting writes one `PresentationChange` (`Stamp.model` provably unchanged).
  - *Model (`Proposal` of `Vec<ProposalUnit>`):* Default agent `WritePolicy` is `Propose` (`Look` and `ApplyThenReview` are per-workbook user settings). Each `ProposalUnit` carries a `Batch` and `needs: Vec<UnitId>` dependencies (`Pro R-45`). Ticking a subset previews writes and budgeted value effects for **that exact subset** (`Pro R-44`). `Operation::Decide` validates all accepted units atomically against `base` + `expects` (refusing everything with inline source diffs if any unit is stale, `Pro R-47`) and commits each accepted unit as its own `Origin::Agent` `HistoryEntry` in one `GroupId` so `Ctrl+Z` undoes one unit at a time (`Pro R-46`).
- **Agent trail (`D14`):** Recorded in `<trail>` inside `urn:dnacalc:views:v1` (timestamp, agent ID, proposal intent, unit targets and accepted/rejected/undone status; capped with a dropped-count summary; toggleable via `PresentationChange::Trail`).
- **Permission boundary:** An agent port cannot change macro security, load add-ins, install extensions, alter calculation precision/iteration silently, save over files or transmit workbook data externally (`Pro R-50`).
- **Two-speed agent verification:**
  1. `dnacalc skin validate <manifest.json> [--workbook file.xlsx]` — sub-second portable CLI check (schema, 61 contrast pairs, keymap collisions/departure count, anchor resolution).
  2. `cargo xtask app smoke calc --skin <manifest.json> --workbook <file.xlsx>` — extends the existing WebDriver harness (`tools/xtask-harness/src/app.rs`) to mount the skin in headless Chrome and Firefox, run keyboard scripts via `__dnaProbe()`, capture Light/Dark/High-Contrast screenshots, and assert `before.model == after.model`.

---

## 11. Web and desktop from one code base

| Aspect | Browser (`/app/`, `app.dnacalc.com`) | Windows Desktop (`WebView2`) | `dnacalc serve` (Linux/macOS/CLI) | Headless tests |
|---|---|---|---|---|
| **UI bundle** | One Leptos wasm bundle (`dna-calc` or `dna-onecalc`) | Identical embedded wasm bundle | Identical wasm bundle in user's browser | Optional wasm or Rust `Runtime` directly |
| **Host execution** | Wasm Web Worker (`dna-calc-worker` / in-page L0) | Native process (`calc-skin-host` / `calc-formula-host`) | Native CLI process | In-process Rust |
| **Wire carrier** | `Worker.postMessage` + transferable `ArrayBuffer`s | WebView2 control JSON + **WebView2 shared binary buffers** (retiring base64-after-`U+0001` in `desktop-bridge.js`) | Loopback WebSocket with capability token (binary frames for tiles) | Direct Rust channel |
| **Add-ins & COM** | JS UDFs, `DnaCalc.Clock`; native `.xll`/COM reported `NotAvailableHere` with desktop link | `.xll`, COM, `.xlam` (`calc-addin-host` + surrogate), COM server (`calc-com-host`) on same `HostSession` | Portable VBA & `DnaCalc.Clock`; native Windows `.xll`/COM reported `NotAvailableHere` | Mock & portable providers |

Views contain **zero** `#[cfg(target_arch = "wasm32")]` or `window.__dnacalcDesktop` branches: platform differences flow strictly through `Capabilities` and `Operation` refusals (`NotAvailableHere`).

---

## 12. Persistence: the view part (`urn:dnacalc:views:v1`)

### 12.1 Schema and `calc-doc` integration

Everything DNA Calc authors outside Excel's own parts lives in **one** custom XML item in namespace `urn:dnacalc:views:v1`, read and written via `calc-doc/src/custom_xml.rs` (`CustomXmlItem`), preserving unknown elements and attributes byte-for-byte for forward compatibility (`Pro C-26`, `C-27`):

```xml
<dnaViews xmlns="urn:dnacalc:views:v1" version="1" writer="DNA Calc 0.2" model="sha256:...">
  <prefs preferred="notebook" supported="classic notebook app">
    <sheet key="7" name="Summary" preferred="notebook" decl="5a2d..."/>
  </prefs>
  <anchors>
    <anchor id="0f1e..." kind="spill" sheet_key="7" ref="Analysis!A10" name="pendingOrders">
      <fingerprint formula_r1c1="9b2c..." label="pendingOrders" rows="4147" cols="7"/>
    </anchor>
  </anchors>
  <decls>
    <decl id="5a2d..." view="notebook" schema="1" sheet_key="7"><![CDATA[{ ... }]]></decl>
  </decls>
  <skins><skin id="calm-review"><![CDATA[{ "schema": "dna.skin/1", ... }]]></skin></skins>
  <readings><!-- confirmed / corrected pattern readings with evidence fingerprints --></readings>
  <notes><!-- anchored review notes and findings (D5) --></notes>
  <trail enabled="true"><!-- agent proposal & unit summary records (D14) --></trail>
  <requires><extension id="dnakode.live-tree" version="^1.0"/></requires>
</dnaViews>
```

- **Unknown major version or unreadable XML:** Preserved untouched byte-for-byte on save; workbook opens in Classic with a non-modal notice (`W05`).
- **Missing/stripped part (e.g. Excel Document Inspector):** Model is 100% intact; opens in Classic and every other view still works from live `Structure` readings.
- **Retiring v1 (`urn:dnacalc:view-manifest`):** Because zero user or corpus workbooks use v1 (verified via `git grep`), `crates/calc-doc/src/{load,save}/manifest.rs` and `calc-doc-model/src/layout.rs` (with `_dna.block.<hex>` hidden names) are deleted outright at the switch, satisfying `D10` and `D13`.
- **Transient viewport state stays local:** Camera pan/zoom, scroll offsets, splitter widths, open companion pane, Inspector recordings, recovery journals (`OPFS` / desktop profile store, replaying committed `CommandIdToken`s once and restoring unrun drafts as drafts, `Pro C-14`) and user skins live in per-user local storage — **never** inside `.xlsx`, avoiding noisy file diffs on scroll. Classic's frozen panes, sheet zoom and active cell remain in Excel's standard `<sheetViews>`.

### 12.2 Anchors and reconciliation after external edits (`D13`)

No hidden defined names are ever written to a workbook. When `model="sha256:..."` matches the workbook on open, all anchors stand as saved. When the digest differs (the file was edited and saved in Excel or another tool), each anchor resolves deterministically and **never guesses**:
1. **User `Name` / `Table` / `Object` target still exists:** Bound to Excel's updated target (why Notebook and Map encourage naming key outputs).
2. **Stored cell/range address still matches `fingerprint`:** `Bound`.
3. **Unique fingerprint match elsewhere in the sheet's used range:** Marked `Relocated { from }` with a one-click `"Confirm new location"` presentation action.
4. **Zero or multiple matches:** Marked `Orphan` (shown visibly broken with `Rebind` / `Remove`; never silently snapped to a nearby cell, `Fable R-77`).

Excel save-route retention (`.xlsx`, `.xlsm` in desktop Excel; explicit loss warning before `.xls` / `.csv` export, `O02`) is verified on Windows under open bead `dc-1nbp.22`.

---

## 13. Automation and CLI

- **Excel object model (`calc-object-model` / `calc_host::automation`):** VBA, COM (`calc-com-host`) and Office.js operate directly on `HostSession` as today. `Selection`, `ActiveCell`, `ActiveWindow` and `Application.StatusBar` share the session's live state; `MsgBox` / `InputBox` / `UserForm` surface through `Event::Macro` over any view; `Operation::RunMacro` starts macros from the Deck, palette or App buttons under `Off` / `Guarded` / `Unguarded` (`G01–G03`).
- **Views library for VBA (`G02`, `Fable R-99`):** Exposed via late-bound `Application.Run` verbs so workbooks compile in Excel without missing type-library references and degrade cleanly under `On Error Resume Next`:
  - `Application.Run "DnaCalc.View", "notebook", "Summary"`
  - `Application.Run "DnaCalc.Select", "Loan!B2:B6"`
  - `Application.Run "DnaCalc.Panel", "inspector.trace", True`
  `Select` updates `HostSession::selection` immediately; `View` and `Panel` emit `Event::Macro(ViewRequest)` applied by `dna-shell` when the run yields or finishes (never interrupting an active user text edit, never creating an undo step, never dismissing Trust Cards or hiding the Strip, and never affecting calculation, `Pro R-65`).
- **Headless & CLI (`apps/dnacalc`):** `recalc`, `get`, `set`, `diff`, `run`, `serve` and `skin validate` drive `calc-host`, `calc-formula-host` and `calc-skin-host` directly with no UI dependency.

---

## 14. Website and app bidirectional architecture

- **Privacy-preserving fragment `Link` grammar:**
  - Web app (`https://app.dnacalc.com/` or `/app/`): `#src=sample:sales&view=notebook&at=Analysis!A10&panel=inspector.trace&skin=calm-review`
  - Standalone Explorer: `#f=%3DPMT(6.5%25%2F12%2C360%2C-250000)&in=B1:0.065&fmt=%24%23%2C%23%230.00`
  - Desktop URI (`dnacalc://open#...`): parses the exact same `Link` struct, prompts before opening external files, and never enables macros or installs extensions from a link.
  - Because parameters sit in `#fragment`, user formulas and trial literals are never transmitted in HTTP request URLs; `"Copy link"` previews exactly what is encoded.
- **Embedded live formulas on `dnacalc.com` and `handbook.dnacalc.com`:** Keeps `marketing.rs`'s `<div id="onecalc">` mount (`tools/xtask-harness/src/site/marketing.rs:249`), extended with `data-link="#f=..."` so every function page and story step embeds an interactive `skin-0` Formula Explorer box.
- **One set of cast workbooks:** `Budget`, `Sales`, `Loan`, `Stock` and `Inherited` are generated `.xlsx` fixtures in the repository shared by `"New from example"` in the app, story walkthroughs on the website, the Pro 21 scene gallery and `cargo xtask app smoke`.
- **App -> Site `HelpId`s & build-record parity:** Every `CommandSpec`, function (`https://handbook.dnacalc.com/functions/<NAME>.html`), view, reading and `Refusal` carries a `HelpId` checked at site-build time (with offline text bundled in the app). `About` and the Explorer function browser embed the exact commit's corpus and witness-bank summary JSON that powers `/status/`.

---

## 15. Code shape: kept, rewritten, thrown away, and the switch

### 15.1 Target workspace layout

| Package | Location | Role & origin |
|---|---|---|
| `calc-skin` | `crates/` | **New.** Portable contract, queries, batches, frames, wire codec (absorbs `dna-calc-protocol`), `Link`, `Page` & `SkinManifest` types. |
| `calc-formula-kernel` | `crates/` | **New.** Pure parse, completion, `Present` lens, `Interpret`, `interpret_entry`, localized value formatting. |
| `calc-formula-host` | `crates/` | **New** (moves ~5,940 lines from `dna-onecalc` + RTD admission from `calc-host`). Level-0 session & wire handler. |
| `calc-host` | `crates/` | **Kept & refined** (51,264 lines + 1,285 lines `printing` moved in from `dna-calc-model`; owns `FormulaHost`, `ViewPart`, anchors, `prepare`, macro run scopes; drops `dna-calc-model` dep and retires `SkinIntent` at M3). |
| `calc-skin-host` | `crates/` | **New** (keeps ~7,510 lines of view-neutral services/publication from `dna-calc-worker`, drops per-skin projections and 1,200-line intent match). |
| `calc-chart-render` | `crates/` | **Kept** (3,723 lines + Carlito fonts). Host-side SVG + PNG/JPG/GIF export/print renderer. |
| `calc-doc`, `calc-doc-model` | `crates/` | **Kept & refined.** `ViewPart` (`urn:dnacalc:views:v1`) over `custom_xml.rs`; deletes v1 `manifest.rs` & `layout.rs` (1,989 lines). |
| `dna-strand` | `apps/` | **Kept & extended** (1,271 lines). Adds runtime `resolve` & CSSOM `--dna-*` property emission. |
| `dna-skin-runtime` | `apps/` | **New** (keeps `tile_mirror`, `EditSession` core, input/transport rules; adds `Frame` store, `Live<T>`, explicit `InputMode`s, `InputQueue`, `Keymap`, `SkinValidator`, `__dnaProbe()`). Portable, no DOM/Leptos. |
| `dna-grid` | `apps/` | **New** (moves ~9,258 lines of DOM-free grid core from `dna-calc-model` + ~4,150 lines of pure painters/panes/axis from `dna-calc/src/grid/` + fresh gesture/cell-edit UI). Registers `"sheet"` component. |
| `dna-formula-ui` | `apps/` | **Kept** (4,001 lines). Re-pointed from `dna-calc-model` to `calc-skin` + `calc-formula-kernel`. |
| `dna-kit` | `apps/` | **New.** Strand Leptos components, `PageRenderer`, `View`/`FormulaView` traits, `Registry`, Wasm-worker extension host. |
| `dna-shell` | `apps/` | **New.** Mast, Deck (key tips), Bridge, Stage + Companion, Panels, Strip, Seam, Trust Cards, Try-On, Classic dialogs. |
| `dna-view-{classic,inspector,notebook,map,app,explorer}` | `apps/` | **New.** Six isolated view crates (`-app` registers both `"app"` and `"forms"`; `-inspector` registers all dockable instruments). |
| `dna-ext-sdk` | `apps/` | **New (M8).** Guest Rust-to-Wasm SDK (`RenderTree`, slot bindings) + built-in Live Tree extension. |
| `dna-calc`, `dna-calc-worker`, `dna-onecalc` | `apps/` | **Rewritten thin** (retained package names for site packer and `sweep` ceiling checks). |
| `dna-calc-desktop`, `dna-onecalc-desktop`, `dnacalc` | `apps/` | **Kept & trimmed.** Shared desktop WebView2 host with binary buffer transport; `dnacalc` adds `skin validate`. `dna-calc-model` and `dna-calc-protocol` are deleted at M3. |

### 15.2 Verified line-count disposition (measured on `e49e830fc`)

- **Thrown away (~33,050 lines):** `controller.rs` + `controller/*` (5,823), 5 old skins (`notebook`, `atlas`, `tree`, `canvas`, `ledger`: 3,729), ad-hoc shell & wire-coupled dialogs in `apps/dna-calc/src` (12,657), duplicate command/wire adapters (`workbook_command.rs`, `model_adapter.rs`, `wire.rs`: 914), `dna-calc-model` wire projections/state (5,883), worker per-skin projections + `server.rs` intent/skin arms (~4,500), OneCalc's separate `surface.rs`/`port.rs`/`bridge.rs`/`panels.rs` (3,906), and v1 manifest/hidden-name layout (1,989).
- **Kept & moved (~26,700 lines + 51,264 in `calc-host`):** `dna-calc-model` DOM-free grid/tile logic (9,258 -> `dna-grid` / `dna-skin-runtime`), `printing.rs` (1,285 -> `calc-host`), `dna-calc/src/grid/` painters/panes/axis/autofit (4,150 -> `dna-grid`), `dna-calc-worker` view-neutral services & tile `publication.rs` (7,510 -> `calc-skin-host`), `dna-onecalc` evaluator/CF/RTD/trace/session core (5,940 -> `calc-formula-host`), `dna-formula-ui` (4,001), `dna-strand` (1,271), `calc-chart-render` (3,723), and **832 Classic/shell smoke cases** (out of 937) + **121 Explorer smoke cases**.

### 15.3 Clean switch without micro-step migration (`D10`)

1. **Move kept logic once (`git mv`):** Extract `calc-formula-host`, move `printing.rs` into `calc-host`, and move kept grid/worker modules into `dna-grid` and `calc-skin-host` with re-exports so the old page compiles unchanged with zero code duplication.
2. **Freeze the old page:** Defect fixes only; zero feature additions on `apps/dna-calc` old modules.
3. **Publish side-by-side from Milestone 2:** Serve the new page alongside the old one for live feedback while running the smoke suite against both.
4. **One deletion commit at Milestone 3:** Once Classic's >= 832 smoke cases pass in headless Chromium and Firefox on the new page, delete all ~33,050 lines of old page, old worker handlers, `dna-calc-model`, `dna-calc-protocol` and `SkinIntent` in a single commit.

---

## 16. Build order (milestones, no dates)

| Milestone | Scope | Verification that closes it |
|---|---|---|
| **M0: Contract, host prep & moves** | `calc-skin`; `printing` moved to `calc-host` (dropping `dna-calc-model` dep); `ViewPart` (`urn:dnacalc:views:v1`) & structural anchor transforms in `calc-doc-model`/`calc-doc`; two-revision `Stamp`; in-process `Carrier`; cast `.xlsx` generator; `architecture.json` rules 1–7. | Portable contract & anchor-transform tests pass natively and in wasm Chrome/Firefox; `cargo xtask check --all` green. |
| **M1: Level 0 & Formula Explorer (`D2`)** | `calc-formula-kernel` & `calc-formula-host`; `dna-strand::resolve`; foundational `dna-skin-runtime`, `dna-kit` (`ValueView`, `DataFrame`) & `dna-shell` (level-0 frame); `dna-view-explorer` (`skin-0`); fragment `Link`s; replaces `dna-onecalc` web & desktop builds. | Explorer session tests + all **121** Explorer browser smoke cases pass; `sweep` ceiling proves zero `calc-engine`/`calc-host` in Explorer; published as early teaser (`D2`). |
| **M2: Level-1 serving, runtime & thin Classic** | `calc-skin-host` (`Cells` tiles with `Interest::Anchors`, core queries, footprints, `Frame`s, `PreviewDepth::WritesOnly`, `Batch` commit); WebView2 binary buffer carrier; `dna-shell` level-1 frame (Bridge, Panels, Seam, Strip, `Alt+Q`, `__dnaProbe()`); thin Classic on `dna-grid` (draw, select, edit, undo). | `tests/workbook.rs` passes on `calc-skin`; type-ahead & optimistic-selection interleave tests pass across all 4 carriers; new page published beside frozen old page. |
| **M3: Classic in full & the switch commit** | Deck with Excel key-tip table (`D11`), full `InputMode`s & mouse gestures in `dna-grid`, 13 Classic dialogs on `dna-kit`, Explorer tool docked inside DNA Calc (`TrialFor`), crash recovery journal; **deletes old page, old worker, `dna-calc-model`, `dna-calc-protocol`, v1 manifest and `SkinIntent`**. | **>= 832** Classic/shell smoke cases pass in Chromium and Firefox (`~105` retired-skin cases removed and counted); corpus opens in Classic with `before.model == after.model`. |
| **M4: Instruments & Inspector** *(parallel after M2)* | Host queries: grouped `Deps`, root `Issues`, `Sources`, `Contributions`, `Census`, `Inventory`, initial `Structure` (runs & tables); all Inspector panels + bench view + anchored `Notes` (`D5`). | `Sales` trace (`S06`/`S07`) and `Inherited` audit storyboards pass end-to-end through `__dnaProbe()`. |
| **M5: Notebook (`D6`)** *(parallel after M2)* | `Placement` (scalar rows + array lanes), `Rows`; `dna-view-notebook` (`jupyter` keymap, `Esc` held drafts, `RichProse` live chips, relocation fixes, unauthored sheet draft); Live Tree as `dna-kit` component (`NC04`). | `NC01–NC06` over `Sales` pass; Jupyter key matrix verified via probe; Windows Excel round-trip check (`dc-1nbp.22`). |
| **M6: App, Forms maker, Skins & Agents** *(parallel after M2)* | `PageRenderer`, `dna-view-app` (`"app"` + `"forms"`), `Structure` input/output role candidates, `TryOn`/Keep, `Proposal` subset preview & `Decide`, `<trail>` (`D14`), `dnacalc skin validate`, `cargo xtask app smoke --skin`. | `Loan` App (`A01–A03`), `NC07` reader report, `U01–U05` three-line agent App & Classic restyle, and multi-unit `Decide`/undo/conflict tests pass. |
| **M7: Model map** *(parallel after M4)* | Hierarchical `Structure` levels, `dna-view-map` (200-node LOD, cycle enclosures vs `z⁻¹` delays, `DnaCalc.Clock` controls, port rewiring via `ReplaceReference`). | `Stock` ticked model (`M01–M02`) and `Inherited` scale decline/submap (`M03–M05`) pass. |
| **M8: Extensions, VBA views & macro runs** | `dna-ext-sdk` Wasm-in-Worker host (porting Live Tree as signed `.dnaext` #1); `Application.Run "DnaCalc.*"` views verbs; `RunMacro` UI + Guarded cards (`G01–G03`); macro run grouping, `OnUndo`, checkpoints & `DocumentOnly` undo experiments (§17.1). | Live Tree runs in worker sandbox; missing-extension card tested; Guarded macro switches view in DNA Calc and runs cleanly under `On Error Resume Next` in Excel. |

---

## 17. Every hard part kept honest

### 17.1 Undo across a macro run (`D4`)
- **v1 commitment (Excel parity + visibility, promised):** Today `calc-host` records every VBA write as a separate undo step, diverging from Excel. In v1, `begin_run` / `end_run` merges all commits of a macro execution into one `HistoryEntry` (`Origin::Macro`, `Reversibility::Irreversible`, listing procedure and touched targets), blocking `Ctrl+Z` past it just as Excel does, implements `Application.OnUndo` / `Application.Undo`, and captures a zero-copy `ModelRoot::pin()` checkpoint before write-capable runs so the user can explicitly invoke `"Restore workbook before macro"` with a plain list of un-restored external state.
- **Experimental design (`Reversibility::DocumentOnly`, tried in M8, not promised in v1):** Restoring `ModelRoot::pin()` on `Ctrl+Z` is cheap in memory (`Arc`-shared `CellPage`s), **but** seven hard problems must be measured before ever turning it on for `Ctrl+Z`:
  1. *VBA runtime memory:* module globals, `Static` locals, `Collection`/`Dictionary` state and open file handles survive document restore and diverge from restored cells;
  2. *External side effects:* file writes, WinHTTP/COM calls and clipboard writes cannot roll back (`Guarded` mode logs audited capabilities; `Unguarded` must state effects were unobserved);
  3. *Event re-entrancy:* whether `Undo` fires `Worksheet_Change` / `Workbook_SheetChange` in Excel must be captured on Windows first, and handler re-writes during undo need loop guards;
  4. *Session vs document mutations:* macro changes to `Application.Calculation`, `EnableEvents` or active sheet across multiple open workbooks;
  5. *Interaction with `Application.Undo` / `OnUndo`* inside macros;
  6. *Interrupted/faulted runs* leaving partial mutations;
  7. *Engine replan cost:* `command::restore` rebinds the dependency graph, which on a 300,000-formula workbook (`Inherited`) can take seconds. Guarded action-plan macros (`G01–G03`) are the tractable first testbed.

### 17.2 Big workbooks (Enron 20.7M formulas; `Inherited` 40 sheets / 300k formulas)
- **Existing bottlenecks to eliminate in M0/M2:** conditional-format commits currently reset every tile subscription (harvest 17 finding 2); geometry for all sheets is cloned per subscription collect (`server.rs:4214`); tile atoms cross WebView2 as base64 text (`desktop-bridge.js:53`); browser workers hit wasm32's 4 GiB address ceiling if `ModelRoot` pages are duplicated unnecessarily.
- **Architectural mitigations & remaining hard work:** `Census` + `View::fit` stop non-grid views before mounting dense sheets; `DataFrame`, `Notebook` and `Map` virtualize DOM/canvas nodes strictly (`<= 200` nodes / `<= 400` wires on Map); opening projects `AsSaved` file-cached values before dependency graph construction finishes (requires staged open in `calc-host`/`calc-doc`); and grouped `Deps` / `Structure` at 300k formulas require an incremental background run/block index rather than whole-sheet rescans. Latency targets (<1 s first viewport, <50 ms keystroke echo, <150 ms view switch) are measured benchmarks, never flaky per-commit checks.

### 17.3 Excel keyboard, key-tip and mouse conformance (`D11`, `D12`)
Every missing gesture in Classic feels broken to an Excel user, and 21 of the 31 `UI_REVIEW.md` parity rows are still partial today (10 Done, 21 Partial). Capturing the exact Excel 365 Windows key-tip path tree, mode transitions (`Ready`/`Enter`/`Edit`/`Point` + `End`/`Extend`/`Add`/`ScrollLock`), mid-drag modifier toggles (`Ctrl`/`Shift`/`Alt` border drags, fill-handle double-click extent), IME composition behaviour and European `AltGr` (`Ctrl+Alt`) collisions requires an automated Windows Excel UI observation script (`RUN-ON-WINDOWS.txt`) rather than guessing from model prose. Browser-owned keys (`Ctrl+W`, `Ctrl+N`, `Ctrl+T`, `F11`) cannot be intercepted without permissions and are published honestly per browser with alternates.

### 17.4 Value preview (`PreviewDepth::WithEffects`)
Structural write preview (`WritesOnly`) is fast and exact via non-committing `prepare`. Computing downstream recalculated value diffs (`WithEffects`, needed for agent subset review `Pro R-44`) cannot safely "commit, calculate and undo" on the live session (which would fire watches, RTD topics and events). Instead, `WithEffects` runs only within an explicit `Budget` over a pinned `ModelRoot` snapshot when the affected dependency closure contains **no volatile functions, RTD topics or VBA UDFs**; otherwise it declines cleanly and offers `"Commit and undo"` (`Ctrl+Z`).

### 17.5 Notebook backing layout & dynamic-array spills (`NC06`)
Allocating scalar rows and array column lanes from commit-time preview shapes prevents vertical spill collisions, **except** when an existing scalar later becomes a spilling array or a dynamic `FILTER` widens past its lane width on new data. Because **no calculation may ever trigger an automatic sheet edit**, Excel's native `#SPILL!` appears honestly, and the Notebook surfaces one-click previewed `ModelChange` repairs (cut-and-paste lane relocation, obstruction move, or column insert — warning explicitly on shared sheets like `W04`'s `Summary` when inserting columns shifts user cells). `"Tidy backing sheet"` offers an explicit cut-and-paste reorder when reading order drifts far from sheet row order.

### 17.6 Anchor reconciliation after external Excel edits
Excel preserves `urn:dnacalc:views:v1` across `.xlsx`/`.xlsm` saves (to be proven across save routes on Windows in `dc-1nbp.22`), but does not rewrite custom XML cell strings when rows are inserted in Excel. Unnamed anchors whose formula *and* position both changed in Excel, or identical repeated blocks matching one fingerprint twice, become `Orphan`s requiring user rebinding rather than risky heuristic guesses.

### 17.7 Optimistic selection & concurrent writers across the wire
Optimistic page cursor moves, queued type-ahead (`Ctrl+Arrow`), macro `Range.Select` calls and agent proposals can interleave in flight. `client_seq` vs `SelectionCause` (§4.2) and `selection_seq` on `Batch` prevent stale echoes from snapping the cursor backward and prevent keystrokes from landing on the wrong cell after a macro `Select`, backed by deterministic slow-carrier interleave tests in M2. Concurrent `PatchDoc` edits to the same view document fail fast (`expect_view_rev`) rather than attempting lossy JSON-patch operational transforms.

### 17.8 Desktop STA thread stalls
On Windows, `calc-skin-host` shares the STA thread with COM (`SessionAccess`, `apps/dna-calc-desktop/src/automation.rs:13`), running calculation in 6 ms slices on an 8 ms timer. Normal formulas keep typing under 50 ms, but a synchronous, long-running native `.xll` or COM call still blocks the thread until `calc-addin-host`'s out-of-process surrogate or marshalled worker threading handles it (deferred until measured stalls exceed budget).

### 17.9 Extension Worker hop & painted-surface accessibility
- **Worker `RenderTree` hop:** Ideal for bounded components like the Live Tree (`NC04`), charts and domain cards; unsuitable for 10,000-cell viewport grids, which is why `"sheet"` and `DataFrame` are native `dna-grid`/`dna-kit` primitives.
- **Accessibility of Canvas2D surfaces:** `dna-grid` maintains its 4,096-cell semantic ARIA projection (`render_plan.rs:1239`) synchronized with real Excel coordinates under virtualization (`Pro R-55`); `dna-view-map` must build an equivalent keyboard-navigable node/port accessibility tree alongside its canvas painter in M7 (`Pro R-59`).

### 17.10 `consistent()` under continuous high-rate RTD feeds
When an RTD feed ticks faster than workbook recalculation completes, waiting indefinitely for a quiescent `CalcGeneration` would starve Print or App summaries. `consistent()` therefore yields immediately when quiescent and otherwise pins the most recently completed `CalcGeneration`, labelling the output with its exact timestamp and generation.

---

## 18. Not taken, and why

| Rejected idea & source | Reason not taken |
|---|---|
| **FerroUI native toolkit now** (`dc-etcxb.1–.3`) | Owner ruling **D7**: keep Leptos + WebView2 baseline; `calc-skin` and `dna-skin-runtime` stay toolkit-free so this remains reversible. |
| **Micro-step migration of today's 6 skins** (`FRONT_END_IMPLEMENTATION_BRIEF` ground rule 3) | Owner ruling **D10**: freeze old page, build fresh against `calc-skin`, delete ~33k lines in one switch commit. |
| **Hidden `_dna.block.<hex>` defined names as anchors** (v1 manifest; Carbon dual-anchor option; `proposal-fable` v1 migration dialog) | Decider ruling **D13**: never add hidden names to user workbooks. Repo grep confirms 0 real workbooks use v1, so v1 is deleted outright rather than migrated. |
| **Separate Author vs Read modes in Notebook** (Pro 21 Author/Read toggle) | Owner ruling **D6**: Read-only is only how a file was opened; reader-style reports without formulas are `dna.page/1` pages in the Forms maker (`NC07`). |
| **Forms maker as an inline edit mode inside App** | Blurs model use with authoring (same failure mode as Pro 21's Notebook modes); Forms maker is its own view (`"forms"`) over the shared `PageRenderer`. |
| **Reverting Notebook text on `Esc`** (`proposal-fable §7.3`) | Violates **D6** (exact Jupyter interaction): in Jupyter `Esc` transitions Edit -> Command mode while keeping unrun text (`"Edited · not run"`). `Esc` still never commits to the workbook. |
| **Run-once stateful Notebook kernel with `[n]` execution counts** (literal Jupyter execution) | Violates one-evaluator rule and scope (`SKIN_DISCUSSION` #1): formulas live in Excel cells with live dependency-graph recalculation. |
| **Per-view undo stacks, or split model vs presentation undo** (Fable Pro 19 §3) | Violates agreed one-history rule: one host `History` with clear Strip notices (`"Undid ... in Model map · [Show]"`). |
| **Moving `calc-chart-render` into the UI page wasm bundle** (`proposal-opus §7.1`, brief item 3) | Verified in repo: `calc-chart-render` embeds **2.7 MB of Carlito TTF fonts** + `resvg`/`tiny-skia` for `Chart.Export` (`dc-oxh8w.22`) and PDF/print. Keeping it host-side avoids 2.7 MB wasm bloat while `Objects` returns both SVG and `ChartData`. |
| **Keeping `printing::paginate` in `apps/`** (`proposal-fable §15.1`) | Leaves `crates/calc-host` depending upward on an `apps/` crate (`calc-host/Cargo.toml:28`). Moving pagination into `calc-host` fixes the layering inversion cleanly. |
| **Storing viewport scroll/camera inside `.xlsx` `<session>` XML** (`proposal-opus §10.1`) | Dirties binary `.xlsx` diffs on pan/scroll and leaks a viewer's camera to colleagues; belongs in per-user local storage (except Excel's own `<sheetViews>`). |
| **Silently applying a file's embedded skin keymap/theme on open** (`proposal-opus §8`) | A received file must never hijack a user's keyboard shortcuts or contrast preferences unasked; file-embedded themes/keymaps open under the Try-On banner (`proposal-fable §8.1`). |
| **HTML/JS `iframe` extensions or DOM Web Components** (`proposal-fable §9.2`, Carbon Pro 19) | Wasm-in-Worker returning Strand `RenderTree` (`dna-ext-sdk`) eliminates DOM/CSS spoofing, obeys the Rust-first rule, enforces Strand contrast/accessibility automatically, and runs in headless Rust tests. |
| **Giving extensions raw `Query` access** (`proposal-opus §11`) | Over-privileged; extensions receive data strictly through their declared `Bound` read/write slots (`proposal-fable §9.2`). |
| **Level 0 as a Cargo feature on `calc-host`** (brief item 9 option a) | Cargo features unify across workspace builds and erode; separate `calc-formula-kernel` + `calc-formula-host` crates are mechanically enforced by `architecture.json`. |
| **Per-skin projections in the worker / 4 duplicate command enums** (current `server.rs`, `SkinIntent`, wire `Intent`, `WorkbookCommand`) | Every view change required worker and wire edits; replaced by one `calc-skin` query and change vocabulary. |
| **`Atlas`, `Tree`, `Canvas` and `Ledger` as top-level views** (current 6 skins) | Folded into Inspector (`Overview`/`Issues`), Outline/Live Tree panels, Model map, and `DataFrame` components (`Pro 19 §10`). |
| **`Ctrl+1…5` (Carbon) or `Ctrl+Alt+5` (`proposal-fable`) for view switching** | `Ctrl+1` is Format Cells; Excel binds `Ctrl+Alt+5` to floating shapes (`A-111`). Only `Ctrl+Alt+1…4` are bound under **D12**, with `Alt+Q` / switcher for App and Forms maker. |
| **Auto-switching views when clicking a sheet tab** (GPT Pro 19 `R-14`) | Disorienting; per-sheet view is a tab badge applied only on initial workbook open (README agreement). |
| **Custom simulation tick/director on the contract, or animated firing pulses without host evidence** (Carbon/Neon Pro 19) | Violates **D3** and Pro 19 push-backs 1 & 2 (`Evidence<T>` separates `Observed` `CalcEvents` from `Derived` `Deps`). |
| **Confidence percentages on pattern recognition** (Neon §5.1) | Arbitrary numbers; replaced by four explicit `ReadingState`s backed by cited evidence (`W02`). |
| **Query-string `?` URLs carrying formulas to servers** (`proposal-opus §14`) | Leaks user formulas/inputs in HTTP request logs; `#fragment` `Link`s (`proposal-fable §14.1`) stay client-side. |
| **Early-bound COM type library for VBA view verbs** (`FRONT_END_SCOPE §3`) | Breaks VBA compilation in Excel when the typelib is absent; late-bound `Application.Run "DnaCalc.*"` works with `On Error Resume Next` (`Fable R-99`). |
| **Promising `Ctrl+Z` across macros in v1** (Carbon `R-35`, Fable Pro 19 `R-98`) | Owner ruling **D4**: group + checkpoint + `OnUndo` in v1; `DocumentOnly` restore is an M8 experiment. |

---

## 19. Open product questions for the owner

All technical discrepancies between Proposals A and B are resolved above. Only three product/policy choices remain for owner confirmation (none blocks Milestones 0–5):

1. **Optional Excel-visible mirror for Notebook prose:** Prose lives in `urn:dnacalc:views:v1` so backing sheets stay clean tables of labelled formulas and spills. Should Notebook also offer an opt-in `"Mirror prose as text cells on backing sheet"` model batch (parallel to review notes in **D5**) for colleagues opening the file in Excel? *(Default assumed: off / view-part only, exportable on request.)*
2. **External local agent socket policy:** In-app agent proposals run over an internal `SkinHost` connection first. When the local loopback agent endpoint (`dnacalc serve` / desktop MCP bridge) is enabled for external local agents, should its default capability token grant `Look` or `Propose`? *(Default assumed: disabled until toggled on by user; `Propose` when enabled, never `ApplyThenReview` without per-workbook confirmation.)*
3. **Extension signing roots post-v1:** Milestone 8 ships with DNA Kode's built-in signing key for first-party extensions (`dnakode.live-tree`) plus explicit local developer trust during authoring. How third-party publisher keys are distributed or countersigned waits until third-party extensions are scheduled.

---

## Appendix A. Pro 19–21 C-needs traced onto `calc-skin`

| Need (Pro 19 `C-1…C-34` + Pro 21 additions) | Met by in this architecture | Milestone |
|---|---|---|
| **C-1** Stable source identity & existence | `SourceRef`, `AnchorId`, `Bound { anchor, address }`, `ViewPart` reconciliation (§4.1, §12.2) | M0 |
| **C-2** Exact formulas, values, text, formats, notes, settings | `Cells` tiles, `Geometry`, `Summary`, `Rules`, `calc-formula-kernel` (§4.6, §5.1) | M0–M2 |
| **C-3** Provenance (`AsSaved`, `Calculated`, `Stale`, `External`, `Refused`) & revisions | `Stamp { model, view, calc }`, `Shown<T>`, `Provenance`, Pivot staleness (§4.3) | M0 |
| **C-4** Scoped change delivery (values, formats, spill shapes) | Atomic `Frame`, tile deltas, footprint-tracked watches, `Changes` (§4.6) | M2 |
| **C-5** Incremental/staged open (`Empty` vs `Loading` vs `Failed`) | `Coverage::Partial`, `AsSaved` projection prior to engine build (§4.3, §17.2) | M2–M3 |
| **C-6** Workbook inventories, hidden/protected state, quick counts | `Census` (zero-calc), `Inventory` with `Coverage` (§4.6) | M0, M4 |
| **C-7, C-8** Grouped precedents/dependents, dynamic markers, exact membership | `Deps { group, budget }` (`Evidence::Derived`), `Structure` (§4.6) | M4, M7 |
| **C-9** Formula AST spans, reference bindings, read-only lens, exact rewrites | `calc-formula-kernel::{Analyse, Present, Interpret}`, `Formula`, `ReplaceReference`, `RenameName` (§4.4, §5.1) | M1, M4 |
| **C-10** Editability (spills, arrays, tables, pivots, protection) | `Rules`, `Refusal::{Protected, ReadOnly, Excel}` (§4.6, §4.8) | M2 |
| **C-11, C-12** Exact consequence preview, atomic batches, stale conflict refusal | `Batch { base, expects }`, `Preview` (`WritesOnly` / `WithEffects`), `Refusal::Conflict` (§4.4) | M0, M2, M6 |
| **C-13, C-33** One history across model & presentation, attribution, `Revert`, irreversible effects | `HistoryEntry`, port-stamped `Origin`, `Reversibility`, `Operation::Revert`, two-revision `Stamp` (§4.5) | M0, M2, M6 |
| **C-14** Crash recovery separating committed work from drafts | `CommandJournal` token dedup + local OPFS/profile recovery journal (§4.4, §12.1) | M3 |
| **C-15** Workbook-free trials & caller-snapshot exploration | `calc-formula-host` `Evaluate(Trial { caller })`, `TrialFor` (§4.6, §5.1) | M1, M3 |
| **C-16, C-17** Provider preview-safety flags, metadata, trust & host routing | `Providers`, `FunctionInfo`, `Capabilities`, `Refusal::NotAvailableHere` (§4.8, §5.1) | M1 |
| **C-18** Raw values kept distinct from number & conditional formatting | `Shown<CellValue>` + `calc-formula-kernel::FormatValue` + `FormulaHost::conditional` (§5.1) | M1 |
| **C-19, C-20** Calc mode, iteration/cycles, observed execution order only when observed | `CalcState`, `CalcEvents` (`Evidence::Observed` vs `Unavailable`) (§4.3, §4.6) | M2, M4 |
| **C-21, C-22** External link & live RTD identity, timestamps, coalescing | `Sources`, `Provenance::External`, `Event::Source`, non-intrusive Inspector recorder (§4.6, §7.2) | M4 |
| **C-23** Windowed row queries & criteria/lookup contribution breakdowns | `Rows { filter, sort, window }`, `Contributions` (§4.6) | M4, M5 |
| **C-24** Charts, pivots, controls, scenarios, validation, print | `Objects` (`calc-chart-render` SVG + `ChartData`), `ModelChange` families, `PrintPlan` (§4.4, §5.3) | M2, M3 |
| **C-25** Macro presence, security levels, run scopes, VBA view verbs | `Summary`, `RunMacro`, `begin_run`/`end_run`, `Application.Run "DnaCalc.*"` (§13, §17.1) | M3, M8 |
| **C-26, C-27** Versioned view part preserving unknown fields & future versions | `urn:dnacalc:views:v1` via `calc-doc::custom_xml`, `model` SHA-256 digest (§12.1) | M0, M5 |
| **C-28** Clipboard with origin, Excel adjustments & format capability disclosure | `CopyToClipboard`, `PreparePaste`, `ModelChange::Paste`, `Capabilities` (§4.4, §11) | M3 |
| **C-29** Consistent single-generation snapshot for print, export & App | `runtime.consistent()`, `PrintPlan` (§4.6, §17.10) | M3, M6 |
| **C-30** Accessible roles, real virtualized coordinates, probe | `dna-kit` ARIA contracts, `dna-grid` 4,096-cell semantic layer, Map tree, `__dnaProbe()` (§6.1, §17.9) | M1–M7 |
| **C-31, C-32** Cancellable/sliced work & pre-mount size estimates | 6 ms calc slices, `CancelCalculation`, `Census` -> `View::fit`, `Budget` -> `OverBudget` (§4.6, §6.2) | M0, M2 |
| **C-34** Unsupported capability kept distinct from invalid user input | `Refusal::NotModelled` & `NotAvailableHere` vs `Invalid`/`Validation` (§4.8) | M0 |
| **Pro 21 (`NC01`, `NC06`, `U01`, `S14–S16`, `O03`)** | Backing scalar rows + array lanes (`Placement`), dual `Bound { anchor, address }`, subset `Proposal` preview/apply (`Decide`), non-consuming snapshot reads | M5, M6 |

---

## Appendix B. Legacy wire inventory (`projection.rs` & `one/port.rs`) -> `calc-skin`

All **72 `Intent`s, 77 `Request`s, 53 `Reply`s, 8 `Event`s** in `dna_calc_model::projection` and **32 `Request`s + 17 `Reply`s** in `dna_onecalc::port` are retired at Milestone 3 (Milestone 1 for Explorer):

- **58 editing/formatting/object `Intent`s** -> `ModelChange` variants inside `Batch`.
- **`NotebookLink`, `NotebookOrder` (2 `Intent`s)** -> `PresentationChange::PatchDoc`.
- **10 calculation/undo/selection `Intent`s** (`Undo`, `Redo`, `Recalculate`, `Rebuild`, `Calculate*`, `Select`, `Navigate`) -> `Operation::{Undo, Redo, Calculate}`, `port.select`, `port.navigate`.
- **8 per-skin projection `Request`s & `Reply`s** (`Atlas`, `AtlasRefusals`, `Outline`, `Notebook`, `BlockDetail`, `Dependents`, `Canvas`, `Ledger`) -> **Deleted**; views read neutral `ViewPart`, `Structure`, `Deps`, `Census`, `Issues`, `Rows`.
- **5 tile subscription `Request`s** (`Subscribe`, `SetInterest`, `Unsubscribe`, `TileAck`, `ResetTiles`) -> `watch(Cells { interest })` + drop/ack flow control.
- **36 workbook inspection `Request`s** (`Summary`, `Geometry`, `UsedArea`, `Charts`, `Shapes`, `Sparklines`, `Controls`, `PivotTables`, `Names`, `EvaluatePart`, `TraceFormula`, `Find`, `GoToSpecial`, `SelectionStats`, `CanEdit`, `Validation*`, `Review*`, `PageSetup`, `PrintPreview`, etc.) -> Neutral `WorkbookQuery` & `FormulaQuery` types with typed answers.
- **28 file/clipboard/macro/add-in/automation `Request`s** -> `Operation` (`Open`, `New`, `Save`, `CopyToClipboard`, `PreparePaste`, `EnableMacros`, `Automate`) and `FormulaOperation` (`LoadProvider`, `UnloadProvider`, `SetMacroSecurity`).
- **8 `Event`s & 4 `FromServer` envelopes** -> Unified `Envelope` + atomic `Frame`s + binary tile frames.
- **49 Formula Explorer `Request`/`Reply` variants** -> Level-0 `FormulaQuery`, `FormulaOperation` and `FormulaEvent`.

---

## Appendix C. Architectural guarantees and proving tests

| Guarantee | Mechanism | Proving test |
|---|---|---|
| **A view or skin can never alter how Excel calculates** | Closure rules 1–7; `ModelChange` maps strictly to Excel operations; no engine handle crosses `calc-skin` | `architecture.json` check; every `ModelChange` saved, opened in Windows Excel without repair and bit-matched (`RUN-ON-WINDOWS.txt`). |
| **Presentation edits touch zero model state** | `PresentationChange` writes `ViewPart` only; `Stamp.model` separated from `Stamp.view` | Property test: 1,000 random `PresentationChange`s across views leave `Stamp.model` and saved worksheet/name XML byte-identical. |
| **Switching views mutates nothing** | `Selection`, `EditSession` and `History` live in session/runtime, never in a `View` | Cycle through all 5 views mid-draft: `Stamp`, `Selection` and active/held drafts are equal before and after. |
| **The preview seam never lies** | `Preview.writes` comes from `HostSession::prepare`; `Preview.kind` is computed from `writes` | Integration assertion: committed journal diff equals `Preview.writes` across all `ModelChange` families. |
| **Stale applies and concurrent conflicts never overwrite silently** | `Batch { base, expects }` and `PatchDoc { expect_view_rev }` checked atomically | Explorer `Apply` and multi-unit agent `Decide` after an intervening cell edit return `Refusal::Conflict` with exact source diffs. |
| **No bare or mixed-revision values shown as current** | `Shown<T>`, `Evidence<T>`, atomic `Frame`s and `consistent()` | Unit tests render every `dna-kit` component across all 7 `Provenance` variants; `Frame` test proves multi-watch updates land atomically. |
| **Formula Explorer ships zero engine code** | `calc-formula-host` below `calc-host`; closure rule 7 | `cargo xtask sweep` dependency ceiling check on `dna-onecalc` (wasm) and `dna-onecalc-desktop` (native). |
| **Opening a workbook executes zero code from the file** | `urn:dnacalc:views:v1`, `dna.skin/1` and `dna.page/1` are pure data; extensions run in signed Wasm workers | Open a workbook referencing an uninstalled extension and embedded skin: missing-extension card and Try-On banner appear; zero code runs, zero network calls fire, keymap stays unchanged. |
| **No skin can break contrast or hide safety chrome** | `dna-strand::resolve` checks 61 pairs; Strip, Trust Cards and protected keys are non-overridable | Validator unit suite rejects skins violating any of the 61 contrast pairs, recolouring error red, hiding the Strip or rebinding `Esc`/`Ctrl+Z`/`Alt+Q`. |
| **Type-ahead is deterministic under latency** | `dna-skin-runtime::InputQueue` + `client_seq` / `SelectionCause` | Simulated slow-carrier test interleaving rapid typing, `Ctrl+Arrow` and macro `Range.Select` matches step-by-step execution identically. |
